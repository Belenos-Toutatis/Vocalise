//! Widgets dessinés sur mesure : cartes, boutons, curseur, vumètre, jauge, badges.

use super::theme::{self, pal, Palette};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Rect, Response,
    RichText, Sense, Shadow, Shape, Stroke, StrokeKind, Ui,
};

// ------------------------------------------------------------------ mise en forme

/// Carte : fond clair, bord fin, ombre douce, coins arrondis ; occupe toute la largeur.
pub fn carte<R>(ui: &mut Ui, contenu: impl FnOnce(&mut Ui) -> R) -> R {
    carte_teintee(ui, None, contenu)
}

/// Carte dont le fond (et le bord) peuvent être teintés, pour un état particulier.
pub fn carte_teintee<R>(ui: &mut Ui, teinte: Option<(Color32, Color32)>, contenu: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    let (fond, bord) = teinte.unwrap_or((p.surface, p.bord));
    Frame::new()
        .fill(fond)
        .stroke(Stroke::new(1.0, bord))
        .corner_radius(CornerRadius::same(theme::RAYON_CARTE))
        .shadow(Shadow { offset: [0, 2], blur: 10, spread: 0, color: p.ombre })
        .inner_margin(Margin::same(theme::MARGE_CARTE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contenu(ui)
        })
        .inner
}

/// Titre de page et phrase d'introduction.
pub fn titre_page(ui: &mut Ui, titre: &str, intro: &str) {
    let p = pal(ui);
    let compact = ui.available_height() < 470.0;
    ui.label(RichText::new(titre).font(theme::police_forte(theme::TAILLE_TITRE)).color(p.texte));
    if !compact {
        // Fenêtre basse : on sacrifie la phrase d'introduction au profit du contenu.
        ui.add_space(-4.0);
        ui.label(RichText::new(intro).size(theme::TAILLE_CORPS).color(p.texte_2));
    }
    ui.add_space(6.0);
}

/// Titre de carte (17 px demi-gras).
pub fn titre_carte(ui: &mut Ui, titre: &str) {
    let p = pal(ui);
    ui.label(RichText::new(titre).font(theme::police_forte(theme::TAILLE_SOUS_TITRE)).color(p.texte));
}

/// Libellé de champ (13 px, atténué, demi-gras).
pub fn etiquette(ui: &mut Ui, texte: &str) {
    let p = pal(ui);
    ui.label(RichText::new(texte).font(theme::police_forte(theme::TAILLE_SECONDAIRE)).color(p.texte_2));
}

/// Texte secondaire (13 px, atténué).
pub fn secondaire(ui: &mut Ui, texte: impl Into<String>) -> Response {
    let p = pal(ui);
    ui.add(egui::Label::new(RichText::new(texte.into()).size(theme::TAILLE_SECONDAIRE).color(p.texte_2)).wrap())
}

/// Encadré d'information teinté (avertissement, erreur, succès, info).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum Ton {
    Info,
    Succes,
    Avert,
    Erreur,
}

pub fn couleurs_ton(p: &Palette, ton: Ton) -> (Color32, Color32) {
    match ton {
        Ton::Info => (p.accent_doux, p.accent_texte),
        Ton::Succes => (p.succes_doux, p.succes),
        Ton::Avert => (p.avert_doux, p.avert),
        Ton::Erreur => (p.danger_doux, p.danger_texte),
    }
}

pub fn encadre(ui: &mut Ui, ton: Ton, texte: &str) {
    let p = pal(ui);
    let (fond, couleur) = couleurs_ton(p, ton);
    let icone = match ton {
        Ton::Info => "ℹ",
        Ton::Succes => "✓",
        Ton::Avert => "⚠",
        Ton::Erreur => "⚠",
    };
    Frame::new()
        .fill(fond)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new(icone).color(couleur).size(theme::TAILLE_CORPS));
                ui.add(egui::Label::new(RichText::new(texte).color(couleur).size(14.0)).wrap());
            });
        });
}

