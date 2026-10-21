//! Boîtes de dialogue modales : nouveau jeu, confirmations, export en voix simple.

use super::theme::{self, pal, Palette};
use super::widgets::{self, Bouton};
use crate::app::{App, Dialogue};
use crate::voicesets;
use eframe::egui::{self, vec2, Align, Align2, CornerRadius, Frame, Id, Key, Layout, Margin, Modal, RichText, Sense, Stroke, Ui};

const LARGEUR: f32 = 460.0;

enum Issue {
    Rester,
    Fermer,
    Valider,
}

pub fn afficher(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.dialogue.take() else { return };
    let p = theme::pal_ctx(ctx);
    let cadre = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.bord))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(26))
        .shadow(ctx.style().visuals.window_shadow);
    let mut issue = Issue::Rester;
    let reponse = Modal::new(Id::new("dialogue_vocalise"))
        .frame(cadre)
        .backdrop_color(egui::Color32::from_black_alpha(if ctx.style().visuals.dark_mode { 150 } else { 90 }))
        .show(ctx, |ui| {
            ui.set_width(LARGEUR);
            ui.spacing_mut().item_spacing.y = 10.0;
            issue = match &mut d {
                Dialogue::NouveauJeu { nom, erreur } => nouveau_jeu(ui, nom, erreur),
                Dialogue::Reinitialiser { jeu, n } => confirmation(
                    ui,
                    &format!("Réinitialiser « {jeu} » ?"),
                    &format!(
                        "{} de ce jeu {} déplacé{} dans la corbeille de Vocalise. Le jeu restera, vide, prêt pour de nouveaux enregistrements.",
                        pluriel(*n, "Le seul échantillon", "Les {} échantillons"),
                        if *n > 1 { "seront" } else { "sera" },
                        if *n > 1 { "s" } else { "" }
                    ),
                    "Réinitialiser",
                ),
                Dialogue::SupprimerJeu { jeu, n } => confirmation(
                    ui,
                    &format!("Supprimer le jeu « {jeu} » ?"),
                    &format!(
                        "Le jeu disparaîtra de la liste des voix. {}",
                        if *n == 0 {
                            "Il ne contient aucun échantillon.".to_string()
                        } else {
                            format!("{} dans la corbeille de Vocalise.", pluriel(*n, "Son échantillon sera déplacé", "Ses {} échantillons seront déplacés"))
                        }
                    ),
                    "Supprimer le jeu",
                ),
                Dialogue::Exporter { chemin, nom, erreur } => exporter(ui, chemin, nom, erreur),
                Dialogue::RemplacerTexte { chemin } => remplacer_texte(ui, chemin),
            };
        });
    if reponse.should_close() && matches!(issue, Issue::Rester) {
        issue = Issue::Fermer;
    }
    match issue {
        Issue::Rester => app.dialogue = Some(d),
        Issue::Fermer => {}
        Issue::Valider => match d {
            Dialogue::NouveauJeu { nom, .. } => {
                if let Err(e) = app.creer_jeu(&nom) {
                    app.dialogue = Some(Dialogue::NouveauJeu { nom, erreur: Some(e) });
                }
            }
            Dialogue::Reinitialiser { jeu, .. } => app.reinitialiser_jeu(&jeu),
            Dialogue::SupprimerJeu { jeu, .. } => app.supprimer_jeu(&jeu),
            Dialogue::Exporter { chemin, nom, .. } => {
                if let Err(e) = app.exporter(&chemin, &nom) {
                    app.dialogue = Some(Dialogue::Exporter { chemin, nom, erreur: Some(e) });
                }
            }
            Dialogue::RemplacerTexte { chemin } => app.charger_texte(&chemin),
        },
    }
}

fn pluriel(n: usize, un: &str, plusieurs: &str) -> String {
    if n <= 1 {
        un.to_string()
    } else {
        plusieurs.replace("{}", &n.to_string())
    }
}

/// Icône ronde en tête de boîte de dialogue.
fn icone(ui: &mut Ui, symbole: &str, fond: egui::Color32, couleur: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 24.0, fond);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, symbole, theme::police(22.0), couleur);
}

fn titre(ui: &mut Ui, t: &str) {
    let p = pal(ui);
    ui.label(RichText::new(t).font(theme::police_forte(20.0)).color(p.texte));
}

fn corps(ui: &mut Ui, t: &str) {
    let p = pal(ui);
    ui.add(egui::Label::new(RichText::new(t).size(15.0).color(p.texte)).wrap());
}

