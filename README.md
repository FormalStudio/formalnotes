# Formal Notes

A desktop notes app for a folder of Markdown files. Every note is a plain `.md` file in your
library folder (`~/Documents/Formal Notes` unless you pick another), folders are folders, and
anything you change outside the app shows up in it. Built on
[NativeDesktop](https://github.com/FormalStudio/NativeDesktop): real AppKit controls on macOS,
GPUI on Linux and Windows.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/shots/home-dark.png">
  <img alt="Home" src="docs/shots/home.png">
</picture>

Home and each folder show their notes as pages and their subfolders as stacked papers. A note
opens out of its card into a centred editor that shows the Markdown as you type it: headings,
emphasis, code, quotes, lists and tasks take their look with the syntax dimmed in place, and
Return continues a list. The title is the note's first `# ` heading, or its file name when it has
none. Edits save half a second after you stop typing (at most two seconds behind), through an
atomic write. Deleted notes and folders go to a `.trash` folder inside the
library until you put them back or empty it.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/shots/editor-dark.png">
  <img alt="A note open in the editor" src="docs/shots/editor.png">
</picture>

## Run

```sh
just dev                          # your library
FORMALNOTES_DEMO=1 just dev       # a demo library of 20 notes in a temp folder
```

`nd check` runs fmt, clippy, the framework lints and the appearance audit; `nd test` runs the
tier 1 tests (`tests/store.rs`, `tests/navigation.rs`) and the audit.

## Keyboard

| | |
|---|---|
| `Cmd+N` / `Shift+Cmd+N` | new note / new folder in the current folder |
| `Cmd+K` | command palette |
| `Cmd+J`, `Cmd+F` | search notes |
| `Cmd+1` | Home |
| `Cmd+[` | back |
| `Cmd+Backspace` | move the open note to the trash (outside the text, where it deletes to the line start) |
| `Cmd+B`, `Cmd+I` | bold, italic in the editor |
| `Tab`, `Shift+Tab` | nest or lift a list item in the editor |
| `Cmd+\` | toggle the sidebar |
| `Cmd+,` | settings: library folder, editor width, theme |

`Ctrl` in place of `Cmd` on Linux and Windows.
