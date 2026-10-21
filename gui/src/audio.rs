//! Lecture/écriture wav et traitements simples, sans dépendance externe (hound).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Signal mono en flottants [-1, 1].
#[derive(Clone, Debug, Default)]
pub struct Wav {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Wav {
    /// Durée en secondes (utilisée par les tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn duree(&self) -> f32 {
        if self.rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / self.rate as f32
        }
    }
}

/// Ramène des échantillons entrelacés (`canaux` canaux) en mono par moyenne.
fn vers_mono(entrelace: Vec<f32>, canaux: usize) -> Vec<f32> {
    if canaux <= 1 {
        return entrelace;
    }
    entrelace
        .chunks_exact(canaux)
        .map(|trame| trame.iter().sum::<f32>() / canaux as f32)
        .collect()
}

/// Lecture avec hound (chemin normal).
fn lire_hound(p: &Path) -> Result<Wav, String> {
    let mut lecteur = hound::WavReader::open(p).map_err(|e| e.to_string())?;
    let spec = lecteur.spec();
    let entrelace: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => lecteur
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?,
        (hound::SampleFormat::Int, bits @ (8 | 16 | 24 | 32)) => {
            let echelle = (1u64 << (bits - 1)) as f32;
            lecteur
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / echelle))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        }
        (f, b) => return Err(format!("format non géré ({f:?}, {b} bits)")),
    };
    Ok(Wav { rate: spec.sample_rate, samples: vers_mono(entrelace, spec.channels.max(1) as usize) })
}

/// Lecteur RIFF minimal de repli : tolère une taille de données fausse (fichier en cours
/// d'écriture, pw-record interrompu), des chunks inconnus, WAVE_FORMAT_EXTENSIBLE.
fn lire_riff(p: &Path) -> Result<Wav, String> {
    let octets = std::fs::read(p).map_err(|e| e.to_string())?;
    if octets.len() < 12 || &octets[0..4] != b"RIFF" || &octets[8..12] != b"WAVE" {
        return Err("ce n'est pas un fichier wav".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([octets[i], octets[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([octets[i], octets[i + 1], octets[i + 2], octets[i + 3]]);
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (code, canaux, taux, bits)
    let mut donnees: Option<&[u8]> = None;
    while pos + 8 <= octets.len() {
        let id = &octets[pos..pos + 4];
        let taille = u32_at(pos + 4) as usize;
        let debut = pos + 8;
        if id == b"fmt " && debut + 16 <= octets.len() {
            let mut code = u16_at(debut);
            let bits = u16_at(debut + 14);
            if code == 0xFFFE && taille >= 26 && debut + 26 <= octets.len() {
                code = u16_at(debut + 24); // sous-format de l'en-tête étendu
            }
            fmt = Some((code, u16_at(debut + 2), u32_at(debut + 4), bits));
        } else if id == b"data" {
            let fin = if taille == 0 || taille == 0xFFFF_FFFF || debut + taille > octets.len() {
                octets.len()
            } else {
                debut + taille
            };
            donnees = Some(&octets[debut..fin]);
            break;
        }
        pos = debut.saturating_add(taille).saturating_add(taille & 1);
    }
    let (code, canaux, rate, bits) = fmt.ok_or("en-tête wav absent")?;
    let brut = donnees.ok_or("données audio absentes")?;
    let entrelace: Vec<f32> = match (code, bits) {
        (3, 32) => brut.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect(),
        (1, 8) => brut.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        (1, 16) => brut.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect(),
        (1, 24) => brut
            .chunks_exact(3)
            .map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0)
            .collect(),
        (1, 32) => brut
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0)
            .collect(),
        _ => return Err(format!("format non géré (code {code}, {bits} bits)")),
    };
    Ok(Wav { rate, samples: vers_mono(entrelace, canaux.max(1) as usize) })
}

/// Lit n'importe quel wav (entier 8/16/24/32 bits ou flottant 32 bits, mono ou multi-canal,
/// chunks fact/PEAK/LIST ignorés) et le ramène en mono.
/// hound d'abord ; en cas d'échec, lecteur RIFF de repli (tailles d'en-tête fausses…).
pub fn lire(p: &Path) -> Result<Wav, String> {
    let w = match lire_hound(p) {
        // hound accepte un en-tête dont la taille de données est 0 (fichier interrompu) et
        // renvoie alors un signal vide : on laisse le lecteur de repli se servir de la taille réelle.
        Ok(w) if w.samples.is_empty() => match lire_riff(p) {
            Ok(r) if !r.samples.is_empty() => Ok(r),
            _ => Ok(w),
        },
        Ok(w) => Ok(w),
        Err(e_hound) => lire_riff(p).map_err(|e_riff| {
            if !p.exists() {
                format!("Fichier audio introuvable : {}", p.display())
            } else {
                format!("Impossible de lire le fichier audio {} ({e_hound} ; {e_riff})", p.display())
            }
        }),
    }?;
    if w.rate == 0 {
        return Err(format!("Fichier audio invalide (fréquence nulle) : {}", p.display()));
    }
    Ok(w)
}

/// Écrit un wav PCM 16 bits mono.
pub fn ecrire_i16(p: &Path, w: &Wav) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: w.rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let msg = |e: &dyn std::fmt::Display| format!("Impossible d'écrire le fichier audio {} : {e}", p.display());
    let mut ecrivain = hound::WavWriter::create(p, spec).map_err(|e| msg(&e))?;
    for &s in &w.samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        ecrivain.write_sample(v).map_err(|e| msg(&e))?;
    }
    ecrivain.finalize().map_err(|e| msg(&e))
}

/// Durée d'un wav sans le charger en entier.
pub fn duree(p: &Path) -> Option<f32> {
    let lecteur = hound::WavReader::open(p).ok()?;
    let spec = lecteur.spec();
    if spec.sample_rate == 0 {
        return None;
    }
    Some(lecteur.duration() as f32 / spec.sample_rate as f32)
}

/// Formats d'export audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Format {
    Wav,
    Flac,
    Mp3,
    Ogg,
}

