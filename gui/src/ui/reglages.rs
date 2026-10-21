//! Écran « Réglages » : serveur AllTalk, dossiers, langue et format, configuration.

use super::theme::{self, pal};
use super::widgets::{self, Bouton, Ton};
use crate::alltalk;
use crate::app::{App, EtatServeur};
use crate::diagnostic::Gravite;
use crate::maj;
use crate::paths;
use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui};
use std::path::{Path, PathBuf};

/// Langues prises en charge par XTTS.
const LANGUES: &[(&str, &str)] = &[
    ("fr", "Français"),
    ("en", "Anglais"),
    ("de", "Allemand"),
    ("es", "Espagnol"),
    ("it", "Italien"),
    ("pt", "Portugais"),
    ("nl", "Néerlandais"),
    ("pl", "Polonais"),
    ("cs", "Tchèque"),
    ("hu", "Hongrois"),
    ("ru", "Russe"),
    ("tr", "Turc"),
    ("ar", "Arabe"),
    ("zh-cn", "Chinois"),
    ("ja", "Japonais"),
    ("ko", "Coréen"),
    ("hi", "Hindi"),
];

pub fn afficher(app: &mut App, ui: &mut Ui) {
    widgets::titre_page(ui, "Réglages", "Serveur de synthèse, dossiers de travail et préférences.");
    widgets::defiler(ui, ScrollArea::vertical().id_salt("reglages").auto_shrink([false, false]), false, |ui| {
        ui.set_max_width(ui.available_width().min(820.0));
        ui.spacing_mut().item_spacing.y = theme::ECART;
        carte_diagnostic(app, ui);
        carte_serveur(app, ui);
        carte_dossiers(app, ui);
        carte_preferences(app, ui);
        carte_mises_a_jour(app, ui);
        carte_config(app, ui);
        ui.add_space(4.0);
    });
}

fn carte_serveur(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let etat = app.etat_serveur();
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        widgets::titre_carte(ui, "Serveur AllTalk");
        let (couleur, titre, detail) = match &etat {
            EtatServeur::Pret { lance_par_nous: true } => (
                p.succes,
                "Prêt".to_string(),
                format!(
                    "Lancé par Vocalise (PID {}). Il sera arrêté à la fermeture de Vocalise.",
                    app.serveur.as_ref().map(|s| s.pid()).unwrap_or_default()
                ),
            ),
            EtatServeur::Pret { lance_par_nous: false } => {
                (p.succes, "Prêt".to_string(), "Lancé en dehors de Vocalise : il restera actif à la fermeture.".to_string())
            }
            EtatServeur::Demarrage(d) => (
                p.accent_texte,
                format!("Démarrage… {}", widgets::chrono(d.as_secs_f32())),
                format!(
                    "PID {} · le chargement du modèle XTTS prend environ une minute.",
                    app.serveur.as_ref().map(|s| s.pid()).unwrap_or_default()
                ),
            ),
            EtatServeur::Arret => (p.texte_2, "Arrêt en cours…".to_string(), "Fermeture propre du serveur (jusqu'à 15 s).".to_string()),
            EtatServeur::Verification => (p.texte_2, "Vérification…".to_string(), "Recherche du serveur sur cet ordinateur.".to_string()),
            EtatServeur::Arrete => (p.avert, "Arrêté".to_string(), "Démarrez-le pour pouvoir générer de l'audio.".to_string()),
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            widgets::pastille(ui, couleur, 6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(RichText::new(titre).font(theme::police_forte(17.0)).color(p.texte));
                ui.label(RichText::new(detail).size(13.5).color(p.texte_2));
            });
        });
        ui.label(RichText::new(format!("Adresse de l'API : {}", alltalk::API)).size(13.0).color(p.texte_2));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            match etat {
                EtatServeur::Arrete => {
                    let script_ok = app.cfg.start_script().exists();
                    if Bouton::principal("Démarrer AllTalk")
                        .icone("▶")
                        .actif(script_ok)
                        .raison("start_alltalk.sh introuvable : vérifiez le dossier d'AllTalk ci-dessous.")
                        .ui(ui)
                        .clicked()
                    {
                        app.demarrer_serveur();
                    }
                }
                EtatServeur::Pret { lance_par_nous: true } | EtatServeur::Demarrage(_) => {
                    if Bouton::secondaire("Arrêter AllTalk").icone("■").ui(ui).clicked() {
                        app.arreter_serveur();
                    }
                }
                _ => {}
            }
            if Bouton::secondaire("Ouvrir l'interface web")
                .icone("🌐")
                .actif(app.serveur_pret())
                .infobulle(alltalk::WEB)
                .raison("Disponible quand AllTalk est prêt.")
                .ui(ui)
                .clicked()
            {
                paths::ouvrir_url(alltalk::WEB);
            }
            let journal = app.cfg.journal();
            if Bouton::discret("Voir le journal")
                .actif(journal.exists())
                .infobulle("Messages du serveur lancé par Vocalise (utile en cas de problème)")
                .raison("Aucun journal pour l'instant : il est créé au premier démarrage par Vocalise.")
                .ui(ui)
                .clicked()
            {
                paths::ouvrir(&journal);
            }
        });
    });
}

