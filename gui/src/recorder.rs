//! Enregistrement micro et liste des micros.
//! Linux : PipeWire (`pw-record`, `pactl`). Windows : WASAPI via cpal (module `wasapi` plus bas).

#[cfg(not(windows))]
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::{Child, Command, Stdio};
#[cfg(not(windows))]
use std::time::Instant;

#[cfg(windows)]
pub use wasapi::{micros, Enregistreur};

#[derive(Clone, Debug, PartialEq)]
pub struct Micro {
    /// Identifiant PipeWire (ex. alsa_input.usb-DJI_...analog-stereo).
    pub id: String,
    /// Libellé lisible : description, sinon device.product.name, sinon alsa.card_name, sinon id.
    pub nom: String,
}

/// Sources d'entrée (`pactl -f json list sources`, sans les ".monitor"). Vide si pactl absent.
#[cfg(not(windows))]
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
#[cfg_attr(windows, allow(dead_code))]
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

#[cfg(not(windows))]
pub struct Enregistreur {
    child: Child,
    fichier: PathBuf,
    debut: Instant,
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
impl Drop for Enregistreur {
    /// Si l'enregistreur est abandonné sans `arreter`, ne pas laisser pw-record tourner.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// Windows : capture WASAPI dans un thread dédié (le flux cpal n'est pas `Send`), écriture d'un
/// wav PCM 16 bits mono à la fréquence native du micro, niveau mesuré dans le rappel audio.
#[cfg(windows)]
mod wasapi {
    use super::Micro;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    type Ecrivain = hound::WavWriter<std::io::BufWriter<std::fs::File>>;

    /// Micros vus par WASAPI ; l'identifiant est le nom du périphérique.
    pub fn micros() -> Vec<Micro> {
        let hote = cpal::default_host();
        let Ok(liste) = hote.input_devices() else { return Vec::new() };
        let mut v: Vec<Micro> = liste
            .filter_map(|d| d.name().ok())
            .filter(|n| !n.trim().is_empty())
            .map(|n| Micro { id: n.clone(), nom: n })
            .collect();
        v.dedup_by(|a, b| a.id == b.id);
        v
    }

    fn trouver(micro: Option<&str>) -> Result<cpal::Device, String> {
        let hote = cpal::default_host();
        if let Some(nom) = micro.filter(|m| !m.is_empty()) {
            if let Ok(mut liste) = hote.input_devices() {
                if let Some(d) = liste.find(|d| d.name().map(|n| n == nom).unwrap_or(false)) {
                    return Ok(d);
                }
            }
        }
        hote.default_input_device()
            .ok_or_else(|| "Aucun micro n'est disponible. Branchez-en un, ou autorisez l'accès au micro dans Paramètres → Confidentialité → Microphone.".into())
    }

    pub struct Enregistreur {
        fichier: PathBuf,
        debut: Instant,
        arret: Option<mpsc::Sender<()>>,
        fil: Option<JoinHandle<Result<(), String>>>,
        niveau: Arc<AtomicU32>,
    }

    impl Enregistreur {
        pub fn demarrer(micro: Option<&str>, fichier: &Path) -> Result<Enregistreur, String> {
            if let Some(parent) = fichier.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Impossible de créer le dossier {} : {e}", parent.display()))?;
            }
            let _ = std::fs::remove_file(fichier);
            let micro = micro.map(str::to_string);
            let chemin = fichier.to_path_buf();
            let niveau = Arc::new(AtomicU32::new(0));
            let niveau_fil = niveau.clone();
            let (tx_arret, rx_arret) = mpsc::channel::<()>();
            let (tx_pret, rx_pret) = mpsc::channel::<Result<(), String>>();
            let fil = std::thread::spawn(move || -> Result<(), String> {
                let ecrivain: Arc<Mutex<Option<Ecrivain>>> = Arc::new(Mutex::new(None));
                let flux = match ouvrir(micro.as_deref(), &chemin, &ecrivain, niveau_fil) {
                    Ok(f) => {
                        let _ = tx_pret.send(Ok(()));
                        f
                    }
                    Err(e) => {
                        let _ = tx_pret.send(Err(e.clone()));
                        return Err(e);
                    }
                };
                // Attend l'ordre d'arrêt (ou l'abandon de l'enregistreur).
                let _ = rx_arret.recv();
                drop(flux);
                let w = ecrivain.lock().map_err(|_| "enregistrement interrompu".to_string())?.take();
                if let Some(w) = w {
                    w.finalize().map_err(|e| format!("Impossible de finaliser l'enregistrement : {e}"))?;
                }
                Ok(())
            });
            match rx_pret.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => Ok(Enregistreur {
                    fichier: fichier.to_path_buf(),
                    debut: Instant::now(),
                    arret: Some(tx_arret),
                    fil: Some(fil),
                    niveau,
                }),
                Ok(Err(e)) => {
                    let _ = fil.join();
                    Err(e)
                }
                Err(_) => {
                    drop(tx_arret);
                    Err("Le micro ne répond pas (délai dépassé).".into())
                }
            }
        }

