//! Dialogue avec le serveur local AllTalk (API HTTP) et gestion de son processus.

use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

pub const API: &str = "http://127.0.0.1:7851";
pub const WEB: &str = "http://127.0.0.1:7852";

/// GET /api/ready == "Ready" (délai 2 s).
pub fn pret() -> bool {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).build();
    match agent.get(&format!("{API}/api/ready")).call() {
        Ok(r) => r.into_string().map(|t| t.trim().eq_ignore_ascii_case("ready")).unwrap_or(false),
        Err(_) => false,
    }
}

/// Liste des voix utilisables, telles qu'attendues par le champ `character_voice_gen` :
/// fichiers "xxx.wav" et jeux "voiceset:nom". Source : GET /api/voices (`{"voices":[...]}`) ;
/// si le serveur ne répond pas, repli sur un parcours de `voices_dir` et de
/// `voices_dir/xtts_multi_voice_sets/*/` (dossiers contenant au moins un .wav).
/// Tri : jeux de voix d'abord, puis fichiers, insensible à la casse.
pub fn voix_disponibles(voices_dir: &Path) -> Vec<String> {
    let mut voix = voix_depuis_api().filter(|v| !v.is_empty()).unwrap_or_else(|| voix_depuis_disque(voices_dir));
    voix.sort_by_key(|v| (!v.starts_with("voiceset:"), v.to_lowercase()));
    voix.dedup();
    voix
}

fn voix_depuis_api() -> Option<Vec<String>> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).build();
    let rep: serde_json::Value = agent.get(&format!("{API}/api/voices")).call().ok()?.into_json().ok()?;
    let liste = rep.get("voices")?.as_array()?;
    Some(liste.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
}

fn est_wav(p: &Path) -> bool {
    p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

fn voix_depuis_disque(voices_dir: &Path) -> Vec<String> {
    let mut res = Vec::new();
    if let Ok(entrees) = std::fs::read_dir(voices_dir) {
        for e in entrees.flatten() {
            let p = e.path();
            if est_wav(&p) {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    res.push(n.to_string());
                }
            }
        }
    }
    if let Ok(entrees) = std::fs::read_dir(voices_dir.join("xtts_multi_voice_sets")) {
        for e in entrees.flatten() {
            let p = e.path();
            let a_un_wav = p.is_dir()
                && std::fs::read_dir(&p).map(|d| d.flatten().any(|f| est_wav(&f.path()))).unwrap_or(false);
            if a_un_wav {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    res.push(format!("voiceset:{n}"));
                }
            }
        }
    }
    res
}

#[derive(Clone, Debug)]
pub struct Requete {
    pub texte: String,
    pub voix: String,
    pub langue: String,
    pub vitesse: f32,
    pub temperature: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Progression {
    /// Morceau en cours, à partir de 1.
    pub morceau: usize,
    pub total: usize,
}

/// Nombre d'essais par morceau quand AllTalk échoue côté serveur.
const ESSAIS: u32 = 3;

/// Une requête de génération. Erreur : (réessayable, message lisible).
fn demander(
    agent: &ureq::Agent,
    req: &Requete,
    morceau: &str,
    nom: &str,
    vitesse: &str,
    temperature: &str,
) -> Result<serde_json::Value, (bool, String)> {
    let rep = agent.post(&format!("{API}/api/tts-generate")).send_form(&[
        ("text_input", morceau),
        ("text_filtering", "standard"),
        ("character_voice_gen", req.voix.as_str()),
        ("narrator_enabled", "false"),
        ("text_not_inside", "character"),
        ("language", req.langue.as_str()),
        ("output_file_name", nom),
        ("output_file_timestamp", "false"),
        ("autoplay", "false"),
        ("speed", vitesse),
        ("temperature", temperature),
    ]);
    let rep = match rep {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let corps = r.into_string().unwrap_or_default();
            let corps = corps.trim();
            let message = if corps.is_empty() {
                format!("AllTalk a refusé la demande (erreur HTTP {code}).")
            } else {
                format!("AllTalk a refusé la demande (erreur HTTP {code}) : {}", abreger(corps, 400))
            };
            return Err((code >= 500, message));
        }
        Err(ureq::Error::Transport(t)) => {
            return Err((
                false,
                format!("Impossible de joindre AllTalk ({t}). Vérifiez que le serveur est démarré (voyant « AllTalk » en haut de la fenêtre)."),
            ));
        }
    };
    let json: serde_json::Value = rep
        .into_json()
        .map_err(|e| (false, format!("Réponse d'AllTalk illisible : {e}")))?;
    if json.get("status").and_then(|s| s.as_str()) == Some("generate-failure") {
        let detail = json.get("error").and_then(|e| e.as_str()).unwrap_or("raison inconnue");
        return Err((true, format!("AllTalk n'a pas pu générer la voix : {detail}")));
    }
    Ok(json)
}

