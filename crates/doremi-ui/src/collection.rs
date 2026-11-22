//! Album, artist and playlist header extracted from the original collection view.
//! Artwork and navigation stay with the host; layout and typography are shared.
//! Copyright (c) Carmine Paolino. MIT; see the repository LICENSE.
use crate::{Palette, bold, medium, regular};
use egui::{Rect, Response, Sense, Ui, Vec2};

pub const HERO_ART: f32 = 212.0;

pub struct Hero<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub round: bool,
    pub cover_clickable: bool,
}

pub enum HeroSlot<'a> {
    Cover {
        rect: Rect,
        radius: f32,
        response: &'a Response,
    },
    Text {
        text: &'a str,
        font: egui::FontId,
        color: egui::Color32,
    },
    Byline,
}

pub fn hero(
    ui: &mut Ui,
    palette: &Palette,
    hero: Hero<'_>,
    mut slot: impl FnMut(&mut Ui, HeroSlot<'_>),
) {
    ui.add_space(12.0);
    let width = ui.available_width();
    let stacked = width < 420.0;
    let cover_size = if stacked {
        width.min(160.0)
    } else if width > 720.0 {
        HERO_ART
    } else if width > 520.0 {
        160.0
    } else {
        96.0
    };
    let mut contents = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing.x = if width > 520.0 { 24.0 } else { 16.0 };
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(cover_size),
            if hero.cover_clickable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        slot(
            ui,
            HeroSlot::Cover {
                rect,
                radius: if hero.round { cover_size / 2.0 } else { 6.0 },
                response: &response,
            },
        );
        ui.vertical(|ui| {
            let width = ui.available_width().max(0.0);
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 6.0;
            if !stacked {
                ui.add_space(cover_size * 0.08);
            }
            slot(
                ui,
                HeroSlot::Text {
                    text: hero.kind,
                    font: medium(12.5),
                    color: palette.text,
                },
            );
            let mut size = if cover_size > 200.0 {
                56.0
            } else if cover_size >= 160.0 {
                40.0
            } else {
                28.0
            };
            loop {
                let galley =
                    ui.painter()
                        .layout_no_wrap(hero.title.to_owned(), bold(size), palette.text);
                if galley.size().x <= width || size <= 22.0 {
                    break;
                }
                size = (size - 6.0).max(22.0);
            }
            slot(
                ui,
                HeroSlot::Text {
                    text: hero.title,
                    font: bold(size),
                    color: palette.text,
                },
            );
            if let Some(description) = hero.description.filter(|text| !text.is_empty()) {
                slot(
                    ui,
                    HeroSlot::Text {
                        text: description,
                        font: regular(13.5),
                        color: palette.secondary,
                    },
                );
            }
            let previous_height = ui.spacing().interact_size.y;
            // Hosts with no credits must not reserve an empty touch-sized row.
            // Interactive credits supply their own explicit hit targets.
            ui.spacing_mut().interact_size.y = 0.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                slot(ui, HeroSlot::Byline);
            });
            ui.spacing_mut().interact_size.y = previous_height;
        });
    };
    if stacked {
        ui.vertical(&mut contents);
    } else {
        ui.horizontal_top(contents);
    }
    ui.add_space(20.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_collection_titles_and_credits_fit_phone_landscape_and_desktop() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        for palette in [Palette::dark(), Palette::light()] {
            for width in [264.0, 320.0, 384.0, 682.0, 1100.0] {
                let mut text_rects = Vec::new();
                let mut cover = Rect::NOTHING;
                let mut output = ctx.run_ui(egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 900.0))),
                    ..Default::default()
                }, |ui| {
                    ui.spacing_mut().interact_size.y = 48.0;
                    hero(ui, &palette, Hero {
                        kind: "ÁLBUM",
                        title: "Una colección musical con un título largo para comprobar el ajuste de texto",
                        description: Some("Metadatos del catálogo seleccionado"),
                        round: false,
                        cover_clickable: false,
                    }, |ui, slot| match slot {
                        HeroSlot::Cover { rect, .. } => cover = rect,
                        HeroSlot::Text { text, font, color } => {
                            text_rects.push(ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).wrap()).rect);
                        }
                        HeroSlot::Byline => {
                            text_rects.push(ui.label("Artista invitado · Edición especial").rect);
                        }
                    });
                    assert_eq!(ui.spacing().interact_size.y, 48.0);
                });
                output.textures_delta.clear();
                assert!(cover.right() <= width);
                for rect in text_rects {
                    assert!(
                        rect.left() >= 0.0 && rect.right() <= width,
                        "{width}: {rect:?}"
                    );
                    if width < 420.0 {
                        assert!(rect.top() >= cover.bottom());
                    } else {
                        assert!(rect.left() >= cover.right());
                    }
                }
            }
        }
    }
}
