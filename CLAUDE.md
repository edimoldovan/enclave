# Enclave

Private tailnet-per-company networking, live at https://enclave.works.

An **enclave** is one company's own private network (never "Enclave
Network"). Every computer in it is connected directly to every other one;
nothing else on the internet can reach them. **Computers appear on
enclaves, never apps.** The flagship experience: drop a file onto a
colleague's computer like AirDrop, from anywhere, with no cloud in the
middle — end-to-end encrypted, machine to machine, no size limits, no
server-side copy.

How it fits together: an admin signs in with OAuth at enclave.works,
creates an enclave and adds people by email; the server hands them a
single-use invite link that the admin delivers personally (the server
sends no email). The person pastes the code into the Enclave tray app
once, and their computer is in — across restarts, until the admin removes
it, which takes effect within seconds. One computer can belong to many
enclaves at once, each fully isolated from the others.

- [PLAN.md](PLAN.md) — plan of record: settled decisions and the roadmap.
- [STORYLINE.md](STORYLINE.md) — the product narrative and
  homepage story.
- [design-prompt.md](design-prompt.md) — the design-system brief for
  Claude Design rounds.
- `server/` — the control plane; `enclaved/` — the client. Each has its
  own CLAUDE.md: read it before working there.

Whether other products (Grido, Viode, …) ever integrate with enclaves is
an open question — do not build for it.
