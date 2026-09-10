# Plan: Enclave Almanac

Calendar for the enclave: an MCP bridge to the company's existing
provider, its own manual UI, and the source of meeting reminders.

## Settled (from the suite plan)

- **Bridge, not server**: Google Calendar / Microsoft 365 over their
  APIs (OAuth), same model as Post.
- **Agent first**: today's/this week's meetings, call links, "join my
  next one" (the agent opens the link) as MCP tools through the
  enclaved daemon socket.
- **Reminders ride enclaved**: the always-running daemon fires
  meeting reminders as desktop notifications from Almanac's data and
  exposes them to the agent.
- **Manual UI** in Rust (egui), launched from the tray and the OS app
  search stub ("Enclave Almanac").

## Starter roadmap

1. Provider bridge: OAuth connect, read events + call links.
2. MCP tools: today/week overview, next meeting, join link.
3. Reminder feed into the enclaved daemon (notification n minutes
   before, configurable).
4. Manual UI: day + week views; event details with join button.
5. "Today's summary" contribution (with Post, via the agent layer).

## High-level approach

Two processes, one product: a **headless Go module inside enclaved**
owning the provider bridge, the event cache and the reminder clock,
and a **Rust egui window** for day and week. The daemon keeps working
with no window open — the whole reason reminders live there.

### Architecture

- **`enclaved/internal/almanac/` (Go)** — OAuth to Google Calendar /
  Microsoft Graph, incremental sync (syncToken / delta), an event
  cache (SQLite in the enclave state dir, covered by offsite backup),
  the reminder clock, and the IPC ops behind the MCP tools. It speaks
  calendars, events, people and instants; no UI concepts leak in.
- **Reminder clock** — one timer over the next 24 h of the cache,
  firing at *lead time* through the daemon's existing notification
  path (the one file drops use), with Join / Snooze / Open actions.
  Fired and snoozed state lives in the daemon, so UI and agent agree
  on what was already shown.
- **`almanac/` in the Rust app** — grido's split verbatim.
  `client.rs` is the only file that opens the socket, handing up plain
  types (`Event`, `Day`, `Calendar`, `Reminder`): the UI never sees
  JSON or a socket, exactly as grido's UI never sees IronCalc. Then
  `state.rs` (`AlmanacApp`), `app.rs` (update loop + dispatch),
  `commands.rs`, `keymap.rs`, and `ui/{ribbon,week,day,detail,
  dialogs,icons}.rs`, each adding one `impl AlmanacApp` block.
- **State split** — durable in the daemon: tokens, calendars, cached
  events, reminder schedule, drafts awaiting confirmation. Ephemeral
  in the UI: focused date, view mode, selection, drafts, scroll —
  closing the window loses nothing. Offline is a real state: the
  cache answers, the status bar says so, and acting tools refuse with
  a reason instead of half-working.

### MCP surface

Registered by the daemon into the one `enclave-mcp`. Read tools run
free; acting tools pop the daemon's native confirm dialog.

Read:

- `enclave_calendar_overview` — compact agenda for a day or range,
  one capped line per event (time, title, who, where, join link,
  RSVP). Today by default; the "today's summary" contribution.
- `enclave_calendar_next` — the next meeting, its join link and
  minutes-until. What "join my next call" reads first.
- `enclave_calendar_event` — one event in full: description,
  attendees + responses, attachments, recurrence, conferencing.
- `enclave_calendar_search` — by text, attendee or range.
- `enclave_calendar_free` — free/busy for a set of `@people`,
  returning ranked slots: scheduling is one call, not ten.
- `enclave_calendar_reminders` — fired and pending, so the agent can
  say "your call starts in 8 minutes".

Acting (preview first, then the confirm dialog):

- `enclave_calendar_draft` — build a new event, or changes to an
  existing one; returns the draft, a before/after diff and a
  `draft_id`. Touches nothing, so the agent iterates for free.
- `enclave_calendar_commit` — apply a draft. The daemon renders that
  diff as a preview card (times, attendees, the invite that goes out)
  and waits for a click.
