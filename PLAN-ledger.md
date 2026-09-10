# Plan: Enclave Ledger

The spreadsheet: grido (repo: sheetz), joining the suite as Enclave
Ledger. Grido's own roadmap stays in its repo; this is the suite
integration plan.

## Enclave suite (Ledger)

Grido joins the Enclave suite as **Enclave Ledger** (product name;
repo stays grido/sheetz). From the suite plan (../PLAN.md):

- [ ] **MCP surface** — open/read/edit/save workbooks over the
      enclaved daemon socket, so the user's agent can work in
      spreadsheets ("update the forecast, send it to @ale").
- [ ] Launchable from the Enclave tray + an "Enclave Ledger" app
      search stub, alongside its normal standalone life.
- [ ] Naming pass: Ledger branding when launched as part of the
      suite.

Non-goals stay non-goals: no co-editing, no cloud sync — the suite's
ownership model (one owner, others fetch copies) matches this.

## High-level approach (Enclave suite)

Scope: the suite integration only. Everything above is unchanged — the
standalone app keeps its own socket, its own registration and its own
name.

### Architecture — the workbook never leaves this process

Ledger is the one suite product whose logic is already Rust and already
local, so the split is lopsided on purpose. IronCalc, xlsx, formulas,
styles, undo, autosave and backups stay behind `engine.rs`. **The Go
daemon parses no spreadsheets and holds no cells.**

The daemon module (`enclaved/internal/ledger`) owns four things: the
`enclave_sheet_*` tool definitions inside the one `enclave-mcp`;
routing each call to a Ledger window; launching Ledger when none runs;
and the native confirm dialogs for acting tools. Its only state is a
**session registry** — window ↦ open file, dirty flag — posted by the
app on open/save/close, so "which workbooks are open" is answerable
without waking anything. It is disposable; the app is the truth.

Transport inverts today's arrangement: the app **dials the daemon
socket** and holds the connection, announcing itself as a Ledger
window. Calls arrive down that connection and are pushed into the
`mcp::bridge` queue `update()` already drains onto the UI thread, so
every tool body in `mcp/tools.rs` is reused verbatim — only the front
door changes. In suite mode the app's own socket and MCP-client
self-registration stay off: one MCP for the suite, registered once at
install.

`src/suite/` is the new thin typed layer, a sibling of `engine.rs`
under the same discipline: **the UI never imports the socket, the
protocol or serde_json**, exactly as it never imports ironcalc. It
calls `app.peers()` / `app.send_to(&Address)` and gets back `Vec<Peer>`,
`SendOutcome`, `Confirm`. Socket work runs on its own thread and posts
results into the same queue as tool calls — no frame blocks on the
network.

### MCP surface

Record-first, capped and compact — the v0.6 lessons carried over under
suite names. Read tools run freely; acting tools go through the
daemon's confirm dialog.

Read:

- `enclave_sheet_overview` — every open workbook: path, sheets, used
  ranges, header rows, unsaved flag. The call that starts a task.
- `enclave_sheet_read` — a window of values, or formulas, for a range;
  defaults to the used range and hard-caps the cell count.
- `enclave_sheet_table` — header row as a schema plus the records
  matching a criteria object. How to say "the Acme row" reliably.
- `enclave_sheet_stats` — sum/avg/count/min/max over a range, so
  questions are answered without dragging cells into context.
- `enclave_sheet_find` — value search across sheets, hits as addresses.

Acting:

- `enclave_sheet_open` — open an .xlsx in a Ledger window: a path, or a
  copy handed over by Post or Depot.
- `enclave_sheet_set` — cells, ranges and formulas.
- `enclave_sheet_record` — append or update rows by header match.
- `enclave_sheet_format` — styles and number formats on a range.
- `enclave_sheet_structure` — rows, columns and sheets; add or remove.
- `enclave_sheet_sort` — sort or filter a block.
- `enclave_sheet_save` — write the file, or save a copy elsewhere.
- `enclave_sheet_send` — hand the workbook to `@person` as a drop, or
  to a Post draft as an attachment.
