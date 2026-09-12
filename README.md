# Enclave

The Enclave desktop app. One binary, `enclave`, hosting the Enclave UIs and the
assistant bridge they share. **Grido**, a fast native spreadsheet for Linux that
reads and writes .xlsx, is the UI it opens today. 100% Rust — no web runtime, no
JavaScript.

## Stack

- **[IronCalc](https://github.com/ironcalc/IronCalc)** — spreadsheet engine:
  xlsx read/write, the standard formula language, dependency-graph recalculation,
  styles, conditional formatting, undo/redo history.
- **[egui](https://github.com/emilk/egui)** (via eframe) — GPU-rendered
  immediate-mode GUI. The grid paints only visible cells, so large sheets
  scroll smoothly.
- **rfd** — native file dialogs. **arboard** — system clipboard.

## Build & run

```bash
cargo build --release
target/release/enclave demo.xlsx
```

Generate a demo workbook (styles, currency/percent formats, conditional
formatting, a formula showcase sheet):

```bash
cargo run -p grido --example make_demo demo.xlsx
```

Install for your desktop (binary, icon, `.desktop` entry, keymap):

```bash
./packaging/install.sh
```

## Architecture

One concern per module, so the pieces stay independently workable:

| File | Responsibility |
| --- | --- |
| [src/main.rs](src/main.rs) | The `enclave` binary: `mcp`, `register`, or open a UI |
| [src/mcp/](src/mcp/) | MCP transport: stdio shim, socket server, UI-thread bridge |
| [crates/grido/src/engine.rs](crates/grido/src/engine.rs) | All IronCalc access, exposed as plain types — the UI never imports ironcalc |
| [crates/grido/src/state.rs](crates/grido/src/state.rs) | `GridoApp` state |
| [crates/grido/src/app.rs](crates/grido/src/app.rs) | Update loop, input handling, command dispatch |
| [crates/grido/src/commands.rs](crates/grido/src/commands.rs) | The command registry every binding and button targets |
| [crates/grido/src/keymap.rs](crates/grido/src/keymap.rs) | `keymap.toml` → key chords |
| [crates/grido/src/ui/](crates/grido/src/ui/) | `grid`, `ribbon`, `editor`, `dialogs`, `charts` |
| [crates/grido/src/mcp/](crates/grido/src/mcp/) | Tool definitions, the skill, client registration |
| [crates/enclave-ui/](crates/enclave-ui/) | Shared theme, fonts, keymap loader, ribbon kit, icons |
| [crates/enclave-client/](crates/enclave-client/) | Typed client for the enclaved daemon's local socket |

## Keyboard shortcuts

The familiar spreadsheet defaults, fully customizable. Bindings live in
[crates/grido/keymap.toml](crates/grido/keymap.toml) and map chords to command
ids:

```toml
[bindings]
"Ctrl+D" = "fill_down"
"Ctrl+Shift+L" = "toggle_filter"
"F2" = "edit_cell"
```

Lookup order (first hit wins): `$GRIDO_KEYMAP`, `./keymap.toml`,
`~/.config/grido/keymap.toml`, then the copy compiled into the binary. Press
**F1** in the app to see every binding currently loaded. The full command list
is `Command::from_id` in
[crates/grido/src/commands.rs](crates/grido/src/commands.rs).

## What works

**Navigation** — arrows, Tab/Enter, Ctrl+Arrow jump-to-edge, Shift to extend,
Ctrl+Shift+Arrow to extend to the edge, Page Up/Down, Ctrl+Home/End, Go To
(Ctrl+G) and a working Name Box.

**Editing** — two edit modes (typing then arrow commits and moves; F2 then
arrow moves the caret), fill down/right with reference adjustment,
formula-aware copy/paste that rewrites relative references, paste-values-only,
undo/redo, Find & Replace across one sheet or the workbook.

**Formatting** — bold/italic/underline/strike, font size and color, fill color,
borders, horizontal and vertical alignment, wrap, and the number formats
(currency, percent, comma, date, custom codes, increase/decrease decimals).
All of it renders from — and saves back into — the .xlsx.

**Structure** — insert/delete rows and columns with reference fixing, column
and row resize, autofit, freeze panes, hide rows, sheet add/rename/duplicate/
delete/reorder/tab-color.

**Data** — sort (single and multi-key), AutoFilter, named ranges, conditional
formatting (cell rules and color scales, including rules loaded from real
workbooks), and Sum/Average/Count of the selection in the status bar.

**Charts** — bar, line and pie, drawn natively in floating windows.

**Omarchy theming** — the app wears the active Omarchy theme (accent and all),
follows theme switches live without a restart, and handles light themes.
Elsewhere it falls back to a neutral dark scheme.

**Assistant (MCP)** — the app is an MCP server, so Claude (or any MCP client)
can create and maintain your spreadsheets while you watch. See below.

See [PLAN.md](PLAN.md) for the roadmap and what is still missing.

## Working with an assistant

**Running the app is the only requirement.** Start it any normal way — desktop
launcher, file manager, anything — and it serves assistants and registers
itself with the MCP clients it finds. No flag, no daemon, no install step, no
JSON to edit. (Registering rewrites nothing once the entry is correct, and
keeps a `.bak` the first time.)

On first run it also installs a skill at `~/.claude/skills/grido/SKILL.md`
that teaches the assistant how to use those tools — look before writing, work
in records rather than cell coordinates, write formulas rather than constants,
and leave undo, backups and saving to Grido. The same guidance is sent as MCP
`initialize` instructions for clients that have no skills.

The one thing outside the app's control: MCP clients read their config at
startup, so a client that was already open needs restarting once to notice.
**File → Assistant** shows the connection status for each client.

Then just talk to it:

> *"Make me a job application tracker."*
> *"I applied to Acme today for the data analyst role, found it on LinkedIn."*
> *"Mark the Globex application as an offer, I need to decide by Friday."*
> *"How many are still waiting on a reply?"*

Edits land in the window you are looking at and are tinted for a few seconds so
you can see what changed. The assistant works in terms of *records* — it
matches "the Acme row" against your header row rather than guessing at
coordinates. Safety rails: one tool call is one Ctrl+Z, deleting rows asks you
first, edits are refused while you are typing in a cell, the file is backed up
before the assistant's first write of a session, and work is autosaved a few
seconds after it goes quiet.

`enclave mcp` is the stdio shim clients launch, and `enclave register` wires up
config from a terminal — both are conveniences, never requirements.

## Known limitations

- **Charts are view-only** — they are not written into the .xlsx (chart XML
  round-tripping is on the roadmap).
- **Merging cells** is not possible: IronCalc exposes merged regions for
  reading but has no write API yet. Existing merges from a file are rendered.
- **Sorting moves formulas verbatim** — references inside sorted cells are not
  rewritten.
- **No pivot tables, print/PDF, images, or macros** (see PLAN.md; macro
  language compatibility is an explicit non-goal).
- IronCalc implements a large subset of the standard function set, not all of
  it. Missing functions are best fixed upstream rather than worked around here.

## Tests

```bash
cargo test --workspace
```

Covers A1 parsing, column naming, jump-to-edge semantics, the MCP tool surface
and skill, and engine-level integration tests for formulas, styles, structure
edits, sorting, find and replace, the clipboard, conditional formatting and
xlsx round-tripping.

## License

Copyright (c) 2026 Eduard Moldovan AB. All rights reserved.

Licensed under the [PolyForm Free Trial License 1.0.0](LICENSE): free to
evaluate for up to 32 consecutive days. Subscriptions and commercial terms at
<https://enclave.works>.
