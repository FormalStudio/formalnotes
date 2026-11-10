//! The Account pane of the Settings window: sign in or create an account, and once signed in
//! the account's use, the last sync and the account commands. Every request runs on the sync
//! thread; the pane shows what it is doing and what went wrong.

use nd::components::FormRow;
use nd::prelude::*;

use crate::sync::{Status, SyncModel, SyncView, Task};

/// What the pane is asking for under the account rows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asking {
    NewPassword,
    DeleteAccount,
}

pub struct AccountPane {
    model: Entity<SyncModel>,
    email: Entity<TextFieldState>,
    password: Entity<TextFieldState>,
    new_password: Entity<TextFieldState>,
    asking: Option<Asking>,
    /// A request was running at the last update: the one that ends closes what it asked for.
    was_busy: bool,
    _model: Subscription,
}

fn bytes(n: u64) -> String {
    match n {
        0..1_000 => format!("{n} bytes"),
        1_000..1_000_000 => format!("{:.0} KB", n as f64 / 1e3),
        _ => format!("{:.1} MB", n as f64 / 1e6),
    }
}

fn task_label(task: Task) -> &'static str {
    match task {
        Task::SigningIn => "Signing in…",
        Task::CreatingAccount => "Creating your account…",
        Task::SigningOut => "Signing out…",
        Task::ChangingPassword => "Changing the password…",
        Task::DeletingAccount => "Deleting the account…",
    }
}

