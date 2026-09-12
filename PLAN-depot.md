# Plan: Enclave Depot

Shared storage without a shared copy: every member has their own
share folder on their own machine, readable by the rest of the
enclave.

## Settled (from the suite plan)

- **One share per member, on their machine.** Everyone else can
  browse, read, and fetch — **read-only** — while that machine is
  online. Only the owner writes.
- **No cloud copy, no replication**: no sync conflicts, no
  propagating deletes, nobody can touch your files. Availability
  follows the owner's machine.
- **Editable copies move by fetching** (or a drop) — then they're
  the recipient's.
- Served by the enclaved daemon over the tailnet; browsing UI in
  Rust from the tray / app search stub; offsite backup (paid) covers
  each share with Time Machine-style recovery.

## Starter roadmap

1. Share protocol on the enclave: list directory, stat, fetch file
   (range/resume for big files) — same spirit as the drop port.
2. The share folder: `~/Enclave/Share/` (name TBD), owner-managed
   in the file manager, nothing special to learn.
3. Browse UI: pick a colleague, walk their share, fetch.
4. MCP tools: list a member's share, fetch a file ("get the logo
   from @carolin's share").
5. Offline behavior: clear "X's machine is offline" everywhere.

## High-level approach

Depot is mostly daemon: a read-only file service on the tailnet with a
browser window over it. The protocol has no write verb — that is the
security model — and no byte moves without someone clicking.

### Architecture

**Go module `enclaved/internal/depot/`, two halves.** The *host* half
serves this machine's `~/Enclave/Share/` to the enclave — one listener
per enclave node, next to the drop port and in its spirit: `list`,
`stat`, `read` (byte ranges, so a big fetch resumes), `find`, `peek`.
The caller's identity is its tailnet node: no tokens, no per-share auth
to get wrong, and read-only is structural because there is no write op
to reach. Path traversal is the only real attack surface, so paths are
canonicalised against the root and escaping symlinks refused. The
*client* half resolves `@person` to a node, browses, and fetches into
`~/Enclave/Fetched/@person/…` with resume; it owns the transfer queue,
the confirm plumbing and the MCP ops. Both run windowless: a 4 GB fetch
survives closing the browser, and an owner serves from the tray.

**Listings are snapshots, not a sync.** A peer's listing is cached for
seconds so browsing feels instant, but every fetch revalidates size,
mtime and hash against the owner's machine — a stale byte is
impossible. The cache holds names, never content; previews stream and
are not kept; a fetched file is a plain copy and nothing invalidates
it, because it is yours. **Offline is a state, not an error**: the
daemon knows last-seen, every surface says it in the same words, and a
fetch from a sleeping colleague queues until the machine returns —
enclaved's send queue, mirrored.

**Rust `depot/`, grido's split verbatim.** `client.rs` is the only file
that opens the socket, handing up plain types (`Peer`, `ShareEntry`,
`Fetch`, `Progress`) — the UI never sees JSON or a socket, as grido's
never imports IronCalc. Then `state.rs` (`DepotApp`), `app.rs`,
`commands.rs`, `keymap.rs`, and
`ui/{ribbon,browser,preview,transfers,dialogs,icons}`, one `impl
DepotApp` block each. Durable state is the daemon's — share root,
transfer queue and resume journal, recent fetches, access log, listing
cache; the window keeps peer, path, selection and scroll, so closing it
loses nothing.

### MCP surface

Browsing a colleague's share is free: it is read-only by construction.
What confirms is anything landing bytes on this machine, and above all
anything that **publishes** — the one way an agent could hurt you here
is copying a private file into the folder the whole enclave reads.

Read (run freely):

- `depot_overview` — the entry point: every member's share in a
  few lines (@address, online or last-seen, top folders, items, size,
  last change).
- `depot_list` — one directory: name, size, modified, kind;
  capped and paged, each row carrying a `ref`.
- `depot_find` — name, glob, extension or date, in one share or
  across the enclave; capped rows of @paths.
- `depot_stat` — one entry in full, plus whether you already
  hold a copy and where.
- `depot_peek` — the head of a file without fetching it: text
  lines, a csv's columns, a docx outline, a workbook's sheet names, a
  thumbnail. "What's in @carolin's Q3 folder", without 400 MB.
- `depot_mine` — what *you* publish and who fetched it. The
  privacy mirror; the call to make before publishing.
- `depot_transfers` — in flight, queued and recent, with
  progress and outcomes.

