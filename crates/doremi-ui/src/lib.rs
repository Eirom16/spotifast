//! Presentation primitives extracted from Spotifast's real theme and sidebar.
//! Copyright (c) Carmine Paolino. MIT; see the repository LICENSE.
//! No catalog, player, filesystem, processes, or platform services run while drawing.
use egui::{Color32, CornerRadius, Rect, Response, Sense, Stroke, Vec2, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Palette {
    pub dark: bool,
    pub window: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_active: Color32,
    pub outline: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub overlay: Color32,
    pub shadow: Color32,
    /// The colour a page's header fades from when album art does not tint
    /// it; its alpha is the strength. Transparent leaves the plain window.
    #[serde(default)]
    pub tint: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            window: Color32::from_rgb(0x0f, 0x11, 0x14),
            panel: Color32::from_rgb(0x15, 0x18, 0x1c),
            surface: Color32::from_rgb(0x1d, 0x21, 0x27),
            surface_hover: Color32::from_rgb(0x26, 0x2b, 0x33),
            surface_active: Color32::from_rgb(0x2f, 0x35, 0x3f),
            outline: Color32::from_rgb(0x2a, 0x30, 0x38),
            text: Color32::from_rgb(0xf2, 0xf4, 0xf6),
            secondary: Color32::from_rgb(0xa9, 0xb1, 0xbc),
            dim: Color32::from_rgb(0x6e, 0x77, 0x84),
            accent: Color32::from_rgb(0x1e, 0xd7, 0x60),
            accent_hover: Color32::from_rgb(0x3c, 0xe8, 0x7a),
            on_accent: Color32::from_rgb(0x0a, 0x14, 0x0e),
            danger: Color32::from_rgb(0xf5, 0x71, 0x7f),
            warning: Color32::from_rgb(0xf2, 0xb8, 0x5c),
            overlay: Color32::from_rgb(0x22, 0x27, 0x2e),
            shadow: Color32::from_black_alpha(140),
            tint: Color32::TRANSPARENT,
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            window: Color32::from_rgb(0xf8, 0xf9, 0xfb),
            panel: Color32::from_rgb(0xff, 0xff, 0xff),
            surface: Color32::from_rgb(0xee, 0xf0, 0xf3),
            surface_hover: Color32::from_rgb(0xe3, 0xe6, 0xeb),
            surface_active: Color32::from_rgb(0xd7, 0xdb, 0xe1),
            outline: Color32::from_rgb(0xdd, 0xe1, 0xe6),
            text: Color32::from_rgb(0x14, 0x17, 0x1a),
            secondary: Color32::from_rgb(0x53, 0x5b, 0x66),
            dim: Color32::from_rgb(0x8b, 0x93, 0x9e),
            accent: Color32::from_rgb(0x15, 0xa6, 0x4a),
            accent_hover: Color32::from_rgb(0x12, 0x8f, 0x40),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(0xd6, 0x3b, 0x4c),
            warning: Color32::from_rgb(0xb8, 0x7a, 0x14),
            overlay: Color32::from_rgb(0xff, 0xff, 0xff),
            shadow: Color32::from_black_alpha(50),
            tint: Color32::TRANSPARENT,
        }
    }

    /// A colour derived from album art, softened so it can sit behind text.
    pub fn tint_from_art(&self, rgb: [u8; 3]) -> Color32 {
        let [r, g, b] = rgb.map(|c| c as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.0;
        let target = if self.dark { 0.30 } else { 0.72 };
        let (r, g, b) = if lightness < 0.01 {
            (target, target, target)
        } else {
            let scale = target / lightness;
            (
                (r * scale).min(1.0),
                (g * scale).min(1.0),
                (b * scale).min(1.0),
            )
        };
        Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
    }
}

#[cfg(feature = "theme-files")]
impl fastframe_theme::Palette for Palette {
    fn base(base: fastframe_theme::Base) -> Self {
        match base {
            fastframe_theme::Base::Dark => Self::dark(),
            fastframe_theme::Base::Light => Self::light(),
        }
    }

