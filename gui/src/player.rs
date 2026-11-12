//! Lecture audio. Linux : `pw-play`. Windows : WASAPI via cpal (module `wasapi` plus bas).

#[cfg(windows)]
pub use wasapi::{sortie_par_defaut, Lecteur};

#[cfg(not(windows))]
use std::path::Path;
#[cfg(not(windows))]
use std::process::{Child, Command, Stdio};

#[cfg(not(windows))]
#[derive(Default)]
pub struct Lecteur {
    child: Option<Child>,
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
impl Drop for Lecteur {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Windows : lecture par le thread audio unique (module `wasapi`).
#[cfg(windows)]
mod wasapi {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    pub use crate::wasapi::sortie_par_defaut;

    #[derive(Default)]
    pub struct Lecteur {
        courant: Option<(u64, Arc<AtomicBool>)>,
    }

    impl Lecteur {
        pub fn new() -> Self {
            Self::default()
        }

        /// Arrête la lecture en cours puis lit `fichier` (wav lu directement ; FLAC, MP3, OGG
        /// convertis d'abord en wav temporaire par ffmpeg).
        pub fn jouer(&mut self, fichier: &Path) -> Result<(), String> {
            self.stop();
            if !fichier.is_file() {
                return Err(format!("Le fichier audio {} est introuvable.", fichier.display()));
            }
            let wav = charger(fichier)?;
            let fini = Arc::new(AtomicBool::new(false));
            let id = crate::wasapi::jouer(wav, fini.clone())?;
            self.courant = Some((id, fini));
            Ok(())
        }

        /// Arrête la lecture en cours (sans effet s'il n'y en a pas).
        pub fn stop(&mut self) {
            if let Some((id, _)) = self.courant.take() {
                crate::wasapi::arreter_lecture(id);
            }
        }

        /// Vrai tant que le son n'est pas fini.
        pub fn en_cours(&mut self) -> bool {
            match &self.courant {
                Some((_, fini)) if !fini.load(Ordering::Relaxed) => true,
                Some(_) => {
                    self.courant = None;
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

    fn charger(fichier: &Path) -> Result<crate::audio::Wav, String> {
        let est_wav = fichier.extension().map(|e| e.eq_ignore_ascii_case("wav")).unwrap_or(false);
        if est_wav {
            return crate::audio::lire(fichier);
        }
        let tmp: PathBuf = std::env::temp_dir().join(format!("vocalise-ecoute-{}.wav", std::process::id()));
        let ok = crate::systeme::commande("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(fichier)
            .args(["-ac", "1"])
            .arg(&tmp)
            .stdin(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err("ffmpeg est nécessaire pour écouter ce format (installez-le, ou exportez en WAV).".into());
        }
        let r = crate::audio::lire(&tmp);
        let _ = std::fs::remove_file(&tmp);
        r
    }
}