- `enclave_sheet_undo` — take back the last agent action.

Two gates, not one. Edits land **live in the visible window**, tinted,
one tool call = one Ctrl+Z, and every write returns a **diff** (cells
changed, a sample of before → after) so the agent can check itself
without re-reading. The daemon dialog appears only where a change
escapes that net: overwriting a non-empty block, deleting rows or
sheets, saving over the source file, and sending. Those dialogs are
previews rather than prompts — destructive ones show the block that
will change, save shows the path and what the session touched, send
shows file, size and the recipient's computer.

### Launch and branding

Ledger opens from the tray menu, from an "Enclave Ledger" app-search
stub, from the .xlsx file association, or from the agent via
`enclave_sheet_open`. All four hit one entry point with a suite flag:
title and mark become Enclave Ledger, standalone socket and
registration stay off. Launch is idempotent — the session registry
answers "already open?", so an existing window is raised instead of a
second copy.

### Interlinks

- **Post → Ledger.** "Open the attachment and fix it": Post saves the
  xlsx locally and calls `enclave_sheet_open`. `enclave_sheet_send`
  with a Post target puts the edited file back on the reply draft; the
  human confirms the send in Post.
- **Depot → Ledger.** A colleague's share is read-only, so the agent
  fetches `@ale/Budget 2026.xlsx` as a local copy and opens that.
  Editing across the network never happens; returning it is a send.
- **Ledger → enclaved.** "Send the forecast to @ale" saves, then drops
  the file on @ale's online computer — queued if it is asleep. Same
  @-addressing everywhere, `@person/computer` when it matters.
- **Bursar → Ledger.** Bursar exports a period's figures as a workbook
  and opens it here for the human to check before the report goes out.
  Bursar owns the numbers; Ledger is where they are read.
- **Ledger → Scribe / Podium.** A range travels into a document or a
  slide as a table: a snapshot of values and formats, never a live link.
- **Ledger → Commons.** A range pastes into a channel as a small table;
  the file itself travels as a drop.
- **Mobile.** Receives workbooks as drops and views them. The phone
  does not edit.

### Non-goals

No co-editing, no shared cursors, no cloud copy, no server-side
recalculation, no headless Ledger (the agent edits a window a human can
watch), no live cross-machine cell links, no second MCP registration,
and no transport code in Ledger — sending, fetching and identity are
the daemon's job. KISS, as above.

## Grido roadmap (the spreadsheet itself)

Guiding principle: build features in the order people actually use a
spreadsheet. Roughly 90% of the time goes on formatting, navigation, basic
formulas, sort/filter and charts — that's v0.2–v0.4. Pivot tables and
automation are the long tail.

"IC" notes what the IronCalc engine already provides vs. what we build.

### Architecture (done)

- [x] **Split the monolith** — `engine.rs` (all IronCalc access, plain-typed
      API), `state.rs` (app state), `app.rs` (update loop + command dispatch),
      `ui/{grid,ribbon,editor,dialogs,charts}.rs`. UI never imports ironcalc.
- [x] **Command registry** — ~100 named commands, every one bindable.
- [x] **Keymap** — the full standard set in `keymap.toml`, plus an in-app
      shortcut viewer (F1).

### v0.2 — Looks like a spreadsheet (formatting & structure)

- [x] **Render cell styles** — bold/italic/underline/strike, font size, text
      and fill color, borders, horizontal + vertical alignment.
- [x] **Edit basic styles** — ribbon Home group with live toggle states.
- [x] **Number format UI** — General, currency, percent, comma, date,
      increase/decrease decimals.
- [x] **Insert/delete rows & columns** — with reference adjustment (IC).
- [x] **Autofit column width** — double-click or ribbon.
- [x] **Right-click context menu** — cut/copy/paste, insert/delete, clear,
      format.
