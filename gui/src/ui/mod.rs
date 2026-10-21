//! Interface egui : thème, cadre (navigation, bandeau du serveur, barre d'état), écrans
//! (synthèse, ma voix, réglages), boîtes de dialogue, widgets et mode capture.

pub mod capture;
pub mod dialogues;
pub mod reglages;
pub mod synthese;
pub mod theme;
pub mod voix;
pub mod widgets;

use crate::app::{App, Ecran, EtatServeur, Niveau};
use eframe::egui::{self, vec2, Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Stroke, StrokeKind};
use theme::pal_ctx;
use widgets::Bouton;

/// Dessine toute la fenêtre.
pub fn afficher(app: &mut App, ctx: &egui::Context) {
    barre_laterale(app, ctx);
    barre_etat(app, ctx);
    bandeau_serveur(app, ctx);
    let p = pal_ctx(ctx);
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.fond).inner_margin(Margin { left: 28, right: 28, top: 10, bottom: 16 }))
        .show(ctx, |ui| match app.ecran {
            Ecran::Synthese => synthese::afficher(app, ui),
            Ecran::Voix => voix::afficher(app, ui),
            Ecran::Reglages => reglages::afficher(app, ui),
        });
    dialogues::afficher(app, ctx);
    voile_depot(ctx);
}

fn barre_laterale(app: &mut App, ctx: &egui::Context) {
    let p = pal_ctx(ctx);
    egui::SidePanel::left("navigation")
        .exact_width(214.0)
        .resizable(false)
        .frame(Frame::new().fill(p.barre).inner_margin(Margin { left: 14, right: 14, top: 20, bottom: 16 }).stroke(Stroke::new(1.0, p.bord)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if let Some(logo) = &app.logo {
                    ui.add(egui::Image::new(logo).fit_to_exact_size(vec2(40.0, 40.0)).corner_radius(CornerRadius::same(10)));
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(RichText::new("Vocalise").font(theme::police_forte(20.0)).color(p.texte));
                    ui.label(RichText::new("Votre voix, vos textes").size(12.5).color(p.texte_2));
                });
            });
            ui.add_space(26.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            let entrees = [
                (Ecran::Synthese, "🔊", "Synthèse"),
                (Ecran::Voix, "🎤", "Ma voix"),
                (Ecran::Reglages, "⚙", "Réglages"),
            ];
            for (ecran, icone, texte) in entrees {
                let r = widgets::bouton_nav(ui, icone, texte, app.ecran == ecran);
                if ecran == Ecran::Voix && app.enregistrement.is_some() {
                    // Point rouge : un enregistrement tourne pendant qu'on regarde ailleurs.
                    let c = r.rect.right_center() - vec2(18.0, 0.0);
                    ui.painter().circle_filled(c, 5.0, p.rec);
                }
                if r.clicked() {
                    app.ecran = ecran;
                }
            }
            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(12.0).color(p.texte_2));
            });
        });
}

