//! Settings: `config.toml` in the app's config folder, edited by the settings sheet.

use std::path::PathBuf;

use nd::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ai::{AssistantSettings, BrainDumpSettings};
use crate::library::Sort;

#[derive(Settings, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(default, deny_unknown_fields)]
pub struct NotesSettings {
    /// The folder your notes live in. Empty means "Formal Notes" in your Documents folder.
    pub library_root: String,
    /// How wide the editor's text column grows.
    pub editor_width: EditorWidth,
    /// Light, dark, or whatever the system uses.
    pub theme: ThemeChoice,
    /// How Home and folder pages order their cards.
    pub sort: SortOrder,
    /// The model Ask and the automations use. The API key is kept in the keychain.
    #[settings(section)]
    pub assistant: AssistantSettings,
    #[settings(section)]
    pub brain_dumps: BrainDumpSettings,
}

impl Default for NotesSettings {
    fn default() -> Self {
        Self {
            library_root: String::new(),
            editor_width: EditorWidth::Standard,
            theme: ThemeChoice::System,
            sort: SortOrder::Modified,
            assistant: AssistantSettings::default(),
            brain_dumps: BrainDumpSettings::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(rename_all = "kebab-case")]
pub enum EditorWidth {
    /// 820 points.
    Standard,
    /// 1020 points.
    Wide,
    /// The whole pane, less its margins.
    Full,
}

impl EditorWidth {
    /// The column's widest, in points; `None` fills the pane.
    pub fn max(self) -> Option<f32> {
        match self {
            EditorWidth::Standard => Some(820.),
            EditorWidth::Wide => Some(1020.),
            EditorWidth::Full => None,
        }
    }

    /// The text column's width in a pane `pane` points wide: at most [`Self::max`], and never
    /// closer than `margin` to either side.
    pub fn column(self, pane: f32, margin: f32) -> f32 {
        let room = (pane - 2. * margin).max(1.);
        self.max().map_or(room, |max| max.min(room))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(rename_all = "kebab-case")]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(rename_all = "kebab-case")]
pub enum SortOrder {
    /// Last edited first.
    Modified,
    /// Newest first.
    Created,
    /// A to Z.
    Title,
}

impl From<SortOrder> for Sort {
    fn from(order: SortOrder) -> Self {
        match order {
            SortOrder::Modified => Sort::Modified,
            SortOrder::Created => Sort::Created,
            SortOrder::Title => Sort::Title,
        }
    }
}

impl SortOrder {
    pub const ALL: [SortOrder; 3] = [SortOrder::Modified, SortOrder::Created, SortOrder::Title];

    pub fn label(self) -> &'static str {
        match self {
            SortOrder::Modified => "Date Modified",
            SortOrder::Created => "Date Created",
            SortOrder::Title => "Title",
        }
    }
}

impl NotesSettings {
    /// The library folder this setting names, or the default.
    pub fn root(&self) -> PathBuf {
        let set = self.library_root.trim();
        if set.is_empty() {
            return default_root();
        }
        match set.strip_prefix("~/") {
            Some(rest) => home().join(rest),
            None => PathBuf::from(set),
        }
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

pub fn default_root() -> PathBuf {
    home().join("Documents").join("Formal Notes")
}

/// Writes one setting through the loaded handle, as the settings sheet would. Without one (a
/// test that provided none) nothing is written.
pub fn set<V: Serialize>(cx: &mut App, key: &str, value: &V) {
    if let Some(source) = cx.try_global::<nd::components::forms::settings::SettingsSource<NotesSettings>>() {
        let _ = source.0.set(key, value);
        cx.refresh_windows();
    }
}

/// The Settings window: the settings form, applied as it changes, as Mac settings windows are.
pub struct SettingsWindow {
    api_key: Entity<crate::views::api_key::ApiKey>,
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .size_full()
            .p(theme.spacing.lg)
            .child(div().flex_1().min_h_0().child(settings_form::<NotesSettings>()))
            .child(self.api_key.clone())
    }
}

/// Opens the Settings window, or brings the open one forward.
pub fn open_window(cx: &mut App) -> Option<WindowHandle<SettingsWindow>> {
    open_settings(cx, |window, cx| {
        let api_key = cx.new(|cx| crate::views::api_key::ApiKey::new(window, cx));
        cx.new(|_| SettingsWindow { api_key })
    })
}

#[cfg(test)]
mod tests {
    use super::EditorWidth;

    #[test]
    fn the_column_keeps_its_margins_and_its_width() {
        assert_eq!(EditorWidth::Standard.column(1000., 66.), 820.);
        assert_eq!(EditorWidth::Standard.column(800., 66.), 668.);
        assert_eq!(EditorWidth::Wide.column(1200., 66.), 1020.);
        assert_eq!(EditorWidth::Full.column(1200., 66.), 1068.);
        assert_eq!(EditorWidth::Full.column(100., 66.), 1.);
    }
}
