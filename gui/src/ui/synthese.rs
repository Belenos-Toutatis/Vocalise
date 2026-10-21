//! Écran « Synthèse » : texte, fichier de sortie et bouton Générer à gauche ; voix, réglages et
//! historique de la session à droite.

use super::theme::{self, pal};
use super::widgets::{self, Bouton, Ton};
use crate::app::{App, EtatServeur, Phase};
use crate::audio::Format;
use eframe::egui::{self, vec2, Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui};

const LARGEUR_DROITE: f32 = 310.0;
const LARGEUR_GENERER: f32 = 190.0;
const LARGEUR_FORMAT: f32 = 92.0;

pub fn afficher(app: &mut App, ui: &mut Ui) {
    widgets::titre_page(ui, "Synthèse vocale", "Écrivez ou ouvrez un texte, choisissez la voix, puis générez le fichier audio.");
    app.maj_stats();

    // Ctrl+Entrée : générer.
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) && app.peut_generer() {
        app.generer();
    }

    let large = ui.available_width() >= 780.0;
    if large {
        let h = ui.available_height();
        let gauche = ui.available_width() - LARGEUR_DROITE - theme::ECART;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme::ECART;
            ui.allocate_ui_with_layout(vec2(gauche, h), Layout::top_down(Align::LEFT), |ui| {
                carte_texte(app, ui, Some(h));
            });
            ui.allocate_ui_with_layout(vec2(LARGEUR_DROITE, h), Layout::top_down(Align::LEFT), |ui| {
                widgets::defiler(ui, ScrollArea::vertical().id_salt("synthese_droite").auto_shrink([false, false]), false, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::ECART;
                    carte_voix(app, ui);
                    carte_historique(app, ui);
                    ui.add_space(4.0);
                });
            });
        });
    } else {
        // Fenêtre étroite : une seule colonne ; la carte du texte tient dans la hauteur visible
        // pour que le bouton Générer reste toujours à l'écran sans défiler.
        let h = ui.available_height();
        widgets::defiler(ui, ScrollArea::vertical().id_salt("synthese_page").auto_shrink([false, false]), false, |ui| {
            ui.spacing_mut().item_spacing.y = theme::ECART;
            carte_texte(app, ui, Some(h));
            carte_voix(app, ui);
            carte_historique(app, ui);
        });
    }
}

// ------------------------------------------------------------------ texte et génération

/// Bandeau affiché sous le bouton Générer.
enum Bandeau {
    Aucun,
    Generation,
    Serveur,
    Resultat,
}

fn bandeau(app: &App) -> Bandeau {
    if app.generation.is_some() {
        Bandeau::Generation
    } else if !app.serveur_pret() {
        Bandeau::Serveur
    } else if app.resultat.is_some() {
        Bandeau::Resultat
    } else {
        Bandeau::Aucun
    }
}

fn carte_texte(app: &mut App, ui: &mut Ui, hauteur: Option<f32>) {
    let haut_debut = ui.cursor().top();
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 10.0;
        ui.horizontal(|ui| {
            ui.set_min_height(theme::HAUTEUR_BOUTON);
            widgets::titre_carte(ui, "Texte à lire");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let occupe = app.generation.is_some();
                if Bouton::secondaire("Ouvrir un texte…")
                    .actif(!occupe)
                    .infobulle("Charger un fichier .txt ou .md\n(vous pouvez aussi le glisser dans la fenêtre)")
                    .ui(ui)
                    .clicked()
                {
                    app.choisir_texte();
                }
                if !app.texte.is_empty() && Bouton::discret("Effacer").actif(!occupe).infobulle("Vider la zone de texte").ui(ui).clicked() {
                    app.texte.clear();
                    app.texte_source = None;
                    app.texte_modifie();
                }
            });
        });

        // Hauteur de la zone de texte : l'espace restant moins le pied de carte.
        let pied = 26.0 + 48.0 + 2.0 * 10.0 + if matches!(bandeau(app), Bandeau::Aucun) { 0.0 } else { 58.0 + 10.0 };
        let h_zone = match hauteur {
            Some(h) => (h - (ui.cursor().top() - haut_debut) - pied - theme::MARGE_CARTE as f32 - 4.0).max(140.0),
            None => 260.0,
        };
        zone_texte(app, ui, h_zone);
        ligne_stats(app, ui);
        ligne_generer(app, ui);
        match bandeau(app) {
            Bandeau::Aucun => {}
            Bandeau::Generation => bandeau_generation(app, ui),
            Bandeau::Serveur => bandeau_serveur(app, ui),
            Bandeau::Resultat => bandeau_resultat(app, ui),
        }
    });
}

