//! État de l'application, messages des tâches de fond, boucle eframe.
//!
//! Toute opération longue (génération, traitement d'un échantillon, sonde ou arrêt du serveur)
//! tourne dans un `std::thread` et renvoie un [`Msg`] par canal ; l'interface ne bloque jamais.

use crate::audio::{self, Format};
use crate::paths::{self, Config};
use crate::player::Lecteur;
use crate::{alltalk, diagnostic, maj, recorder, texte, ui, voicesets};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Options de la ligne de commande.
#[derive(Default)]
pub struct Options {
    /// `--capture <dossier>` : captures d'écran automatiques puis fermeture.
    pub capture: Option<PathBuf>,
    /// `--theme clair|sombre` : impose un thème (sinon celui du système).
    pub theme: Option<egui::Theme>,
    /// `--taille 860x560` : taille initiale de la fenêtre (utile pour les captures).
    pub taille: Option<[f32; 2]>,
}

pub fn lancer(opts: Options) -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Vocalise")
        .with_app_id("vocalise-gui")
        .with_inner_size(opts.taille.unwrap_or([1100.0, 720.0]))
        .with_min_inner_size([860.0, 560.0]);
    if let Ok(icone) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/vocalise-256.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icone));
    }
    if opts.capture.is_some() {
        // Mode capture : fenêtre via X11 (XWayland) et au-dessus des autres. Sous Wayland, une
        // fenêtre recouverte n'est plus redessinée du tout et la capture se figeait (voir ui/capture).
        if std::env::var("DISPLAY").map(|d| !d.is_empty()).unwrap_or(false) {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
        viewport = viewport.with_always_on_top();
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let r = eframe::run_native("Vocalise", options, Box::new(move |cc| Ok(Box::new(App::new(cc, opts)))));
    if let Err(e) = &r {
        if !AFFICHE.load(Ordering::Relaxed) && std::env::var_os("VOCALISE_LOGICIEL").is_none() {
            // La carte graphique a refusé le contexte OpenGL (mémoire vidéo saturée, par exemple
            // quand AllTalk occupe le GPU) : on relance en rendu logiciel (Mesa llvmpipe).
            eprintln!("Vocalise : rendu matériel indisponible ({e}), relance en rendu logiciel.");
            return relancer_en_logiciel();
        }
    }
    r
}

/// Vrai dès qu'un premier frame a été dessiné.
static AFFICHE: AtomicBool = AtomicBool::new(false);

/// Date du jour (AAAA-MM-JJ), pour ne vérifier les mises à jour qu'une fois par jour.
pub fn aujourd_hui() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn relancer_en_logiciel() -> eframe::Result {
    let exe = std::env::current_exe().map_err(|e| eframe::Error::AppCreation(Box::new(e)))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args_os().skip(1)).env("VOCALISE_LOGICIEL", "1").env("LIBGL_ALWAYS_SOFTWARE", "1");
    let mesa = Path::new("/usr/share/glvnd/egl_vendor.d/50_mesa.json");
    if mesa.exists() {
        cmd.env("__EGL_VENDOR_LIBRARY_FILENAMES", mesa);
    }
    let statut = cmd.status().map_err(|e| eframe::Error::AppCreation(Box::new(e)))?;
    std::process::exit(statut.code().unwrap_or(1));
}