- [x] **Sheet management** — new, rename (double-click tab), delete,
      duplicate, reorder.
- [x] **Clear** — contents / formats / all.
- [x] **Column/row resize by dragging** header edges, with resize cursors and
      double-click-to-autofit; header click/drag selects whole rows/columns.
- [x] **Merged cells** — existing merges render as one cell and select as a
      unit. *Creating* merges is blocked upstream: IronCalc exposes merged
      regions for reading only.
- [x] **Freeze panes** — frozen rows/columns painted as a pinned overlay with
      a stronger boundary line, and reported in the status bar.

### v0.3 — Edits like a spreadsheet should (clipboard, fill, find)

- [x] **Formula-aware clipboard** — internal copy/paste keeps formulas, styles
      and adjusts relative references; cut marks the source; external apps
      still get TSV (IC).
- [x] **Paste values only** — Ctrl+Shift+V.
- [x] **Enter-mode vs Edit-mode** — typing then arrow commits and moves; F2
      then arrow moves the caret.
- [x] **Find & Replace** — Ctrl+F/H, match case, all-sheets, find next/prev,
      replace all.
- [x] **Undo/redo** — covers edits, styles, structure (IC history).
- [x] **Fill handle** — the drag-corner on the selection, with a preview
      outline; fills down/right through the engine's series logic.
- [x] **F4 reference cycling** — A1 → $A$1 → A$1 → $A1 while editing.
- [x] **Click-to-reference while editing a formula**, with each distinct
      reference in the formula given its own highlight color.

### v0.4 — Works with data (sort, filter, formulas UX)

- [x] **Sort** — ascending/descending on the selection or surrounding block,
      with header detection; plus a sort dialog.
- [x] **Named ranges** — name manager (add/list/delete).
- [x] **Go To** — Ctrl+G / F5 and a working Name Box.
- [x] **Selection stats** — Sum / Average / Count in the status bar.
- [x] **AutoFilter** — header dropdown buttons opening a searchable value
      checklist; non-matching rows are hidden.
- [x] **Formula autocomplete** — function-name popup while typing, Up/Down to
      choose, Tab/Enter to accept.
- [x] **Multi-key sort** — up to three levels, ascending/descending each,
      blanks last.
- [ ] **Data validation** — dropdown lists first
- [ ] **Multi-range selection** — Ctrl+click disjoint ranges

### v0.5 — Shows data (visual layer)

- [x] **Conditional formatting** — cell-value rules and color scales via a
      dialog; rendering comes through the engine's extended styles, so rules
      loaded from a real .xlsx paint correctly too.
- [x] **Charts (native)** — bar / line / pie drawn with the egui painter in
      floating windows over the sheet.
- [x] **Zoom** — Ctrl+scroll, ribbon buttons, status-bar readout.
- [x] **Toggle gridlines**.
- [x] **Data bars** — painted behind cell text from the engine's evaluated
      conditional formatting.
- [ ] **Charts in xlsx** — reading/writing chart XML so charts survive a round
      trip; native charts are view-only today
- [ ] **Comments/notes**, **hyperlinks**

### v0.6 — LLM-operable, live (MCP)