fn zone_texte(app: &mut App, ui: &mut Ui, h: f32) {
    let p = pal(ui);
    let id = egui::Id::new("texte_synthese");
    let focus = ui.memory(|m| m.has_focus(id));
    let bord = if focus { Stroke::new(1.5, p.accent_texte) } else { Stroke::new(1.0, p.bord_controle) };
    Frame::new()
        .fill(p.creux)
        .stroke(bord)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::defiler(
                ui,
                ScrollArea::vertical()
                    .id_salt("texte_defilant")
                    .max_height(h)
                    .min_scrolled_height(h)
                    .auto_shrink([false, false]),
                true,
                |ui| {
                    let w = ui.available_width();
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut app.texte)
                            .id(id)
                            .font(theme::police(16.5))
                            .frame(false)
                            .margin(Margin { left: 12, right: 12, top: 10, bottom: 20 })
                            .desired_width(w)
                            .min_size(vec2(w, h - 8.0))
                            .interactive(app.generation.is_none())
                            .hint_text(
                                RichText::new(
                                    "Tapez ou collez votre texte ici…\n\nVous pouvez aussi ouvrir un fichier .txt ou .md, ou le glisser dans la fenêtre.",
                                )
                                .color(p.texte_3)
                                .size(16.0),
                            ),
                    );
                    if r.changed() {
                        app.texte_modifie();
                    }
                });
        });
}

fn ligne_stats(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let s = &app.stats;
    ui.horizontal(|ui| {
        ui.set_min_height(16.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        if let Some(src) = &app.texte_source {
            widgets::badge(ui, src, p.accent_doux, p.accent_texte).on_hover_text("Fichier d'origine du texte");
        }
        let t = if s.caracteres == 0 {
            "Aucun texte pour l'instant.".to_string()
        } else {
            let mut t = format!("{} caractères · ~{} de lecture", widgets::entier(s.caracteres), widgets::duree_lisible(s.secondes));
            if s.morceaux > 1 {
                t.push_str(&format!(" · {} morceaux", s.morceaux));
            }
            t
        };
        ui.add(egui::Label::new(RichText::new(t).size(13.5).color(p.texte_2)).truncate()).on_hover_text(format!(
            "{} mots environ.\nAllTalk reçoit le texte par morceaux de 1 800 caractères au plus, coupés entre deux phrases.",
            s.mots
        ));
    });
}

fn ligne_generer(app: &mut App, ui: &mut Ui) {
    let occupe = app.generation.is_some();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let l_nom = (ui.available_width() - LARGEUR_FORMAT - LARGEUR_GENERER - 2.0 * 8.0 - 8.0).max(90.0);
        ui.add_enabled_ui(!occupe, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                widgets::etiquette(ui, "Nom du fichier");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    widgets::champ(ui, &mut app.nom_sortie, "narration", l_nom)
                        .on_hover_text("Nom du fichier audio, sans extension.\nUn numéro est ajouté s'il existe déjà.");
                    selecteur_format(app, ui, LARGEUR_FORMAT, "format_synthese");
                });
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
            let raison = if !app.serveur_pret() {
                "AllTalk doit être démarré pour générer."
            } else if app.texte.trim().is_empty() {
                "Saisissez ou ouvrez d'abord un texte."
            } else if app.cfg.voix.is_empty() {
                "Choisissez une voix."
            } else {
                "Une génération est déjà en cours."
            };
            let r = Bouton::principal(if occupe { "Génération…" } else { "Générer" })
                .icone(if occupe { "" } else { "▶" })
                .grand()
                .largeur_min(LARGEUR_GENERER)
                .actif(app.peut_generer())
                .infobulle("Créer le fichier audio (Ctrl+Entrée)")
                .raison(raison)
                .ui(ui);
            if r.clicked() {
                app.generer();
            }
        });
    });
}

