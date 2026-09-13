# Plan: enclaved — suite evolution

enclaved grows from the tray app into the suite's spine: the one
install, the one daemon, the one MCP. What's built stays; this is
what changes.

## Settled (from the suite plan)

- **One executable**: the Rust `enclave` binary embeds the Go daemon
  as bytes, extracts it to the state dir (build-hash versioned,
  atomic) and spawns it. Two processes at runtime — a UI crash never
  drops the network.
- **The daemon stays Go and thin**: tsnet, the local socket, and the
  headless product modules. Rust rewrite rejected (no mature Rust
  tailscale client) — the socket API isolates the suite from that
  choice.
- **The tray moves to Rust** (egui) and becomes the launcher: every
  product's manual UI in its context menu, plus OS app-search
  launcher stubs per product.
- **Autostart owns the daemon** (systemd user unit / launchd agent,
  Restart=always). The UI dials the socket, pings, spawns the daemon
  only if dead; the socket bind is the single-instance lock.
- **Version handshake**: a `version` op on the socket; updates
  replace the install dir atomically, UI asks the daemon to restart
  on mismatch — always a matched pair.
- **@-addressing**: `@person` (email or its local part; resolves to
  their one online computer, ambiguity is an error naming options)
  and `@person/computer` — resolved in the daemon, spelled out in
  the MCP tool descriptions, same scheme in every product.
- **No server process — there are only the daemon, the shim, and
  the apps.** Nothing in the package needs a hub: the client spawns
  `enclave mcp` per chat session; the shim routes `enclave_*` and
  provider verbs (`post_*`, …) straight to the enclaved daemon
  socket, and each product's verbs to that product's own socket,
  spawning the product process if its socket is dead. Nothing needs
  to be running beforehand — the chat starts what it needs.
- **Confirm dialogs are their own process** (`enclave --confirm`),
  spawned per acting call by the shim; approval is only a human
  click answered over that child's private pipe — a hijacked agent
  can never both decide and approve. Read verbs run freely;
  per-action allowlists are opt-in.
- **One process per product UI, one socket each**: the product
  process binds its socket — the bind is the single-instance lock,
  the `open` op on it is the second-launch handoff, and one slow
  canvas never stalls another window.
- **The agent layer ships from here**: one `enclave-mcp` registered
  at install exposes every product's tools over the daemon socket;
  suite skills ("today's summary") live alongside it.

## Starter roadmap

1. Daemon/UI split: headless daemon binary owning socket + nodes +
   drops; today's Fyne tray becomes a thin socket client (interim).
2. Socket API: `version` op, @-addressing in `send`, confirm-dialog
   plumbing for acting ops.
3. Rust tray (egui): replaces Fyne — menu, join window, send window,
   launcher entries; ships in the artifact next to the daemon.
4. Install artifact per platform with both binaries + launcher
   stubs; autostart repointed at the daemon.
5. enclave-mcp: tools grow with each product module; descriptions
   teach the @-scheme.

## High-level approach

enclaved is not a product with a window; it is the process the others
run in. One Go daemon holds the network, the state and the policy; one
Rust process holds the tray and every product's window.

### Architecture

- **Go daemon — the spine.** Today's `internal/{daemon,control,drop}`
  keeps its job (a tsnet node per enclave, control polling, the drop
  port, autostart) and grows a **module registry**: each product is a
  package under `internal/` registering its socket ops, MCP tools and
  confirm rules at start. The core knows enclaves, peers, files and
  policy — never a document, a mailbox or a cell.
- **One socket, one protocol.** `ipc.go` grown up: JSON lines,
  namespaced ops (`enclave.send`, `almanac.overview`, `sheet.read`), a
  `version` op, and a **subscribe** channel pushing events — peer
  up/down, drop arrived, transfer progress. Three clients speak it and
  no fourth: the Rust app, `enclave-mcp`, the mobile app.
