//! Mode capture (`--capture <dossier>`) : enchaîne les écrans, prend une capture de chacun au
//! 3e frame rendu (et au moins 1,2 s après l'affichage, le temps que les données arrivent),
//! l'enregistre en PNG, puis ferme l'application. Rien n'est démarré ni enregistré.
//!
//! La progression dépend des frames rendus. Sous Wayland, Mutter ne renvoie plus de « frame
//! callback » à une fenêtre entièrement recouverte (ou sur un autre espace de travail) et winit ne
//! redessine alors plus : la capture restait bloquée dès que la fenêtre passait derrière une autre.
//! C'est pourquoi `app::lancer` ouvre la fenêtre de capture via X11 (XWayland), où une fenêtre
//! recouverte continue d'être dessinée (plus lentement, environ une image par seconde), et la place
//! au-dessus des autres.

use crate::app::{App, Dialogue, Ecran, Genere};
use crate::diagnostic::{Gravite, Verification};
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Captures produites, dans l'ordre.
const ETAPES: &[&str] = &["synthese", "ma-voix", "reglages", "confirmation", "enregistrement", "synthese-sans-alltalk", "diagnostic", "mises-a-jour"];

pub struct Capture {
    dossier: PathBuf,
    etape: usize,
    prepare: bool,
    frames: u32,
    debut: Instant,
    demande: Option<Instant>,
    essais: u32,
}

impl Capture {
    pub fn new(dossier: PathBuf) -> Capture {
        let _ = std::fs::create_dir_all(&dossier);
        // Garde-fou : si la fenêtre cessait malgré tout d'être dessinée, on ne reste pas bloqué
        // indéfiniment (une série complète prend 10 s fenêtre visible, 40 s recouverte).
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(120));
            eprintln!("capture : délai dépassé, abandon");
            std::process::abort();
        });
        Capture { dossier, etape: 0, prepare: false, frames: 0, debut: Instant::now(), demande: None, essais: 0 }
    }

    /// Appelé à la fin de chaque frame.
    pub fn avancer(&mut self, app: &mut App, ctx: &egui::Context) {
        ctx.request_repaint();
        let Some(nom) = ETAPES.get(self.etape) else { return };
        if !self.prepare {
            preparer(app, self.etape);
            self.prepare = true;
            self.frames = 0;
            self.debut = Instant::now();
            return;
        }
        self.frames += 1;

        if let Some(quand) = self.demande {
            let image = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            let fini = if let Some(img) = image {
                let chemin = self.dossier.join(format!("{nom}.png"));
                let [w, h] = img.size;
                match image::RgbaImage::from_raw(w as u32, h as u32, img.as_raw().to_vec()) {
                    Some(buf) => match buf.save_with_format(&chemin, image::ImageFormat::Png) {
                        Ok(()) => println!("capture : {}", chemin.display()),
                        Err(e) => eprintln!("capture : écriture de {} impossible : {e}", chemin.display()),
                    },
                    None => eprintln!("capture : image invalide pour {nom}"),
                }
                true
            } else if quand.elapsed() > Duration::from_secs(4) {
                if self.essais < 2 {
                    self.essais += 1;
                    eprintln!("capture : pas de réponse pour {nom}, nouvel essai");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                    self.demande = Some(Instant::now());
                    return;
                }
                eprintln!("capture : pas de réponse pour {nom}, étape ignorée");
                true
            } else {
                false
            };
            if fini {
                self.demande = None;
                self.essais = 0;
                self.prepare = false;
                self.etape += 1;
                if self.etape >= ETAPES.len() {
                    app.dialogue = None;
                    app.enregistrement = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            return;
        }

        if self.frames >= 3 && self.debut.elapsed() >= Duration::from_millis(1200) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.demande = Some(Instant::now());
        }
    }
}

