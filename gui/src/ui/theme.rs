//! Thème de Vocalise : palettes claire et sombre, polices, espacements, styles egui.
//!
//! Toutes les couleurs de l'interface passent par [`Palette`] : aucun écran ne code de couleur
//! en dur, ce qui garantit la cohérence et le mode sombre.

use eframe::egui::{
    self, style::HandleShape, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Margin, Shadow, Stroke, TextStyle, Theme, Visuals,
};
use std::sync::Arc;

/// Rayon des cartes et des boutons.
pub const RAYON_CARTE: u8 = 12;
pub const RAYON_BOUTON: u8 = 9;
/// Marge intérieure des cartes.
pub const MARGE_CARTE: i8 = 18;
/// Écart vertical entre deux cartes.
pub const ECART: f32 = 16.0;
/// Hauteur des boutons principaux et secondaires.
pub const HAUTEUR_BOUTON: f32 = 38.0;
/// Hauteur des petits boutons (lignes de liste) : cible cliquable ≥ 32 px.
pub const HAUTEUR_PETIT: f32 = 32.0;

/// Tailles de la hiérarchie typographique.
pub const TAILLE_TITRE: f32 = 24.0;
pub const TAILLE_SOUS_TITRE: f32 = 17.0;
pub const TAILLE_CORPS: f32 = 15.0;
pub const TAILLE_SECONDAIRE: f32 = 13.0;
pub const TAILLE_LECTURE: f32 = 20.0;

/// Famille « demi-gras » pour les titres.
pub fn fort() -> FontFamily {
    FontFamily::Name("fort".into())
}

pub fn police(taille: f32) -> FontId {
    FontId::new(taille, FontFamily::Proportional)
}

pub fn police_forte(taille: f32) -> FontId {
    FontId::new(taille, fort())
}

/// Couleurs sémantiques de l'interface.
pub struct Palette {
    /// Fond de la zone de contenu.
    pub fond: Color32,
    /// Fond de la barre latérale.
    pub barre: Color32,
    /// Fond des cartes.
    pub surface: Color32,
    /// Fond « en creux » (zones de texte, listes).
    pub creux: Color32,
    pub bord: Color32,
    pub bord_controle: Color32,
    pub controle: Color32,
    pub survol: Color32,
    pub presse: Color32,

    pub texte: Color32,
    pub texte_2: Color32,
    pub texte_3: Color32,

    pub accent: Color32,
    pub accent_survol: Color32,
    pub accent_presse: Color32,
    /// Accent utilisable comme couleur de texte sur `surface`.
    pub accent_texte: Color32,
    /// Fond teinté d'accent (élément sélectionné).
    pub accent_doux: Color32,
    pub selection: Color32,

    pub succes: Color32,
    pub succes_doux: Color32,
    pub avert: Color32,
    pub avert_doux: Color32,
    /// Fond des boutons destructifs (texte blanc).
    pub danger: Color32,
    pub danger_survol: Color32,
    /// Rouge utilisable comme couleur de texte.
    pub danger_texte: Color32,
    pub danger_doux: Color32,
    /// Rouge du bouton d'enregistrement.
    pub rec: Color32,
    pub rec_survol: Color32,

    /// Désactivé.
    pub inactif_fond: Color32,
    pub inactif_texte: Color32,

    /// Zones du vumètre.
    pub vu_vert: Color32,
    pub vu_orange: Color32,
    pub vu_rouge: Color32,
    pub vu_eteint: Color32,

