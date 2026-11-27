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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    // Nom seul sans extrait préparé : jeu de voix du même nom, timbre seul.
    if jeu.is_none() && Path::new(voix).extension().is_none() && voicesets_dir.join(voix).is_dir() {
        return reference(&crate::voicesets::nom_api(voix), voices_dir, voicesets_dir);
    }
    Err(format!("Voix introuvable pour Qwen3-TTS : {voix}"))
}

/// Durée conseillée de l'extrait de référence, et bornes du curseur (secondes).
pub const EXTRAIT_DEFAUT: f32 = 10.0;
pub const EXTRAIT_MIN: f32 = 4.0;
pub const EXTRAIT_MAX: f32 = 20.0;

/// Voix de Qwen3-TTS : extrait préparé et texte prononcé dedans (`voix/<nom>.wav` + `.txt`).
#[derive(Clone, Debug)]
pub struct Reference {
    pub nom: String,
    pub wav: PathBuf,
    pub texte: String,
    pub duree: f32,
}

fn dossier_voix() -> PathBuf {
    dossier().join("voix")
}

fn reference_preparee_dans(d: &Path, nom: &str) -> Option<Reference> {
    let wav = d.join(format!("{nom}.wav"));
    if !wav.is_file() {
        return None;
    }
    let texte = std::fs::read_to_string(wav.with_extension("txt")).unwrap_or_default().trim().to_string();
    let duree = crate::audio::duree(&wav).unwrap_or(0.0);
    Some(Reference { nom: nom.to_string(), wav, texte, duree })
}

/// Toutes les voix préparées pour Qwen3-TTS, par ordre alphabétique.
pub fn voix_preparees() -> Vec<Reference> {
    voix_preparees_dans(&dossier_voix())
}

fn voix_preparees_dans(d: &Path) -> Vec<Reference> {
    let mut noms: Vec<String> = std::fs::read_dir(d)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "wav"))
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
                // « x.nouveau.wav » : écriture en cours.
                .filter(|n| !n.contains('.'))
                .collect()
        })
        .unwrap_or_default();
    noms.sort_by_key(|n| n.to_lowercase());
    noms.iter().filter_map(|n| reference_preparee_dans(d, n)).collect()
}

/// Dossier des enregistrements faits pour Qwen3-TTS (nettoyés, jamais supprimés) : ils servent
/// de source aux extraits sans entrer dans un jeu de voix d'XTTS.
pub fn dossier_enregistrements() -> PathBuf {
    dossier().join("enregistrements")
}

/// Nettoie un enregistrement brut (silences, volume) et le range dans `enregistrements/`.
pub fn ranger_enregistrement(brut: &crate::audio::Wav, nom: &str) -> Result<PathBuf, String> {
    let d = dossier_enregistrements();
    std::fs::create_dir_all(&d).map_err(|e| format!("Impossible de créer {} : {e}", d.display()))?;
    let propre = crate::audio::nettoyer(brut)?;
    let base = crate::paths::nom_sur(nom);
    let mut dest = d.join(format!("{base}.wav"));
    let mut i = 2;
    while dest.exists() {
        dest = d.join(format!("{base}-{i}.wav"));
        i += 1;
    }
    let part = dest.with_extension("wav.part");
    crate::audio::ecrire_i16(&part, &propre)?;
    std::fs::rename(&part, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Impossible d'écrire {} : {e}", dest.display())
    })?;
    Ok(dest)
}

/// Fichiers utilisables comme source d'un extrait : enregistrements faits pour Qwen3-TTS (les
/// plus récents d'abord), puis échantillons de chaque jeu de voix d'XTTS. (libellé, chemin)
pub fn sources(voicesets_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut res = Vec::new();
    let mut enr: Vec<PathBuf> = std::fs::read_dir(dossier_enregistrements())
        .map(|it| it.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect())
        .unwrap_or_default();
    enr.sort();
    enr.reverse();
    for p in enr {
        let n = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        res.push((format!("Enregistrement Qwen · {n}"), p));
    }
    for jeu in crate::voicesets::lister_jeux(voicesets_dir) {
        for e in crate::voicesets::echantillons(voicesets_dir, &jeu) {
            res.push((format!("Jeu XTTS « {jeu} » · {}", e.nom), e.chemin));
        }
    }
    res
}