/// Cadre teinté pleine largeur, 58 px de haut.
fn cadre_bandeau<R>(ui: &mut Ui, ton: Ton, contenu: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    let (fond, _) = widgets::couleurs_ton(p, ton);
    Frame::new()
        .fill(fond)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(42.0);
            contenu(ui)
        })
        .inner
}

fn bandeau_generation(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let Some(g) = &app.generation else { return };
    let (texte, fraction) = match &g.phase {
        Phase::Preparation => ("Envoi du texte à AllTalk…".to_string(), None),
        Phase::Morceau(pr) => (
            format!("Morceau {} / {}", pr.morceau, pr.total),
            Some(pr.morceau.saturating_sub(1) as f32 / pr.total.max(1) as f32),
        ),
        Phase::Conversion(f) => (format!("Conversion en {}…", f.extension().to_uppercase()), None),
    };
    let fichier = g.sortie.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let ecoule = widgets::chrono(g.debut.elapsed().as_secs_f32());
    cadre_bandeau(ui, Ton::Info, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            widgets::sablier(ui, 15.0, p.accent_texte);
            ui.label(RichText::new(texte).font(theme::police_forte(14.5)).color(p.texte));
            ui.add(egui::Label::new(RichText::new(format!("{ecoule} · {fichier}")).size(13.0).color(p.texte_2)).truncate());
        });
        widgets::progression(ui, fraction);
    });
}

fn bandeau_serveur(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let etat = app.etat_serveur();
    let ton = if matches!(etat, EtatServeur::Arrete) { Ton::Avert } else { Ton::Info };
    cadre_bandeau(ui, ton, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(42.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            match etat {
                EtatServeur::Arrete => {
                    ui.label(RichText::new("⚠").size(15.0).color(p.avert));
                    ui.add(egui::Label::new(RichText::new("AllTalk doit être démarré pour générer.").size(14.0).color(p.avert)).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Bouton::secondaire("Démarrer AllTalk").petit().ui(ui).clicked() {
                            app.demarrer_serveur();
                        }
                    });
                }
                EtatServeur::Demarrage(d) => {
                    widgets::sablier(ui, 15.0, p.accent_texte);
                    ui.label(
                        RichText::new(format!("AllTalk démarre ({}) : Générer sera disponible dans un instant.", widgets::chrono(d.as_secs_f32())))
                            .size(14.0)
                            .color(p.accent_texte),
                    );
                }
                _ => {
                    widgets::sablier(ui, 15.0, p.accent_texte);
                    ui.label(RichText::new("Connexion à AllTalk…").size(14.0).color(p.accent_texte));
                }
            }
        });
    });
}

