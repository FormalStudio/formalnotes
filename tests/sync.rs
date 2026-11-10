#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Sync against the in-memory server: two devices, each a library in a temp folder with its
//! own state, both signed in to one account. Nothing here touches the network.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use formalnotes::sync::account::{self, AccountError, Keystore};
use formalnotes::sync::crypto::{self, Key};
use formalnotes::sync::engine::{Busy, Engine, Report};
use formalnotes::sync::object::{Kind, Payload};
use formalnotes::sync::remote::{Fake, Push, Remote};
use formalnotes::sync::picture::Picture;
use formalnotes::sync::service::{Command, Event, Service, Status, Task};

struct Device {
    dir: tempfile::TempDir,
    _data: tempfile::TempDir,
    engine: Engine,
    busy: Busy,
}

struct Account {
    fake: Fake,
    token: String,
    vault: Key,
}

fn account() -> Account {
    let fake = Fake::new();
    let token = fake.sign_up("ada@example.com", &"p".repeat(43)).unwrap();
    Account { fake, token, vault: [9; 32] }
}

impl Account {
    fn device(&self) -> Device {
        self.device_with_page(500)
    }

    fn device_with_page(&self, page: u32) -> Device {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let busy = Busy::default();
        let engine = Engine::new(
            dir.path().to_owned(),
            Arc::new(self.fake.clone()),
            self.token.clone(),
            self.vault,
            data.path().join("state.json"),
            busy.clone(),
        )
        .unwrap()
        .with_page(page);
        Device { dir, _data: data, engine, busy }
    }

    fn live(&self) -> usize {
        self.fake.live_blobs("ada@example.com").len()
    }
}

impl Device {
    fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> Option<String> {
        fs::read_to_string(self.path(rel)).ok()
    }

    fn sync(&mut self) -> Report {
        let report = self.engine.sync().unwrap();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        report
    }

    /// Every note, folder and view file, relative and sorted; the trash left out.
    fn files(&self) -> Vec<String> {
        fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == ".trash" {
                    continue;
                }
                let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                if entry.path().is_dir() {
                    out.push(format!("{path}/"));
                    walk(&entry.path(), &path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(self.dir.path(), "", &mut out);
        out.sort();
        out
    }

    fn trashed(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.path(".trash")) else { return Vec::new() };
        let mut out: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
            .collect();
        out.sort();
        out
    }

    fn id_of(&self, path: &str) -> Option<String> {
        self.engine.state().at(path).map(str::to_owned)
    }
}

#[test]
fn create_edit_and_delete_reach_the_other_device() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Plan.md", "# Plan\n");
    a.write("Work/Todo.md", "- ship\n");
    a.sync();
    b.sync();
    assert_eq!(b.files(), ["Plan.md", "Work/", "Work/Todo.md"]);
    assert_eq!(b.read("Work/Todo.md").unwrap(), "- ship\n");

    b.write("Plan.md", "# Plan\n\nMore.\n");
    b.sync();
    a.sync();
    assert_eq!(a.read("Plan.md").unwrap(), "# Plan\n\nMore.\n");

    fs::remove_file(a.path("Work/Todo.md")).unwrap();
    a.sync();
    b.sync();
    assert_eq!(b.files(), ["Plan.md", "Work/"]);
    assert_eq!(b.trashed(), ["Todo.md"], "a remote delete goes to the trash");
    assert_eq!(acc.live(), 2);
}

#[test]
fn the_server_holds_no_text_and_no_names() {
    let acc = account();
    let mut a = acc.device();
    a.write("Secret plans/Launch.md", "launch codes");
    a.sync();
    for (_, blob) in acc.fake.live_blobs("ada@example.com") {
        let bytes = crypto::from_base64(&blob).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("launch") && !text.contains("Secret"), "plaintext on the server");
        assert_eq!(bytes.len(), 1 + 12 + 1024 + 16, "padded to a whole kilobyte");
    }
}

