# Enclave — homepage storyline

The homepage tells this story top to bottom. Big text is for everyone;
each *details* block is the smaller print for readers who want the
technical truth behind the claim.

---

## Hero

**Your company's own private network.**
Send files straight to a colleague's computer — like AirDrop, but it works
from anywhere.

*Get Enclave → paste your invite code → done.*

---

## 1. What Enclave is

A private network just for your company — the whole team, not a pair.
Every computer in it is connected to every other one, directly. When you
send a file, it travels on one straight line between the two computers
involved; everyone else on the team is one line away too. Nothing else on
the internet can reach any of them at all.

*Visual note: show the whole team — several computers, each connected to
each, with one line highlighted for a transfer. Two lone computers reads
as a pairing app, which this is not.*

> *Details: each company gets its own encrypted mesh network (WireGuard).
> Every machine holds its own keys and talks directly to the others.
> Machines outside your enclave can't connect, probe, or even see yours.*

## 2. Drop files like AirDrop — from anywhere

Pick a colleague's computer, pick a file, done. Office, home, hotel wifi —
distance doesn't matter. No size limits. The other side accepts, and the
file lands in their Enclave folder.

> *Details: the file travels through one end-to-end encrypted tunnel from
> your machine to theirs. If a direct connection is impossible, an
> encrypted relay forwards packets it cannot read.*

## 2b. Sends that wait — and tell you they landed

Their laptop is closed? Send anyway. The file waits on your computer and
delivers itself the moment they come online — and you see "received 14:02"
when it does. No more "did you get it?".

> *Details: queued transfers stay on the sender's machine, encrypted like
> everything else; nothing is parked on a server in between. Receipts come
> from the receiving computer itself.*

## 3. Private, because there is no middle

Your files never sit in a cloud. There's no server-side inbox, no shared
drive, no copy for anyone to look into. One computer, one tunnel, another
computer — that's the whole path.

> *Details: our control server only handles identity and key exchange —
> who belongs to which enclave. File contents never pass through it and
> couldn't be decrypted if they did.*

## 4. Your computer stays yours

Enclave adds a private connection to your colleagues. It changes nothing
else: your browsing, your apps, your other software all work exactly as
before, and none of that traffic goes anywhere near the company.

> *Details: only traffic addressed to enclave machines (their private
> 100.x addresses) uses the enclave. There is no exit node, no traffic
> inspection, no VPN-style rerouting of your internet.*

## 5. Admins see the roster, never your data

The person running your enclave sees which computers belong to it, who
they belong to, and whether they're online. That's all. Not your files,
not your traffic, not your other activity.

## 5b. Nothing joins without a yes

Every new computer is approved by your admin before it's in — and the
enclave keeps a plain log of who was added, approved and removed, and
when. You always know exactly what your enclave is made of.

## 6. Joining is one paste; leaving is instant

Your admin sends you a link. You paste the code once, and your computer is
in — and stays in, across restarts, until the admin removes it. When
someone leaves the company, one click disconnects all their machines
immediately.

## 7. One computer, many enclaves

Work and home. Two clients, one contractor machine. A computer can be part
of several enclaves at once, each fully isolated from the others.

## 8. Set up your company's enclave in minutes

For the person running it: sign in, name your enclave, add your people by
email. Each person gets a link from you — one paste and their computer is
in. No servers to run, no network knowledge needed.

*Three steps: create → send links → done.*
**[Create your enclave]** — the page's primary call to action, leading to
sign-in.

## 9. Price

**Free while in beta.**

---

## 9b. Boring in the right ways

Signed installers on every platform. Enclave keeps itself up to date, so
security fixes reach every computer without anyone doing anything.

## Closing

**Enclave. The private network your company always assumed it had.**
Download for macOS, Windows, Linux.
Footer: privacy policy · terms.