/// Rangée de boutons alignée à droite : [Annuler] [action].
fn boutons(ui: &mut Ui, action: Bouton<'_>) -> Issue {
    let mut issue = Issue::Rester;
    ui.add_space(6.0);
    ui.allocate_ui_with_layout(vec2(ui.available_width(), theme::HAUTEUR_BOUTON), Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if action.largeur_min(130.0).ui(ui).clicked() {
            issue = Issue::Valider;
        }
        if Bouton::secondaire("Annuler").largeur_min(110.0).ui(ui).clicked() {
            issue = Issue::Fermer;
        }
    });
    issue
}

fn confirmation(ui: &mut Ui, t: &str, texte: &str, action: &str) -> Issue {
    let p: &Palette = pal(ui);
    icone(ui, "⚠", p.danger_doux, p.danger_texte);
    titre(ui, t);
    corps(ui, texte);
    widgets::secondaire(ui, "Rien n'est effacé définitivement : les fichiers restent récupérables dans le dossier echantillons/corbeille.");
    let issue = boutons(ui, Bouton::danger(action));
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        return Issue::Fermer;
    }
    issue
}

fn nouveau_jeu(ui: &mut Ui, nom: &mut String, erreur: &mut Option<String>) -> Issue {
    let p = pal(ui);
    icone(ui, "＋", p.accent_doux, p.accent_texte);
    titre(ui, "Nouveau jeu de voix");
    corps(ui, "Un jeu de voix regroupe vos échantillons. Donnez-lui un nom court, par exemple « emmanuel-classe ».");
    widgets::etiquette(ui, "Nom du jeu");
    let r = widgets::champ(ui, nom, "emmanuel-classe", ui.available_width());
    if ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    if r.changed() {
        *erreur = None;
    }
    let valide = voicesets::nom_valide(nom.trim());
    if let Some(e) = erreur {
        ui.label(RichText::new(format!("⚠ {e}")).size(13.5).color(p.danger_texte));
    } else if !nom.is_empty() && !valide {
        ui.label(RichText::new("⚠ Lettres sans accent, chiffres, « - » et « _ » uniquement, 40 au plus.").size(13.5).color(p.danger_texte));
    } else {
        widgets::secondaire(ui, "Lettres sans accent, chiffres, « - » et « _ » (40 au plus).");
    }
    let entree = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    match boutons(ui, Bouton::principal("Créer").actif(valide)) {
        Issue::Rester if entree && valide => Issue::Valider,
        i => i,
    }
}

fn exporter(ui: &mut Ui, chemin: &std::path::Path, nom: &mut String, erreur: &mut Option<String>) -> Issue {
    let p = pal(ui);
    icone(ui, "⇪", p.accent_doux, p.accent_texte);
    titre(ui, "Exporter en voix simple");
    corps(
        ui,
        "Copie cet échantillon dans le dossier des voix d'AllTalk : il devient une voix utilisable seule dans la synthèse (moins fidèle qu'un jeu complet).",
    );
    widgets::secondaire(ui, format!("Échantillon : {}", chemin.file_name().unwrap_or_default().to_string_lossy()));
    widgets::etiquette(ui, "Nom de la voix");
    let r = widgets::champ(ui, nom, "ma-voix", ui.available_width());
    if ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    if r.changed() {
        *erreur = None;
    }
    match erreur {
        Some(e) => {
            ui.label(RichText::new(format!("⚠ {e}")).size(13.5).color(p.danger_texte));
        }
        None => {
            let n = crate::paths::nom_sur(nom.trim());
            widgets::secondaire(ui, format!("Fichier créé : voices/{}.wav", if n.is_empty() { "…".into() } else { n }));
        }
    }
    let valide = !nom.trim().is_empty();
    let entree = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    match boutons(ui, Bouton::principal("Exporter").actif(valide)) {
        Issue::Rester if entree && valide => Issue::Valider,
        i => i,
    }
}

fn remplacer_texte(ui: &mut Ui, chemin: &std::path::Path) -> Issue {
    let p = pal(ui);
    icone(ui, "?", p.accent_doux, p.accent_texte);
    titre(ui, "Remplacer le texte actuel ?");
    corps(
        ui,
        &format!(
            "Le texte en cours sera remplacé par le contenu de « {} ».",
            chemin.file_name().unwrap_or_default().to_string_lossy()
        ),
    );
    widgets::secondaire(ui, "Le texte actuel n'est enregistré nulle part : copiez-le d'abord si vous voulez le garder.");
    let issue = boutons(ui, Bouton::principal("Remplacer"));
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        return Issue::Fermer;
    }
    issue
}