/// Bandeau d'état du serveur AllTalk, en haut de la zone de contenu.
fn bandeau_serveur(app: &mut App, ctx: &egui::Context) {
    let p = pal_ctx(ctx);
    let etat = app.etat_serveur();
    egui::TopBottomPanel::top("bandeau_serveur")
        .show_separator_line(false)
        .frame(Frame::new().fill(p.fond).inner_margin(Margin { left: 28, right: 28, top: 14, bottom: 4 }))
        .show(ctx, |ui| {
            if app.diagnostic_bloque() {
                // Installation incomplète : plus important que l'état du serveur.
                encadre_installation(app, ui);
                return;
            }
            let arrete = matches!(etat, EtatServeur::Arrete);
            let (fond, bord) = if arrete { (p.avert_doux, p.avert_doux) } else { (p.surface, p.bord) };
            Frame::new()
                .fill(fond)
                .stroke(Stroke::new(1.0, bord))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(14, if arrete { 10 } else { 6 }))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.set_min_height(if arrete { 38.0 } else { 30.0 });
                        match etat {
                            EtatServeur::Pret { lance_par_nous } => {
                                widgets::pastille(ui, p.succes, 5.0);
                                ui.label(RichText::new("AllTalk est prêt").font(theme::police_forte(14.5)).color(p.texte));
                                let n = app.voix_liste.len();
                                let detail = if lance_par_nous {
                                    format!("{n} voix disponibles · lancé par Vocalise")
                                } else {
                                    format!("{n} voix disponibles")
                                };
                                ui.label(RichText::new(detail).size(13.5).color(p.texte_2));
                            }
                            EtatServeur::Verification => {
                                widgets::sablier(ui, 16.0, p.texte_2);
                                ui.label(RichText::new("Recherche du serveur AllTalk…").size(14.5).color(p.texte_2));
                            }
                            EtatServeur::Demarrage(d) => {
                                widgets::sablier(ui, 16.0, p.accent_texte);
                                ui.label(RichText::new("AllTalk démarre…").font(theme::police_forte(14.5)).color(p.texte));
                                ui.label(
                                    RichText::new(format!("{} · le chargement du modèle prend environ une minute", widgets::chrono(d.as_secs_f32())))
                                        .size(13.5)
                                        .color(p.texte_2),
                                );
                            }
                            EtatServeur::Arret => {
                                widgets::sablier(ui, 16.0, p.texte_2);
                                ui.label(RichText::new("Arrêt d'AllTalk…").size(14.5).color(p.texte_2));
                            }
                            EtatServeur::Arrete => {
                                widgets::pastille(ui, p.avert, 5.0);
                                let l = (ui.available_width() - 200.0).max(120.0);
                                ui.allocate_ui_with_layout(vec2(l, 38.0), Layout::top_down(Align::LEFT), |ui| {
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    ui.label(RichText::new("AllTalk n'est pas démarré").font(theme::police_forte(15.0)).color(p.texte));
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new("Il est nécessaire pour générer de l'audio. L'enregistrement d'échantillons fonctionne sans lui.")
                                                .size(13.5)
                                                .color(p.texte_2),
                                        )
                                        .wrap(),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if Bouton::principal("Démarrer AllTalk").icone("▶").ui(ui).clicked() {
                                        app.demarrer_serveur();
                                    }
                                });
                            }
                        }
                    });
                });
            avis_non_bloquant(app, ui);
            avis_mise_a_jour(app, ui);
        });
}

/// Encadré rouge « Installation incomplète » avec les composants bloquants manquants.
fn encadre_installation(app: &mut App, ui: &mut egui::Ui) {
    let p = theme::pal(ui);
    let manques = app.manques(crate::diagnostic::Gravite::Bloquant);
    Frame::new()
        .fill(p.danger_doux)
        .stroke(Stroke::new(1.0, p.danger_texte.gamma_multiply(0.35)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(38.0);
                widgets::pastille(ui, p.danger_texte, 5.0);
                let l = (ui.available_width() - 210.0).max(120.0);
                ui.allocate_ui_with_layout(vec2(l, 38.0), Layout::top_down(Align::LEFT), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new("Installation incomplète").font(theme::police_forte(15.0)).color(p.danger_texte));
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("Manquant : {}. La synthèse ne peut pas fonctionner.", manques.join(", ")))
                                .size(13.5)
                                .color(p.texte),
                        )
                        .wrap(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if app.ecran != Ecran::Reglages && Bouton::principal("Voir le diagnostic").ui(ui).clicked() {
                        app.ecran = Ecran::Reglages;
                    }
                });
            });
        });
}

/// Encadré bleu discret, fermable : une version plus récente est publiée.
fn avis_mise_a_jour(app: &mut App, ui: &mut egui::Ui) {
    if app.avis_maj_ferme || app.ecran == Ecran::Reglages {
        return;
    }
    let Some(version) = app.maj_disponible().map(|p| p.version) else { return };
    let p = theme::pal(ui);
    ui.add_space(6.0);
    Frame::new()
        .fill(p.accent_doux)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new("⬆").size(14.0).color(p.accent));
                let l = (ui.available_width() - 210.0).max(100.0);
                ui.allocate_ui_with_layout(vec2(l, 30.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(format!("Nouvelle version {version} de Vocalise disponible")).size(13.5).color(p.accent))
                            .truncate(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if Bouton::discret("✕").petit().infobulle("Masquer jusqu'au prochain lancement").ui(ui).clicked() {
                        app.avis_maj_ferme = true;
                    }
                    if Bouton::secondaire("Voir").petit().ui(ui).clicked() {
                        app.ecran = Ecran::Reglages;
                    }
                });
            });
        });
}