/// Petite pastille de texte (« Nouveau », « +2 »…).
pub fn badge(ui: &mut Ui, texte: &str, fond: Color32, couleur: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(texte.to_string(), theme::police_forte(12.0), couleur);
    let taille = galley.size() + vec2(16.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(taille, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), fond);
    painter.galley(rect.center() - galley.size() / 2.0, galley, couleur);
    resp
}

/// Pastille d'état (point coloré avec halo).
pub fn pastille(ui: &mut Ui, couleur: Color32, rayon: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(rayon * 2.0 + 8.0, rayon * 2.0 + 8.0), Sense::hover());
    let c = rect.center();
    ui.painter().circle_filled(c, rayon + 4.0, couleur.gamma_multiply(0.22));
    ui.painter().circle_filled(c, rayon, couleur);
    resp
}

// ------------------------------------------------------------------ boutons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Genre {
    /// Plein, couleur d'accent, texte blanc : l'action principale de l'écran.
    Principal,
    /// Fond de carte, bord fin.
    Secondaire,
    /// Sans fond ni bord tant qu'on ne le survole pas.
    Discret,
    /// Rouge plein : uniquement dans les confirmations.
    Danger,
}

pub struct Bouton<'a> {
    texte: &'a str,
    icone: Option<&'a str>,
    genre: Genre,
    actif: bool,
    hauteur: f32,
    taille: f32,
    largeur_min: f32,
    infobulle: Option<&'a str>,
    raison: Option<&'a str>,
}

impl<'a> Bouton<'a> {
    fn new(genre: Genre, texte: &'a str) -> Self {
        Bouton {
            texte,
            icone: None,
            genre,
            actif: true,
            hauteur: theme::HAUTEUR_BOUTON,
            taille: theme::TAILLE_CORPS,
            largeur_min: 0.0,
            infobulle: None,
            raison: None,
        }
    }
    pub fn principal(texte: &'a str) -> Self {
        Self::new(Genre::Principal, texte)
    }
    pub fn secondaire(texte: &'a str) -> Self {
        Self::new(Genre::Secondaire, texte)
    }
    pub fn discret(texte: &'a str) -> Self {
        Self::new(Genre::Discret, texte)
    }
    pub fn danger(texte: &'a str) -> Self {
        Self::new(Genre::Danger, texte)
    }
    pub fn icone(mut self, icone: &'a str) -> Self {
        self.icone = Some(icone);
        self
    }
    pub fn actif(mut self, actif: bool) -> Self {
        self.actif = actif;
        self
    }
    /// Gros bouton (48 px, 17 px) pour l'action principale d'un écran.
    pub fn grand(mut self) -> Self {
        self.hauteur = 48.0;
        self.taille = 17.0;
        self
    }
    /// Bouton compact (32 px) pour les lignes de liste.
    pub fn petit(mut self) -> Self {
        self.hauteur = theme::HAUTEUR_PETIT;
        self.taille = 14.0;
        self
    }
    pub fn largeur_min(mut self, l: f32) -> Self {
        self.largeur_min = l;
        self
    }
    pub fn infobulle(mut self, t: &'a str) -> Self {
        self.infobulle = Some(t);
        self
    }
    /// Explication affichée au survol quand le bouton est désactivé.
    pub fn raison(mut self, t: &'a str) -> Self {
        self.raison = Some(t);
        self
    }

