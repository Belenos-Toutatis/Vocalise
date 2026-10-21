//! Vérification des composants externes dont Vocalise dépend, avec liens officiels
//! et commande d'installation pour chacun. Aucun accès réseau.

use crate::paths::Config;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gravite {
    /// Sans ce composant, Vocalise ne sert à rien (AllTalk, moteur XTTS).
    Bloquant,
    /// Une fonction est indisponible (micro, écoute, formats compressés).
    Fonction,
}

#[derive(Clone, Debug)]
pub struct Verification {
    /// Nom court : « Serveur AllTalk », « Moteur XTTS », « PipeWire (micro et écoute) », « ffmpeg »…
    pub composant: String,
    pub ok: bool,
    pub gravite: Gravite,
    /// Ce qui a été trouvé ou ce qui manque, en une phrase lisible.
    pub detail: String,
    /// Page officielle de téléchargement ou d'installation, s'il y a lieu.
    pub lien: Option<&'static str>,
    /// Commande à exécuter dans un terminal pour installer, s'il y a lieu (Ubuntu/Debian).
    pub commande: Option<String>,
}

/// Exécute toutes les vérifications, dans l'ordre d'affichage :
/// 1. Dossier AllTalk : `cfg.alltalk_dir` existe et contient `start_alltalk.sh` et `voices/`.
///    Lien <https://github.com/erew123/alltalk_tts>, commande
///    `git clone -b alltalkbeta https://github.com/erew123/alltalk_tts ~/alltalk_tts && cd ~/alltalk_tts && ./atsetup.sh`.
/// 2. Moteur XTTS : `moteur_configure` vaut "xtts" ET le dossier `models/xtts/` contient au moins un
///    sous-dossier avec `model.pth`. Lien vers le wiki AllTalk, pas de commande (bouton dédié dans l'UI).
/// 3. GPU NVIDIA : `nvidia-smi` répond (détail : nom de la carte et mémoire). Gravité Fonction
///    (XTTS tourne aussi sur CPU, lentement). Lien <https://www.nvidia.com/drivers>, commande
///    `sudo ubuntu-drivers install`.
/// 4. PipeWire : `pw-record`, `pw-play` et `pactl` trouvés dans le PATH. Lien <https://pipewire.org/>,
///    commande `sudo apt install pipewire-bin pulseaudio-utils`.
/// 5. ffmpeg : `ffmpeg -version` répond. Lien <https://ffmpeg.org/download.html>, commande
///    `sudo apt install ffmpeg`.
/// 6. Jeu de voix : `cfg.voicesets_dir()/cfg.jeu_actif` contient au moins un wav. Gravité Fonction,
///    pas de lien ni de commande (détail : « Enregistrez un premier échantillon dans Ma voix »).
pub fn verifier(cfg: &Config) -> Vec<Verification> {
    let mut res = Vec::new();
    let dir = &cfg.alltalk_dir;

    // 1. Dossier AllTalk.
    let script = dir.join("start_alltalk.sh");
    let voices = dir.join("voices");
    let (ok1, detail1) = if !dir.is_dir() {
        (false, format!("Le dossier d'AllTalk est introuvable : {}. Installez AllTalk ou corrigez le dossier dans les réglages.", dir.display()))
    } else if !script.is_file() {
        (false, format!("Le dossier {} existe, mais il ne contient pas start_alltalk.sh : ce n'est pas une installation d'AllTalk.", dir.display()))
    } else if !voices.is_dir() {
        (false, format!("Le dossier {} ne contient pas de sous-dossier voices : l'installation d'AllTalk semble incomplète.", dir.display()))
    } else {
        (true, format!("Trouvé : {}", dir.display()))
    };
    res.push(Verification {
        composant: "Serveur AllTalk".into(),
        ok: ok1,
        gravite: Gravite::Bloquant,
        detail: detail1,
        lien: Some("https://github.com/erew123/alltalk_tts"),
        commande: Some(
            "git clone -b alltalkbeta https://github.com/erew123/alltalk_tts ~/alltalk_tts && cd ~/alltalk_tts && ./atsetup.sh".into(),
        ),
    });

    // 2. Moteur XTTS : configuré ET modèle présent.
    let moteur = moteur_configure(dir);
    let modele = modele_xtts_present(dir);
    let (ok2, detail2) = match (moteur.as_deref(), modele) {
        (None, _) => (
            false,
            format!("Impossible de lire la configuration des moteurs d'AllTalk ({}). Installez AllTalk puis lancez-le une première fois.",
                dir.join("system/tts_engines/tts_engines.json").display()),
        ),
        (Some(m), _) if m != "xtts" => (false, format!("Moteur configuré : {m} (il faut xtts). Utilisez le bouton de configuration, puis redémarrez AllTalk.")),
        (Some(_), false) => (
            false,
            "Modèle XTTS absent de models/xtts/ : lancez AllTalk une première fois et téléchargez le modèle dans son interface web.".into(),
        ),
        (Some(_), true) => (true, "Moteur XTTS configuré et modèle présent.".into()),
    };
    res.push(Verification {
        composant: "Moteur XTTS".into(),
        ok: ok2,
        gravite: Gravite::Bloquant,
        detail: detail2,
        lien: Some("https://github.com/erew123/alltalk_tts/wiki"),
        commande: None,
    });

    // 3. GPU NVIDIA.
    let gpu = executer("nvidia-smi", &["--query-gpu=name,memory.total", "--format=csv,noheader"])
        .map(|t| t.lines().next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty());
    res.push(Verification {
        composant: "Carte graphique NVIDIA".into(),
        ok: gpu.is_some(),
        gravite: Gravite::Fonction,
        detail: match &gpu {
            Some(l) => format!("Trouvée : {l}"),
            None => "Aucune carte NVIDIA détectée (nvidia-smi absent ou muet). La synthèse fonctionnera sur le processeur, beaucoup plus lentement.".into(),
        },
        lien: Some("https://www.nvidia.com/drivers"),
        commande: Some("sudo ubuntu-drivers install".into()),
    });

    // 4. PipeWire.
    let manquants: Vec<&str> = ["pw-record", "pw-play", "pactl"].into_iter().filter(|p| !dans_le_path(p)).collect();
    res.push(Verification {
        composant: "PipeWire (micro et écoute)".into(),
        ok: manquants.is_empty(),
        gravite: Gravite::Fonction,
        detail: if manquants.is_empty() {
            "Trouvé : pw-record, pw-play et pactl.".into()
        } else {
            format!("Programmes manquants : {}. L'enregistrement et l'écoute ne marcheront pas.", manquants.join(", "))
        },
        lien: Some("https://pipewire.org/"),
        commande: Some("sudo apt install pipewire-bin pulseaudio-utils".into()),
    });

    // 5. ffmpeg.
    let ffmpeg = executer("ffmpeg", &["-version"]).map(|t| t.lines().next().unwrap_or("").trim().to_string());
    res.push(Verification {
        composant: "ffmpeg".into(),
        ok: ffmpeg.is_some(),
        gravite: Gravite::Fonction,
        detail: match &ffmpeg {
            Some(v) if !v.is_empty() => format!("Trouvé : {v}"),
            Some(_) => "Trouvé.".into(),
            None => "ffmpeg est absent : seul l'export WAV est possible (pas FLAC, MP3 ni OGG).".into(),
        },
        lien: Some("https://ffmpeg.org/download.html"),
        commande: Some("sudo apt install ffmpeg".into()),
    });

    // 6. Jeu de voix actif.
    let dossier_jeu = cfg.voicesets_dir().join(&cfg.jeu_actif);
    let nb = crate::voicesets::echantillons(&cfg.voicesets_dir(), &cfg.jeu_actif).len();
    res.push(Verification {
        composant: "Jeu de voix".into(),
        ok: nb > 0,
        gravite: Gravite::Fonction,
        detail: if nb > 0 {
            format!("Jeu « {} » : {nb} échantillon(s) dans {}", cfg.jeu_actif, dossier_jeu.display())
        } else {
            format!("Le jeu de voix « {} » est vide ou absent. Enregistrez un premier échantillon dans Ma voix.", cfg.jeu_actif)
        },
        lien: None,
        commande: None,
    });
    res
}