impl Format {
    pub const TOUS: [Format; 4] = [Format::Wav, Format::Flac, Format::Mp3, Format::Ogg];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Wav => "wav",
            Format::Flac => "flac",
            Format::Mp3 => "mp3",
            Format::Ogg => "ogg",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Format::Wav => "WAV (sans perte)",
            Format::Flac => "FLAC (sans perte, compressé)",
            Format::Mp3 => "MP3",
            Format::Ogg => "OGG Opus",
        }
    }

    /// Insensible à la casse, avec ou sans point initial (utilisée par les tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn depuis_extension(ext: &str) -> Option<Format> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "wav" => Some(Format::Wav),
            "flac" => Some(Format::Flac),
            "mp3" => Some(Format::Mp3),
            "ogg" | "opus" => Some(Format::Ogg),
            _ => None,
        }
    }
}

impl Default for Format {
    fn default() -> Self {
        Format::Wav
    }
}

/// Vrai si la commande ffmpeg est disponible (`ffmpeg -version`).
pub fn ffmpeg_disponible() -> bool {
    use std::process::{Command, Stdio};
    Command::new("ffmpeg")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Convertit un wav vers `format` à l'emplacement `sortie` (l'extension est forcée d'après le
/// format). Wav → copie simple. Autres formats → ffmpeg. Retourne le chemin réellement écrit.
pub fn convertir(wav: &Path, sortie: &Path, format: Format) -> Result<PathBuf, String> {
    use std::process::{Command, Stdio};
    let sortie = sortie.with_extension(format.extension());
    if let Some(parent) = sortie.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Impossible de créer le dossier {} : {e}", parent.display()))?;
    }
    if !wav.exists() {
        return Err(format!("Fichier audio introuvable : {}", wav.display()));
    }
    let options: &[&str] = match format {
        Format::Wav => {
            if wav != sortie {
                std::fs::copy(wav, &sortie)
                    .map_err(|e| format!("Impossible de copier le fichier audio vers {} : {e}", sortie.display()))?;
            }
            return Ok(sortie);
        }
        Format::Flac => &["-c:a", "flac", "-compression_level", "8"],
        Format::Mp3 => &["-c:a", "libmp3lame", "-q:a", "2"],
        Format::Ogg => &["-c:a", "libopus", "-b:a", "96k"],
    };
    if wav == sortie {
        return Err("Le fichier de sortie ne peut pas être le fichier source.".into());
    }
    let res = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(wav)
        .args(options)
        .arg(&sortie)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "ffmpeg n'est pas installé : impossible d'exporter en {}. Choisissez le format WAV ou installez ffmpeg.",
                    format.libelle()
                )
            } else {
                format!("Impossible de lancer ffmpeg : {e}")
            }
        })?;
    if !res.status.success() {
        let _ = std::fs::remove_file(&sortie);
        let err = String::from_utf8_lossy(&res.stderr);
        return Err(format!(
            "La conversion en {} a échoué (ffmpeg) : {}",
            format.libelle(),
            err.trim()
        ));
    }
    Ok(sortie)
}

