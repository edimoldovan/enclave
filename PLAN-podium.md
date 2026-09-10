# Plan: Enclave Podium

Presentations: the third leg of the office trio next to Scribe and
Ledger.

## Settled (from the suite plan)

- **pptx native** (same reasoning as Scribe's docx and grido's xlsx:
  attachments open without conversion, decks sent out surprise
  nobody).
- **Rust + egui**, grido's architecture as the template.
- **MCP surface built in**: the agent can build and edit decks
  ("make a 5-slide deck from this doc") and hand them to Post.
- **No live co-editing** — same ownership model as the rest.
- Launched from the tray, app search stub ("Enclave Podium"), and
  .pptx association.

## Starter roadmap

1. pptx engine spike: what Rust offers for read/write (likely
   thinner than docx — same make-or-break unknown as Scribe's).
2. Read + render slides: text boxes, images, shapes, basic layouts.
3. Edit + save round-trip preserving what we don't render.
4. Present mode: full-screen, presenter notes, second display.
5. MCP tools: create deck from outline, edit slide text, export.

## High-level approach

One pptx writer, in Rust, in the window the user is looking at; the Go
daemon is protocol, policy and the enclave — never OOXML. The rest
follows from that.

### Architecture

- **Rust `podium/`, grido's split verbatim**: `deck.rs` (all OOXML
  access, plain types out — the UI never imports the pptx crates),
  `state.rs`, `app.rs` (update loop + dispatch), `commands.rs` (the
  registry every binding and button targets), `keymap.rs`,
  `ui/{rail,canvas,notes,ribbon,dialogs,present,icons}`, `theme.rs`.
- **`ooxml/` — one crate, shared with Scribe**: zip container, content
  types, relationship graphs, part round-tripping, theme (colour +
  font schemes), DrawingML shapes/text/fills/images. Scribe puts a
  document model on it, Podium a slide model; the models stay apart,
  the plumbing is not written twice. Crate-level guarantee: **parts we
  do not model survive a save byte-for-byte** — the promise both
  products live or die on.
- **Go `enclaved/internal/podium` stays thin**: socket ops, a deck
  registry (path, title, slide count, cached outline, last opened) so
  list/outline answer without opening a window, MCP routing, the
  native confirm dialogs, @-resolution, handoff to Post, Depot and the
  file drop. No parsing, no rendering, ever.
- **State**: parsed deck, undo stack, selection, thumbnails and
  present state → the Rust process; routing, registry and pending
  confirmations → the daemon. The .pptx on disk is the only durable
  truth; the registry is a rebuildable cache.