    pub ombre: Color32,
    pub ombre_forte: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const CLAIR: Palette = Palette {
    fond: hex(0xF3F5F9),
    barre: hex(0xE8ECF3),
    surface: hex(0xFFFFFF),
    creux: hex(0xF7F9FC),
    bord: hex(0xDDE3EC),
    bord_controle: hex(0xC9D1DE),
    controle: hex(0xE1E6EE),
    survol: hex(0xEEF2F8),
    presse: hex(0xE1E8F3),

    texte: hex(0x1B2333),
    texte_2: hex(0x535D71),
    texte_3: hex(0x6B7487),

    accent: hex(0x2F5A9E),
    accent_survol: hex(0x284E8A),
    accent_presse: hex(0x213F70),
    accent_texte: hex(0x2B4C7E),
    accent_doux: hex(0xDCE6F6),
    selection: hex(0xC7D8F3),

    succes: hex(0x1D7A46),
    succes_doux: hex(0xE2F3E9),
    avert: hex(0x9A5800),
    avert_doux: hex(0xFFF2D9),
    danger: hex(0xC0362C),
    danger_survol: hex(0xA52C23),
    danger_texte: hex(0xB3261E),
    danger_doux: hex(0xFBE7E5),
    rec: hex(0xD93A30),
    rec_survol: hex(0xBF2F26),

    inactif_fond: hex(0xD5DBE5),
    inactif_texte: hex(0x5F6880),

    vu_vert: hex(0x2E9E5B),
    vu_orange: hex(0xE59A1A),
    vu_rouge: hex(0xD93A30),
    vu_eteint: hex(0xE3E7EE),

    ombre: Color32::from_rgba_premultiplied(0, 0, 0, 16),
    ombre_forte: Color32::from_rgba_premultiplied(0, 0, 0, 40),
};

pub const SOMBRE: Palette = Palette {
    fond: hex(0x12151B),
    barre: hex(0x181C24),
    surface: hex(0x1F242E),
    creux: hex(0x171B22),
    bord: hex(0x2E3542),
    bord_controle: hex(0x3C4556),
    controle: hex(0x2C3340),
    survol: hex(0x272D39),
    presse: hex(0x2F3745),

    texte: hex(0xE7EAF0),
    texte_2: hex(0xA9B2C2),
    texte_3: hex(0x8C95A6),

    accent: hex(0x3A64A3),
    accent_survol: hex(0x4672B4),
    accent_presse: hex(0x30578F),
    accent_texte: hex(0x8FC4FF),
    accent_doux: hex(0x223452),
    selection: hex(0x2C4A7C),

    succes: hex(0x5FD39A),
    succes_doux: hex(0x17322A),
    avert: hex(0xF2B35A),
    avert_doux: hex(0x3A2D16),
    danger: hex(0xB9352C),
    danger_survol: hex(0xCC4237),
    danger_texte: hex(0xFF8A80),
    danger_doux: hex(0x3D1D1B),
    rec: hex(0xC93A30),
    rec_survol: hex(0xDD473C),

    inactif_fond: hex(0x2C3340),
    inactif_texte: hex(0x8C95A6),

    vu_vert: hex(0x3DBB72),
    vu_orange: hex(0xF0A32A),
    vu_rouge: hex(0xF0544A),
    vu_eteint: hex(0x2A303C),

    ombre: Color32::from_rgba_premultiplied(0, 0, 0, 70),
    ombre_forte: Color32::from_rgba_premultiplied(0, 0, 0, 120),
};

/// Palette correspondant au thème courant de `ui`.
pub fn pal(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &SOMBRE
    } else {
        &CLAIR
    }
}

pub fn pal_ctx(ctx: &egui::Context) -> &'static Palette {
    if ctx.style().visuals.dark_mode {
        &SOMBRE
    } else {
        &CLAIR
    }
}

/// Lit le schéma de couleurs de GNOME (utile quand le système de fenêtrage ne le transmet pas).
fn theme_gnome() -> Option<Theme> {
    let sortie = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&sortie.stdout);
    if s.contains("dark") {
        Some(Theme::Dark)
    } else if s.contains("light") || s.contains("default") {
        Some(Theme::Light)
    } else {
        None
    }
}

/// Installe polices et styles. `force` impose un thème (option `--theme`), sinon on suit le
/// système, avec repli sur le réglage GNOME puis sur le thème clair.
pub fn installer(ctx: &egui::Context, force: Option<Theme>) {
    installer_polices(ctx);
    ctx.set_style_of(Theme::Light, style(&CLAIR, false));
    ctx.set_style_of(Theme::Dark, style(&SOMBRE, true));
    let repli = theme_gnome().unwrap_or(Theme::Light);
    ctx.options_mut(|o| o.fallback_theme = repli);
    match force {
        Some(t) => ctx.set_theme(t),
        None => ctx.set_theme(egui::ThemePreference::System),
    }
}

fn lire_premier(chemins: &[&str]) -> Option<Vec<u8>> {
    chemins.iter().find_map(|c| std::fs::read(c).ok())
}

