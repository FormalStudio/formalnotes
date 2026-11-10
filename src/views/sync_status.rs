//! The sync line at the foot of the sidebar. Quiet while all is well: it shows the first sync
//! after signing in, being offline, and anything that stopped a sync, and nothing otherwise.

use nd::prelude::*;

use crate::sync::{Status, SyncModel};

pub struct SyncStatus {
    model: Entity<SyncModel>,
    /// A sync has finished since the last sign-in or failure, so the routine ones stay quiet.
    settled: bool,
    _model: Subscription,
}

impl SyncStatus {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let model = crate::sync::model(cx);
        let sub = cx.observe(&model, |this, model, cx| {
            match &model.read(cx).view.status {
                Status::Synced { .. } => this.settled = true,
                Status::Syncing => {}
                _ => this.settled = false,
            }
            cx.notify();
        });
        Self { model, settled: false, _model: sub }
    }

    /// What the line says, or nothing.
    pub fn line(&self, cx: &App) -> Option<(Icon, String)> {
        match &self.model.read(cx).view.status {
            Status::Syncing if !self.settled => Some((Icon::RotateCw, "Syncing…".into())),
            Status::Offline => Some((Icon::TriangleAlert, "Offline".into())),
            Status::Failed(_) => Some((Icon::TriangleAlert, "Sync stopped".into())),
            Status::Synced { problems, .. } if !problems.is_empty() => {
                Some((Icon::TriangleAlert, format!("{} could not sync", crate::views::format::items(problems.len()))))
            }
            _ => None,
        }
    }
}

impl Render for SyncStatus {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some((glyph, line)) = self.line(cx) else { return div().id("sync-status").into_any_element() };
        let detail = match &self.model.read(cx).view.status {
            Status::Failed(why) => why.clone(),
            Status::Synced { problems, .. } => problems.join("\n"),
            Status::Offline => "Changes sync once the server can be reached.".into(),
            _ => line.clone(),
        };
        h_flex()
            .id("sync-status")
            .gap(theme.spacing.xs)
            .items_center()
            .px(theme.spacing.md)
            .py(theme.spacing.xs)
            .child(icon(glyph, 12.).text_color(theme.text.secondary))
            .child(button("sync-status-line", line).borderless().tooltip(detail).on_click(|_: &ClickEvent, _, cx| {
                crate::settings::open_tab(cx, crate::settings::ACCOUNT_TAB);
            }))
            .into_any_element()
    }
}
