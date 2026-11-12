//! Windows : tout l'audio (cpal / WASAPI) passe par un **unique thread** qui vit autant que le
//! programme. Les objets COM de WASAPI sont liés au thread qui les a créés : utilisés depuis des
//! threads éphémères (diagnostic, enregistrement, lecture…), ils provoquaient une violation
//! d'accès. Les autres modules envoient des ordres et attendent la réponse.
//!
//! Enregistrement : wav PCM 16 bits mono à la fréquence native du micro, niveau mesuré dans le
//! rappel audio. Lecture : signal mono rééchantillonné (interpolation linéaire) vers la carte son.

use crate::audio::Wav;
use crate::recorder::Micro;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;

type Ecrivain = hound::WavWriter<std::io::BufWriter<std::fs::File>>;
type Reponse<T> = mpsc::Sender<T>;

enum Ordre {
    Micros(Reponse<Vec<Micro>>),
    SortieParDefaut(Reponse<Option<String>>),
    Enregistrer { micro: Option<String>, fichier: PathBuf, niveau: Arc<AtomicU32>, reponse: Reponse<Result<u64, String>> },
    ArreterEnregistrement { id: u64, reponse: Reponse<Result<(), String>> },
    Jouer { wav: Wav, fini: Arc<AtomicBool>, reponse: Reponse<Result<u64, String>> },
    ArreterLecture { id: u64 },
}

const DELAI: Duration = Duration::from_secs(10);

fn fil() -> &'static mpsc::Sender<Ordre> {
    static FIL: OnceLock<mpsc::Sender<Ordre>> = OnceLock::new();
    FIL.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("audio-wasapi".into())
            .spawn(move || boucle(rx))
            .expect("thread audio");
        tx
    })
}

/// Envoie un ordre et attend la réponse (10 s au plus).
fn demander<T>(ordre: impl FnOnce(Reponse<T>) -> Ordre) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    fil().send(ordre(tx)).map_err(|_| "Le système audio ne répond plus.".to_string())?;
    rx.recv_timeout(DELAI).map_err(|_| "Le système audio ne répond pas (délai dépassé).".to_string())
}

// ------------------------------------------------------------------ interface pour les modules

/// Micros vus par WASAPI ; l'identifiant est le nom du périphérique.
pub fn micros() -> Vec<Micro> {
    demander(Ordre::Micros).unwrap_or_default()
}

/// Nom de la sortie son par défaut, s'il y en a une.
pub fn sortie_par_defaut() -> Option<String> {
    demander(Ordre::SortieParDefaut).ok().flatten()
}

/// Démarre un enregistrement ; rend son identifiant.
pub fn enregistrer(micro: Option<&str>, fichier: &Path, niveau: Arc<AtomicU32>) -> Result<u64, String> {
    let micro = micro.filter(|m| !m.is_empty()).map(str::to_string);
    let fichier = fichier.to_path_buf();
    demander(|reponse| Ordre::Enregistrer { micro, fichier, niveau, reponse })?
}

/// Arrête l'enregistrement et finalise le wav.
pub fn arreter_enregistrement(id: u64) -> Result<(), String> {
    demander(|reponse| Ordre::ArreterEnregistrement { id, reponse })?
}

/// Démarre une lecture ; `fini` passe à vrai quand le son est terminé ou arrêté.
pub fn jouer(wav: Wav, fini: Arc<AtomicBool>) -> Result<u64, String> {
    demander(|reponse| Ordre::Jouer { wav, fini, reponse })?
}

/// Arrête une lecture (sans effet si elle est déjà finie).
pub fn arreter_lecture(id: u64) {
    let _ = fil().send(Ordre::ArreterLecture { id });
}

// ------------------------------------------------------------------ thread audio

struct Enregistrement {
    _flux: cpal::Stream,
    ecrivain: Arc<Mutex<Option<Ecrivain>>>,
}

struct Lecture {
    _flux: cpal::Stream,
    fini: Arc<AtomicBool>,
}

fn boucle(rx: mpsc::Receiver<Ordre>) {
    let mut enregistrements: HashMap<u64, Enregistrement> = HashMap::new();
    let mut lectures: HashMap<u64, Lecture> = HashMap::new();
    let mut suivant: u64 = 1;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ordre::Micros(r)) => {
                let _ = r.send(lister_micros());
            }
            Ok(Ordre::SortieParDefaut(r)) => {
                let _ = r.send(cpal::default_host().default_output_device().and_then(|d| d.name().ok()));
            }
            Ok(Ordre::Enregistrer { micro, fichier, niveau, reponse }) => {
                let ecrivain = Arc::new(Mutex::new(None));
                let r = ouvrir_micro(micro.as_deref(), &fichier, &ecrivain, niveau).map(|flux| {
                    let id = suivant;
                    suivant += 1;
                    enregistrements.insert(id, Enregistrement { _flux: flux, ecrivain });
                    id
                });
                let _ = reponse.send(r);
            }
            Ok(Ordre::ArreterEnregistrement { id, reponse }) => {
                let r = match enregistrements.remove(&id) {
                    Some(e) => {
                        let Enregistrement { _flux, ecrivain } = e;
                        drop(_flux); // plus aucun rappel audio après ceci
                        let w = ecrivain.lock().ok().and_then(|mut g| g.take());
                        match w {
                            Some(w) => w.finalize().map_err(|e| format!("Impossible de finaliser l'enregistrement : {e}")),
                            None => Ok(()),
                        }
                    }
                    None => Ok(()),
                };
                let _ = reponse.send(r);
            }
            Ok(Ordre::Jouer { wav, fini, reponse }) => {
                let r = ouvrir_sortie(wav, fini.clone()).map(|flux| {
                    let id = suivant;
                    suivant += 1;
                    lectures.insert(id, Lecture { _flux: flux, fini });
                    id
                });
                let _ = reponse.send(r);
            }
            Ok(Ordre::ArreterLecture { id }) => {
                if let Some(l) = lectures.remove(&id) {
                    l.fini.store(true, Ordering::Relaxed);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Libère les lectures terminées.
        lectures.retain(|_, l| !l.fini.load(Ordering::Relaxed));
    }
}

