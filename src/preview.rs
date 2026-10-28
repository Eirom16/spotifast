//! Isolated offline composition. The legacy executable never enters this module.
use crate::app::App;
use crate::backend::Waker;
use crate::model::{Action, Loadable};
use crate::paths::AppDirs;
use crate::settings::{Settings, ThemeChoice};
use crate::theme::Palette;
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const VERSION: &str = "0.1.0";

pub type Controls = std::collections::BTreeMap<String, [f32; 2]>;

/// Geometry of real controls, for external mouse QA; never synthesizes images.
pub fn note_control(ctx: &egui::Context, name: &str, rect: egui::Rect) {
    ctx.data_mut(|data| {
        let controls = data.get_temp_mut_or_default::<Controls>(egui::Id::new("preview-controls"));
        controls.insert(name.into(), [rect.center().x, rect.center().y]);
    });
}

pub fn controls(ctx: &egui::Context) -> Controls {
    ctx.data(|data| {
        data.get_temp(egui::Id::new("preview-controls"))
            .unwrap_or_default()
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Purple,
    Green,
}

impl Accent {
    pub fn palette(self, mut palette: Palette) -> Palette {
        if self == Self::Purple {
            palette.accent = Color32::from_rgb(0x99, 0x66, 0xe8);
            palette.accent_hover = Color32::from_rgb(0xad, 0x83, 0xed);
            // Dark ink gives >5:1 contrast on #9966e8 (white is <4:1).
            palette.on_accent = Color32::from_rgb(0x10, 0x0c, 0x18);
        }
        palette
    }
}

/// Only appearance preferences, not a copy of the legacy profile.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub accent: Accent,
    pub theme: ThemeChoice,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            accent: Accent::Purple,
            theme: ThemeChoice::Dark,
        }
    }
}

impl Preferences {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
}

/// A portable folder chosen by this launcher only; no profile adoption,
/// default Spotifast directories, registration or platform keyring access.
pub fn dirs(base: &Path) -> AppDirs {
    AppDirs {
        config: base.join("config"),
        state: base.join("state"),
        cache: base.join("cache"),
    }
}

pub fn create(waker: &Waker, dirs: AppDirs, preferences: Preferences) -> App {
    let settings = Settings {
        theme: preferences.theme,
        accent_from_art: false,
        device_name: "Doremi Preview (simulated)".into(),
        ..Settings::default()
    };
    let mut app = App::new_preview(waker, dirs, settings);
    crate::demo::populate(&mut app);
    app.preview = Some(preferences);
    if let Some(user) = &mut app.user {
        user.display_name = Some("Doremi Demo".into());
        user.product = Some("Offline demo — no audio".into());
        user.images.clear();
    }
    app.receivers.clear();
    for device in &mut app.devices {
        device.name = "Doremi Preview (simulated)".into();
    }
    if let Some(remote) = &mut app.remote {
        remote.state.is_playing = false;
        if let Some(device) = &mut remote.state.device {
            device.name = "Demo · no audio".into();
        }
    }
    app
}

/// Allowlist: real navigation, read-only fixtures and appearance only.
/// Unsupported actions cannot reach legacy auth, native integration or updates.
pub(crate) fn handle(app: &mut App, action: &Action, _ctx: &egui::Context) -> bool {
    match action {
        Action::Search(query) => {
            search(app, query);
            true
        }
        Action::Open(_)
        | Action::OpenUri(_)
        | Action::Back
        | Action::Forward
        | Action::FocusSearch
        | Action::EndSearchRun
        | Action::ForgetSearch(_)
        | Action::SetSearchFilter(_)
        | Action::SetTopPeriod(_)
        | Action::SetDiscographyFilter { .. }
        | Action::ToggleSidebar
        | Action::ToggleQueuePanel
        | Action::SetQueueTab(_)
        | Action::SetTheme(_)
        | Action::SettingsChanged
        | Action::ShowDialog(_)
        | Action::CloseDialog
        | Action::Quit
        | Action::PrepareTint(_) => false,
        _ => {
            app.toast(
                "Demo: simulated data only; playback, accounts and system changes are disabled.",
            );
            true
        }
    }
}

pub(crate) fn search(app: &mut App, query: &str) {
    use crate::api::models::{Page, SearchResults};
    let needle = query.trim().to_lowercase();
    let mut tracks: Vec<_> = app
        .track_cache
        .values()
        .filter(|track| {
            needle.is_empty()
                || track.name.to_lowercase().contains(&needle)
                || track
                    .artists
                    .iter()
                    .any(|a| a.name.to_lowercase().contains(&needle))
        })
        .cloned()
        .collect();
    tracks.sort_by(|a, b| a.uri.cmp(&b.uri));
    tracks.truncate(20);
    app.search.query = query.into();
    app.search.committed = query.into();
    app.search.typed_at = None;
    app.search.catalogue_pending = false;
    app.search.playlists_pending = false;
    app.search.results = Loadable::Loaded(SearchResults {
        tracks: Some(Page::whole(tracks)),
        ..Default::default()
    });
    app.open(crate::model::Page::Search);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn purple_is_preview_only_and_green_is_exactly_legacy() {
        for palette in [Palette::dark(), Palette::light()] {
            assert_eq!(Accent::Green.palette(palette), palette);
            assert_eq!(
                Accent::Purple.palette(palette).accent,
                Color32::from_rgb(153, 102, 232)
            );
        }
    }

    #[test]
    fn preferences_roundtrip_without_legacy_fields_or_secrets() {
        let preferences = Preferences {
            accent: Accent::Green,
            theme: ThemeChoice::Light,
        };
        let text = serde_json::to_string(&preferences).unwrap();
        let restored: Preferences = serde_json::from_str(&text).unwrap();
        assert_eq!(restored.accent, Accent::Green);
        assert_eq!(restored.theme, ThemeChoice::Light);
        assert!(serde_json::from_str::<Preferences>(r#"{"token":"bad"}"#).is_err());
    }

    #[test]
    fn preview_blocks_account_updates_playback_and_persists_only_its_appearance() {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/preview-tests")
            .join(std::process::id().to_string());
        let dirs = dirs(&base);
        dirs.ensure().unwrap();
        let mut app = create(&Waker::default(), dirs.clone(), Preferences::default());
        let ctx = egui::Context::default();
        assert!(app.is_preview());
        for action in [Action::SignOut, Action::CheckForUpdates, Action::TogglePlay] {
            app.apply(action, &ctx);
            assert!(app.is_connected());
            assert!(app.update.is_none());
            assert!(!app.now_playing().unwrap().playing);
        }
        app.apply(Action::Search("Rosewood".into()), &ctx);
        let tracks = app.search.results.get().unwrap().tracks.as_ref().unwrap();
        assert!(!tracks.items.is_empty());
        assert!(tracks.items.iter().all(|t| t.name.contains("Rosewood")));
        app.preview.as_mut().unwrap().accent = Accent::Green;
        app.apply(Action::SetTheme(ThemeChoice::Light), &ctx);
        app.shutdown();
        let restored = Preferences::load(&dirs.config.join("preview.json"));
        assert_eq!(restored.accent, Accent::Green);
        assert_eq!(restored.theme, ThemeChoice::Light);
        assert!(!dirs.settings_file().exists());
        assert!(!dirs.session_file().exists());
        assert!(!dirs.credentials_dir().exists());
        assert!(!dirs.proxy_secret_file().exists());
    }
}