- **Rust app — one process, many windows.** `enclave-app` owns the
  tray and the spine's window and hosts each product UI as an egui
  viewport in the same process: one theme, one keymap loader, one
  update, one dock icon.
- **grido's discipline, suite-wide.** `enclave-client` is the spine's
  `engine.rs` — the only crate that opens the socket, handing up plain
  types (`Peer`, `Address`, `Enclave`, `Transfer`, `Confirm`); product
  UIs are library crates that never link it. Then `state.rs`,
  `app.rs`, `commands.rs`, `keymap.rs` and
  `ui/{tray,people,activity,enclaves,confirm,dialogs,icons}`, one
  `impl EnclaveApp` block each. `enclave-ui` carries grido's
  `theme.rs`, ribbon geometry, painter icons and the @-picker, so
  every product inherits the same looks for free.
- **State.** Durable in the daemon: session token, memberships, node
  state, roster, drops, send queue, pending confirms and outcomes,
  notifications, allowlists, each module's data. Ephemeral in the UI:
  windows, selection, scroll, drafts. Killing the UI loses nothing and
  the machine stays reachable — the reason for the split.
- **The confirm broker is daemon-side, on purpose.** A module marks an
  op as acting; the daemon mints a pending confirm carrying that
  product's preview payload and asks a **UI process it spawned** to
  render it. Approval comes back only over that process's channel —
  there is no `approve` op on the socket, so a hijacked agent cannot
  reach one. With no UI running the daemon starts one, or routes to
  the phone. Timeout denies. @-addresses likewise resolve in the
  daemon and nowhere else, so every product agrees on who `@ale` is.

### MCP surface

One `enclave-mcp`, registered once at install, carries every module's
tools; these are the spine's own. Read runs free, acting confirms.

Read:

- `enclave_overview` — the session's first call: this computer, its
  enclaves, who is online, waiting drops, pending approvals, which
  products are available. A dozen lines.
- `enclave_people` — the roster: @addresses, their computers, online.
- `enclave_computers` — machines as `@person/computer`: OS, online,
  last seen. Capped, one line each.
- `enclave_resolve` — what an address means before acting on it.
- `enclave_send_check` — the preview: which computer a send reaches,
  name and size, online or queued, what the human will see. Touches
  nothing, so iterating is free.
- `enclave_transfers` — sends by state (queued, in flight, delivered)
  and drops received, each with a stable id.
- `enclave_received` — recent incoming files with local paths, so
  "open what @ale just sent" needs no guessing.
- `enclave_approvals` — decided and pending confirmations with their
  outcomes, so an agent resuming later learns what happened.

Acting:

- `enclave_send_file` — drop a file on `@person` or
  `@person/computer`; the dialog shows the file card and the target
  machine, an offline target queues instead of failing.
- `enclave_open` — open a local file in the product owning its type
  (docx → Scribe, xlsx → Grido, pptx → Podium). One-line confirm, the
  natural allowlist candidate.
- `enclave_notify` — a desktop notification on *this* computer for a
  long job. Visible by definition, no confirm.

Rules in the descriptions: start at the overview; pass ids and
addresses back verbatim; errors are instructions ("`@ale` is
ambiguous: ale@…, alessandro@…"; "`@ale/laptop` is offline — send
anyway to queue it"). Joining or leaving an enclave, removing a member
and editing allowlists have no tools at all.

### UI

- **The tray is the launcher, not a window**: grey icon disconnected,
  green connected; account line, each enclave with Connect/Disconnect,
  Send a file…, Open received files, the product list, Settings, Quit.
  A per-OS tray crate behind a thin trait; egui draws what it opens.
- **The Enclave window** — three tabs on grido's button geometry (icon
  band, one shared label line) rather than a full ribbon, the spine
  having no formatting to offer. **People**: the roster as calm cards,
  colleagues and their computers, online dots, drop a file on a person
  to send — the AirDrop face. **Activity**: transfers with receipts,
  drops received and what the agent did, in plain sentences.
  **Enclaves**: memberships, this computer's name, join another.