fn bandeau_resultat(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let Some(g) = app.resultat.clone() else { return };
    cadre_bandeau(ui, Ton::Succes, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(42.0);
            ui.spacing_mut().item_spacing.x = 10.0;
            let lit = app.en_lecture(&g.chemin);
            if widgets::bouton_lecture(ui, lit, app.enregistrement.is_none()).clicked() {
                app.basculer_lecture(&g.chemin);
            }
            let reste = ui.available_width() - 150.0;
            ui.allocate_ui_with_layout(vec2(reste.max(80.0), 40.0), Layout::top_down(Align::LEFT), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(egui::Label::new(RichText::new(format!("✓ {}", g.nom())).font(theme::police_forte(14.5)).color(p.succes)).truncate())
                    .on_hover_text(g.chemin.display().to_string());
                ui.label(RichText::new(format!("{} · créé à {}", widgets::duree_lisible(g.duree), g.heure)).size(13.0).color(p.texte_2));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if Bouton::secondaire("Ouvrir le dossier").petit().infobulle("Afficher le fichier dans le gestionnaire de fichiers").ui(ui).clicked() {
                    crate::paths::ouvrir(g.chemin.parent().unwrap_or(&app.cfg.sorties_dir));
                }
            });
        });
    });
}

// ------------------------------------------------------------------ voix et réglages

fn carte_voix(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    let occupe = app.generation.is_some();
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        widgets::titre_carte(ui, "Voix");
        ui.add_space(2.0);
        let largeur = ui.available_width();
        ui.add_enabled_ui(!occupe, |ui| {
            egui::ComboBox::from_id_salt("choix_voix")
                .width(largeur)
                .height(380.0)
                .selected_text(
                    RichText::new(if app.cfg.voix.is_empty() { "Choisir une voix…".into() } else { super::libelle_voix(&app.cfg.voix) }).size(15.0),
                )
                .show_ui(ui, |ui| {
                    if app.voix_liste.is_empty() {
                        ui.label(RichText::new(if app.voix_chargees { "Aucune voix trouvée." } else { "Chargement…" }).color(p.texte_2));
                    }
                    let (jeux, fichiers): (Vec<&String>, Vec<&String>) =
                        app.voix_liste.iter().partition(|v| crate::voicesets::jeu_depuis_api(v).is_some());
                    if !jeux.is_empty() {
                        ui.label(RichText::new("JEUX DE VOIX (LES PLUS FIDÈLES)").font(theme::police_forte(11.5)).color(p.texte_2));
                        for v in jeux {
                            ui.selectable_value(&mut app.cfg.voix, v.clone(), super::libelle_voix(v));
                        }
                    }
                    if !fichiers.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new("VOIX SIMPLES (UN SEUL FICHIER)").font(theme::police_forte(11.5)).color(p.texte_2));
                        for v in fichiers {
                            ui.selectable_value(&mut app.cfg.voix, v.clone(), super::libelle_voix(v));
                        }
                    }
                })
                .response
                .on_hover_text("Un jeu de voix combine plusieurs échantillons :\nc'est le choix le plus fidèle à votre voix.");
        });
        if app.voix_chargees && !app.cfg.voix.is_empty() && !app.voix_liste.contains(&app.cfg.voix) {
            widgets::encadre(ui, Ton::Avert, "Cette voix n'est plus disponible : choisissez-en une autre.");
        }

        ui.add_space(8.0);
        ligne_reglage(ui, "Vitesse", &format!("{}×", widgets::decimal(app.cfg.vitesse, 2)));
        ui.add_enabled_ui(!occupe, |ui| {
            widgets::curseur(ui, &mut app.cfg.vitesse, 0.7, 1.3, 0.05, largeur)
                .on_hover_text("Glissez, ou utilisez les flèches du clavier.");
        });
        widgets::secondaire(ui, "1 = débit naturel ; moins = plus posé.");

        ui.add_space(8.0);
        ligne_reglage(ui, "Température", &widgets::decimal(app.cfg.temperature, 2));
        ui.add_enabled_ui(!occupe, |ui| {
            widgets::curseur(ui, &mut app.cfg.temperature, 0.3, 1.0, 0.05, largeur)
                .on_hover_text("Glissez, ou utilisez les flèches du clavier.");
        });
        widgets::secondaire(ui, "Basse : diction régulière. Haute : plus vivante, moins prévisible.");
    });
}

