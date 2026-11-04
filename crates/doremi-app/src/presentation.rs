//! Compose extracted Spotifast widgets with the neutral host state and intents.
use super::*;
use doremi_ui::{self as theme, Icon, Palette};
use egui::{Align, Frame, Layout, Margin, RichText, Vec2};

impl DoremiApp {
    pub(super) fn record(&mut self, name: &str, response: &egui::Response) {
        self.controls.push(Control {
            name: name.into(),
            rect: [
                response.rect.min.x,
                response.rect.min.y,
                response.rect.max.x,
                response.rect.max.y,
            ],
        });
    }

    pub(super) fn navigate(&mut self, page: Page) {
        let model = self.model.as_mut().unwrap();
        model.page = page;
        model.revision += 1;
    }

    pub(super) fn shell(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let wide = ui.available_width() >= 760.0;
        let ctx = ui.ctx().clone();
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.navigate(Page::Search);
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("catalog-query")));
        }
        if !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            let model = self.model.as_mut().unwrap();
            let command = if model.queue.playback() == Playback::Playing {
                Command::Pause
            } else {
                Command::Play
            };
            model.command(command);
        }
        if !wide {
            egui::Panel::bottom("navigation")
                .frame(
                    Frame::new()
                        .fill(palette.panel)
                        .inner_margin(Margin::same(4)),
                )
                .show(ui, |ui| {
                    let width = (ui.available_width() - ui.spacing().item_spacing.x * 4.0) / 5.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().button_padding.x = 8.0;
                        for (page, name, label) in navigation() {
                            let response = ui.add_sized(
                                [width, TOUCH],
                                egui::Button::new(if page == Page::Library {
                                    "Biblio."
                                } else {
                                    label
                                })
                                .selected(self.model.as_ref().unwrap().page == page),
                            );
                            self.record(name, &response);
                            if response.clicked() {
                                self.navigate(page);
                            }
                        }
                    });
                });
        }
        self.player_bar(ui, palette, wide);
        if wide {
            egui::Panel::left("sidebar")
                .default_size(240.0)
                .size_range(210.0..=320.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::new().fill(palette.panel).inner_margin(Margin {
                    left: 12,
                    right: 8,
                    top: 12,
                    bottom: 8,
                }))
                .show(ui, |ui| {
                    for (page, name, label) in navigation() {
                        let icon = match page {
                            Page::Home => Icon::House,
                            Page::Search => Icon::Search,
                            Page::Library => Icon::Library,
                            Page::Queue => Icon::ListMusic,
                            _ => Icon::Settings,
                        };
                        let response = theme::nav_row(
                            ui,
                            palette,
                            icon,
                            label,
                            self.model.as_ref().unwrap().page == page,
                            40.0,
                        );
                        self.record(name, &response);
                        if response.clicked() {
                            self.navigate(page);
                        }
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    ui.label(RichText::new("Tu biblioteca").font(theme::bold(15.0)));
                    let rows = self.model.as_ref().unwrap().library.items().to_vec();
                    egui::ScrollArea::vertical()
                        .id_salt("sidebar-library")
                        .show(ui, |ui| {
                            if rows.is_empty() {
                                ui.weak("Guarda música para encontrarla aquí.");
                            }
                            for (index, media) in rows.iter().enumerate() {
                                let response =
                                    ui.add(egui::Button::new(&media.title).frame(false).wrap());
                                if response.clicked() {
                                    self.model
                                        .as_mut()
                                        .unwrap()
                                        .start_context(rows.clone(), index);
                                }
                            }
                        });
                });
        }
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(palette.window)
                    .inner_margin(Margin::same(if wide { 24 } else { 12 })),
            )
            .show(ui, |ui| {
                if let Some(error) = self
                    .model
                    .as_ref()
                    .unwrap()
                    .storage_warning
                    .or(self.storage_error.map(StorageError::message))
                {
                    ui.colored_label(palette.danger, error);
                }
                let rect = ui.max_rect();
                self.page_rect = [rect.left(), rect.top(), rect.right(), rect.bottom()];
                let page = self.model.as_ref().unwrap().page;
                egui::ScrollArea::vertical()
                    .id_salt(format!("page:{page:?}"))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        self.page_content(ui);
                    });
            });
    }

    fn icon_control(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        name: &str,
        icon: Icon,
        label: &str,
        touch: bool,
    ) -> bool {
        // Same glyph and focus treatment; mobile expands the target, not the icon.
        let response = theme::icon_button(
            ui,
            icon,
            if touch { 36.0 } else { 20.0 },
            palette.secondary,
            palette.text,
            label,
        );
        self.record(name, &response);
        response.clicked()
    }

    fn transport(&mut self, ui: &mut egui::Ui, palette: &Palette, touch: bool) {
        ui.horizontal(|ui| self.transport_controls(ui, palette, touch));
    }
    fn transport_controls(&mut self, ui: &mut egui::Ui, palette: &Palette, touch: bool) {
        if self.icon_control(ui, palette, "shuffle", Icon::Shuffle, "Aleatorio", touch) {
            let queue = &self.model.as_ref().unwrap().queue;
            let order = if queue.shuffled() {
                Order::Sequential
            } else {
                shuffled(
                    queue.context().len(),
                    queue.current().and_then(|current| {
                        queue
                            .context()
                            .iter()
                            .position(|entry| entry.id == current.id)
                    }),
                )
            };
            self.model
                .as_mut()
                .unwrap()
                .command(Command::SetOrder(order));
        }
        if self.icon_control(
            ui,
            palette,
            "previous",
            Icon::SkipBackFilled,
            "Anterior",
            touch,
        ) {
            self.model.as_mut().unwrap().command(Command::Previous);
        }
        let playing = self.model.as_ref().unwrap().queue.playback() == Playback::Playing;
        let response = theme::circle_button(
            ui,
            if playing {
                Icon::PauseFilled
            } else {
                Icon::PlayFilled
            },
            if touch { 48.0 } else { 32.0 },
            palette.accent,
            palette.accent_hover,
            palette.on_accent,
            if playing { "Pausa" } else { "Reproducir" },
        );
        self.record(if playing { "pause" } else { "play" }, &response);
        if response.clicked() {
            self.model.as_mut().unwrap().command(if playing {
                Command::Pause
            } else {
                Command::Play
            });
        }
        if self.icon_control(
            ui,
            palette,
            "next",
            Icon::SkipForwardFilled,
            "Siguiente",
            touch,
        ) {
            self.model.as_mut().unwrap().command(Command::Next);
        }
        if self.icon_control(ui, palette, "repeat", Icon::Repeat, "Repetir", touch) {
            let repeat = match self.model.as_ref().unwrap().queue.repeat() {
                Repeat::Off => Repeat::Context,
                Repeat::Context => Repeat::Track,
                Repeat::Track => Repeat::Off,
            };
            self.model
                .as_mut()
                .unwrap()
                .command(Command::SetRepeat(repeat));
        }
    }

    fn progress(&mut self, ui: &mut egui::Ui) {
        let model = self.model.as_mut().unwrap();
        let duration = model
            .queue
            .current()
            .and_then(|entry| entry.media.duration_ms)
            .unwrap_or(0);
        let mut position = model.queue.position_ms();
        let response = ui
            .horizontal(|ui| {
                ui.small(time(position));
                let identity = egui::Id::new((
                    "seek",
                    model
                        .queue
                        .current()
                        .map(|entry| (entry.id.as_u64(), &entry.media.id)),
                ));
                let width = (ui.available_width() - 52.0).max(24.0);
                let response = ui
                    .add_enabled_ui(model.real_audio() && duration > 0, |ui| {
                        ui.scope_builder(
                            egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                ui.cursor().min,
                                Vec2::new(width, 20.0),
                            )),
                            |ui| {
                                ui.spacing_mut().slider_width = width;
                                ui.spacing_mut().interact_size.y = 20.0;
                                seek_slider(ui, identity, &mut position, duration)
                            },
                        )
                        .inner
                    })
                    .inner;
                ui.small(time(duration));
                response
            })
            .inner;
        if response.changed() {
            model.command(Command::Seek(position));
        }
        self.record("seek", &response);
    }

    fn volume(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        theme::icon(ui, Icon::Volume2, 18.0, palette.secondary);
        let model = self.model.as_mut().unwrap();
        let size = Vec2::new(
            90.0_f32.min((ui.available_width() - 12.0).max(24.0)),
            ui.spacing().interact_size.y,
        );
        let (event, response) = theme::thin_slider_sized(
            ui,
            palette,
            ui.id().with("volume"),
            "Volumen",
            model.prefs.volume,
            size,
            Some(0.05),
        );
        if let theme::SliderEvent::Dragging(value) | theme::SliderEvent::Committed(value) = event {
            model.prefs.volume = value;
            if let Some(engine) = &model.audio {
                engine.set_volume(model.prefs.volume);
            }
            model.revision += 1;
        }
        self.record("volume", &response);
    }

    fn now_playing(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let model = self.model.as_ref().unwrap();
        let title = model
            .queue
            .current()
            .map(|entry| entry.media.title.as_str())
            .unwrap_or("Ninguna canción seleccionada");
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
            theme::cover_placeholder(ui, palette, rect, 4.0, Icon::Music);
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(title).font(theme::semibold(14.0))).truncate(),
                );
                ui.weak(if model.queue.playback() == Playback::Failed {
                    "No se pudo reproducir; abre el reproductor para ver el error"
                } else if model.queue.playback() == Playback::Loading {
                    "Cargando audio…"
                } else if model.real_audio() {
                    "Audio nativo"
                } else {
                    "Demostración · sin audio"
                });
            });
        });
    }

    fn player_bar(&mut self, ui: &mut egui::Ui, palette: &Palette, wide: bool) {
        let compact = !wide && ui.available_height() < 500.0;
        theme::player_bar(
            ui,
            palette.panel,
            if wide {
                theme::PLAYER_BAR_HEIGHT
            } else if compact {
                104.0
            } else {
                208.0
            },
            |ui| {
                if wide {
                    ui.columns(3, |columns| {
                        columns[0].add_space(18.0);
                        self.now_playing(&mut columns[0], palette);
                        columns[1].add_space(8.0);
                        columns[1].with_layout(Layout::top_down(Align::Center), |ui| {
                            self.transport(ui, palette, false);
                            self.progress(ui);
                        });
                        columns[2].add_space(22.0);
                        columns[2].with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if self.icon_control(
                                ui,
                                palette,
                                "stop",
                                Icon::Square,
                                "Detener",
                                false,
                            ) {
                                self.model.as_mut().unwrap().command(Command::Cancel);
                            }
                            if self.icon_control(
                                ui,
                                palette,
                                "player",
                                Icon::Expand,
                                "Reproductor",
                                false,
                            ) {
                                self.navigate(Page::Player);
                            }
                            self.volume(ui, palette);
                        });
                    });
                } else if compact {
                    ui.add_space(4.0);
                    let title = self
                        .model
                        .as_ref()
                        .unwrap()
                        .queue
                        .current()
                        .map(|entry| entry.media.title.as_str())
                        .unwrap_or("Ninguna canción seleccionada");
                    ui.add(egui::Label::new(RichText::new(title).size(13.0)).truncate());
                    ui.horizontal(|ui| {
                        self.transport_controls(ui, palette, true);
                        let response = theme::icon_button(
                            ui,
                            Icon::Ellipsis,
                            36.0,
                            palette.secondary,
                            palette.text,
                            "Más controles",
                        );
                        self.record("player-more", &response);
                        egui::Popup::menu(&response).show(|ui| {
                            if self.button(ui, "stop", "Detener", 160.0) {
                                self.model.as_mut().unwrap().command(Command::Cancel);
                                ui.close();
                            }
                            if self.button(ui, "player", "Reproductor", 160.0) {
                                self.navigate(Page::Player);
                                ui.close();
                            }
                            ui.horizontal(|ui| self.volume(ui, palette));
                        });
                    });
                    self.progress(ui);
                } else {
                    ui.add_space(4.0);
                    self.now_playing(ui, palette);
                    self.transport(ui, palette, true);
                    self.progress(ui);
                    ui.horizontal(|ui| {
                        self.volume(ui, palette);
                        if self.icon_control(ui, palette, "stop", Icon::Square, "Detener", false) {
                            self.model.as_mut().unwrap().command(Command::Cancel);
                        }
                        if self.icon_control(
                            ui,
                            palette,
                            "player",
                            Icon::Expand,
                            "Reproductor",
                            false,
                        ) {
                            self.navigate(Page::Player);
                        }
                    });
                }
            },
        );
    }

    pub(super) fn rows(&mut self, ui: &mut egui::Ui, rows: &[Media]) {
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        for (index, media) in rows.iter().enumerate() {
            Frame::new()
                .inner_margin(Margin::symmetric(8, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
                        theme::cover_placeholder(ui, &palette, rect, 4.0, Icon::Music);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&media.title).font(theme::medium(14.0)));
                            ui.small(if demo_candidate(&media.id) {
                                "Demostración · sin audio"
                            } else if self.model.as_ref().unwrap().online_candidate(&media.id) {
                                "Commons · tono CC0"
                            } else {
                                "Audio local"
                            });
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        if self.button(
                            ui,
                            &format!("select:{}", media.id.local()),
                            "Seleccionar",
                            118.0,
                        ) {
                            #[cfg(any(
                                target_os = "linux",
                                target_os = "android",
                                target_os = "windows"
                            ))]
                            if media.id.provider.as_str() == "musicbrainz" {
                                let model = self.model.as_mut().unwrap();
                                model.music.selected = true;
                                model.music.open(doremi_core::media::CatalogEntry {
                                    media: media.clone(),
                                    kind: doremi_core::media::EntityKind::Recording,
                                    artists: vec![],
                                });
                                self.navigate(Page::Search);
                            } else {
                                self.model
                                    .as_mut()
                                    .unwrap()
                                    .start_context(rows.to_vec(), index);
                                self.navigate(Page::Player);
                            }
                            #[cfg(not(any(
                                target_os = "linux",
                                target_os = "android",
                                target_os = "windows"
                            )))]
                            {
                                self.model
                                    .as_mut()
                                    .unwrap()
                                    .start_context(rows.to_vec(), index);
                                self.navigate(Page::Player);
                            }
                        }
                        if self.button(ui, &format!("add:{}", media.id.local()), "+ Cola", 90.0) {
                            self.model
                                .as_mut()
                                .unwrap()
                                .command(Command::Append(vec![media.clone()]));
                        }
                        let saved = self.model.as_ref().unwrap().library.contains(&media.id);
                        if self.button(
                            ui,
                            &format!("save:{}", media.id.local()),
                            if saved { "Guardada" } else { "Guardar" },
                            90.0,
                        ) {
                            self.model.as_mut().unwrap().save_media(media.clone());
                        }
                        if self.model.as_ref().unwrap().page == Page::Library
                            && self.button(
                                ui,
                                &format!("forget:{}", media.id.local()),
                                "Quitar",
                                90.0,
                            )
                        {
                            self.model.as_mut().unwrap().remove_saved(&media.id);
                        }
                    });
                });
        }
    }
}

fn time(ms: u64) -> String {
    format!("{}:{:02}", ms / 60000, ms / 1000 % 60)
}
fn navigation() -> [(Page, &'static str, &'static str); 5] {
    [
        (Page::Home, "home", "Inicio"),
        (Page::Search, "search", "Buscar"),
        (Page::Library, "library", "Biblioteca"),
        (Page::Queue, "queue", "Cola"),
        (Page::Settings, "settings", "Ajustes"),
    ]
}
