# Plan: Enclave Scribe

The document product: open, edit, save .docx natively — attachments
from the outside world open without conversion, files sent out
surprise nobody.

## Settled (from the suite plan)

- **docx native** (read/write directly, as grido does .xlsx).
- **Rust + egui**, following grido's proven architecture: engine /
  state / app / ui split, command registry, keymap.
- **MCP surface built in**: the agent can open, read, edit, save a
  document and hand it to Post as an attachment — the benchmark
  sentence ("open the attachment for editing") runs through Scribe.
- **No live co-editing, by design**: a file has one owner; others
  read via Depot or receive a copy — then it's theirs.
- Launched from the tray, the OS app search stub ("Enclave Scribe"),
  and .docx file association.

## Starter roadmap

1. Pick/validate the docx engine crate (docx-rs or equivalent —
   the IronCalc-shaped decision; may need our own model on top).
2. Read + render: text, headings, lists, tables, images, basic
   styles.
3. Edit + save round-trip that preserves what we don't render.
4. MCP tools: open, extract text, targeted edits (find/replace,
   append, fill placeholders), save, save-as.
5. Manual editor UI: cursor editing, styles ribbon, spellcheck later.

## High-level approach

### Architecture

Two processes; one docx engine, in Rust; a boundary drawn so the
crate choice stays reversible.

- **Rust process (egui)** owns the engine, layout, rendering and
  editing. Started from the tray, the app-search stub, a .docx
  association, or by the daemon for an agent — a `--headless` mode
  runs the same binary windowless, so read tools never pop a window.
- **Go daemon module** (`enclaved/internal/scribe/`) stays thin: the
  socket protocol, routing to the right Scribe process, the open-doc
  registry, the change-set journal, recents, and the native confirm
  dialogs. **It never parses docx** — one engine, nothing to drift.
  Scribe registers with it at start, so `enclave-mcp` keeps one
  socket and one namespace.
- State split: the daemon keeps what outlives a window (open docs,
  pending change-sets, recents, Post/Depot hand-offs); Scribe keeps
  the document, the undo history and the caret.

grido's discipline, adapted to flowing text:

- `docx.rs` is the **only** file importing the docx crate. `ui/`
  imports neither it nor the daemon client: it reads `state.rs` and
  reaches the socket through `link.rs`, a thin typed layer — no JSON
  in the UI, exactly as grido's UI never imports IronCalc.
- `doc.rs` — blocks (paragraph, heading, list item, table, image,
  break) of runs, styles by name, stable `BlockId`s. Ids are the
  addressing scheme for edits, previews and the agent: prose's
  answer to grido's "records, not cell coordinates".
- `layout.rs` — model → laid-out lines, cached per block, painted
  only where visible. grido's virtualized grid, one dimension down.
- `state.rs` / `app.rs` / `commands.rs` / `keymap.rs` as in grido:
  every action a named command, every command bindable from
  `keymap.toml`, F1 lists what is bound. Cursor is `(BlockId,
  offset)`; the Enter-vs-Edit mode split has no prose analogue.
- **The fidelity bet**: edit the OOXML tree in place and preserve
  every part we don't model — open→save is a no-op on untouched
  content. That is what makes "opens without conversion" true in
  both directions.
- **Spike first (roadmap 1)**: candidates measured on a
  Word-authored contract, a Google Docs export and a LibreOffice
  save — round-trip bytes, styles, tables, images, headers/footers,
  surviving revisions. If none holds up we own the zip+XML layer and
  model only what we render. `docx.rs` is the blast radius either
  way.

### MCP surface

Read tools run freely. Acting tools never write: they produce a
**change-set** the agent can see and iterate on, and only
`enclave_doc_apply` — through the daemon's native confirm dialog —
puts anything on disk.

