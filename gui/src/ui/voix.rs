//! Écran « Ma voix » : jeu de voix actif, échantillons, studio d'enregistrement.

use super::theme::{self, pal};
use super::widgets::{self, Bouton, Ton};
use crate::app::{App, Dialogue};
use crate::voicesets::{DUREE_MAX_S, MAX_UTILES};
use eframe::egui::{self, vec2, Align, CornerRadius, Frame, Layout, Margin, PopupCloseBehavior, RichText, ScrollArea, Stroke, Ui};
use std::path::PathBuf;

const LARGEUR_LISTE: f32 = 320.0;

pub fn afficher(app: &mut App, ui: &mut Ui) {
    widgets::titre_page(ui, "Ma voix", "Enregistrez des échantillons de votre voix : AllTalk s'en sert pour l'imiter.");
    let large = ui.available_width() >= 800.0;
    if large {
        let h = ui.available_height();
        let gauche = ui.available_width() - LARGEUR_LISTE - theme::ECART;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme::ECART;
            ui.allocate_ui_with_layout(vec2(gauche, h), Layout::top_down(Align::LEFT), |ui| {
                widgets::defiler(ui, ScrollArea::vertical().id_salt("voix_gauche").auto_shrink([false, false]), false, |ui| {
                    carte_studio(app, ui, Some(h));
                });
            });
            ui.allocate_ui_with_layout(vec2(LARGEUR_LISTE, h), Layout::top_down(Align::LEFT), |ui| {
                widgets::defiler(ui, ScrollArea::vertical().id_salt("voix_droite").auto_shrink([false, false]), false, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::ECART;
                    carte_jeu(app, ui);
                    carte_conseils(ui);
                    ui.add_space(4.0);
                });
            });
        });
    } else {
        // Fenêtre étroite : le studio tient dans la hauteur visible (bouton rouge toujours à l'écran).
        let h = ui.available_height();
        widgets::defiler(ui, ScrollArea::vertical().id_salt("voix_page").auto_shrink([false, false]), false, |ui| {
            ui.spacing_mut().item_spacing.y = theme::ECART;
            carte_studio(app, ui, Some(h));
            carte_jeu(app, ui);
            carte_conseils(ui);
        });
    }
}

fn carte_conseils(ui: &mut Ui) {
    let p = pal(ui);
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        widgets::titre_carte(ui, "Conseils");
        for c in [
            "Pièce calme, porte fermée, sans écho.",
            "Micro à 15–20 cm, toujours à la même distance.",
            "Voix naturelle et posée, comme en classe.",
            "20 à 30 s par échantillon : au-delà, la fin est ignorée.",
            "Variez les textes : explication, récit, questions.",
        ] {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new("•").size(14.0).color(p.accent_texte));
                ui.add(egui::Label::new(RichText::new(c).size(13.5).color(p.texte_2)).wrap());
            });
        }
    });
}

// ------------------------------------------------------------------ jeu de voix et échantillons

enum Action {
    Choisir(String),
    Nouveau,
    Reinitialiser,
    Supprimer,
    OuvrirDossier,
    Ecouter(PathBuf),
    Exporter(PathBuf, String),
    Retirer(PathBuf),
}

