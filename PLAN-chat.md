# Plan: Enclave Chat

Team chat + calls for the enclave: a simplified Slack with Zoom-like
calls, serverless — no server ever holds company chat.

## Settled (from the suite plan)

- **Storage is files, not a server**: messages are append-only logs in
  a `Chat/` folder inside each member's Enclaved folder, replicated
  machine-to-machine over the enclave. An offline member catches up
  from any online peer that has the history (gossip).
- **Calls are live P2P** over the enclave — nothing stored.
- **Deliberately very easy.** **No threads**: conversations are flat,
  channels and DMs, one scrolling list each. **Copy-paste works
  everywhere**: text, images and files paste into the composer from
  the clipboard, and messages, images and attachments copy back out.
- **@-addressing**: mentions use the suite scheme (`@person`,
  `@person/computer`) with real autocomplete in the UI.
- **Manual UI** in Rust (egui) from the tray / app search stub;
  MCP tools through the enclaved daemon socket (read channels, post —
  posting confirms via the daemon dialog).
- **Mobile carries Chat** (chat + calls) in the native mobile app.

## Starter roadmap

1. Message model: append-only per-sender logs, ordering, gossip
   catch-up between peers; channels + DMs.
2. Replication over the enclave (drop-port style protocol).
3. UI: channel list, conversation view, mentions with autocomplete,
   file sharing via enclaved drops and clipboard paste.
4. Calls: 1:1 audio first (P2P over the tailnet), then group + video +
   screen share.
5. MCP tools: unread overview, read channel, post message.

## High-level approach

Chat is the one product whose engine must keep working with **no
window open**: messages arrive, logs replicate, calls ring. The Go
module inside enclaved *is* the product; the Rust window is a view.

### Architecture

- **`enclaved/internal/chat/` (Go)** — log store, gossip, channel
  membership, unread state, notifications, call signalling, and the
  IPC ops behind the MCP tools. It speaks people, channels, messages
  and calls; no UI concepts leak in.
- **The files are the truth, the index is a cache.** `Chat/` in the
  Enclaved folder holds one append-only log per (author, computer) —
  single writer, append only, nothing to merge — beside the copies of
  other people's logs this machine received. A rebuildable SQLite
  index over them answers channel views, unreads and search.
- **Ordering without a server**: a record carries its author's
  sequence number and a vector of what that author had seen, so an
  answer never renders before what it answers; display order is
  (timestamp, author) with a stable tiebreak. Records are immutable —
  an edit or retraction is a new record pointing at the old one, the
  only honest way to change something already on six machines.
- **Gossip**: on any peer connection, trade a have-vector (author →
  last seq) and pull the gaps; whoever is online and holds the history
  serves it, so a new member catches up from anyone. One more stream
  type on enclaved's drop listener, not a second network. DMs and
  named private channels are sealed to their member set, so a relaying
  peer stores bytes it cannot read. Attachments stay out of the log —
  name, size, hash and origin machine go in; bytes travel as a drop.
- **Calls**: the daemon owns signalling and ring state, the Rust
  process owns capture, encode, decode, mix and render. Desktop tsnet
  is userspace and invisible to other processes, so media rides a
  framed local channel to the daemon, which relays it onto the tailnet
  and models none of it — machine to machine, nothing stored.
- **`chat/` in the Rust app — grido's split verbatim.**
  `client.rs` is the only file that opens the socket, handing up plain
  types (`Channel`, `Message`, `Person`, `Call`, `Unread`): the UI
  never sees JSON or a socket, as grido's never sees IronCalc.
  `media.rs` is the second such boundary, equally off-limits to `ui/`.
  Then `state.rs` (`ChatApp`), `app.rs` (update loop + dispatch),
  `commands.rs`, `keymap.rs`, `ui/{sidebar,conversation,composer,
  call,header,dialogs,icons}.rs`, one `impl` block each.