Read: `enclave_doc_list` (open docs + recents), `enclave_doc_open`
(path, Post attachment or a colleague's Depot file → doc id),
`enclave_doc_outline` (the overview tool and the right first call:
heading tree with block ids, word counts, where tables and images
are — a 60-page report in a few hundred tokens), `enclave_doc_read`
(one section, block range or table, capped so "read the whole
document" is not accidentally possible), `enclave_doc_find` (matches
as block ids + snippets), `enclave_doc_styles` (the names this
document actually uses, so the agent writes "Heading 2" instead of
inventing one), `enclave_doc_comments`.

Acting, all draft-first: `enclave_doc_edit` (the general editor —
operations against block ids: replace text, insert after, delete,
restyle, insert table or image; returns a **diff** and raises the
preview in Scribe), `enclave_doc_replace` (find/replace as a
change-set, expressible without ids), `enclave_doc_fill` (template
placeholders and content controls — the invoice, the offer letter),
`enclave_doc_new` (from a template; nothing on disk until saved),
`enclave_doc_apply` (confirm dialog → apply → save; one apply is one
Ctrl+Z), `enclave_doc_save_as`, `enclave_doc_export_pdf`, and
`enclave_doc_send` (to `@person` via the enclaved drop, or to Post
as an attachment; the dialog names file and destination).

Change-sets carry the document's content hash: if the human typed
meanwhile, apply fails with a sentence the model can act on and
re-draft — grido's "refuse while a cell editor is open", grown up.
They are journaled by the daemon and expire in minutes, so a preview
survives a crash but never lingers. Errors name the fix, never a
code. A Scribe skill ships with the agent layer, as Sheetz's does:
outline before reading, edit by heading, keep the document's own
styles, never rewrite a page to change a sentence.

### UI

One document window, grido's chrome, calm typography.

- **Ribbon** — Home (styles, font, paragraph, lists), Insert (table,
  image, break, link), Layout (margins, orientation), Review (find,
  comments, word count), View (zoom, outline, marks), Help. grido's
  button geometry and hover/active states verbatim.
- **Page canvas** — pages on a neutral desk, centered, one column,
  real margins; the document's own fonts where present, a sane
  fallback otherwise, substitutions shown rather than hidden.
- **Outline pane** (left, collapsible) — the heading tree, click to
  jump. It is the same map the agent's block ids describe, so human
  and agent talk about the document in the same terms.
- **Review pane** (right, appears with a change-set) — the flagship
  surface: proposed changes as before/after, insertions and
  deletions in the accent color, grouped by heading, accept all /
  accept one / reject. Never raw XML.
- **Backstage** (File tab, full screen) — new from template, open,
  recents, assistant status, save. Also the empty state: no document
  open shows templates and recents, never a blank page.
- **Status bar** — page x of y, words, selection words, save state,
  and "assistant working" while the agent holds the document.

Auxiliary surfaces: the daemon's confirm dialog (the same preview
inside it, plus the destination when the file leaves the machine);
find & replace, insert table, paragraph/style, word count and
shortcut-viewer dialogs; native file pickers (as enclaved does); an
**@-picker** with online state for sending; a Depot browser; a Post
attachment picker; and grido's activity log listing each agent call
in plain English.

Theming is grido's `theme.rs` unchanged — Omarchy `colors.toml`,
every surface derived, live switches, light themes included, neutral
dark elsewhere. The paper derives too: warm under a light theme, a
soft dark page that does not glare under a dark one. Generous
spacing, sentence case, no icon soup.

### Interlinks

Scribe is the suite's document surface; most document flows end or
start here, and every destination field takes `@person` or
`@person/computer` — the same string that works in Commons and in
the agent chat.

- **Post → Scribe** — the benchmark sentence: Post fetches the
  attachment, the daemon opens it in Scribe, the agent gets a doc id.
- **Scribe → Post** — `enclave_doc_send` attaches the saved docx or
  PDF to a reply or a new mail; Post's own confirm sends it.
- **Scribe → enclaved** — "send it to @ale" drops the file onto his
  computer; an offline target rides the send queue.
- **Depot → Scribe** — open `@carolin`'s file read-only from her
  share; editing offers "save as your own copy", which makes the
  suite's ownership model visible instead of explained.
- **Ledger → Scribe** — "put the Q3 numbers in the board memo":
  values read from the workbook, written as a table. A copy, not a
  live link.
- **Scribe → Podium** — "make a deck from this doc": the outline is
  Podium's input, and both share the OOXML packaging layer.
- **Bursar → Scribe** — invoices and the VAT/annual reports render
  through Scribe's model into PDF, then leave via Post. Bursar owns
  the numbers, Scribe owns the page.
- **Almanac → Scribe** — "write the minutes for tomorrow's board
  meeting" opens a document titled from the calendar entry; a
  meeting link inside it is a Commons call.
- **Commons → Scribe** — a document shared in a channel rides an
  enclaved drop; the message carries the reference, clicking opens
  Scribe.
- **Vault → Scribe** — a password-protected docx takes its
  passphrase from Vault, never from the agent's context. **Mobile**
  reads and forwards; the phone gets no editor.

### Non-goals

KISS, in grido's spirit — these are incumbent moats, not document
features.

- **No live co-editing, no presence, no cloud sync.** One owner per
  file; others fetch a copy and then it is theirs.
- **docx only** — no .doc, no ODT, no RTF, no Markdown import path.
- **No macro/VBA compatibility**, no add-ins, no template market.
- **No pixel-identical Word pagination.** The promise is round-trip
  safety — what we don't render, we preserve — not Word's exact
  line breaks.
- **No authoring of tracked changes in v1** (existing revisions are
  read and preserved), no mail merge, no citation manager, no
  bundled fonts.
- **No web version, no browser runtime, no JavaScript.**

## Open

- Whether a good-enough Rust docx engine exists or we build the
  document model ourselves (the make-or-break unknown — resolve
  first).
- PDF export in v1.
