//! Lecture audio via `pw-play`.

use std::path::Path;
use std::process::{Child, Command, Stdio};

#[derive(Default)]
pub struct Lecteur {
    child: Option<Child>,
}

impl Lecteur {
    pub fn new() -> Self {
        Self::default()
    }

    /// Arrête la lecture en cours puis lance `pw-play <fichier>`.
    pub fn jouer(&mut self, fichier: &Path) -> Result<(), String> {
        self.stop();
        if !fichier.is_file() {
            return Err(format!("Le fichier audio {} est introuvable.", fichier.display()));
        }
        let child = Command::new("pw-play")
            .arg(fichier)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "pw-play est introuvable : la lecture nécessite PipeWire (paquet pipewire-bin).".to_string()
                } else {
                    format!("Impossible de lancer la lecture : {e}")
                }
            })?;
        self.child = Some(child);
        Ok(())
    }

    /// Arrête la lecture en cours (sans effet s'il n'y en a pas).
    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// Vrai tant que pw-play tourne (nettoie l'état quand il a fini).
    pub fn en_cours(&mut self) -> bool {
        match self.child.as_mut().map(|c| c.try_wait()) {
            Some(Ok(None)) => true,
            Some(_) => {
                // Terminé (ou état illisible) : on récolte le processus.
                if let Some(mut c) = self.child.take() {
                    let _ = c.wait();
                }
                false
            }
            None => false,
        }
    }
}

impl Drop for Lecteur {
    fn drop(&mut self) {
        self.stop();
    }
}