- **Headless is a mode, not a second engine**: a tool call with no
  window open makes the daemon launch Podium and act there, so the
  human watches the deck being built (grido's rule). `--offscreen` is
  for export and thumbnail batches only.
- **The UI's only view of the suite is a typed layer**: `enclave.rs`
  wraps the socket as `peers()`, `send(addr, path)`, `attach(path)` —
  same discipline as the engine; `ui/` never sees JSON or a socket.

### MCP surface

Read tools run free; acting tools go through the daemon's native
confirm dialog, and one apply is one undo step.

Read:

- `enclave_deck_list` — known decks: path, title, slides, last opened.
- `enclave_deck_outline` — the whole deck, compact: per slide its
  layout, title, bullets, notes, media. **Call this first**; 60 slides
  fit in a few hundred lines.
- `enclave_deck_slide` — one slide in full: each shape with a stable
  id, placeholder name, geometry, text runs, style.
- `enclave_deck_theme` — masters, layouts, colour and font scheme, so
  generated slides wear the company template.
- `enclave_deck_render` — PNG of a slide or range: the agent sees what
  it made, other products embed it.
- `enclave_deck_notes` — presenter notes across the deck.

Acting:

- `enclave_deck_draft` — slides from an outline (or a Scribe doc,
  Ledger range, Bursar figures). **Writes nothing**: returns a draft
  id, the outline it would produce, and thumbnails. Iterating is free.
- `enclave_deck_apply` — commit a draft or an edit set. The confirm
  dialog *is* the preview: before/after thumbnails, added / changed /
  removed marked.
- `enclave_deck_edit_text` — title, body or notes on one slide, or a
  named placeholder; confirms with a text diff.
- `enclave_deck_slides` — add, duplicate, reorder, delete.
- `enclave_deck_media` — an image or a Ledger chart into a named
  placeholder, never at raw coordinates.
- `enclave_deck_export` — PDF or PNGs beside the deck.
- `enclave_deck_send` — the deck or its PDF to `@ale`, to Post as an
  attachment, or into a Depot share.
- `enclave_deck_present` — start/stop, go to slide n. **No confirm**:
  visible by definition, Escape undoes it.

Good to drive: overview before detail, hard caps, never raw XML at the
model; slides by index *and* stable id, shapes by placeholder name
(Ledger's `table_*` lesson — coordinates are where models err); draft
→ preview → apply for anything multi-slide; errors as advice ("layout
Two Content has no Chart placeholder; it has Title, Left, Right");
edits refused while the human types in a shape or presents; a shipped
skill teaching the working style, condensed into `initialize`
instructions.

### UI

One window: ribbon on grido's geometry (icon band, one shared label
line), tabs **Home / Insert / Design / Slide Show / View / Help**, and
a full-screen File backstage (recents, open, save, export, assistant
status).

- **Slide rail** left: thumbnails, drag to reorder, multi-select.
- **Canvas** centre: the slide at true aspect on a neutral desk,
  generous margin, snap guides, in-place text editing, shape handles;
  paints only what is visible, like grido's grid.
- **Notes** under the canvas, collapsible — the pane people actually
  type in the night before. **Status bar**: slide n of m, zoom, theme,
  present display.
- **Present mode** — a second egui viewport, fullscreen on the picked
  display. Audience: the slide alone, B blanks, W whites, Escape ends.
  Presenter, on the laptop: current + next slide, notes at reading
  size, elapsed timer, n/m. One display → present fullscreen,
  presenter view on a hotkey. A viewport, not a second process: one
  deck model, nothing to sync.
- **Other surfaces**: draft preview; daemon confirm dialogs; pickers
  for layout (the master's layouts as thumbnails), image (native
  dialog plus a Depot tab), template, display, and @-recipient
  (autocompleting from the daemon's peers); F1 shortcut viewer
  generated off the command registry.
- **Theming**: Omarchy `colors.toml`, every surface derived, light
  themes included, live switch — grido's `theme.rs` as is. **The
  canvas is the deliberate exception**: a slide renders its own
  colours, because it must look like the projector, not the desktop.
- **Empty state**: a calm centred panel — New, Open, recents, and one
  line teaching the real door: *"or ask your agent: make a 5-slide
  deck from Q3.docx"*.

Calm typography on one type scale, generous spacing, no chrome for its
own sake. Every action is a registry command, so every action binds.

### Interlinks

- **Scribe** — "make a deck from this doc": Scribe hands over its
  outline; the shared `ooxml/` crate moves images and charts across
  without re-encoding.
- **Ledger** — a chart or range becomes a slide; re-running
  `deck_media` refreshes it. A copy, never a live link.
- **Post** — a .pptx attachment opens straight in Podium (the suite's
  benchmark sentence); "send the deck to @ale" leaves as attachment or
  PDF.
- **enclaved** — "drop it on @ale": machine to machine, no size limit,
  `@ale/laptop` to pick the machine, send queue when she is offline.
- **Depot** — open a colleague's deck read-only from their share;
  fetch a copy to make it yours. The open dialog has a Depot tab.
- **Commons** — present into a call: the audience viewport is the
  screen-share source; "@here presenting now" posts the deck.
- **Almanac** — "the deck for my 10:00" opens the meeting's
  attachment; enclaved fires the reminder; present mode offers the
  display setup that meeting used last time.
- **Bursar** — quarterly figures draft the board deck; Podium exports
  the PDF, Post sends it.
- **Mobile** — read a deck and its notes, and be the clicker: next /
  previous / black, notes on the phone, over the enclave.
- **Vault** — nothing. Decks hold no secrets; better said than
  invented.

### Non-goals

Live co-editing, cloud sync, comment-and-review workflow. Animation
and transition *authoring* (preserve what a file carries, fade at
most). VBA and macros. Add-ins. SmartArt and 3D authoring — render
what is there. Vector illustration: Podium is not a drawing app.
Auto-design magic — the template and the agent do that job. Video and
audio playback in v1 (preserved on round-trip, not played). Web/HTML
export. PDF editing; export only. A template marketplace: use the
company's .potx. Each is an incumbent moat, not a presentation
feature. KISS.

## Open

- Shared engine layer with Scribe (both are OOXML zip + XML — one
  crate for the packaging/format plumbing, two document models?).
- Templates/themes in v1 or plain slides first.
