//! Vérification de mise à jour : interroge la dernière publication (« release ») du dépôt GitHub
//! de Vocalise et la compare à la version compilée. Aucune installation automatique : on propose
//! seulement le lien du paquet `.deb` et la page des nouveautés.
//!
//! C'est le seul endroit du programme qui sort de la machine (HTTPS vers api.github.com, sans
//! authentification : 60 requêtes par heure suffisent largement pour une vérification par jour).

use serde::Deserialize;
use std::time::Duration;

/// Dépôt GitHub « propriétaire/nom » dont les publications font foi.
pub const DEPOT: &str = "Belenos-Toutatis/Vocalise";

/// Page des publications, pour l'utilisateur.
pub fn page_publications() -> String {
    format!("https://github.com/{DEPOT}/releases")
}

/// Version « majeure.mineure.correctif », comparable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Accepte « 0.2.1 », « v0.2.1 », « V0.2 » (correctif absent = 0) ; refuse le reste.
    pub fn parser(texte: &str) -> Option<Version> {
        let t = texte.trim().trim_start_matches(['v', 'V']);
        let mut parties = t.split('.');
        let majeure = parties.next()?.parse().ok()?;
        let mineure = parties.next()?.parse().ok()?;
        let correctif = match parties.next() {
            None => 0,
            Some(c) => c.parse().ok()?,
        };
        if parties.next().is_some() {
            return None;
        }
        Some(Version(majeure, mineure, correctif))
    }

    /// Version compilée, d'après `Cargo.toml`.
    pub fn courante() -> Version {
        Version::parser(env!("CARGO_PKG_VERSION")).expect("version Cargo valide")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Une publication GitHub.
#[derive(Clone, Debug, PartialEq)]
pub struct Publication {
    pub version: Version,
    /// Page de la publication (nouveautés, fichiers).
    pub page: String,
    /// Lien direct du paquet `.deb`, s'il est joint.
    pub paquet: Option<String>,
    /// Date de publication (AAAA-MM-JJ), si GitHub la donne.
    pub date: Option<String>,
}

/// Résultat d'une vérification : `None` quand la version installée est la plus récente.
pub type Resultat = Result<Option<Publication>, String>;

#[derive(Deserialize)]
struct ReponseRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Lit la réponse de `GET /repos/<dépôt>/releases/latest`.
pub fn parser_publication(json: &str) -> Result<Publication, String> {
    let r: ReponseRelease = serde_json::from_str(json).map_err(|e| format!("réponse de GitHub illisible ({e})"))?;
    let version = Version::parser(&r.tag_name).ok_or_else(|| format!("étiquette de version inattendue : « {} »", r.tag_name))?;
    let paquet = r.assets.iter().find(|a| a.name.ends_with(".deb")).map(|a| a.browser_download_url.clone());
    let date = r.published_at.as_deref().map(|d| d.chars().take(10).collect());
    Ok(Publication { version, page: r.html_url, paquet, date })
}

/// Compare la publication à la version installée : `Some` si elle est plus récente.
pub fn plus_recente(publication: Publication, installee: Version) -> Option<Publication> {
    (publication.version > installee).then_some(publication)
}

/// Interroge GitHub (jusqu'à 8 s) et renvoie la publication plus récente que la version compilée,
/// `None` si l'on est à jour ou si rien n'a encore été publié. Les erreurs sont des phrases
/// prêtes à afficher.
pub fn verifier(depot: &str) -> Resultat {
    let url = format!("https://api.github.com/repos/{depot}/releases/latest");
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build();
    let reponse = agent
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", concat!("vocalise-gui/", env!("CARGO_PKG_VERSION")));
    match reponse.call() {
        Ok(r) => {
            let texte = r.into_string().map_err(|e| format!("réponse de GitHub illisible ({e})"))?;
            Ok(plus_recente(parser_publication(&texte)?, Version::courante()))
        }
        // Dépôt sans publication (ou privé) : rien à proposer, ce n'est pas une erreur.
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(ureq::Error::Status(code, _)) => Err(format!("GitHub a répondu avec le code {code}")),
        Err(ureq::Error::Transport(t)) => Err(format!("pas de connexion à GitHub ({})", t.kind())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPONSE: &str = r#"{
        "tag_name": "v0.3.0",
        "html_url": "https://github.com/Belenos-Toutatis/Vocalise/releases/tag/v0.3.0",
        "published_at": "2026-11-02T10:11:12Z",
        "assets": [
            {"name": "SHA256SUMS", "browser_download_url": "https://x/SHA256SUMS"},
            {"name": "vocalise_0.3.0_amd64.deb", "browser_download_url": "https://x/vocalise_0.3.0_amd64.deb"}
        ]
    }"#;

    #[test]
    fn parser_versions() {
        assert_eq!(Version::parser("0.2.1"), Some(Version(0, 2, 1)));
        assert_eq!(Version::parser("v1.0.0"), Some(Version(1, 0, 0)));
        assert_eq!(Version::parser(" V2.5 "), Some(Version(2, 5, 0)));
        assert_eq!(Version::parser("1.2.3.4"), None);
        assert_eq!(Version::parser("beta"), None);
        assert_eq!(Version::parser(""), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert!(Version(1, 0, 0) > Version(0, 99, 99));
        assert_eq!(Version(0, 2, 1).to_string(), "0.2.1");
        assert_eq!(Version::courante(), Version::parser(env!("CARGO_PKG_VERSION")).unwrap());
    }

    #[test]
    fn parser_reponse_github() {
        let p = parser_publication(REPONSE).unwrap();
        assert_eq!(p.version, Version(0, 3, 0));
        assert_eq!(p.page, "https://github.com/Belenos-Toutatis/Vocalise/releases/tag/v0.3.0");
        assert_eq!(p.paquet.as_deref(), Some("https://x/vocalise_0.3.0_amd64.deb"));
        assert_eq!(p.date.as_deref(), Some("2026-11-02"));
    }

    #[test]
    fn reponse_sans_paquet_ni_date() {
        let p = parser_publication(r#"{"tag_name":"v0.2.1","html_url":"https://x"}"#).unwrap();
        assert_eq!(p.version, Version(0, 2, 1));
        assert_eq!(p.paquet, None);
        assert_eq!(p.date, None);
    }

    #[test]
    fn reponses_invalides() {
        assert!(parser_publication("pas du json").unwrap_err().contains("illisible"));
        assert!(parser_publication(r#"{"tag_name":"nightly","html_url":"https://x"}"#).unwrap_err().contains("nightly"));
    }

    #[test]
    fn comparaison_avec_la_version_installee() {
        let p = parser_publication(REPONSE).unwrap();
        assert_eq!(plus_recente(p.clone(), Version(0, 2, 1)).as_ref(), Some(&p));
        assert_eq!(plus_recente(p.clone(), Version(0, 3, 0)), None);
        assert_eq!(plus_recente(p, Version(1, 0, 0)), None);
    }

    /// Test réel contre GitHub : `cargo test -- --ignored maj::`.
    #[test]
    #[ignore]
    fn integration_github() {
        let r = verifier(DEPOT).unwrap();
        println!("publication plus récente de {DEPOT} : {r:?}");
        // Dépôt public connu pour avoir des publications bien plus récentes que Vocalise :
        // valide le chemin réseau et la lecture d'une vraie réponse.
        let p = verifier("cli/cli").unwrap().expect("cli/cli publie des versions");
        println!("cli/cli : {p:?}");
        assert!(p.version > Version::courante());
        assert!(p.page.starts_with("https://github.com/cli/cli/releases/"));
    }
}