    pub fn ui(self, ui: &mut Ui) -> Response {
        let p = pal(ui);
        let police = if matches!(self.genre, Genre::Principal | Genre::Danger) {
            theme::police_forte(self.taille)
        } else {
            theme::police(self.taille)
        };
        let mut libelle = String::new();
        if let Some(i) = self.icone.filter(|i| !i.is_empty()) {
            libelle.push_str(i);
            if !self.texte.is_empty() {
                libelle.push_str("  ");
            }
        }
        libelle.push_str(self.texte);
        let galley = ui.painter().layout_no_wrap(libelle, police, Color32::PLACEHOLDER);
        let pad = if self.hauteur >= 44.0 { 26.0 } else if self.hauteur <= theme::HAUTEUR_PETIT { 12.0 } else { 16.0 };
        let largeur = (galley.size().x + 2.0 * pad).max(self.largeur_min).max(self.hauteur);
        let sense = if self.actif { Sense::click() } else { Sense::hover() };
        let (rect, mut resp) = ui.allocate_exact_size(vec2(largeur, self.hauteur), sense);
        let nom_accessible = self.texte.to_string();
        let actif = self.actif;
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, actif, &nom_accessible));

        if ui.is_rect_visible(rect) {
            let survol = self.actif && resp.hovered();
            let presse = self.actif && resp.is_pointer_button_down_on();
            let (fond, bord, texte) = match (self.genre, self.actif) {
                (_, false) if self.genre == Genre::Discret => (Color32::TRANSPARENT, Color32::TRANSPARENT, p.texte_3),
                (_, false) => (p.inactif_fond, Color32::TRANSPARENT, p.inactif_texte),
                (Genre::Principal, true) => (
                    if presse { p.accent_presse } else if survol { p.accent_survol } else { p.accent },
                    Color32::TRANSPARENT,
                    Color32::WHITE,
                ),
                (Genre::Danger, true) => (
                    if survol || presse { p.danger_survol } else { p.danger },
                    Color32::TRANSPARENT,
                    Color32::WHITE,
                ),
                (Genre::Secondaire, true) => (
                    if presse { p.presse } else if survol { p.survol } else { p.surface },
                    if survol { p.accent_texte } else { p.bord_controle },
                    p.texte,
                ),
                (Genre::Discret, true) => (
                    if presse { p.presse } else if survol { p.survol } else { Color32::TRANSPARENT },
                    Color32::TRANSPARENT,
                    if survol { p.accent_texte } else { p.texte },
                ),
            };
            let painter = ui.painter();
            let r = CornerRadius::same(theme::RAYON_BOUTON);
            if self.genre == Genre::Principal && self.actif && !presse {
                // Légère ombre portée : le bouton principal « sort » de la carte.
                painter.add(Shadow { offset: [0, 2], blur: 6, spread: 0, color: p.ombre_forte }.as_shape(rect, r));
            }
            painter.rect(rect, r, fond, Stroke::new(1.0, bord), StrokeKind::Inside);
            painter.galley(rect.center() - galley.size() / 2.0, galley, texte);
            if resp.has_focus() {
                painter.rect_stroke(rect.expand(3.0), CornerRadius::same(theme::RAYON_BOUTON + 3), Stroke::new(2.0, p.accent_texte), StrokeKind::Outside);
            }
        }
        if self.actif {
            resp = resp.on_hover_cursor(CursorIcon::PointingHand);
            if let Some(t) = self.infobulle {
                resp = resp.on_hover_text(t);
            }
        } else if let Some(t) = self.raison.or(self.infobulle) {
            resp = resp.on_hover_text(t);
        }
        resp
    }
}

/// Bouton rond à icône seule (▶, ■, ⋯…), toujours avec infobulle.
pub fn bouton_icone(ui: &mut Ui, icone: &str, infobulle: &str, enfonce: bool) -> Response {
    let p = pal(ui);
    let d = theme::HAUTEUR_PETIT + 2.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(d, d), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, infobulle));
    let survol = resp.hovered();
    let (fond, couleur) = if enfonce {
        (if survol { p.accent_survol } else { p.accent }, Color32::WHITE)
    } else if survol {
        (p.selection, p.accent_texte)
    } else {
        (p.accent_doux, p.accent_texte)
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), d / 2.0, fond);
    painter.text(rect.center() + vec2(0.5, 0.0), Align2::CENTER_CENTER, icone, theme::police(15.0), couleur);
    if resp.has_focus() {
        painter.circle_stroke(rect.center(), d / 2.0 + 3.0, Stroke::new(2.0, p.accent_texte));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(infobulle)
}