- **State split** — durable in the daemon: logs, index, membership,
  read marks, mutes, drafts, pending confirmations, call state.
  Ephemeral in the UI: scroll, selection, the audio pipeline. Closing
  the window loses nothing. **Presence is reachability** — the daemon
  knows which nodes are up; there is no second presence system.

### MCP surface

Registered by the daemon into the one `enclave-mcp`. Read tools run
free; acting tools pop the daemon's native confirm dialog, the only
door a message leaves by.

Read:

- `chat_overview` — unread across channels and DMs, one capped
  line each, mentions first. The first call, and Chat' share of
  "today's summary". `chat_channels` lists them all: purpose,
  members, last activity, unread, muted.
- `chat_read` — a window of one channel or DM: messages with
  stable refs, `@author`, time, attachment refs; capped, defaulting to
  what is unread. There is no thread to read — a conversation is flat.
- `chat_search` — text, author, channel or range → refs and
  snippets. `chat_mentions` — where the user was named,
  answered and not: "did anyone need me?" in one call.
- `chat_calls` — calls happening now, who is in them, whether
  this computer is being rung.

Acting (draft first, then the dialog):

- `chat_draft` — compose for a channel or DM: resolves
  `@mentions`, attaches files, returns the rendered message and a
  `draft_id`. Posts nothing, so iterating is free.