enum ActionDossier {
    Choisir,
    Ouvrir,
}

/// Ligne « libellé / chemin / boutons ».
fn ligne_dossier(ui: &mut Ui, libelle: &str, aide: &str, chemin: &Path, choisir: bool) -> Option<ActionDossier> {
    let p = pal(ui);
    let mut action = None;
    widgets::etiquette(ui, libelle);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 36.0), Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if choisir && Bouton::secondaire("Choisir…").petit().infobulle("Choisir un autre dossier").ui(ui).clicked() {
            action = Some(ActionDossier::Choisir);
        }
        if Bouton::secondaire("Ouvrir").petit().actif(chemin.exists()).infobulle("Ouvrir dans le gestionnaire de fichiers").ui(ui).clicked() {
            action = Some(ActionDossier::Ouvrir);
        }
        let l = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(l, 36.0), Layout::left_to_right(Align::Center), |ui| {
            Frame::new()
                .fill(p.creux)
                .stroke(Stroke::new(1.0, p.bord))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(l - 22.0);
                    ui.add(egui::Label::new(RichText::new(chemin.display().to_string()).size(14.0).color(p.texte)).truncate())
                        .on_hover_text(chemin.display().to_string());
                });
        });
    });
    if !aide.is_empty() {
        widgets::secondaire(ui, aide);
    }
    action
}

fn choisir_dossier(titre: &str, depart: &Path) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(titre);
    if depart.is_dir() {
        d = d.set_directory(depart);
    }
    d.pick_folder()
}

fn carte_dossiers(app: &mut App, ui: &mut Ui) {
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        widgets::titre_carte(ui, "Dossiers");
        ui.add_space(2.0);

        let alltalk_dir = app.cfg.alltalk_dir.clone();
        match ligne_dossier(ui, "Dossier d'AllTalk", "Contient start_alltalk.sh et le dossier voices/.", &alltalk_dir, true) {
            Some(ActionDossier::Choisir) => {
                if let Some(d) = choisir_dossier("Dossier d'installation d'AllTalk", &alltalk_dir) {
                    app.cfg.alltalk_dir = d;
                    app.sauver();
                    app.rafraichir_jeux();
                    app.rafraichir_voix();
                    app.xtts_configure = false;
                    app.lancer_diagnostic();
                    app.succes("Dossier d'AllTalk modifié.");
                }
            }
            Some(ActionDossier::Ouvrir) => paths::ouvrir(&alltalk_dir),
            None => {}
        }
        if !app.cfg.start_script().exists() {
            widgets::encadre(ui, Ton::Avert, "start_alltalk.sh est introuvable dans ce dossier : Vocalise ne pourra pas démarrer AllTalk.");
        }
        ui.add_space(6.0);

        let sorties = app.cfg.sorties_dir.clone();
        match ligne_dossier(ui, "Fichiers générés", "Les fichiers audio créés par la synthèse y sont enregistrés.", &sorties, true) {
            Some(ActionDossier::Choisir) => {
                if let Some(d) = choisir_dossier("Dossier des fichiers générés", &sorties) {
                    app.cfg.sorties_dir = d;
                    app.sauver();
                    app.succes("Dossier des fichiers générés modifié.");
                }
            }
            Some(ActionDossier::Ouvrir) => paths::ouvrir(&sorties),
            None => {}
        }
        ui.add_space(6.0);

        let bruts = app.cfg.echantillons_dir();
        if let Some(ActionDossier::Ouvrir) = ligne_dossier(
            ui,
            "Enregistrements bruts et corbeille",
            "Chaque prise est gardée telle quelle ; les échantillons retirés vont dans corbeille/.",
            &bruts,
            false,
        ) {
            paths::ouvrir(&bruts);
        }
    });
}

