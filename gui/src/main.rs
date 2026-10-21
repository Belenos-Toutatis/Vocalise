//! Vocalise : synthèse vocale avec ma propre voix (AllTalk + XTTS) et gestion des jeux de voix.
//!
//! Modules « métier » (sans interface) : paths, audio, texte, alltalk, voicesets, recorder, player,
//! diagnostic, maj.
//! Interface : app (état et boucle) et ui/* (thème, écrans, widgets).
//!
//! Options :
//!   --capture <dossier>   captures PNG de chaque écran puis fermeture
//!   --theme clair|sombre  impose un thème (par défaut : celui du système)
//!   --taille LxH          taille initiale de la fenêtre (ex. 860x560)

mod alltalk;
mod app;
mod audio;
mod diagnostic;
mod maj;
mod paths;
mod player;
mod recorder;
mod texte;
mod ui;
mod voicesets;

fn main() -> eframe::Result {
    let mut opts = app::Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--capture" => match args.next() {
                Some(d) => opts.capture = Some(std::path::PathBuf::from(d)),
                None => {
                    eprintln!("--capture attend un dossier");
                    std::process::exit(2);
                }
            },
            "--theme" => {
                opts.theme = match args.next().as_deref() {
                    Some("clair") | Some("light") => Some(eframe::egui::Theme::Light),
                    Some("sombre") | Some("dark") => Some(eframe::egui::Theme::Dark),
                    _ => {
                        eprintln!("--theme attend « clair » ou « sombre »");
                        std::process::exit(2);
                    }
                }
            }
            "--taille" => {
                let t = args.next().unwrap_or_default();
                match t.split_once('x').and_then(|(l, h)| Some([l.parse::<f32>().ok()?, h.parse::<f32>().ok()?])) {
                    Some(t) => opts.taille = Some(t),
                    None => {
                        eprintln!("--taille attend LARGEURxHAUTEUR, par exemple 860x560");
                        std::process::exit(2);
                    }
                }
            }
            "-h" | "--help" => {
                println!("vocalise-gui [--capture <dossier>] [--theme clair|sombre] [--taille 860x560]");
                return Ok(());
            }
            autre => eprintln!("option inconnue ignorée : {autre}"),
        }
    }
    app::lancer(opts)
}