- **The confirm dialog** — the suite's most important surface: its own
  always-on-top viewport, never inside the asking product's window.
  One sentence of what will happen, who asked, the product's real
  preview (mail body, file card, diff, rendered invoice), Approve /
  Deny / Open first, a countdown defaulting to deny. No default button
  under Enter, and approving is the one action the command registry
  withholds — it takes a real click.
- **Other surfaces**: the join window (paste the code, opens on first
  run); the send sheet (file picker plus @-picker with online state
  and a queue notice); drop notifications with Open / Show in folder;
  Settings (device name, autostart, notifications, allowlists,
  assistant registration, update state); the F1 shortcut viewer.
- **Commands and keymap** — every other action is a named command
  (`send_file`, `open_received`, `connect`, `join_enclave`, …) bound
  in `keymap.toml` with grido's lookup order; products keep their own
  keymaps, the spine's holds the global chords.
- **Theming** — grido's `theme.rs` lives in `enclave-ui` and loads
  once per process, so an Omarchy switch restyles the tray, the spine
  window and every open product at once; the system accent plays that
  role on macOS and Windows, Enclave green is the fallback. Calm
  typography on one scale, generous spacing, one accent, red kept for
  disconnected and failed. Empty states teach: "No colleagues online —
  anything you send waits in the queue."

### Interlinks

The socket, the roster, the notifier and the confirm for all of them.

- **Post** — an attachment for a colleague inside the enclave leaves
  as a drop, not as mail; incoming ones go to `enclave_open`.
- **Almanac** — the reminder schedule lives in the daemon, so a closed
  window still reminds; the notification path is the drop path.
- **Chat** — chat logs replicate machine-to-machine on the same
  tailnet and drop-style protocol; mentions autocomplete from the
  daemon's roster; calls dial peers' enclave addresses.
- **Scribe / Grido / Podium** — "send it to @ale" is
  `enclave_send_file`; a received .docx/.xlsx/.pptx opens in the right
  window by type; each reaches the suite through the same typed
  client, never a socket of its own.
- **Bursar** — dropped receipts land in its inbox; filing deadlines
  ride the same notifier as meetings.
- **Depot** — the daemon serves each member's share read-only, and its
  online state is what "@carolin's machine is offline" means suite-wide.
- **Vault** — secrets replicate over the enclave, membership is
  enclaved's identity, revealing a value confirms like any acting op.
- **Mobile** — the phone runs *this* daemon module through `gomobile`,
  speaks the same protocol, appears as `@ed/phone`, and is the
  approval surface when the desk is empty.
- **The agent layer** — one registration at install, one namespace,
  the @-scheme in every tool description; skills ship beside the
  binary.
- **enclave-control** — the only cloud piece: memberships, invites,
  preauth keys, update manifests, content-free push wake-ups. No file,
  message or secret passes through it.

### Non-goals

No CLI and no local web UI — the tray, the windows and the agent are
the whole interface. No second MCP server, socket, autostart entry or
installer per product: one install, one daemon, one namespace. The
daemon parses no documents, spreadsheets, decks or mail bodies; it
moves bytes and enforces policy. No approval path an agent can reach:
no `approve` op, no agent-settable allowlists, no tools for joining,
leaving or removing. No admin surface in the client — invites and
removals stay on the dashboard. No plugin API, no third-party modules.
No sync engine, no conflict resolution, no cloud-drive semantics:
Depot's ownership model is the answer. Not a general VPN — no exit
nodes, no subnet routers; computers join enclaves, networks do not. No
custom crypto beyond tailscale and the enclave identity. KISS.

## Open

- Which Rust tray-icon path is solid on all three platforms (egui is
  windows; the tray itself needs a per-OS crate).
- Windows installer (also the signed-installers launch blocker in
  the product plan).
