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
    let cover_size = if width > 720.0 {
        HERO_ART
    } else if width > 520.0 {
        160.0
    } else {
        96.0
    };
    ui.horizontal(|ui| {
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
            ui.add_space(cover_size * 0.08);
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
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                slot(ui, HeroSlot::Byline);
            });
        });
    });
    ui.add_space(20.0);
}