// ------------------------------------------------------------------ types

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ecran {
    Synthese,
    Voix,
    Reglages,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Niveau {
    Info,
    Succes,
    Erreur,
}

/// Dernier message affiché dans la barre d'état.
pub struct Statut {
    pub texte: String,
    pub niveau: Niveau,
    pub quand: Instant,
}

/// Messages des tâches de fond vers l'état.
pub enum Msg {
    Sonde(bool),
    Voix(Vec<String>),
    Progression(alltalk::Progression),
    Conversion(Format),
    Generation(Result<Genere, String>),
    Echantillon(Result<PathBuf, String>),
    ServeurArrete,
    Diagnostic(Vec<diagnostic::Verification>),
    /// Résultat d'une vérification de mise à jour (`manuelle` = demandée par un bouton).
    MiseAJour { resultat: maj::Resultat, manuelle: bool },
}

/// Fichier produit pendant la session.
#[derive(Clone, Debug)]
pub struct Genere {
    pub chemin: PathBuf,
    pub duree: f32,
    pub heure: String,
}

impl Genere {
    pub fn nom(&self) -> String {
        self.chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

pub enum Phase {
    Preparation,
    Morceau(alltalk::Progression),
    Conversion(Format),
}

pub struct Generation {
    pub phase: Phase,
    pub debut: Instant,
    pub sortie: PathBuf,
}

pub enum EtatServeur {
    /// Première sonde pas encore revenue.
    Verification,
    Pret { lance_par_nous: bool },
    Demarrage(Duration),
    Arret,
    Arrete,
}

pub enum Source {
    Reel(recorder::Enregistreur),
    /// Enregistrement factice (mode capture) : chrono et niveau figés.
    Simule,
}

pub struct Enregistrement {
    pub source: Source,
    pub jeu: String,
    pub nom: String,
    /// Niveau affiché (dBFS), avec retombée progressive.
    pub niveau_db: f32,
    /// Pic retenu (dBFS) et instant où il a été atteint.
    pub pic_db: f32,
    pub pic_t: Instant,
    pub mesure_t: Instant,
}

impl Enregistrement {
    pub fn duree(&self) -> f32 {
        match &self.source {
            Source::Reel(e) => e.duree(),
            Source::Simule => 23.0,
        }
    }
}

/// Boîtes de dialogue modales.
pub enum Dialogue {
    NouveauJeu { nom: String, erreur: Option<String> },
    Reinitialiser { jeu: String, n: usize },
    SupprimerJeu { jeu: String, n: usize },
    Exporter { chemin: PathBuf, nom: String, erreur: Option<String> },
    RemplacerTexte { chemin: PathBuf },
}

#[derive(Default)]
pub struct Stats {
    pub rev: u64,
    pub caracteres: usize,
    pub mots: usize,
    pub secondes: f32,
    pub morceaux: usize,
}

// ------------------------------------------------------------------ application

pub struct App {
    pub cfg: Config,
    pub capture: Option<ui::capture::Capture>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ctx: egui::Context,

    pub ecran: Ecran,
    pub statut: Option<Statut>,
    pub dialogue: Option<Dialogue>,
    pub logo: Option<egui::TextureHandle>,

    // Serveur AllTalk
    pub pret: Option<bool>,
    /// Mode capture : ignorer la sonde pour montrer l'état « arrêté ».
    pub sonde_figee: bool,
    pub serveur: Option<alltalk::Serveur>,
    pub serveur_debut: Option<Instant>,
    pub arret_en_cours: bool,

    // Synthèse
    pub voix_liste: Vec<String>,
    pub voix_chargees: bool,
    pub texte: String,
    pub texte_rev: u64,
    pub stats: Stats,
    pub texte_source: Option<String>,
    pub nom_sortie: String,
    pub ffmpeg: bool,
    pub generation: Option<Generation>,
    pub resultat: Option<Genere>,
    pub historique: Vec<Genere>,

    // Lecture
    pub lecteur: Lecteur,
    pub lecture: Option<PathBuf>,

    // Ma voix
    pub jeux: Vec<String>,
    pub echantillons: Vec<voicesets::Echantillon>,
    pub modeles: Vec<(String, String)>,
    pub modele_idx: usize,
    pub micros: Vec<recorder::Micro>,
    /// Micro réellement utilisé pendant cette session : `cfg.micro` s'il est branché, sinon le
    /// micro par défaut. `cfg.micro` n'est modifié que par un choix explicite de l'utilisateur.
    pub micro_effectif: String,
    /// Textes modèles découpés en paragraphes (précalculé).
    pub modeles_paragraphes: Vec<Vec<String>>,
    /// Début de la lecture en cours (pour détecter un pw-play qui échoue aussitôt).
    pub lecture_debut: Option<Instant>,
    pub etiquette_echantillon: String,
    pub enregistrement: Option<Enregistrement>,
    pub traitement: bool,
    pub dernier_ajout: Option<PathBuf>,

    // Diagnostic de l'installation
    pub diagnostic: Option<Vec<diagnostic::Verification>>,
    pub diagnostic_en_cours: bool,
    /// Mode capture : diagnostic simulé, les résultats réels sont ignorés.
    pub diagnostic_simule: bool,
    /// L'avertissement orange (manques non bloquants) a été fermé pour cette session.
    pub avis_diagnostic_ferme: bool,
    /// Le moteur XTTS vient d'être configuré : AllTalk doit être redémarré.
    pub xtts_configure: bool,
    /// Afficher le détail du diagnostic même quand tout est en ordre.
    pub diagnostic_detail: bool,
    /// Relancer AllTalk dès que son arrêt est terminé.
    pub redemarrer_apres_arret: bool,

    // Mise à jour
    /// Dernier résultat connu (None : jamais vérifié depuis le lancement).
    pub maj: Option<maj::Resultat>,
    pub maj_en_cours: bool,
    /// L'avis « nouvelle version disponible » a été fermé pour cette session.
    pub avis_maj_ferme: bool,
    /// Mode capture : faire défiler Réglages jusqu'à la carte « Mises à jour ».
    pub defiler_vers_maj: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, opts: Options) -> App {
        let ctx = cc.egui_ctx.clone();
        ui::theme::installer(&ctx, opts.theme);
        let (tx, rx) = mpsc::channel();
        let cfg = Config::charger();
        let capture = opts.capture.map(ui::capture::Capture::new);

        let micros = recorder::micros();
        let modeles = texte::modeles(&cfg.textes_dir());
        let modeles_paragraphes = modeles.iter().map(|(_, corps)| paragraphes(corps)).collect();
        let etiquette_echantillon = modeles.first().map(|(t, _)| etiquette_modele(t)).unwrap_or_default();

        let mut app = App {
            capture,
            tx,
            rx,
            ctx: ctx.clone(),
            ecran: Ecran::Synthese,
            statut: None,
            dialogue: None,
            logo: charger_logo(&ctx),
            pret: None,
            sonde_figee: false,
            serveur: None,
            serveur_debut: None,
            arret_en_cours: false,
            voix_liste: Vec::new(),
            voix_chargees: false,
            texte: String::new(),
            texte_rev: 1,
            stats: Stats::default(),
            texte_source: None,
            nom_sortie: "narration".into(),
            ffmpeg: audio::ffmpeg_disponible(),
            generation: None,
            resultat: None,
            historique: Vec::new(),
            lecteur: Lecteur::new(),
            lecture: None,
            jeux: Vec::new(),
            echantillons: Vec::new(),
            modeles,
            modele_idx: 0,
            micros,
            micro_effectif: String::new(),
            modeles_paragraphes,
            lecture_debut: None,
            diagnostic: None,
            diagnostic_en_cours: false,
            diagnostic_simule: false,
            avis_diagnostic_ferme: false,
            xtts_configure: false,
            diagnostic_detail: false,
            redemarrer_apres_arret: false,
            maj: None,
            maj_en_cours: false,
            avis_maj_ferme: false,
            defiler_vers_maj: false,
            etiquette_echantillon,
            enregistrement: None,
            traitement: false,
            dernier_ajout: None,
            cfg,
        };
        app.resoudre_micro();
        app.rafraichir_jeux();
        app.rafraichir_voix();
        app.lancer_sonde();
        app.lancer_diagnostic();
        if app.cfg.verifier_maj && app.capture.is_none() && app.cfg.maj_verifiee_le.as_deref() != Some(&aujourd_hui()) {
            app.verifier_maj(false);
        }
        if let Some(m) = app.cfg.avertissement_chargement.clone() {
            app.erreur(m);
        }
        app
    }

    /// Calcule le micro de la session à partir de `cfg.micro` et des micros branchés.
    pub fn resoudre_micro(&mut self) {
        self.micro_effectif = if !self.cfg.micro.is_empty() && self.micros.iter().any(|m| m.id == self.cfg.micro) {
            self.cfg.micro.clone()
        } else {
            recorder::micro_par_defaut(&self.micros).map(|m| m.id.clone()).unwrap_or_default()
        };
    }

    /// Choix explicite d'un micro dans la liste ("" = automatique).
    pub fn choisir_micro(&mut self, id: &str) {
        self.cfg.micro = id.to_string();
        self.resoudre_micro();
    }

    /// Le micro habituel (`cfg.micro`) est-il débranché ?
    pub fn micro_habituel_absent(&self) -> bool {
        !self.cfg.micro.is_empty() && !self.micros.iter().any(|m| m.id == self.cfg.micro)
    }

    pub fn nom_micro(&self, id: &str) -> String {
        if id.is_empty() {
            return "Source par défaut du système".into();
        }
        self.micros.iter().find(|m| m.id == id).map(|m| m.nom.clone()).unwrap_or_else(|| id.to_string())
    }

    // -------------------------------------------------------------- messages

    pub fn info(&mut self, t: impl Into<String>) {
        self.statut = Some(Statut { texte: t.into(), niveau: Niveau::Info, quand: Instant::now() });
    }
    pub fn succes(&mut self, t: impl Into<String>) {
        self.statut = Some(Statut { texte: t.into(), niveau: Niveau::Succes, quand: Instant::now() });
    }
    pub fn erreur(&mut self, t: impl Into<String>) {
        self.statut = Some(Statut { texte: t.into(), niveau: Niveau::Erreur, quand: Instant::now() });
    }

    fn traiter_messages(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Sonde(_) if self.sonde_figee => {}
                Msg::Sonde(ok) => {
                    let avant = self.pret;
                    self.pret = Some(ok);
                    if ok && avant != Some(true) {
                        if self.serveur.is_some() && avant.is_some() {
                            self.succes("AllTalk est prêt : vous pouvez générer.");
                        }
                        self.rafraichir_voix();
                    }
                }
                Msg::Voix(liste) => {
                    self.voix_liste = liste;
                    self.voix_chargees = true;
                    if self.cfg.voix.is_empty() {
                        if let Some(v) = self.voix_liste.first() {
                            self.cfg.voix = v.clone();
                        }
                    }
                }
                Msg::Progression(p) => {
                    if let Some(g) = &mut self.generation {
                        g.phase = Phase::Morceau(p);
                    }
                }
                Msg::Conversion(f) => {
                    if let Some(g) = &mut self.generation {
                        g.phase = Phase::Conversion(f);
                    }
                }
                Msg::Generation(r) => {
                    let duree_calcul = self.generation.take().map(|g| g.debut.elapsed().as_secs_f32());
                    match r {
                        Ok(g) => {
                            let mut m = format!("Audio prêt : {} ({})", g.nom(), ui::widgets::duree_lisible(g.duree));
                            if let Some(d) = duree_calcul {
                                m.push_str(&format!(", calculé en {}.", ui::widgets::duree_lisible(d)));
                            }
                            self.succes(m);
                            self.historique.insert(0, g.clone());
                            self.historique.truncate(20);
                            self.resultat = Some(g);
                        }
                        Err(e) => self.erreur(format!("La génération a échoué : {e}")),
                    }
                }
                Msg::Echantillon(r) => {
                    self.traitement = false;
                    match r {
                        Ok(p) => {
                            self.rafraichir_echantillons();
                            let duree = self.echantillons.iter().find(|e| e.chemin == p).map(|e| e.duree);
                            let nom = p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            self.succes(match duree {
                                Some(d) => format!("Échantillon « {nom} » ajouté ({}). Écoutez-le pour vérifier.", ui::widgets::duree_lisible(d)),
                                None => format!("Échantillon « {nom} » ajouté."),
                            });
                            self.dernier_ajout = Some(p);
                            self.rafraichir_jeux();
                            self.rafraichir_voix();
                            self.lancer_diagnostic();
                        }
                        Err(e) => self.erreur(format!("L'échantillon n'a pas pu être ajouté : {e}")),
                    }
                }
                Msg::ServeurArrete => {
                    self.arret_en_cours = false;
                    self.pret = Some(false);
                    if std::mem::take(&mut self.redemarrer_apres_arret) {
                        self.demarrer_serveur();
                    } else {
                        self.info("AllTalk est arrêté.");
                    }
                }
                Msg::Diagnostic(liste) => {
                    self.diagnostic_en_cours = false;
                    if !self.diagnostic_simule {
                        self.diagnostic = Some(liste);
                    }
                }
                Msg::MiseAJour { resultat, manuelle } => {
                    self.maj_en_cours = false;
                    match &resultat {
                        Ok(Some(p)) => {
                            self.cfg.maj_verifiee_le = Some(aujourd_hui());
                            self.sauver();
                            self.info(format!("Nouvelle version {} disponible : voir Réglages → Mises à jour.", p.version));
                        }
                        Ok(None) => {
                            self.cfg.maj_verifiee_le = Some(aujourd_hui());
                            self.sauver();
                            if manuelle {
                                self.succes(format!("Vous avez la dernière version ({}).", maj::Version::courante()));
                            }
                        }
                        // Au démarrage, une panne de réseau reste silencieuse : on réessaiera au prochain lancement.
                        Err(e) if manuelle => self.erreur(format!("Vérification impossible : {e}.")),
                        Err(_) => {}
                    }
                    self.maj = Some(resultat);
                }
            }
        }
    }

    // -------------------------------------------------------------- serveur

    fn lancer_sonde(&self) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || loop {
            let ok = alltalk::pret();
            if tx.send(Msg::Sonde(ok)).is_err() {
                break;
            }
            ctx.request_repaint();
            thread::sleep(Duration::from_secs(if ok { 5 } else { 2 }));
        });
    }

    pub fn etat_serveur(&self) -> EtatServeur {
        if self.arret_en_cours {
            EtatServeur::Arret
        } else if self.pret == Some(true) {
            EtatServeur::Pret { lance_par_nous: self.serveur.is_some() }
        } else if self.serveur.is_some() {
            EtatServeur::Demarrage(self.serveur_debut.map(|d| d.elapsed()).unwrap_or_default())
        } else if self.pret.is_none() {
            EtatServeur::Verification
        } else {
            EtatServeur::Arrete
        }
    }

    pub fn serveur_pret(&self) -> bool {
        self.pret == Some(true) && !self.arret_en_cours
    }

    pub fn demarrer_serveur(&mut self) {
        if self.capture.is_some() || self.serveur.is_some() {
            return;
        }
        match alltalk::Serveur::demarrer(&self.cfg.alltalk_dir, &self.cfg.journal()) {
            Ok(s) => {
                self.serveur = Some(s);
                self.serveur_debut = Some(Instant::now());
                self.info("Démarrage d'AllTalk… Le chargement du modèle prend environ une minute.");
            }
            Err(e) => self.erreur(format!("Impossible de démarrer AllTalk : {e}")),
        }
    }

    pub fn arreter_serveur(&mut self) {
        let Some(mut s) = self.serveur.take() else { return };
        self.arret_en_cours = true;
        self.serveur_debut = None;
        self.info("Arrêt d'AllTalk…");
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            s.arreter();
            let _ = tx.send(Msg::ServeurArrete);
            ctx.request_repaint();
        });
    }

    /// Détecte un serveur lancé par l'application qui se serait arrêté tout seul.
    fn surveiller_serveur(&mut self) {
        if let Some(s) = &mut self.serveur {
            if !s.en_vie() {
                self.serveur = None;
                self.serveur_debut = None;
                self.pret = Some(false);
                self.erreur("AllTalk s'est arrêté de façon inattendue. Consultez le journal (Réglages → Voir le journal).");
            }
        }
    }

    // -------------------------------------------------------------- diagnostic

    /// Vérifie l'installation dans un thread (au démarrage, sur demande, après un changement).
    pub fn lancer_diagnostic(&mut self) {
        if self.diagnostic_en_cours || self.diagnostic_simule {
            return;
        }
        self.diagnostic_en_cours = true;
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            // Si la vérification échoue (panique), on rend une liste vide plutôt que de rester bloqué.
            let liste = std::panic::catch_unwind(|| diagnostic::verifier(&cfg)).unwrap_or_default();
            let _ = tx.send(Msg::Diagnostic(liste));
            ctx.request_repaint();
        });
    }

    // -------------------------------------------------------------- mise à jour

    /// Interroge GitHub dans un thread ; `manuelle` quand l'utilisateur a cliqué (messages explicites).
    pub fn verifier_maj(&mut self, manuelle: bool) {
        if self.maj_en_cours || self.capture.is_some() {
            return;
        }
        self.maj_en_cours = true;
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let resultat = std::panic::catch_unwind(|| maj::verifier(maj::DEPOT))
                .unwrap_or_else(|_| Err("échec interne de la vérification".into()));
            let _ = tx.send(Msg::MiseAJour { resultat, manuelle });
            ctx.request_repaint();
        });
    }

    /// Publication plus récente connue, s'il y en a une.
    pub fn maj_disponible(&self) -> Option<&maj::Publication> {
        match &self.maj {
            Some(Ok(Some(p))) => Some(p),
            _ => None,
        }
    }

    pub fn diagnostic_bloque(&self) -> bool {
        self.diagnostic.as_deref().is_some_and(diagnostic::bloque)
    }

    /// Vérifications en échec, bloquantes ou non.
    pub fn manques(&self, gravite: diagnostic::Gravite) -> Vec<String> {
        self.diagnostic
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|v| !v.ok && v.gravite == gravite)
            .map(|v| v.composant.clone())
            .collect()
    }

    /// Bouton « Configurer XTTS » : force le moteur XTTS dans la configuration d'AllTalk.
    pub fn configurer_xtts(&mut self) {
        if self.capture.is_some() {
            return;
        }
        match diagnostic::configurer_xtts(&self.cfg.alltalk_dir) {
            Ok(()) => {
                self.xtts_configure = true;
                self.succes("Moteur configuré : redémarrez AllTalk.");
            }
            Err(e) => self.erreur(format!("Impossible de configurer XTTS : {e}")),
        }
        self.lancer_diagnostic();
    }

    /// Arrête puis relance le serveur lancé par Vocalise.
    pub fn redemarrer_serveur(&mut self) {
        if self.serveur.is_some() {
            self.redemarrer_apres_arret = true;
            self.arreter_serveur();
        } else {
            self.demarrer_serveur();
        }
    }

    // -------------------------------------------------------------- synthèse

    pub fn rafraichir_voix(&self) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let dir = self.cfg.voices_dir();
        thread::spawn(move || {
            let v = alltalk::voix_disponibles(&dir);
            let _ = tx.send(Msg::Voix(v));
            ctx.request_repaint();
        });
    }

    pub fn texte_modifie(&mut self) {
        self.texte_rev = self.texte_rev.wrapping_add(1);
    }

    pub fn maj_stats(&mut self) {
        if self.stats.rev == self.texte_rev {
            return;
        }
        let (mots, secondes) = if self.texte.trim().is_empty() { (0, 0.0) } else { texte::estimation(&self.texte) };
        self.stats = Stats {
            rev: self.texte_rev,
            caracteres: self.texte.chars().count(),
            mots,
            secondes,
            morceaux: if self.texte.trim().is_empty() { 0 } else { texte::decouper(&self.texte).len() },
        };
    }

    /// Format réellement utilisable (WAV si ffmpeg est absent).
    pub fn format_effectif(&self) -> Format {
        if self.ffmpeg {
            self.cfg.format
        } else {
            Format::Wav
        }
    }

    pub fn nom_de_base(&self) -> String {
        let brut = self.nom_sortie.trim();
        let n = paths::nom_sur(if brut.is_empty() { "narration" } else { brut });
        if n.is_empty() {
            "narration".into()
        } else {
            n
        }
    }

    /// Chemin de sortie libre : `<nom>.<ext>`, sinon `<nom>-2.<ext>`, etc.
    pub fn chemin_sortie(&self) -> PathBuf {
        let base = self.nom_de_base();
        let ext = self.format_effectif().extension();
        let dir = &self.cfg.sorties_dir;
        let mut c = dir.join(format!("{base}.{ext}"));
        let mut i = 2;
        while c.exists() || (self.format_effectif() != Format::Wav && c.with_extension("tmp.wav").exists()) {
            c = dir.join(format!("{base}-{i}.{ext}"));
            i += 1;
        }
        c
    }

    pub fn peut_generer(&self) -> bool {
        self.generation.is_none() && self.serveur_pret() && !self.texte.trim().is_empty() && !self.cfg.voix.is_empty()
    }

    pub fn generer(&mut self) {
        if !self.peut_generer() {
            return;
        }
        let _ = std::fs::create_dir_all(&self.cfg.sorties_dir);
        let format = self.format_effectif();
        let sortie = self.chemin_sortie();
        let wav = if format == Format::Wav { sortie.clone() } else { sortie.with_extension("tmp.wav") };
        let req = alltalk::Requete {
            texte: self.texte.clone(),
            voix: self.cfg.voix.clone(),
            langue: self.cfg.langue.clone(),
            vitesse: self.cfg.vitesse,
            temperature: self.cfg.temperature,
        };
        self.generation = Some(Generation { phase: Phase::Preparation, debut: Instant::now(), sortie: sortie.clone() });
        self.resultat = None;
        self.info(format!("Génération de {}…", sortie.file_name().unwrap_or_default().to_string_lossy()));
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let r = (|| -> Result<Genere, String> {
                let tx_p = tx.clone();
                let ctx_p = ctx.clone();
                let rappel = move |p: alltalk::Progression| {
                    let _ = tx_p.send(Msg::Progression(p));
                    ctx_p.request_repaint();
                };
                let produit = alltalk::generer(&req, &wav, &rappel)?;
                let duree = audio::duree(&produit).unwrap_or(0.0);
                let chemin = if format == Format::Wav {
                    produit
                } else {
                    let _ = tx.send(Msg::Conversion(format));
                    ctx.request_repaint();
                    match audio::convertir(&produit, &sortie, format) {
                        Ok(p) => {
                            // Le wav intermédiaire n'est supprimé qu'une fois la conversion réussie.
                            let _ = std::fs::remove_file(&produit);
                            p
                        }
                        Err(e) => {
                            let garde = chemin_libre(&sortie.with_extension("wav"));
                            let conserve = if std::fs::rename(&produit, &garde).is_ok() { garde } else { produit };
                            return Err(format!("{e} (audio WAV conservé : {})", conserve.display()));
                        }
                    }
                };
                Ok(Genere { chemin, duree, heure: chrono::Local::now().format("%H:%M").to_string() })
            })();
            let _ = tx.send(Msg::Generation(r));
            ctx.request_repaint();
        });
    }

    /// Charge un fichier texte dans la zone de synthèse.
    /// Demande d'ouverture d'un fichier texte (bouton ou glisser-déposer) : vérifications, puis
    /// confirmation si la zone de texte contient déjà quelque chose.
    pub fn ouvrir_texte(&mut self, chemin: &Path) {
        self.ecran = Ecran::Synthese;
        if self.generation.is_some() {
            self.erreur("Impossible d'ouvrir un texte pendant une génération.");
            return;
        }
        let ext = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext != "txt" && ext != "md" {
            self.erreur("Seuls les fichiers texte .txt et .md peuvent être ouverts.");
            return;
        }
        match std::fs::metadata(chemin) {
            Ok(m) if m.len() > 1_000_000 => {
                self.erreur("Fichier trop volumineux (plus de 1 Mo) : découpez-le en plusieurs textes.");
                return;
            }
            Err(e) => {
                self.erreur(format!("Impossible de lire {} : {e}", chemin.display()));
                return;
            }
            _ => {}
        }
        if self.texte.trim().is_empty() {
            self.charger_texte(chemin);
        } else {
            self.dialogue = Some(Dialogue::RemplacerTexte { chemin: chemin.to_path_buf() });
        }
    }

    /// Charge le fichier dans la zone de texte (vérifications déjà faites).
    pub fn charger_texte(&mut self, chemin: &Path) {
        match std::fs::read(chemin) {
            Ok(octets) => {
                let t = String::from_utf8_lossy(&octets).into_owned();
                self.texte = t;
                self.texte_modifie();
                let nom = chemin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(stem) = chemin.file_stem() {
                    let s = paths::nom_sur(&stem.to_string_lossy());
                    if !s.is_empty() {
                        self.nom_sortie = s;
                    }
                }
                self.info(format!("Texte chargé : {nom}"));
                self.texte_source = Some(nom);
            }
            Err(e) => self.erreur(format!("Impossible de lire {} : {e}", chemin.display())),
        }
    }

    pub fn choisir_texte(&mut self) {
        let mut d = rfd::FileDialog::new()
            .set_title("Ouvrir un texte à synthétiser")
            .add_filter("Texte (.txt, .md)", &["txt", "md"]);
        if let Ok(home) = std::env::var("HOME") {
            d = d.set_directory(home);
        }
        if let Some(p) = d.pick_file() {
            self.ouvrir_texte(&p);
        }
    }

    // -------------------------------------------------------------- lecture

    pub fn en_lecture(&self, p: &Path) -> bool {
        self.lecture.as_deref() == Some(p)
    }

    /// Lit `p`, ou l'arrête s'il est déjà en lecture.
    pub fn basculer_lecture(&mut self, p: &Path) {
        if self.en_lecture(p) {
            self.lecteur.stop();
            self.lecture = None;
            return;
        }
        if self.enregistrement.is_some() {
            // Le micro capterait le haut-parleur.
            self.info("Pas de lecture pendant l'enregistrement.");
            return;
        }
        match self.lecteur.jouer(p) {
            Ok(()) => {
                self.lecture = Some(p.to_path_buf());
                self.lecture_debut = Some(Instant::now());
            }
            Err(e) => self.erreur(format!("Lecture impossible : {e}")),
        }
    }

    fn surveiller_lecture(&mut self) {
        // Mode capture : la lecture affichée est simulée, aucun pw-play ne tourne.
        if self.capture.is_some() {
            return;
        }
        if self.lecture.is_some() {
            let ecoule = self.lecture_debut.map(|d| d.elapsed()).unwrap_or_default();
            if self.lecteur.en_cours() {
                // Surveillance serrée au début pour repérer un pw-play qui s'arrête aussitôt.
                let pas = if ecoule < Duration::from_millis(400) { 50 } else { 250 };
                self.ctx.request_repaint_after(Duration::from_millis(pas));
            } else {
                self.lecture = None;
                self.lecture_debut = None;
                if ecoule < Duration::from_millis(350) {
                    self.erreur("Lecture impossible (format non pris en charge ?)");
                }
            }
        }
    }

    // -------------------------------------------------------------- jeux de voix

    pub fn rafraichir_jeux(&mut self) {
        self.jeux = voicesets::lister_jeux(&self.cfg.voicesets_dir());
        if self.cfg.jeu_actif.is_empty() {
            if let Some(j) = self.jeux.first() {
                self.cfg.jeu_actif = j.clone();
            }
        }
        self.rafraichir_echantillons();
    }

    pub fn rafraichir_echantillons(&mut self) {
        self.echantillons = if self.cfg.jeu_actif.is_empty() {
            Vec::new()
        } else {
            voicesets::echantillons(&self.cfg.voicesets_dir(), &self.cfg.jeu_actif)
        };
    }

    pub fn choisir_jeu(&mut self, jeu: &str) {
        if self.cfg.jeu_actif == jeu {
            return;
        }
        self.cfg.jeu_actif = jeu.to_string();
        self.dernier_ajout = None;
        self.rafraichir_echantillons();
        self.lancer_diagnostic();
    }

    pub fn creer_jeu(&mut self, nom: &str) -> Result<(), String> {
        let nom = nom.trim();
        if !voicesets::nom_valide(nom) {
            return Err("Nom invalide : utilisez de 1 à 40 lettres sans accent, chiffres, « - » ou « _ ».".into());
        }
        if self.jeux.iter().any(|j| j == nom) {
            return Err(format!("Le jeu « {nom} » existe déjà."));
        }
        voicesets::creer_jeu(&self.cfg.voicesets_dir(), nom)?;
        self.cfg.jeu_actif = nom.to_string();
        self.rafraichir_jeux();
        self.sauver();
        self.succes(format!("Jeu de voix « {nom} » créé. Enregistrez maintenant 3 à 5 échantillons."));
        Ok(())
    }

    pub fn reinitialiser_jeu(&mut self, jeu: &str) {
        match voicesets::reinitialiser_jeu(&self.cfg.voicesets_dir(), jeu, &self.cfg.corbeille_dir()) {
            Ok(n) => self.succes(format!("Jeu « {jeu} » réinitialisé : {n} échantillon(s) déplacé(s) dans la corbeille.")),
            Err(e) => self.erreur(format!("Réinitialisation impossible : {e}")),
        }
        self.arreter_lecture_si_dans(&self.cfg.voicesets_dir().join(jeu));
        self.rafraichir_jeux();
        self.rafraichir_voix();
        self.lancer_diagnostic();
    }

    pub fn supprimer_jeu(&mut self, jeu: &str) {
        self.arreter_lecture_si_dans(&self.cfg.voicesets_dir().join(jeu));
        match voicesets::supprimer_jeu(&self.cfg.voicesets_dir(), jeu, &self.cfg.corbeille_dir()) {
            Ok(()) => {
                self.succes(format!("Jeu « {jeu} » supprimé ; ses échantillons sont dans la corbeille."));
                if self.cfg.jeu_actif == jeu {
                    self.cfg.jeu_actif.clear();
                }
                if self.cfg.voix == voicesets::nom_api(jeu) {
                    self.cfg.voix.clear();
                }
            }
            Err(e) => self.erreur(format!("Suppression impossible : {e}")),
        }
        self.rafraichir_jeux();
        self.rafraichir_voix();
        self.lancer_diagnostic();
        self.sauver();
    }

    fn arreter_lecture_si_dans(&mut self, dossier: &Path) {
        if self.lecture.as_deref().is_some_and(|l| l.starts_with(dossier)) {
            self.lecteur.stop();
            self.lecture = None;
        }
    }

    pub fn retirer_echantillon(&mut self, chemin: &Path) {
        if self.en_lecture(chemin) {
            self.lecteur.stop();
            self.lecture = None;
        }
        match voicesets::retirer_echantillon(chemin, &self.cfg.corbeille_dir()) {
            Ok(_) => {
                let nom = chemin.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.succes(format!("« {nom} » retiré du jeu (déplacé dans la corbeille)."));
            }
            Err(e) => self.erreur(format!("Impossible de retirer l'échantillon : {e}")),
        }
        self.rafraichir_echantillons();
        self.rafraichir_voix();
    }

    pub fn exporter(&mut self, chemin: &Path, nom: &str) -> Result<(), String> {
        let nom = nom.trim();
        if nom.is_empty() {
            return Err("Donnez un nom à la voix.".into());
        }
        let p = voicesets::exporter_voix_simple(chemin, &self.cfg.voices_dir(), nom)?;
        self.succes(format!(
            "Voix simple créée : {}. Elle apparaît dans la liste des voix de la synthèse.",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));
        self.rafraichir_voix();
        Ok(())
    }

    // -------------------------------------------------------------- enregistrement

    /// Nom de l'échantillon tel qu'il sera écrit : `<jeu>-<horodatage>[-<étiquette>]`.
    pub fn nom_echantillon(&self, horodatage: &str) -> String {
        let etiquette = paths::nom_sur(self.etiquette_echantillon.trim());
        if etiquette.is_empty() {
            format!("{}-{horodatage}", self.cfg.jeu_actif)
        } else {
            format!("{}-{horodatage}-{etiquette}", self.cfg.jeu_actif)
        }
    }

    pub fn demarrer_enregistrement(&mut self) {
        if self.enregistrement.is_some() || self.traitement || self.capture.is_some() {
            return;
        }
        let jeu = self.cfg.jeu_actif.clone();
        if !voicesets::nom_valide(&jeu) {
            self.erreur("Choisissez ou créez d'abord un jeu de voix.");
            return;
        }
        let h = paths::horodatage();
        let dir = self.cfg.echantillons_dir();
        let _ = std::fs::create_dir_all(&dir);
        let mut brut = dir.join(format!("{jeu}-{h}-brut.wav"));
        let mut i = 2;
        while brut.exists() {
            brut = dir.join(format!("{jeu}-{h}-{i}-brut.wav"));
            i += 1;
        }
        let nom = self.nom_echantillon(&h);
        if self.lecture.is_some() {
            self.lecteur.stop();
            self.lecture = None;
        }
        let micro = if self.micro_effectif.is_empty() { None } else { Some(self.micro_effectif.as_str()) };
        match recorder::Enregistreur::demarrer(micro, &brut) {
            Ok(e) => {
                self.dernier_ajout = None;
                self.enregistrement = Some(Enregistrement {
                    source: Source::Reel(e),
                    jeu,
                    nom,
                    niveau_db: widgets_min(),
                    pic_db: widgets_min(),
                    pic_t: Instant::now(),
                    mesure_t: Instant::now(),
                });
                self.info("Enregistrement en cours : lisez le texte d'une voix naturelle.");
            }
            Err(e) => self.erreur(format!("Impossible de démarrer l'enregistrement : {e}")),
        }
    }

    /// Enregistrement factice pour les captures d'écran.
    pub fn simuler_enregistrement(&mut self) {
        let h = paths::horodatage();
        self.enregistrement = Some(Enregistrement {
            source: Source::Simule,
            jeu: self.cfg.jeu_actif.clone(),
            nom: self.nom_echantillon(&h),
            niveau_db: -19.0,
            pic_db: -13.0,
            pic_t: Instant::now(),
            mesure_t: Instant::now(),
        });
        self.info("Enregistrement en cours : lisez le texte d'une voix naturelle.");
    }

    /// Arrête l'enregistrement ; `garder` = l'ajouter au jeu (sinon le brut reste seul sur disque).
    pub fn arreter_enregistrement(&mut self, garder: bool) {
        let Some(e) = self.enregistrement.take() else { return };
        let Source::Reel(enr) = e.source else { return };
        if !garder || enr.duree() < 2.0 {
            // pw-record peut mettre quelques secondes à finaliser : jamais sur le thread de l'interface.
            if garder {
                self.erreur("Enregistrement trop court (moins de 2 s) : il n'a pas été ajouté.");
            } else {
                self.info("Enregistrement annulé (le fichier brut reste dans le dossier echantillons).");
            }
            thread::spawn(move || {
                let _ = enr.arreter();
            });
            return;
        }
        self.traitement = true;
        self.info("Nettoyage de l'échantillon (silences, volume)…");
        let dir = self.cfg.voicesets_dir();
        let (jeu, nom) = (e.jeu, e.nom);
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let r = enr
                .arreter()
                .and_then(|brut| audio::lire(&brut))
                .and_then(|wav| voicesets::ajouter_echantillon(&dir, &jeu, &wav, &nom));
            let _ = tx.send(Msg::Echantillon(r));
            ctx.request_repaint();
        });
    }

    /// Mesure du niveau toutes les 50 ms, retombée de 20 dB/s, pic retenu 1 s.
    fn mesurer_niveau(&mut self) {
        let Some(e) = &mut self.enregistrement else { return };
        if let Source::Reel(enr) = &e.source {
            let maintenant = Instant::now();
            let dt = maintenant.duration_since(e.mesure_t).as_secs_f32();
            if dt >= 0.05 {
                let rms = enr.niveau();
                let db = if rms > 0.0 { (20.0 * rms.log10()).max(widgets_min()) } else { widgets_min() };
                e.niveau_db = if db >= e.niveau_db { db } else { (e.niveau_db - 20.0 * dt).max(db) };
                if db >= e.pic_db || e.pic_t.elapsed() > Duration::from_secs(1) {
                    e.pic_db = db;
                    e.pic_t = maintenant;
                }
                e.mesure_t = maintenant;
            }
        }
        self.ctx.request_repaint_after(Duration::from_millis(50));
    }

    // -------------------------------------------------------------- config

    /// Enregistre la configuration (jamais en mode capture).
    pub fn sauver(&mut self) {
        if self.capture.is_some() {
            return;
        }
        if let Err(e) = self.cfg.enregistrer() {
            self.erreur(format!("Configuration non enregistrée : {e}"));
        }
    }

    // -------------------------------------------------------------- glisser-déposer

    fn gerer_depot(&mut self, ctx: &egui::Context) {
        let deposes = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(p) = deposes.iter().find_map(|f| f.path.clone()) {
            if self.enregistrement.is_some() {
                self.erreur("Terminez d'abord l'enregistrement en cours.");
            } else if self.dialogue.is_none() {
                self.ouvrir_texte(&p);
            }
        }
    }
}

