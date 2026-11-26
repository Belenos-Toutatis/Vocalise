//! Moteur Qwen3-TTS 0.6B, sans AllTalk. Il tourne dans son propre environnement Python (qwen-tts
//! et torch, incompatibles avec ceux d'AllTalk), sur le processeur : avec le bureau, il ne tient pas
//! sur une carte de 4 Go. Le script `moteur_qwen.py` (embarqué, réécrit dans le dossier de données
//! s'il a changé) est lancé pour une génération : il charge le modèle en quelques secondes, reçoit
//! les morceaux un par un sur son entrée (une ligne JSON chacun) et se termine quand l'entrée se
//! ferme. Rien ne reste en mémoire entre deux générations.
//!
//! Dossier : `<données>/vocalise/qwen/` (`~/.local/share` sous Linux, `%LOCALAPPDATA%` sous Windows)
//! avec `env/` (environnement Python), `moteur_qwen.py`, `voix/<nom>.wav` (+ `.txt`, le texte
//! prononcé dans l'extrait) et `qwen.log`.

use crate::alltalk::{Progression, Requete};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

const SCRIPT: &str = include_str!("../../moteur_qwen.py");

pub fn dossier() -> PathBuf {
    crate::systeme::dossier_donnees().join("vocalise").join("qwen")
}

fn dossier_env() -> PathBuf {
    dossier().join("env")
}

/// Python de l'environnement de Qwen3-TTS (variable `VOCALISE_QWEN_PYTHON` pour un autre).
pub fn python() -> PathBuf {
    if let Some(p) = std::env::var_os("VOCALISE_QWEN_PYTHON").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    if crate::systeme::WINDOWS {
        dossier_env().join("Scripts").join("python.exe")
    } else {
        dossier_env().join("bin").join("python")
    }
}

pub fn installe() -> bool {
    python().is_file()
}

/// Commandes d'installation (uv) à coller dans un terminal ou dans PowerShell.
pub fn commande_installation() -> String {
    let env = dossier_env();
    format!(
        "uv venv --python 3.12 \"{e}\"{sep}uv pip install --python \"{e}\" qwen-tts --torch-backend cpu",
        e = env.display(),
        sep = if crate::systeme::WINDOWS { "; " } else { " && " }
    )
}

/// Écrit le script embarqué dans le dossier de données s'il est absent ou différent.
fn script() -> Result<PathBuf, String> {
    let d = dossier();
    std::fs::create_dir_all(&d).map_err(|e| format!("Impossible de créer {} : {e}", d.display()))?;
    let p = d.join("moteur_qwen.py");
    if std::fs::read_to_string(&p).ok().as_deref() != Some(SCRIPT) {
        let tmp = p.with_extension("py.part");
        std::fs::write(&tmp, SCRIPT).map_err(|e| format!("Impossible d'écrire {} : {e}", tmp.display()))?;
        std::fs::rename(&tmp, &p).map_err(|e| format!("Impossible d'écrire {} : {e}", p.display()))?;
    }
    Ok(p)
}