    fn set(&mut self, name: &str, color: Color32) -> bool {
        match name {
            "window" => self.window = color,
            "panel" => self.panel = color,
            "surface" => self.surface = color,
            "surface_hover" => self.surface_hover = color,
            "surface_active" => self.surface_active = color,
            "outline" => self.outline = color,
            "text" => self.text = color,
            "secondary" => self.secondary = color,
            "dim" => self.dim = color,
            "accent" => self.accent = color,
            "accent_hover" => self.accent_hover = color,
            "on_accent" => self.on_accent = color,
            "danger" => self.danger = color,
            "warning" => self.warning = color,
            "overlay" => self.overlay = color,
            "shadow" => self.shadow = color,
            "tint" => self.tint = color,
            _ => return false,
        }
        true
    }

    /// Menus, tooltips and dialogs follow a file's window when it leaves
    /// `overlay` out, as Omarchy's template and the shared palettes do:
    /// otherwise they kept Spotifast's own blue-grey on every other theme.
    /// The proportions are the built-in palettes': a step above the window
    /// toward the text in dark ones, the panel in light ones.
    fn derive(&mut self, given: &std::collections::BTreeSet<&str>) {
        if given.contains("overlay") || !given.contains("window") {
            return;
        }
        self.overlay = if self.dark {
            self.window.lerp_to_gamma(self.text, 0.08)
        } else {
            self.panel
        };
    }
}

#[cfg(feature = "theme-files")]
impl fastframe_theme::PaletteBase for Palette {
    fn palette_base(&self) -> fastframe_theme::Base {
        fastframe_theme::Base::from_dark(self.dark)
    }
}

pub const RADIUS: u8 = 8;
pub const RADIUS_SMALL: u8 = 4;
pub const ROW_HEIGHT: f32 = 56.0;
pub const COMPACT_ROW_HEIGHT: f32 = 48.0;
/// The compact track list: one line, no cover.
pub const THIN_ROW_HEIGHT: f32 = 36.0;
pub const PLAYER_BAR_HEIGHT: f32 = 88.0;
/// The narrowest either right-hand panel goes. The queue and the lyrics
/// take the same edge and swap places there, so a width that suits one
/// has to suit the other, or the window would jump on the swap.
pub const SIDE_PANEL_MIN_WIDTH: f32 = 280.0;
pub const TOP_BAR_HEIGHT: f32 = 56.0;
pub const CUSTOM_TOP_BAR_HEIGHT: f32 = 48.0;

pub fn regular(size: f32) -> egui::FontId {
    fastframe_fonts::Weight::Regular.font_id(size)
}

pub fn medium(size: f32) -> egui::FontId {
    fastframe_fonts::Weight::Medium.font_id(size)
}

pub fn semibold(size: f32) -> egui::FontId {
    fastframe_fonts::Weight::SemiBold.font_id(size)
}

pub fn bold(size: f32) -> egui::FontId {
    fastframe_fonts::Weight::Bold.font_id(size)
}