Acting (the daemon's native dialog, preview inside):

- `depot_fetch` — copy a file or folder here; the dialog shows
  source @path, size (count and total for a folder), destination and
  the peer's state. Opt-in allowlist per peer and size, since this is
  the one people repeat all day.
- `depot_open` — open an entry read-only in Scribe, Grido,
  Podium or the previewer — streamed, not copied. One-line confirm.
- `depot_publish` — put a local file or folder into *your*
  share. The dialog says plainly that the whole enclave will be able to
  read it, and shows what it is. Never allowlistable.
- `depot_unpublish` — move something out of your share. It
  stops being visible; it is never deleted.

Rules baked into the descriptions: one address everywhere —
`@carolin/Designs/logo.svg`, the suite's `@person[/computer]` plus a
path inside their share, never a URL. Overview before list, list before
peek, peek before fetch; "read the whole share" is impossible by cap.
Refetching an unchanged file is refused with the path you already have.
Offline is a sentence, not a code ("@carolin's machine is offline, last
seen 18:40 — queued"). It is said outright that no tool writes into
someone else's share, so the model stops looking. Sending stays
enclaved's `enclave_send_file`: fetch is the pull, drop is the push,
one of each. A Depot skill ships with the agent layer.

### UI

One window: a calm two-pane browser on grido's furniture.

- **Left rail — the enclave.** "My share" pinned on top, then every
  colleague with an online dot and last-seen; offline members stay
  listed and greyed, never vanish. @-addressing you can click.
- **Centre — the share.** Breadcrumb (`@carolin / Designs /`), rows of
  name, size, modified, kind — or thumbnails in image folders.
  Type-to-jump, sortable headers, generous rows, tabular figures.
  Double-click opens read-only; Fetch takes a copy.
- **Right — preview and detail** (collapsible): images, PDFs and text
  streamed on demand, documents summarised through Scribe, Grido or
  Podium; size, hash, "you fetched this on 12 Aug".
- **Bottom — transfers** (collapsible): in flight with progress, queued
  for sleeping peers, recent with reveal-in-folder.
- **My share** — the same browser on your own root, plus one honest
  line ("everyone in Acme can read these 214 files") and the **access
  log**: who fetched what, when. Depot's trust feature.
- **Ribbon** on grido's geometry, few groups because a browser needs
  few: Home (Open, Fetch, Publish, Reveal, Refresh), View (list/grid,
  sort, preview, hidden), Share (open the folder, access log, what's
  visible), Help (F1). Every action is a command id — `fetch`,
  `open_readonly`, `publish`, `up`, `back`, `refresh`, `copy_address`,
  `reveal`, `shortcut_help` — bound in `keymap.toml`, grido's order.
- **Other surfaces** — the daemon's confirm previews, a native
  destination picker, the suite's @-picker, and a conflict dialog when
  a fetch would overwrite (keep both / replace / skip, never silent).
- **Theming** is grido's `theme.rs` unchanged: Omarchy `colors.toml`,
  every surface derived, live switches, light themes real, neutral dark
  elsewhere. One accent; offline is grey, not red, because offline is
  normal. Empty states instruct: "Nothing in your share yet — put files
  in ~/Enclave/Share and the enclave can read them".

### Interlinks

- **enclaved** — the tailnet, @-resolution, notifications and confirm
  dialogs are all its; the share protocol is the drop port's twin, same
  handshake, opposite direction.
- **Scribe / Grido / Podium** — a colleague's .docx, .xlsx or .pptx
  opens read-only from their machine, and editing offers "save as your
  own copy": ownership shown, not explained. Their open dialogs each
  carry a Depot tab.
- **Post** — an attachment too big for mail goes in your share and the
  mail carries `@you/…` instead; "put that where the team can find it"
  is a publish. Depot never mails.
- **Bursar** — the year's exports sit in the owner's share and the
  accountant fetches them read-only: no second set of books.
- **Chat** — a message carries a Depot address, so a big file is
  referenced rather than replicated into everyone's log.
- **Almanac** — a meeting's pre-read lives in the organiser's share and
  the event card links to it; nothing is duplicated.
- **Vault** — the line between the two: a share is readable by the
  whole enclave, so secrets never go in one.
- **Mobile** — browse, preview, fetch to the phone or forward to your
  own computer. The phone reads; it does not publish.
- **The agent layer** — "get the logo from @carolin's share" is
  overview → find → peek → fetch: four calls, one confirm. Depot stays
  out of "today's summary": a changed share is not news.

### Non-goals

No cloud copy, replication, mirroring or "make available offline" tier
— availability follows the owner's machine, and saying so beats faking
it. No write access to anyone else's share, ever; no locking or
checkout, because nothing is co-edited. No versioning or trash: offsite
backup is the time machine, Depot shows what is there now. No conflicts
to resolve — a fetched file is a copy and the copy is yours. No FUSE
mount, network drive, SMB or WebDAV gateway: a window, not a
filesystem. No external sharing, public links or expiring URLs. No
groups or roles, no quotas, no admin console over other people's
shares. No media streaming, no transcoding, no local index of other
people's files — search asks the owner's machine. KISS.

## Open

- Per-file/per-folder visibility within a share (v1: whole share
  visible to the whole enclave).
- Local cache of recently fetched files, and whether it needs
  invalidation or stays a plain copy.
