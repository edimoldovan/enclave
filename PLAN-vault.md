# Plan: Enclave Vault

Team passwords and shared secrets for the enclave, end-to-end
encrypted, replicated to every member's machine.

## Settled (from the suite plan)

- **Fully replicated** (unlike Depot): secrets are tiny and must be
  at hand offline and survive any one machine.
- **Enclave membership grants access** — the admin removing a member
  from the enclave is the revocation story (plus rotating what they
  knew).
- **E2E encrypted**: the server never holds keys or plaintext;
  replication rides the tailnet.
- **Manual UI** in Rust from the tray / app search stub; MCP tools
  through the daemon socket with the confirm dialog guarding reads
  of secret values (reading a password is an acting operation).

## Starter roadmap

1. Secret model: entries (login, password, URL, note), per-enclave
   vaults, local encrypted store.
2. Replication + conflict handling between members' machines
   (last-writer-wins per entry with history).
3. Key model: how enclave membership maps to vault keys, and what
   re-keying on member removal looks like — design doc first.
4. UI: list, search, copy-to-clipboard (auto-clearing), add/edit.
5. MCP tools: list entries (names only), fetch secret (confirm
   dialog), create entry.

## High-level approach

The daemon owns the crypto and hands a secret to nobody who did not
just click a dialog; the window is a search box; the agent is taught
to *use* secrets rather than read them. Enclave membership is the
account — nothing to sign up for, and nothing for us to reset.

### Architecture

**Go daemon module (`enclaved/internal/vault/`) — the engine.** It
holds the per-enclave encrypted store, the keys, the replication and
the unlock state, and it is the only code that ever sees plaintext.
Files sit in the enclave state dir (`vault/<enclave>/`); offsite
backup covers them as ciphertext, because our server never holds a
key.

**Crypto off the shelf, none invented.** `age` primitives (X25519 +
ChaCha20-Poly1305). Each machine generates an identity keypair when
it joins and publishes only the *public* half with its machine
record; the private half lives in the OS keychain (Keychain /
libsecret / Credential Manager), never in a file we write. A vault
has one symmetric key, sealed once per member machine into a keybox
beside the store; entries are sealed with that key.

**Membership is the key model.** A new member's first machine gets
the vault re-sealed to its public key by a machine that already has
it — joining needs one colleague online, once, and the UI says so
rather than failing silently; a second machine of the same member is
the same hand-off. A removed member loses their keyboxes, the vault
key rotates and entries re-seal — but rotation cannot change the
bank's password, so everything they could read is flagged **needs
changing** until a human changes it upstream. Saying that plainly is
the design.

**Replication: full, append-only, gossip.** Every machine holds the
whole vault — secrets are tiny, and offline is exactly when you need
them. Versions append with an HLC stamp; last-writer-wins per entry,
history kept, deletes are tombstones; a machine that was off catches
up from any peer that is on, as Chat does. No server copy.

**Unlock is daemon state**, not a UI mode: unwrapped at login
(keychain, biometrics where they exist) or by passphrase, auto-locked
on idle and on sleep. UI and agent read the same lock state, so a
locked vault refuses with a sentence instead of an empty list.

**Rust egui UI ("Enclave Vault") — a client, nothing more.** grido's
split with one layer renamed: `client.rs` (the only module that opens
the socket — typed `Vault`, `Entry`, `Field`, `Grant`; no JSON, no
crypto, and no plaintext held longer than a paint), `state.rs`,
`app.rs` (update loop + dispatch), `commands.rs` (the registry every
binding and button targets), `keymap.rs`, and `ui/{rail,list,detail,
ribbon,dialogs,generator,icons}`, each adding one `impl VaultApp`
block. `ui/` never opens a socket and never decrypts, exactly as
grido's UI never imports IronCalc. Nothing it holds is authoritative;
closing the window loses a search string.

### MCP surface

Served through the one `enclave-mcp`. Vault inverts the usual split —
**reading a value is an acting operation** — and its best tools let
the agent finish the job without ever holding the secret.

Read (free; none of them returns a secret):

- `enclave_vault_overview` — the entry point: vaults, entry counts,
  lock state, and what needs attention (flagged by a removal, reused,
  ancient). A handful of lines.
- `enclave_vault_search` — by name, url, username or tag; capped
  rows, each carrying a stable id.
- `enclave_vault_entry` — one entry's shape: fields present, which
  are secret, username, url, tags, who changed it when. Values are
  described (`password: 24 chars, set 2026-04-11`), never printed.
- `enclave_vault_access` — who can decrypt this vault, on which
  machines. Access is legible *before* a secret goes in.

Acting (daemon confirm dialog; none allowlistable except where
noted):

- `enclave_vault_use` — **the tool the agent should reach for**: put
  the secret where the work is — the focused field, the clipboard
  (auto-clearing), or an env var for one named command — without it
  entering the model's context. The dialog names entry, destination
  and requester.
- `enclave_vault_reveal` — hand the value to the agent, when nothing
  else will do. The dialog's primary button is *Copy instead*; the
  secondary gives it up. Audited and rate-limited.
- `enclave_vault_code` — the current TOTP code. Thirty seconds of
  exposure, so this is the one worth an opt-in allowlist.
- `enclave_vault_generate` — a password or passphrase to a policy,
  returned as a **handle, not a string**: the agent can create a
  strong credential it has never read.