#[test]
fn rename_and_move_keep_the_id() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Ideas.md", "spark");
    a.write("Projects/Roadmap.md", "q4");
    fs::create_dir_all(a.path("Archive")).unwrap();
    a.sync();
    b.sync();
    let note = a.id_of("Ideas.md").unwrap();
    let folder = a.id_of("Projects").unwrap();
    let live = acc.live();

    fs::rename(a.path("Ideas.md"), a.path("Archive/Old ideas.md")).unwrap();
    fs::rename(a.path("Projects"), a.path("Work")).unwrap();
    a.sync();
    assert_eq!(a.id_of("Archive/Old ideas.md").as_deref(), Some(note.as_str()), "a move keeps the id");
    assert_eq!(a.id_of("Work").as_deref(), Some(folder.as_str()), "a folder rename keeps the id");
    assert_eq!(acc.live(), live, "nothing was created or deleted");

    b.sync();
    assert_eq!(b.files(), ["Archive/", "Archive/Old ideas.md", "Work/", "Work/Roadmap.md"]);
    assert!(b.trashed().is_empty(), "a move on one device is a move on the other");
    assert_eq!(b.id_of("Archive/Old ideas.md").as_deref(), Some(note.as_str()));
    assert_eq!(b.id_of("Work").as_deref(), Some(folder.as_str()));
}

#[test]
fn both_edited_keeps_both_texts() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Note.md", "one");
    a.sync();
    b.sync();
    a.write("Note.md", "from a");
    b.write("Note.md", "from b");
    a.sync();
    let report = b.sync();
    assert_eq!(report.conflicts, 1);
    assert_eq!(b.read("Note.md").unwrap(), "from a", "the server's version takes the file");
    let copy: Vec<String> = b.files().into_iter().filter(|f| f.starts_with("Note (conflict ")).collect();
    assert_eq!(copy.len(), 1, "{:?}", b.files());
    assert!(copy[0].ends_with(").md"));
    assert_eq!(b.read(&copy[0]).unwrap(), "from b");

    a.sync();
    assert_eq!(a.files(), b.files());
    assert_eq!(a.read(&copy[0]).unwrap(), "from b", "the conflict copy is a note of its own");
}

#[test]
fn an_edit_wins_over_a_delete_either_way() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Keep.md", "v1");
    a.write("Back.md", "v1");
    a.sync();
    b.sync();

    // Deleted there, edited here: the edit is pushed again.
    fs::remove_file(a.path("Keep.md")).unwrap();
    a.sync();
    b.write("Keep.md", "v2");
    b.sync();
    assert_eq!(b.read("Keep.md").unwrap(), "v2");
    a.sync();
    assert_eq!(a.read("Keep.md").unwrap(), "v2", "the edit comes back to the device that deleted it");

    // Deleted here, edited there: the server's version is restored.
    a.write("Back.md", "v2");
    a.sync();
    fs::remove_file(b.path("Back.md")).unwrap();
    b.sync();
    assert_eq!(b.read("Back.md").unwrap(), "v2");
    a.sync();
    assert_eq!(a.read("Back.md").unwrap(), "v2");
    assert_eq!(acc.live(), 2);
}

#[test]
fn the_same_path_under_two_ids_numbers_the_second() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.sync();
    b.sync();
    a.write("Untitled.md", "from a");
    b.write("Untitled.md", "from b");
    a.sync();
    b.sync();
    assert_eq!(b.files(), ["Untitled 2.md", "Untitled.md"]);
    assert_eq!(b.read("Untitled.md").unwrap(), "from a");
    assert_eq!(b.read("Untitled 2.md").unwrap(), "from b");
    a.sync();
    assert_eq!(a.files(), ["Untitled 2.md", "Untitled.md"]);
    assert_eq!(a.read("Untitled 2.md").unwrap(), "from b");
    assert_eq!(acc.live(), 2);
}

#[test]
fn the_first_sync_adopts_matching_files() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    for d in [&a, &b] {
        d.write("Shared.md", "same text");
        d.write("Folder/Inside.md", "same too");
    }
    a.write("Only a.md", "a");
    b.write("Folder/Only b.md", "b");
    a.sync();
    b.sync();
    assert_eq!(b.id_of("Shared.md"), a.id_of("Shared.md"), "the remote id is adopted");
    assert_eq!(b.id_of("Folder"), a.id_of("Folder"));
    a.sync();
    assert_eq!(a.files(), ["Folder/", "Folder/Inside.md", "Folder/Only b.md", "Only a.md", "Shared.md"]);
    assert_eq!(b.files(), a.files());
    assert_eq!(acc.live(), 5, "no duplicates");
}

