#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::{Duration, SystemTime};

use nd_test::{LaunchOptions, WaitState, expect, launch_app};

#[test]
#[ignore = "calls the real gateway; run with LIVE_PROVIDER and LIVE_THEME set and --run-ignored"]
fn live() {
    let provider = std::env::var("LIVE_PROVIDER").unwrap();
    let theme = std::env::var("LIVE_THEME").unwrap();
    let out = std::path::PathBuf::from("/tmp/fn-live");
    let (model, base) = if provider == "anthropic" {
        ("anthropic/claude-sonnet-5", "https://canaryllm.canarycoders.es")
    } else {
        ("openai/gpt-5.4", "https://canaryllm.canarycoders.es/v1")
    };
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("config.toml"),
        format!(
            "theme = \"{theme}\"\n[assistant]\nprovider = \"{provider}\"\nmodel = \"{model}\"\nbase_url = \"{base}\"\n\
             [brain_dumps]\norganize = true\ninbox_folder = \"Inbox\"\nidle_minutes = 1\n"
        ),
    )
    .unwrap();
    let app = launch_app(
        LaunchOptions::new(env!("CARGO_BIN_EXE_formalnotes"))
            .env("FORMALNOTES_DEMO", "1")
            .env("FORMALNOTES_LIVE_KEY_FILE", format!("{}/.claude/secrets/canaryllm-api-key", std::env::var("HOME").unwrap()))
            .env("ND_APP_CONFIG_DIR", config.path().to_string_lossy()),
    )
    .expect("launch");
    expect(&app.get_by_test_id("face:folder:Recipes")).to_be_visible().unwrap();

    let root = std::env::temp_dir().join(format!("formalnotes-demo-{}", app.pid()));
    std::fs::create_dir_all(root.join("Inbox")).unwrap();
    let dump = root.join("Inbox/Brain dump.md");
    std::fs::write(
        &dump,
        "call the dentist about the crown thursday\nfocaccia next time: less salt on top, try rosemary\n\
         idea for the garden: raised bed by the fence, tomatoes and basil\nbuy lemons and oat milk",
    )
    .unwrap();
    std::fs::File::options()
        .write(true)
        .open(&dump)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(600))
        .unwrap();

    std::thread::sleep(Duration::from_secs(1));
    app.get_by_test_id("ask").first().click().unwrap();
    let field = app.get_by_test_id("ask-field");
    field.fill("How long does the focaccia bake, and at what temperature?").unwrap();
    field.press("enter").unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let done = app.get_by_test_id("ask-stop").with_timeout(Duration::from_secs(120)).wait_for(WaitState::Detached);
    eprintln!("ask done: {done:?}\n{}", app.stderr_tail(60).join("\n"));
    std::thread::sleep(Duration::from_millis(800));
    let id = std::process::Command::new("/tmp/fn-live/winid").arg(app.pid().to_string()).output().unwrap();
    let id = String::from_utf8_lossy(&id.stdout).trim().to_owned();
    let shot = out.join(format!("ask-{provider}-{theme}.png"));
    std::process::Command::new("screencapture").args(["-x", "-o", "-l", &id]).arg(&shot).status().unwrap();
    std::fs::write(out.join(format!("ask-{provider}-{theme}.txt")), app.snapshot().unwrap()).unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(240);
    while dump.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(3));
        let rss = std::process::Command::new("ps").args(["-o", "rss=", "-p", &app.pid().to_string()]).output().unwrap();
        eprintln!("rss kb {}", String::from_utf8_lossy(&rss.stdout).trim());
    }
    std::thread::sleep(Duration::from_secs(1));
    app.screenshot(&out.join(format!("organized-{provider}-{theme}.png")), None).unwrap();
    std::fs::write(out.join(format!("organized-{provider}-{theme}.txt")), app.snapshot().unwrap()).unwrap();
    let mut files = Vec::new();
    for e in walk(&root) {
        files.push(format!("== {}\n{}", e.strip_prefix(&root).unwrap().display(), std::fs::read_to_string(&e).unwrap_or_default()));
    }
    std::fs::write(out.join(format!("tree-{provider}-{theme}.txt")), files.join("\n")).unwrap();
    assert!(!dump.exists(), "the dump was not organized");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p);
        }
    }
    out
}