        /// Arrête la capture, finalise le wav, rend le fichier.
        pub fn arreter(mut self) -> Result<PathBuf, String> {
            self.terminer()?;
            let taille = std::fs::metadata(&self.fichier).map(|m| m.len()).unwrap_or(0);
            if taille <= 44 {
                return Err(
                    "Aucun son n'a été enregistré. Vérifiez que le micro est branché et bien choisi dans les réglages.".into(),
                );
            }
            Ok(self.fichier.clone())
        }

        fn terminer(&mut self) -> Result<(), String> {
            self.arret.take(); // fermer le canal réveille le thread
            match self.fil.take().map(|f| f.join()) {
                Some(Ok(r)) => r,
                Some(Err(_)) => Err("L'enregistrement s'est interrompu de façon inattendue.".into()),
                None => Ok(()),
            }
        }

        pub fn duree(&self) -> f32 {
            self.debut.elapsed().as_secs_f32()
        }

        /// Niveau RMS (0..1) du dernier paquet reçu du micro (~10 ms).
        pub fn niveau(&self) -> f32 {
            f32::from_bits(self.niveau.load(Ordering::Relaxed))
        }
    }

    impl Drop for Enregistreur {
        fn drop(&mut self) {
            let _ = self.terminer();
        }
    }

    fn ouvrir(
        micro: Option<&str>,
        chemin: &Path,
        ecrivain: &Arc<Mutex<Option<Ecrivain>>>,
        niveau: Arc<AtomicU32>,
    ) -> Result<cpal::Stream, String> {
        let device = trouver(micro)?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Le micro refuse l'enregistrement : {e}"))?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: config.sample_rate().0,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let w = hound::WavWriter::create(chemin, spec)
            .map_err(|e| format!("Impossible de créer {} : {e}", chemin.display()))?;
        *ecrivain.lock().map_err(|_| "verrou empoisonné".to_string())? = Some(w);
        let canaux = config.channels() as usize;
        let format = config.sample_format();
        let cfg: cpal::StreamConfig = config.into();
        let flux = match format {
            cpal::SampleFormat::F32 => construire::<f32>(&device, &cfg, canaux, ecrivain.clone(), niveau),
            cpal::SampleFormat::I16 => construire::<i16>(&device, &cfg, canaux, ecrivain.clone(), niveau),
            cpal::SampleFormat::U16 => construire::<u16>(&device, &cfg, canaux, ecrivain.clone(), niveau),
            cpal::SampleFormat::I32 => construire::<i32>(&device, &cfg, canaux, ecrivain.clone(), niveau),
            autre => return Err(format!("Format de micro non géré : {autre:?}")),
        }
        .map_err(|e| format!("Impossible d'ouvrir le micro : {e}"))?;
        flux.play().map_err(|e| format!("Impossible de démarrer le micro : {e}"))?;
        Ok(flux)
    }

    fn construire<T>(
        device: &cpal::Device,
        cfg: &cpal::StreamConfig,
        canaux: usize,
        ecrivain: Arc<Mutex<Option<Ecrivain>>>,
        niveau: Arc<AtomicU32>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let canaux = canaux.max(1);
        device.build_input_stream(
            cfg,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut somme = 0.0f64;
                let mut n = 0usize;
                let mut garde = ecrivain.lock().ok();
                let w = garde.as_mut().and_then(|g| g.as_mut());
                let mut w = w;
                for trame in data.chunks(canaux) {
                    let v: f32 = trame.iter().map(|&x| x.to_sample::<f32>()).sum::<f32>() / trame.len() as f32;
                    somme += (v as f64) * (v as f64);
                    n += 1;
                    if let Some(w) = w.as_mut() {
                        let _ = w.write_sample((v.clamp(-1.0, 1.0) * 32767.0).round() as i16);
                    }
                }
                if n > 0 {
                    niveau.store(((somme / n as f64).sqrt() as f32).to_bits(), Ordering::Relaxed);
                }
            },
            |e| eprintln!("micro : {e}"),
            None,
        )
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
