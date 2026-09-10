# Plan: Enclave Bursar

Accounting for the small company, basic by design: bookkeeping,
invoices, expenses, and producing/sending the recurring reports.

## Settled (from the suite plan)

- **Basic on purpose**: what a 2–20-person firm actually files —
  ledger entries, invoices in/out, expenses, VAT + annual figures.
  Not an ERP.
- **Agent drafts, human approves, Post sends**: report generation and
  submission run through MCP tools with the daemon's confirm dialog
  before anything leaves.
- **Manual UI** in Rust (egui) from the tray / app search stub;
  data lives on the company's machines like everything else.

## Starter roadmap

1. Data model: accounts, entries, invoices, expenses — SQLite on the
   owner's machine, in the Enclave state dir (and in offsite backup).
2. Invoicing: create, number, PDF, send via Post; mark paid.
3. Expenses: capture (drop a receipt file on it), categorize.
4. Reports: VAT summary and yearly figures as drafts the human
   approves; export in the format the local tax authority takes
   (Sweden/Skatteverket first).
5. MCP tools: record entry, create invoice, expense from file,
   draft report.

## High-level approach

The daemon owns the books, the window is a view, and the agent drafts
what a human approves in front of the real document. Sweden first: BAS
accounts, gapless invoice series, VAT periods, SIE export.

### Architecture

**Go daemon module (`bursar/` in enclaved) — the books.** SQLite in
the enclave state dir (`bursar/books.db`, covered by offsite backup),
and the only writer: chart of accounts (BAS), journal entries,
invoices, expenses, the original receipt files (content-hashed, never
mutated), report drafts, series counters, fiscal-year config. It
renders the invoice and report PDFs, watches the receipt inboxes, and
serves every op over the daemon socket. It lives there and not in the
UI because receipts arrive while no window is open, filing deadlines
fire as notifications like meeting reminders, and the MCP surface goes
through that socket anyway.

**Rust egui UI ("Enclave Bursar") — a client, nothing more.** grido's
split with one layer renamed: `client.rs` (the only module that speaks
to the daemon — plain typed requests and rows, grido's `engine.rs`
role), `state.rs` (`BursarApp`: cached view, selection, drafts),
`app.rs` (update loop, input, command dispatch), `commands.rs` (the
registry every binding and button targets), `keymap.rs`, and `ui/`
(`dashboard`, `invoices`, `expenses`, `books`, `reports`, `ribbon`,
`preview`, `dialogs`, `icons`).

`ui/` never opens a socket, never sees JSON and never does arithmetic
on money — it asks `client` for typed rows and paints them. Nothing it
holds is authoritative; a restart loses scroll position, no more.

**One op set, two callers.** UI and agent issue the same daemon ops, so
there is one code path, one validation, one audit trail. The daemon
pushes change events back, so an open window shows the agent's work as
it lands, tinted for a few seconds the way grido does.

**Corrections, not deletions.** Nothing booked is edited or removed; a
mistake is a reversing entry with a reason. Swedish law requires it,
and it makes an agent's mistakes cheap and visible.

### MCP surface

Served through the one `enclave-mcp`. Reading and drafting are free — a
draft is a local row plus a rendered file, nothing leaves the machine.
Anything that leaves, or moves the books, goes through the daemon's
native confirm dialog, and none of those are allowlistable.

Read / draft:

- `enclave_books_overview` — the entry point: period figures, VAT owed,
  unpaid invoices, waiting receipts, next deadline. One compact answer.
- `enclave_books_inbox` — receipts that arrived (drop or Post
  attachment) and aren't booked, with vendor/amount/date guesses.
- `enclave_books_search` — invoices, expenses, entries by counterparty,
  amount, date or text; capped rows, each carrying an id.
- `enclave_books_item` — one item in full: lines, VAT, attachments.
- `enclave_books_accounts` — BAS accounts with period balances, so the
  agent picks a real one instead of inventing it.
- `enclave_invoice_draft` — compose or amend a draft; returns the draft
  *and* the path of the rendered PDF.
- `enclave_books_report_draft` — VAT period or annual figures: the
  filled boxes plus what feeds each one.

Acting (confirm dialog, showing that same preview):

- `enclave_books_post_entry` — post a balanced journal entry.
- `enclave_expense_record` — book a receipt from the inbox; the dialog
  shows it beside the proposed posting.
- `enclave_invoice_issue` — next number in the series, PDF frozen.
- `enclave_invoice_send` — hand the issued PDF to Post; one confirm
  covers issue+send when the ask was "send it".