/// Copie l'extrait d'une voix Qwen3-TTS dans un jeu de voix d'XTTS (nouvel échantillon).
pub fn copier_vers_jeu(r: &Reference, voicesets_dir: &Path, jeu: &str) -> Result<PathBuf, String> {
    let w = crate::audio::lire(&r.wav).map_err(|e| format!("Lecture de {} impossible : {e}", r.wav.display()))?;
    let nom = format!("{jeu}-{}-qwen-{}", crate::paths::horodatage(), r.nom);
    crate::voicesets::ajouter_echantillon(voicesets_dir, jeu, &w, &nom)
}

/// Début de l'échantillon, coupé dans un silence vers `secondes` (au plus tard), en PCM 16 bits.
/// Renvoie la durée réelle.
pub fn ecrire_extrait(echantillon: &Path, secondes: f32, dest: &Path) -> Result<f32, String> {
    let w = crate::audio::lire(echantillon).map_err(|e| format!("Lecture de {} impossible : {e}", echantillon.display()))?;
    let e = crate::audio::tronquer(&w, secondes);
    let part = dest.with_extension("wav.part");
    crate::audio::ecrire_i16(&part, &e)?;
    std::fs::rename(&part, dest).map_err(|err| {
        let _ = std::fs::remove_file(&part);
        format!("Impossible d'écrire {} : {err}", dest.display())
    })?;
    Ok(e.samples.len() as f32 / e.rate.max(1) as f32)
}

/// Prépare la référence de `nom` : extrait de l'échantillon et texte. Une référence existante
/// est déplacée dans `voix/corbeille/`, jamais supprimée.
pub fn preparer_reference(echantillon: &Path, secondes: f32, nom: &str, texte: &str) -> Result<Reference, String> {
    preparer_reference_dans(&dossier_voix(), echantillon, secondes, nom, texte)
}

fn preparer_reference_dans(d: &Path, echantillon: &Path, secondes: f32, nom: &str, texte: &str) -> Result<Reference, String> {
    let texte = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    if texte.is_empty() {
        return Err("Écrivez le texte prononcé dans l'extrait.".into());
    }
    std::fs::create_dir_all(d).map_err(|e| format!("Impossible de créer {} : {e}", d.display()))?;
    // Nouvel extrait écrit à part d'abord : en cas d'échec, l'ancienne référence reste en place.
    let neuf = d.join(format!("{nom}.nouveau.wav"));
    ecrire_extrait(echantillon, secondes, &neuf)?;
    let nettoyer = |e: String| {
        let _ = std::fs::remove_file(&neuf);
        e
    };
    retirer_reference_dans(d, nom).map_err(nettoyer)?;
    let txt = d.join(format!("{nom}.txt"));
    let part = d.join(format!("{nom}.txt.part"));
    std::fs::write(&part, format!("{texte}\n"))
        .and_then(|_| std::fs::rename(&part, &txt))
        .map_err(|e| nettoyer(format!("Impossible d'écrire {} : {e}", txt.display())))?;
    let wav = d.join(format!("{nom}.wav"));
    std::fs::rename(&neuf, &wav).map_err(|e| nettoyer(format!("Impossible d'écrire {} : {e}", wav.display())))?;
    reference_preparee_dans(d, nom).ok_or_else(|| format!("{} introuvable après écriture.", wav.display()))
}

/// Déplace la référence de `nom` (wav et txt) dans `voix/corbeille/`. Sans effet s'il n'y en a pas.
pub fn retirer_reference(nom: &str) -> Result<(), String> {
    retirer_reference_dans(&dossier_voix(), nom)
}