/// Bouton de lecture : ▶ ou ■ selon que ce fichier est en cours de lecture.
/// `actif` faux pendant un enregistrement (le micro capterait le haut-parleur).
pub fn bouton_lecture(ui: &mut Ui, en_lecture: bool, actif: bool) -> Response {
    if en_lecture {
        bouton_icone(ui, "■", "Arrêter la lecture", true)
    } else if actif {
        bouton_icone(ui, "▶", "Écouter", false)
    } else {
        let p = pal(ui);
        let d = theme::HAUTEUR_PETIT + 2.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(d, d), Sense::hover());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, false, "Écouter"));
        ui.painter().circle_filled(rect.center(), d / 2.0, p.inactif_fond);
        ui.painter().text(rect.center() + vec2(0.5, 0.0), Align2::CENTER_CENTER, "▶", theme::police(15.0), p.inactif_texte);
        resp.on_hover_text("Pas de lecture pendant l'enregistrement")
    }
}

/// Petite animation d'attente (arc tournant), cadencée à 10 images/s pour ménager le processeur
/// (un `egui::Spinner` force un rafraîchissement continu).
pub fn sablier(ui: &mut Ui, taille: f32, couleur: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(taille, taille), Sense::hover());
    let t = ui.input(|i| i.time);
    let c = rect.center();
    let r = taille / 2.0 - 1.5;
    let debut = (t * std::f64::consts::TAU * 0.9) as f32;
    let points: Vec<egui::Pos2> = (0..=20)
        .map(|k| {
            let a = debut + k as f32 / 20.0 * std::f32::consts::PI * 1.5;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    let painter = ui.painter();
    painter.circle_stroke(c, r, Stroke::new(2.0, couleur.gamma_multiply(0.2)));
    painter.add(Shape::line(points, Stroke::new(2.0, couleur)));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    resp
}

/// Bouton « ⋯ » discret ouvrant un menu.
pub fn bouton_menu(ui: &mut Ui, infobulle: &str) -> Response {
    let p = pal(ui);
    let d = theme::HAUTEUR_PETIT + 2.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(d, d), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, infobulle));
    let survol = resp.hovered();
    let painter = ui.painter();
    if survol {
        painter.rect_filled(rect, CornerRadius::same(8), p.survol);
    }
    // Trois points dessinés : indépendant des polices.
    for dx in [-6.0, 0.0, 6.0] {
        painter.circle_filled(rect.center() + vec2(dx, 0.0), 2.2, if survol { p.accent_texte } else { p.texte_2 });
    }
    if resp.has_focus() {
        painter.rect_stroke(rect.expand(2.0), CornerRadius::same(10), Stroke::new(2.0, p.accent_texte), StrokeKind::Outside);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(infobulle)
}

/// Élément de menu contextuel (pleine largeur, 34 px).
pub fn element_menu(ui: &mut Ui, icone: &str, texte: &str, rouge: bool) -> Response {
    let p = pal(ui);
    let largeur = ui.available_width().max(220.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(largeur, 34.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, texte));
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(6), if rouge { p.danger_doux } else { p.survol });
    }
    let couleur = if rouge { p.danger_texte } else { p.texte };
    painter.text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, icone, theme::police(15.0), if rouge { p.danger_texte } else { p.texte_2 });
    painter.text(rect.left_center() + vec2(38.0, 0.0), Align2::LEFT_CENTER, texte, theme::police(theme::TAILLE_CORPS), couleur);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Bouton de navigation de la barre latérale.
pub fn bouton_nav(ui: &mut Ui, icone: &str, texte: &str, choisi: bool) -> Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, choisi, texte));
    let painter = ui.painter();
    let survol = resp.hovered();
    if choisi {
        painter.rect_filled(rect, CornerRadius::same(10), p.accent_doux);
        let barre = Rect::from_min_size(rect.left_top() + vec2(0.0, 10.0), vec2(4.0, rect.height() - 20.0));
        painter.rect_filled(barre, CornerRadius::same(2), p.accent_texte);
    } else if survol {
        painter.rect_filled(rect, CornerRadius::same(10), p.survol);
    }
    let couleur = if choisi { p.accent_texte } else { p.texte };
    painter.text(rect.left_center() + vec2(26.0, 0.0), Align2::CENTER_CENTER, icone, theme::police(18.0), if choisi { p.accent_texte } else { p.texte_2 });
    let police = if choisi { theme::police_forte(15.5) } else { theme::police(15.5) };
    painter.text(rect.left_center() + vec2(48.0, 0.0), Align2::LEFT_CENTER, texte, police, couleur);
    if resp.has_focus() {
        painter.rect_stroke(rect, CornerRadius::same(10), Stroke::new(2.0, p.accent_texte), StrokeKind::Inside);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Gros bouton rond d'enregistrement (rouge ; carré blanc pendant l'enregistrement).