/// Synthétise `req.texte` : découpe (texte::decouper), POST /api/tts-generate par morceau
/// (champs : text_input, text_filtering=standard, character_voice_gen, narrator_enabled=false,
/// text_not_inside=character, language, output_file_name=vocalise_HHMMSS_NNN,
/// output_file_timestamp=false, autoplay=false, speed, temperature ; délai 30 min),
/// appelle `progression` avant chaque morceau, puis concatène (audio::concatener, pause 0,25 s)
/// et écrit `sortie` en PCM 16 bits. Erreurs lisibles : HTTP non-2xx avec corps,
/// `status == "generate-failure"` avec `error`, fichier de sortie AllTalk introuvable.
pub fn generer(req: &Requete, sortie: &Path, progression: &dyn Fn(Progression)) -> Result<PathBuf, String> {
    let morceaux = crate::texte::decouper(&req.texte);
    if morceaux.is_empty() {
        return Err("Le texte est vide : il n'y a rien à lire.".into());
    }
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(30 * 60)).build();
    let heure = chrono::Local::now().format("%H%M%S").to_string();
    let vitesse = format!("{}", req.vitesse);
    let temperature = format!("{}", req.temperature);
    let total = morceaux.len();
    let mut fichiers: Vec<PathBuf> = Vec::with_capacity(total);
    for (i, morceau) in morceaux.iter().enumerate() {
        progression(Progression { morceau: i + 1, total });
        let nom = format!("vocalise_{heure}_{:03}", i + 1);
        // Carte de 4 Go : la VRAM manque parfois de quelques Mo (« CUDA out of memory »,
        // « CUFFT_INTERNAL_ERROR ») ; l'échec en libère et l'essai suivant passe en général.
        let mut essai = 1;
        let json = loop {
            match demander(&agent, req, morceau, &nom, &vitesse, &temperature) {
                Ok(json) => break json,
                Err((reessayable, e)) if !reessayable || essai >= ESSAIS => {
                    return Err(if reessayable {
                        format!("{e} (après {ESSAIS} essais ; cause probable : mémoire de la carte graphique presque pleine, fermez des applications gourmandes puis réessayez)")
                    } else {
                        e
                    });
                }
                Err(_) => {
                    essai += 1;
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        };
        let chemin = json
            .get("output_file_path")
            .and_then(|p| p.as_str())
            .filter(|p| !p.is_empty())
            .ok_or_else(|| format!("AllTalk n'a pas indiqué où il a écrit le fichier audio (réponse : {})", abreger(&json.to_string(), 300)))?;
        let chemin = PathBuf::from(chemin);
        if !chemin.is_file() {
            return Err(format!("Le fichier audio produit par AllTalk est introuvable : {}", chemin.display()));
        }
        fichiers.push(chemin);
    }
    let w = crate::audio::concatener(&fichiers, 0.25)?;
    if let Some(parent) = sortie.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Impossible de créer le dossier {} : {e}", parent.display()))?;
    }
    crate::audio::ecrire_i16(sortie, &w)?;
    Ok(sortie.to_path_buf())
}