#[test]
fn a_push_whose_answer_was_lost_settles() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Lost.md", "first");
    acc.fake.lose_answers(1);
    assert!(a.engine.sync().is_err());
    a.sync();
    assert_eq!(acc.live(), 1, "the retry did not make a second object");

    a.write("Lost.md", "second");
    acc.fake.lose_answers(1);
    assert!(a.engine.sync().is_err());
    let report = a.sync();
    assert_eq!(report.conflicts, 0);
    assert!(a.files().iter().all(|f| !f.contains("conflict")), "{:?}", a.files());
    b.sync();
    assert_eq!(b.read("Lost.md").unwrap(), "second");
    assert_eq!(acc.live(), 1);
}

#[test]
fn many_changes_arrive_over_several_pages() {
    let acc = account();
    let mut a = acc.device();
    for i in 0..12 {
        a.write(&format!("Note {i}.md"), &format!("text {i}"));
    }
    a.sync();
    let mut b = acc.device_with_page(5);
    let report = b.sync();
    assert_eq!(report.pulled, 12);
    assert_eq!(b.files().len(), 12);
    assert_eq!(b.engine.state().cursor, 12);
}

#[test]
fn folder_views_and_empty_folders_sync() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    fs::create_dir_all(a.path("Empty")).unwrap();
    a.write("Tasks/.folder.json", "{\"v\":1,\"view\":\"table\"}");
    a.write("Tasks/One.md", "1");
    a.write(".folder.json", "{\"v\":1,\"view\":\"cards\"}");
    a.sync();
    b.sync();
    assert_eq!(b.files(), [".folder.json", "Empty/", "Tasks/", "Tasks/.folder.json", "Tasks/One.md"]);
    assert_eq!(b.read("Tasks/.folder.json").unwrap(), "{\"v\":1,\"view\":\"table\"}");

    b.write("Tasks/.folder.json", "{\"v\":1,\"view\":\"cards\"}");
    b.sync();
    a.sync();
    assert_eq!(a.read("Tasks/.folder.json").unwrap(), "{\"v\":1,\"view\":\"cards\"}");

    fs::rename(a.path("Empty"), a.path("Still empty")).unwrap();
    a.sync();
    b.sync();
    assert!(b.path("Still empty").is_dir() && !b.path("Empty").exists());
}

#[test]
fn a_remote_change_never_overwrites_unsynced_edits() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Draft.md", "v1");
    a.sync();
    b.sync();
    a.write("Draft.md", "remote edit");
    a.sync();

    // The app holds typing for the note that is not on disk yet: the change waits.
    b.busy.lock().unwrap().insert("Draft.md".into());
    b.sync();
    assert_eq!(b.read("Draft.md").unwrap(), "v1", "nothing is written under unsaved typing");

    // The typing lands on disk; now it is an edit on both sides.
    b.write("Draft.md", "local typing");
    b.busy.lock().unwrap().clear();
    let report = b.sync();
    assert_eq!(report.conflicts, 1);
    assert_eq!(b.read("Draft.md").unwrap(), "remote edit");
    let copy = b.files().into_iter().find(|f| f.contains("(conflict ")).unwrap();
    assert_eq!(b.read(&copy).unwrap(), "local typing");
}

#[test]
fn a_held_change_applies_once_the_typing_is_saved_unchanged() {
    let acc = account();
    let (mut a, mut b) = (acc.device(), acc.device());
    a.write("Draft.md", "v1");
    a.sync();
    b.sync();
    a.write("Draft.md", "v2");
    a.sync();
    b.busy.lock().unwrap().insert("Draft.md".into());
    b.sync();
    b.busy.lock().unwrap().clear();
    b.sync();
    assert_eq!(b.read("Draft.md").unwrap(), "v2");
}

