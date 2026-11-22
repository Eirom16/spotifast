//! Card geometry extracted from Spotifast's shelves. Hosts supply covers and
//! localized text painting, keeping network, RTL policy and navigation outside.
//! Copyright (c) Carmine Paolino. MIT; see the repository LICENSE.
use crate::{Icon, Palette};
use egui::{CornerRadius, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
pub const CARD_WIDTH: f32 = 172.0;
pub const CARD_ART: f32 = CARD_WIDTH - 24.0;
pub const CARD_GAP: f32 = 14.0;
pub struct CardResponse {
    pub response: Response,
    pub clicked: bool,
    pub play: bool,
}
#[derive(Clone, Copy, Default)]
pub struct CardCover {
    pub circular: bool,
    pub playable: bool,
    pub playing: bool,
}
impl CardCover {
    pub fn square(playing: bool) -> Self {
        Self {
            playable: true,
            playing,
            ..Self::default()
        }
    }
    pub fn portrait(playing: bool) -> Self {
        Self {
            circular: true,
            ..Self::square(playing)
        }
    }
}
pub struct Card<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub cover: CardCover,
    pub play_label: &'a str,
    pub play_size: f32,
}
pub enum Slot<'a> {
    Cover {
        rect: Rect,
        radius: f32,
    },
    Text {
        text: &'a str,
        rect: Rect,
        font: egui::FontId,
        color: egui::Color32,
        rows: usize,
    },
}
pub fn row_height(ui: &mut Ui) -> f32 {
    let (title, subtitle) = ui.fonts_mut(|fonts| {
        (
            fonts.row_height(&crate::semibold(14.0)),
            fonts.row_height(&crate::regular(12.5)),
        )
    });
    12.0 + CARD_ART + 10.0 + title + 2.0 + 2.0 * subtitle + 8.0
}
pub fn card(
    ui: &mut Ui,
    palette: &Palette,
    card: Card<'_>,
    slot: impl FnMut(&mut Ui, Slot<'_>),
) -> CardResponse {
    card_sized(ui, palette, card, CARD_WIDTH, slot)
}

/// Responsive catalog tiles use the same padding and typography as shelves.
pub fn card_sized(
    ui: &mut Ui,
    palette: &Palette,
    card: Card<'_>,
    width: f32,
    mut slot: impl FnMut(&mut Ui, Slot<'_>),
) -> CardResponse {
    let art = (width - 24.0).max(1.0);
    let (title_row, subtitle_row) = ui.fonts_mut(|fonts| {
        (
            fonts.row_height(&crate::semibold(14.0)),
            fonts.row_height(&crate::regular(12.5)),
        )
    });
    let height = 12.0 + art + 10.0 + title_row + 2.0 + 2.0 * subtitle_row + 8.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("{}, {}", card.title, card.subtitle),
        )
    });
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    let mut play = false;
    if ui.is_rect_visible(rect) {
        let hovered = ui.rect_contains_pointer(rect);
        if hovered {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(crate::RADIUS),
                palette
                    .surface_hover
                    .gamma_multiply(if palette.dark { 0.8 } else { 1.0 }),
            );
        }
        let image = Rect::from_min_size(rect.min + vec2(12.0, 12.0), Vec2::splat(art));
        slot(
            ui,
            Slot::Cover {
                rect: image,
                radius: if card.cover.circular { art / 2.0 } else { 6.0 },
            },
        );
        let title_rect = Rect::from_min_size(
            pos2(rect.left() + 12.0, image.bottom() + 10.0),
            vec2(art, title_row),
        );
        slot(
            ui,
            Slot::Text {
                text: card.title,
                rect: title_rect,
                font: crate::semibold(14.0),
                color: palette.text,
                rows: 1,
            },
        );
        slot(
            ui,
            Slot::Text {
                text: card.subtitle,
                rect: Rect::from_min_size(
                    pos2(title_rect.left(), title_rect.bottom() + 2.0),
                    vec2(art, 2.0 * subtitle_row),
                ),
                font: crate::regular(12.5),
                color: palette.secondary,
                rows: 2,
            },
        );
        if card.cover.playable && hovered {
            let inset = card.play_size / 2.0 + 4.0;
            let button = Rect::from_center_size(
                pos2(image.right() - inset, image.bottom() - inset),
                Vec2::splat(card.play_size),
            );
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(button)
                    .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
            );
            play = crate::circle_button(
                &mut child,
                if card.cover.playing {
                    Icon::PauseFilled
                } else {
                    Icon::PlayFilled
                },
                card.play_size,
                palette.accent,
                palette.accent_hover,
                palette.on_accent,
                card.play_label,
            )
            .clicked();
        }
    }
    crate::focus_ring(ui, &response);
    CardResponse {
        clicked: response.clicked() && !play,
        response,
        play,
    }
}

/// The original restrained dark cover shadow.
pub fn shadow(ui: &Ui, palette: &Palette, rect: Rect, radius: f32) {
    if !palette.dark {
        return;
    }
    let shadow = egui::epaint::Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: egui::Color32::from_black_alpha(120),
    };
    ui.painter()
        .add(shadow.as_shape(rect, CornerRadius::same(radius as u8)));
}
