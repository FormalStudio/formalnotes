//! Settings: `config.toml` in the app's config folder, edited by the settings sheet.

use std::path::PathBuf;

use nd::prelude::*;
use serde::{Deserialize, Serialize};

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
}

impl Default for NotesSettings {
    fn default() -> Self {
        Self {
            library_root: String::new(),
            editor_width: EditorWidth::Standard,
            theme: ThemeChoice::System,
            sort: SortOrder::Modified,
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

pub const GENERAL_TAB: &str = "general";
pub const ACCOUNT_TAB: &str = "account";

/// The Settings window: the settings form, applied as it changes, as Mac settings windows are,
/// and the sync account.
pub struct SettingsWindow {
    tab: SharedString,
    account: Entity<crate::views::account::AccountPane>,
}

impl SettingsWindow {
    pub fn select(&mut self, tab: &str, cx: &mut Context<Self>) {
        self.tab = SharedString::from(tab.to_owned());
        cx.notify();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let tabs = toolbar()
            .tab(GENERAL_TAB, "General", Icon::Settings)
            .tab(ACCOUNT_TAB, "Account", Icon::User)
            .selected_tab(self.tab.clone())
            .on_tab(cx.listener(|this, e: &TabSelected, _, cx| this.select(&e.id, cx)));
        let body = if self.tab.as_ref() == ACCOUNT_TAB {
            div().size_full().p(theme.spacing.lg).child(self.account.clone())
        } else {
            div().size_full().p(theme.spacing.lg).child(settings_form::<NotesSettings>())
        };
        div().size_full().child(tabs).child(body)
    }
}

/// Opens the Settings window, or brings the open one forward.
pub fn open_window(cx: &mut App) -> Option<WindowHandle<SettingsWindow>> {
    open_tab(cx, GENERAL_TAB)
}

/// Opens the Settings window on `tab`, or brings the open one forward on it.
pub fn open_tab(cx: &mut App, tab: &str) -> Option<WindowHandle<SettingsWindow>> {
    if let Some(OpenWindow(open)) = cx.try_global::<OpenWindow>() {
        let _ = open.clone().update(cx, |this, cx| this.select(tab, cx));
    }
    let tab = SharedString::from(tab.to_owned());
    let handle = open_settings(cx, move |window, cx| {
        let account = cx.new(|cx| crate::views::account::AccountPane::new(window, cx));
        cx.new(|_| SettingsWindow { tab, account })
    })?;
    cx.set_global(OpenWindow(handle.root().downgrade()));
    Some(handle)
}

/// The Settings window once it has opened, so a second request can switch its tab.
struct OpenWindow(WeakEntity<SettingsWindow>);

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