fn ligne_reglage(ui: &mut Ui, nom: &str, valeur: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        widgets::etiquette(ui, nom);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(valeur).font(theme::police_forte(14.5)).color(p.accent_texte));
        });
    });
}

/// Sélecteur du format de sortie ; enregistre la configuration à chaque changement.
pub fn selecteur_format(app: &mut App, ui: &mut Ui, largeur: f32, id: &str) {
    let p = pal(ui);
    let actuel = app.format_effectif();
    let mut choix = actuel;
    let r = egui::ComboBox::from_id_salt(id)
        .width(largeur)
        .selected_text(RichText::new(format!(".{}", actuel.extension())).size(15.0))
        .show_ui(ui, |ui| {
            ui.set_min_width(260.0);
            let formats: Vec<Format> = if app.ffmpeg { Format::TOUS.to_vec() } else { vec![Format::Wav] };
            for f in formats {
                ui.selectable_value(&mut choix, f, format!(".{}  —  {}", f.extension(), f.libelle()));
            }
            if !app.ffmpeg {
                ui.add_space(4.0);
                ui.label(RichText::new("Installer ffmpeg pour FLAC, MP3, OGG").size(13.0).color(p.texte_2));
            }
        })
        .response;
    r.on_hover_text(if app.ffmpeg {
        format!("Format du fichier : {}", actuel.libelle())
    } else {
        "WAV uniquement.\nInstaller ffmpeg pour FLAC, MP3, OGG.".to_string()
    });
    if choix != actuel {
        app.cfg.format = choix;
        app.sauver();
    }
}

// ------------------------------------------------------------------ historique

fn carte_historique(app: &mut App, ui: &mut Ui) {
    let p = pal(ui);
    widgets::carte(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.horizontal(|ui| {
            widgets::titre_carte(ui, "Cette session");
            if !app.historique.is_empty() {
                widgets::badge(ui, &app.historique.len().to_string(), p.accent_doux, p.accent_texte);
            }
        });
        if app.historique.is_empty() {
            widgets::secondaire(ui, "Les fichiers générés apparaîtront ici, prêts à être réécoutés.");
        }
        for (i, h) in app.historique.clone().into_iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                // ■ à la place de ▶ pendant la lecture de ce fichier.
                if widgets::bouton_lecture(ui, app.en_lecture(&h.chemin), app.enregistrement.is_none()).clicked() {
                    app.basculer_lecture(&h.chemin);
                }
                let reste = (ui.available_width() - 44.0).max(60.0);
                ui.allocate_ui_with_layout(vec2(reste, 36.0), Layout::top_down(Align::LEFT), |ui| {
                    ui.set_min_width(reste);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(egui::Label::new(RichText::new(h.nom()).size(14.5).color(p.texte)).truncate())
                        .on_hover_text(h.chemin.display().to_string());
                    ui.label(RichText::new(format!("{} · {}", widgets::duree_lisible(h.duree), h.heure)).size(12.5).color(p.texte_2));
                });
                let id_menu = ui.make_persistent_id(("menu_historique", i));
                let r = widgets::bouton_menu(ui, "Actions sur ce fichier");
                if r.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(id_menu));
                }
                egui::popup_below_widget(ui, id_menu, &r, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                    ui.set_min_width(240.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let lit = app.en_lecture(&h.chemin);
                    if widgets::element_menu(ui, if lit { "■" } else { "▶" }, if lit { "Arrêter la lecture" } else { "Écouter" }, false).clicked() {
                        app.basculer_lecture(&h.chemin);
                    }
                    if widgets::element_menu(ui, "↗", "Ouvrir le dossier", false).clicked() {
                        crate::paths::ouvrir(h.chemin.parent().unwrap_or(&app.cfg.sorties_dir));
                    }
                });
            });
        }
        if Bouton::discret("Ouvrir le dossier des fichiers").petit().ui(ui).clicked() {
            crate::paths::ouvrir(&app.cfg.sorties_dir);
        }
    });
}
