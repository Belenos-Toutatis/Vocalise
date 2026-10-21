//! Configuration persistante et chemins du projet.
//! Fichier : ~/.config/vocalise/config.json

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Dossier d'installation d'AllTalk (contient start_alltalk.sh et voices/).
    pub alltalk_dir: PathBuf,
    /// Dossier du projet Vocalise (echantillons/, sorties/, gui/textes/).
    pub vocalise_dir: PathBuf,
    /// Dossier où écrire les wav générés.
    pub sorties_dir: PathBuf,
    /// Code langue envoyé à AllTalk ("fr").
    pub langue: String,
    /// Voix sélectionnée dans l'onglet Synthèse (nom de fichier ou "voiceset:nom").
    pub voix: String,
    /// Jeu de voix actif pour l'enregistrement.
    pub jeu_actif: String,
    /// Identifiant PipeWire du micro choisi ("" = source par défaut).
    pub micro: String,
    pub vitesse: f32,
    pub temperature: f32,
    /// Format d'export des fichiers générés (wav par défaut).
    pub format: crate::audio::Format,
    /// Vérifier au démarrage (au plus une fois par jour) s'il existe une version plus récente.
    pub verifier_maj: bool,
    /// Date (AAAA-MM-JJ) de la dernière vérification de mise à jour réussie.
    pub maj_verifiee_le: Option<String>,
    /// Renseigné par `charger` si le fichier de configuration était illisible (non enregistré).
    #[serde(skip)]
    pub avertissement_chargement: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/home/ewenner".into()));
        let alltalk_dir = std::env::var("ALLTALK_DIR").map(PathBuf::from).unwrap_or(home.join("alltalk_tts"));
        let vocalise_dir = std::env::var("VOCALISE_DIR").map(PathBuf::from).unwrap_or(home.join("Vocalise"));
        Config {
            sorties_dir: vocalise_dir.join("sorties"),
            alltalk_dir,
            vocalise_dir,
            langue: "fr".into(),
            voix: "voiceset:emmanuel".into(),
            jeu_actif: "emmanuel".into(),
            micro: String::new(),
            vitesse: 1.0,
            temperature: 0.7,
            format: crate::audio::Format::Wav,
            verifier_maj: true,
            maj_verifiee_le: None,
            avertissement_chargement: None,
        }
    }
}

