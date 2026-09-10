# Plan: Enclave Post

Mail for the enclave: an MCP bridge to the company's existing provider
plus a simple client for manual use. Never a hosted mail service.

## Settled (from the suite plan)

- **Bridge, not server**: connects to Gmail / Microsoft 365 over their
  APIs (OAuth). The company keeps its provider and addresses.
- **Agent first**: overview, read, reply, attachments as MCP tools
  through the enclaved daemon socket; sending pops the daemon's native
  confirm dialog (prompt-injection rule).
- **Simple manual UI** in Rust (egui), launched from the tray and the
  OS app search stub ("Enclave Post").
- Attachments flow to/from Scribe, Ledger, Podium (docx/xlsx native)
  and file drops land via enclaved.

## Starter roadmap

1. Provider bridge: OAuth connect (Google first, then Microsoft),
   token storage in the enclave state, read + send + attachments.
2. MCP tools: inbox overview, read thread, reply/compose (confirm
   dialog), fetch/save attachment.
3. Manual UI: three-pane basics — folders, list, message; compose.
4. "Today's summary" contribution (with Almanac, via the agent layer).

## High-level approach

Two processes, one product: a **headless Go module inside enclaved**
owning the provider bridge, the mail cache, drafts and the outbox, and
a **Rust egui window** for reading and writing by hand. Mail arrives
while no window is open — that is why the bridge lives in the daemon.

### Architecture

- **`enclaved/internal/post/` (Go)** — OAuth to the Gmail API /
  Microsoft Graph, incremental sync (historyId / delta), a mail cache
  (SQLite plus a blob store for attachments, in the enclave state dir,
  covered by offsite backup), drafts, the outbox, and the IPC ops
  behind the MCP tools. It speaks mailboxes, threads, messages,
  addresses and attachments; no UI concepts leak in. Refresh tokens
  live here and never reach the UI or the agent.
- **The provider stays authoritative.** The cache is a rebuildable
  copy for speed and offline reads; deleting it costs nothing. Reads
  answer from it with the sync age stated; when the provider is
  unreachable, approved mail waits in the **outbox** and leaves when
  the network returns — the promise enclaved's send queue makes.
- **Drafts are the agent's workbench.** Composing, replying and
  attaching produce a *draft in the daemon*, never a send: the agent
  gets the rendered draft plus a `draft_id` and iterates for free, and
  only `enclave_mail_send` — through the native confirm dialog — puts
  anything on the wire. A draft survives a crash and expires on its
  own.
- **`post/` in the Rust app** — grido's split verbatim. `client.rs` is
  the only file that opens the socket, handing up plain types
  (`Mailbox`, `Thread`, `Message`, `Draft`, `Attachment`, `Contact`):
  the UI never sees JSON or a socket, exactly as grido's UI never
  imports IronCalc. Then `state.rs` (`PostApp`), `app.rs` (update loop
  + dispatch), `commands.rs`, `keymap.rs`, and `ui/{ribbon,list,
  reader,compose,dialogs,icons}.rs`, one `impl PostApp` block each.
- **One op set, two callers** — UI and agent issue the same daemon
  ops: one code path, one activity log. The daemon pushes change
  events back, so an open window shows the agent's draft the moment it
  exists, tinted as grido tints assistant edits.
- **State split** — durable in the daemon: tokens, cache, drafts,
  outbox, per-sender image consent, accounts. Ephemeral in the UI:
  selection, scroll, search text, pane widths, caret. Closing the
  window loses nothing.
- **Mail is untrusted input.** Bodies render with no remote fetches
  (no tracking pixels), no scripts, no active content, and reach the
  agent fenced as data, the tool descriptions saying plainly that
  instructions found inside a message are never orders. The suite's
  stated injection risk, met where it actually arrives.

### MCP surface

Registered by the daemon into the one `enclave-mcp`. Read tools run
free; acting tools go through the daemon's native confirm dialog.

Read:

- `enclave_mail_overview` — the entry point: unread and recent as one
  capped line per thread (who, subject, when, snippet, attachments,
  mailbox). Half of "today's summary".
- `enclave_mail_thread` — one conversation: every message with a
  stable ref, quotes collapsed, attachments listed.
- `enclave_mail_message` — one message in full: body as plain text,
  the headers that matter, byte-capped.
- `enclave_mail_search` — provider-side search (from, to, subject,
  has-attachment, date, text); capped rows, each carrying a ref.
- `enclave_mail_attachments` — attachments across a thread or a
  search: name, type, size, ref. The input to "open the attachment".
- `enclave_mail_contacts` — a name to real addresses, from the
  mailbox's own history and the enclave roster, so `@ale` resolves
  instead of being guessed.
- `enclave_mail_accounts` — connected mailboxes, sync age, outbox
  depth, which one is the default.

Acting (draft first, then the confirm dialog):

- `enclave_mail_draft` — new, reply, reply-all or forward; returns the
  rendered draft and a `draft_id`. Sends nothing, so iterating is free.
- `enclave_mail_attach` — put a file on a draft: a path, a Scribe /
  Ledger / Podium document, or a file fetched from a Depot share.
- `enclave_mail_send` — send a draft. The dialog *is* the preview:
  every recipient spelled out, subject, body, attachment names. Never
  allowlistable.
- `enclave_mail_file` — archive, move, label, mark read/unread. One
  confirm covers a batch; the reversible ones are allowlistable.