/// Tronque un message d'erreur trop long (en caractères).
fn abreger(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(unix)]
/// Entoure de guillemets simples pour le shell (échappe les guillemets simples internes).
fn quote_sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Envoie un signal au groupe de processus `pid` via la commande `kill`.
#[cfg(unix)]
fn signaler_groupe(signal: &str, pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg(signal)
        .arg("--")
        .arg(format!("-{pid}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Commande qui lance AllTalk depuis son dossier.
#[cfg(unix)]
fn commande_lanceur(alltalk_dir: &Path) -> std::process::Command {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new("bash");
    cmd.arg("-lc")
        .arg(format!("cd {} && exec ./start_alltalk.sh", quote_sh(&alltalk_dir.to_string_lossy())))
        .process_group(0);
    cmd
}

#[cfg(windows)]
fn commande_lanceur(alltalk_dir: &Path) -> std::process::Command {
    use crate::systeme::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
    use std::os::windows::process::CommandExt;
    // Chemin complet du .bat : avec NoDefaultCurrentDirectoryInExePath (défini par Claude Code et
    // hérité par ses sous-processus), cmd ne cherche plus un nom seul dans le dossier courant.
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/D", "/C"])
        .arg(alltalk_dir.join(crate::systeme::LANCEUR_ALLTALK))
        .current_dir(alltalk_dir)
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    cmd
}

/// Termine un processus et tous ses descendants (`taskkill /PID <pid> /T /F`).
#[cfg(windows)]
fn tuer_arbre(pid: u32) {
    let _ = crate::systeme::commande("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

impl Drop for Serveur {
    /// Ne jamais laisser AllTalk orphelin : si le processus tourne encore à la destruction,
    /// SIGTERM au groupe, attente de 3 s au plus, puis SIGKILL.
    /// Sous Windows : `taskkill /T /F` sur l'arbre de processus.
    fn drop(&mut self) {
        let pid = self.child.id();
        #[cfg(windows)]
        if matches!(self.child.try_wait(), Ok(None)) {
            tuer_arbre(pid);
            self.attendre_fin(Some(Duration::from_secs(5)));
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        #[cfg(unix)]
        if matches!(self.child.try_wait(), Ok(None)) {
            signaler_groupe("-TERM", pid);
            self.attendre_fin(Some(Duration::from_secs(3)));
            if matches!(self.child.try_wait(), Ok(None)) {
                signaler_groupe("-KILL", pid);
                let _ = self.child.wait();
            }
        }
    }
}

/// Processus AllTalk lancé par l'application (groupe de processus propre, journal sur disque).
pub struct Serveur {
    child: Child,
}

impl Serveur {
    /// Linux : `bash -lc "cd <alltalk_dir> && exec ./start_alltalk.sh"` dans son propre groupe de
    /// processus. Windows : `cmd /C <alltalk_dir>\start_alltalk.bat` dans le dossier d'AllTalk, sans fenêtre,
    /// nouveau groupe de processus. stdout+stderr → `journal`. Erreur si le lanceur n'existe pas.
    pub fn demarrer(alltalk_dir: &Path, journal: &Path) -> Result<Serveur, String> {
        use std::process::Stdio;
        let script = alltalk_dir.join(crate::systeme::LANCEUR_ALLTALK);
        if !script.is_file() {
            return Err(format!(
                "AllTalk est introuvable : le fichier {} n'existe pas. Vérifiez le dossier d'AllTalk dans les réglages.",
                script.display()
            ));
        }
        if let Some(parent) = journal.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(parent);
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(journal)
            .map_err(|e| format!("Impossible d'ouvrir le journal {} : {e}", journal.display()))?;
        let log_err = log.try_clone().map_err(|e| format!("Impossible d'ouvrir le journal : {e}"))?;
        let mut cmd = commande_lanceur(alltalk_dir);
        let child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            // Erreurs du moteur visibles tout de suite dans le journal. (Ne pas ajouter
            // PYTORCH_CUDA_ALLOC_CONF=expandable_segments : essayé, il fait échouer presque
            // toutes les générations sur la GTX 1650.)
            .env("PYTHONUNBUFFERED", "1")
            .spawn()
            .map_err(|e| format!("Impossible de lancer AllTalk : {e}"))?;
        Ok(Serveur { child })
    }

    /// Ctrl-C au groupe de processus (`kill -INT -- -<pid>`) puis attente de fin (max ~15 s,
    /// ensuite SIGKILL du groupe).
    pub fn arreter(&mut self) {
        let pid = self.child.id();
        #[cfg(windows)]
        {
            // Pas de Ctrl-C possible vers un processus sans console visible : on termine l'arbre
            // (cmd, python script.py et le serveur du moteur). Le système rend alors la mémoire.
            if !self.attendre_fin(None) {
                tuer_arbre(pid);
                if !self.attendre_fin(Some(Duration::from_secs(10))) {
                    let _ = self.child.kill();
                }
            }
            let _ = self.child.wait();
            return;
        }
        #[cfg(unix)]
        {
        if !self.attendre_fin(None) {
            signaler_groupe("-INT", pid);
            if !self.attendre_fin(Some(Duration::from_secs(8))) {
                signaler_groupe("-TERM", pid);
                self.attendre_fin(Some(Duration::from_secs(3)));
            }
        }
        // Le chef est parti : laisser 3 s aux autres membres du groupe (`kill -0` réussit tant
        // que le groupe existe), puis SIGKILL pour ce qui reste.
        let limite = Instant::now() + Duration::from_secs(3);
        while Instant::now() < limite && signaler_groupe("-0", pid) && !matches!(self.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if matches!(self.child.try_wait(), Ok(None)) || signaler_groupe("-0", pid) {
            signaler_groupe("-KILL", pid);
            let _ = self.child.wait();
        }
        }
    }

    /// Attend la fin du chef de groupe (`delai` = None : simple test). Vrai s'il est terminé.
    fn attendre_fin(&mut self, delai: Option<Duration>) -> bool {
        let limite = Instant::now() + delai.unwrap_or_default();
        loop {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return true;
            }
            if Instant::now() >= limite {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Le processus tourne-t-il encore ?
    pub fn en_vie(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn quote_sh_echappe() {
        assert_eq!(quote_sh("/a b/c"), "'/a b/c'");
        assert_eq!(quote_sh("l'ami"), "'l'\\''ami'");
    }

    #[test]
    fn voix_depuis_disque_trie_et_filtre() {
        let d = std::env::temp_dir().join(format!("vocalise-test-voix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("xtts_multi_voice_sets/plein")).unwrap();
        std::fs::create_dir_all(d.join("xtts_multi_voice_sets/vide")).unwrap();
        std::fs::write(d.join("xtts_multi_voice_sets/plein/a.wav"), b"x").unwrap();
        std::fs::write(d.join("Zed.wav"), b"x").unwrap();
        std::fs::write(d.join("alice.WAV"), b"x").unwrap();
        std::fs::write(d.join("notes.txt"), b"x").unwrap();
        let mut v = voix_depuis_disque(&d);
        v.sort_by_key(|x| (!x.starts_with("voiceset:"), x.to_lowercase()));
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(v, vec!["voiceset:plein", "alice.WAV", "Zed.wav"]);
    }

    #[cfg(unix)]
    #[test]
    fn serveur_drop_tue_le_groupe() {
        let d = std::env::temp_dir().join(format!("vocalise-test-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("start_alltalk.sh"), "#!/bin/bash\nsleep 300 &\nwait\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.join("start_alltalk.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let s = Serveur::demarrer(&d, &d.join("alltalk.log")).unwrap();
        let pid = s.pid();
        std::thread::sleep(Duration::from_millis(300));
        assert!(signaler_groupe("-0", pid));
        drop(s);
        assert!(!signaler_groupe("-0", pid));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn serveur_script_absent() {
        let d = std::env::temp_dir().join("vocalise-inexistant-alltalk");
        let r = Serveur::demarrer(&d, &std::env::temp_dir().join("vocalise-test-journal.log"));
        assert!(r.err().is_some_and(|e| e.contains("introuvable")));
    }

    #[cfg(unix)]
    #[test]
    fn serveur_demarre_et_s_arrete() {
        // Faux « start_alltalk.sh » : un sleep dans le groupe de processus.
        let d = std::env::temp_dir().join(format!("vocalise-test-serveur-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("start_alltalk.sh"), "#!/bin/bash\nsleep 300 &\nwait\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.join("start_alltalk.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut s = Serveur::demarrer(&d, &d.join("alltalk.log")).unwrap();
        assert!(s.en_vie());
        let t = Instant::now();
        s.arreter();
        assert!(!s.en_vie());
        assert!(t.elapsed() < Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Windows : faux « start_alltalk.bat » qui lance un processus enfant long (ping) ; l'arrêt
    /// doit terminer tout l'arbre rapidement, et l'abandon (Drop) aussi.
    #[cfg(windows)]
    #[test]
    fn serveur_windows_demarre_et_s_arrete() {
        let d = std::env::temp_dir().join(format!("vocalise-test-serveur-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("start_alltalk.bat"), "@echo off\r\nping -n 300 127.0.0.1 >nul\r\n").unwrap();
        let mut s = Serveur::demarrer(&d, &d.join("alltalk.log")).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(s.en_vie());
        let t = Instant::now();
        s.arreter();
        assert!(!s.en_vie());
        assert!(t.elapsed() < Duration::from_secs(15));
        let s = Serveur::demarrer(&d, &d.join("alltalk.log")).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let t = Instant::now();
        drop(s);
        assert!(t.elapsed() < Duration::from_secs(15));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Test d'intégration manuel (nécessite AllTalk lancé) : `cargo test -- --ignored alltalk`.
    #[test]
    #[ignore]
    fn integration_serveur_en_marche() {
        assert!(pret());
        let voix = voix_disponibles(Path::new("/nonexistent"));
        assert!(!voix.is_empty());
        let sortie = std::env::temp_dir().join(format!("vocalise-test-live-{}.wav", std::process::id()));
        let req = Requete {
            texte: "Bonjour. Ceci est un test.".into(),
            voix: voix.iter().find(|v| v.ends_with(".wav")).cloned().unwrap(),
            langue: "fr".into(),
            vitesse: 1.0,
            temperature: 0.7,
        };
        let r = generer(&req, &sortie, &|p| eprintln!("morceau {}/{}", p.morceau, p.total)).unwrap();
        let w = crate::audio::lire(&r).unwrap();
        let _ = std::fs::remove_file(&r);
        assert!(w.duree() > 0.5);
    }
}
