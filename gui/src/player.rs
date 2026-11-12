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

#[cfg(windows)]
mod wasapi {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::JoinHandle;
    use std::time::Duration;

    /// Nom de la sortie son par défaut, s'il y en a une.
    pub fn sortie_par_defaut() -> Option<String> {
        cpal::default_host().default_output_device().and_then(|d| d.name().ok())
    }

    #[derive(Default)]
    pub struct Lecteur {
        arret: Option<mpsc::Sender<()>>,
        fini: Arc<AtomicBool>,
        fil: Option<JoinHandle<()>>,
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
            let fini_fil = fini.clone();
            let (tx_arret, rx_arret) = mpsc::channel::<()>();
            let (tx_pret, rx_pret) = mpsc::channel::<Result<(), String>>();
            let fil = std::thread::spawn(move || {
                let flux = match ouvrir(wav, fini_fil.clone()) {
                    Ok(f) => {
                        let _ = tx_pret.send(Ok(()));
                        f
                    }
                    Err(e) => {
                        fini_fil.store(true, Ordering::Relaxed);
                        let _ = tx_pret.send(Err(e));
                        return;
                    }
                };
                // Jusqu'à la fin du son ou l'ordre d'arrêt.
                while !fini_fil.load(Ordering::Relaxed) {
                    match rx_arret.recv_timeout(Duration::from_millis(50)) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        _ => break,
                    }
                }
                drop(flux);
                fini_fil.store(true, Ordering::Relaxed);
            });
            match rx_pret.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => {
                    self.arret = Some(tx_arret);
                    self.fini = fini;
                    self.fil = Some(fil);
                    Ok(())
                }
                Ok(Err(e)) => {
                    let _ = fil.join();
                    Err(e)
                }
                Err(_) => Err("La sortie son ne répond pas (délai dépassé).".into()),
            }
        }

        /// Arrête la lecture en cours (sans effet s'il n'y en a pas).
        pub fn stop(&mut self) {
            self.arret.take();
            if let Some(f) = self.fil.take() {
                let _ = f.join();
            }
        }

        /// Vrai tant que le son n'est pas fini.
        pub fn en_cours(&mut self) -> bool {
            if self.fil.is_some() && self.fini.load(Ordering::Relaxed) {
                self.stop();
            }
            self.fil.is_some()
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

    fn ouvrir(wav: crate::audio::Wav, fini: Arc<AtomicBool>) -> Result<cpal::Stream, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("Aucune sortie son n'est disponible.")?;
        let config = device
            .default_output_config()
            .map_err(|e| format!("La sortie son refuse la lecture : {e}"))?;
        let canaux = config.channels() as usize;
        let format = config.sample_format();
        let cfg: cpal::StreamConfig = config.into();
        let pas = wav.rate as f64 / cfg.sample_rate.0 as f64;
        let source = Source { samples: wav.samples, pos: 0.0, pas, fini };
        let flux = match format {
            cpal::SampleFormat::F32 => construire::<f32>(&device, &cfg, canaux, source),
            cpal::SampleFormat::I16 => construire::<i16>(&device, &cfg, canaux, source),
            cpal::SampleFormat::U16 => construire::<u16>(&device, &cfg, canaux, source),
            cpal::SampleFormat::I32 => construire::<i32>(&device, &cfg, canaux, source),
            autre => return Err(format!("Format de sortie son non géré : {autre:?}")),
        }
        .map_err(|e| format!("Impossible d'ouvrir la sortie son : {e}"))?;
        flux.play().map_err(|e| format!("Impossible de démarrer la lecture : {e}"))?;
        Ok(flux)
    }

    /// Signal mono lu à la fréquence de la carte son par interpolation linéaire.
    struct Source {
        samples: Vec<f32>,
        pos: f64,
        pas: f64,
        fini: Arc<AtomicBool>,
    }

    impl Source {
        fn suivant(&mut self) -> f32 {
            let i = self.pos as usize;
            if i + 1 >= self.samples.len() {
                self.fini.store(true, Ordering::Relaxed);
                return 0.0;
            }
            let t = (self.pos - i as f64) as f32;
            self.pos += self.pas;
            self.samples[i] * (1.0 - t) + self.samples[i + 1] * t
        }
    }

    fn construire<T>(
        device: &cpal::Device,
        cfg: &cpal::StreamConfig,
        canaux: usize,
        mut source: Source,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        use cpal::Sample;
        let canaux = canaux.max(1);
        device.build_output_stream(
            cfg,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for trame in data.chunks_mut(canaux) {
                    let v = source.suivant().to_sample::<T>();
                    for x in trame.iter_mut() {
                        *x = v;
                    }
                }
            },
            |e| eprintln!("sortie son : {e}"),
            None,
        )
    }
}