fn carte_jeu(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let mut action = None;
    let verrou = app.enregistrement.is_some() || app.traitement;
    let n = app.echantillons.len();
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            widgets::titre_carte(ui, "Jeu de voix");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let id_menu = ui.make_persistent_id("menu_jeu");
                let r = widgets::bouton_menu(ui, "Autres actions sur ce jeu");
                if r.clicked() && !verrou {
                    ui.memory_mut(|m| m.toggle_popup(id_menu));
                }
                egui::popup_below_widget(ui, id_menu, &r, PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(250.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if widgets::element_menu(ui, "↗", "Ouvrir le dossier du jeu", false).clicked() {
                        action = Some(Action::OuvrirDossier);
                    }
                    ui.separator();
                    if widgets::element_menu(ui, "↺", "Réinitialiser…", false).clicked() {
                        action = Some(Action::Reinitialiser);
                    }
                    if widgets::element_menu(ui, "✕", "Supprimer le jeu…", true).clicked() {
                        action = Some(Action::Supprimer);
                    }
                });
                if Bouton::secondaire("Nouveau jeu…").petit().actif(!verrou).infobulle("Créer un nouveau jeu de voix, vide").ui(ui).clicked() {
                    action = Some(Action::Nouveau);
                }
            });
        });

        let mut liste = app.jeux.clone();
        if !app.cfg.jeu_actif.is_empty() && !liste.contains(&app.cfg.jeu_actif) {
            liste.push(app.cfg.jeu_actif.clone());
        }
        let largeur = ui.available_width();
        ui.add_enabled_ui(!verrou, |ui| {
            egui::ComboBox::from_id_salt("jeu_actif")
                .width(largeur)
                .selected_text(
                    RichText::new(if app.cfg.jeu_actif.is_empty() { "Aucun jeu" } else { app.cfg.jeu_actif.as_str() }).font(theme::police_forte(15.0)),
                )
                .show_ui(ui, |ui| {
                    for j in &liste {
                        if ui.selectable_label(*j == app.cfg.jeu_actif, j).clicked() {
                            action = Some(Action::Choisir(j.clone()));
                        }
                    }
                })
                .response
                .on_hover_text("Jeu dans lequel les nouveaux échantillons sont ajoutés")
                .on_disabled_hover_text("Terminez d'abord l'enregistrement en cours.");
        });

        // Jauge « N / 5 ».
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            widgets::jauge(ui, n, MAX_UTILES).on_hover_text(format!(
                "AllTalk exploite jusqu'à {MAX_UTILES} échantillons\nde {} s au plus pour calculer l'empreinte de votre voix.",
                DUREE_MAX_S as u32
            ));
            if n > MAX_UTILES {
                widgets::badge(ui, &format!("+{}", n - MAX_UTILES), p.avert_doux, p.avert);
            }
        });
        let utiles = n.min(MAX_UTILES);
        ui.label(RichText::new(format!("{utiles} / {MAX_UTILES} échantillons exploités")).font(theme::police_forte(15.0)).color(p.texte));
        let total: f32 = app.echantillons.iter().map(|e| e.duree.min(DUREE_MAX_S)).sum();
        let detail = match n {
            0 => "Enregistrez de 3 à 5 lectures d'environ 30 s.".to_string(),
            1..=2 => format!("{} de voix. Ajoutez-en encore {} à {} pour une voix plus fidèle.", widgets::duree_lisible(total), 3 - n, MAX_UTILES - n),
            _ => format!("{} de voix au total.", widgets::duree_lisible(total)),
        };
        widgets::secondaire(ui, detail);
        if n > MAX_UTILES {
            widgets::encadre(
                ui,
                Ton::Avert,
                &format!("{n} échantillons : AllTalk en tire {MAX_UTILES} au hasard à chaque génération. Retirez les moins réussis."),
            );
        }

        ui.add_space(2.0);
        ui.separator();
        if app.echantillons.is_empty() {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Aucun échantillon").font(theme::police_forte(15.5)).color(p.texte));
                ui.add(egui::Label::new(RichText::new("Votre premier enregistrement apparaîtra ici.").size(13.0).color(p.texte_2)));
            });
            ui.add_space(8.0);
        }
        ui.spacing_mut().item_spacing.y = 6.0;
        for (i, e) in app.echantillons.clone().iter().enumerate() {
            let nouveau = app.dernier_ajout.as_ref() == Some(&e.chemin);
            let fond = if nouveau { p.succes_doux } else { p.creux };
            Frame::new().fill(fond).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if widgets::bouton_lecture(ui, app.en_lecture(&e.chemin), app.enregistrement.is_none()).clicked() {
                        action = Some(Action::Ecouter(e.chemin.clone()));
                    }
                    let reste = ui.available_width() - 42.0;
                    ui.allocate_ui_with_layout(vec2(reste, 36.0), Layout::top_down(Align::LEFT), |ui| {
                        ui.set_min_width(reste);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add(egui::Label::new(RichText::new(&e.nom).size(14.0).color(p.texte)).truncate())
                            .on_hover_text(e.chemin.display().to_string());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(RichText::new(widgets::duree_lisible(e.duree)).size(12.5).color(p.texte_2));
                            if nouveau {
                                ui.label(RichText::new("· nouveau").size(12.5).color(p.succes));
                            } else if e.duree < 6.0 {
                                ui.label(RichText::new("· un peu court").size(12.5).color(p.avert));
                            }
                        });
                    });
                    let id_menu = ui.make_persistent_id(("menu_ech", i));
                    let r = widgets::bouton_menu(ui, "Actions sur cet échantillon");
                    if r.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(id_menu));
                    }
                    egui::popup_below_widget(ui, id_menu, &r, PopupCloseBehavior::CloseOnClick, |ui| {
                        ui.set_min_width(280.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if widgets::element_menu(ui, "⇪", "Exporter en voix simple…", false).clicked() {
                            action = Some(Action::Exporter(e.chemin.clone(), e.nom.clone()));
                        }
                        if widgets::element_menu(ui, "✕", "Retirer du jeu (vers la corbeille)", false).clicked() {
                            action = Some(Action::Retirer(e.chemin.clone()));
                        }
                    });
                });
            });
        }
    });

    match action {
        Some(Action::Choisir(j)) => app.choisir_jeu(&j),
        Some(Action::Nouveau) => app.dialogue = Some(Dialogue::NouveauJeu { nom: String::new(), erreur: None }),
        Some(Action::Reinitialiser) if !app.cfg.jeu_actif.is_empty() => {
            app.dialogue = Some(Dialogue::Reinitialiser { jeu: app.cfg.jeu_actif.clone(), n })
        }
        Some(Action::Supprimer) if !app.cfg.jeu_actif.is_empty() => {
            app.dialogue = Some(Dialogue::SupprimerJeu { jeu: app.cfg.jeu_actif.clone(), n })
        }
        Some(Action::OuvrirDossier) => {
            let d = app.cfg.voicesets_dir().join(&app.cfg.jeu_actif);
            let d = if d.is_dir() { d } else { app.cfg.voicesets_dir() };
            crate::paths::ouvrir(&d);
        }
        Some(Action::Ecouter(c)) => app.basculer_lecture(&c),
        Some(Action::Exporter(c, nom)) => {
            let nom = crate::paths::nom_sur(&nom);
            app.dialogue = Some(Dialogue::Exporter { chemin: c, nom, erreur: None });
        }
        Some(Action::Retirer(c)) => app.retirer_echantillon(&c),
        _ => {}
    }
}

