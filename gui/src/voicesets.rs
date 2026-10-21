//! Jeux de voix XTTS d'AllTalk : `voices/xtts_multi_voice_sets/<jeu>/*.wav`.
//! AllTalk prend jusqu'à 5 fichiers (au hasard au-delà) et 30 s par fichier.

use crate::audio::Wav;
use std::path::{Path, PathBuf};

/// Nombre de fichiers réellement exploités par AllTalk pour un jeu.
pub const MAX_UTILES: usize = 5;
/// Durée maximale exploitée par fichier (max_ref_len du modèle).
pub const DUREE_MAX_S: f32 = 30.0;

#[derive(Clone, Debug)]
pub struct Echantillon {
    pub chemin: PathBuf,
    /// Nom de fichier sans extension.
    pub nom: String,
    pub duree: f32,
}

/// Nom tel qu'attendu par l'API AllTalk.
pub fn nom_api(jeu: &str) -> String {
    format!("voiceset:{jeu}")
}

/// Jeu à partir d'un nom API ("voiceset:x" → Some("x")).
pub fn jeu_depuis_api(voix: &str) -> Option<&str> {
    voix.strip_prefix("voiceset:")
}

/// Vérifie un nom de jeu : 1 à 40 caractères parmi [A-Za-z0-9_-].
pub fn nom_valide(nom: &str) -> bool {
    let n = nom.chars().count();
    (1..=40).contains(&n) && nom.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn verifier_nom(nom: &str) -> Result<(), String> {
    if nom_valide(nom) {
        Ok(())
    } else {
        Err(format!(
            "Le nom « {nom} » n'est pas valide : utilisez de 1 à 40 lettres sans accent, chiffres, tirets ou soulignés."
        ))
    }
}

fn est_wav(p: &Path) -> bool {
    p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

/// Jeux existants (dossiers de `voicesets_dir`, même vides, au nom valide), triés.
pub fn lister_jeux(voicesets_dir: &Path) -> Vec<String> {
    let mut jeux: Vec<String> = std::fs::read_dir(voicesets_dir)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| nom_valide(n))
                .collect()
        })
        .unwrap_or_default();
    jeux.sort_by_key(|j| (j.to_lowercase(), j.clone()));
    jeux
}

/// Crée le dossier du jeu (erreur si nom invalide ; ok s'il existe déjà).
pub fn creer_jeu(voicesets_dir: &Path, nom: &str) -> Result<PathBuf, String> {
    verifier_nom(nom)?;
    let d = voicesets_dir.join(nom);
    std::fs::create_dir_all(&d)
        .map_err(|e| format!("Impossible de créer le jeu de voix « {nom} » ({}) : {e}", d.display()))?;
    Ok(d)
}

/// Échantillons (.wav) d'un jeu, triés par nom, avec leur durée.
pub fn echantillons(voicesets_dir: &Path, jeu: &str) -> Vec<Echantillon> {
    if !nom_valide(jeu) {
        return Vec::new();
    }
    let mut res: Vec<Echantillon> = std::fs::read_dir(voicesets_dir.join(jeu))
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| est_wav(p))
                .filter_map(|p| {
                    let nom = p.file_stem()?.to_str()?.to_string();
                    let duree = crate::audio::duree(&p).unwrap_or(0.0);
                    Some(Echantillon { chemin: p, nom, duree })
                })
                .collect()
        })
        .unwrap_or_default();
    res.sort_by_key(|e| (e.nom.to_lowercase(), e.nom.clone()));
    res
}

/// Premier chemin libre `<dossier>/<base>[-N].<ext>` (N = 2, 3, …).
fn chemin_libre(dossier: &Path, base: &str, ext: &str) -> PathBuf {
    let mut p = dossier.join(format!("{base}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dossier.join(format!("{base}-{n}.{ext}"));
        n += 1;
    }
    p
}

/// `<fichier>.part` : nom provisoire pendant l'écriture (jamais pris pour un .wav).
fn fichier_part(chemin: &Path) -> PathBuf {
    let mut nom = chemin.file_name().unwrap_or_default().to_os_string();
    nom.push(".part");
    chemin.with_file_name(nom)
}

/// Ajoute un échantillon au jeu : nettoyage (audio::nettoyer), troncature à DUREE_MAX_S
/// (audio::tronquer), écriture PCM 16 bits sous `<jeu>/<nom>.wav` (suffixe -2, -3… si le nom
/// existe). Crée le jeu si besoin. Retourne le chemin écrit.
/// Le nom est passé par `paths::nom_sur` (pas de caractère dangereux dans un nom de fichier).
pub fn ajouter_echantillon(voicesets_dir: &Path, jeu: &str, brut: &Wav, nom: &str) -> Result<PathBuf, String> {
    verifier_nom(jeu)?;
    let propre = crate::audio::nettoyer(brut)?;
    let court = crate::audio::tronquer(&propre, DUREE_MAX_S);
    let dossier = creer_jeu(voicesets_dir, jeu)?;
    let base = if nom.trim().is_empty() { "echantillon".to_string() } else { crate::paths::nom_sur(nom) };
    let chemin = chemin_libre(&dossier, &base, "wav");
    // AllTalk lit ce dossier pendant qu'on écrit : fichier « .part » puis renommage atomique.
    let part = fichier_part(&chemin);
    if let Err(e) = crate::audio::ecrire_i16(&part, &court) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &chemin).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Impossible d'enregistrer l'échantillon {} : {e}", chemin.display())
    })?;
    Ok(chemin)
}