pub fn apply_to_style(style: &mut egui::Style, palette: &Palette) {
    let visuals = &mut style.visuals;
    *visuals = if palette.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = palette.dark;
    // Glyph coverage, hinting and sub-pixel positions as the desktop draws
    // them: linear coverage in both themes on Linux, where egui's dark curve
    // (2c - c²) made text heavier than GTK's.

    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.overlay;
    visuals.extreme_bg_color = palette.surface;
    visuals.faint_bg_color = palette.surface;
    visuals.code_bg_color = palette.surface;
    visuals.override_text_color = Some(palette.text);
    visuals.weak_text_color = Some(palette.secondary);
    visuals.hyperlink_color = palette.text;
    visuals.selection.bg_fill = palette.accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.window_stroke = Stroke::new(1.0, palette.outline);
    visuals.window_corner_radius = CornerRadius::same(RADIUS + 2);
    visuals.menu_corner_radius = CornerRadius::same(RADIUS);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 24,
        spread: 0,
        color: palette.shadow,
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: palette.shadow,
    };
    let corner = CornerRadius::same(RADIUS_SMALL + 2);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = corner;
        widget.bg_stroke = Stroke::NONE;
        widget.fg_stroke = Stroke::new(1.0, palette.text);
        widget.expansion = 0.0;
    }
    visuals.widgets.noninteractive.corner_radius = corner;
    visuals.widgets.noninteractive.bg_fill = palette.panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.outline);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.bg_fill = palette.surface;
    visuals.widgets.inactive.weak_bg_fill = palette.surface;
    visuals.widgets.hovered.bg_fill = palette.surface_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.surface_hover;
    visuals.widgets.active.bg_fill = palette.surface_active;
    visuals.widgets.active.weak_bg_fill = palette.surface_active;
    visuals.widgets.open.bg_fill = palette.surface_hover;
    visuals.widgets.open.weak_bg_fill = palette.surface_hover;
    visuals.text_cursor.stroke = Stroke::new(2.0, palette.accent);
    visuals.striped = false;
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;

    use egui::FontFamily::{Monospace, Proportional};
    use egui::{FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.5, Proportional)),
        (TextStyle::Body, FontId::new(14.0, Proportional)),
        (TextStyle::Button, FontId::new(14.0, Proportional)),
        (TextStyle::Heading, FontId::new(22.0, Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, Monospace)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.interact_size = Vec2::new(40.0, 28.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: 8.0,
        floating_width: 6.0,
        floating_allocated_width: 0.0,
        handle_min_length: 28.0,
        bar_inner_margin: 3.0,
        bar_outer_margin: 2.0,
        dormant_background_opacity: 0.0,
        dormant_handle_opacity: 0.0,
        active_background_opacity: 0.0,
        active_handle_opacity: 0.55,
        interact_handle_opacity: 0.85,
        foreground_color: true,
        ..egui::style::ScrollStyle::floating()
    };
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.4;
    style.animation_time = 0.12;
    style.url_in_tooltip = false;
}

fastframe_icons::icons! {
    /// Every icon the interface draws. The shared Lucide icons come from
    /// fastframe-icons; the rest are Spotifast's own files.
    pub enum Icon {
        prefix: "spotifast-icon-",
        directory: "../../../assets/icons/",
        ArrowLeft => lucide "arrow-left",
        ArrowRight => "arrow-right",
        AudioLines => "audio-lines",
        BadgeCheck => "badge-check",
        Bookmark => "bookmark",
        BookmarkFilled => "bookmark-filled",
        Car => "car",
        Cast => "cast",
        Check => lucide "check",
        ChevronDown => lucide "chevron-down",
        ChevronLeft => lucide "chevron-left",
        ChevronRight => lucide "chevron-right",
        ChevronUp => lucide "chevron-up",
        CircleAlert => lucide "circle-alert",
        CircleCheck => lucide "circle-check",
        CirclePlay => "circle-play",
        CirclePlus => "circle-plus",
        CircleX => lucide "circle-x",
        Clock => lucide "clock",
        Compass => "compass",
        Copy => lucide "copy",
        Disc => "disc-3",
        Ellipsis => lucide "ellipsis",
        Expand => "expand",
        Eye => "eye",
        EyeOff => "eye-off",
        ExternalLink => lucide "external-link",
        Gamepad => "gamepad-2",
        Globe => "globe",
        GripVertical => "grip-vertical",
        Headphones => "headphones",
        Heart => "heart",
        HeartFilled => "heart-filled",
        House => "house",
        Info => lucide "info",
        Laptop => "laptop",
        Library => "library",
        LayoutGrid => "layout-grid",
        LayoutList => "layout-list",
        ListEnd => "list-end",
        ListMusic => "list-music",
        ListPlus => "list-plus",
        ListVideo => "list-video",
        Loader => "loader-circle",
        Lock => lucide "lock",
        LogOut => lucide "log-out",
        Mic => lucide "mic",
        Minus => lucide "minus",
        Monitor => lucide "monitor",
        Moon => lucide "moon",
        Music => "music",
        Pause => lucide "pause",
        PauseFilled => "pause-filled",
        PanelLeft => lucide "panel-left",
        Pin => lucide "pin",
        PinOff => lucide "pin-off",
        Pencil => lucide "pencil",
        Play => lucide "play",
        PlayFilled => "play-filled",
        Plus => lucide "plus",
        Radio => "radio",
        Refresh => lucide "refresh-cw",
        Repeat => "repeat",
        Repeat1 => "repeat-1",
        Search => lucide "search",
        Settings => lucide "settings",
        Shrink => "shrink",
        Shuffle => "shuffle",
        SkipBack => "skip-back",
        SkipBackFilled => "skip-back-filled",
        SkipForward => "skip-forward",
        SkipForwardFilled => "skip-forward-filled",
        Smartphone => lucide "smartphone",
        Sparkles => "sparkles",
        Speaker => "speaker",
        Square => "square",
        SquarePen => lucide "square-pen",
        Sun => lucide "sun",
        Tablet => "tablet",
        Trash => lucide "trash-2",
        TrendingUp => "trending-up",
        Tv => "tv",
        User => lucide "user",
        Users => lucide "users",
        Volume => "volume",
        Volume1 => "volume-1",
        Volume2 => lucide "volume-2",
        VolumeX => lucide "volume-x",
        Watch => "watch",
        WifiOff => "wifi-off",
        X => lucide "x",
        Zap => "zap",
    }
}