/// `models/xtts/` contient-il un sous-dossier avec `model.pth` ?
fn modele_xtts_present(alltalk_dir: &Path) -> bool {
    std::fs::read_dir(alltalk_dir.join("models").join("xtts"))
        .map(|d| d.flatten().any(|e| e.path().join("model.pth").is_file()))
        .unwrap_or(false)
}

/// Lance une commande (entrées/sorties capturées) et rend sa sortie standard si elle réussit
/// en moins de 2 s ; sinon (absente, en erreur, trop lente → tuée) None.
fn executer(programme: &str, args: &[&str]) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new(programme)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let limite = Instant::now() + Duration::from_secs(2);
    let statut = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < limite => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !statut.success() {
        return None;
    }
    let mut sortie = String::new();
    child.stdout.take()?.read_to_string(&mut sortie).ok()?;
    Some(sortie)
}

/// Vrai si au moins une vérification bloquante échoue.
pub fn bloque(liste: &[Verification]) -> bool {
    liste.iter().any(|v| !v.ok && v.gravite == Gravite::Bloquant)
}

/// Moteur configuré au démarrage d'AllTalk : clé `engine_loaded` de
/// `<alltalk_dir>/system/tts_engines/tts_engines.json`. None si le fichier est absent ou illisible.
pub fn moteur_configure(alltalk_dir: &Path) -> Option<String> {
    let json = lire_moteurs(alltalk_dir).ok()?;
    json.get("engine_loaded")?.as_str().map(str::to_string)
}