- `enclave_invoice_mark_paid` — record a payment.
- `enclave_books_report_file` — export or submit the approved report in
  Skatteverket's format.
- `enclave_books_export` — SIE 4 (or xlsx) for the accountant, to a
  path or to an `@person`.

Rules baked into the tool descriptions: start at the overview; pass ids
back verbatim (`INV-2026-014`, `EXP-1183`) — there are no coordinates
to guess; amounts are integer minor units plus a currency, never
floats; refusals say what to do instead (unbalanced entry, closed
period, unknown account, duplicate receipt hash); one tool call is one
journal transaction and one audit line.

### UI

One window with grido's furniture: a ribbon (Home, Invoices, Expenses,
Books, Reports, Help), a body per tab, a status bar carrying period,
VAT owed and waiting receipts. Every action is a command id;
`keymap.toml` binds chords with grido's lookup order and F1 lists them.

- **Home** — the calm landing: this period in a few large figures, next
  deadline, unpaid invoices, the receipt inbox. Nothing else.
- **Invoices** — list (number, customer, dates, amount, state) with a
  detail pane; drafting is a form, not a grid.
- **Expenses** — waiting receipts on top, booked below, the receipt
  image beside the posting form.
- **Books** — the journal, filterable by account and period;
  read-mostly, corrections raised from here.
- **Reports** — VAT and annual drafts, each opened for review.

Auxiliary surfaces: the **invoice preview** (the real rendered page,
with recipient and subject when it is a send); the **report preview**
(the boxes as they will be filed, each figure drilling down to the
entries behind it); the daemon's **confirm dialogs**, embedding those
renders rather than describing them; **pickers** for account (BAS
search-as-you-type), customer, period and `@person`; an **activity
log** in plain language; **empty states** that instruct ("No receipts
waiting — drop one on Bursar").

Looks: grido's theme module unchanged — every surface derived from the
Omarchy/system background, foreground and accent, live switches, light
themes included, neutral dark elsewhere. Money in tabular figures,
right-aligned, minor units always shown. Generous spacing, one accent,
red reserved for overdue and unbalanced.

### Interlinks

- **Post → Bursar**: supplier invoices and receipts arrive as email
  attachments and Post hands them to the inbox. Bursar never reads mail.
- **Bursar → Post**: the issued PDF goes out as an attachment, overdue
  reminders the same way — Post's send confirm, Bursar's preview inside.
- **enclaved**: receipts dropped from a phone or a colleague land in
  `~/Enclaved/<computer>/` and show up in the inbox; "send the SIE file
  to @cristiano" is one confirm over the drop.
- **Almanac**: VAT and annual deadlines are Bursar dates published to
  Almanac, so "today's summary" says the declaration is due Thursday;
  the reminder itself fires from enclaved.
- **Depot**: the year's exports sit in the owner's share, so the
  accountant browses and fetches read-only — no second set of books.
- **Ledger**: "the year in a spreadsheet" exports trial balance and
  entries as .xlsx and opens them in Ledger.
- **Scribe**: the annual report's prose (förvaltningsberättelse) is a
  .docx opened with the approved figures already filled in.
- **Podium**: quarterly figures land in a deck on request.
- **Commons**: "ask @ale about this expense" opens a thread with the
  receipt attached and a link back to the item.
- **Vault**: bank, Skatteverket ombud and provider credentials live
  there, never in `books.db`.
- **Mobile**: photograph a receipt and it drops onto the owner's
  machine; figures are readable on the phone, approvals are not — those
  happen where the daemon's dialog is.
- **@-addressing**: colleagues are `@person`; customers and suppliers
  sit outside the enclave, so they stay email addresses through Post.

### Non-goals

No payroll, inventory, projects, time tracking, purchase orders,
multi-currency revaluation (a foreign invoice is booked at the rate
used), consolidation, audit tooling or tax advice. No bank connection
in v1. No second set of books: one owner machine writes, everyone else
reads exports — no co-editing, no cloud copy, no web UI. No
auto-filing: nothing reaches a customer or Skatteverket without a human
looking at the actual document. Beyond Sweden, and beyond a small K2
aktiebolag, is a later question, not a v1 shape. KISS.

## Open

- Country scope for report formats beyond Sweden.
- Bank feed import (CSV first? PSD2 APIs much later).
- Who in the enclave sees the books (owner-only vs a bookkeeper
  role).