- `enclave_vault_draft` — compose an entry or changes to one; returns
  a diff with secret fields as *set / unchanged / generated*. Writes
  nothing, so iterating is free.
- `enclave_vault_apply` — commit a draft; the dialog shows that diff.
- `enclave_vault_grant` — put an entry in another vault, or give a
  vault to `@person`; the dialog spells out who gains access.
- `enclave_vault_rotate` — record a changed credential and clear the
  needs-changing flag; or re-key a vault.
- `enclave_vault_delete` — tombstone with history; always confirms.

Rules baked into the descriptions: never write a secret into a file,
a message, an email or your own reply — use `enclave_vault_use`; pass
ids back verbatim; a locked vault is a state, not an error ("ask the
human to unlock Vault"); refusals say what to do instead. A shipped
skill teaches the habit, condensed into `initialize` instructions for
clients without skills.

### UI

Search first: what people do is find one entry and copy one field, so
that is one keystroke deep.

- **Quick find** (a global chord, and the tray's Vault item): type,
  Enter copies the password and closes, Shift+Enter the username.
  Most sessions end here without the window ever settling.
- **Main window** — left rail of vaults and tags, a filtered list
  (name, username, url, updated) painting only visible rows, a detail
  pane. grido's ribbon, lightly loaded: Home (new, edit, copy,
  reveal, generate, delete), Vault (create, members, re-key, lock
  now, import), View (sort, tags, needs attention), Help. Every
  action is a command id; `keymap.toml` binds them with grido's
  lookup order and F1 lists what is bound.
- **Detail** — fields masked as dots with per-field Copy and Reveal
  (reveal times out and re-masks), the TOTP code with its countdown
  ring, notes, history, and one plain line: *"Everyone in Acme can
  read this."*
- **Status bar** — lock state, last sync, machines carrying this
  vault.

Auxiliary surfaces: the **lock screen** (nothing shown until
unlocked — no teasing list behind a blur); the daemon's **confirm
dialogs**, Copy-instead as the primary button; the **generator**
popover; the **access sheet** (who can decrypt, what a re-key would
do, which entries a removal flagged); the **@-picker** for grants;
the **history** drawer; the **import** wizard (1Password / Bitwarden
/ CSV, once, then it offers to shred the file); an **audit log** in
plain language ("Claude asked for 'Stripe live key' at 14:02 — you
approved"); and a **waiting state** for the bootstrap case: "Ana has
this vault; it arrives when her machine is online."

Looks: grido's `theme.rs` unchanged — every surface derived from the
Omarchy/system background, foreground and accent, live switches,
light themes real, neutral dark elsewhere. Calm typography, generous
spacing, one accent, red reserved for *needs changing*. Empty states
instruct ("Nothing here yet — put the wifi password in, then the bank
login"). Revealed values render monospaced with slashed zeros — a
password you cannot read is a support ticket — re-mask when the
window loses focus, and the window opts out of screen capture where
the platform allows.

### Interlinks

- **enclaved** — the spine: identity, roster, tailnet transport,
  notifications, the confirm dialogs. Vault is a daemon module, not a
  service, and `@person` / `@person/computer` address a grant exactly
  as they address a file drop.
- **Post** — the mail bridge's app password or refresh token lives
  here, not in a config file. "Email @ale the wifi password" is
  refused and turned into a grant: the reference travels, the value
  never does.
- **Chat** — "share from Vault" in the composer posts an entry
  *reference*; clicking it asks Vault, not the channel. Secrets stop
  being pasted into chat, which is the actual disease.
- **Almanac** — a meeting needing a shared credential links the entry
  by name; joining does not reveal it.
- **Scribe / Ledger / Podium** — a protected .docx or .xlsx takes its
  passphrase through `enclave_vault_use`, never through the agent's
  context.
- **Bursar** — bank, Skatteverket ombud and provider logins live
  here; Bursar asks by entry id at the moment it needs one and keeps
  nothing in `books.db`.
- **Depot** — the vault never travels as files: a share is for
  documents, and a `.env` sitting in one is a nudge to move it in
  here.
- **Mobile** — the whole vault replicates to the phone (tiny data),
  reveals sit behind Face ID, and the phone is the approval surface
  when the desk is empty: `enclave_ask_phone` carries the reveal
  *request*, while the value stays on the machine that will use it.
- **The agent layer** — one skill, one habit: use, don't read.

### Non-goals

Not an infrastructure secrets manager: no dynamic secrets, leases,
PKI, machine identities or CI integration. No SSO, no identity
provider, no OAuth broker. No sharing outside the enclave — no
send-a-link-to-a-customer, no guest accounts; membership is the whole
access model, and per-entry ACLs are not v1 (the vault is the unit,
as the share is Depot's). No admin override and no escrow: nobody
reads a vault they are not in, us least of all, and there is no
recovery we could offer if every member's machine burned — full
replication plus the paid offsite backup is the answer. No custom
crypto. No breach-database lookups in v1 (they leave the enclave), no
strength theatre beyond reused, weak and old. No browser extension in
v1 — the one omission we may regret, and the reason
`enclave_vault_use` can type into the focused field. No web vault, no
cloud copy, no hosted anything. KISS.

## Open

- Browser autofill integration (big value, big surface — later?).
- Personal (non-shared) vaults in the same app.
- Whether history/versions of a secret are kept forever.
