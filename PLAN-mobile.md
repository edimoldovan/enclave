# Plan: Enclave Mobile

One native app per platform (iOS, Android) that joins the phone to
the enclave — the suite's mobile-worthy faces, not a port of the
editors. The phone is for reaching, reading, and replying.

## Settled (from the suite plan)

- **One app, many features**, mirroring the desktop model.
- Carries: Chat (chat + calls), meeting reminders, Post/Almanac
  overviews, Vault, browsing Depot shares, receiving drops.
- The phone is a real enclave member: a node on the tailnet
  (Tailscale ships iOS/Android network extensions — the join model
  is the same paste-a-code).

## Starter roadmap

1. Network spike: embed the tailscale client libraries in a minimal
   app on both platforms, join an enclave, reach a peer.
2. Join flow: paste the invite code, autostart/always-on VPN
   profile.
3. Chat first (chat, then calls) — the existential mobile
   feature.
4. Notifications: drops received, meeting reminders, mentions.
5. Vault read + Depot browse + Post/Almanac overviews.

## High-level approach

### Architecture

**The same split as the desktop, in a different host process.** The
headless half is the *same Go daemon module* the desktop runs —
enclave state, tailnet nodes, drops, Chat replication, Vault
store, the approval channel — built with `gomobile bind` into an
`.xcframework` / `.aar`. The UI half is native per platform: SwiftUI
on iOS, Compose on Android. No egui on the phone, no shared UI
toolkit.

- **The daemon lives in the VPN process** — iOS: a
  `NEPacketTunnelProvider` writing into an App Group container;
  Android: a foreground `VpnService`. Always-on rules keep it up, so
  the phone is reachable while asleep like any other computer.
- **The app is just another socket client**, speaking the *same*
  JSON-line protocol as `internal/daemon/ipc.go` over a unix socket
  in the shared container (iOS) or a bound service (Android). One
  protocol, three clients: the Rust UIs, enclave-mcp, the phone.
- **The system VPN is the phone's advantage.** Desktop tsnet is
  userspace and invisible to other apps; a packet tunnel puts every
  app socket on the tailnet, so the UI opens ordinary sockets to a
  peer's enclave address for call media and Depot fetches. It has to:
  the iOS extension has a hard memory ceiling and no camera.
- **State**: authoritative state is the daemon's — memberships, the
  session token (Keychain / Keystore, never a file), peer roster,
  send queue, the recent Chat window, the Vault store, pending
  approvals. The app holds view state only: navigation, drafts,
  scroll, the live call, thumbnails. Kill it and nothing is lost.
- **grido's discipline, native idioms.** `EnclaveKit` (Swift) /
  `enclave-core` (Kotlin) is the `engine.rs` boundary: it owns the
  socket, the wire types and the domain model (`Peer`, `Enclave`,
  `Message`, `ShareEntry`, `Secret`). Views never see JSON and never
  import the gomobile bridge, exactly as grido's UI never imports
  ironcalc. Layout: `core/` (Go), `ios/{EnclaveKit,Enclave,
  EnclaveTunnel}`, `android/{core,app}`.
- **The command registry survives; keymap.toml does not.** Every
  action is a named command (`chat.send`, `depot.fetch`,
  `vault.reveal`, `call.join`) dispatched in one place; with no key
  chords on a phone the bindings become App Intents / App Actions,
  share-sheet and notification actions, assistant phrases.
- **Push without a server that learns anything.** APNs/FCM are
  unavoidable on iOS, so the *sending* daemon asks enclave-control
  for a **content-free wake-up** for a device id — the server holds
  push tokens ↦ account and nothing else; content is then fetched
  peer-to-peer. No message, sender or filename reaches the server.
  Android's always-on service usually needs no push at all.

### MCP surface

Mobile adds few tools, because it mostly **extends the address
space**: `@ed/phone` is a computer like any other, so
`enclave_computers` and `enclave_send_file` already reach it. What is
genuinely new is the phone as *the approval surface when the desk is
empty*.

Read (run freely):

- `mobile_devices` — the user's phones: @address, platform,
  enclaves, online, push/approval capability, last seen. One line
  each, never a dump.
- `mobile_approvals` — pending and recent approval requests
  with outcomes, so an agent resuming later learns what was decided.

Acting (each confirms — on the desktop daemon's native dialog when
the user is there, on the phone when they are not):

- `enclave_ask_phone` — route a decision to the phone with a preview
  payload (mail draft, file card, report diff) and wait; returns
  approved / denied / edited / timed out. The agent gets the draft
  back to revise; the human sees a rendered preview, not JSON.