fn retirer_reference_dans(d: &Path, nom: &str) -> Result<(), String> {
    let corbeille = d.join("corbeille");
    let h = crate::paths::horodatage();
    for ext in ["wav", "txt"] {
        let src = d.join(format!("{nom}.{ext}"));
        if !src.is_file() {
            continue;
        }
        std::fs::create_dir_all(&corbeille).map_err(|e| format!("Impossible de créer {} : {e}", corbeille.display()))?;
        let mut dest = corbeille.join(format!("{nom}_{h}.{ext}"));
        let mut i = 2;
        while dest.exists() {
            dest = corbeille.join(format!("{nom}_{h}-{i}.{ext}"));
            i += 1;
        }
        std::fs::rename(&src, &dest).map_err(|e| format!("Impossible de déplacer {} : {e}", src.display()))?;
    }
    Ok(())
}

/// Proposition de texte pour un extrait de `secondes` d'une lecture de `modele` : les premiers
/// mots (environ 2,6 par seconde), arrêtés si possible à une ponctuation. À vérifier à l'écoute.
pub fn texte_propose(modele: &str, secondes: f32) -> String {
    let mots: Vec<&str> = modele
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(|l| l.split_whitespace())
        .collect();
    let cible = ((secondes * 2.6).round() as usize).clamp(1, mots.len().max(1));
    let fin_phrase = |m: &str| m.ends_with(['.', '!', '?', ',', ';', ':']);
    let n = (cible.saturating_sub(6)..=cible.min(mots.len()))
        .rev()
        .find(|&i| i > 0 && fin_phrase(mots[i - 1]))
        .unwrap_or(cible.min(mots.len()));
    mots[..n].join(" ")
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
/// (pause 0,25 s, PCM 16 bits) que `alltalk::generer`. Quand `arret` passe à vrai, le processus
/// est tué aussitôt, même au milieu d'un morceau.
pub fn generer(
    req: &Requete,
    voices_dir: &Path,
    voicesets_dir: &Path,
    sortie: &Path,
    progression: &dyn Fn(Progression),
    arret: &Arc<AtomicBool>,
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
    // Surveillance de la demande d'arrêt : la lecture des réponses bloque, c'est ce fil qui tue
    // le processus (la lecture reçoit alors une fin de fichier).
    let enfant = Arc::new(Mutex::new(enfant));
    let fini = Arc::new(AtomicBool::new(false));
    let veilleur = {
        let (enfant, fini, arret) = (enfant.clone(), fini.clone(), arret.clone());
        std::thread::spawn(move || {
            while !fini.load(Ordering::Relaxed) {
                if arret.load(Ordering::Relaxed) {
                    if let Ok(mut e) = enfant.lock() {
                        let _ = e.kill();
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
    };

    let resultat = (|| -> Result<Vec<PathBuf>, String> {
        let etat = lire(&mut lecteur, &journal)?;
        if etat.get("pret").and_then(|v| v.as_bool()) != Some(true) {
            let e = etat.get("erreur").and_then(|v| v.as_str()).unwrap_or("raison inconnue");
            return Err(format!("Qwen3-TTS n'a pas pu se charger : {e}"));
        }
        let total = morceaux.len();
        let mut fichiers = Vec::with_capacity(total);
        for (i, m) in morceaux.iter().enumerate() {
            if arret.load(Ordering::Relaxed) {
                return Err(crate::alltalk::INTERROMPUE.into());
            }
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
    fini.store(true, Ordering::Relaxed);
    let _ = veilleur.join();
    if let Ok(mut e) = enfant.lock() {
        if resultat.is_err() {
            let _ = e.kill();
        }
        let _ = e.wait();
    }
    // Processus tué à la demande : l'erreur de lecture qui en résulte n'en est pas une.
    let resultat = match resultat {
        Err(_) if arret.load(Ordering::Relaxed) => Err(crate::alltalk::INTERROMPUE.to_string()),
        r => r,
    };

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
        // Nom seul sans voix préparée : jeu du même nom, timbre seul.
        let (w, t) = reference("jeu-test-qwen", &voices, &jeux).unwrap();
        assert_eq!(w, jeux.join("jeu-test-qwen").join("a.wav"));
        assert!(t.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn preparer_puis_remplacer_reference() {
        let d = std::env::temp_dir().join(format!("vocalise_test_qwen_ref_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let ech = d.join("ech.wav");
        let w = crate::audio::Wav { rate: 24000, samples: (0..24000 * 15).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect() };
        crate::audio::ecrire_i16(&ech, &w).unwrap();
        let voix = d.join("voix");
        assert!(preparer_reference_dans(&voix, &ech, 10.0, "jeu", "  ").is_err());
        let r = preparer_reference_dans(&voix, &ech, 10.0, "jeu", "Bonjour à tous,\n et bienvenue.").unwrap();
        assert!(r.duree > 6.9 && r.duree <= 10.01, "{}", r.duree);
        assert_eq!(r.texte, "Bonjour à tous, et bienvenue.");
        // Remplacement : l'ancienne référence part dans la corbeille.
        let r2 = preparer_reference_dans(&voix, &ech, 5.0, "jeu", "Bonjour.").unwrap();
        assert!(r2.duree <= 5.01);
        assert_eq!(std::fs::read_dir(voix.join("corbeille")).unwrap().count(), 2);
        // Liste des voix préparées : « jeu » et « autre », pas les écritures en cours.
        preparer_reference_dans(&voix, &ech, 6.0, "autre", "Bonjour.").unwrap();
        std::fs::write(voix.join("x.nouveau.wav"), b"x").unwrap();
        let noms: Vec<String> = voix_preparees_dans(&voix).into_iter().map(|r| r.nom).collect();
        assert_eq!(noms, ["autre", "jeu"]);
        retirer_reference_dans(&voix, "jeu").unwrap();
        assert!(reference_preparee_dans(&voix, "jeu").is_none());
        assert_eq!(std::fs::read_dir(voix.join("corbeille")).unwrap().count(), 4);
        assert_eq!(voix_preparees_dans(&voix).len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn proposition_de_texte() {
        let m = "# Titre\nBonjour à tous, et bienvenue dans cette nouvelle vidéo. Aujourd'hui, nous allons observer ce qui se passe.";
        let t = texte_propose(m, 4.0);
        assert!(t.starts_with("Bonjour à tous,"), "{t}");
        assert!(!t.contains('#'));
        assert_eq!(texte_propose(m, 60.0), m.lines().nth(1).unwrap());
    }

    /// Interruption réelle : le processus est tué au milieu d'un morceau, sans fichier produit.
    /// `cargo test -- --ignored integration_qwen_interruption`.
    #[test]
    #[ignore]
    fn integration_qwen_interruption() {
        let cfg = crate::paths::Config::default();
        let req = Requete {
            texte: "Bonjour. Ceci est un long essai qui ne doit pas aller à son terme. ".repeat(6),
            voix: "emmanuel".into(),
            langue: "fr".into(),
            vitesse: 1.0,
            temperature: 0.7,
        };
        let sortie = std::env::temp_dir().join("vocalise_integration_qwen_interruption.wav");
        let arret = Arc::new(AtomicBool::new(false));
        let a = arret.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(10));
            a.store(true, Ordering::Relaxed);
        });
        let debut = std::time::Instant::now();
        let r = generer(&req, &cfg.voices_dir(), &cfg.voicesets_dir(), &sortie, &|_| {}, &arret);
        assert_eq!(r.unwrap_err(), crate::alltalk::INTERROMPUE);
        assert!(debut.elapsed().as_secs_f32() < 12.0, "{:?}", debut.elapsed());
        assert!(!sortie.exists());
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
        let p = generer(&req, &cfg.voices_dir(), &cfg.voicesets_dir(), &sortie, &|_| {}, &Arc::new(AtomicBool::new(false))).unwrap();
        assert!(crate::audio::duree(&p).unwrap() > 1.0);
        let _ = std::fs::remove_file(p);
    }
}