fn carte_preferences(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        widgets::titre_carte(ui, "Synthèse");
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.vertical(|ui| {
                widgets::etiquette(ui, "Langue du texte");
                let actuelle = LANGUES
                    .iter()
                    .find(|(c, _)| *c == app.cfg.langue)
                    .map(|(c, n)| format!("{n} ({c})"))
                    .unwrap_or_else(|| app.cfg.langue.clone());
                let mut choix = app.cfg.langue.clone();
                egui::ComboBox::from_id_salt("langue").width(220.0).selected_text(RichText::new(actuelle).size(15.0)).show_ui(ui, |ui| {
                    for (c, n) in LANGUES {
                        ui.selectable_value(&mut choix, c.to_string(), format!("{n} ({c})"));
                    }
                });
                if choix != app.cfg.langue {
                    app.cfg.langue = choix;
                    app.sauver();
                }
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                widgets::etiquette(ui, "Format par défaut");
                super::synthese::selecteur_format(app, ui, 110.0, "format_reglages");
            });
        });
        if app.ffmpeg {
            ui.label(RichText::new(format!("Format actuel : {}.", app.format_effectif().libelle())).size(13.0).color(p.texte_2));
        } else {
            widgets::encadre(ui, Ton::Info, "Installer ffmpeg pour FLAC, MP3, OGG (commande : sudo apt install ffmpeg). En attendant, les fichiers sont en WAV.");
        }
    });
}

fn carte_config(_app: &mut App, ui: &mut Ui) {
    let fichier = paths::Config::fichier();
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        widgets::titre_carte(ui, "Configuration");
        let dossier = fichier.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Some(ActionDossier::Ouvrir) =
            ligne_dossier(ui, "Fichier de configuration", "Enregistré automatiquement à chaque modification et à la fermeture.", &fichier, false)
        {
            paths::ouvrir(&dossier);
        }
    });
}

// ------------------------------------------------------------------ mises à jour