- `enclave_notify_phone` — a nudge with no decision attached ("the
  VAT draft is ready").
- `enclave_open_on_phone` — handoff: open a Chat thread, a Depot
  path, a call link or a received file on the phone.

Rules: overviews answer in a handful of lines; every preview is a
real surface on the phone and a compact diff to the agent; approvals
travel over the enclave, never the control server; an offline phone
falls back to the desktop dialog. The phone never both decides and
approves.

### UI

**One app, five destinations** — iOS `TabView`, Android bottom bar,
identical information architecture:

1. **Chat** — channels and DMs, thread view, composer with `@`
   autocomplete; call button in the thread header.
2. **Today** — Almanac's next meetings with a Join button, Post's
   unread summary, reminders. The mobile face of "today's summary".
3. **People** — the enclave roster: colleagues, their computers,
   online dots. Tap for send a file / call / browse their share.
   Where @-addressing becomes something you touch.
4. **Files** — received drops, the send queue, Depot browsing.
5. **Vault** — search and entries; values reveal behind biometrics
   and copy with auto-clear.

Auxiliary surfaces: **Join** (paste the code, then the one
unavoidable system dialog — VPN profile consent — framed so a
non-technical person is not scared off); **Call** (full screen, 1:1
and n:n grids, video both ways, PiP, CallKit / ConnectionService so
it rings and answers from the lock screen); **Approval sheet** (the
daemon confirm dialog's mobile twin: what the agent wants, the
preview, Approve / Deny / Open first, biometrics on high-stakes
ones); **Preview** (QuickLook / system viewer — the phone reads, it
does not edit); **Share extension** ("Send with Enclave" from any
app, the twin of the desktop right-click); **one @-picker** reused by
send, mention, call and handoff; **notification actions** (reply,
accept a drop, approve — without opening the app); **Settings**
(enclaves with local-only Connect/Disconnect, device name,
notifications, sign out).

Look and feel — grido's calm, natively. **The theme derives from the
system** as grido's derives from Omarchy: iOS dynamic colors, Android
Material You from the wallpaper, Enclave green as accent and
fallback; light and dark both first class, no in-app picker.
**Calm typography** — platform faces at generous sizes, Dynamic Type
honoured, few weights, no display face beyond the brand mark.
Generous spacing, one accent, no badge soup (unread counts on Chat
and nowhere else). **Empty states carry the voice** ("Nothing yet —
anything a colleague drops on this phone lands here") and name the
next action; **offline is a state, not an error**: "Ana's machine is
offline", the desktop's own words.

### Interlinks

- **enclaved** — the phone is a node, not a client: it shows up in
  `enclave_computers`, receives drops into Files, sends via the share
  extension. "Send to @ale" works from the phone exactly as from the
  desk; the offline send queue covers a sleeping laptop.
- **Chat** — the flagship. Same append-only logs gossiped over the
  enclave, but the phone keeps a *recent window* and pulls older
  threads on demand. Calls are P2P over the tunnel.
- **Almanac** — reminders arrive as push; Join opens a Chat call
  or the provider's link.
- **Post** — inbox overview, read and reply; an attachment you cannot
  edit becomes "open on my laptop" — a drop to `@ed/laptop` that
  opens in **Scribe** (or **Grido** / **Podium** by type).
- **Depot** — browse `@carolin`'s share read-only, fetch to the
  phone, or forward straight to your own computer.
- **Vault** — replicated to the phone (tiny data), so the password
  you need away from the desk is there offline, behind Face ID.
- **Bursar** — no mobile UI: the phone is where "the VAT draft is
  ready" lands, the PDF previews, Approve makes **Post** send it.
- **Grido / Scribe / Podium** — read-only previews, never editors.
  The verb on the phone is *hand off*.
- **The agent layer** — the desktop agent reaches the human through
  the phone when the desk is empty; a second confirm surface, never a
  second decider. On-device, the same command registry is exposed as
  App Intents / App Actions so the *phone's* assistant runs the same
  verbs.

### Non-goals

Editors on the phone (docx/xlsx/pptx stay desktop). A Bursar UI. An
admin dashboard — invites and removals stay on the web. An agent
runtime on the phone. React Native, Flutter or a web app: native per
platform, no shared UI toolkit. Any server-side copy to make mobile
convenient — no hosted message store, no file cache, no "mobile sync"
tier; wake-ups stay content-free. Full Chat history on the device.
Offline writes beyond a queued message or drop. Tablet layouts in v1.
A Podium presenter remote. Custom crypto — the enclave's identity and
the tailnet are the whole trust story. KISS.

## Open

- Tech per platform (Swift/Kotlin native vs shared Rust core with
  native shells — decide after the network spike).
- iOS background limits vs "always reachable" expectations.
- Store review realities for a VPN-profile app.