fn fichier_moteurs(alltalk_dir: &Path) -> std::path::PathBuf {
    alltalk_dir.join("system").join("tts_engines").join("tts_engines.json")
}

fn lire_moteurs(alltalk_dir: &Path) -> Result<serde_json::Value, String> {
    let f = fichier_moteurs(alltalk_dir);
    let txt = std::fs::read_to_string(&f)
        .map_err(|e| format!("Impossible de lire la configuration des moteurs d'AllTalk ({}) : {e}", f.display()))?;
    serde_json::from_str(&txt)
        .map_err(|e| format!("La configuration des moteurs d'AllTalk ({}) est illisible : {e}", f.display()))
}

/// Force le moteur XTTS dans ce fichier : `engine_loaded = "xtts"` et `selected_model` = l'entrée
/// de `engines_available` dont `name == "xtts"` (sinon `"xtts - xttsv2_2.0.3"`). Écriture atomique
/// (`.tmp` + rename), puis relecture de contrôle. Refuse si `engines_available` ne contient pas
/// `xtts`. Le fichier est réécrit avec des clés triées et 2 espaces d'indentation (contenu identique).
/// AllTalk doit être redémarré ensuite (c'est l'interface qui le dit).
/// On n'utilise pas `/api/enginereload` : il plante dans cette version d'AllTalk.
pub fn configurer_xtts(alltalk_dir: &Path) -> Result<(), String> {
    let mut json = lire_moteurs(alltalk_dir)?;
    let entree = json
        .get("engines_available")
        .and_then(|e| e.as_array())
        .and_then(|liste| liste.iter().find(|e| e.get("name").and_then(|n| n.as_str()) == Some("xtts")))
        .ok_or("Le moteur XTTS n'est pas installé dans cette version d'AllTalk (il est absent de la liste des moteurs).")?;
    let modele = entree
        .get("selected_model")
        .and_then(|m| m.as_str())
        .unwrap_or("xtts - xttsv2_2.0.3")
        .to_string();
    let objet = json.as_object_mut().ok_or("La configuration des moteurs d'AllTalk a un format inattendu.")?;
    objet.insert("engine_loaded".into(), "xtts".into());
    objet.insert("selected_model".into(), modele.into());
    let fichier = fichier_moteurs(alltalk_dir);
    let texte = serde_json::to_string_pretty(&json).map_err(|e| format!("Impossible de préparer la configuration : {e}"))?;
    let tmp = fichier.with_file_name("tts_engines.json.tmp");
    let echec = |e: std::io::Error| format!("Impossible d'écrire {} : {e}", fichier.display());
    std::fs::write(&tmp, texte).map_err(echec)?;
    std::fs::rename(&tmp, &fichier).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        echec(e)
    })?;
    if moteur_configure(alltalk_dir).as_deref() != Some("xtts") {
        return Err("La modification a été écrite mais la relecture ne donne pas XTTS : vérifiez le fichier tts_engines.json.".into());
    }
    Ok(())
}