#[test]
fn paths_from_the_server_stay_inside_the_library() {
    let acc = account();
    let mut a = acc.device();
    a.sync();
    let id = crypto::new_id().unwrap();
    let evil = Payload {
        v: 1,
        kind: Kind::Note,
        path: "../escape.md".into(),
        text: Some("x".into()),
        created: 0,
        modified: 0,
    };
    let blob = evil.seal(&acc.vault, &id).unwrap();
    acc.fake.push(&acc.token, &[Push { id, base: 0, deleted: false, blob: Some(blob) }]).unwrap();
    let report = a.engine.sync().unwrap();
    assert_eq!(report.problems.len(), 1, "{report:?}");
    assert!(!a.dir.path().parent().unwrap().join("escape.md").exists());
}

// Accounts.

#[derive(Default)]
struct Memory(Mutex<HashMap<String, String>>);

impl Keystore for Memory {
    fn get(&self, name: &str) -> io::Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }
    fn delete(&self, name: &str) -> io::Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

const PASSWORD: &str = "correct horse battery staple";

#[test]
fn sign_up_and_sign_in_share_one_vault_key() {
    let fake = Fake::new();
    let (one, two) = (Memory::default(), Memory::default());
    let made = account::sign_up(&fake, &one, " Ada@Example.com ", PASSWORD).unwrap();
    assert_eq!(made.email, "ada@example.com");
    assert_eq!(account::load(&one).unwrap().unwrap().vault, made.vault, "the keychain holds the session");

    let signed_in = account::sign_in(&fake, &two, "ada@example.com", PASSWORD).unwrap();
    assert_eq!(signed_in.vault, made.vault);
    assert!(matches!(
        account::sign_in(&fake, &two, "ada@example.com", "wrong password"),
        Err(AccountError::WrongCredentials)
    ));
    assert!(matches!(account::sign_up(&fake, &two, "ada@example.com", PASSWORD), Err(AccountError::Exists)));
    assert!(matches!(account::sign_up(&fake, &two, "bob@example.com", "short"), Err(AccountError::ShortPassword)));
}

#[test]
fn a_sign_up_cut_short_gets_its_keys_at_sign_in() {
    let fake = Fake::new();
    let keys = crypto::Keys::derive("ada@example.com", PASSWORD);
    fake.sign_up("ada@example.com", &keys.auth_password()).unwrap();
    let store = Memory::default();
    let session = account::sign_in(&fake, &store, "ada@example.com", PASSWORD).unwrap();
    assert!(fake.keys(&session.token).unwrap().is_some());
    let again = account::sign_in(&fake, &Memory::default(), "ada@example.com", PASSWORD).unwrap();
    assert_eq!(again.vault, session.vault);
}

#[test]
fn a_new_password_rewraps_the_vault_key_and_ends_other_sessions() {
    let fake = Fake::new();
    let store = Memory::default();
    let session = account::sign_up(&fake, &store, "ada@example.com", PASSWORD).unwrap();
    let other = account::sign_in(&fake, &Memory::default(), "ada@example.com", PASSWORD).unwrap();
    assert!(matches!(
        account::change_password(&fake, &session, "not it at all", "a brand new password"),
        Err(AccountError::WrongPassword)
    ));
    account::change_password(&fake, &session, PASSWORD, "a brand new password").unwrap();
    assert!(fake.usage(&session.token).is_ok(), "this device stays signed in");
    assert!(fake.usage(&other.token).is_err(), "other devices are signed out");
    assert!(account::sign_in(&fake, &Memory::default(), "ada@example.com", PASSWORD).is_err());
    let fresh = account::sign_in(&fake, &Memory::default(), "ada@example.com", "a brand new password").unwrap();
    assert_eq!(fresh.vault, session.vault, "the same vault key under the new password");
}

#[test]
fn signing_out_and_deleting_the_account_keep_local_notes() {
    let fake = Fake::new();
    let store = Memory::default();
    let session = account::sign_up(&fake, &store, "ada@example.com", PASSWORD).unwrap();
    account::sign_out(&fake, &store, &session).unwrap();
    assert!(account::load(&store).unwrap().is_none());
    assert!(fake.usage(&session.token).is_err());

    let session = account::sign_in(&fake, &store, "ada@example.com", PASSWORD).unwrap();
    assert!(matches!(account::delete_account(&fake, &store, &session, "wrong one!"), Err(AccountError::WrongPassword)));
    account::delete_account(&fake, &store, &session, PASSWORD).unwrap();
    assert!(!fake.user_exists("ada@example.com"));
    assert!(account::load(&store).unwrap().is_none());
}