fn lister_micros() -> Vec<Micro> {
    let Ok(liste) = cpal::default_host().input_devices() else { return Vec::new() };
    let mut v: Vec<Micro> = liste
        .filter_map(|d| d.name().ok())
        .filter(|n| !n.trim().is_empty())
        .map(|n| Micro { id: n.clone(), nom: n })
        .collect();
    v.dedup_by(|a, b| a.id == b.id);
    v
}

fn trouver_micro(micro: Option<&str>) -> Result<cpal::Device, String> {
    let hote = cpal::default_host();
    if let Some(nom) = micro {
        if let Ok(mut liste) = hote.input_devices() {
            if let Some(d) = liste.find(|d| d.name().map(|n| n == nom).unwrap_or(false)) {
                return Ok(d);
            }
        }
    }
    hote.default_input_device().ok_or_else(|| {
        "Aucun micro n'est disponible. Branchez-en un, ou autorisez l'accès au micro dans Paramètres → Confidentialité → Microphone.".into()
    })
}

fn ouvrir_micro(
    micro: Option<&str>,
    chemin: &Path,
    ecrivain: &Arc<Mutex<Option<Ecrivain>>>,
    niveau: Arc<AtomicU32>,
) -> Result<cpal::Stream, String> {
    let device = trouver_micro(micro)?;
    let config = device.default_input_config().map_err(|e| format!("Le micro refuse l'enregistrement : {e}"))?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: config.sample_rate().0,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let w = hound::WavWriter::create(chemin, spec).map_err(|e| format!("Impossible de créer {} : {e}", chemin.display()))?;
    *ecrivain.lock().map_err(|_| "verrou empoisonné".to_string())? = Some(w);
    let canaux = config.channels() as usize;
    let format = config.sample_format();
    let cfg: cpal::StreamConfig = config.into();
    let e = ecrivain.clone();
    let flux = match format {
        cpal::SampleFormat::F32 => flux_micro::<f32>(&device, &cfg, canaux, e, niveau),
        cpal::SampleFormat::I16 => flux_micro::<i16>(&device, &cfg, canaux, e, niveau),
        cpal::SampleFormat::U16 => flux_micro::<u16>(&device, &cfg, canaux, e, niveau),
        cpal::SampleFormat::I32 => flux_micro::<i32>(&device, &cfg, canaux, e, niveau),
        autre => return Err(format!("Format de micro non géré : {autre:?}")),
    }
    .map_err(|e| format!("Impossible d'ouvrir le micro : {e}"))?;
    flux.play().map_err(|e| format!("Impossible de démarrer le micro : {e}"))?;
    Ok(flux)
}

fn flux_micro<T>(
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
            let mut w = garde.as_mut().and_then(|g| g.as_mut());
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

fn ouvrir_sortie(wav: Wav, fini: Arc<AtomicBool>) -> Result<cpal::Stream, String> {
    let device = cpal::default_host().default_output_device().ok_or("Aucune sortie son n'est disponible.")?;
    let config = device.default_output_config().map_err(|e| format!("La sortie son refuse la lecture : {e}"))?;
    let canaux = config.channels() as usize;
    let format = config.sample_format();
    let cfg: cpal::StreamConfig = config.into();
    let pas = wav.rate as f64 / cfg.sample_rate.0 as f64;
    let source = Source { samples: wav.samples, pos: 0.0, pas, fini };
    let flux = match format {
        cpal::SampleFormat::F32 => flux_sortie::<f32>(&device, &cfg, canaux, source),
        cpal::SampleFormat::I16 => flux_sortie::<i16>(&device, &cfg, canaux, source),
        cpal::SampleFormat::U16 => flux_sortie::<u16>(&device, &cfg, canaux, source),
        cpal::SampleFormat::I32 => flux_sortie::<i32>(&device, &cfg, canaux, source),
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

fn flux_sortie<T>(
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

#[cfg(test)]
mod tests {
    /// Appels répétés depuis des threads différents, qui se terminent : c'est ce qui plantait.
    #[test]
    fn appels_depuis_plusieurs_threads() {
        for _ in 0..3 {
            let t = std::thread::spawn(|| (super::micros().len(), super::sortie_par_defaut()));
            let _ = t.join().expect("pas de plantage");
        }
        let _ = super::micros();
        // Sans micro (machine de test), l'enregistrement échoue proprement.
        let f = std::env::temp_dir().join(format!("vocalise-test-wasapi-{}.wav", std::process::id()));
        if super::micros().is_empty() {
            assert!(super::enregistrer(None, &f, Default::default()).is_err());
        }
        let _ = std::fs::remove_file(f);
    }
}
