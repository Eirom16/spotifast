//! Honest, shared feedback for loading, empty collections and provider failures.
//! Hosts supply the message and retry actions; this component never initiates work.
use crate::{Icon, Palette, semibold};
use egui::{Frame, Margin, RichText, Ui};

#[derive(Clone, Copy)]
pub enum Kind {
    Loading,
    Empty,
    Error,
    Info,
}

pub fn panel(ui: &mut Ui, palette: &Palette, kind: Kind, title: &str, detail: &str) {
    Frame::new()
        .fill(palette.surface)
        .corner_radius(12)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if matches!(kind, Kind::Loading) {
                    ui.add(egui::Spinner::new().size(22.0).color(palette.accent));
                } else {
                    let (icon, color) = match kind {
                        Kind::Error => (Icon::CircleAlert, palette.danger),
                        Kind::Empty => (Icon::Search, palette.secondary),
                        _ => (Icon::Info, palette.accent),
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(22.0), egui::Sense::hover());
                    crate::paint_icon(ui, icon, rect, 22.0, color);
                }
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            RichText::new(title)
                                .font(semibold(15.0))
                                .color(palette.text),
                        )
                        .wrap(),
                    );
                    if !detail.is_empty() {
                        ui.add(
                            egui::Label::new(RichText::new(detail).color(palette.secondary)).wrap(),
                        );
                    }
                });
            });
        });
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_wrap_without_exceeding_phone_or_desktop_widths() {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        for palette in [Palette::light(), Palette::dark()] {
            for width in [240.0, 320.0, 384.0, 1000.0] {
                for kind in [Kind::Empty, Kind::Error, Kind::Info, Kind::Loading] {
                    let mut bounds = egui::Rect::NOTHING;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))),
                            ..Default::default()
                        },
                        |ui| {
                            panel(ui, &palette, kind, "No hay resultados para esta consulta", "Prueba con otro nombre de canción, artista o álbum. No se ha cambiado de proveedor.");
                            bounds = ui.min_rect();
                        },
                    );
                    output.textures_delta.clear();
                    assert!(bounds.right() <= width, "{bounds:?} exceeds {width}");
                    assert!(bounds.height() < 320.0);
                }
            }
        }
    }
}