- `enclave_calendar_respond` — RSVP yes/no/maybe; it mails the
  organizer, so it confirms.
- `enclave_calendar_cancel` — cancel or delete. Always confirms.
- `enclave_calendar_join` — open an event's call link on this
  computer: a one-line confirm, and the one action worth an opt-in
  allowlist.

Rules baked into the tool descriptions: every event carries a short
stable `ref` the acting tools take, so nothing is addressed by fuzzy
title; times come back local *and* ISO with the home timezone stated
once; relative dates resolve in the daemon, not the model; errors are
instructions ("`@ale` is ambiguous: ale@…, alessandro@…"). Read-only
ships first; creation, when it lands, takes exactly this shape.

### UI

One window, grido's discipline, calm by construction.

- **Ribbon** (grido's tabs, labelled groups, painter-drawn icons):
  Home (Today, back/forward, Day/Week, New event, Join), View (week
  start, working hours, show declined, zoom), Calendars (visibility,
  colors, sync now, connect), Help (F1 shortcut viewer).
- **Week view (default)** — seven columns over an hour grid, now-line,
  all-day band pinned above, events as rounded blocks tinted from the
  calendar color mixed toward the theme surface, drag to move or
  resize; egui painter, like grido's grid. **Day view** — one wide
  column plus a detail rail: attendees with RSVP dots, location, a
  big Join button, description, attachments. **Left rail** — mini
  month, calendar checkboxes, "next up" card. **Status bar** — sync
  age, timezone, next reminder.
- **Auxiliary surfaces** — event editor; people picker with
  `@`-autocomplete over the enclave roster (Commons' mention widget);
  free/busy overlay shading attendees' busy blocks behind the week
  grid; go-to-date; search; the daemon's confirm previews; the
  reminder toast (Join / Snooze / Open); backstage (provider, lead
  time, working hours).
- **Commands + keymap** — everything bindable (`today`, `next_week`,
  `view_day`, `new_event`, `join_call`, `rsvp_yes`, `refresh`,
  `shortcut_help`, …) in `keymap.toml`, grido's lookup order, F1.
- **Theming and type** — grido's `theme.rs` unchanged: Omarchy
  `colors.toml`, every surface derived from bg/fg/accent, live theme
  switches, light themes real. One type scale, tabular figures for
  times, generous gutters, and empty states that say something:
  "Nothing until Thursday 09:00 — design review".

### Interlinks

- **enclaved** — socket, notifications, identity: `@person` resolves
  through the daemon's roster, so "invite @ale and @fredrik" becomes
  real addresses. Almanac has no notifier of its own.
- **Commons** — internal meetings carry a Commons link, not Zoom;
  "join my next call" opens the Commons call window, external links
  open the browser, and a meeting can announce itself in its channel.
- **Post** — invitations arrive as mail and Post hands the .ics to
  `enclave_calendar_draft`, so RSVP never leaves the agent chat;
  agendas and minutes go back out through Post. "Today's summary" is
  Almanac's overview plus Post's inbox overview, one answer.
- **Scribe / Ledger / Podium** — a prep doc bound to an event, linked
  from its card; event attachments open natively — the deck you
  present, the model you review.
- **Depot / Vault** — an attachment in a colleague's share opens
  read-only from their machine, nothing copied to a server; a meeting
  needing a shared credential links a Vault entry *by name*, never
  the secret itself.
- **Bursar / Mobile** — filing deadlines (VAT, annual figures) land
  as events on the same notifier; the phone shows the same agenda and
  gets the same reminders, and joining there opens Commons mobile.

### Non-goals

Not a calendar server: no hosting, no CalDAV service, no publishing
free/busy outward. No booking pages (Calendly-style), no room and
resource booking, no org-wide calendar administration. No task list —
a calendar shows time, not a backlog. No mail duties (Post does
mail). No transcription or meeting notes. No recurrence editor beyond
what the provider's rules express: exotic recurrences are shown and
respected, edited upstream. No co-editing, no cloud sync of our own.
KISS.

## Open

- Event creation/editing in v1 or read-only first.
- CalDAV fallback for companies on neither Google nor Microsoft.