- `chat_post` — post a draft. The dialog *is* the preview:
  the message as it will look, the channel, who gets woken ("@here —
  notifies 7 people").
- `chat_share_file` — a file into a channel: a drop, or a
  reference into the sender's Depot share when it is large; the dialog
  names file, size and destination.
- `chat_react` — an emoji on a message. Cheap and reversible:
  a one-line confirm, and the obvious first allowlist entry.
- `chat_edit` / `chat_retract` — one's own message
  only; confirms with a before/after diff, and says plainly that peers
  drop the old text when the tombstone reaches them.
- `chat_channel_new` — a channel with a purpose and members;
  the dialog names everyone added.
- `chat_call` — start or join a call with `@people`. Ringing
  someone's computer confirms; joining one you were invited to is the
  one-liner. `chat_mute` changes only your own state, so it
  needs no dialog at all.

Rules baked into the descriptions: every message has a short stable
`ref` (author + seq), so nothing is addressed by fuzzy quote;
`@person` resolves through the daemon and ambiguity is an error naming
the options; times come back local and ISO; reads are capped, so "read
the whole channel" is not expressible; overview before detail, draft
before post; errors are instructions ("#sales has no member @fredrik —
he is in this enclave; add him?"). What colleagues *write* reaches the
model fenced as data, never as orders.

### UI

One window, three columns, grido's chrome vocabulary without four rows
of ribbon over a message list. Easy on purpose: flat conversations, no
threads to keep track of, and paste anything anywhere.

- **Rail** (left, only with several enclaves) — enclaves with unread
  dots. **Sidebar** — search, channels, DMs, people with online dots;
  muted dimmed.
- **Conversation pane** — one flat scrolling list: messages grouped
  by author, day separators, an unread line, mentions tinted with the
  accent, times in tabular figures; virtualised like grido's grid,
  layouts cached per message. Answering someone is a message that
  quotes them, not a side channel. Attachments are file cards (name,
  size, whose machine, Open / Save).
- **Header strip** — channel, purpose, members, call, search: grido's
  ribbon geometry (icon band over one shared label line, painter-drawn
  icons) shrunk to one row, because a chat window earns one row of
  chrome, not four. The full-screen backstage holds the rest —
  enclaves, notification rules, retention, devices, assistant status.
- **Composer** — autogrowing, `@` autocomplete over the roster (the
  widget Post and Almanac borrow), `#` for channels, drop a file to
  attach, Enter sends and Shift+Enter breaks — both bindable.
  **Paste is a first-class input**: text, a screenshot or an image
  from the clipboard, and files copied in the file manager all paste
  in and attach; copy goes the other way, out of a message, an image
  or an attachment. **Right pane**, on demand: channel details or the
  call.
- **Call window** — a second egui viewport, as Podium's present mode:
  participant tiles, a speaking ring, mute / camera / screen share /
  leave, network quality in words; minimised, a call bar above the
  composer. **Incoming calls ring from the daemon**, so a closed
  window still rings.
- **Other surfaces** — confirm dialogs with the message preview inside
  them; @-picker; channel creator; file picker with a Depot tab; quick
  switcher; device picker; toasts with Reply and Mute; F1 shortcut
  viewer generated off the command registry, which every action goes
  through (`quick_switch`, `next_unread`, `send_message`,
  `quote_message`, `copy_message`, `paste`, `toggle_mute`,
  `call_answer`, `call_mute_mic`, …)
  and `keymap.toml` binds, in grido's lookup order.
- **Theming and type** — grido's `theme.rs` unchanged: Omarchy
  `colors.toml`, every surface derived, live switches, light themes
  real. One type scale, few weights, initials tinted from the accent
  rather than a colour per person, generous spacing, no badge soup.
  Empty states say something ("Nothing in #general yet — say hello to
  @ale"); offline is a state, not an error: "Ana's machine is asleep —
  she catches up when it wakes."

### Interlinks

- **enclaved** — the whole transport: roster and identity, the drop
  protocol carrying logs and attachments, notifications, the confirm
  dialogs, and the offline send queue, which is why a message to a
  sleeping colleague lands later instead of failing.
- **Almanac** — internal meetings carry a Chat call link, not Zoom;
  the reminder's Join opens the call window, and a meeting can
  announce itself in its channel ("@here standup in 5").
- **Post** — an outside mail conversation continues inside: a mail
  forwards into a channel as a reference card, the mail staying in
  Post; "answer him in mail" hands the text to `post_draft`,
  whose own confirm sends it. Chat never becomes a mail client.
- **Scribe / Grido / Podium** — a document shared in a channel rides
  a drop and opens natively; a Grido range pastes as a small table;
  Podium presents into a call, its audience viewport being the
  screen-share source.
- **Depot** — a large file posts as a reference into the sender's
  share: read-only, on their machine, fetched by whoever wants a copy.
  The attach button carries a Depot tab.
- **Vault** — a log replicates forever, so it must never hold a
  secret: something credential-shaped in the composer offers "put it
  in Vault and post the entry name" — the message carries the name.
- **Bursar** — "ask @ale about this expense" opens a DM with the
  item card attached; his answer comes back as a message, while the
  decision still happens behind Bursar's own confirm.
- **Mobile** — the existential mobile feature: the same channels over
  a recent window, content-free push wake-ups, calls that ring from
  the lock screen, mentions that notify.
- **The agent layer** — "did anything need me while I was out?" is
  Chat' overview beside Post's inbox and Almanac's agenda, one
  answer; and Chat is where the agent reaches a *colleague* rather
  than a file.

### Non-goals

No server, ever: no hosted history, no cloud copy, no bridge to Slack,
Teams, Discord or Matrix, no guests or external users — enclave
membership is the whole ACL. No app platform: no bots, webhooks,
slash-command directory, custom emoji uploads, per-channel themes. No
channel hierarchy, no workspaces inside an enclave, and no threads at
all — a conversation is one flat list. No rich text beyond plain
markdown; canvases, wikis and docs are Scribe's job. No read receipts,
typing indicators, away states or custom statuses — presence is
whether the machine is on. No call recording or transcription: nothing
is stored, and that is the point. No PSTN dialling, no webinars. No
compliance tooling — eDiscovery, admin deletion, legal hold — on a
network with nothing to hold it. No transport or crypto code in the
Rust process, no second MCP registration. KISS.

## Open

- Retention: do logs live forever on every machine, or age out.
- Group call topology (mesh works to ~5 people; beyond that needs an
  SFU on some member's machine — decide when it hurts).