/// A static icon.
pub fn icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> Response {
    ui.add(icon.image(color, size))
}

/// Paints an icon centred in `rect` without allocating space.
pub fn paint_icon(ui: &egui::Ui, icon: Icon, rect: egui::Rect, size: f32, color: Color32) {
    let icon_rect = egui::Rect::from_center_size(
        rect.center() + play_glyph_offset(icon, size),
        Vec2::splat(size),
    );
    icon.image(color, size).paint_at(ui, icon_rect);
}

/// Make keyboard focus visible without changing the control's layout.
pub fn focus_ring(ui: &egui::Ui, response: &Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            4.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Outside,
        );
    }
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
}

/// The padding [`icon_button`] keeps around its glyph on every side. A
/// caller that places an icon button inside a fixed box must give the box
/// at least `size + ICON_BUTTON_PAD`: in a tighter one, egui clamps the
/// centered layout to the box's top edge and the glyph lands off-center.
pub const ICON_BUTTON_PAD: f32 = 12.0;

/// A frameless icon control whose colour lifts on hover.
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    color: Color32,
    hover: Color32,
    tooltip: &str,
) -> Response {
    let edge = size + ICON_BUTTON_PAD;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(edge), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tooltip)
    });
    if ui.is_rect_visible(rect) {
        let tint = if response.hovered() || response.has_focus() {
            hover
        } else {
            color
        };
        let scale = if response.is_pointer_button_down_on() {
            0.92
        } else {
            1.0
        };
        paint_icon(ui, icon, rect, size * scale, tint);
    }
    focus_ring(ui, &response);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// Horizontal offset that optically centers play triangles.
///
/// Lucide includes a 1/24-width shift; a measured 3% shift centers the icon at
/// Spotifast's sizes. Use this everywhere instead of per-call adjustments.
pub fn play_glyph_offset(icon: Icon, icon_size: f32) -> Vec2 {
    if matches!(icon, Icon::PlayFilled | Icon::Play) {
        Vec2::new(icon_size * (0.03 - 1.0 / 24.0), 0.0)
    } else {
        Vec2::ZERO
    }
}

pub fn circle_button(
    ui: &mut egui::Ui,
    icon: Icon,
    diameter: f32,
    fill: Color32,
    fill_hover: Color32,
    icon_color: Color32,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tooltip)
    });
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let grow = if hovered { 1.05 } else { 1.0 };
        let radius = diameter / 2.0 * grow;
        let fill = if hovered { fill_hover } else { fill };
        ui.painter().circle_filled(rect.center(), radius, fill);
        let icon_size = diameter * 0.46;
        let offset = play_glyph_offset(icon, icon_size);
        let icon_rect =
            egui::Rect::from_center_size(rect.center() + offset, Vec2::splat(icon_size));
        icon.image(icon_color, icon_size).paint_at(ui, icon_rect);
    }
    focus_ring(ui, &response);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A pill-shaped text button: filled for the primary action, outlined otherwise.
