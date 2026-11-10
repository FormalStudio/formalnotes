//! `FORMALNOTES_DEMO=1` opens a fixed demo library instead of the user's: screenshots and tests
//! start from it and never touch real notes.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::library::disk;
use crate::library::view::VIEW_FILE;

pub fn enabled() -> bool {
    std::env::var_os("FORMALNOTES_DEMO").is_some()
}

/// Folders, then notes as (path, minutes since last edit, text).
const FOLDERS: &[&str] = &["Projects", "Projects/Launch", "Journal", "Reading", "Recipes"];

/// Folders shown as a table, with their `.folder.json`.
const VIEWS: &[(&str, &str)] = &[(
    "Reading",
    r#"{
  "v": 1,
  "view": "table",
  "columns": [
    { "name": "Status", "type": "select", "options": ["To read", "Reading", "Finished"], "width": 110 },
    { "name": "Author", "type": "text", "width": 150 },
    { "name": "Rating", "type": "number", "width": 70 },
    { "name": "Finished", "type": "date", "width": 120 },
    { "name": "Tags", "type": "multi_select", "options": ["design", "architecture", "work", "classic"], "width": 140 }
  ]
}
"#,
)];

const NOTES: &[(&str, u64, &str)] = &[
    (
        "Weekend plans.md",
        12,
        "# Weekend plans\n\nSaturday is for the market and a long walk along the coast. Sunday stays empty on purpose.\n\n- Farmers market before ten\n- Bike to the lighthouse\n- Call Grandma\n- [ ] Pick up the film scans\n- [x] Book the table for Sunday lunch\n\nIf it rains, the museum has the new photography wing open.",
    ),
    (
        "Ideas.md",
        95,
        "# Ideas\n\nSmall things worth building one day.\n\n- A notes app that stays out of the way\n- A timer that only shows the minutes left\n- Postcards generated from a year of photos\n\n> The best interface is the one you stop noticing.\n\nNone of these need an account.",
    ),
    (
        "Packing list.md",
        60 * 26,
        "# Packing list\n\n- [x] Passport\n- [x] Chargers and the travel adapter\n- [ ] Rain jacket\n- [ ] Book for the train\n- [ ] Swimsuit\n- [ ] Headphones\n\nLeave the laptop at home this time.",
    ),
    (
        "Reading notes.md",
        60 * 70,
        "Loose quotes that did not fit anywhere else.\n\n> Simplicity is not the goal. It is the by-product of a good idea and modest expectations.\n\nRe-read the chapter on attention next week.",
    ),
    (
        "Projects/Roadmap.md",
        40,
        "# Roadmap\n\n## Now\n\n- Folder cards with real previews\n- Autosave that never loses a keystroke\n\n## Next\n\n- Search across every note\n- Sync through any folder you already use\n\n## Later\n\n- Shared notebooks",
    ),
    (
        "Projects/Meeting notes.md",
        60 * 5,
        "# Meeting notes\n\nAttendees: Ana, Joris, Mei\n\n## Decisions\n\n- Ship the beta on the 14th\n- Keep the free tier unlimited\n\n## Follow-ups\n\n- [ ] Ana drafts the release post\n- [ ] Joris checks the crash reports\n- [x] Mei sends the invite list",
    ),
    (
        "Projects/Budget.md",
        60 * 30,
        "# Budget\n\nHosting stays under 40 a month until the beta ends.\n\n- Domain: 12 a year\n- Storage: 5 a month\n- Email: 8 a month\n\nRevisit once there are a hundred active users.",
    ),
    (
        "Projects/Launch/Checklist.md",
        25,
        "# Launch checklist\n\n- [x] Final build signed\n- [x] Release notes written\n- [ ] Screenshots for the site\n- [ ] Tell the mailing list\n- [ ] Post in the forum\n\nTag the release only once the last build passes its checks.",
    ),
    (
        "Projects/Launch/Release post.md",
        60 * 3,
        "# Release post\n\nToday the first public version is out. It keeps your notes as **plain Markdown files** in a folder you choose, so they outlive the app.\n\n## What is in it\n\n- Folders and notes, side by side\n- Instant search\n- A *quiet* editor\n\n---\n\nInstall it with Homebrew:\n\n```\nbrew install --cask formalnotes\n```\n\n> Every note is a `.md` file you can open anywhere.\n\nThe full list of changes is in [the changelog](CHANGELOG.md).",
    ),
    (
        "Projects/Launch/Press kit.md",
        60 * 50,
        "# Press kit\n\nOne paragraph about the app, two screenshots, the icon at 1024 pixels and a contact address.\n\nKeep it under a megabyte.",
    ),
    (
        "Journal/Monday.md",
        60 * 8,
        "# Monday\n\nSlow start. Coffee on the balcony, then two hours of focused work before lunch.\n\nThe afternoon went to email, which is fine once a week.",
    ),
    (
        "Journal/Tuesday.md",
        60 * 32,
        "# Tuesday\n\nFinished the layout for the home grid. The cards finally feel like paper.\n\nRan in the evening, 6 km.",
    ),
    (
        "Journal/Wednesday.md",
        60 * 56,
        "# Wednesday\n\nRain all day. Read in the afternoon and cooked the lentil soup again.",
    ),
    (
        "Reading/Books to read.md",
        60 * 20,
        "# Books to read\n\n- The Design of Everyday Things\n- A Pattern Language\n- The Shape of Design\n- Shop Class as Soulcraft\n- How Buildings Learn",
    ),
    (
        "Reading/Articles.md",
        60 * 90,
        "# Articles\n\n- On writing tools that respect your files\n- Why plain text lasts\n- The cost of a notification\n\nSave the long ones for the train.",
    ),
    (
        "Reading/The Design of Everyday Things.md",
        60 * 30,
        "---\nStatus: Finished\nAuthor: Don Norman\nRating: 5\nFinished: 2026-09-12\nTags: [design, classic]\n---\n# The Design of Everyday Things\n\nDoors that need a sign have already failed.",
    ),
    (
        "Reading/A Pattern Language.md",
        60 * 6,
        "---\nStatus: Reading\nAuthor: Christopher Alexander\nTags: [architecture, design]\n---\n# A Pattern Language\n\nPattern 159: light on two sides of every room.",
    ),
    (
        "Reading/How Buildings Learn.md",
        60 * 75,
        "---\nStatus: To read\nAuthor: Stewart Brand\nTags: [architecture]\n---\n# How Buildings Learn\n\nRecommended twice this month.",
    ),
    (
        "Reading/Shop Class as Soulcraft.md",
        60 * 140,
        "---\nStatus: Finished\nAuthor: Matthew B. Crawford\nRating: 4\nFinished: 2026-07-30\nTags: [work]\n---\n# Shop Class as Soulcraft\n\nOn the satisfaction of work you can see.",
    ),
    (
        "Recipes/Lentil soup.md",
        60 * 48,
        "# Lentil soup\n\n## Ingredients\n\n- 250 g red lentils\n- 1 onion, 2 carrots\n- 1 tin of tomatoes\n- Cumin, smoked paprika\n\n## Method\n\nSoften the onion and carrots, add the spices, then the lentils and a litre of stock. Simmer for 25 minutes.",
    ),
    (
        "Recipes/Pancakes.md",
        60 * 120,
        "# Pancakes\n\n- 200 g flour\n- 2 eggs\n- 300 ml milk\n- A pinch of salt\n\nRest the batter for half an hour. Thin is better.",
    ),
    (
        "Recipes/Focaccia.md",
        60 * 200,
        "# Focaccia\n\nA wet dough, an overnight rise in the fridge, plenty of olive oil.\n\nBake at 230 degrees for 22 minutes.",
    ),
    (
        "Groceries.md",
        3,
        "# Groceries\n\n- [ ] Oat milk\n- [ ] Lemons\n- [ ] The good bread\n- [x] Coffee beans\n- [ ] Basil",
    ),
    ("Untitled.md", 60 * 400, "A blank page, kept for later."),
];