impl Config {
    /// Chemin du fichier de configuration.
    pub fn fichier() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
        base.join("vocalise").join("config.json")
    }

    /// Charge la configuration, ou les valeurs par défaut si absente/illisible.
    /// Crée les dossiers de travail (sorties, echantillons, corbeille).
    pub fn charger() -> Config {
        let fichier = Self::fichier();
        let mut avertissement = None;
        let mut cfg = Config::default();
        if fichier.exists() {
            let lecture = std::fs::read_to_string(&fichier)
                .map_err(|e| e.to_string())
                .and_then(|txt| serde_json::from_str::<Config>(&txt).map_err(|e| e.to_string()));
            match lecture {
                Ok(c) => cfg = c,
                Err(erreur) => {
                    // Fichier présent mais illisible : on en garde une copie avant qu'il soit écrasé.
                    let copie = Self::chemin_sauvegarde(&fichier);
                    avertissement = Some(match std::fs::copy(&fichier, &copie) {
                        Ok(_) => format!(
                            "Le fichier de réglages était illisible ({erreur}). Les réglages par défaut sont utilisés ; l'ancien fichier est conservé dans {}.",
                            copie.display()
                        ),
                        Err(e) => format!(
                            "Le fichier de réglages était illisible ({erreur}) et n'a pas pu être sauvegardé ({e}). Les réglages par défaut sont utilisés."
                        ),
                    });
                }
            }
        }
        cfg.avertissement_chargement = avertissement;
        // Création des dossiers de travail ; une erreur ici n'est pas fatale (elle se
        // reproduira, avec un message clair, au moment d'écrire un fichier).
        for d in [&cfg.sorties_dir, &cfg.echantillons_dir(), &cfg.corbeille_dir()] {
            let _ = std::fs::create_dir_all(d);
        }
        cfg
    }

    /// `config.json.bak`, ou `config.json.bak-AAAAMMJJ-HHMMSS[-N]` si ce nom est déjà pris.
    fn chemin_sauvegarde(fichier: &Path) -> PathBuf {
        let nom = fichier.file_name().and_then(|n| n.to_str()).unwrap_or("config.json");
        let simple = fichier.with_file_name(format!("{nom}.bak"));
        if !simple.exists() {
            return simple;
        }
        let horo = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let mut p = fichier.with_file_name(format!("{nom}.bak-{horo}"));
        let mut n = 2;
        while p.exists() {
            p = fichier.with_file_name(format!("{nom}.bak-{horo}-{n}"));
            n += 1;
        }
        p
    }

    /// Enregistre la configuration (crée le dossier parent si besoin). Écriture atomique :
    /// fichier temporaire `config.json.tmp` puis renommage.
    pub fn enregistrer(&self) -> Result<(), String> {
        let fichier = Self::fichier();
        if let Some(parent) = fichier.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("Impossible de créer le dossier de configuration {} : {e}", parent.display())
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Impossible de préparer la configuration : {e}"))?;
        let tmp = fichier.with_file_name("config.json.tmp");
        let erreur = |e: std::io::Error| {
            format!("Impossible d'enregistrer la configuration dans {} : {e}", fichier.display())
        };
        std::fs::write(&tmp, json).map_err(erreur)?;
        std::fs::rename(&tmp, &fichier).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            erreur(e)
        })
    }

    pub fn voices_dir(&self) -> PathBuf {
        self.alltalk_dir.join("voices")
    }
    pub fn voicesets_dir(&self) -> PathBuf {
        self.voices_dir().join("xtts_multi_voice_sets")
    }
    pub fn echantillons_dir(&self) -> PathBuf {
        self.vocalise_dir.join("echantillons")
    }
    /// Les échantillons retirés d'un jeu de voix sont déplacés ici, jamais supprimés.
    pub fn corbeille_dir(&self) -> PathBuf {
        self.vocalise_dir.join("echantillons").join("corbeille")
    }
    pub fn textes_dir(&self) -> PathBuf {
        self.vocalise_dir.join("gui").join("textes")
    }
    pub fn journal(&self) -> PathBuf {
        self.vocalise_dir.join("alltalk.log")
    }
    pub fn start_script(&self) -> PathBuf {
        self.alltalk_dir.join("start_alltalk.sh")
    }
}

/// Horodatage compact pour nommer les fichiers : 20261021-2254
pub fn horodatage() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M").to_string()
}

/// Nom de fichier sûr : lettres, chiffres, tiret, souligné ; accents translittérés grossièrement.
/// Toute autre suite de caractères devient un seul « _ » ; pas de « _ » en début ni en fin.
/// Une chaîne sans aucun caractère exploitable donne « sans_nom ».
pub fn nom_sur(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut dernier_sep = true; // évite un « _ » initial
    let pousser = |out: &mut String, c: char, dernier_sep: &mut bool| {
        out.push(c);
        *dernier_sep = c == '_';
    };
    for c in s.chars() {
        let translit: Option<&str> = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => Some("a"),
            'À' | 'Â' | 'Ä' | 'Á' | 'Ã' | 'Å' => Some("A"),
            'é' | 'è' | 'ê' | 'ë' => Some("e"),
            'É' | 'È' | 'Ê' | 'Ë' => Some("E"),
            'î' | 'ï' | 'í' | 'ì' => Some("i"),
            'Î' | 'Ï' | 'Í' | 'Ì' => Some("I"),
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => Some("o"),
            'Ô' | 'Ö' | 'Ó' | 'Ò' | 'Õ' => Some("O"),
            'ù' | 'û' | 'ü' | 'ú' => Some("u"),
            'Ù' | 'Û' | 'Ü' | 'Ú' => Some("U"),
            'ç' => Some("c"),
            'Ç' => Some("C"),
            'ñ' => Some("n"),
            'Ñ' => Some("N"),
            'ÿ' => Some("y"),
            'œ' => Some("oe"),
            'Œ' => Some("OE"),
            'æ' => Some("ae"),
            'Æ' => Some("AE"),
            _ => None,
        };
        if let Some(t) = translit {
            for ch in t.chars() {
                pousser(&mut out, ch, &mut dernier_sep);
            }
        } else if c.is_ascii_alphanumeric() || c == '-' {
            pousser(&mut out, c, &mut dernier_sep);
        } else if !dernier_sep {
            pousser(&mut out, '_', &mut dernier_sep);
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "sans_nom".to_string()
    } else {
        out
    }
}