/// Déplace un fichier (rename ; repli copie + vérification de taille + suppression de l'original
/// quand `/home` et la destination sont sur des systèmes de fichiers différents).
fn deplacer(src: &Path, dst: &Path) -> Result<(), String> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let copie = std::fs::copy(src, dst).map_err(|e| {
        format!("Impossible de déplacer {} vers {} : {e}", src.display(), dst.display())
    })?;
    let taille_src = std::fs::metadata(src).map(|m| m.len()).unwrap_or(copie);
    if copie != taille_src {
        let _ = std::fs::remove_file(dst);
        return Err(format!("La copie de {} est incomplète ; l'original est conservé.", src.display()));
    }
    std::fs::remove_file(src)
        .map_err(|e| format!("Copie réussie, mais l'original {} n'a pas pu être retiré : {e}", src.display()))
}

/// Retire un échantillon : déplacé dans `corbeille_dir` (préfixé par le jeu et l'horodatage),
/// jamais supprimé.
pub fn retirer_echantillon(chemin: &Path, corbeille_dir: &Path) -> Result<PathBuf, String> {
    if !chemin.is_file() {
        return Err(format!("L'échantillon {} n'existe plus.", chemin.display()));
    }
    std::fs::create_dir_all(corbeille_dir)
        .map_err(|e| format!("Impossible de créer la corbeille {} : {e}", corbeille_dir.display()))?;
    let jeu = chemin
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("jeu");
    let stem = chemin.file_stem().and_then(|s| s.to_str()).unwrap_or("echantillon");
    let ext = chemin.extension().and_then(|e| e.to_str()).unwrap_or("wav");
    let base = format!("{jeu}_{}_{stem}", crate::paths::horodatage());
    let dest = chemin_libre(corbeille_dir, &base, ext);
    deplacer(chemin, &dest)?;
    Ok(dest)
}

/// Réinitialise un jeu : déplace tous ses échantillons dans la corbeille, garde le dossier.
/// Retourne le nombre déplacé.
pub fn reinitialiser_jeu(voicesets_dir: &Path, jeu: &str, corbeille_dir: &Path) -> Result<usize, String> {
    verifier_nom(jeu)?;
    let mut n = 0;
    for e in echantillons(voicesets_dir, jeu) {
        retirer_echantillon(&e.chemin, corbeille_dir)?;
        n += 1;
    }
    Ok(n)
}

/// Supprime un jeu : réinitialise puis retire le dossier.
/// Les wav vont dans la corbeille ; si le dossier contient d'autres fichiers, il est conservé
/// et une erreur est renvoyée.
pub fn supprimer_jeu(voicesets_dir: &Path, jeu: &str, corbeille_dir: &Path) -> Result<(), String> {
    reinitialiser_jeu(voicesets_dir, jeu, corbeille_dir)?;
    let d = voicesets_dir.join(jeu);
    if !d.exists() {
        return Ok(());
    }
    std::fs::remove_dir(&d).map_err(|e| {
        format!("Les échantillons ont été mis à la corbeille, mais le dossier du jeu « {jeu} » n'a pas pu être retiré : {e}")
    })
}