/// Encadré orange discret, fermable, pour les manques non bloquants (une fois par session).
fn avis_non_bloquant(app: &mut App, ui: &mut egui::Ui) {
    if app.avis_diagnostic_ferme || app.ecran == Ecran::Reglages {
        return;
    }
    let manques = app.manques(crate::diagnostic::Gravite::Fonction);
    if manques.is_empty() {
        return;
    }
    let p = theme::pal(ui);
    ui.add_space(6.0);
    Frame::new()
        .fill(p.avert_doux)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new("⚠").size(14.0).color(p.avert));
                let l = (ui.available_width() - 210.0).max(100.0);
                ui.allocate_ui_with_layout(vec2(l, 30.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(format!("Fonctions limitées : {}", manques.join(", "))).size(13.5).color(p.avert))
                            .truncate(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if Bouton::discret("✕").petit().infobulle("Masquer jusqu'au prochain lancement").ui(ui).clicked() {
                        app.avis_diagnostic_ferme = true;
                    }
                    if Bouton::secondaire("Voir le diagnostic").petit().ui(ui).clicked() {
                        app.ecran = Ecran::Reglages;
                    }
                });
            });
        });
}

/// Barre d'état : dernier message (erreur en rouge, succès en vert, info neutre).
fn barre_etat(app: &mut App, ctx: &egui::Context) {
    let p = pal_ctx(ctx);
    egui::TopBottomPanel::bottom("barre_etat")
        .show_separator_line(false)
        .frame(Frame::new().fill(p.barre).inner_margin(Margin::symmetric(18, 7)).stroke(Stroke::new(1.0, p.bord)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(22.0);
                let (icone, couleur, texte) = match &app.statut {
                    Some(s) => match s.niveau {
                        Niveau::Erreur => ("⚠", p.danger_texte, s.texte.as_str()),
                        Niveau::Succes => ("✓", p.succes, s.texte.as_str()),
                        Niveau::Info => ("ℹ", p.texte_2, s.texte.as_str()),
                    },
                    None => ("ℹ", p.texte_2, "Bienvenue dans Vocalise."),
                };
                ui.label(RichText::new(icone).color(couleur).size(14.0));
                let r = ui.add(egui::Label::new(RichText::new(texte).color(couleur).size(13.5)).truncate());
                if r.hovered() && texte.len() > 80 {
                    r.on_hover_text(texte);
                }
                let fraiche = app.statut.as_ref().is_some_and(|s| s.niveau != Niveau::Info && s.quand.elapsed().as_secs_f32() < 0.6);
                if fraiche {
                    ctx.request_repaint();
                }
            });
        });
}

/// Voile affiché pendant qu'un fichier est glissé au-dessus de la fenêtre.
fn voile_depot(ctx: &egui::Context) {
    let survol = ctx.input(|i| !i.raw.hovered_files.is_empty());
    if !survol {
        return;
    }
    let p = pal_ctx(ctx);
    let ecran = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("voile_depot")));
    painter.rect_filled(ecran, CornerRadius::ZERO, p.fond.gamma_multiply(0.85));
    let zone = ecran.shrink(40.0);
    painter.rect(zone, CornerRadius::same(18), p.accent_doux.gamma_multiply(0.6), Stroke::new(3.0, p.accent_texte), StrokeKind::Inside);
    painter.text(zone.center() - vec2(0.0, 14.0), Align2::CENTER_CENTER, "Déposez le fichier ici", theme::police_forte(26.0), p.accent_texte);
    painter.text(
        zone.center() + vec2(0.0, 20.0),
        Align2::CENTER_CENTER,
        "Son texte sera chargé dans l'écran Synthèse (.txt ou .md)",
        theme::police(15.0),
        p.texte_2,
    );
}

/// Libellé lisible d'une voix AllTalk.
pub fn libelle_voix(v: &str) -> String {
    if let Some(j) = crate::voicesets::jeu_depuis_api(v) {
        format!("Jeu de voix : {j}")
    } else {
        v.strip_suffix(".wav").unwrap_or(v).to_string()
    }
}