#[test]
fn the_service_signs_up_syncs_and_signs_out() {
    let fake = Fake::new();
    let lib = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::write(lib.path().join("Hello.md"), "# Hello\n").unwrap();
    let (events_tx, events) = flume::unbounded();
    let (commands, _rx) = flume::unbounded::<Command>();
    let mut service = Service::new(
        Arc::new(fake.clone()),
        Arc::new(Memory::default()),
        data.path().to_owned(),
        Busy::default(),
        events_tx,
        commands.downgrade(),
    );
    service.handle(Command::Root(Some(lib.path().to_owned())));
    service.handle(Command::SignUp { email: "ada@example.com".into(), password: PASSWORD.into() });
    let seen: Vec<Event> = events.try_iter().collect();
    assert!(seen.contains(&Event::SignedIn("ada@example.com".into())), "{seen:?}");
    assert!(
        seen.iter().any(|e| matches!(e, Event::Status(Status::Synced { problems, .. }) if problems.is_empty())),
        "{seen:?}"
    );
    assert!(seen.iter().any(|e| matches!(e, Event::Usage(u) if u.objects == 1)), "{seen:?}");

    fake.set_offline(true);
    service.handle(Command::SyncNow);
    assert!(events.try_iter().any(|e| e == Event::Status(Status::Offline)));
    fake.set_offline(false);

    service.handle(Command::SignOut);
    let seen: Vec<Event> = events.try_iter().collect();
    assert!(seen.contains(&Event::SignedOut), "{seen:?}");
    assert!(lib.path().join("Hello.md").exists(), "local notes stay");
}

fn service(fake: &Fake, lib: &Path, data: &Path) -> (Service, flume::Receiver<Event>) {
    let (events_tx, events) = flume::unbounded();
    let (commands, _rx) = flume::unbounded::<Command>();
    let mut service = Service::new(
        Arc::new(fake.clone()),
        Arc::new(Memory::default()),
        data.to_owned(),
        Busy::default(),
        events_tx,
        commands.downgrade(),
    );
    service.handle(Command::Root(Some(lib.to_owned())));
    (service, events)
}

fn pictures(events: &flume::Receiver<Event>) -> Vec<Option<Picture>> {
    events.try_iter().filter_map(|e| if let Event::Picture(p) = e { Some(p) } else { None }).collect()
}

#[test]
fn a_profile_picture_reaches_the_other_device_sealed() {
    let fake = Fake::new();
    let (lib_a, lib_b, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (mut a, events_a) = service(&fake, lib_a.path(), &data.path().join("a"));
    let (mut b, events_b) = service(&fake, lib_b.path(), &data.path().join("b"));
    a.handle(Command::SignUp { email: "ada@example.com".into(), password: PASSWORD.into() });
    b.handle(Command::SignIn { email: "ada@example.com".into(), password: PASSWORD.into() });
    assert_eq!(pictures(&events_a), [None]);

    let file = data.path().join("photo.png");
    image::RgbImage::from_pixel(40, 80, image::Rgb([200, 30, 30])).save(&file).unwrap();
    a.handle(Command::SetPicture(file));
    let shown = pictures(&events_a);
    let Some(Some(picture)) = shown.last() else { panic!("no picture: {shown:?}") };
    let decoded = image::load_from_memory(&picture.jpeg).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (512, 512));

    // The server holds a sealed blob, not the JPEG.
    let blob = fake.avatar_blob("ada@example.com").unwrap();
    assert!(!blob.contains(&formalnotes::sync::crypto::to_base64(&picture.jpeg[..64])));

    b.handle(Command::SyncNow);
    assert_eq!(pictures(&events_b).last(), Some(&Some(picture.clone())));

    b.handle(Command::RemovePicture);
    assert_eq!(pictures(&events_b), [None]);
    a.handle(Command::SyncNow);
    assert_eq!(pictures(&events_a), [None]);

    a.handle(Command::SetPicture(data.path().join("missing.png")));
    assert!(events_a.try_iter().any(|e| matches!(e, Event::Finished(Task::SavingPicture, Some(_)))));
}