pub fn pill_button(ui: &mut egui::Ui, palette: &Palette, label: &str, primary: bool) -> Response {
    let font = semibold(13.0);
    let color = if primary {
        palette.on_accent
    } else {
        palette.text
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, color);
    let padding = Vec2::new(18.0, 8.0);
    let size = galley.size() + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let radius = rect.height() / 2.0;
        if primary {
            let fill = if hovered {
                palette.accent_hover
            } else {
                palette.accent
            };
            ui.painter().rect_filled(rect, radius, fill);
        } else {
            let stroke_color = if hovered { palette.text } else { palette.dim };
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, stroke_color),
                egui::StrokeKind::Inside,
            );
        }
        let pos = rect.center() - galley.size() / 2.0;
        ui.painter().galley(pos, galley, color);
    }
    focus_ring(ui, &response);
    response
}

pub fn nav_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Icon,
    label: &str,
    active: bool,
    height: f32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if ui.is_rect_visible(rect) {
        let color = if active || response.hovered() {
            palette.text
        } else {
            palette.secondary
        };
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), Vec2::splat(22.0));
        icon.image(color, 22.0).paint_at(ui, icon_rect);
        ui.painter().text(
            pos2(rect.left() + 46.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            bold(15.0),
            color,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), active, label)
    });
    focus_ring(ui, &response);
    response
}

/// Install the same Inter weights and Lucide/Spotifast artwork as the legacy UI.
/// Call once during startup, before the first frame.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fastframe_fonts::FontSetup::default().definitions());
    egui_extras::install_image_loaders(ctx);
    fastframe_icons::install::<Icon>(ctx);
}

/// Product accent over the inherited light/dark palette; green keeps its original values.
pub fn palette(light: bool, green: bool) -> Palette {
    let mut palette = if light {
        Palette::light()
    } else {
        Palette::dark()
    };
    if !green {
        palette.accent = Color32::from_rgb(0x99, 0x66, 0xe8);
        palette.accent_hover = Color32::from_rgb(0xad, 0x82, 0xed);
        palette.on_accent = Color32::from_rgb(0x14, 0x0d, 0x22);
    }
    palette
}

/// The original bottom player panel, shared without any player or Spotify types.
pub fn player_bar<R>(
    ui: &mut egui::Ui,
    fill: Color32,
    height: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Panel::bottom("player-bar")
        .exact_size(height)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(16, 0)),
        )
        .show(ui, content)
}

/// Original artwork fallback; never invent an image for an unknown release.
pub fn cover_placeholder(
    ui: &egui::Ui,
    palette: &Palette,
    rect: Rect,
    radius: f32,
    fallback: Icon,
) {
    let corner = CornerRadius::same(radius.min(127.0) as u8);
    let painter = ui.painter();
    let fill = if palette.dark {
        palette.surface_hover
    } else {
        palette.surface_active
    };
    if radius >= rect.width() / 2.0 - 0.5 {
        painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    } else {
        painter.rect_filled(rect, corner, fill);
    }
    let icon_size = (rect.width() * 0.42).clamp(12.0, 64.0);
    paint_icon(ui, fallback, rect, icon_size, palette.dim);
}

use egui::Ui;

pub enum SliderEvent {
    None,
    Dragging(f32),
    Committed(f32),
}

/// Whole notches the wheel turned over `response` since last asked, up
/// being positive. A mouse's detent is one event, however many lines the
/// system multiplies it into (Windows says three by default, #103); a
/// free-spinning wheel's fractional lines and a trackpad's points add up
/// to the same steps, fifty points to a notch.
pub fn wheel_notches(ui: &Ui, response: &egui::Response) -> i32 {
    const NOTCH: f32 = 50.0;
    if !response.hovered() {
        return 0;
    }
    let (lines, points) = ui.input(|input| {
        let mut lines = 0.0f32;
        let mut points = 0.0f32;
        for event in &input.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = event {
                match unit {
                    egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => {
                        lines += if delta.y.abs() >= 1.0 {
                            delta.y.signum()
                        } else {
                            delta.y
                        };
                    }
                    egui::MouseWheelUnit::Point => points += delta.y,
                }
            }
        }
        (lines, points)
    });
    let id = response.id.with("wheel");
    let total = ui.data(|data| data.get_temp::<f32>(id)).unwrap_or(0.0) + points + lines * NOTCH;
    let notches = (total / NOTCH).trunc();
    ui.data_mut(|data| data.insert_temp(id, total - notches * NOTCH));
    notches as i32
}