const PREFIX: &str = "formalnotes-demo-";

/// Writes the demo library to a fresh folder of its own and returns it. Each process gets its
/// own (the appearance audit runs backends side by side); folders older than an hour, left by
/// earlier runs, are removed first.
pub fn seed() -> std::io::Result<PathBuf> {
    let temp = std::env::temp_dir();
    sweep(&temp);
    let root = temp.join(format!("{PREFIX}{}", std::process::id()));
    seed_into(&root)?;
    Ok(root)
}

fn sweep(temp: &Path) {
    let Ok(entries) = std::fs::read_dir(temp) else { return };
    let cutoff = SystemTime::now() - Duration::from_secs(60 * 60);
    for entry in entries.flatten() {
        let ours = entry
            .file_name()
            .to_str()
            .and_then(|n| n.strip_prefix(PREFIX))
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
        let old = entry.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff);
        if ours && old && entry.path().starts_with(temp) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Writes the demo library into `root`, which must be empty or missing.
pub fn seed_into(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    for folder in FOLDERS {
        std::fs::create_dir_all(root.join(folder))?;
    }
    for (folder, json) in VIEWS {
        std::fs::write(root.join(folder).join(VIEW_FILE), json)?;
    }
    let now = SystemTime::now();
    for (path, minutes, text) in NOTES {
        let file = root.join(path);
        std::fs::write(&file, text)?;
        disk::set_modified(&file, now - Duration::from_secs(minutes * 60))?;
    }
    Ok(())
}
