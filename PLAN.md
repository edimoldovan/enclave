# Plan: Enclave

Enclave is an **MCP-driven product suite for SMBs** on a private
company network. The products:

| Product | What it is |
|---|---|
| **enclaved** | the private network client — built |
| **Post** | mail |
| **Almanac** | calendar |
| **Chat** | team chat + calls |
| **Scribe** | documents |
| **Ledger** | spreadsheets (repo `grido/`) |
| **Bursar** | accounting |
| **Depot** | everyone's own share, read-only for the others |
| **Vault** | team passwords |
| **Podium** | presentations |
| **agent layer** | the skills + MCP glue that spans them |

The catch that sets the suite apart: the user's own agent is the
primary interface; each product's UI exists for manual use, not as the
main door. "Show me today's summary" returns calendar + email
overviews; "reply to X", "join my next call", "open the attachment and
fix it" all work from the agent chat, with meeting reminders landing
in that same single UI. The enclave — the private tailnet-per-company
network — is the trust spine underneath: who you are, which computers
are yours, who your colleagues are.

## The products

Named as places inside the enclave, always branded with the house mark
(“Enclave Post”, “Enclave Ledger”, …):

1. **enclaved** — the network client. Built. Identity, reachability,
   file drop, and the agent surface (enclave-mcp).
2. **Post** — mail: an MCP bridge to the company's existing provider
   (Gmail / Microsoft 365): overview, read, reply, attachments. Plus a
   **simple email client** for when the user wants to work manually —
   a client only, never a hosted mail service.
3. **Almanac** — calendar: same bridge model, its own product: today's
   meetings, reminders, call links to click or have the agent join.
   Like every product, it has its own manual UI.
4. **Chat** — a simplified Slack with Zoom-like calls among
   enclave members. Serverless: messages are append-only logs in a
   `Chat/` folder inside each member's Enclaved folder, replicated
   machine-to-machine over the enclave; an offline member catches up
   from any online peer that has the history (gossip, no server copy).
   Calls are live P2P — nothing to store.
5. **Scribe** — our own document product, **docx native** (read/write
   .docx directly, as grido does .xlsx — attachments from outside
   open without conversion), with the MCP surface built in — so the agent can edit a
   document and attach it to an email, and the human gets a real
   editor when they want one.
6. **Ledger** — the spreadsheet: fast native xlsx editor (Rust,
   IronCalc + egui; repo `grido/`, already in this folder); gets its
   MCP surface so the agent can read and edit workbooks too.
7. **Bursar** — accounting for the small company, basic by design:
   bookkeeping, invoices in and out, expenses, and producing/sending
   the recurring reports (VAT, annual figures) — the agent can draft
   the report, the human approves, it goes out.
8. **Depot** — shared storage without a shared copy: every member
   has their **own share folder on their own machine**, and the rest
   of the enclave can browse, read, and fetch it — **read-only** —
   whenever that machine is online. Only the owner writes; files live
   only where their owner put them. No cloud copy, no replication, no
   sync conflicts, nothing propagates deletes, and nobody can touch
   your files. The trade-off is availability: a share is offline when
   its owner's machine is. To hand someone an editable copy, they
   fetch it (or you drop it) — then it's theirs.
9. **Vault** — team passwords and shared secrets, end-to-end
   encrypted and replicated to every member's machine (tiny data —
   full replication is right here, unlike Depot); membership in the
   enclave is what grants access.
10. **Podium** — presentations: the third leg of the office trio next
    to Scribe and Ledger, MCP surface included so the agent can build
    and edit decks.
11. **The agent layer** — not an app: the suite's skills + MCP
   registrations, installed once, that let the user's own agent span
   the products — "today's summary" (calendar + email in one answer),
   "check X's reply, open the attachment for editing" (Post → Scribe).
   The user's default agent chat is the single UI; we ship the tools,
   not the agent.