/// A thin horizontal slider whose handle appears on hover, for seeking and
/// volume. `value` is 0..=1.
pub fn thin_slider(
    ui: &mut Ui,
    palette: &Palette,
    id: egui::Id,
    label: &str,
    value: f32,
    width: f32,
    wheel_step: Option<f32>,
) -> SliderEvent {
    thin_slider_sized(ui, palette, id, label, value, vec2(width, 16.0), wheel_step).0
}

/// Same slider and accessibility semantics, with a host-selected hit target.
pub fn thin_slider_sized(
    ui: &mut Ui,
    palette: &Palette,
    id: egui::Id,
    label: &str,
    value: f32,
    size: Vec2,
    wheel_step: Option<f32>,
) -> (SliderEvent, Response) {
    let (_, rect) = ui.allocate_space(size);
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let dragging_value = ui.data(|data| data.get_temp::<f32>(id));
    let pointer_value = response
        .interact_pointer_pos()
        .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
    let mut event = SliderEvent::None;
    // Preview on press, before egui's click-or-drag movement threshold.
    if (response.is_pointer_button_down_on() || response.drag_started() || response.dragged())
        && let Some(v) = pointer_value
    {
        ui.data_mut(|data| data.insert_temp(id, v));
        event = SliderEvent::Dragging(v);
    }
    if response.drag_stopped() {
        let v = dragging_value.or(pointer_value).unwrap_or(value);
        ui.data_mut(|data| data.remove::<f32>(id));
        event = SliderEvent::Committed(v);
    } else if response.clicked()
        && let Some(v) = pointer_value
    {
        ui.data_mut(|data| data.remove::<f32>(id));
        event = SliderEvent::Committed(v);
    }
    if let Some(step) = wheel_step
        && response.enabled()
    {
        let notches = wheel_notches(ui, &response);
        if notches != 0 {
            event = SliderEvent::Committed((value + step * notches as f32).clamp(0.0, 1.0));
        }
    }
    let step = wheel_step.unwrap_or(0.01);
    let focused = response.has_focus();
    if focused {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    if response.enabled() {
        ui.input(|input| {
            use egui::accesskit::{Action, ActionData};
            let mut change = input.num_accesskit_action_requests(response.id, Action::Increment)
                as i32
                - input.num_accesskit_action_requests(response.id, Action::Decrement) as i32;
            if focused {
                change += input.num_presses(egui::Key::ArrowRight) as i32
                    - input.num_presses(egui::Key::ArrowLeft) as i32;
            }
            if change != 0 {
                event = SliderEvent::Committed((value + change as f32 * step).clamp(0.0, 1.0));
            }
            for request in input.accesskit_action_requests(response.id, Action::SetValue) {
                if let Some(ActionData::NumericValue(value)) = request.data
                    && value.is_finite()
                {
                    event = SliderEvent::Committed((value / 100.0).clamp(0.0, 1.0) as f32);
                }
            }
        });
    }
    let shown = match &event {
        SliderEvent::Dragging(v) => *v,
        SliderEvent::Committed(v) => *v,
        SliderEvent::None => dragging_value.unwrap_or(value),
    };
    response
        .widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), f64::from(shown) * 100.0, label));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        use egui::accesskit::Action;
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(100.0);
        node.set_numeric_value_step(f64::from(step) * 100.0);
        node.add_action(Action::SetValue);
        if shown > 0.0 {
            node.add_action(Action::Decrement);
        }
        if shown < 1.0 {
            node.add_action(Action::Increment);
        }
    });
    if ui.is_rect_visible(rect) {
        let active = response.hovered()
            || response.has_focus()
            || response.dragged()
            || dragging_value.is_some();
        let bar = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
        let track_color = if palette.dark {
            Color32::from_white_alpha(50)
        } else {
            Color32::from_black_alpha(40)
        };
        ui.painter().rect_filled(bar, 2.0, track_color);
        let filled = Rect::from_min_max(
            bar.min,
            pos2(bar.left() + bar.width() * shown.clamp(0.0, 1.0), bar.max.y),
        );
        let fill = if active { palette.accent } else { palette.text };
        ui.painter().rect_filled(filled, 2.0, fill);
        if active {
            ui.painter()
                .circle_filled(pos2(filled.right(), bar.center().y), 6.0, palette.text);
        }
    }
    (event, response)
}