fn widgets_min() -> f32 {
    ui::widgets::VU_MIN
}

/// Étiquette d'échantillon tirée du titre d'un texte modèle (« 1. Cours : l'ébullition » → « cours-ebullition »).
pub fn etiquette_modele(titre: &str) -> String {
    let sans_numero = titre.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
    let s = paths::nom_sur(sans_numero).to_lowercase();
    let s: String = s.split(['_', '-']).filter(|x| !x.is_empty() && x.len() > 1).take(3).collect::<Vec<_>>().join("-");
    s
}

fn charger_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory_with_format(include_bytes!("../assets/vocalise-256.png"), image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let taille = [rgba.width() as usize, rgba.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(taille, rgba.as_raw());
    Some(ctx.load_texture("logo-vocalise", ci, egui::TextureOptions::LINEAR))
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        AFFICHE.store(true, Ordering::Relaxed);
        self.traiter_messages();
        self.surveiller_serveur();
        self.surveiller_lecture();
        self.mesurer_niveau();
        self.gerer_depot(ctx);
        if self.generation.is_some() || self.traitement || self.arret_en_cours || matches!(self.etat_serveur(), EtatServeur::Demarrage(_)) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        ui::afficher(self, ctx);

        if let Some(mut c) = self.capture.take() {
            c.avancer(self, ctx);
            self.capture = Some(c);
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.lecteur.stop();
        if let Some(e) = self.enregistrement.take() {
            if let Source::Reel(enr) = e.source {
                let _ = enr.arreter();
            }
        }
        // On n'arrête que le serveur que l'application a elle-même lancé.
        if let Some(mut s) = self.serveur.take() {
            s.arreter();
        }
        self.sauver();
    }
}

/// `p` s'il n'existe pas, sinon `<nom>-2.<ext>`, `<nom>-3.<ext>`…
fn chemin_libre(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    let base = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    (2..).map(|i| dir.join(format!("{base}-{i}.{ext}"))).find(|c| !c.exists()).unwrap_or_else(|| p.to_path_buf())
}

/// Découpe un texte modèle en paragraphes d'une ligne (sans les titres Markdown).
fn paragraphes(corps: &str) -> Vec<String> {
    let sans_titres = corps.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
    sans_titres
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect()
}