/// Nettoie un enregistrement : coupe les silences aux deux extrémités (seuil 1 % de l'échelle,
/// en gardant 0,3 s avant/après), normalise le pic à -3 dBFS, puis ajoute 0,3 s de silence
/// au début et à la fin. Erreur si le signal est entièrement silencieux.
pub fn nettoyer(w: &Wav) -> Result<Wav, String> {
    const SEUIL: f32 = 0.01;
    let marge = (0.3 * w.rate as f32).round() as usize;
    let premier = w.samples.iter().position(|s| s.abs() > SEUIL);
    let dernier = w.samples.iter().rposition(|s| s.abs() > SEUIL);
    let (premier, dernier) = match (premier, dernier) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err("L'enregistrement est silencieux : aucun son n'a été détecté. Vérifiez le micro.".into()),
    };
    let debut = premier.saturating_sub(marge);
    let fin = (dernier + 1 + marge).min(w.samples.len());
    let tranche = &w.samples[debut..fin];
    let pic = tranche.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let gain = 10f32.powf(-3.0 / 20.0) / pic; // pic > SEUIL > 0
    let mut samples = Vec::with_capacity(tranche.len() + 2 * marge);
    samples.resize(marge, 0.0);
    samples.extend(tranche.iter().map(|s| s * gain));
    samples.resize(samples.len() + marge, 0.0);
    Ok(Wav { rate: w.rate, samples })
}

/// Tronque à `secondes` maximum en coupant de préférence sur un silence proche de la limite
/// (cherche dans les 3 dernières secondes la fenêtre de 50 ms la plus calme), avec un fondu
/// de sortie de 30 ms. Ne fait rien si le signal est plus court.
///
/// Algorithme : fenêtres de 50 ms décalées de 10 ms dans [limite-3 s, limite] ; on retient celle
/// de plus faible énergie (à égalité, la plus tardive) et on coupe en son milieu ; le fondu de
/// 30 ms (linéaire) s'applique aux derniers échantillons conservés.
pub fn tronquer(w: &Wav, secondes: f32) -> Wav {
    let limite = (secondes.max(0.0) * w.rate as f32) as usize;
    if w.samples.len() <= limite {
        return w.clone();
    }
    let rate = w.rate as f32;
    let fenetre = ((0.05 * rate) as usize).max(1);
    let pas = ((0.01 * rate) as usize).max(1);
    let debut_zone = limite.saturating_sub((3.0 * rate) as usize);
    let mut coupe = limite;
    if limite >= fenetre {
        // Énergies cumulées pour calculer chaque fenêtre en O(1).
        let mut cumul = Vec::with_capacity(limite + 1);
        cumul.push(0.0f64);
        let mut acc = 0.0f64;
        for &s in &w.samples[..limite] {
            acc += (s as f64) * (s as f64);
            cumul.push(acc);
        }
        let mut meilleur = f64::INFINITY;
        let mut a = debut_zone;
        while a + fenetre <= limite {
            let e = cumul[a + fenetre] - cumul[a];
            if e <= meilleur {
                meilleur = e;
                coupe = a + fenetre / 2;
            }
            a += pas;
        }
    }
    let mut samples = w.samples[..coupe.min(w.samples.len())].to_vec();
    let fondu = ((0.03 * rate) as usize).min(samples.len());
    let n = samples.len();
    for i in 0..fondu {
        samples[n - fondu + i] *= 1.0 - (i as f32 + 1.0) / fondu as f32;
    }
    Wav { rate: w.rate, samples }
}

/// Rééchantillonnage linéaire.
fn reechantillonner(samples: &[f32], de: u32, vers: u32) -> Vec<f32> {
    if de == vers || samples.is_empty() {
        return samples.to_vec();
    }
    let n_sortie = ((samples.len() as u64 * vers as u64) / de as u64).max(1) as usize;
    let ratio = de as f64 / vers as f64;
    (0..n_sortie)
        .map(|i| {
            let pos = i as f64 * ratio;
            let i0 = (pos.floor() as usize).min(samples.len() - 1);
            let i1 = (i0 + 1).min(samples.len() - 1);
            let frac = (pos - i0 as f64) as f32;
            samples[i0] * (1.0 - frac) + samples[i1] * frac
        })
        .collect()
}