pub fn bouton_enregistrer(ui: &mut Ui, en_cours: bool, actif: bool, t: f64) -> Response {
    let p = pal(ui);
    let d = 76.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(d + 16.0, d + 16.0), if actif { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, actif, if en_cours { "Arrêter l'enregistrement" } else { "Enregistrer" })
    });
    let c = rect.center();
    let survol = actif && resp.hovered();
    let painter = ui.painter();
    if en_cours {
        // Halo pulsé pendant l'enregistrement.
        let phase = ((t * 2.0).sin() * 0.5 + 0.5) as f32;
        painter.circle_filled(c, d / 2.0 + 4.0 + 4.0 * phase, p.rec.gamma_multiply(0.18 + 0.12 * phase));
    } else if survol {
        painter.circle_filled(c, d / 2.0 + 6.0, p.rec.gamma_multiply(0.16));
    }
    let fond = if !actif { p.inactif_fond } else if survol { p.rec_survol } else { p.rec };
    painter.circle_filled(c, d / 2.0, fond);
    painter.circle_stroke(c, d / 2.0 - 1.0, Stroke::new(2.0, Color32::from_white_alpha(60)));
    if en_cours {
        painter.rect_filled(Rect::from_center_size(c, vec2(24.0, 24.0)), CornerRadius::same(4), Color32::WHITE);
    } else {
        painter.circle_filled(c, 13.0, if actif { Color32::WHITE } else { p.inactif_texte });
    }
    if resp.has_focus() {
        painter.circle_stroke(c, d / 2.0 + 5.0, Stroke::new(2.0, p.accent_texte));
    }
    let resp = if actif { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp };
    resp.on_hover_text(if en_cours { "Arrêter l'enregistrement" } else { "Commencer l'enregistrement" })
}

// ------------------------------------------------------------------ champs

/// Champ de texte d'une ligne, 36 px de haut.
pub fn champ(ui: &mut Ui, valeur: &mut String, indice: &str, largeur: f32) -> Response {
    ui.add(
        egui::TextEdit::singleline(valeur)
            .hint_text(RichText::new(indice).color(pal(ui).texte_3))
            .desired_width(largeur)
            .margin(Margin::symmetric(10, 8))
            .font(theme::police(theme::TAILLE_CORPS)),
    )
}

