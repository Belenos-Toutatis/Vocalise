//! Enregistrement micro via PipeWire (`pw-record`) et liste des sources (`pactl`).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
pub struct Micro {
    /// Identifiant PipeWire (ex. alsa_input.usb-DJI_...analog-stereo).
    pub id: String,
    /// Libellé lisible : description, sinon device.product.name, sinon alsa.card_name, sinon id.
    pub nom: String,
}

/// Sources d'entrée (`pactl -f json list sources`, sans les ".monitor"). Vide si pactl absent.
pub fn micros() -> Vec<Micro> {
    let sortie = match Command::new("pactl")
        .args(["-f", "json", "list", "sources"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    analyser_sources(&sortie)
}

/// Extrait les micros du JSON de `pactl -f json list sources`.
fn analyser_sources(json: &[u8]) -> Vec<Micro> {
    let valeur: serde_json::Value = match serde_json::from_slice(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let texte_utile = |v: Option<&serde_json::Value>| -> Option<String> {
        v.and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|t| !t.is_empty() && *t != "(null)")
            .map(str::to_string)
    };
    valeur
        .as_array()
        .map(|sources| {
            sources
                .iter()
                .filter_map(|src| {
                    let id = src.get("name")?.as_str()?.to_string();
                    if id.is_empty() || id.ends_with(".monitor") {
                        return None;
                    }
                    let props = src.get("properties");
                    let nom = texte_utile(src.get("description"))
                        .or_else(|| texte_utile(props.and_then(|p| p.get("device.product.name"))))
                        .or_else(|| texte_utile(props.and_then(|p| p.get("alsa.card_name"))))
                        .unwrap_or_else(|| id.clone());
                    Some(Micro { id, nom })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Choix par défaut : un micro dont l'id ou le nom contient "DJI", sinon le premier.
pub fn micro_par_defaut(liste: &[Micro]) -> Option<&Micro> {
    liste
        .iter()
        .find(|m| m.id.to_uppercase().contains("DJI") || m.nom.to_uppercase().contains("DJI"))
        .or_else(|| liste.first())
}

pub struct Enregistreur {
    child: Child,
    fichier: PathBuf,
    debut: Instant,
}

impl Enregistreur {
    /// `pw-record [--target <micro>] --rate 22050 --channels 1 --format s16 <fichier>`.
    pub fn demarrer(micro: Option<&str>, fichier: &Path) -> Result<Enregistreur, String> {
        if let Some(parent) = fichier.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Impossible de créer le dossier {} : {e}", parent.display()))?;
        }
        let _ = std::fs::remove_file(fichier); // pas de reste d'un enregistrement précédent
        let mut cmd = Command::new("pw-record");
        if let Some(m) = micro.filter(|m| !m.is_empty()) {
            cmd.arg("--target").arg(m);
        }
        let child = cmd
            .args(["--rate", "22050", "--channels", "1", "--format", "s16"])
            .arg(fichier)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "pw-record est introuvable : l'enregistrement nécessite PipeWire (paquet pipewire-bin).".to_string()
                } else {
                    format!("Impossible de lancer l'enregistrement : {e}")
                }
            })?;
        Ok(Enregistreur { child, fichier: fichier.to_path_buf(), debut: Instant::now() })
    }

    /// Envoie SIGINT (pw-record finalise alors l'en-tête wav), attend la fin, rend le fichier.
    pub fn arreter(mut self) -> Result<PathBuf, String> {
        use std::time::Duration;
        let pid = self.child.id();
        let mut deja_fini = matches!(self.child.try_wait(), Ok(Some(_)));
        if !deja_fini {
            let _ = Command::new("kill")
                .arg("-INT")
                .arg(pid.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            // pw-record finalise l'en-tête avant de quitter ; on lui laisse 5 s au plus.
            let limite = Instant::now() + Duration::from_secs(5);
            while Instant::now() < limite {
                if !matches!(self.child.try_wait(), Ok(None)) {
                    deja_fini = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if !deja_fini {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
        let taille = std::fs::metadata(&self.fichier).map(|m| m.len()).unwrap_or(0);
        if taille <= 44 {
            return Err(
                "Aucun son n'a été enregistré. Vérifiez que le micro est branché et bien choisi dans les réglages.".into(),
            );
        }
        Ok(self.fichier.clone())
    }

    pub fn duree(&self) -> f32 {
        self.debut.elapsed().as_secs_f32()
    }

    /// Niveau courant (RMS 0..1 des 100 dernières ms) d'après le fichier en cours d'écriture.
    pub fn niveau(&self) -> f32 {
        crate::audio::niveau_direct(&self.fichier, 0.1).unwrap_or(0.0)
    }

}

impl Drop for Enregistreur {
    /// Si l'enregistreur est abandonné sans `arreter`, ne pas laisser pw-record tourner.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyse_pactl() {
        let json = r#"[
          {"name":"alsa_output.pci.analog-stereo.monitor","description":"Monitor","properties":{}},
          {"name":"alsa_input.usb-DJI_Mic-00.analog-stereo","description":"(null)",
           "properties":{"device.product.name":"DJI MIC MINI","alsa.card_name":"DJI"}},
          {"name":"alsa_input.pci-0000.analog-stereo","description":"Audio interne Stéréo analogique",
           "properties":{"device.product.name":"X"}},
          {"name":"alsa_input.autre","properties":{"alsa.card_name":"Carte"}},
          {"name":"alsa_input.rien","description":"","properties":{}}
        ]"#;
        let m = analyser_sources(json.as_bytes());
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].nom, "DJI MIC MINI");
        assert_eq!(m[1].nom, "Audio interne Stéréo analogique");
        assert_eq!(m[2].nom, "Carte");
        assert_eq!(m[3].nom, "alsa_input.rien");
        assert_eq!(micro_par_defaut(&m).unwrap().id, "alsa_input.usb-DJI_Mic-00.analog-stereo");
        assert_eq!(micro_par_defaut(&m[1..]).unwrap().id, "alsa_input.pci-0000.analog-stereo");
        assert!(micro_par_defaut(&[]).is_none());
        assert!(analyser_sources(b"pas du json").is_empty());
        assert!(analyser_sources(b"{}").is_empty());
    }
}