/// Concatène des wav (formats quelconques) en un seul signal, avec `pause_s` de silence entre
/// deux morceaux. Le taux d'échantillonnage est celui du premier fichier ; un fichier d'un autre
/// taux est rééchantillonné linéairement.
pub fn concatener(fichiers: &[PathBuf], pause_s: f32) -> Result<Wav, String> {
    if fichiers.is_empty() {
        return Err("Aucun fichier audio à assembler.".into());
    }
    let mut sortie = Wav::default();
    for (i, f) in fichiers.iter().enumerate() {
        let w = lire(f)?;
        if i == 0 {
            sortie.rate = w.rate;
            sortie.samples = w.samples;
        } else {
            let pause = (pause_s.max(0.0) * sortie.rate as f32).round() as usize;
            sortie.samples.resize(sortie.samples.len() + pause, 0.0);
            sortie.samples.extend(reechantillonner(&w.samples, w.rate, sortie.rate));
        }
    }
    Ok(sortie)
}

/// Description du format lu dans les premiers octets d'un wav (éventuellement inachevé).
struct EnteteBrut {
    /// 1 = entier, 3 = flottant.
    code: u16,
    canaux: usize,
    rate: u32,
    bits: u16,
    /// Offset du premier octet de données.
    data: u64,
}

/// Analyse l'en-tête RIFF dans `tete` (les premiers Ko du fichier) en parcourant les chunks
/// depuis l'octet 12 : `fmt ` de 16, 18 ou 40 octets (WAVE_FORMAT_EXTENSIBLE), chunks `LIST`, etc.
/// Repli : en-tête canonique de 44 octets, PCM 16 bits mono 22050 Hz (réglages de pw-record).
fn analyser_entete(tete: &[u8]) -> EnteteBrut {
    let defaut = EnteteBrut { code: 1, canaux: 1, rate: 22_050, bits: 16, data: 44 };
    if tete.len() < 12 || &tete[0..4] != b"RIFF" || &tete[8..12] != b"WAVE" {
        return defaut;
    }
    let u16_at = |i: usize| u16::from_le_bytes([tete[i], tete[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([tete[i], tete[i + 1], tete[i + 2], tete[i + 3]]);
    let mut fmt: Option<(u16, usize, u32, u16)> = None;
    let mut pos = 12usize;
    while pos + 8 <= tete.len() {
        let id = &tete[pos..pos + 4];
        let taille = u32_at(pos + 4) as usize;
        let debut = pos + 8;
        if id == b"fmt " && taille >= 16 && debut + 16 <= tete.len() {
            let mut code = u16_at(debut);
            if code == 0xFFFE && taille >= 26 && debut + 26 <= tete.len() {
                code = u16_at(debut + 24); // sous-format de WAVE_FORMAT_EXTENSIBLE
            }
            fmt = Some((code, u16_at(debut + 2).max(1) as usize, u32_at(debut + 4), u16_at(debut + 14)));
        } else if id == b"data" {
            return match fmt {
                Some((code, canaux, rate, bits)) => EnteteBrut { code, canaux, rate, bits, data: debut as u64 },
                None => EnteteBrut { data: debut as u64, ..defaut },
            };
        }
        pos = debut.saturating_add(taille).saturating_add(taille & 1);
    }
    // Chunk `data` non trouvé dans les premiers Ko : en-tête canonique supposé.
    match fmt {
        Some((code, canaux, rate, bits)) => EnteteBrut { code, canaux, rate, bits, data: 44 },
        None => defaut,
    }
}

/// Niveau instantané d'un wav **en cours d'écriture** par pw-record (PCM 16 bits mono par défaut) :
/// RMS (0..1) des `fenetre_s` dernières secondes d'après la taille actuelle du fichier,
/// en ignorant l'en-tête (tailles déclarées non fiables tant que le fichier est ouvert).
/// L'en-tête est analysé (chunks RIFF, 4 Ko lus au plus) ; formats gérés : entier 16, 24 ou
/// 32 bits et flottant 32 bits, multi-canal compris. None si le fichier n'existe pas encore,
/// si le format n'est pas géré, ou s'il y a moins de la moitié de la fenêtre demandée.
pub fn niveau_direct(p: &Path, fenetre_s: f32) -> Option<f32> {
    let mut f = std::fs::File::open(p).ok()?;
    let taille = f.metadata().ok()?.len();
    let mut tete = Vec::with_capacity(4096);
    (&mut f).take(4096).read_to_end(&mut tete).ok()?;
    let h = analyser_entete(&tete);
    let octets = match (h.code, h.bits) {
        (1, 16) => 2usize,
        (1, 24) => 3,
        (1, 32) | (3, 32) => 4,
        _ => return None,
    };
    let rate = if (1000..=384_000).contains(&h.rate) { h.rate } else { 22_050 };
    let trame = octets * h.canaux;
    if taille <= h.data {
        return None;
    }
    let dispo = (taille - h.data) / trame as u64; // trames complètes réellement présentes
    let voulu = ((fenetre_s.max(0.0) * rate as f32) as u64).max(1);
    if dispo * 2 < voulu {
        return None;
    }
    let n = voulu.min(dispo);
    f.seek(SeekFrom::Start(h.data + (dispo - n) * trame as u64)).ok()?;
    let mut brut = vec![0u8; n as usize * trame];
    f.read_exact(&mut brut).ok()?;
    let mut somme = 0.0f64;
    let mut compte = 0usize;
    for b in brut.chunks_exact(octets) {
        let v = match (h.code, octets) {
            (1, 2) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            (1, 3) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (1, _) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        };
        somme += (v as f64) * (v as f64);
        compte += 1;
    }
    if compte == 0 {
        return None;
    }
    Some((somme / compte as f64).sqrt() as f32)
}

/// RMS en dBFS (-inf → -90) d'un signal (utilisée par les tests).
#[cfg_attr(not(test), allow(dead_code))]
pub fn rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -90.0;
    }
    let rms = (samples.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / samples.len() as f64).sqrt();
    if rms <= 0.0 {
        return -90.0;
    }
    ((20.0 * rms.log10()) as f32).max(-90.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COMPTEUR: AtomicUsize = AtomicUsize::new(0);

    /// Dossier temporaire unique, supprimé à la fin du test.
    pub(crate) struct Tmp(pub PathBuf);
    impl Tmp {
        pub fn new(etiquette: &str) -> Tmp {
            let n = COMPTEUR.fetch_add(1, Ordering::SeqCst);
            let d = std::env::temp_dir().join(format!("vocalise-test-{}-{etiquette}-{n}", std::process::id()));
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

    fn sinus(rate: u32, secondes: f32, freq: f32, amp: f32) -> Vec<f32> {
        (0..(secondes * rate as f32) as usize)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn silence(rate: u32, secondes: f32) -> Vec<f32> {
        vec![0.0; (secondes * rate as f32) as usize]
    }

    #[test]
    fn nettoyer_coupe_et_normalise() {
        let rate = 16000;
        let mut s = silence(rate, 2.0);
        s.extend(sinus(rate, 1.0, 440.0, 0.2));
        s.extend(silence(rate, 2.0));
        let w = nettoyer(&Wav { rate, samples: s }).unwrap();
        // 1 s de son + 0,3 s de marge gardée + 0,3 s ajoutée, de chaque côté.
        let attendu = 1.0 + 4.0 * 0.3;
        assert!((w.duree() - attendu).abs() < 0.05, "durée {}", w.duree());
        let pic = w.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let cible = 10f32.powf(-3.0 / 20.0);
        assert!((pic - cible).abs() < 0.005, "pic {pic}");
        // Début et fin silencieux.
        assert!(w.samples[..(0.5 * rate as f32) as usize].iter().all(|s| s.abs() < 1e-6));
        assert!(w.samples[w.samples.len() - (0.5 * rate as f32) as usize..].iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn nettoyer_signal_silencieux_est_une_erreur() {
        let w = Wav { rate: 16000, samples: silence(16000, 1.0) };
        assert!(nettoyer(&w).is_err());
        let bruit = Wav { rate: 16000, samples: vec![0.005; 16000] };
        assert!(nettoyer(&bruit).is_err());
        assert!(nettoyer(&Wav { rate: 16000, samples: vec![] }).is_err());
    }

    #[test]
    fn tronquer_ne_touche_pas_un_signal_court() {
        let w = Wav { rate: 8000, samples: sinus(8000, 2.0, 300.0, 0.5) };
        let t = tronquer(&w, 5.0);
        assert_eq!(t.samples.len(), w.samples.len());
    }

    #[test]
    fn tronquer_coupe_sur_le_silence() {
        let rate = 8000;
        // 12 s de son, avec un silence entre 9,0 s et 9,5 s ; limite à 10 s.
        let mut s = sinus(rate, 12.0, 300.0, 0.5);
        for v in &mut s[(9.0 * rate as f32) as usize..(9.5 * rate as f32) as usize] {
            *v = 0.0;
        }
        let t = tronquer(&Wav { rate, samples: s }, 10.0);
        assert!(t.duree() <= 10.0);
        // Coupe dans le silence : entre 9,0 et 9,5 s.
        assert!(t.duree() >= 9.0 && t.duree() <= 9.5, "coupe à {}", t.duree());
        // Fondu : dernier échantillon quasi nul.
        assert!(t.samples.last().unwrap().abs() < 0.01);
    }

    #[test]
    fn tronquer_sans_silence_respecte_la_limite_et_fait_un_fondu() {
        let rate = 8000;
        let w = Wav { rate, samples: sinus(rate, 20.0, 300.0, 0.5) };
        let t = tronquer(&w, 10.0);
        assert!(t.duree() <= 10.0 && t.duree() >= 7.0, "durée {}", t.duree());
        assert!(t.samples.last().unwrap().abs() < 0.01);
    }

    #[test]
    fn ecriture_lecture_aller_retour() {
        let tmp = Tmp::new("aller-retour");
        let p = tmp.0.join("a.wav");
        let w = Wav { rate: 22050, samples: sinus(22050, 0.5, 200.0, 0.5) };
        ecrire_i16(&p, &w).unwrap();
        let r = lire(&p).unwrap();
        assert_eq!(r.rate, 22050);
        assert_eq!(r.samples.len(), w.samples.len());
        assert!(r.samples.iter().zip(&w.samples).all(|(a, b)| (a - b).abs() < 1e-3));
        assert!((duree(&p).unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn lecture_stereo_float_et_formats_entiers() {
        let tmp = Tmp::new("formats");
        // Stéréo float 32 : moyenne des canaux.
        let p = tmp.0.join("st.wav");
        let spec = hound::WavSpec { channels: 2, sample_rate: 24000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut e = hound::WavWriter::create(&p, spec).unwrap();
        for _ in 0..100 {
            e.write_sample(0.5f32).unwrap();
            e.write_sample(-0.1f32).unwrap();
        }
        e.finalize().unwrap();
        let w = lire(&p).unwrap();
        assert_eq!(w.rate, 24000);
        assert_eq!(w.samples.len(), 100);
        assert!((w.samples[0] - 0.2).abs() < 1e-6);
        // Entiers 24 bits.
        let p = tmp.0.join("i24.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
        let mut e = hound::WavWriter::create(&p, spec).unwrap();
        e.write_sample(4_194_304i32).unwrap(); // 0,5
        e.finalize().unwrap();
        assert!((lire(&p).unwrap().samples[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn lecteur_de_repli_tolere_les_tailles_fausses() {
        let tmp = Tmp::new("repli");
        let p = tmp.0.join("pw.wav");
        let w = Wav { rate: 22050, samples: sinus(22050, 0.2, 200.0, 0.5) };
        ecrire_i16(&p, &w).unwrap();
        // Simule pw-record en cours : tailles déclarées à 0.
        let mut b = std::fs::read(&p).unwrap();
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        b[40..44].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(&p, &b).unwrap();
        let r = lire(&p).unwrap();
        assert_eq!(r.samples.len(), w.samples.len());
    }

    #[test]
    fn concatener_avec_pause_et_reechantillonnage() {
        let tmp = Tmp::new("concat");
        let a = tmp.0.join("a.wav");
        let b = tmp.0.join("b.wav");
        ecrire_i16(&a, &Wav { rate: 24000, samples: sinus(24000, 1.0, 200.0, 0.5) }).unwrap();
        ecrire_i16(&b, &Wav { rate: 12000, samples: sinus(12000, 1.0, 200.0, 0.5) }).unwrap();
        let w = concatener(&[a.clone(), b.clone()], 0.25).unwrap();
        assert_eq!(w.rate, 24000);
        assert!((w.duree() - 2.25).abs() < 0.01, "durée {}", w.duree());
        // La pause est du silence.
        let i = 24000 + 3000;
        assert!(w.samples[24000..i].iter().all(|s| *s == 0.0));
        // Un seul fichier : pas de pause.
        assert!((concatener(&[a], 0.25).unwrap().duree() - 1.0).abs() < 0.001);
        assert!(concatener(&[], 0.25).is_err());
        assert!(concatener(&[tmp.0.join("absent.wav")], 0.0).is_err());
    }

    #[test]
    fn niveau_direct_fichier_en_cours_d_ecriture() {
        let tmp = Tmp::new("niveau");
        let p = tmp.0.join("live.wav");
        assert!(niveau_direct(&p, 0.1).is_none());
        // En-tête à tailles nulles + 0,5 s d'un carré à 0,5 de l'échelle.
        let w = Wav { rate: 22050, samples: vec![0.5; 11025] };
        ecrire_i16(&p, &w).unwrap();
        let mut b = std::fs::read(&p).unwrap();
        b[4..8].copy_from_slice(&0u32.to_le_bytes());
        b[40..44].copy_from_slice(&0u32.to_le_bytes());
        std::fs::write(&p, &b).unwrap();
        let n = niveau_direct(&p, 0.1).unwrap();
        assert!((n - 0.5).abs() < 0.01, "niveau {n}");
        // Seulement l'en-tête : pas assez de données.
        std::fs::write(&p, &b[..44]).unwrap();
        assert!(niveau_direct(&p, 0.1).is_none());
    }

    #[test]
    fn format_extensions() {
        assert_eq!(Format::depuis_extension("FLAC"), Some(Format::Flac));
        assert_eq!(Format::depuis_extension(".mp3"), Some(Format::Mp3));
        assert_eq!(Format::depuis_extension("opus"), Some(Format::Ogg));
        assert_eq!(Format::depuis_extension("txt"), None);
        for f in Format::TOUS {
            assert_eq!(Format::depuis_extension(f.extension()), Some(f));
            assert!(!f.libelle().is_empty());
        }
        assert_eq!(Format::default(), Format::Wav);
    }

    #[test]
    fn convertir_wav_copie_et_force_l_extension() {
        let tmp = Tmp::new("conv-wav");
        let src = tmp.0.join("a.wav");
        ecrire_i16(&src, &Wav { rate: 22050, samples: sinus(22050, 0.3, 200.0, 0.5) }).unwrap();
        let r = convertir(&src, &tmp.0.join("sous/dossier/b.mp3"), Format::Wav).unwrap();
        assert_eq!(r, tmp.0.join("sous/dossier/b.wav"));
        assert_eq!(std::fs::read(&r).unwrap(), std::fs::read(&src).unwrap());
        assert!(convertir(&tmp.0.join("absent.wav"), &tmp.0.join("x.wav"), Format::Wav).is_err());
    }

    #[test]
    fn convertir_flac_reel() {
        if !ffmpeg_disponible() {
            return;
        }
        let tmp = Tmp::new("conv-flac");
        let src = tmp.0.join("a.wav");
        ecrire_i16(&src, &Wav { rate: 22050, samples: sinus(22050, 0.5, 200.0, 0.5) }).unwrap();
        for f in [Format::Flac, Format::Mp3, Format::Ogg] {
            let r = convertir(&src, &tmp.0.join("sortie"), f).unwrap();
            assert_eq!(r.extension().and_then(|e| e.to_str()), Some(f.extension()));
            assert!(std::fs::metadata(&r).unwrap().len() > 100);
        }
        // FLAC sans perte : relu par ffmpeg vers wav, mêmes échantillons.
        let flac = tmp.0.join("sortie.flac");
        let retour = tmp.0.join("retour.wav");
        let ok = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(&flac)
            .arg(&retour)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let (a, b) = (lire(&src).unwrap(), lire(&retour).unwrap());
        assert_eq!(a.samples.len(), b.samples.len());
        assert!(a.samples.iter().zip(&b.samples).all(|(x, y)| (x - y).abs() < 1e-4));
        // Source absente de ffmpeg → erreur lisible avec le fichier supprimé.
        assert!(convertir(&tmp.0.join("nul.wav"), &tmp.0.join("z"), Format::Flac).is_err());
    }

    /// Construit un wav « en cours d'écriture » : tailles déclarées à 0, `fmt` et chunks au choix.
    fn wav_brut(chunks_avant_data: &[(&[u8; 4], Vec<u8>)], donnees: &[u8]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
        for (id, contenu) in chunks_avant_data {
            b.extend_from_slice(*id);
            b.extend_from_slice(&(contenu.len() as u32).to_le_bytes());
            b.extend_from_slice(contenu);
            if contenu.len() % 2 == 1 {
                b.push(0);
            }
        }
        b.extend_from_slice(b"data\0\0\0\0");
        b.extend_from_slice(donnees);
        b
    }

    fn fmt_base(code: u16, canaux: u16, rate: u32, bits: u16) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&code.to_le_bytes());
        f.extend_from_slice(&canaux.to_le_bytes());
        f.extend_from_slice(&rate.to_le_bytes());
        f.extend_from_slice(&(rate * (bits as u32 / 8) * canaux as u32).to_le_bytes());
        f.extend_from_slice(&(canaux * bits / 8).to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f
    }

    fn carre_i16(n: usize, v: f32) -> Vec<u8> {
        (0..n).flat_map(|_| ((v * 32767.0) as i16).to_le_bytes()).collect()
    }

    #[test]
    fn niveau_direct_entete_extensible_et_list() {
        let tmp = Tmp::new("niveau-ext");
        let p = tmp.0.join("ext.wav");
        // fmt de 40 octets (WAVE_FORMAT_EXTENSIBLE, sous-format PCM), 16 bits, 8000 Hz, mono.
        let mut fmt = fmt_base(0xFFFE, 1, 8000, 16);
        fmt.extend_from_slice(&22u16.to_le_bytes()); // taille de l'extension
        fmt.extend_from_slice(&16u16.to_le_bytes()); // bits valides
        fmt.extend_from_slice(&4u32.to_le_bytes()); // masque de canaux
        fmt.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]); // GUID PCM
        assert_eq!(fmt.len(), 40);
        std::fs::write(&p, wav_brut(&[(b"fmt ", fmt)], &carre_i16(8000, 0.25))).unwrap();
        let n = niveau_direct(&p, 0.5).unwrap();
        assert!((n - 0.25).abs() < 0.01, "niveau {n}");

        // Chunk LIST (taille impaire, donc avec octet de bourrage) entre fmt et data.
        let p2 = tmp.0.join("list.wav");
        let liste = b"INFOISFT\x05\0\0\0Lavf\0".to_vec();
        std::fs::write(&p2, wav_brut(&[(b"fmt ", fmt_base(1, 1, 8000, 16)), (b"LIST", liste)], &carre_i16(8000, 0.5))).unwrap();
        let n = niveau_direct(&p2, 0.5).unwrap();
        assert!((n - 0.5).abs() < 0.01, "niveau {n}");
        // Avec un en-tête de 44 octets supposé, on lirait l'en-tête comme du son : ici non.
        std::fs::write(&p2, wav_brut(&[(b"fmt ", fmt_base(1, 1, 8000, 16)), (b"LIST", b"INFO".to_vec())], &vec![0u8; 16000])).unwrap();
        assert!(niveau_direct(&p2, 0.5).unwrap() < 1e-6);
    }

    #[test]
    fn niveau_direct_autres_formats() {
        let tmp = Tmp::new("niveau-fmt");
        // Flottant 32 bits stéréo : 0,5 sur les deux canaux.
        let p = tmp.0.join("f.wav");
        let donnees: Vec<u8> = (0..4000).flat_map(|_| 0.5f32.to_le_bytes()).collect();
        std::fs::write(&p, wav_brut(&[(b"fmt ", fmt_base(3, 2, 8000, 32))], &donnees)).unwrap();
        assert!((niveau_direct(&p, 0.25).unwrap() - 0.5).abs() < 1e-4);
        // Entier 24 bits.
        let p = tmp.0.join("i24.wav");
        let donnees: Vec<u8> = (0..4000).flat_map(|_| [0x00, 0x00, 0x40]).collect(); // 0,5
        std::fs::write(&p, wav_brut(&[(b"fmt ", fmt_base(1, 1, 8000, 24))], &donnees)).unwrap();
        assert!((niveau_direct(&p, 0.25).unwrap() - 0.5).abs() < 1e-4);
        // Format non géré (8 bits) : None.
        let p = tmp.0.join("i8.wav");
        std::fs::write(&p, wav_brut(&[(b"fmt ", fmt_base(1, 1, 8000, 8))], &[128u8; 4000])).unwrap();
        assert!(niveau_direct(&p, 0.25).is_none());
        // Pas d'en-tête reconnaissable : repli sur 44 octets, s16 mono.
        let p = tmp.0.join("nu.wav");
        let mut b = vec![0u8; 44];
        b.extend(carre_i16(22050, 0.5));
        std::fs::write(&p, b).unwrap();
        assert!((niveau_direct(&p, 0.1).unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn rms_dbfs_valeurs() {
        assert_eq!(rms_dbfs(&[]), -90.0);
        assert_eq!(rms_dbfs(&[0.0; 10]), -90.0);
        assert!((rms_dbfs(&[1.0; 10]) - 0.0).abs() < 1e-4);
        assert!((rms_dbfs(&[0.5; 10]) + 6.02).abs() < 0.01);
    }

    #[test]
    fn lecture_d_un_vrai_fichier_alltalk() {
        let p = Path::new("/home/ewenner/Vocalise/tests/test-emmanuel.wav");
        if !p.exists() {
            return; // fichier de référence absent de cette machine
        }
        // hound doit lire directement ce wav float 32 bits avec chunks fact/PEAK.
        let w = lire_hound(p).expect("hound doit lire un wav AllTalk");
        assert_eq!(w.rate, 24000);
        assert!(w.samples.len() > 24000);
        assert!(w.samples.iter().any(|s| s.abs() > 0.01));
        assert!(w.samples.iter().all(|s| s.abs() <= 1.5));
        assert!(duree(p).unwrap() > 1.0);
        // Le lecteur de repli donne le même résultat.
        let r = lire_riff(p).unwrap();
        assert_eq!(r.samples.len(), w.samples.len());
    }
}