/// Extrait de référence pour une voix ("voiceset:nom" ou "fichier.wav") et texte prononcé dedans
/// ("" si inconnu : seul le timbre est alors copié). Ordre : référence préparée
/// `voix/<nom>.wav` (+ `.txt`), sinon premier échantillon du jeu de voix, sinon le wav de `voices/`.
pub fn reference(voix: &str, voices_dir: &Path, voicesets_dir: &Path) -> Result<(PathBuf, String), String> {
    let jeu = crate::voicesets::jeu_depuis_api(voix);
    let nom = match jeu {
        Some(j) => j.to_string(),
        None => Path::new(voix).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    let prepare = dossier().join("voix").join(format!("{nom}.wav"));
    if prepare.is_file() {
        let texte = std::fs::read_to_string(prepare.with_extension("txt")).unwrap_or_default();
        return Ok((prepare, texte.trim().to_string()));
    }
    if let Some(j) = jeu {
        let mut wavs: Vec<PathBuf> = std::fs::read_dir(voicesets_dir.join(j))
            .map(|it| {
                it.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")))
                    .collect()
            })
            .unwrap_or_default();
        wavs.sort();
        if let Some(w) = wavs.into_iter().next() {
            return Ok((w, String::new()));
        }
    }
    let simple = voices_dir.join(voix);
    if simple.is_file() {
        return Ok((simple, String::new()));
    }
    Err(format!("Voix introuvable pour Qwen3-TTS : {voix}"))
}

/// Une réponse du script, une ligne JSON.
fn lire(sortie: &mut impl BufRead, journal: &Path) -> Result<serde_json::Value, String> {
    let mut ligne = String::new();
    match sortie.read_line(&mut ligne) {
        Ok(n) if n > 0 => serde_json::from_str(&ligne).map_err(|e| format!("Réponse de Qwen3-TTS illisible ({e}) : {}", ligne.trim())),
        _ => Err(format!("Qwen3-TTS s'est arrêté sans répondre (journal : {}).", journal.display())),
    }
}

/// Synthétise `req.texte` avec Qwen3-TTS : même découpage, même progression et même assemblage
/// (pause 0,25 s, PCM 16 bits) que `alltalk::generer`.
pub fn generer(
    req: &Requete,
    voices_dir: &Path,
    voicesets_dir: &Path,
    sortie: &Path,
    progression: &dyn Fn(Progression),
) -> Result<PathBuf, String> {
    let morceaux = crate::texte::decouper(&req.texte);
    if morceaux.is_empty() {
        return Err("Le texte est vide : il n'y a rien à lire.".into());
    }
    let py = python();
    if !py.is_file() {
        return Err(format!("Qwen3-TTS n'est pas installé ({} introuvable) : voir Réglages › Diagnostic.", py.display()));
    }
    let script = script()?;
    let (ref_audio, ref_texte) = reference(&req.voix, voices_dir, voicesets_dir)?;
    let journal = dossier().join("qwen.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal)
        .map_err(|e| format!("Impossible d'ouvrir {} : {e}", journal.display()))?;
    let tmp = std::env::temp_dir().join(format!("vocalise_qwen_{}_{}", std::process::id(), chrono::Local::now().format("%H%M%S")));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("Impossible de créer {} : {e}", tmp.display()))?;

    let mut enfant = crate::systeme::commande(&py)
        .arg(&script)
        .arg("--ref-audio")
        .arg(&ref_audio)
        .arg("--ref-texte")
        .arg(&ref_texte)
        .arg("--langue")
        .arg(&req.langue)
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|e| format!("Impossible de lancer Qwen3-TTS ({}) : {e}", py.display()))?;
    let mut entree = enfant.stdin.take().expect("entrée du processus");
    let mut lecteur = BufReader::new(enfant.stdout.take().expect("sortie du processus"));

    let resultat = (|| -> Result<Vec<PathBuf>, String> {
        let etat = lire(&mut lecteur, &journal)?;
        if etat.get("pret").and_then(|v| v.as_bool()) != Some(true) {
            let e = etat.get("erreur").and_then(|v| v.as_str()).unwrap_or("raison inconnue");
            return Err(format!("Qwen3-TTS n'a pas pu se charger : {e}"));
        }
        let total = morceaux.len();
        let mut fichiers = Vec::with_capacity(total);
        for (i, m) in morceaux.iter().enumerate() {
            progression(Progression { morceau: i + 1, total });
            let f = tmp.join(format!("morceau_{:03}.wav", i + 1));
            let demande = serde_json::json!({ "texte": m, "sortie": f.to_string_lossy() });
            writeln!(entree, "{demande}").and_then(|_| entree.flush()).map_err(|e| format!("Qwen3-TTS ne répond plus : {e}"))?;
            let r = lire(&mut lecteur, &journal)?;
            if r.get("ok").and_then(|v| v.as_bool()) != Some(true) {
                let e = r.get("erreur").and_then(|v| v.as_str()).unwrap_or("raison inconnue");
                return Err(format!("Qwen3-TTS n'a pas pu générer la voix : {e}"));
            }
            fichiers.push(f);
        }
        Ok(fichiers)
    })();
    drop(entree); // fin de l'entrée : le script se termine et rend la mémoire
    if resultat.is_err() {
        let _ = enfant.kill();
    }
    let _ = enfant.wait();

    let ecrit = resultat.and_then(|fichiers| {
        let w = crate::audio::concatener(&fichiers, 0.25)?;
        if let Some(parent) = sortie.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("Impossible de créer le dossier {} : {e}", parent.display()))?;
        }
        crate::audio::ecrire_i16(sortie, &w)
    });
    let _ = std::fs::remove_dir_all(&tmp);
    ecrit.map(|_| sortie.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_embarque_et_commande() {
        assert!(SCRIPT.contains("generate_voice_clone"));
        let c = commande_installation();
        assert!(c.contains("qwen-tts") && c.contains("--torch-backend cpu"));
        assert!(python().to_string_lossy().contains("python"));
    }

    #[test]
    fn reference_par_ordre_de_preference() {
        let d = std::env::temp_dir().join(format!("vocalise_test_qwen_{}", std::process::id()));
        let voices = d.join("voices");
        let jeux = voices.join("xtts_multi_voice_sets");
        std::fs::create_dir_all(jeux.join("jeu-test-qwen")).unwrap();
        std::fs::write(jeux.join("jeu-test-qwen").join("b.wav"), b"x").unwrap();
        std::fs::write(jeux.join("jeu-test-qwen").join("a.wav"), b"x").unwrap();
        std::fs::write(voices.join("simple-test-qwen.wav"), b"x").unwrap();
        // Jeu de voix sans référence préparée : premier échantillon, sans texte.
        let (w, t) = reference("voiceset:jeu-test-qwen", &voices, &jeux).unwrap();
        assert_eq!(w, jeux.join("jeu-test-qwen").join("a.wav"));
        assert!(t.is_empty());
        // Voix simple.
        let (w, _) = reference("simple-test-qwen.wav", &voices, &jeux).unwrap();
        assert_eq!(w, voices.join("simple-test-qwen.wav"));
        assert!(reference("voiceset:absent-test-qwen", &voices, &jeux).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Génération réelle (environnement Qwen installé) : `cargo test -- --ignored integration_qwen`.
    #[test]
    #[ignore]
    fn integration_qwen() {
        let cfg = crate::paths::Config::default();
        let req = Requete {
            texte: "Bonjour. Ceci est un essai.".into(),
            voix: "voiceset:emmanuel".into(),
            langue: "fr".into(),
            vitesse: 1.0,
            temperature: 0.7,
        };
        let sortie = std::env::temp_dir().join("vocalise_integration_qwen.wav");
        let p = generer(&req, &cfg.voices_dir(), &cfg.voicesets_dir(), &sortie, &|_| {}).unwrap();
        assert!(crate::audio::duree(&p).unwrap() > 1.0);
        let _ = std::fs::remove_file(p);
    }
}