// ------------------------------------------------------------------ studio d'enregistrement

/// Hauteur fixe du studio hors texte modèle (titre, micro, commandes, marges).
const STUDIO_FIXE: f32 = 302.0;

fn carte_studio(app: &mut App, ui: &mut Ui, hauteur: Option<f32>) {
    let p = pal(ui);
    let en_cours = app.enregistrement.is_some();
    let verrou = en_cours || app.traitement;
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            ui.set_min_height(theme::HAUTEUR_PETIT);
            widgets::titre_carte(ui, "Enregistrer un échantillon");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let titre = app.modeles.get(app.modele_idx).map(|m| m.0.clone()).unwrap_or_default();
                let mut choix = app.modele_idx;
                let l = (ui.available_width() - 16.0).clamp(140.0, 250.0);
                egui::ComboBox::from_id_salt("modele")
                    .width(l)
                    .selected_text(RichText::new(titre).size(14.5))
                    .show_ui(ui, |ui| {
                        ui.set_min_width(260.0);
                        for (i, (t, _)) in app.modeles.iter().enumerate() {
                            ui.selectable_value(&mut choix, i, t);
                        }
                    })
                    .response
                    .on_hover_text("Texte modèle à lire à voix haute.\nVarier les textes enrichit votre voix.");
                if choix != app.modele_idx {
                    app.modele_idx = choix;
                    if !en_cours {
                        if let Some((t, _)) = app.modeles.get(choix) {
                            app.etiquette_echantillon = crate::app::etiquette_modele(t);
                        }
                    }
                }
            });
        });

        // Le texte modèle, en grand, dans une zone défilante.
        let h_texte = match hauteur {
            Some(h) => (h - STUDIO_FIXE).max(110.0),
            None => 240.0,
        };
        let paragraphes = app.modeles_paragraphes.get(app.modele_idx).cloned().unwrap_or_default();
        Frame::new()
            .fill(p.creux)
            .stroke(Stroke::new(if en_cours { 1.5 } else { 1.0 }, if en_cours { p.rec } else { p.bord }))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(20, 14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::defiler(
                    ui,
                    ScrollArea::vertical()
                        .id_salt("modele_defilant")
                        .max_height(h_texte)
                        .min_scrolled_height(h_texte)
                        .auto_shrink([false, false]),
                    true,
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 14.0;
                        for paragraphe in paragraphes {
                            ui.add(egui::Label::new(RichText::new(paragraphe).size(theme::TAILLE_LECTURE).color(p.texte)).wrap());
                        }
                    });
            });

        // Micro et étiquette.
        ui.add_enabled_ui(!verrou, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let l_micro = (ui.available_width() * 0.5).clamp(160.0, 260.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    widgets::etiquette(ui, "Micro");
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let nom_micro = app.nom_micro(&app.micro_effectif);
                        let mut choix: Option<String> = None;
                        egui::ComboBox::from_id_salt("micro")
                            .width(l_micro - 40.0)
                            .selected_text(RichText::new(nom_micro).size(14.5))
                            .show_ui(ui, |ui| {
                                ui.set_min_width(320.0);
                                if ui
                                    .selectable_label(app.cfg.micro.is_empty(), "Automatique (micro DJI s'il est branché)")
                                    .clicked()
                                {
                                    choix = Some(String::new());
                                }
                                for m in &app.micros {
                                    if ui.selectable_label(app.cfg.micro == m.id, &m.nom).on_hover_text(&m.id).clicked() {
                                        choix = Some(m.id.clone());
                                    }
                                }
                            });
                        if let Some(id) = choix {
                            app.choisir_micro(&id);
                        }
                        if widgets::bouton_icone(ui, "↻", "Actualiser la liste des micros", false).clicked() {
                            app.micros = crate::recorder::micros();
                            app.resoudre_micro();
                        }
                    });
                });
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    widgets::etiquette(ui, "Étiquette (facultative)");
                    widgets::champ(ui, &mut app.etiquette_echantillon, "ex. cours-ebullition", ui.available_width())
                        .on_hover_text("Ajoutée au nom de l'échantillon, après le jeu et l'horodatage,\npour le reconnaître dans la liste.");
                });
            });
        });

        if app.micro_habituel_absent() {
            let actuel = app.nom_micro(&app.micro_effectif);
            widgets::encadre(ui, Ton::Avert, &format!("Micro habituel débranché, utilisation de : {actuel}"));
        }

        ui.separator();
        commandes(app, ui);
    });
}