/// Copie un échantillon comme voix simple `voices/<nom>.wav` (utilisable sans jeu de voix).
/// Ne remplace jamais une voix existante : suffixe -2, -3… si le nom est pris.
pub fn exporter_voix_simple(echantillon: &Path, voices_dir: &Path, nom: &str) -> Result<PathBuf, String> {
    if !echantillon.is_file() {
        return Err(format!("L'échantillon {} n'existe plus.", echantillon.display()));
    }
    std::fs::create_dir_all(voices_dir)
        .map_err(|e| format!("Impossible de créer le dossier {} : {e}", voices_dir.display()))?;
    let dest = chemin_libre(voices_dir, &crate::paths::nom_sur(nom), "wav");
    let part = fichier_part(&dest);
    let erreur = |e: std::io::Error| format!("Impossible de copier l'échantillon vers {} : {e}", dest.display());
    if let Err(e) = std::fs::copy(echantillon, &part) {
        let _ = std::fs::remove_file(&part);
        return Err(erreur(e));
    }
    std::fs::rename(&part, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        erreur(e)
    })?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::lire;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(e: &str) -> Tmp {
            let d = std::env::temp_dir().join(format!("vocalise-test-vs-{}-{e}", std::process::id()));
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

    fn voix(secondes: f32) -> Wav {
        let rate = 16000;
        let mut s = vec![0.0; 8000];
        s.extend((0..(secondes * rate as f32) as usize).map(|i| 0.3 * (i as f32 * 0.05).sin()));
        s.extend(vec![0.0; 8000]);
        Wav { rate, samples: s }
    }

    #[test]
    fn noms_valides() {
        assert!(nom_valide("emmanuel"));
        assert!(nom_valide("Jeu_2-b"));
        assert!(nom_valide(&"a".repeat(40)));
        assert!(!nom_valide(""));
        assert!(!nom_valide(&"a".repeat(41)));
        assert!(!nom_valide("é"));
        assert!(!nom_valide("a b"));
        assert!(!nom_valide("../x"));
        assert!(!nom_valide("a/b"));
        assert!(!nom_valide("voiceset:x"));
    }

    #[test]
    fn nom_api_et_retour() {
        assert_eq!(nom_api("x"), "voiceset:x");
        assert_eq!(jeu_depuis_api("voiceset:x"), Some("x"));
        assert_eq!(jeu_depuis_api("x.wav"), None);
    }

    #[test]
    fn creer_et_lister() {
        let t = Tmp::new("creer");
        let vs = t.0.join("sets");
        assert!(lister_jeux(&vs).is_empty());
        creer_jeu(&vs, "beta").unwrap();
        creer_jeu(&vs, "Alpha").unwrap();
        creer_jeu(&vs, "beta").unwrap(); // déjà là : ok
        assert!(creer_jeu(&vs, "mauvais nom").is_err());
        assert!(creer_jeu(&vs, "../evil").is_err());
        assert_eq!(lister_jeux(&vs), vec!["Alpha", "beta"]);
    }

    #[test]
    fn lister_ignore_les_noms_invalides() {
        let t = Tmp::new("liste-invalide");
        let vs = t.0.join("sets");
        for n in ["bon", "mauvais nom", "é", ".cache"] {
            std::fs::create_dir_all(vs.join(n)).unwrap();
        }
        std::fs::write(vs.join("fichier"), b"x").unwrap();
        assert_eq!(lister_jeux(&vs), vec!["bon"]);
    }

    #[test]
    fn ecriture_sans_reste_part() {
        let t = Tmp::new("part");
        let vs = t.0.join("sets");
        let p = ajouter_echantillon(&vs, "moi", &voix(1.0), "a").unwrap();
        exporter_voix_simple(&p, &t.0.join("voices"), "v").unwrap();
        for d in [vs.join("moi"), t.0.join("voices")] {
            assert!(std::fs::read_dir(d).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".part")));
        }
    }

    #[test]
    fn ajouter_avec_collision() {
        let t = Tmp::new("ajout");
        let vs = t.0.join("sets");
        let p1 = ajouter_echantillon(&vs, "moi", &voix(2.0), "prise").unwrap();
        let p2 = ajouter_echantillon(&vs, "moi", &voix(2.0), "prise").unwrap();
        let p3 = ajouter_echantillon(&vs, "moi", &voix(2.0), "prise").unwrap();
        assert_eq!(p1.file_name().unwrap(), "prise.wav");
        assert_eq!(p2.file_name().unwrap(), "prise-2.wav");
        assert_eq!(p3.file_name().unwrap(), "prise-3.wav");
        let e = echantillons(&vs, "moi");
        assert_eq!(e.iter().map(|x| x.nom.as_str()).collect::<Vec<_>>(), vec!["prise", "prise-2", "prise-3"]);
        // 2 s de voix + 0,3 s de marge + 0,3 s ajoutée de chaque côté = 3,2 s ; PCM 16 bits mono.
        assert!((e[0].duree - 3.2).abs() < 0.1, "durée {}", e[0].duree);
        let w = hound::WavReader::open(&p1).unwrap();
        assert_eq!(w.spec().bits_per_sample, 16);
        assert_eq!(w.spec().channels, 1);
        // Nom dangereux assaini ; nom vide accepté.
        let p = ajouter_echantillon(&vs, "moi", &voix(1.0), "../x y").unwrap();
        assert_eq!(p.parent().unwrap(), vs.join("moi"));
        assert!(ajouter_echantillon(&vs, "moi", &voix(1.0), "").unwrap().ends_with("echantillon.wav"));
        // Jeu invalide ou signal silencieux : erreurs.
        assert!(ajouter_echantillon(&vs, "../x", &voix(1.0), "a").is_err());
        assert!(ajouter_echantillon(&vs, "moi", &Wav { rate: 16000, samples: vec![0.0; 16000] }, "a").is_err());
    }

    #[test]
    fn ajouter_tronque_a_30_secondes() {
        let t = Tmp::new("tronque");
        let vs = t.0.join("sets");
        let p = ajouter_echantillon(&vs, "long", &voix(40.0), "long").unwrap();
        let d = lire(&p).unwrap().duree();
        assert!(d <= DUREE_MAX_S + 0.01 && d > 26.0, "durée {d}");
    }

    #[test]
    fn retirer_vers_corbeille() {
        let t = Tmp::new("retirer");
        let vs = t.0.join("sets");
        let corb = t.0.join("corbeille");
        let p = ajouter_echantillon(&vs, "moi", &voix(1.0), "a").unwrap();
        let taille = std::fs::metadata(&p).unwrap().len();
        let dest = retirer_echantillon(&p, &corb).unwrap();
        assert!(!p.exists());
        assert!(dest.starts_with(&corb));
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), taille);
        let nom = dest.file_name().unwrap().to_str().unwrap();
        assert!(nom.starts_with("moi_") && nom.ends_with("_a.wav"), "{nom}");
        assert!(echantillons(&vs, "moi").is_empty());
        // Retirer un fichier absent : erreur.
        assert!(retirer_echantillon(&p, &corb).is_err());
        // Deux retraits du même nom dans la même minute ne s'écrasent pas.
        let p1 = ajouter_echantillon(&vs, "moi", &voix(1.0), "a").unwrap();
        let d1 = retirer_echantillon(&p1, &corb).unwrap();
        assert_ne!(d1, dest);
        assert!(dest.exists() && d1.exists());
    }

    #[test]
    fn reinitialiser_et_supprimer() {
        let t = Tmp::new("reinit");
        let vs = t.0.join("sets");
        let corb = t.0.join("corbeille");
        for n in ["a", "b", "c"] {
            ajouter_echantillon(&vs, "moi", &voix(1.0), n).unwrap();
        }
        ajouter_echantillon(&vs, "autre", &voix(1.0), "z").unwrap();
        assert_eq!(reinitialiser_jeu(&vs, "moi", &corb).unwrap(), 3);
        assert!(vs.join("moi").is_dir());
        assert!(echantillons(&vs, "moi").is_empty());
        assert_eq!(std::fs::read_dir(&corb).unwrap().count(), 3);
        assert_eq!(echantillons(&vs, "autre").len(), 1); // autre jeu intact
        assert_eq!(reinitialiser_jeu(&vs, "inexistant", &corb).unwrap(), 0);
        assert!(reinitialiser_jeu(&vs, "../x", &corb).is_err());
        supprimer_jeu(&vs, "autre", &corb).unwrap();
        assert_eq!(lister_jeux(&vs), vec!["moi"]);
        assert_eq!(std::fs::read_dir(&corb).unwrap().count(), 4); // aucun wav perdu
    }

    #[test]
    fn exporter_en_voix_simple() {
        let t = Tmp::new("export");
        let vs = t.0.join("sets");
        let voices = t.0.join("voices");
        let p = ajouter_echantillon(&vs, "moi", &voix(1.0), "a").unwrap();
        let d1 = exporter_voix_simple(&p, &voices, "Ma voix").unwrap();
        let d2 = exporter_voix_simple(&p, &voices, "Ma voix").unwrap();
        assert_eq!(d1.file_name().unwrap(), "Ma_voix.wav");
        assert_eq!(d2.file_name().unwrap(), "Ma_voix-2.wav");
        assert!(p.exists());
        assert_eq!(std::fs::read(&p).unwrap(), std::fs::read(&d1).unwrap());
        // Un .wav illisible est listé avec une durée nulle.
        let faux = vs.join("moi").join("faux.wav");
        std::fs::write(&faux, b"pas un wav").unwrap();
        let e = echantillons(&vs, "moi");
        assert_eq!(e.len(), 2);
        assert_eq!(e.iter().find(|x| x.nom == "faux").unwrap().duree, 0.0);
    }
}