12. **Reminders ride enclaved** — the one always-running daemon.
   It already notifies on file drops; it also fires meeting reminders
   (from Almanac's data) as desktop notifications, and exposes
   them to the agent.

Every product ships with its MCP server. The benchmark sentence the
suite must satisfy: *"check the emails from X, see if he has feedback
on what I sent; if he does, open the attachment for editing."*

Use-case coverage:

- Today's summary (calendar + email) → Almanac + Post + agent layer.
- Read/reply email from the agent chat → Post.
- See calls, click the link, or "join my next one" → Almanac (call
  links; the agent opens the link).
- Meeting reminders in the single UI → enclaved notifications + agent
  layer.
- Team chat, calls, sharing → Chat (+ enclaved file drop).
- "Open a docx, edit, save, email to X" → Scribe + Post.
- "Check X's feedback, open the attachment" → Post + Scribe + agent
  layer.
- Spreadsheets in the same loop → Ledger.
- Basic accounting, sending in reports → Bursar (+ Post to send).
- Reach a colleague's shared files → Depot.
- Team passwords → Vault.
- Decks → Podium.

Suite architecture (settled):

- **One app to the user: one install, one daemon, one MCP.** The suite
  ships as a single install artifact per platform containing two
  binaries side by side (macOS: both inside `Contents/MacOS/`, signed
  and notarized together — no extract-at-runtime tricks, which
  Gatekeeper and corporate AV punish): the Rust app (tray + product
  UIs) and the Go daemon. One `enclave-mcp`, registered once at
  install, exposes every product's tools through the daemon's local
  socket.
- **Every UI / desktop app is Rust** (egui, as grido already proves
  out). The current Fyne tray moves to Rust with it.
- **The tray is the launcher.** Every product's manual UI starts from
  the tray app's context menu (Post, Almanac, Chat, …) — one place
  to find the suite, no separate dock icons or start-menu clutter to
  hunt through. Products can also open via the agent ("open the
  attachment in Scribe") and OS file associations where they fit.
  Each product additionally appears in the OS app search (Spotlight /
  Start menu / .desktop) as a launcher stub — "Enclave Post.app" etc.
  — that just opens its window in the one app, so cmd+space finds the
  suite too.
- **The daemon stays Go, thin.** It is the network (tsnet exists only
  in Go), the local socket, and the headless product modules. A Rust
  rewrite was considered and rejected: tsnet's value is Tailscale's
  battle-tested NAT traversal (disco, DERP) plus the headscale
  protocol, and no mature Rust crate implements either — revisit only
  if one appears; the socket API already isolates the suite from that
  choice.
- **Autostart owns the daemon, not the UI**: the systemd user unit /
  launchd agent points straight at the daemon binary,
  `Restart=always` (as today). The UI dials the socket, pings, and
  only spawns the daemon if the socket is dead; the socket bind is
  the single-instance lock.
- **Version handshake**: the socket API gets a `version` op. Updates
  replace the whole install dir atomically; after an update the UI
  notices the mismatch and asks the daemon to restart itself, so UI
  and daemon always run as a matched pair.

- **One shared layer, two crates.** `enclave-ui` is the design
  system every product wears: grido's theme generalized (Omarchy
  `colors.toml` on Linux, system accent on macOS/Windows, every
  surface derived from bg/fg/accent, light and dark first-class),
  one type scale, one spacing scale, the painter-drawn icon set, the
  ribbon kit (tabs, labelled groups, icon band with one shared label
  line), the widgets (buttons, fields, dropdowns, checkboxes,
  list/detail split view, toasts, empty states, the @-picker, the
  confirm-dialog viewport), window chrome, the keymap loader +
  command registry, the F1 shortcut viewer. `enclave-client` is the
  only crate that opens the daemon socket: typed API, subscribe
  events, the `version` op — product UIs never see JSON or a socket.
  No product ships its own theme, icons, ribbon, widgets or socket
  code.
- **HTML is a component, not a platform.** One sandboxed webview
  (wry) in enclave-ui, used only where HTML is mandatory: Post's
  email bodies (remote images off by default) and provider OAuth
  sign-in. Nowhere else. OAuth tokens live in the OS keychain,
  through enclave-client.
- **@-addressing, one scheme everywhere.** Enclave members and their
  computers are addressed as `@person` (their email or the part before
  its @; resolves to their one online computer, ambiguity is an error
  naming the options) and `@person/computer` for a specific machine.
  The scheme is baked into the MCP tool descriptions so any agent chat
  resolves "send this to @ale" — and Chat uses the same mentions
  with real autocomplete. UX rule: an address means the same thing in
  every product and in the agent.

- **Mobile: one native app, many features** — same model as desktop.
  A single native app per platform (iOS, Android) that joins the
  phone to the enclave and carries the suite's mobile-worthy faces:
  Chat (chat + calls), meeting reminders, Post/Almanac overviews,
  Vault, browsing colleagues' Depot shares, receiving drops. Not a
  port of the editors — the phone is for reaching, reading, and
  replying.

Process: one project, one codebase. Each product has a plan file
here — PLAN-enclaved.md, PLAN-post.md, PLAN-almanac.md,
PLAN-chat.md, PLAN-scribe.md, PLAN-ledger.md, PLAN-bursar.md,
PLAN-depot.md, PLAN-vault.md, PLAN-podium.md, PLAN-mobile.md —
implementation is then delegated to Opus 5 agents.

Risks (open, with the current answer):

- **Total machine loss** → offsite backup for a separate fee:
  E2E-encrypted object storage (only the enclave holds keys — the
  provider sees ciphertext), versioned with Time Machine-style
  recovery: browse back in time, restore a file, a folder, or the
  machine's Enclave data wholesale. Remains: companies that skip the
  fee have no net.
- **Prompt injection at the agent** → read tools run freely; acting
  tools (send file, send mail, delete) make the *daemon* pop a native
  confirm dialog, so a hijacked agent can never both decide and
  approve. Per-action allowlists are opt-in. Remains: a hostile email
  can still mislead the human into approving.

Pricing (settled): **€10 per member per month**, everything included,
billed per enclave via the control server. Offsite backup is the one
add-on: **+€5 per member per month** with a clear storage allowance.
Round numbers, one invoice, no per-product nickel-and-diming.

## The network

Private tailnet-per-company networking. Two pieces:

- **enclave-control** (`server/`, Go) — control server: admin dashboard,
  invites, embedded Headscale. ✅ built.
- **enclaved** (`enclaved/`, Go + Fyne) — the client: a system tray app
  owning this machine's tailnet nodes and file drops. ✅ built.

An account can be part of **multiple enclaves** (companies). **Computers
appear on enclaves, never apps** — one tailscale node per (machine, enclave),
owned by enclaved.

### Decisions (settled)

- **Server built on `~/dev/go/go-app-template`** — layout, conventions,
  `__okf/`, check.sh, Kamal + Litestream deploy. SQLite (headscale supports
  nothing else self-contained; two DB files on `/data`, both replicated).
- **Headscale v0.29.3 embedded in-process**, driven over its gRPC unix
  socket. One public port; reverse proxy to headscale on loopback; `/v1/*`
  blocked; embedded DERP advertises the public host (TLS via kamal-proxy),
  STUN on UDP 3478.
- **Company isolation is an ACL**: one headscale user per company + static
  `autogroup:member → autogroup:self:*` policy. Verified: same-company nodes
  ping, cross-company nodes don't even see each other.
- **Admin signs in with OAuth** (Google/GitHub, template auth: code+PKCE,
  sealed-cookie sessions; `GET /auth/dev?email=` in development). Dashboard:
  create enclaves, add people, copy invite links, remove members.
- **The admin delivers invites personally. The server sends no email.**
  Adding a person creates their account + membership immediately; the invite
  (single-use, 7 days, bound to that person) only hands a machine its
  session. The raw link is shown once on the dashboard.
- **Joining is paste-a-code, not deep links.** The invite link opens a public
  page: which enclave, download links, and the invite code (the token) as a
  click-to-copy field. The employee pastes it into enclaved's join window.
  Deep links rejected: tray apps can't reliably own a URL scheme across the
  three platforms and the failure mode is silent.
- **enclaved is Go + tsnet + Fyne**: a system tray app, autostarts at login.
  It owns per-enclave nodes and state, listens on the drop port, lands
  accepted files in `~/Enclave/Received` — the computer is reachable
  whenever it is awake, no window open.
- **Two session kinds.** Admin browser: sealed cookies, no table. Machines:
  hashed Bearer tokens (`sessions`), minted by invite redemption, spanning
  all the account's enclaves. Removing a member from one enclave kicks only
  that enclave's nodes; sessions and other memberships survive.

### Server: enclave-control ✅

Template layout + `internal/tailnet` (embedded headscale), `internal/token`,
`internal/api`, `internal/account`, `internal/company`, `internal/machine`,
pages (login, dashboard, join, download). Tables: accounts (email + optional
OAuth identity), companies (↦ headscale user), memberships (many-to-many,
owner|member), invites (account-bound, hashed, single-use), sessions
(machine Bearer), machines (preauth key ↦ account — how peers get colleague
names and removal finds nodes).

Routes: see `server/__okf/api/`. Machine API: `POST /join` (invite → session),
`GET /me`, `POST /machines/register` (→ 1-hour single-use preauth key),
`GET /machines`. Admin: dashboard forms. Public: `/join/{token}`, `/download`.

Verified end to end: dashboard flow, single-use invites, real tailscale nodes
registering, same/cross-enclave ACLs, per-enclave member removal.

### enclaved (the client) ✅

Go + Fyne system tray app, one binary, all platforms. Grey icon = not
connected, green = connected. No CLI, no local web UI — the tray and its
small windows are the whole interface.

- Menu, not joined: "Join an enclave…" (paste window, opens automatically on
  first run). Joined: account line, per enclave "Name — connected /
  disconnected" with Connect/Disconnect, "Send a file…", "Open received
  files", "Join another enclave…".
- **Disconnect is local only**: the machine stays on the enclave until the
  admin removes it in the dashboard; Connect brings it back, no new invite.
- Multi-enclave: one tsnet node per enclave, state under
  `~/.local/share/enclave/`, membership poll picks up enclaves the admin
  adds later.
- File drop on port 40404: JSON header → accept byte → stream →
  `~/Enclaved/<sending-computer>/`, accepted by default with a desktop
  notification on arrival.
- Autostart installs itself on first run (systemd user unit / launchd
  agent; Windows with the installer work).
- `e2e/` is a headless test driver (join/wait/send) — dev tool only.

### Roadmap (all committed, in priority order)

Launch blockers:

1. **Signed installers** — macOS notarized dmg/pkg, Windows signed installer
   (+ registry autostart), Linux package. Unsigned scare screens kill SMB
   adoption. Includes the /download/{platform} routes serving real builds.
2. ✅ Auto-update — the client polls the server every minute and
   reconciles its state: enclaves the admin added appear, machines the
   admin removed disconnect and clean themselves up, the tray always
   tells the truth. (Getting new binaries onto machines is part of the
   installers work, item 1.)
3. **Offline send queue** — sending to a sleeping/offline colleague queues
   on the sender and delivers when the target comes online. Beats AirDrop
   at its own game.
4. Deploy: fill the YOUR_* placeholders in server/config/deploy.yml and
   litestream.yml, hostname at the Hetzner box, real OAuth creds, bake the
   production control URL into enclaved's config default.

Security hardening:

5. **New-machine approval** — admin approves each computer before it joins
   the enclave (security fix + trust feature in the dashboard).
6. **Machine session expiry/rotation** — sessions currently live forever;
   add rotation and a revocation story beyond member removal.
7. **Audit log** — who added/removed whom and when, visible to the owner.
8. **Privacy policy + ToS** pages linked from the homepage.

Server operability:

9. **DERP bandwidth monitoring** — relayed transfers ride the Hetzner box;
   meter it, alert on it, consider per-company caps.
10. **Restore drill** — actually restore both SQLite files from Litestream
    on a clean box and write down the steps (docs/restore.md).
11. **Metrics + alerts** — basic server health, join/registration failures,
    node counts.

Product:

12. **OS integration** — right-click "Send with Enclave" (Finder/Explorer/
    file managers) and drag-a-file-onto-the-tray. Habit-forming.
13. **Delivery receipts** — sender sees "received 14:02" per drop.

Being considered (not committed): —