fn installer_polices(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let defauts = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let normal = lire_premier(&[
        "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]);
    let demi_gras = lire_premier(&[
        "/usr/share/fonts/opentype/inter/Inter-SemiBold.otf",
        "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    ]);
    // Couverture des symboles (▶ ■ ⋯ ✓ …) absents de la police principale.
    let symboles = lire_premier(&["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]);

    let mut prop = Vec::new();
    let mut gras = Vec::new();
    if let Some(b) = normal {
        defs.font_data.insert("vocalise-normal".into(), Arc::new(FontData::from_owned(b)));
        prop.push("vocalise-normal".to_string());
    }
    if let Some(b) = demi_gras {
        defs.font_data.insert("vocalise-fort".into(), Arc::new(FontData::from_owned(b)));
        gras.push("vocalise-fort".to_string());
    } else {
        gras.extend(prop.iter().cloned());
    }
    prop.extend(defauts.iter().cloned());
    gras.extend(defauts.iter().cloned());
    if let Some(b) = symboles {
        defs.font_data.insert("vocalise-symboles".into(), Arc::new(FontData::from_owned(b)));
        prop.push("vocalise-symboles".into());
        gras.push("vocalise-symboles".into());
    }
    defs.families.insert(FontFamily::Proportional, prop);
    defs.families.insert(fort(), gras);
    ctx.set_fonts(defs);
}

fn style(p: &Palette, sombre: bool) -> egui::Style {
    let mut s = egui::Style::default();
    s.text_styles = [
        (TextStyle::Heading, police_forte(TAILLE_TITRE)),
        (TextStyle::Body, police(TAILLE_CORPS)),
        (TextStyle::Button, police(TAILLE_CORPS)),
        (TextStyle::Small, police(TAILLE_SECONDAIRE)),
        (TextStyle::Monospace, FontId::monospace(13.5)),
    ]
    .into();

    let sp = &mut s.spacing;
    sp.item_spacing = egui::vec2(10.0, 10.0);
    sp.button_padding = egui::vec2(14.0, 7.0);
    sp.interact_size = egui::vec2(40.0, HAUTEUR_PETIT);
    sp.window_margin = Margin::same(22);
    sp.menu_margin = Margin::same(8);
    sp.combo_height = 360.0;
    sp.icon_width = 18.0;
    sp.icon_width_inner = 10.0;
    sp.icon_spacing = 8.0;
    sp.tooltip_width = 360.0;
    // Barre de défilement fine mais toujours visible dès qu'il y a une suite : l'utilisateur
    // voit qu'il peut faire défiler (en plus du fondu dessiné en bas des zones défilantes).
    sp.scroll = egui::style::ScrollStyle::thin();
    sp.scroll.bar_width = 8.0;
    sp.scroll.floating_allocated_width = 10.0;
    sp.scroll.dormant_background_opacity = 0.0;
    sp.scroll.active_background_opacity = 0.0;
    sp.scroll.interact_background_opacity = 0.15;
    sp.scroll.dormant_handle_opacity = 0.5;
    sp.scroll.active_handle_opacity = 0.7;
    sp.scroll.interact_handle_opacity = 0.85;

    s.visuals = visuels(p, sombre);
    s.interaction.tooltip_delay = 0.35;
    s.animation_time = 0.12;
    s
}

fn visuels(p: &Palette, sombre: bool) -> Visuals {
    let mut v = if sombre { Visuals::dark() } else { Visuals::light() };
    let r = CornerRadius::same(RAYON_BOUTON);

    v.panel_fill = p.fond;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.bord);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_shadow = Shadow { offset: [0, 10], blur: 32, spread: 0, color: p.ombre_forte };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: p.ombre_forte };
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.creux;
    v.faint_bg_color = p.creux;
    v.code_bg_color = p.creux;
    v.hyperlink_color = p.accent_texte;
    v.warn_fg_color = p.avert;
    v.error_fg_color = p.danger_texte;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.5, if sombre { p.accent_texte } else { p.accent });
    v.slider_trailing_fill = true;
    v.handle_shape = HandleShape::Circle;
    v.striped = false;
    v.indent_has_left_vline = false;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    v.text_cursor.stroke = Stroke::new(2.0, if sombre { p.accent_texte } else { p.accent });

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.bord);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.texte);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.controle;
    w.inactive.weak_bg_fill = p.surface;
    w.inactive.bg_stroke = Stroke::new(1.0, p.bord_controle);
    w.inactive.fg_stroke = Stroke::new(1.5, p.texte);
    w.inactive.corner_radius = r;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.bord_controle;
    w.hovered.weak_bg_fill = p.survol;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent_texte);
    w.hovered.fg_stroke = Stroke::new(1.5, p.texte);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.bord_controle;
    w.active.weak_bg_fill = p.presse;
    w.active.bg_stroke = Stroke::new(1.0, p.accent_texte);
    w.active.fg_stroke = Stroke::new(1.5, p.texte);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.controle;
    w.open.weak_bg_fill = p.survol;
    w.open.bg_stroke = Stroke::new(1.0, p.accent_texte);
    w.open.fg_stroke = Stroke::new(1.5, p.texte);
    w.open.corner_radius = r;
    v
}