/// Un programme est-il dans le PATH ? (parcours de `PATH`, fichier exécutable)
pub fn dans_le_path(programme: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if programme.is_empty() {
        return false;
    }
    std::env::var_os("PATH")
        .map(|chemins| {
            std::env::split_paths(&chemins).any(|d| {
                std::fs::metadata(d.join(programme)).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(e: &str) -> Tmp {
            let d = std::env::temp_dir().join(format!("vocalise-test-diag-{}-{e}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Tmp(d)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ecrire_moteurs(dir: &Path, contenu: &str) {
        let f = fichier_moteurs(dir);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, contenu).unwrap();
    }

    const AVEC_XTTS: &str = r#"{"engines_available":[{"name":"piper","selected_model":"piper"},
        {"name":"xtts","selected_model":"xtts - xttsv2_2.0.3"}],
        "engine_loaded":"piper","selected_model":"piper","autre":42}"#;

    #[test]
    fn path() {
        assert!(dans_le_path("sh"));
        assert!(!dans_le_path("programme-vraiment-improbable-xyz"));
        assert!(!dans_le_path(""));
    }

    #[test]
    fn moteur_lu_et_configure() {
        let t = Tmp::new("moteur");
        assert_eq!(moteur_configure(&t.0), None); // fichier absent
        assert!(configurer_xtts(&t.0).is_err());
        ecrire_moteurs(&t.0, "pas du json");
        assert_eq!(moteur_configure(&t.0), None);
        assert!(configurer_xtts(&t.0).is_err());
        ecrire_moteurs(&t.0, AVEC_XTTS);
        assert_eq!(moteur_configure(&t.0).as_deref(), Some("piper"));
        configurer_xtts(&t.0).unwrap();
        assert_eq!(moteur_configure(&t.0).as_deref(), Some("xtts"));
        let j = lire_moteurs(&t.0).unwrap();
        assert_eq!(j["selected_model"], "xtts - xttsv2_2.0.3");
        assert_eq!(j["autre"], 42); // le reste est conservé
        assert!(!fichier_moteurs(&t.0).with_file_name("tts_engines.json.tmp").exists());
    }

    #[test]
    fn configurer_sans_entree_xtts_refuse() {
        let t = Tmp::new("sans-xtts");
        ecrire_moteurs(&t.0, r#"{"engines_available":[{"name":"piper","selected_model":"piper"}],"engine_loaded":"piper"}"#);
        let e = configurer_xtts(&t.0).unwrap_err();
        assert!(e.contains("XTTS"), "{e}");
        assert_eq!(moteur_configure(&t.0).as_deref(), Some("piper")); // inchangé
    }

    fn cfg_pour(dir: &Path) -> Config {
        let mut c = Config::default();
        c.alltalk_dir = dir.join("alltalk");
        c.vocalise_dir = dir.join("vocalise");
        c.sorties_dir = c.vocalise_dir.join("sorties");
        c.jeu_actif = "moi".into();
        c
    }

    #[test]
    fn verifier_dossier_vide() {
        let t = Tmp::new("vide");
        let v = verifier(&cfg_pour(&t.0));
        assert_eq!(v.len(), 6);
        assert!(!v[0].ok && !v[1].ok && !v[5].ok);
        assert_eq!(v[0].gravite, Gravite::Bloquant);
        assert_eq!(v[1].gravite, Gravite::Bloquant);
        assert!(bloque(&v));
        assert!(v.iter().all(|x| !x.detail.is_empty()));
    }

    #[test]
    fn verifier_faux_alltalk_complet() {
        let t = Tmp::new("complet");
        let cfg = cfg_pour(&t.0);
        let a = &cfg.alltalk_dir;
        std::fs::create_dir_all(a.join("voices")).unwrap();
        std::fs::write(a.join("start_alltalk.sh"), "#!/bin/sh\n").unwrap();
        ecrire_moteurs(a, r#"{"engines_available":[{"name":"xtts","selected_model":"xtts - xttsv2_2.0.3"}],"engine_loaded":"xtts"}"#);
        // Moteur configuré mais modèle absent : point 2 en échec avec le bon message.
        let v = verifier(&cfg);
        assert!(v[0].ok && !v[1].ok);
        assert!(v[1].detail.contains("models/xtts"), "{}", v[1].detail);
        std::fs::create_dir_all(a.join("models/xtts/xttsv2_2.0.3")).unwrap();
        std::fs::write(a.join("models/xtts/xttsv2_2.0.3/model.pth"), b"x").unwrap();
        let v = verifier(&cfg);
        assert!(v[0].ok && v[1].ok, "{} / {}", v[0].detail, v[1].detail);
        assert!(!bloque(&v));
        assert!(v[0].detail.starts_with("Trouvé : "));
        // Mauvais moteur : message explicite.
        ecrire_moteurs(a, r#"{"engines_available":[],"engine_loaded":"piper"}"#);
        let v = verifier(&cfg);
        assert!(!v[1].ok && v[1].detail.contains("piper"), "{}", v[1].detail);
    }
}