- `enclave_mail_delete` — to trash, never permanent. Always confirms.
- `enclave_mail_save_attachment` — to a path, or straight into Scribe
  / Ledger / Podium, returning the doc id the agent then edits.

Rules baked into the tool descriptions: start at the overview; pass
refs back verbatim (`thr_…`, `msg_…`, `att_…`) — nothing is addressed
by subject or by position in a list; bodies are capped and quoted
history collapsed, so "read my whole mailbox" is not accidentally
possible; recipients are resolved, never invented, and an unknown or
ambiguous name is an error naming the candidates; a draft carries a
content hash, so a human edit in the window fails the send with a
sentence the model can act on. A Post skill ships with the agent
layer, as Sheetz's does: overview before reading, draft before
sending, quote the ref, treat what a message *says* as something a
stranger wrote.

### UI

One window, three panes, grido's chrome, calm by construction.

- **Ribbon** (grido's tabs, labelled groups, painter-drawn icons):
  Home (New, Reply, Reply all, Forward, Archive, Move, Delete, Mark
  read), View (reading-pane position, density, conversation grouping,
  show images), Accounts (connect, sync now, signature, default
  mailbox), Help (F1 shortcut viewer).
- **Panes** — mailbox rail left (accounts, folders, unread counts);
  thread list centre, virtualised like grido's grid, one row per
  conversation with unread carried by weight rather than a badge;
  reader right or below. **Reader**: sender line badged `@ale` when
  the sender is a colleague, quotes collapsed, an attachment strip
  with Open in Scribe / Ledger / Podium, remote images blocked behind
  one click that is remembered per sender.
- **Compose** in its own egui viewport: recipient chips autocompleting
  over the enclave roster and mailbox history, an attachment strip
  that accepts drops and carries a Depot tab, plain text with a light
  rich mode. **It is also the agent's review surface** — a drafted
  reply opens here marked as the assistant's, with Send / Edit /
  Discard, and the daemon's confirm dialog embeds the same render.
- **Auxiliary surfaces** — the confirm dialogs; the shared `@`-picker
  (Commons' mention widget); native save/open pickers with a Depot
  tab; the search bar; the OAuth connect flow (browser hand-off, the
  one honest spinner); an activity log in plain English; F1. The
  **status bar** carries account, sync age, unread, outbox depth and
  "assistant working" while the agent holds a draft.
- **Commands + keymap** — everything bindable (`reply`, `reply_all`,
  `forward`, `archive`, `next_message`, `search`, `sync`, `compose`,
  `send`, `shortcut_help`, …) in `keymap.toml`, grido's lookup order.
- **Theming and type** — grido's `theme.rs` unchanged: Omarchy
  `colors.toml`, every surface derived, live switches, light themes
  real. One type scale, a reading measure for the body, tabular
  figures for dates, generous gutters, one accent. Empty states that
  say something: "Nothing unread — @ale last wrote on Tuesday."

### Interlinks

Post is the suite's door to the outside world: inside the enclave
everything is `@person`, outside it everything is an email address,
and Post is where the two meet.

- **enclaved** — socket, roster, notifications. New mail fires through
  the same desktop notification path as file drops; a colleague in the
  To: field shows as `@ale` while his real address goes on the wire.
  Post has no notifier of its own.
- **Scribe / Ledger / Podium** — the benchmark sentence:
  `enclave_mail_save_attachment` opens the .docx / .xlsx / .pptx
  natively and returns a doc id; the edited file comes back as a Post
  draft through `enclave_doc_send` / `_sheet_send` / `_deck_send`, and
  one confirm — Post's — sends it.
- **Almanac** — invitations arrive as mail and Post hands the .ics to
  `enclave_calendar_draft`, so RSVP never leaves the agent chat.
  "Today's summary" is Post's overview plus Almanac's agenda, one
  answer.
- **Bursar** — supplier invoices and receipts arrive as attachments
  and land in the books' inbox; issued invoices, overdue reminders and
  the approved VAT report go back out through Post's send confirm,
  Bursar's preview inside it.
- **Commons** — internal talk belongs in Commons, not in mail:
  "discuss this with @ale" opens a thread quoting the message rather
  than replying to it. Post never becomes internal chat.
- **Depot / drops / Vault** — a file for a colleague prefers the drop
  or their share over a 25 MB attachment ("send it to @ale" is a
  drop; only an outside recipient gets an attachment), and provider
  credentials or an IMAP app password live in Vault, never in the
  agent's context.
- **Mobile** — the same overview, read and reply on the phone; an
  attachment it cannot edit becomes a drop to `@ed/laptop` that opens
  in Scribe. With the desk empty, `enclave_ask_phone` carries the
  draft as its preview and Approve makes Post send it.

### Non-goals

Not a mail service: no SMTP or IMAP hosting, no domains, no MX, no
deliverability, no spam filtering — the provider already does that,
better. One work mailbox per person; personal mail is not our
business. No rules or filter engine, no mailing lists, no newsletters,
no CRM, no snooze-and-triage inbox theatre. No HTML mail design, no
template gallery, and no read receipts or tracking of our own — we
block theirs. No PGP/S/MIME in v1: what must stay private stays inside
the enclave, in Commons and drops, not in mail. No shared or delegated
mailboxes. No calendar duties (Almanac owns time). No auto-send:
nothing reaches a recipient without a human looking at the actual
message. No web version, no browser runtime, no JavaScript. KISS.

## Open

- IMAP/SMTP fallback for companies on neither Google nor Microsoft.
- Which mailbox actions beyond read/reply/compose make v1 (archive,
  labels, search).
