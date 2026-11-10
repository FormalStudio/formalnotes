//! The Assistant pane of the Settings window: the API key and, while sharing is on, where a
//! device connects. The key goes to the keychain, never to the settings file, and is never
//! shown again once saved: the row says whether one is stored.

use nd::prelude::*;

use crate::ai::chat::Provider;
use crate::settings::NotesSettings;

#[derive(Clone, Debug, PartialEq)]
enum Stored {
    Unknown,
    Yes,
    No,
    Failed(String),
    /// The provider takes no key.
    NotNeeded,
}

pub struct ApiKey {
    field: Entity<TextFieldState>,
    provider: Option<Provider>,
    stored: Stored,
}

impl ApiKey {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self { field: cx.new(|cx| TextFieldState::new(window, cx)), provider: None, stored: Stored::Unknown }
    }

    /// Reads, off the main thread, whether the provider has a key stored.
    fn refresh(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.provider = Some(provider);
        self.stored = Stored::Unknown;
        let this = cx.weak_entity();
        let Some(account) = provider.key_account() else {
            self.stored = Stored::NotNeeded;
            return;
        };
        nd::system::spawn(async move { crate::secrets::get(account) }, move |found, cx| {
            let _ = this.update(cx, |this, cx| {
                if this.provider == Some(provider) {
                    this.stored = match found {
                        Ok(Some(key)) if !key.trim().is_empty() => Stored::Yes,
                        Ok(_) => Stored::No,
                        Err(e) => Stored::Failed(e.to_string()),
                    };
                    cx.notify();
                }
            });
        });
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(provider), key) = (self.provider, self.field.read(cx).text().trim().to_owned()) else { return };
        if key.is_empty() {
            return;
        }
        self.field.update(cx, |f, cx| f.set_text("", window, cx));
        self.write(provider, Some(key), cx);
    }

    fn write(&mut self, provider: Provider, key: Option<String>, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let Some(account) = provider.key_account() else { return };
        nd::system::spawn(
            async move {
                match key {
                    Some(key) => crate::secrets::set(account, &key),
                    None => crate::secrets::delete(account),
                }
            },
            move |_, cx| {
                let _ = this.update(cx, |this, cx| this.refresh(provider, cx));
            },
        );
    }
}

impl ApiKey {
    fn key_section(&self, provider: Provider, cx: &mut Context<Self>) -> FormSection {
        if self.stored == Stored::NotNeeded {
            return form_section()
                .title("API key")
                .footer(format!("{} runs on this computer, on the account it is signed in to there.", provider.name()))
                .child(form_row(provider.name()).id("api-key.row").child(label("Needs no key").secondary()));
        }
        let footer = match &self.stored {
            Stored::Unknown | Stored::NotNeeded => "Kept in your keychain, never in the settings file.".to_owned(),
            Stored::Yes => format!("Your {} key is in your keychain. Save a new one to replace it.", provider.name()),
            Stored::No => format!("No {} key yet. Paste one to use Ask and the automations.", provider.name()),
            Stored::Failed(e) => format!("Could not read the keychain: {e}"),
        };
        form_section().title("API key").footer(footer).child(
            form_row(format!("{} key", provider.name())).id("api-key.row").child(
                h_flex()
                    .gap(cx.theme().spacing.sm)
                    .child(
                        secure_field("api-key")
                            .bind(&self.field)
                            .label("API key")
                            .placeholder(if self.stored == Stored::Yes { "Saved" } else { "Paste your key" })
                            .w(px(220.))
                            .on_submit(cx.listener(|this, _: &TextSubmitted, window, cx| this.save(window, cx))),
                    )
                    .child(
                        button("api-key-save", "Save")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.save(window, cx))),
                    )
                    .when(self.stored == Stored::Yes, |row| {
                        row.child(
                            button("api-key-remove", "Remove").on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| this.write(provider, None, cx)),
                            ),
                        )
                    }),
            ),
        )
    }
}

impl Render for ApiKey {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let provider = cx.settings::<NotesSettings>().assistant.provider;
        if self.provider != Some(provider) {
            self.refresh(provider, cx);
        }
        let key = self.key_section(provider, cx);
        form().child(key).children(super::sharing::section(cx))
    }
}