The goal: a non-technical person talks to Claude/GPT ("log that I applied to
Acme today", "which applications haven't replied in two weeks?") and watches
their real spreadsheet update in front of them. The assistant maintains the
file over months; the human stays in control and can always undo.

That rules out a headless server: the model edits **the workbook already open
in the GUI**, and every change is visible immediately.

#### Transport — the live bridge

**Requirement: launching Sheetz normally — from the desktop launcher, a file
manager, anywhere — is the only step.** No terminal, no flag, no separate
daemon to remember. A CLI entry point may exist, but nothing may *depend* on
the user typing it.

MCP clients launch a server and speak newline-delimited JSON-RPC 2.0 over
stdio, so the running GUI can't be the server directly (its stdio isn't the
client's). Hence a socket the GUI always holds, plus a shim that bridges to it.

- [x] **Socket server starts with the app, unconditionally** — spawned during
      startup, not behind a flag or subcommand, so a launcher start is fully
      equivalent to a terminal start.
- [x] **Socket path and liveness** — `$XDG_RUNTIME_DIR/sheetz.sock`, falling
      back to `/tmp/sheetz-$UID.sock` (a launcher's environment is thinner than
      a shell's, so nothing may assume shell-only variables). On startup, try
      connecting to an existing socket first: if it answers, another instance
      is primary and this one serves nothing; if it refuses, the file is stale
      — unlink and bind. Remove it on clean exit.
- [x] **Commands marshalled onto the UI thread** — the socket thread sends a
      request plus a reply channel; `update()` drains the queue, applies it to
      the `Engine` and replies. `Context::request_repaint()` (Send + Sync) wakes
      the loop, so edits land even while the app sits idle.
- [x] **`sheetz mcp` stdio shim** — the tiny binary the MCP client launches.
      Proxies stdio to the socket; if nothing is listening it starts the GUI
      itself and waits for the socket, so an assistant-first flow works too.
- [x] Protocol layer: `initialize`, `ping`, `tools/list`, `tools/call`, tool
      failures as `isError` results rather than protocol errors. Hand-rolled,
      no framework (the shape is proven in viode's `viode-mcp`).
- [x] Tests covering the protocol surface: schemas, initialize, tools/list,
      notifications, malformed input, and tool failures arriving as `isError`
      results. The socket and GUI halves are exercised by driving the real
      binary with an MCP client script.

#### Registration — the other half of "no terminal commands"

A running socket is useless if the assistant doesn't know Sheetz exists.
Registering an MCP server normally means hand-editing a client's JSON config,
which is precisely the step being ruled out — so Sheetz does it:

- [x] **Installer registers the server** — `packaging/install.sh` adds the
      `sheetz` entry (pointing at `sheetz mcp`) to the MCP client configs it
      finds, merging into existing JSON rather than overwriting it, and
      leaving a backup.
- [x] **In-app "Connect to assistant"** — a File/backstage action that does the
      same registration on demand and reports what it wrote, for people who
      installed some other way or added a client later. This is the path a
      non-technical user actually takes.
- [x] **Status readout** — the backstage shows whether the socket is listening,
      whether this instance is the primary, and which clients are registered,
      so "why can't Claude see my sheet?" is answerable without a terminal.
- [x] `.mcp.json` in the repo for development use.
- [x] **Registration happens at app startup** — running Sheetz is genuinely the
      only requirement. No installer step, no `register` command needed, no
      hand-edited JSON. Idempotent: once the entry is right nothing is
      rewritten, key order is preserved and a `.bak` is kept.
- [x] **A skill teaching the tools** — `~/.claude/skills/sheetz/SKILL.md`,
      written at startup like the registration. Tool schemas say what the
      arguments are; the skill says how to *work*: look before writing, prefer
      records over coordinates, write formulas not constants, and leave undo,
      backups, autosave and confirmations to Sheetz. The same guidance goes out
      condensed as MCP `initialize` instructions, so clients without skills
      still get it.

#### Tools the assistant needs

Raw cell addressing is not enough — asking a model to compute "row 14, column
D" is where it will make mistakes. Sheets that hold records get record-shaped
tools on top of the cell-level ones.

- [x] **Discovery** — `workbook_info` (sheets, used ranges, dirty state, path),
      `sheet_read` returning a *window* with values and optionally formulas.
      Hard cap the cell count and default to the used range; a naive
      "read the sheet" on a million rows must be impossible.
- [x] **Cell level** — `cell_set`, `range_set`, `range_format`,
      `rows_insert/delete`, `cols_insert/delete`, `sheet_add/rename/delete` —
      one-liners over `engine.rs`.
- [x] **Record level** — treat a header row as a schema: `table_schema`,
      `table_append(record)`, `table_find(criteria)`, `table_update(match,
      changes)`. This is what makes "mark the Acme application as rejected"
      reliable instead of a guess about coordinates.
- [x] **Templates** — `workbook_from_template("job-applications" | "budget" |
      …)`: headers, formats, freeze, filter, a couple of summary formulas. Gets
      a useful file existing in one turn.
- [x] **Query** — `stats`, `find`, `sort_range`, `filter` so the model can
      answer questions without dragging the whole sheet into its context.

#### Trust, safety and visibility

The point of doing this in a visible GUI is that the human stays in the loop.

- [x] **Change highlighting** — cells the assistant just wrote get a temporary
      tint that fades, so you can see what it touched at a glance.
- [x] **Activity log** — a side panel listing each tool call in plain English
      ("added row 12: Acme, Applied, 2026-08-30"), with the range it touched.
- [x] **One tool call = one undo step** — done at the app layer: the engine
      counts the undo entries a batch produced and `undo_grouped` pops them
      together. IronCalc keeps its history private with no grouping API, so
      collapsing them *inside* the engine stays blocked upstream.
- [x] **Destructive operations gated** — deleting sheets, clearing ranges or
      overwriting non-empty blocks prompt for confirmation in the UI (or are
      refused in an unattended mode). The model must not be able to quietly
      destroy a year of records.
- [x] **Edit conflicts** — refuse (with a clear error the model can act on)
      while the user has a cell editor open, rather than yanking it away.
- [x] **Save policy** — debounced autosave after assistant edits, since the
      target user will not think to press Ctrl+S; explicit `workbook_save` too,
      and never silently discard the human's unsaved work.
- [x] **Backups** — keep the previous version alongside the file before the
      first assistant write of a session. Cheap insurance for a file someone
      maintains for months.

#### Engine gaps this needs first

- [x] Make `parse_a1_range` public — tools should take `"B2:D9"`, not integers.
- [x] `read_range` returning a value grid in one call.
- [x] Sheet lookup by name, since tools take names and the engine takes indices.
- [x] Transaction begin/end around a batch of edits — batches the recalc (one
      evaluation per tool call) and counts undo steps for the grouping above.

### v0.7+ — The long tail

- [ ] **Pivot tables** — read existing first, then create/edit
- [ ] **Print / export PDF** — page setup, print areas
- [ ] **Images in sheets**
- [ ] **Workbook protection**
- [ ] **Goal seek**
- [ ] **Scripting** — an embedded Rust scripting language (Rhai), not a
      macro language clone. Explicit non-goal until the above ships.

### Engine parity track (continuous, upstream)

IronCalc implements a large subset of the ~500 standard functions. As we hit gaps
(XLOOKUP, dynamic arrays/spill, LAMBDA, …):

1. Log the missing function in an issue here,
2. Implement it **upstream in IronCalc** and pull the release — everyone wins
   and we don't fork the engine.

Same for xlsx fidelity bugs and for the missing merge-cells write API.

### App infrastructure track (continuous)

- [x] **Omarchy theming** — reads the active theme's `colors.toml` (accent,
      foreground, background; `alacritty.toml` as fallback) from
      `~/.local/state/omarchy/current/theme/` and derives every surface from
      it, light themes included. A watcher polls the theme identity and
      restyles the running app in place — and keeps a slow repaint scheduled
      so even an idle window follows a switch. Neutral dark without Omarchy.
- [x] Unit tests for A1 parsing, column names, jump-to-edge, replace; xlsx
      round-trip integration test.
- [ ] Per-frame cell format cache (the grid currently queries the engine per
      visible cell per frame)
- [ ] Packaging: `.desktop` file + icon, AUR PKGBUILD, release CI
- [ ] Chorded/multi-key bindings in the keymap

### Explicit non-goals

Macro-language compatibility, query/ETL tooling, real-time collaboration,
cloud sync, an add-in ecosystem. These are incumbent-ecosystem moats, not
spreadsheet
features; chasing them kills the project. KISS.