#[test]
fn the_fake_follows_the_servers_rules() {
    use formalnotes::sync::remote::{Outcome, RemoteError};
    let acc = account();
    let blob = |id: &str| crypto::to_base64(&crypto::seal(&acc.vault, b"x", id.as_bytes()).unwrap());
    let id = |n: u8| format!("00000000-0000-4000-8000-0000000000{n:02x}");
    let push = |n: u8, base: u64| Push { id: id(n), base, deleted: false, blob: Some(blob(&id(n))) };

    // An id the server never saw, with a base above 0.
    let out = acc.fake.push(&acc.token, &[push(1, 5)]).unwrap();
    assert_eq!(out, vec![Outcome::Conflict { id: id(1), seq: 0, deleted: true, blob: None }]);

    // A malformed change rejects the whole request and keeps nothing.
    let bad = Push { id: "not-an-id".into(), base: 0, deleted: false, blob: Some(blob("x")) };
    let err = acc.fake.push(&acc.token, &[push(2, 0), bad]).unwrap_err();
    assert_eq!(err.code(), Some("bad_request"));
    assert_eq!(acc.fake.usage(&acc.token).unwrap().objects, 0);

    // A blob over 1 MiB is `too_large`.
    let huge = crypto::to_base64(&vec![1u8; 1024 * 1024 + 1]);
    let big = Push { id: id(3), base: 0, deleted: false, blob: Some(huge) };
    assert_eq!(acc.fake.push(&acc.token, &[push(2, 0), big]).unwrap_err().code(), Some("too_large"));
    assert_eq!(acc.fake.usage(&acc.token).unwrap().objects, 0);

    // The account password is exactly 43 characters.
    assert_eq!(acc.fake.sign_up("b@example.com", "short").unwrap_err().code(), Some("password_too_short"));
    assert_eq!(acc.fake.sign_up("b@example.com", &"p".repeat(44)).unwrap_err().code(), Some("password_too_long"));
    let twice = acc.fake.sign_up("ada@example.com", &"p".repeat(43)).unwrap_err();
    assert_eq!(twice.code(), Some("user_already_exists_use_another_email"));
    assert!(matches!(acc.fake.sign_in("ada@example.com", &"q".repeat(43)), Err(RemoteError::Unauthorized)));
    assert_eq!(acc.fake.delete_account(&acc.token, &"q".repeat(43)).unwrap_err().code(), Some("wrong_password"));
}

#[test]
fn an_object_the_server_lost_is_created_again() {
    let acc = account();
    let mut a = acc.device();
    a.write("Hello.md", "hi");
    a.sync();
    assert_eq!(acc.live(), 1);
    acc.fake.forget_objects("ada@example.com");
    a.write("Hello.md", "hi again");
    a.sync();
    assert_eq!(acc.live(), 1);
    let mut b = acc.device();
    b.sync();
    assert_eq!(b.read("Hello.md").unwrap(), "hi again");
}

