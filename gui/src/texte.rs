//! Découpage du texte à synthétiser et textes modèles à lire.

use std::path::Path;

/// Marge sous la limite `api_max_characters` (2000) d'AllTalk.
pub const MAX_CHARS: usize = 1800;

/// Textes modèles embarqués (titre, corps). Environ une minute de lecture chacun.
pub const EMBARQUES: &[(&str, &str)] = &[
    ("1. Cours : l'ébullition", include_str!("../textes/01-cours-ebullition.md")),
    ("2. Récit expressif", include_str!("../textes/02-recit-expressif.md")),
    ("3. Consignes et nombres", include_str!("../textes/03-consignes-nombres.md")),
    ("4. Questions et dialogue", include_str!("../textes/04-questions-dialogue.md")),
    ("5. Explication technique", include_str!("../textes/05-explication-technique.md")),
];

/// Abréviations (minuscules, sans le point) après lesquelles un point ne termine pas la phrase.
const ABREVIATIONS: &[&str] = &[
    "m", "mm", "mme", "mmes", "mlle", "mlles", "dr", "pr", "me", "mr", "st", "ste", "cf", "fig", "no", "vs",
    "env", "approx", "ex", "p", "pp", "av", "bd", "min", "max", "sec", "vol", "éd", "ed", "mgr", "cie",
];

