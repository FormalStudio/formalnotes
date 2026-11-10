// The desktop's demo library (src/demo.rs), plus properties and a table view on Projects/Launch.

import { FOLDER_VIEW, join } from './model';
import type { Library } from './store';

export const DEMO_FOLDERS = ['Projects', 'Projects/Launch', 'Journal', 'Reading', 'Recipes'];

/** Notes as path, minutes since the last edit, text. */
export const DEMO_NOTES: [string, number, string][] = [
  ["Weekend plans.md", 12, "# Weekend plans\n\nSaturday is for the market and a long walk along the coast. Sunday stays empty on purpose.\n\n- Farmers market before ten\n- Bike to the lighthouse\n- Call Grandma\n- [ ] Pick up the film scans\n- [x] Book the table for Sunday lunch\n\nIf it rains, the museum has the new photography wing open."],
  ["Ideas.md", 95, "# Ideas\n\nSmall things worth building one day.\n\n- A notes app that stays out of the way\n- A timer that only shows the minutes left\n- Postcards generated from a year of photos\n\n> The best interface is the one you stop noticing.\n\nNone of these need an account."],
  ["Packing list.md", 1560, "# Packing list\n\n- [x] Passport\n- [x] Chargers and the travel adapter\n- [ ] Rain jacket\n- [ ] Book for the train\n- [ ] Swimsuit\n- [ ] Headphones\n\nLeave the laptop at home this time."],
  ["Reading notes.md", 4200, "Loose quotes that did not fit anywhere else.\n\n> Simplicity is not the goal. It is the by-product of a good idea and modest expectations.\n\nRe-read the chapter on attention next week."],
  ["Projects/Roadmap.md", 40, "# Roadmap\n\n## Now\n\n- Folder cards with real previews\n- Autosave that never loses a keystroke\n\n## Next\n\n- Search across every note\n- Sync through any folder you already use\n\n## Later\n\n- Shared notebooks"],
  ["Projects/Meeting notes.md", 300, "# Meeting notes\n\nAttendees: Ana, Joris, Mei\n\n## Decisions\n\n- Ship the beta on the 14th\n- Keep the free tier unlimited\n\n## Follow-ups\n\n- [ ] Ana drafts the release post\n- [ ] Joris checks the crash reports\n- [x] Mei sends the invite list"],
  ["Projects/Budget.md", 1800, "# Budget\n\nHosting stays under 40 a month until the beta ends.\n\n- Domain: 12 a year\n- Storage: 5 a month\n- Email: 8 a month\n\nRevisit once there are a hundred active users."],
  ["Projects/Launch/Checklist.md", 25, "---\nstatus: Doing\ndue: 2026-11-14\ntags: [launch]\n---\n# Launch checklist\n\n- [x] Final build signed\n- [x] Release notes written\n- [ ] Screenshots for the site\n- [ ] Tell the mailing list\n- [ ] Post in the forum\n\nTag the release only once the last build passes its checks."],
  ["Projects/Launch/Release post.md", 180, "---\nstatus: Todo\ndue: 2026-11-14\ntags: [launch, writing]\n---\n# Release post\n\nToday the first public version is out. It keeps your notes as **plain Markdown files** in a folder you choose, so they outlive the app.\n\n## What is in it\n\n- Folders and notes, side by side\n- Instant search\n- A *quiet* editor\n\n---\n\nInstall it with Homebrew:\n\n```\nbrew install --cask formalnotes\n```\n\n> Every note is a `.md` file you can open anywhere.\n\nThe full list of changes is in [the changelog](CHANGELOG.md)."],
  ["Projects/Launch/Press kit.md", 3000, "---\nstatus: Done\ndue: 2026-11-07\ntags: [launch]\n---\n# Press kit\n\nOne paragraph about the app, two screenshots, the icon at 1024 pixels and a contact address.\n\nKeep it under a megabyte."],
  ["Journal/Monday.md", 480, "# Monday\n\nSlow start. Coffee on the balcony, then two hours of focused work before lunch.\n\nThe afternoon went to email, which is fine once a week."],
  ["Journal/Tuesday.md", 1920, "# Tuesday\n\nFinished the layout for the home grid. The cards finally feel like paper.\n\nRan in the evening, 6 km."],
  ["Journal/Wednesday.md", 3360, "# Wednesday\n\nRain all day. Read in the afternoon and cooked the lentil soup again."],
  ["Reading/Books to read.md", 1200, "# Books to read\n\n- The Design of Everyday Things\n- A Pattern Language\n- The Shape of Design\n- Shop Class as Soulcraft\n- How Buildings Learn"],
  ["Reading/Articles.md", 5400, "# Articles\n\n- On writing tools that respect your files\n- Why plain text lasts\n- The cost of a notification\n\nSave the long ones for the train."],
  ["Recipes/Lentil soup.md", 2880, "# Lentil soup\n\n## Ingredients\n\n- 250 g red lentils\n- 1 onion, 2 carrots\n- 1 tin of tomatoes\n- Cumin, smoked paprika\n\n## Method\n\nSoften the onion and carrots, add the spices, then the lentils and a litre of stock. Simmer for 25 minutes."],
  ["Recipes/Pancakes.md", 7200, "# Pancakes\n\n- 200 g flour\n- 2 eggs\n- 300 ml milk\n- A pinch of salt\n\nRest the batter for half an hour. Thin is better."],
  ["Recipes/Focaccia.md", 12000, "# Focaccia\n\nA wet dough, an overnight rise in the fridge, plenty of olive oil.\n\nBake at 230 degrees for 22 minutes."],
  ["Groceries.md", 3, "# Groceries\n\n- [ ] Oat milk\n- [ ] Lemons\n- [ ] The good bread\n- [x] Coffee beans\n- [ ] Basil"],
  ["Untitled.md", 24000, "A blank page, kept for later."],
];

export const DEMO_VIEWS: [string, string][] = [
  ['Projects/Launch', "{\n  \"v\": 1,\n  \"view\": \"table\",\n  \"columns\": [\n    {\n      \"name\": \"status\",\n      \"type\": \"select\",\n      \"options\": [\n        \"Todo\",\n        \"Doing\",\n        \"Done\"\n      ]\n    },\n    {\n      \"name\": \"due\",\n      \"type\": \"date\"\n    },\n    {\n      \"name\": \"tags\",\n      \"type\": \"multi_select\",\n      \"options\": [\n        \"launch\",\n        \"writing\"\n      ]\n    }\n  ],\n  \"sort\": {\n    \"column\": \"modified\",\n    \"descending\": true\n  }\n}\n"],
];

/** Fills an empty library with the demo notes. A library that holds anything is left alone. */
export function seedDemo(lib: Library, newId: () => string, now = Date.now()): void {
  if (lib.all().length > 0) return;
  lib.sql.tx(() => {
    for (const path of DEMO_FOLDERS) lib.insert({ id: newId(), path, kind: 'folder', text: null, created: now, modified: now });
    for (const [path, minutes, text] of DEMO_NOTES) {
      const at = now - minutes * 60000;
      lib.insert({ id: newId(), path, kind: 'note', text, created: at, modified: at });
    }
    for (const [folder, text] of DEMO_VIEWS) {
      lib.insert({ id: newId(), path: join(folder, FOLDER_VIEW), kind: 'file', text, created: now, modified: now });
    }
  });
  lib.changed(false);
}