fn carte_mises_a_jour(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let mut verifier = false;
    let mut lien: Option<String> = None;
    if app.defiler_vers_maj {
        ui.scroll_to_cursor(Some(Align::Min));
    }
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            widgets::titre_carte(ui, "Mises à jour");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if app.maj_en_cours {
                    ui.horizontal(|ui| {
                        widgets::sablier(ui, 14.0, p.texte_2);
                        widgets::secondaire(ui, "Vérification…");
                    });
                } else if Bouton::secondaire("Vérifier maintenant").petit().ui(ui).clicked() {
                    verifier = true;
                }
            });
        });
        widgets::secondaire(ui, format!("Version installée : {}", maj::Version::courante()));
        match &app.maj {
            Some(Ok(Some(pub_)) ) => {
                let (fond, couleur) = (p.accent_doux, p.accent);
                Frame::new().fill(fond).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let quand = pub_.date.as_deref().map(|d| format!(", publiée le {}", date_lisible(d))).unwrap_or_default();
                    ui.label(RichText::new(format!("Version {} disponible{quand}.", pub_.version)).font(theme::police_forte(14.0)).color(couleur));
                    widgets::secondaire(ui, "Téléchargez le paquet puis installez-le : sudo apt install ./vocalise_<version>_amd64.deb");
                    ui.horizontal(|ui| {
                        if let Some(paquet) = &pub_.paquet {
                            if Bouton::principal("Télécharger le paquet .deb").petit().ui(ui).clicked() {
                                lien = Some(paquet.clone());
                            }
                        }
                        if Bouton::secondaire("Voir les nouveautés").petit().ui(ui).clicked() {
                            lien = Some(pub_.page.clone());
                        }
                    });
                });
            }
            Some(Ok(None)) => widgets::encadre(ui, Ton::Succes, "Vous avez la dernière version."),
            Some(Err(e)) => widgets::encadre(ui, Ton::Avert, &format!("Vérification impossible : {e}.")),
            None => {
                let quand = app.cfg.maj_verifiee_le.as_deref().map(|d| format!("Dernière vérification le {}.", date_lisible(d)));
                widgets::secondaire(ui, quand.unwrap_or_else(|| "Pas encore vérifié.".into()));
            }
        }
        ui.horizontal(|ui| {
            if ui.checkbox(&mut app.cfg.verifier_maj, "Vérifier automatiquement au démarrage (une fois par jour)").changed() {
                app.sauver();
            }
        });
        ui.horizontal(|ui| {
            widgets::secondaire(ui, "Les versions sont publiées sur GitHub :");
            if ui.link(RichText::new(maj::page_publications()).size(theme::TAILLE_SECONDAIRE).color(p.accent)).clicked() {
                lien = Some(maj::page_publications());
            }
        });
    });
    if verifier {
        app.verifier_maj(true);
    }
    if let Some(l) = lien {
        paths::ouvrir_url(&l);
    }
}

/// « 2026-11-02 » → « 2 novembre 2026 » ; texte inchangé s'il n'a pas cette forme.
fn date_lisible(d: &str) -> String {
    const MOIS: [&str; 12] = [
        "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre",
    ];
    let parties: Vec<u32> = d.split('-').filter_map(|x| x.parse().ok()).collect();
    match parties.as_slice() {
        [a, m, j] if (1..=12).contains(m) => {
            let jour = if *j == 1 { "1er".to_string() } else { j.to_string() };
            format!("{jour} {} {a}", MOIS[(*m - 1) as usize])
        }
        _ => d.to_string(),
    }
}

// ------------------------------------------------------------------ diagnostic