fn est_terminateur(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

fn est_fermant(c: char) -> bool {
    matches!(c, '»' | '"' | ')' | ']' | '\'' | '”' | '’')
}

/// Le mot qui précède l'indice `fin` (exclu) est-il une abréviation ou une initiale ?
fn precede_abreviation(chars: &[char], fin: usize) -> bool {
    let mut debut = fin;
    while debut > 0 && !chars[debut - 1].is_whitespace() {
        debut -= 1;
    }
    let mot: String = chars[debut..fin]
        .iter()
        .collect::<String>()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    // Initiale isolée (« J. Dupont », « M. ») ou abréviation connue.
    (mot.chars().count() == 1 && mot.chars().all(|c| c.is_alphabetic())) || ABREVIATIONS.contains(&mot.as_str())
}

/// Découpe un texte (blancs déjà normalisés) en phrases.
fn phrases(texte: &str) -> Vec<String> {
    let chars: Vec<char> = texte.chars().collect();
    let mut res = Vec::new();
    let mut debut = 0;
    let mut i = 0;
    while i < chars.len() {
        if est_terminateur(chars[i]) {
            // Suite de terminateurs (« ?! », « ... »), puis fermants éventuels.
            let mut j = i;
            while j + 1 < chars.len() && est_terminateur(chars[j + 1]) {
                j += 1;
            }
            let fin_terms = j;
            loop {
                if j + 1 < chars.len() && est_fermant(chars[j + 1]) {
                    j += 1;
                } else if j + 2 < chars.len() && chars[j + 1] == ' ' && chars[j + 2] == '»' {
                    j += 2; // « Vraiment ? » : espace insécable ou non avant le guillemet fermant
                } else {
                    break;
                }
            }
            let fin_texte = j + 1 >= chars.len();
            let suivi_de_blanc = !fin_texte && chars[j + 1].is_whitespace();
            let coupe_ok = (fin_texte || suivi_de_blanc)
                && !(chars[fin_terms] == '.' && fin_terms == i && j == i && precede_abreviation(&chars, i));
            if coupe_ok {
                let p: String = chars[debut..=j].iter().collect();
                let p = p.trim();
                if !p.is_empty() {
                    res.push(p.to_string());
                }
                debut = j + 1;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    let reste: String = chars[debut.min(chars.len())..].iter().collect();
    let reste = reste.trim();
    if !reste.is_empty() {
        res.push(reste.to_string());
    }
    res
}

/// Découpe une phrase trop longue en morceaux de moins de MAX_CHARS caractères : de préférence
/// après une virgule, un point-virgule ou deux-points, sinon sur un espace, sinon brutalement.
fn couper_long(phrase: &str) -> Vec<String> {
    let mut chars: Vec<char> = phrase.chars().collect();
    let mut res = Vec::new();
    while chars.len() >= MAX_CHARS {
        let fenetre = &chars[..MAX_CHARS - 1];
        let pos = fenetre
            .iter()
            .rposition(|c| matches!(c, ',' | ';' | ':'))
            .map(|p| p + 1)
            .or_else(|| fenetre.iter().rposition(|c| c.is_whitespace()))
            .filter(|&p| p > 0)
            .unwrap_or(MAX_CHARS - 1);
        let morceau: String = chars[..pos].iter().collect();
        let morceau = morceau.trim();
        if !morceau.is_empty() {
            res.push(morceau.to_string());
        }
        chars.drain(..pos);
        while chars.first().is_some_and(|c| c.is_whitespace()) {
            chars.remove(0);
        }
    }
    let reste: String = chars.iter().collect();
    if !reste.trim().is_empty() {
        res.push(reste.trim().to_string());
    }
    res
}

/// Normalise les blancs puis découpe en morceaux de moins de MAX_CHARS caractères (Unicode),
/// en ne coupant qu'entre deux phrases (après . ! ? … suivi d'un blanc ou de la fin).
/// Un "3.5" ou "M. Dupont" ne coupe pas. Texte vide → vec vide.
/// Une phrase seule plus longue que la limite est coupée aux virgules/espaces (cas rare).
pub fn decouper(texte: &str) -> Vec<String> {
    let normalise = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalise.is_empty() {
        return Vec::new();
    }
    let mut morceaux: Vec<String> = Vec::new();
    let mut courant = String::new();
    let mut n_courant = 0usize;
    for ph in phrases(&normalise) {
        let pieces = if ph.chars().count() >= MAX_CHARS { couper_long(&ph) } else { vec![ph] };
        for piece in pieces {
            let n = piece.chars().count();
            if n_courant > 0 && n_courant + 1 + n >= MAX_CHARS {
                morceaux.push(std::mem::take(&mut courant));
                n_courant = 0;
            }
            if n_courant > 0 {
                courant.push(' ');
                n_courant += 1;
            }
            courant.push_str(&piece);
            n_courant += n;
        }
    }
    if !courant.is_empty() {
        morceaux.push(courant);
    }
    morceaux
}

/// Nombre de mots approximatif et durée de lecture estimée (150 mots/min) : (mots, secondes).
pub fn estimation(texte: &str) -> (usize, f32) {
    let mots = texte.split_whitespace().filter(|m| m.chars().any(|c| c.is_alphanumeric())).count();
    (mots, mots as f32 * 60.0 / 150.0)
}

/// Textes modèles : les embarqués, puis tout .md/.txt de `dir_extra` dont le nom ne commence
/// pas par "0" (ceux-là sont déjà embarqués), triés par nom, titre = nom de fichier sans extension.
pub fn modeles(dir_extra: &Path) -> Vec<(String, String)> {
    let mut res: Vec<(String, String)> = EMBARQUES.iter().map(|(t, c)| (t.to_string(), c.to_string())).collect();
    let mut extras: Vec<(String, String)> = Vec::new();
    if let Ok(entrees) = std::fs::read_dir(dir_extra) {
        for e in entrees.flatten() {
            let chemin = e.path();
            let nom = match chemin.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let ext = chemin.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase());
            if !chemin.is_file() || nom.starts_with('0') || !matches!(ext.as_deref(), Some("md") | Some("txt")) {
                continue;
            }
            if let Ok(contenu) = std::fs::read_to_string(&chemin) {
                let titre = chemin.file_stem().and_then(|s| s.to_str()).unwrap_or(&nom).to_string();
                extras.push((nom, format!("{titre}\u{0}{contenu}")));
            }
        }
    }
    extras.sort_by(|a, b| a.0.cmp(&b.0));
    res.extend(extras.into_iter().filter_map(|(_, tc)| {
        let (t, c) = tc.split_once('\u{0}')?;
        Some((t.to_string(), c.to_string()))
    }));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vide() {
        assert!(decouper("").is_empty());
        assert!(decouper("  \n\t ").is_empty());
    }

    #[test]
    fn texte_court_un_seul_morceau_blancs_normalises() {
        let r = decouper("Bonjour   à tous.\n\nComment  ça va ? Très bien !");
        assert_eq!(r, vec!["Bonjour à tous. Comment ça va ? Très bien !"]);
    }

    #[test]
    fn nombre_decimal_ne_coupe_pas() {
        assert_eq!(phrases("La valeur est 3.5 mètres. Fin."), vec!["La valeur est 3.5 mètres.", "Fin."]);
        assert_eq!(phrases("Pi vaut 3.14159 environ."), vec!["Pi vaut 3.14159 environ."]);
    }

    #[test]
    fn abreviations_ne_coupent_pas() {
        assert_eq!(phrases("M. Dupont arrive. Mme Durand part."), vec!["M. Dupont arrive.", "Mme Durand part."]);
        assert_eq!(phrases("Voir J. Martin ici. Oui ?!"), vec!["Voir J. Martin ici.", "Oui ?!"]);
    }

    #[test]
    fn points_de_suspension_et_guillemets() {
        assert_eq!(phrases("Alors… Je pars. « Vraiment ? » Oui."), vec!["Alors…", "Je pars.", "« Vraiment ? »", "Oui."]);
    }

    fn phrase_de(n: usize, motif: &str) -> String {
        // Phrase d'environ n caractères, terminée par un point.
        let mut s = String::new();
        while s.chars().count() < n - 1 {
            s.push_str(motif);
            s.push(' ');
        }
        s.truncate(s.trim_end().len());
        s.push('.');
        s
    }

    #[test]
    fn long_texte_coupe_entre_les_phrases() {
        let phrases_src: Vec<String> = (0..30).map(|_| phrase_de(150, "mot")).collect();
        let texte = phrases_src.join(" ");
        let r = decouper(&texte);
        assert!(r.len() >= 3, "{} morceaux", r.len());
        for m in &r {
            assert!(m.chars().count() < MAX_CHARS, "{}", m.chars().count());
            assert!(m.ends_with('.'), "coupe au milieu d'une phrase");
        }
        // Rien de perdu : on retrouve toutes les phrases.
        assert_eq!(r.join(" "), texte);
    }

    #[test]
    fn accents_comptes_en_caracteres() {
        // 1 700 caractères mais ~3 400 octets : un seul morceau.
        let un = phrase_de(1700, "éàèù");
        assert!(un.len() > MAX_CHARS);
        let r = decouper(&un);
        assert_eq!(r.len(), 1);
        // Deux phrases de 1 000 caractères accentués : 2 morceaux (2 001 > 1 800).
        let deux = format!("{} {}", phrase_de(1000, "éèê"), phrase_de(1000, "çàù"));
        let r = decouper(&deux);
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|m| m.chars().count() < MAX_CHARS));
    }

    #[test]
    fn phrase_geante_sans_ponctuation_est_coupee() {
        let t = "abc def ".repeat(600); // 4 800 caractères, aucune ponctuation
        let r = decouper(&t);
        assert!(r.len() >= 3);
        assert!(r.iter().all(|m| m.chars().count() < MAX_CHARS));
        let t2 = "é".repeat(4000); // aucun espace non plus
        let r2 = decouper(&t2);
        assert!(r2.iter().all(|m| m.chars().count() < MAX_CHARS));
        assert_eq!(r2.concat().chars().count(), 4000);
    }

    #[test]
    fn estimation_mots() {
        let (m, s) = estimation("un deux  trois — quatre\ncinq");
        assert_eq!(m, 5);
        assert!((s - 2.0).abs() < 1e-4);
        assert_eq!(estimation("").0, 0);
    }

    #[test]
    fn modeles_embarques_et_extra() {
        let d = std::env::temp_dir().join(format!("vocalise-test-modeles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("06-doublon.md"), "ignoré").unwrap();
        std::fs::write(d.join("b-texte.txt"), "Corps B").unwrap();
        std::fs::write(d.join("a-texte.md"), "Corps A").unwrap();
        std::fs::write(d.join("image.png"), "x").unwrap();
        let m = modeles(&d);
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(m.len(), EMBARQUES.len() + 2);
        assert_eq!(m[0].0, EMBARQUES[0].0);
        assert_eq!(m[EMBARQUES.len()], ("a-texte".to_string(), "Corps A".to_string()));
        assert_eq!(m[EMBARQUES.len() + 1].0, "b-texte");
        assert_eq!(modeles(Path::new("/dossier/inexistant")).len(), EMBARQUES.len());
    }

    #[test]
    fn embarques_se_decoupent() {
        for (_, corps) in EMBARQUES {
            let r = decouper(corps);
            assert!(!r.is_empty());
            assert!(r.iter().all(|m| m.chars().count() < MAX_CHARS));
        }
    }
}