/// Curseur horizontal dessiné (rail arrondi, poignée ronde), pilotable au clavier.
pub fn curseur(ui: &mut Ui, valeur: &mut f32, min: f32, max: f32, pas: f32, largeur: f32) -> Response {
    let p = pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(largeur, 32.0), Sense::click_and_drag());
    let v = *valeur as f64;
    resp.widget_info(|| egui::WidgetInfo::slider(true, v, ""));
    let x0 = rect.left() + 11.0;
    let x1 = rect.right() - 11.0;
    let arrondir = |v: f32| ((v / pas).round() * pas).clamp(min, max);
    let mut nouvelle = *valeur;
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on() {
            let t = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            nouvelle = arrondir(min + t * (max - min));
        }
    }
    if resp.has_focus() {
        let (g, d) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp),
            )
        });
        if g {
            nouvelle = arrondir(nouvelle - pas);
        }
        if d {
            nouvelle = arrondir(nouvelle + pas);
        }
    }
    if (nouvelle - *valeur).abs() > f32::EPSILON {
        *valeur = nouvelle;
        resp.mark_changed();
    }
    let t = ((*valeur - min) / (max - min)).clamp(0.0, 1.0);
    let xc = x0 + t * (x1 - x0);
    let cy = rect.center().y;
    let painter = ui.painter();
    let rail = Rect::from_min_max(pos2(x0, cy - 3.0), pos2(x1, cy + 3.0));
    painter.rect_filled(rail, CornerRadius::same(3), p.controle);
    painter.rect_filled(Rect::from_min_max(rail.min, pos2(xc, rail.max.y)), CornerRadius::same(3), p.accent);
    let actif = resp.hovered() || resp.dragged();
    let r = if actif { 10.5 } else { 9.5 };
    painter.circle_filled(pos2(xc, cy + 1.0), r + 1.0, p.ombre_forte);
    painter.circle(pos2(xc, cy), r, p.surface, Stroke::new(2.5, p.accent));
    if resp.has_focus() {
        painter.circle_stroke(pos2(xc, cy), r + 4.0, Stroke::new(2.0, p.accent_texte));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

// ------------------------------------------------------------------ indicateurs

/// Barre de progression arrondie ; `fraction` None = indéterminée (animée).
pub fn progression(ui: &mut Ui, fraction: Option<f32>) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), p.controle);
    match fraction {
        Some(f) => {
            let f = f.clamp(0.0, 1.0);
            if f > 0.0 {
                let r = Rect::from_min_size(rect.min, vec2((rect.width() * f).max(8.0), rect.height()));
                painter.rect_filled(r, CornerRadius::same(4), p.accent);
            }
        }
        None => {
            let t = ui.input(|i| i.time) as f32;
            let l = rect.width() * 0.3;
            let x = rect.left() + ((t * 0.6).fract() * (rect.width() + l)) - l;
            let r = Rect::from_min_max(pos2(x.max(rect.left()), rect.top()), pos2((x + l).min(rect.right()), rect.bottom()));
            if r.width() > 0.0 {
                painter.rect_filled(r, CornerRadius::same(4), p.accent);
            }
            ui.ctx().request_repaint();
        }
    }
}

/// Bornes du vumètre (dBFS).
pub const VU_MIN: f32 = -60.0;
pub const VU_ORANGE: f32 = -12.0;
pub const VU_ROUGE: f32 = -4.0;

/// Vumètre horizontal segmenté : `db` niveau courant, `pic` pic retenu (dBFS).
pub fn vumetre(ui: &mut Ui, db: f32, pic: f32, largeur: f32) -> Response {
    let p = pal(ui);
    let hauteur_barre = 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(largeur, hauteur_barre + 18.0), Sense::hover());
    let barre = Rect::from_min_size(rect.min, vec2(largeur, hauteur_barre));
    let painter = ui.painter();
    let n = 36usize;
    let ecart = 3.0;
    let w = (barre.width() - ecart * (n as f32 - 1.0)) / n as f32;
    let pas = -VU_MIN / n as f32;
    let couleur_zone = |haut: f32| {
        if haut > VU_ROUGE {
            p.vu_rouge
        } else if haut > VU_ORANGE {
            p.vu_orange
        } else {
            p.vu_vert
        }
    };
    let idx_pic = (((pic - VU_MIN) / pas).floor() as isize).clamp(-1, n as isize - 1);
    for i in 0..n {
        let bas = VU_MIN + i as f32 * pas;
        let haut = bas + pas;
        let x = barre.left() + i as f32 * (w + ecart);
        let seg = Rect::from_min_size(pos2(x, barre.top()), vec2(w, hauteur_barre));
        let allume = db > bas + 0.01;
        let couleur = if allume || i as isize == idx_pic {
            couleur_zone(haut)
        } else {
            // Segment éteint, légèrement teinté de sa zone pour montrer la graduation.
            p.vu_eteint
        };
        painter.rect_filled(seg, CornerRadius::same(2), couleur);
    }
    // Graduations.
    for g in [-48.0f32, -36.0, -24.0, -12.0, 0.0] {
        let x = barre.left() + (g - VU_MIN) / -VU_MIN * barre.width();
        let align = if g == 0.0 { Align2::RIGHT_TOP } else { Align2::CENTER_TOP };
        let texte = if g == 0.0 { "0 dB".to_string() } else { format!("−{}", -g as i32) };
        painter.text(pos2(x, barre.bottom() + 3.0), align, texte, theme::police(11.5), p.texte_2);
    }
    resp
}