fn commandes(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let t = ui.input(|i| i.time);
    let en_cours = app.enregistrement.is_some();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        let r = widgets::bouton_enregistrer(ui, en_cours, !app.traitement, t);
        if r.clicked() {
            if en_cours {
                app.arreter_enregistrement(true);
            } else {
                app.demarrer_enregistrement();
            }
        }
        let l = ui.available_width();
        ui.allocate_ui_with_layout(vec2(l, 92.0), Layout::top_down(Align::LEFT), |ui| {
            ui.set_width(l);
            ui.spacing_mut().item_spacing.y = 6.0;
            let largeur_vu = (l - 120.0).max(140.0);
            if let Some(e) = &app.enregistrement {
                let d = e.duree();
                let (db, pic) = (e.niveau_db, e.pic_db);
                let trop_long = d > DUREE_MAX_S;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.set_min_height(34.0);
                    widgets::badge(ui, "● ENREGISTREMENT", p.danger_doux, p.danger_texte);
                    ui.label(RichText::new(widgets::chrono(d)).font(theme::police_forte(26.0)).color(if trop_long { p.avert } else { p.texte }));
                    let reste = ui.available_width() - 96.0;
                    if reste > 60.0 {
                        ui.allocate_ui_with_layout(vec2(reste, 30.0), Layout::left_to_right(Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(if trop_long { "au-delà de 30 s, la fin sera coupée" } else { "idéal : 20 à 30 s" })
                                        .size(13.0)
                                        .color(if trop_long { p.avert } else { p.texte_2 }),
                                )
                                .truncate(),
                            );
                        });
                    }
                    if Bouton::discret("Annuler").petit().infobulle("Arrêter sans ajouter l'échantillon au jeu").ui(ui).clicked() {
                        app.arreter_enregistrement(false);
                    }
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    widgets::vumetre(ui, db, pic, largeur_vu);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let (texte, couleur) = appreciation(pic, p);
                        ui.label(RichText::new(texte).font(theme::police_forte(14.5)).color(couleur));
                        ui.label(RichText::new(format!("{} dB", widgets::decimal(db.round(), 0).replace('-', "−"))).size(13.0).color(p.texte_2));
                    });
                });
            } else if app.traitement {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    widgets::sablier(ui, 18.0, p.accent_texte);
                    ui.label(RichText::new("Nettoyage de l'échantillon…").font(theme::police_forte(16.0)).color(p.texte));
                });
                widgets::secondaire(ui, "Silences coupés, volume ajusté, durée limitée à 30 s.");
            } else {
                ui.add_space(8.0);
                ui.label(RichText::new("Prêt à enregistrer").font(theme::police_forte(17.0)).color(p.texte));
                ui.add(
                    egui::Label::new(RichText::new("Cliquez sur le bouton rouge, puis lisez le texte ci-dessus à voix haute.").size(13.5).color(p.texte_2))
                        .wrap(),
                );
                let nom = app.nom_echantillon(&crate::paths::horodatage());
                ui.add(
                    egui::Label::new(RichText::new(format!("Jeu « {} » · {nom}", app.cfg.jeu_actif)).size(13.0).color(p.texte_2)).truncate(),
                )
                .on_hover_text("Le nouvel échantillon sera ajouté à ce jeu, sous ce nom.");
            }
        });
    });
}

fn appreciation(pic: f32, p: &theme::Palette) -> (&'static str, egui::Color32) {
    if pic > widgets::VU_ROUGE {
        ("Trop fort !", p.danger_texte)
    } else if pic > widgets::VU_ORANGE {
        ("Fort", p.avert)
    } else if pic > -40.0 {
        ("Bon niveau", p.succes)
    } else {
        ("Trop faible", p.texte_2)
    }
}