/// The real server: a throwaway account, two libraries standing in for two devices, deleted at
/// the end. `cargo test --test sync live -- --ignored`, against `FORMALNOTES_SYNC_URL` or the
/// default server.
#[test]
#[ignore = "talks to the real server"]
fn live_round_trip() {
    use formalnotes::sync::remote::{Http, base_url, healthy};
    let url = base_url();
    assert!(healthy(&url), "{url} does not answer /health");
    let remote: Arc<dyn Remote> = Arc::new(Http::new(url));
    let email = format!("roundtrip-{}@example.com", &crypto::new_id().unwrap()[..8]);
    let (one, two) = (Memory::default(), Memory::default());
    let session = account::sign_up(&*remote, &one, &email, PASSWORD).unwrap();
    let other = account::sign_in(&*remote, &two, &email, PASSWORD).unwrap();
    assert_eq!(other.vault, session.vault);

    let device = |s: &account::Session| {
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let engine = Engine::new(
            dir.path().to_owned(),
            remote.clone(),
            s.token.clone(),
            s.vault,
            data.path().join("s.json"),
            Busy::default(),
        )
        .unwrap();
        Device { dir, _data: data, engine, busy: Busy::default() }
    };
    let (mut a, mut b) = (device(&session), device(&other));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        a.write("Journal/Slow start.md", "# Slow start\n\nCoffee, then café.\n");
        a.write("Journal/.folder.json", "{\"v\":1,\"view\":\"table\"}");
        a.sync();
        b.sync();
        assert_eq!(b.read("Journal/Slow start.md").unwrap(), "# Slow start\n\nCoffee, then café.\n");
        b.write("Journal/Slow start.md", "edited on b");
        fs::rename(b.path("Journal"), b.path("Diary")).unwrap();
        b.sync();
        a.sync();
        assert_eq!(a.read("Diary/Slow start.md").unwrap(), "edited on b");
        a.write("Diary/Slow start.md", "from a");
        b.write("Diary/Slow start.md", "from b");
        a.sync();
        assert_eq!(b.sync().conflicts, 1);
        a.sync();
        assert_eq!(a.files(), b.files());
        let usage = remote.usage(&session.token).unwrap();
        assert_eq!(usage.objects, 4, "{usage:?}");
        fs::remove_file(a.path("Diary/Slow start.md")).unwrap();
        a.sync();
        b.sync();
        assert!(b.read("Diary/Slow start.md").is_none());
        assert_eq!(b.trashed().len(), 1, "{:?}", b.trashed());
        assert_eq!(remote.usage(&session.token).unwrap().objects, 3);
    }));
    account::delete_account(&*remote, &one, &session, PASSWORD).unwrap();
    assert!(remote.usage(&session.token).is_err(), "the account is gone");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

/// Interop with the reference client (`server/scripts/roundtrip.ts` helpers): phase one pushes
/// from a new account and writes its credentials to `INTEROP_FILE`; the Bun script checks the
/// objects, adds one of its own; phase two pulls it and deletes the account.
#[test]
#[ignore = "talks to the real server"]
fn live_interop_push() {
    use formalnotes::sync::remote::{Http, base_url};
    let remote: Arc<dyn Remote> = Arc::new(Http::new(base_url()));
    let email = format!("interop-{}@example.com", &crypto::new_id().unwrap()[..8]);
    let session = account::sign_up(&*remote, &Memory::default(), &email, PASSWORD).unwrap();
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(
        dir.path().to_owned(),
        remote.clone(),
        session.token.clone(),
        session.vault,
        data.path().join("s.json"),
        Busy::default(),
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("Work")).unwrap();
    fs::write(dir.path().join("Work/Plan.md"), "# Plan\n\nCafé, naïve, 日本語.\n").unwrap();
    fs::write(dir.path().join("Work/.folder.json"), "{\"v\":1,\"view\":\"table\"}").unwrap();
    engine.sync().unwrap();
    let out = std::env::var("INTEROP_FILE").expect("INTEROP_FILE");
    fs::write(out, serde_json::json!({ "email": email, "password": PASSWORD }).to_string()).unwrap();
}

#[test]
#[ignore = "talks to the real server"]
fn live_interop_pull() {
    use formalnotes::sync::remote::{Http, base_url};
    let remote: Arc<dyn Remote> = Arc::new(Http::new(base_url()));
    let creds: serde_json::Value =
        serde_json::from_slice(&fs::read(std::env::var("INTEROP_FILE").unwrap()).unwrap()).unwrap();
    let email = creds["email"].as_str().unwrap();
    let session = account::sign_in(&*remote, &Memory::default(), email, PASSWORD).unwrap();
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(
        dir.path().to_owned(),
        remote.clone(),
        session.token.clone(),
        session.vault,
        data.path().join("s.json"),
        Busy::default(),
    )
    .unwrap();
    let report = engine.sync().unwrap();
    let text = fs::read_to_string(dir.path().join("FromBun.md"));
    account::delete_account(&*remote, &Memory::default(), &session, PASSWORD).unwrap();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(text.unwrap(), "# From Bun\n\nSealed by WebCrypto.\n");
}