/// Lance `xdg-open` sans attendre ni hériter des flux ; un thread récolte le processus fini.
fn lancer_xdg(arg: &std::ffi::OsStr) {
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("xdg-open")
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Ouvre un chemin ou une URL avec l'application par défaut (xdg-open), sans bloquer.
/// Échec silencieux si xdg-open est absent.
pub fn ouvrir(cible: &Path) {
    lancer_xdg(cible.as_os_str());
}

pub fn ouvrir_url(url: &str) {
    lancer_xdg(std::ffi::OsStr::new(url));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_sur_simple() {
        assert_eq!(nom_sur("Cours d'ébullition n°3"), "Cours_d_ebullition_n_3");
        assert_eq!(nom_sur("  Été --- Çà  "), "Ete_---_Ca");
        assert_eq!(nom_sur("a/b\\c:d"), "a_b_c_d");
        assert_eq!(nom_sur("déjà-vu_ok"), "deja-vu_ok");
        assert_eq!(nom_sur("œuvre"), "oeuvre");
    }

    #[test]
    fn nom_sur_vide() {
        assert_eq!(nom_sur(""), "sans_nom");
        assert_eq!(nom_sur("???"), "sans_nom");
        assert_eq!(nom_sur("日本"), "sans_nom");
    }

    #[test]
    fn nom_sur_ne_garde_que_des_caracteres_surs() {
        let r = nom_sur("a b\tc!é@#$%^&*()+=[]{};:'\",.<>?/|`~");
        assert!(r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{r}");
        assert!(!r.contains("__"));
    }

    #[test]
    fn chemins_derives() {
        let c = Config::default();
        assert!(c.voicesets_dir().ends_with("voices/xtts_multi_voice_sets"));
        assert!(c.corbeille_dir().ends_with("echantillons/corbeille"));
    }

    #[test]
    fn ancienne_config_sans_format() {
        let c: Config = serde_json::from_str(r#"{"langue":"en","vitesse":1.2}"#).unwrap();
        assert_eq!(c.format, crate::audio::Format::Wav);
        assert_eq!(c.langue, "en");
        let c: Config = serde_json::from_str(r#"{"format":"Flac"}"#).unwrap();
        assert_eq!(c.format, crate::audio::Format::Flac);
    }

    #[test]
    fn sauvegarde_ne_remplace_pas_un_bak_existant() {
        let d = std::env::temp_dir().join(format!("vocalise-test-bak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("config.json");
        assert_eq!(Config::chemin_sauvegarde(&f), d.join("config.json.bak"));
        std::fs::write(d.join("config.json.bak"), "ancien").unwrap();
        let p = Config::chemin_sauvegarde(&f);
        let _ = std::fs::remove_dir_all(&d);
        assert_ne!(p, d.join("config.json.bak"));
        assert!(p.file_name().unwrap().to_str().unwrap().starts_with("config.json.bak-"));
    }

    #[test]
    fn horodatage_format() {
        let h = horodatage();
        assert_eq!(h.len(), 13);
        assert_eq!(h.as_bytes()[8], b'-');
    }
}