/// Jauge « N / max » : cercles pleins (cochés) puis vides ; « +k » si dépassement.
pub fn jauge(ui: &mut Ui, n: usize, max: usize) -> Response {
    let p = pal(ui);
    let r = 12.0;
    let ecart = 8.0;
    let largeur = max as f32 * (2.0 * r + ecart) - ecart;
    let (rect, resp) = ui.allocate_exact_size(vec2(largeur, 2.0 * r + 4.0), Sense::hover());
    let painter = ui.painter();
    for i in 0..max {
        let c = pos2(rect.left() + r + i as f32 * (2.0 * r + ecart), rect.center().y);
        if i < n {
            painter.circle_filled(c, r, p.accent);
            let coche = vec![c + vec2(-5.0, 0.5), c + vec2(-1.5, 4.0), c + vec2(5.5, -3.5)];
            painter.add(Shape::line(coche, Stroke::new(2.2, Color32::WHITE)));
        } else {
            painter.circle_stroke(c, r - 1.0, Stroke::new(2.0, p.bord_controle));
        }
    }
    resp
}

/// Durée lisible : « 45 s », « 1 min 05 s », « 1 h 02 min ».
pub fn duree_lisible(s: f32) -> String {
    let s = s.max(0.0).round() as u64;
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min {:02} s", s / 60, s % 60)
    } else {
        format!("{} h {:02} min", s / 3600, (s % 3600) / 60)
    }
}

/// Chronomètre mm:ss.
pub fn chrono(s: f32) -> String {
    let s = s.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Nombre décimal à la française (« 0,70 »).
pub fn decimal(v: f32, chiffres: usize) -> String {
    format!("{v:.chiffres$}").replace('.', ",")
}

/// Nombre entier avec espace fine des milliers (« 1 234 »).
pub fn entier(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202F}');
        }
        out.push(c);
    }
    out
}

// ------------------------------------------------------------------ défilement

/// Hauteur du fondu signalant une suite au-dessus ou au-dessous d'une zone défilante.
const FONDU: f32 = 24.0;

/// Affiche `zone` puis dessine un fondu (couleur `fond` → transparent) en bas s'il reste du
/// contenu plus bas, et en haut si l'on a déjà défilé : une ligne coupée à mi-hauteur se lit
/// alors comme « il y a une suite », et non comme une erreur de mise en page.
/// `creux` : la zone est posée sur un fond « en creux » (zone de texte) plutôt que sur le fond de page.
pub fn defiler<R>(ui: &mut Ui, zone: egui::ScrollArea, creux: bool, contenu: impl FnOnce(&mut Ui) -> R) -> R {
    let fond = if creux { pal(ui).creux } else { pal(ui).fond };
    let sortie = zone.show(ui, contenu);
    let r = sortie.inner_rect;
    let decalage = sortie.state.offset.y;
    let reste = sortie.content_size.y - r.height() - decalage;
    // On épargne la barre de défilement, à droite.
    let largeur = (r.width() - ui.spacing().scroll.allocated_width() - 2.0).max(0.0);
    let painter = ui.painter().with_clip_rect(r);
    if reste > 1.0 {
        let h = FONDU.min(reste + 4.0);
        let bande = Rect::from_min_size(pos2(r.left(), r.bottom() - h), vec2(largeur, h));
        degrade(&painter, bande, Color32::TRANSPARENT, fond);
    }
    if decalage > 1.0 {
        let h = FONDU.min(decalage + 4.0) * 0.6;
        let bande = Rect::from_min_size(r.min, vec2(largeur, h));
        degrade(&painter, bande, fond, Color32::TRANSPARENT);
    }
    sortie.inner
}

/// Dégradé vertical de `haut` à `bas` sur `rect`.
fn degrade(painter: &egui::Painter, rect: Rect, haut: Color32, bas: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), haut);
    mesh.colored_vertex(rect.right_top(), haut);
    mesh.colored_vertex(rect.left_bottom(), bas);
    mesh.colored_vertex(rect.right_bottom(), bas);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(Shape::mesh(mesh));
}