impl AccountPane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = crate::sync::model(cx);
        let sub = cx.observe(&model, |this, model, cx| {
            let view = &model.read(cx).view;
            let busy = view.task.is_some();
            if this.was_busy && !busy && view.failure.is_none() {
                this.asking = None;
            }
            this.was_busy = busy;
            cx.notify();
        });
        Self {
            model,
            email: cx.new(|cx| TextFieldState::new(window, cx)),
            password: cx.new(|cx| TextFieldState::new(window, cx)),
            new_password: cx.new(|cx| TextFieldState::new(window, cx)),
            asking: None,
            was_busy: false,
            _model: sub,
        }
    }

    fn take_password(&mut self, window: &mut Window, cx: &mut Context<Self>) -> String {
        let password = self.password.read(cx).text().to_string();
        self.password.update(cx, |s, cx| s.set_text("", window, cx));
        password
    }

    fn submit(&mut self, create: bool, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.email.read(cx).text().trim().to_owned();
        if email.is_empty() || self.password.read(cx).text().is_empty() {
            return;
        }
        let password = self.take_password(window, cx);
        self.model.update(cx, |m, _| if create { m.sign_up(email, password) } else { m.sign_in(email, password) });
    }

    fn change_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.take_password(window, cx);
        let new = self.new_password.read(cx).text().to_string();
        self.new_password.update(cx, |s, cx| s.set_text("", window, cx));
        self.model.update(cx, |m, _| m.change_password(current, new));
    }

    fn delete_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let email = self.model.read(cx).view.email.clone().unwrap_or_default();
        let this = cx.weak_entity();
        let alert = Alert::confirm("Delete your account?", "Delete account").message(format!(
            "Everything the server holds for {email} is deleted for good. The notes on this device stay."
        ));
        window.alert(cx, alert, move |answer, cx| {
            if answer.0 != 0 {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                let password = this.password.read(cx).text().to_string();
                this.model.update(cx, |m, _| m.delete_account(password));
            });
        });
    }

    fn ask(&mut self, asking: Option<Asking>, window: &mut Window, cx: &mut Context<Self>) {
        self.asking = asking;
        self.password.update(cx, |s, cx| s.set_text("", window, cx));
        self.new_password.update(cx, |s, cx| s.set_text("", window, cx));
        cx.notify();
    }

    fn progress_row(&self, model: &SyncView, cx: &mut Context<Self>) -> Option<FormRow> {
        let theme = cx.theme().clone();
        if let Some(task) = model.task {
            return Some(
                form_row("").id("account-progress").child(
                    h_flex()
                        .gap(theme.spacing.sm)
                        .items_center()
                        .child(spinner())
                        .child(text(task_label(task)).text_color(theme.text.secondary)),
                ),
            );
        }
        let (_, failure) = model.failure.as_ref()?;
        Some(form_row("").id("account-failure").child(text(failure.clone()).text_color(theme.text.danger)))
    }

    fn signed_out(&self, model: &SyncView, cx: &mut Context<Self>) -> AnyElement {
        let busy = model.task.is_some();
        let field_w = px(260.);
        let section = form_section()
            .title("Account")
            .footer("Notes are encrypted on this device before they sync. A forgotten password cannot be reset.")
            .child(form_row("Email").child(
                text_field("account-email").bind(&self.email).label("Email").placeholder("name@example.com").w(field_w),
            ))
            .child(
                form_row("Password").child(
                    secure_field("account-password")
                        .bind(&self.password)
                        .label("Password")
                        .w(field_w)
                        .on_submit(cx.listener(|this, _: &TextSubmitted, window, cx| this.submit(false, window, cx))),
                ),
            )
            .child(
                form_row("").id("account-actions").child(
                    h_flex()
                        .gap(cx.theme().spacing.sm)
                        .child(
                            button("account-create", "Create account").disabled(busy).on_click(
                                cx.listener(|this, _: &ClickEvent, window, cx| this.submit(true, window, cx)),
                            ),
                        )
                        .child(
                            button("account-sign-in", "Sign in").primary().disabled(busy).on_click(
                                cx.listener(|this, _: &ClickEvent, window, cx| this.submit(false, window, cx)),
                            ),
                        ),
                ),
            )
            .children(self.progress_row(model, cx));
        form().child(section).into_any_element()
    }

    fn last_sync(status: &Status) -> String {
        match status {
            Status::Synced { at, problems } if problems.is_empty() => {
                crate::views::format::relative(*at, jiff::Timestamp::now())
            }
            Status::Synced { problems, .. } => {
                format!("{} could not sync", crate::views::format::items(problems.len()))
            }
            Status::Syncing => "Syncing…".into(),
            Status::Waiting => "Not yet".into(),
            Status::Offline => "Offline, will try again".into(),
            Status::Failed(why) => why.clone(),
            Status::SignedOut => String::new(),
        }
    }

    fn signed_in(&self, model: &SyncView, email: &str, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let busy = model.task.is_some();
        let field_w = px(220.);
        let usage = model.usage.as_ref().map_or_else(
            || "…".to_owned(),
            |u| format!("{}, {}", crate::views::format::items(u.objects as usize), bytes(u.bytes)),
        );
        let account = form_section()
            .title("Account")
            .footer("Notes are encrypted on this device before they sync. A forgotten password cannot be reset.")
            .child(form_row("Email").child(text(email.to_owned()).text_color(theme.text.secondary)))
            .child(form_row("Stored").child(text(usage).text_color(theme.text.secondary)))
            .child(
                form_row("Last synced").child(
                    h_flex()
                        .gap(theme.spacing.sm)
                        .items_center()
                        .child(text(Self::last_sync(&model.status)).text_color(theme.text.secondary))
                        .child(
                            button("account-sync-now", "Sync now")
                                .disabled(busy || model.status == Status::Syncing)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.model.update(cx, |m, _| m.sync_now())
                                })),
                        ),
                ),
            );
        let commands = match self.asking {
            None => form_section()
                .child(form_row("Password").child(
                    button("account-change-password", "Change password…").disabled(busy).on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.ask(Some(Asking::NewPassword), window, cx)),
                    ),
                ))
                .child(form_row("This device").child(
                    button("account-sign-out", "Sign out").disabled(busy).on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.model.update(cx, |m, _| m.sign_out())),
                    ),
                ))
                .child(form_row("Account").child(
                    button("account-delete", "Delete account…").destructive().disabled(busy).on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| this.ask(Some(Asking::DeleteAccount), window, cx),
                    )),
                )),
            Some(Asking::NewPassword) => form_section()
                .title("Change password")
                .footer("Other devices are signed out and sign in again with the new password.")
                .child(form_row("Current password").child(
                    secure_field("account-current-password").bind(&self.password).label("Current password").w(field_w),
                ))
                .child(form_row("New password").child(
                    secure_field("account-new-password").bind(&self.new_password).label("New password").w(field_w),
                ))
                .child(
                    form_row("").id("account-password-actions").child(
                        h_flex()
                            .gap(theme.spacing.sm)
                            .child(
                                button("account-password-cancel", "Cancel").disabled(busy).on_click(
                                    cx.listener(|this, _: &ClickEvent, window, cx| this.ask(None, window, cx)),
                                ),
                            )
                            .child(
                                button("account-password-save", "Change password").primary().disabled(busy).on_click(
                                    cx.listener(|this, _: &ClickEvent, window, cx| this.change_password(window, cx)),
                                ),
                            ),
                    ),
                ),
            Some(Asking::DeleteAccount) => {
                form_section()
                    .title("Delete account")
                    .footer("Enter your password to delete the account and everything stored with it.")
                    .child(form_row("Password").child(
                        secure_field("account-delete-password").bind(&self.password).label("Password").w(field_w),
                    ))
                    .child(
                        form_row("").id("account-delete-actions").child(
                            h_flex()
                                .gap(theme.spacing.sm)
                                .child(button("account-delete-cancel", "Cancel").disabled(busy).on_click(
                                    cx.listener(|this, _: &ClickEvent, window, cx| this.ask(None, window, cx)),
                                ))
                                .child(
                                    button("account-delete-confirm", "Delete account…")
                                        .destructive()
                                        .disabled(busy)
                                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                            this.delete_account(window, cx)
                                        })),
                                ),
                        ),
                    )
            }
        };
        form().child(account).child(commands.children(self.progress_row(model, cx))).into_any_element()
    }
}

impl Render for AccountPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.model.read(cx).view.clone();
        match view.email.clone() {
            Some(email) => self.signed_in(&view, &email, cx),
            None => self.signed_out(&view, cx),
        }
    }
}
