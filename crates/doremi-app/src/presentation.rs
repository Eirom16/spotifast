//! Compose extracted Spotifast widgets with the neutral host state and intents.
use super::*;
use doremi_ui::{self as theme, Icon, Palette};
use egui::{Align, Frame, Layout, Margin, RichText, Vec2};

impl DoremiApp {
    /// Reuse the catalog's bounded image worker for every music surface.
    /// The drawing thread only tries the client lock and queues visible work.
    pub(super) fn media_artwork(
        &mut self,
        ui: &egui::Ui,
        id: &MediaId,
        rect: egui::Rect,
        radius: u8,
    ) {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        if id.provider.as_str() == doremi_youtube::PROVIDER && ui.is_rect_visible(rect) {
            let model = self.model.as_mut().unwrap();
            let url = model
                .music
                .session
                .client
                .try_lock()
                .ok()
                .and_then(|client| {
                    client
                        .as_ref()
                        .and_then(|client| client.as_ref().ok())
                        .and_then(|client| client.thumbnail(id))
                });
            model.artwork.request_thumbnail(id, url, ui.ctx());
            if let Some(cover) = self.artwork_textures.current(&model.artwork, id, ui.ctx()) {
                egui::Image::new((cover.id(), cover.size_vec2()))
                    .fit_to_exact_size(rect.size())
                    .corner_radius(radius)
                    .paint_at(ui, rect);
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
        let _ = (ui, id, rect, radius);
    }

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
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
        {
            if page == Page::Search && model.page != Page::Search && model.music.home_root() {
                model.music.search(&model.query);
            }
            if page == Page::Home && model.music.home_started() && !model.music.home_root() {
                model.youtube_page(doremi_youtube::Request::Home);
            }
        }
        if page == Page::Player && model.page != Page::Player {
            model.player_return = model.page;
        }
        model.page = page;
        model.revision += 1;
    }
    pub(super) fn can_go_back(&self) -> bool {
        self.model.as_ref().is_some_and(|model| {
            if model.page != Page::Home {
                return true;
            }
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
            {
                model.music.home_root() && model.music.has_back()
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "android")))]
            {
                false
            }
        })
    }
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
    pub(super) fn catalog_back(&mut self) {
        self.model.as_mut().unwrap().music.back();
        if self.model.as_ref().unwrap().music.home_root() {
            self.navigate(Page::Home);
        }
    }
    pub(super) fn go_back(&mut self) {
        let model = self.model.as_mut().unwrap();
        if model.page == Page::Player {
            let page = model.player_return;
            self.navigate(page);
            return;
        }
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
        if (model.page == Page::Search || model.page == Page::Home && model.music.home_root())
            && model.music.selected
            && model.music.has_back()
        {
            self.catalog_back();
            return;
        }
        if model.page == Page::Playlist {
            self.navigate(Page::Library);
        } else {
            self.navigate(Page::Home);
        }
    }

    pub(super) fn choice_button(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        label: &str,
        width: f32,
        selected: bool,
    ) -> bool {
        let height = if ui.ctx().content_rect().width() < 760.0 {
            TOUCH
        } else {
            36.0
        };
        // Measure intrinsic labels before a wrapping layout allocates their cell.
        let width = if width > 0.0 {
            width
        } else {
            ui.painter()
                .layout_no_wrap(
                    label.to_owned(),
                    egui::TextStyle::Button.resolve(ui.style()),
                    ui.visuals().text_color(),
                )
                .size()
                .x
                + 2.0 * ui.spacing().button_padding.x
        };
        let response = ui.add_sized(
            [width.max(TOUCH), height],
            egui::Button::new(label)
                .selected(selected)
                .corner_radius(18)
                .wrap(),
        );
        self.record(name, &response);
        response.clicked()
    }

    pub(super) fn home_header(&mut self, ui: &mut egui::Ui) {
        let prefs = &self.model.as_ref().unwrap().prefs;
        let palette = theme::palette(prefs.light, prefs.green);
        Frame::new()
            .fill(palette.surface)
            .corner_radius(16)
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("DOREMI")
                        .font(theme::semibold(12.0))
                        .color(palette.accent),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Tu música, a tu ritmo")
                        .font(theme::bold(if ui.available_width() < 400.0 {
                            26.0
                        } else {
                            32.0
                        }))
                        .color(palette.text),
                );
                ui.label(
                    RichText::new("Busca canciones, artistas y álbumes. Guarda tus descubrimientos en este dispositivo.")
                        .font(theme::regular(14.0))
                        .color(palette.secondary),
                );
                ui.add_space(16.0);
                let width = ui.available_width();
                // A disabled scope is one layout item, not a wrapping button.
                // Choose the direction from the whole group's required width.
                let inline = width >= 220.0 + GAP + 250.0;
                let layout = if inline {
                    Layout::left_to_right(Align::Center)
                } else {
                    Layout::top_down(Align::Min)
                };
                let height = if ui.ctx().content_rect().width() >= 760.0 {
                    32.0
                } else {
                    TOUCH
                };
                let height = if inline { height } else { 2.0 * height + GAP };
                ui.allocate_ui_with_layout(Vec2::new(width, height), layout, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(GAP);
                    if self.button(ui, "browse-catalog", "Buscar música", width.min(220.0)) {
                        self.navigate(Page::Search);
                    }
                    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                    ui.add_enabled_ui(
                        self.model.as_ref().unwrap().music.can_refresh_home(),
                        |ui| {
                            if self.button(
                                ui,
                                "youtube-home",
                                if self.model.as_ref().unwrap().music.home_root() {
                                    "Actualizar inicio"
                                } else {
                                    "Explorar YouTube Music"
                                },
                                width.min(250.0),
                            ) {
                                self.model.as_mut().unwrap().refresh_music_home();
                            }
                        },
                    );
                });
            });
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
                let page = self.model.as_ref().unwrap().page;
                let search_root = {
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android"))]
                    {
                        let music = &self.model.as_ref().unwrap().music;
                        !(music.selected && music.detail.is_some())
                    }
                    #[cfg(not(any(
                        target_os = "linux",
                        target_os = "windows",
                        target_os = "android"
                    )))]
                    {
                        true
                    }
                };
                if !wide && page == Page::Search && search_root {
                    // Fixed header: viewport/IME changes and result scrolling
                    // cannot push the active search field under the keyboard.
                    self.search_field(ui);
                }
                if let Some(error) = self
                    .model
                    .as_ref()
                    .unwrap()
                    .storage_warning
                    .or(self.storage_error.map(StorageError::message))
                {
                    ui.colored_label(palette.danger, error);
                }
                let rect = ui.available_rect_before_wrap();
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
        // Explicit spacing also keeps the mobile centering calculation truthful.
        ui.spacing_mut().item_spacing.x = GAP;
        let dimensions = if touch { [24.0, TOUCH] } else { [20.0, 32.0] };
        let shuffle_active = self.model.as_ref().unwrap().queue.shuffled();
        let response = theme::icon_toggle(
            ui,
            palette,
            Icon::Shuffle,
            dimensions,
            shuffle_active,
            if shuffle_active {
                "Aleatorio activado"
            } else {
                "Aleatorio desactivado"
            },
        );
        self.record("shuffle", &response);
        if response.clicked() {
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
        let repeat = self.model.as_ref().unwrap().queue.repeat();
        let (icon, label) = repeat_presentation(repeat);
        let response =
            theme::icon_toggle(ui, palette, icon, dimensions, repeat != Repeat::Off, label);
        self.record("repeat", &response);
        if response.clicked() {
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
        let id = model.queue.current().map(|entry| entry.media.id.clone());
        let caption = playback_caption(model);
        let title = model
            .queue
            .current()
            .map(|entry| entry.media.title.as_str())
            .unwrap_or("Ninguna canción seleccionada")
            .to_owned();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
            theme::cover_placeholder(ui, palette, rect, 4.0, Icon::Music);
            if let Some(id) = &id {
                self.media_artwork(ui, id, rect, 4);
            }
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(title).font(theme::semibold(14.0))).truncate(),
                );
                ui.weak(caption);
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
                self.media_artwork(ui, &media.id, cover, 4);
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
        let id = current.media.id.clone();
        let caption = playback_caption(model);
        let failed = model.queue.playback() == Playback::Failed;
        let notice = model.notice.clone();
        ui.spacing_mut().item_spacing.y = 8.0;
        let height = (self.page_rect[3] - self.page_rect[1]).max(0.0);
        let art = (height * 0.28).clamp(96.0, 256.0).min(ui.available_width());
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(art), egui::Sense::hover());
            theme::cover_placeholder(ui, palette, rect, 8.0, Icon::Music);
            self.media_artwork(ui, &id, rect, 8);
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
        let id = model.queue.current().unwrap().media.id.clone();
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
                self.media_artwork(ui, &id, rect, 8);
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
            ui.spacing_mut().item_spacing.x = GAP;
            let width = 4.0 * TOUCH + 64.0 + 4.0 * GAP;
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
        if media.id.provider.as_str() == "musicbrainz"
            || metadata_kind(&media.id)
                .is_some_and(|kind| kind != doremi_core::media::EntityKind::Recording)
        {
            let Some(kind) = metadata_kind(&media.id) else {
                self.model.as_mut().unwrap().notice =
                    "No se reconoce este identificador de catálogo. Se conserva en la biblioteca."
                        .into();
                return;
            };
            let model = self.model.as_mut().unwrap();
            model.music.selected = true;
            model.music.open(doremi_core::media::CatalogEntry {
                media: media.clone(),
                kind,
                artists: vec![],
                edition: None,
            });
            self.navigate(Page::Search);
            return;
        }
        let playable: Vec<_> = rows
            .iter()
            .filter(|item| {
                item.id.provider.as_str() != "musicbrainz"
                    && metadata_kind(&item.id)
                        .is_none_or(|kind| kind == doremi_core::media::EntityKind::Recording)
            })
            .cloned()
            .collect();
        if !playable.is_empty() {
            let index = rows[..index]
                .iter()
                .filter(|item| {
                    item.id.provider.as_str() != "musicbrainz"
                        && metadata_kind(&item.id)
                            .is_none_or(|kind| kind == doremi_core::media::EntityKind::Recording)
                })
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
                let subtitle = if media.id.provider.as_str() == "musicbrainz"
                    || media.id.provider.as_str() == "youtube-music"
                {
                    match metadata_kind(&media.id) {
                        Some(doremi_core::media::EntityKind::Artist) => {
                            if media.id.provider.as_str() == "musicbrainz" {
                                "Artista · MusicBrainz · metadatos"
                            } else {
                                "Artista · YouTube Music"
                            }
                        }
                        Some(doremi_core::media::EntityKind::Release) => {
                            if media.id.provider.as_str() == "musicbrainz" {
                                "Álbum / edición · MusicBrainz · metadatos"
                            } else {
                                "Álbum · YouTube Music"
                            }
                        }
                        Some(doremi_core::media::EntityKind::Playlist) => {
                            "Playlist · YouTube Music"
                        }
                        _ => {
                            if media.id.provider.as_str() == "youtube-music" {
                                "Canción · YouTube Music"
                            } else {
                                "MusicBrainz · metadatos"
                            }
                        }
                    }
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
                let duration = media.duration_ms.filter(|_| {
                    matches!(
                        media.id.provider.as_str(),
                        "wikimedia-commons" | "youtube-music"
                    )
                });
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
                        round: metadata_kind(&media.id)
                            == Some(doremi_core::media::EntityKind::Artist),
                        actions: 2,
                        action_label: "Seleccionar",
                    },
                );
                self.media_artwork(
                    ui,
                    &media.id,
                    row.cover_rect(),
                    if metadata_kind(&media.id) == Some(doremi_core::media::EntityKind::Artist) {
                        20
                    } else {
                        4
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
                    "Más opciones",
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

fn repeat_presentation(repeat: Repeat) -> (Icon, &'static str) {
    match repeat {
        Repeat::Off => (Icon::Repeat, "Repetición desactivada"),
        Repeat::Context => (Icon::Repeat, "Repetir toda la lista"),
        Repeat::Track => (Icon::Repeat1, "Repetir una canción"),
    }
}

pub(super) fn metadata_kind(id: &MediaId) -> Option<doremi_core::media::EntityKind> {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
    {
        doremi_youtube::entity_kind(id).or_else(|| doremi_musicbrainz::entity_kind(id).ok())
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
    {
        let _ = id;
        None
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

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn home_actions_wrap_inside_narrow_and_desktop_headers() {
        let _guard = crate::tests::UI_TEST_LOCK.lock().unwrap();
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/home-header-layout-test");
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory.clone());
        for light in [false, true] {
            app.model.as_mut().unwrap().prefs.light = light;
            for width in [320.0, 384.0, 440.0, 760.0, 1280.0] {
                for _ in 0..3 {
                    app.controls.clear();
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(width, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ui| app.home_header(ui),
                    );
                    output.textures_delta.clear();
                }
                let search = app
                    .controls
                    .iter()
                    .find(|c| c.name == "browse-catalog")
                    .unwrap();
                assert!(search.rect[0] >= 20.0);
                assert!(search.rect[2] <= width - 20.0);
                assert!(
                    search.rect[1] < 220.0,
                    "Actions must follow the text, not center in the remaining viewport"
                );
                #[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
                {
                    let explore = app
                        .controls
                        .iter()
                        .find(|c| c.name == "youtube-home")
                        .unwrap();
                    assert!(explore.rect[0] >= 20.0);
                    assert!(explore.rect[2] <= width - 20.0);
                    if width < 510.0 {
                        assert!(explore.rect[1] >= search.rect[3] + GAP);
                    } else {
                        assert_eq!(explore.rect[1], search.rect[1]);
                        assert!(explore.rect[0] >= search.rect[2] + GAP);
                    }
                }
            }
        }
        drop(app);
        DoremiApp::shutdown();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn repeat_modes_have_distinct_labels_and_single_track_glyph() {
        let (off_icon, off_label) = repeat_presentation(Repeat::Off);
        let (context_icon, context_label) = repeat_presentation(Repeat::Context);
        let (track_icon, track_label) = repeat_presentation(Repeat::Track);
        assert!(matches!(off_icon, Icon::Repeat));
        assert!(matches!(context_icon, Icon::Repeat));
        assert!(matches!(track_icon, Icon::Repeat1));
        assert_ne!(off_label, context_label);
        assert_ne!(context_label, track_label);
        assert_ne!(off_label, track_label);
    }
}