/// Met l'application dans l'état à capturer.
fn preparer(app: &mut App, etape: usize) {
    app.dialogue = None;
    app.lecture = None;
    match ETAPES[etape] {
        "synthese" => {
            app.ecran = Ecran::Synthese;
            if app.texte.is_empty() {
                if let Some((titre, corps)) = app.modeles.first().cloned() {
                    app.texte = corps.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n").trim().to_string();
                    app.texte_source = Some(format!("{}.md", crate::app::etiquette_modele(&titre)));
                    app.nom_sortie = "cours-ebullition".into();
                    app.texte_modifie();
                }
            }
            // Résultat et historique factices, pour montrer la carte « Audio prêt ».
            let dir = app.cfg.sorties_dir.clone();
            let ext = app.format_effectif().extension();
            app.resultat = Some(Genere { chemin: dir.join(format!("cours-ebullition.{ext}")), duree: 74.0, heure: "14:32".into() });
            app.historique = vec![
                app.resultat.clone().unwrap(),
                Genere { chemin: dir.join(format!("consignes-tp.{ext}")), duree: 41.0, heure: "14:05".into() },
                Genere { chemin: dir.join(format!("narration.{ext}")), duree: 128.0, heure: "13:48".into() },
            ];
            // Lecture simulée du 2e fichier : son bouton montre ■ au lieu de ▶.
            app.lecture = Some(app.historique[1].chemin.clone());
        }
        "ma-voix" => app.ecran = Ecran::Voix,
        "reglages" => {
            app.ecran = Ecran::Reglages;
            // Une version plus récente simulée, pour montrer la carte « Mises à jour » et son avis.
            app.maj = Some(Ok(Some(crate::maj::Publication {
                version: crate::maj::Version(0, 3, 0),
                page: format!("https://github.com/{}/releases/tag/v0.3.0", crate::maj::DEPOT),
                paquet: Some(format!("https://github.com/{}/releases/download/v0.3.0/vocalise_0.3.0_amd64.deb", crate::maj::DEPOT)),
                date: Some("2026-11-02".into()),
            })));
        }
        "confirmation" => {
            // L'avis « nouvelle version » reste visible sur les autres écrans.
            app.ecran = Ecran::Voix;
            app.dialogue = Some(Dialogue::Reinitialiser { jeu: app.cfg.jeu_actif.clone(), n: app.echantillons.len() });
        }
        "enregistrement" => {
            app.ecran = Ecran::Voix;
            app.simuler_enregistrement();
        }
        "synthese-sans-alltalk" => {
            // Premier lancement typique : AllTalk arrêté, aucun fichier encore généré.
            app.enregistrement = None;
            app.ecran = Ecran::Synthese;
            app.sonde_figee = true;
            app.pret = Some(false);
            app.resultat = None;
            app.historique.clear();
            app.statut = None;
        }
        "mises-a-jour" => {
            // Réglages, défilé jusqu'à la carte « Mises à jour » (version simulée posée à l'étape reglages).
            app.diagnostic_simule = false;
            app.diagnostic = None;
            app.ecran = Ecran::Reglages;
            app.defiler_vers_maj = true;
        }
        "diagnostic" => {
            // Diagnostic simulé : AllTalk trouvé, moteur autre que XTTS (bloquant), ffmpeg absent,
            // jeu de voix vide (non bloquants). configurer_xtts n'est jamais appelé en capture.
            app.sonde_figee = false;
            app.ecran = Ecran::Reglages;
            app.diagnostic_simule = true;
            app.diagnostic = Some(diagnostic_simule(app));
        }
        _ => {}
    }
}

fn diagnostic_simule(app: &App) -> Vec<Verification> {
    let v = |composant: &str, ok: bool, gravite: Gravite, detail: String, lien: Option<&'static str>, commande: Option<&str>| Verification {
        composant: composant.into(),
        ok,
        gravite,
        detail,
        lien,
        commande: commande.map(String::from),
    };
    vec![
        v(
            "Serveur AllTalk",
            true,
            Gravite::Bloquant,
            format!("Installé dans {}.", app.cfg.alltalk_dir.display()),
            Some("https://github.com/erew123/alltalk_tts"),
            None,
        ),
        v(
            "Moteur XTTS",
            false,
            Gravite::Bloquant,
            "AllTalk est réglé pour charger le moteur « piper » : Vocalise a besoin de XTTS pour cloner votre voix.".into(),
            Some("https://github.com/erew123/alltalk_tts/wiki"),
            None,
        ),
        v("GPU NVIDIA", true, Gravite::Fonction, "NVIDIA GeForce GTX 1650, 4096 Mio.".into(), Some("https://www.nvidia.com/drivers"), None),
        v(
            "PipeWire (micro et écoute)",
            true,
            Gravite::Fonction,
            "pw-record, pw-play et pactl sont disponibles.".into(),
            Some("https://pipewire.org/"),
            None,
        ),
        v(
            "ffmpeg",
            false,
            Gravite::Fonction,
            "Introuvable : seuls les fichiers WAV peuvent être produits (pas de FLAC, MP3 ni OGG).".into(),
            Some("https://ffmpeg.org/download.html"),
            Some("sudo apt install ffmpeg"),
        ),
        v(
            "Jeu de voix",
            false,
            Gravite::Fonction,
            format!("Le jeu « {} » est vide : enregistrez un premier échantillon dans Ma voix.", app.cfg.jeu_actif),
            None,
            None,
        ),
    ]
}
