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
        if page == Page::Player && model.page != Page::Player {
            model.player_return = model.page;
        }
        model.page = page;
        model.revision += 1;
    }
    pub(super) fn can_go_back(&self) -> bool {
        self.model
            .as_ref()
            .is_some_and(|model| model.page != Page::Home)
    }
    pub(super) fn go_back(&mut self) {
        let model = self.model.as_mut().unwrap();
        if model.page == Page::Player {
            let page = model.player_return;
            self.navigate(page);
            return;
        }
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        if model.page == Page::Search && model.music.selected && model.music.has_back() {
            model.music.back();
            model.revision += 1;
            return;
        }
        if model.page == Page::Playlist {
            self.navigate(Page::Library);
        } else {
            self.navigate(Page::Home);
        }
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
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let width = (ui.available_width() - 16.0) / 5.0;
                    ui.horizontal(|ui| {
                        for (page, name, label) in navigation() {
                            let response = theme::mobile_nav(
                                ui,
                                palette,
                                navigation_icon(page),
                                label,
                                self.model.as_ref().unwrap().page == page
                                    || (page == Page::Home
                                        && self.model.as_ref().unwrap().page == Page::History)
                                    || (page == Page::Library
                                        && self.model.as_ref().unwrap().page == Page::Playlist),
                                width,
                            );
                            self.record(name, &response);
                            if response.clicked() {
                                self.navigate(page);
                            }
                        }
                    });
                });
        }
        if wide {
            self.player_bar(ui, palette);
        } else if self.model.as_ref().unwrap().page != Page::Player
            && self.model.as_ref().unwrap().queue.current().is_some()
        {
            self.mini_player(ui, palette);
        }
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
                        let icon = navigation_icon(page);
                        let response = theme::nav_row(
                            ui,
                            palette,
                            icon,
                            label,
                            self.model.as_ref().unwrap().page == page
                                || (page == Page::Home
                                    && self.model.as_ref().unwrap().page == Page::History)
                                || (page == Page::Library
                                    && self.model.as_ref().unwrap().page == Page::Playlist),
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
                    let playlists = self.model.as_ref().unwrap().playlists.clone();
                    egui::ScrollArea::vertical()
                        .id_salt("sidebar-library")
                        .show(ui, |ui| {
                            if rows.is_empty() && playlists.lists().is_empty() {
                                ui.weak("Guarda música para encontrarla aquí.");
                            }
                            for list in playlists.lists() {
                                let model = self.model.as_ref().unwrap();
                                let response = theme::nav_row(
                                    ui,
                                    palette,
                                    Icon::ListMusic,
                                    &list.name,
                                    model.page == Page::Playlist
                                        && model.selected_playlist == Some(list.id),
                                    40.0,
                                );
                                self.record(&format!("sidebar-playlist:{}", list.id), &response);
                                if response.clicked() {
                                    self.open_playlist(list.id);
                                }
                            }
                            for (index, media) in rows.iter().enumerate() {
                                let response =
                                    ui.add(egui::Button::new(&media.title).frame(false).wrap());
                                self.record(
                                    &format!("sidebar-select:{}", media.id.local()),
                                    &response,
                                );
                                if response.clicked() {
                                    self.open_library_entry(&rows, index);
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
        let response = theme::icon_button_sized(
            ui,
            icon,
            if touch { [24.0, TOUCH] } else { [20.0, 32.0] },
            palette.secondary,
            palette.text,
            label,
        );
        self.record(name, &response);
        response.clicked()
    }

    fn play_control(&mut self, ui: &mut egui::Ui, palette: &Palette, size: f32) {
        let playing = self.model.as_ref().unwrap().queue.playback() == Playback::Playing;
        let response = theme::circle_button(
            ui,
            if playing {
                Icon::PauseFilled
            } else {
                Icon::PlayFilled
            },
            size,
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
        self.play_control(ui, palette, if touch { 64.0 } else { 32.0 });
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
        self.progress_sized(ui, 20.0);
    }
    fn progress_sized(&mut self, ui: &mut egui::Ui, height: f32) {
        let model = self.model.as_mut().unwrap();
        let duration = model
            .queue
            .current()
            .and_then(|entry| entry.media.duration_ms)
            .unwrap_or(0);
        let mut position = model.queue.position_ms();
        let response = ui
            .scope(|ui| {
                ui.spacing_mut().interact_size.y = height;
                ui.horizontal(|ui| {
                    let elapsed = ui.small(time(position));
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
                                    Vec2::new(width, height),
                                )),
                                |ui| {
                                    ui.spacing_mut().slider_width = width;
                                    ui.spacing_mut().interact_size.y = height;
                                    seek_slider(ui, identity, &mut position, duration)
                                },
                            )
                            .inner
                        })
                        .inner;
                    let total = ui.small(time(duration));
                    (response, elapsed, total)
                })
                .inner
            })
            .inner;
        let (response, elapsed, total) = response;
        if response.changed() {
            model.command(Command::Seek(position));
        }
        self.record("seek", &response);
        self.record("elapsed-time", &elapsed);
        self.record("total-time", &total);
    }

    fn volume(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        self.volume_sized(ui, palette, 90.0);
    }
    fn volume_sized(&mut self, ui: &mut egui::Ui, palette: &Palette, width: f32) {
        theme::icon(ui, Icon::Volume2, 18.0, palette.secondary);
        let model = self.model.as_mut().unwrap();
        let size = Vec2::new(
            width.min((ui.available_width() - 12.0).max(24.0)),
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
                ui.weak(playback_caption(model));
            });
        });
    }

    fn player_bar(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        theme::player_bar(ui, palette.panel, theme::PLAYER_BAR_HEIGHT, |ui| {
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
                    if self.icon_control(ui, palette, "stop", Icon::Square, "Detener", false) {
                        self.model.as_mut().unwrap().command(Command::Cancel);
                    }
                    if self.icon_control(ui, palette, "player", Icon::Expand, "Reproductor", false)
                    {
                        self.navigate(Page::Player);
                    }
                    self.volume(ui, palette);
                });
            });
        });
    }

    fn mini_player(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        egui::Panel::bottom("mobile-mini-player")
            .exact_size(72.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(palette.panel)
                    .inner_margin(Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                let model = self.model.as_ref().unwrap();
                let media = model.queue.current().unwrap().media.clone();
                let caption = playback_caption(model);
                let position = model.queue.position_ms();
                let failed = model.queue.playback() == Playback::Failed;
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), TOUCH),
                    egui::Sense::hover(),
                );
                let body = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.right() - 2.0 * (TOUCH + 8.0), rect.bottom()),
                );
                let response =
                    ui.interact(body, ui.id().with("mini-player-open"), egui::Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Abrir reproductor: {}", media.title),
                    )
                });
                let cover = egui::Rect::from_center_size(
                    egui::pos2(body.left() + 20.0, body.center().y),
                    Vec2::splat(40.0),
                );
                theme::cover_placeholder(ui, palette, cover, 4.0, Icon::Music);
                let text = egui::Rect::from_min_max(
                    egui::pos2(cover.right() + 12.0, body.top() + 5.0),
                    egui::pos2(body.right() - 4.0, body.bottom()),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(text)
                        .layout(Layout::top_down(Align::Min)),
                );
                child.spacing_mut().item_spacing.y = 2.0;
                child.add(
                    egui::Label::new(RichText::new(&media.title).font(theme::semibold(14.0)))
                        .truncate(),
                );
                child.add(
                    egui::Label::new(RichText::new(caption).font(theme::regular(11.0)).color(
                        if failed {
                            palette.danger
                        } else {
                            palette.secondary
                        },
                    ))
                    .truncate(),
                );
                theme::focus_ring(ui, &response);
                self.record("player", &response);
                if response.clicked() {
                    self.navigate(Page::Player);
                }
                let play = egui::Rect::from_min_size(
                    egui::pos2(body.right() + 8.0, rect.top()),
                    Vec2::splat(TOUCH),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(play)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                self.play_control(&mut child, palette, TOUCH);
                let next = egui::Rect::from_min_size(
                    egui::pos2(play.right() + 8.0, rect.top()),
                    Vec2::splat(TOUCH),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(next)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                if self.icon_control(
                    &mut child,
                    palette,
                    "next",
                    Icon::SkipForwardFilled,
                    "Siguiente",
                    true,
                ) {
                    self.model.as_mut().unwrap().command(Command::Next);
                }
                let (track, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), 2.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(track, 1.0, palette.outline);
                if let Some(duration) = media.duration_ms.filter(|duration| *duration > 0) {
                    let progress = egui::Rect::from_min_size(
                        track.min,
                        Vec2::new(
                            track.width() * (position as f32 / duration as f32).clamp(0.0, 1.0),
                            2.0,
                        ),
                    );
                    ui.painter().rect_filled(progress, 1.0, palette.accent);
                }
            });
    }

    pub(super) fn mobile_player(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let width = self.page_rect[2] - self.page_rect[0];
        let height = self.page_rect[3] - self.page_rect[1];
        if width >= 616.0
            && width > height
            && self.model.as_ref().unwrap().queue.current().is_some()
        {
            self.mobile_player_landscape(ui, palette, height);
            return;
        }
        if self.can_go_back()
            && self.icon_control(
                ui,
                palette,
                "player-back",
                Icon::ChevronDown,
                "Volver",
                true,
            )
        {
            self.go_back();
            return;
        }
        let model = self.model.as_ref().unwrap();
        let Some(current) = model.queue.current() else {
            self.empty_player(ui);
            return;
        };
        let title = current.media.title.clone();
        let caption = playback_caption(model);
        let failed = model.queue.playback() == Playback::Failed;
        let notice = model.notice.clone();
        ui.spacing_mut().item_spacing.y = 8.0;
        let height = (self.page_rect[3] - self.page_rect[1]).max(0.0);
        let art = (height * 0.28).clamp(96.0, 256.0).min(ui.available_width());
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(art), egui::Sense::hover());
            theme::cover_placeholder(ui, palette, rect, 8.0, Icon::Music);
            self.record("player-art", &response);
            let mut text = egui::text::LayoutJob::simple(
                title,
                theme::bold(23.0),
                palette.text,
                ui.available_width(),
            );
            text.wrap.max_rows = 2;
            text.halign = Align::Center;
            ui.add(egui::Label::new(text));
            ui.label(
                RichText::new(caption)
                    .font(theme::regular(13.0))
                    .color(if failed {
                        palette.danger
                    } else {
                        palette.secondary
                    }),
            );
        });
        if failed {
            ui.colored_label(palette.danger, notice);
        }
        self.mobile_player_controls(ui, palette);
        #[cfg(target_os = "android")]
        ui.weak("La reproducción se pausa al salir de Doremi.");
    }

    fn mobile_player_landscape(&mut self, ui: &mut egui::Ui, palette: &Palette, height: f32) {
        let model = self.model.as_ref().unwrap();
        let title = model.queue.current().unwrap().media.title.clone();
        let caption = playback_caption(model);
        let failed = model.queue.playback() == Playback::Failed;
        let notice = model.notice.clone();
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.columns(2, |columns| {
            if self.icon_control(
                &mut columns[0],
                palette,
                "player-back",
                Icon::ChevronDown,
                "Volver",
                true,
            ) {
                self.go_back();
            }
            columns[0].vertical_centered(|ui| {
                #[cfg(any(target_os = "android", test))]
                let notice = ui.painter().layout(
                    "La reproducción se pausa al salir de Doremi.".into(),
                    theme::regular(12.5),
                    palette.secondary,
                    ui.available_width(),
                );
                #[cfg(any(target_os = "android", test))]
                let notice_height = notice.size().y;
                #[cfg(not(any(target_os = "android", test)))]
                let notice_height = 0.0;
                // Reserve breathing room for device-pixel rounding and the notice.
                let art = (height - TOUCH - 3.0 * GAP - notice_height)
                    .clamp(96.0, 200.0)
                    .min(ui.available_width());
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(art), egui::Sense::hover());
                theme::cover_placeholder(ui, palette, rect, 8.0, Icon::Music);
                self.record("player-art", &response);
                #[cfg(any(target_os = "android", test))]
                {
                    let response = ui.add(egui::Label::new(notice));
                    self.record("player-background-notice", &response);
                }
            });
            columns[1].spacing_mut().item_spacing.y = 4.0;
            columns[1].vertical_centered(|ui| {
                let mut text = egui::text::LayoutJob::simple(
                    title,
                    theme::bold(20.0),
                    palette.text,
                    ui.available_width(),
                );
                text.wrap.max_rows = 2;
                text.halign = Align::Center;
                ui.add(egui::Label::new(text));
                ui.label(
                    RichText::new(caption)
                        .font(theme::regular(13.0))
                        .color(if failed {
                            palette.danger
                        } else {
                            palette.secondary
                        }),
                );
            });
            self.mobile_player_controls(&mut columns[1], palette);
        });
        if failed {
            ui.colored_label(palette.danger, notice);
        }
    }

    pub(super) fn empty_player(&mut self, ui: &mut egui::Ui) {
        ui.heading("Elige qué escuchar");
        ui.weak("Busca música o abre una canción de tu biblioteca.");
        if self.button(ui, "player-search", "Buscar música", 200.0) {
            self.navigate(Page::Search);
        }
    }

    fn mobile_player_controls(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        self.progress_sized(ui, TOUCH);
        ui.horizontal(|ui| {
            let width = 4.0 * TOUCH + 64.0 + 4.0 * 8.0;
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            self.transport_controls(ui, palette, true);
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - TOUCH - 18.0 - 24.0).max(48.0);
            self.volume_sized(ui, palette, width);
            if self.icon_control(ui, palette, "stop", Icon::Square, "Detener", true) {
                self.model.as_mut().unwrap().command(Command::Cancel);
            }
        });
    }

    pub(super) fn open_library_entry(&mut self, rows: &[Media], index: usize) {
        let Some(media) = rows.get(index) else {
            return;
        };
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        if media.id.provider.as_str() == "musicbrainz" {
            let model = self.model.as_mut().unwrap();
            model.music.selected = true;
            model.music.open(doremi_core::media::CatalogEntry {
                media: media.clone(),
                kind: doremi_core::media::EntityKind::Recording,
                artists: vec![],
            });
            self.navigate(Page::Search);
            return;
        }
        let playable: Vec<_> = rows
            .iter()
            .filter(|item| item.id.provider.as_str() != "musicbrainz")
            .cloned()
            .collect();
        if !playable.is_empty() {
            let index = rows[..index]
                .iter()
                .filter(|item| item.id.provider.as_str() != "musicbrainz")
                .count();
            self.model.as_mut().unwrap().start_context(playable, index);
            self.navigate(Page::Player);
        }
    }

    pub(super) fn rows(&mut self, ui: &mut egui::Ui, rows: &[Media]) {
        self.rows_with_subtitle(ui, rows, None);
    }
    pub(super) fn rows_with_subtitle(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[Media],
        prefix: Option<&str>,
    ) {
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        for (index, media) in rows.iter().enumerate() {
            ui.push_id((&media.id, index), |ui| {
                let subtitle = if media.id.provider.as_str() == "musicbrainz" {
                    "MusicBrainz · metadatos"
                } else if demo_candidate(&media.id) {
                    "Demostración · sin audio"
                } else if self.model.as_ref().unwrap().online_candidate(&media.id) {
                    "Commons · CC0"
                } else {
                    "Audio local"
                };
                let subtitle = prefix.map_or_else(
                    || subtitle.to_owned(),
                    |prefix| format!("{prefix} · {subtitle}"),
                );
                let duration = media
                    .duration_ms
                    .filter(|_| media.id.provider.as_str() == "wikimedia-commons");
                let subtitle = duration.map_or_else(
                    || subtitle.to_owned(),
                    |ms| {
                        let seconds = ms / 1000;
                        format!("{}:{:02} · {subtitle}", seconds / 60, seconds % 60)
                    },
                );
                let row = theme::list_row(
                    ui,
                    &palette,
                    ui.id().with("media-row"),
                    theme::ListRow {
                        title: &media.title,
                        subtitle: &subtitle,
                        round: false,
                        actions: 2,
                        action_label: "Seleccionar",
                    },
                );
                self.record(&format!("select:{}", media.id.local()), &row.body);
                if self.model.as_ref().unwrap().page == Page::Playlist {
                    self.record(&format!("playlist-select:{index}"), &row.body);
                }
                if row.body.clicked() {
                    self.open_library_entry(rows, index);
                }
                let saved = self.model.as_ref().unwrap().library.contains(&media.id);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.action_rect(0))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let save = theme::icon_button_sized(
                    &mut child,
                    if saved {
                        Icon::HeartFilled
                    } else {
                        Icon::Heart
                    },
                    [20.0, TOUCH],
                    if saved {
                        palette.accent
                    } else {
                        palette.secondary
                    },
                    palette.text,
                    if saved {
                        "Quitar de biblioteca"
                    } else {
                        "Guardar en biblioteca"
                    },
                );
                self.record(&format!("save:{}", media.id.local()), &save);
                if save.clicked() {
                    let model = self.model.as_mut().unwrap();
                    if saved {
                        model.remove_saved(&media.id);
                    } else {
                        model.save_media(media.clone());
                    }
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.action_rect(1))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let more = theme::icon_button_sized(
                    &mut child,
                    Icon::Ellipsis,
                    [22.0, TOUCH],
                    palette.secondary,
                    palette.text,
                    "Más opciones de la canción",
                );
                self.record(&format!("row-more:{}", media.id.local()), &more);
                if self.model.as_ref().unwrap().page == Page::Playlist {
                    self.record(&format!("playlist-row-more:{index}"), &more);
                }
                egui::Popup::menu(&more).show(|ui| {
                    if media.id.provider.as_str() != "musicbrainz"
                        && self.button(
                            ui,
                            &format!("add:{}", media.id.local()),
                            "Añadir a la cola",
                            190.0,
                        )
                    {
                        self.model
                            .as_mut()
                            .unwrap()
                            .command(Command::Append(vec![media.clone()]));
                        ui.close();
                    }
                    if saved
                        && self.button(
                            ui,
                            &format!("forget:{}", media.id.local()),
                            "Quitar de biblioteca",
                            190.0,
                        )
                    {
                        self.model.as_mut().unwrap().remove_saved(&media.id);
                        ui.close();
                    }
                    if saved
                        && self.button(
                            ui,
                            &format!("save-again:{}", media.id.local()),
                            "Guardar de nuevo",
                            190.0,
                        )
                    {
                        self.model.as_mut().unwrap().save_media(media.clone());
                        ui.close();
                    }
                    self.row_playlist_actions(ui, media, index);
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

fn navigation_icon(page: Page) -> Icon {
    match page {
        Page::Home => Icon::House,
        Page::Search => Icon::Search,
        Page::Library | Page::Playlist => Icon::Library,
        Page::Queue => Icon::ListMusic,
        _ => Icon::Settings,
    }
}
pub(super) fn playback_caption(model: &Model) -> &'static str {
    if model.queue.current().is_none() {
        "Sin reproducción activa"
    } else if model.queue.playback() == Playback::Failed {
        "No se pudo reproducir"
    } else if !model.real_audio() {
        "Demostración · sin audio"
    } else if model.queue.playback() == Playback::Loading {
        "Cargando audio…"
    } else if model.queue.playback() == Playback::Playing
        && model
            .audio
            .as_ref()
            .and_then(Engine::metrics)
            .is_some_and(|metrics| metrics.buffering)
    {
        "Buffering…"
    } else if model.queue.playback() == Playback::Paused {
        "En pausa"
    } else {
        "Audio nativo"
    }
}