enum ActionDiag {
    Reverifier,
    Lien(&'static str),
    Copier(String),
    ConfigurerXtts,
    Redemarrer,
}

fn carte_diagnostic(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let mut action = None;
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            ui.set_min_height(theme::HAUTEUR_PETIT);
            widgets::titre_carte(ui, "Diagnostic");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if Bouton::secondaire("Revérifier")
                    .petit()
                    .actif(!app.diagnostic_en_cours)
                    .infobulle("Relancer toutes les vérifications")
                    .ui(ui)
                    .clicked()
                {
                    action = Some(ActionDiag::Reverifier);
                }
                if app.diagnostic_en_cours {
                    widgets::sablier(ui, 16.0, p.accent_texte);
                }
            });
        });
        let Some(mut liste) = app.diagnostic.clone() else {
            widgets::secondaire(ui, "Vérification des composants en cours…");
            return;
        };
        if liste.is_empty() {
            widgets::secondaire(ui, "Le diagnostic n'a pas pu être établi.");
            return;
        }
        let tout_ok = liste.iter().all(|v| v.ok);
        if tout_ok {
            widgets::encadre(ui, Ton::Succes, "Tout est en place : Vocalise dispose de tous ses composants.");
            let libelle = if app.diagnostic_detail { "Masquer le détail" } else { "Afficher le détail" };
            if Bouton::discret(libelle).petit().ui(ui).clicked() {
                app.diagnostic_detail = !app.diagnostic_detail;
            }
            if !app.diagnostic_detail {
                return;
            }
        }
        // Les problèmes d'abord (indispensables, puis fonctions limitées), les composants en ordre ensuite.
        liste.sort_by_key(|v| (v.ok, v.gravite != Gravite::Bloquant));
        let dossier_ok = app.cfg.start_script().exists();
        for (i, v) in liste.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            let couleur = if v.ok {
                p.succes
            } else if v.gravite == Gravite::Bloquant {
                p.danger_texte
            } else {
                p.avert
            };
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                widgets::pastille(ui, couleur, 5.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&v.composant).font(theme::police_forte(15.0)).color(p.texte));
                        let (etat, fond) = if v.ok {
                            ("OK", p.succes_doux)
                        } else if v.gravite == Gravite::Bloquant {
                            ("Indispensable", p.danger_doux)
                        } else {
                            ("Fonction limitée", p.avert_doux)
                        };
                        widgets::badge(ui, etat, fond, couleur);
                    });
                    ui.add(egui::Label::new(RichText::new(&v.detail).size(13.5).color(p.texte_2)).wrap());
                    if v.ok {
                        return;
                    }
                    let est_xtts = v.composant.contains("XTTS");
                    if (est_xtts && dossier_ok) || v.lien.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if est_xtts && dossier_ok {
                            let r = Bouton::principal("Configurer XTTS")
                                .petit()
                                .infobulle("Règle AllTalk pour qu'il charge le moteur XTTS au démarrage")
                                .ui(ui);
                            if r.clicked() {
                                action = Some(ActionDiag::ConfigurerXtts);
                            }
                        }
                        if let Some(lien) = v.lien {
                            if Bouton::secondaire("Site officiel").petit().infobulle(lien).ui(ui).clicked() {
                                action = Some(ActionDiag::Lien(lien));
                            }
                        }
                    });
                    }
                    if let Some(cmd) = &v.commande {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let l = (ui.available_width() - 110.0).max(120.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut cmd.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(l)
                                    .margin(Margin::symmetric(10, 8)),
                            )
                            .on_hover_text("Commande à coller dans un terminal");
                            if Bouton::secondaire("Copier").petit().infobulle("Copier la commande dans le presse-papiers").ui(ui).clicked() {
                                action = Some(ActionDiag::Copier(cmd.clone()));
                            }
                        });
                    }
                });
            });
        }
        if app.xtts_configure {
            ui.add_space(2.0);
            widgets::encadre(ui, Ton::Succes, "Moteur configuré : redémarrez AllTalk pour qu'il charge XTTS.");
            match app.etat_serveur() {
                EtatServeur::Pret { lance_par_nous: true } | EtatServeur::Demarrage(_) => {
                    if Bouton::principal("Redémarrer AllTalk").icone("↻").ui(ui).clicked() {
                        action = Some(ActionDiag::Redemarrer);
                    }
                }
                EtatServeur::Arrete => {
                    if Bouton::principal("Démarrer AllTalk").icone("▶").ui(ui).clicked() {
                        action = Some(ActionDiag::Redemarrer);
                    }
                }
                EtatServeur::Pret { lance_par_nous: false } => {
                    widgets::secondaire(ui, "AllTalk a été lancé en dehors de Vocalise : arrêtez-le, puis relancez-le.");
                }
                _ => {}
            }
        }
    });
    match action {
        Some(ActionDiag::Reverifier) => app.lancer_diagnostic(),
        Some(ActionDiag::Lien(l)) => paths::ouvrir_url(l),
        Some(ActionDiag::Copier(c)) => {
            ui.ctx().copy_text(c);
            app.succes("Commande copiée : collez-la dans un terminal (Ctrl+Maj+V).");
        }
        Some(ActionDiag::ConfigurerXtts) => app.configurer_xtts(),
        Some(ActionDiag::Redemarrer) => {
            app.xtts_configure = false;
            app.redemarrer_serveur();
        }
        None => {}
    }
}
