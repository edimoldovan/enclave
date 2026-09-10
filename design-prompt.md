# Design brief: Enclave

You are designing the visual system and the two most important pages for
Enclave. Work inside the existing CSS architecture — it is parametric, so
the design system is delivered as new *values*, not new structure.

## The product

Read `STORYLINE.md` first — it is the homepage's entire
narrative and tone. One line: Enclave gives a company its own private
network; employees drop files straight onto each other's computers like
AirDrop, from anywhere, with no cloud in the middle.

Audience: average computer users at small companies. The admin dashboard is
used by slightly more technical owners, but must feel like the same product.

## Brand direction

Private, calm, solid, quietly premium. Feels like a Swiss bank vault run
by people who like Linux. Monochrome warm greys, one deep green accent
(the tray-icon green, #1d9e75 as a starting point — translate into the
oklch seed system). Generous whitespace, serif display face.

Distinctive idea: the homepage visualizes "no middle" — the whole team's
computers, each connected to each, with one direct line highlighted for a
transfer. Nothing between any pair. (Not just two lone computers — that
reads as a pairing app.)

## The CSS architecture you must design within

The system lives in `server/internal/static/assets/css/` — read all eight
sheets before designing: `reset.css`, `variables.css`, `fonts.css`,
`colors.css`, `type-scales.css`, `space.css`, `grids.css`, `elements.css`.

It is parametric: `variables.css` holds the seeds — neutral hue/chroma,
four color seed chromas, hue tiers, type scale ratios (`--scale-*`),
fluid viewport range, `--border-radius`. `colors.css` derives the full
light and dark palettes (`--color-neutral-1..10`, `--color-color-1..12`,
oklch) from those seeds; `type-scales.css` and `space.css` derive fluid
type and spacing. Dark mode is automatic via one `prefers-color-scheme`
block in `colors.css`.

Hard rules (enforced by tests in the source repo):

- Change **values only** — never variable names, never new variables.
- Components use tokens only: `--color-neutral-*`, `--color-color-*`,
  `--space-*`, `--font-size-*`, `--border-radius`. No raw hex/px in
  component CSS.
- No media queries anywhere except the color-scheme block in `colors.css`.
  Responsiveness = the fluid tokens + `repeat(auto-fit, minmax(…, 1fr))`.
- Page layout: one `<main class="grid">`; **every immediate child of it is
  `<section class="<name> grid full">`** — exactly those classes. The
  mechanics matter: `.grid .full` (two classes) beats `.grid > *` (one),
  which is what lets the section bleed to the viewport; and because the
  section is itself a `.grid`, its own children land back in the standard
  reading column automatically, with no classes. Content inside a section
  escapes the column only via `.wide-1`, `.wide-2`, or `.full` on that
  child. **Never nest a second `class="grid full"` inside a section.**
  Every section carries its own fluid block padding (a `--space-*` pair)
  in its own stylesheet — no blanket `main.grid > section` rules.
- Typography comes from `elements.css` on semantic elements; components
  override only with a stated reason, always via tokens.
  `--font-family-0` is the display accent, `--font-family-1` the body.
- Buttons: `.primary`, `.secondary`, `.tertiary`, `.delete` only.
- Semantic HTML, CSS nesting with `>` children, no BEM, no utility soup,
  no inline styles, sentence case everywhere.

## Deliverables

1. **`variables.css`** — the design system: your hue/chroma seeds, scale
   ratio choices, radius. Same file, same names, new values.
2. **`fonts.css`** — your two typefaces (`@font-face` with hosted woff2 +
   the two family variables). Display face may have personality; body face
   must work at dashboard-table sizes.
3. **`homepage.html`** — standalone single file (inline all sheets as
   `<style>`, which is the accepted exception for standalone deliveries).
   Implements `STORYLINE.md` top to bottom: hero, the nine sections (including the admin call to action and pricing), the
   closing. This page is the brand.
4. **`dashboard.html`** — standalone single file, same system. Real
   content, not lorem: header (product name, signed-in email, sign out) ·
   a highlighted one-time invite-link box with a copy field · one or more
   enclave cards, each with a members table (email, role, computers,
   remove button), a pending-invites line, and an "add a person" email
   form · a "new enclave" card with a name form. It must look great with
   one enclave of three people — that is the common case.

Both pages must be excellent in light and dark mode — check both.

## Header and auth entry (follow this exactly)

Modeled on nomi.family. **There is no login page.** The homepage header
carries **two auth buttons**, so nobody hunts for either entry point:

- "Sign in" — quiet (tertiary/link style)
- "Create your enclave" — the primary button

**Both anchor to the same auth section on the homepage** (`/#signup`):
three buttons — "Continue with Google" (`/auth/google`), "Continue with
Microsoft" (`/auth/microsoft`), "Continue with Yahoo" (`/auth/yahoo`) —
no Apple, no others — plus one reassurance line ("Signing in creates
your account if you don't have one. We only email you about Enclave.").
OAuth is both sign-in and sign-up — first click creates the account; no
passwords, no separate registration. The section 8 "Create your enclave"
CTA points to the same anchor.

Provider logos exist as assets — use them inside the continue buttons:
`/assets/img/google.svg` (official multicolor G) and
`/assets/img/microsoft.svg` (four squares), both keeping their brand
colors, roughly 1.25em before the label. Yahoo is text-only, no logo.

## Update round 2 (current task)

The first design round is shipped — the repo's pages and stylesheets ARE
the current design (read them; the appendix at the bottom of this file is
now stale, ignore it). This round only extends the homepage; the dashboard
is untouched.

`STORYLINE.md` gained sections — design them in its order:

- **2b "Sends that wait"** — offline queueing + delivery receipts. Sales
  weight: this is the beats-AirDrop moment; give it presence, possibly a
  small visual (a send waiting, then a receipt).
- **5b "Nothing joins without a yes"** — admin approval of every new
  computer + the audit log. Trust section, quiet and solid.
- **9b "Boring in the right ways"** — signed installers, auto-update. One
  short reassurance band, not a hero.
- Closing gains a footer line: privacy policy · terms (link targets
  /privacy and /terms, pages exist later).

No mobile anywhere. Match the shipped design's voice and rhythm; deliver
the same way as round 1 (updated homepage-body.html + homepage.css).

### Also in this round: the two legal pages

Deliver `privacy-body.html` + `privacy.css` and `terms-body.html` +
`terms.css` (sections `.privacy` / `.terms`, same architecture rules, a
quiet header with the wordmark linking home). They are calm reading
pages — the design should make them feel as honest as the homepage
claims: clear headings, comfortable measure, nothing dressed up. The
content below is factual and fixed — you may tune rhythm and microcopy,
never the claims. Placeholders in [brackets] stay as placeholders.

**Privacy policy — content:**

- What our server stores: your email address and name (from the sign-in
  provider you choose), which enclaves you belong to and your role, your
  computers' names, their enclave addresses, and whether they are online.
  Invite links and machine sessions are stored only as hashes. That is
  the complete list.
- What we never see: your files, their names, or their contents; your
  traffic; anything else on your computer. Transfers are encrypted end to
  end between the two computers involved. When a transfer has to be
  relayed, the relay forwards encrypted packets it cannot read.
- Sign-in: through Google, Microsoft or Yahoo. We receive your name and
  email from them; we never see a password. One cookie keeps you signed
  in to the dashboard. No analytics, no trackers, no third-party cookies.
- Email: our server sends none. Invites reach you from your admin
  personally.
- Where: our server runs in the EU (Hetzner, Germany). [Backups are
  encrypted and stored with an EU object-storage provider.]
- Retention: invite links delete themselves when used or after 7 days.
  When an admin removes a member, that member's computers are removed
  from the enclave immediately. Write to [contact email] to have an
  account deleted entirely.
- Enclave is in beta; this policy will grow more formal. Questions:
  [contact email]. Last updated: [date].

**Terms of service — content:**

- Enclave is beta software, free while in beta, provided as is, without
  warranty of any kind. It may change, break, or be unavailable.
- You may use Enclave to connect computers and people you have the right
  to connect. Don't use it for anything illegal, and don't abuse the
  relay infrastructure.
- Admins choose who joins their enclave and are responsible for those
  choices. We may suspend accounts that abuse the service.
- To the extent the law allows, our liability is limited to what you paid
  us — during the beta, nothing.
- We may update these terms; the current version always lives on this
  page. Governing law: [jurisdiction]. Contact: [contact email]. Last
  updated: [date].

## What good looks like

Distinctive, not template-y. The homepage should feel like a product with
a point of view; the storyline's *details* blocks are the smaller technical
print and should be visually quieter than the claims they support. The
dashboard should feel calm and administrative — the invite link moment is
its one highlight. If a choice fights the architecture above, the
architecture wins.


---

# Appendix — everything you need, inline

There is **no homepage today** — it is net-new, built from `STORYLINE.md`.
The dashboard below is the current markup with sample data; you may
restructure its layout freely as long as the content, the forms, and the
architecture rules survive. Design for the common case: one enclave,
three people.

## Current dashboard markup (sample data)

```html
<main class="grid">
	<section class="dashboard grid full">
		<div>
			<header>
				<h1>Enclave</h1>
				<div>
					<span>ed@example.com</span>
					<form action="/logout" method="post">
						<button class="tertiary" type="submit">Sign out</button>
					</form>
				</div>
			</header>

			<article class="invite-link">
				<h3>Invite link for alice@acme.com</h3>
				<p>Copy it and send it to them yourself — it is shown only once, is single-use, and expires in 7 days.</p>
				<input type="text" readonly value="https://net.enclave.example.com/join/nUf2phFrZrQas4xHQRFIqA9sa8j7mCcT" onclick="this.select()">
			</article>

			<article class="company">
				<h2>Acme</h2>
				<table>
					<thead>
						<tr><th>Member</th><th>Role</th><th>Computers</th><th></th></tr>
					</thead>
					<tbody>
						<tr><td>ed@example.com</td><td>owner</td><td>2</td><td></td></tr>
						<tr><td>alice@acme.com</td><td>member</td><td>1</td>
							<td><form action="/memberships/remove" method="post"><button class="delete" type="submit">Remove</button></form></td></tr>
						<tr><td>bob@acme.com</td><td>member</td><td>0</td>
							<td><form action="/memberships/remove" method="post"><button class="delete" type="submit">Remove</button></form></td></tr>
					</tbody>
				</table>
				<p>Pending invites: <span class="pending">bob@acme.com (until 2026-09-12)</span></p>
				<form action="/invites" method="post">
					<input type="hidden" name="company_id" value="1">
					<label>Add a person or computer
						<input type="email" name="email" placeholder="colleague@company.com" required>
					</label>
					<button class="primary" type="submit">Create invite link</button>
				</form>
			</article>

			<article class="new-company">
				<h2>New enclave</h2>
				<form action="/companies" method="post">
					<label>Company name
						<input type="text" name="name" placeholder="Acme" required>
					</label>
					<button class="primary" type="submit">Create</button>
				</form>
			</article>
		</div>
	</section>
</main>
```

## Current CSS — all sheets

The first eight are the system (your deliverables replace the values in
`variables.css` and `fonts.css`; everything else derives). The last four
are the current per-page sheets, shown so you can see the component
conventions.

### reset.css

```css
*, *::before, *::after { box-sizing: border-box; }
* { line-height: calc(1em + 0.5rem); }
html { -moz-text-size-adjust: none; -webkit-text-size-adjust: none; text-size-adjust: none; }
body, h1, h2, h3, h4, h5, h6, p, figure, blockquote, dl, dd { margin: 0; }
ul[role='list'], ol[role='list'] { list-style: none; }
body { min-height: 100vh; }
h1, h2, h3, h4, h5, h6 { text-wrap: balance; }
img { max-width: 100%; display: block; }
img[alt=""], img:not([alt]) { border: 5px solid red; }
input, button, textarea, select { font: inherit; }
textarea:not([rows]) { min-height: 10em; }
:target { scroll-margin-block: 5ex; }
```

### variables.css

```css
:root {
  /* ---------- modular scale ratios (pick one for --min-scale / --max-scale below) ---------- */
  --scale-minor-second:     1.067;
  --scale-major-second:     1.125;
  --scale-minor-third:      1.2;
  --scale-major-third:      1.25;
  --scale-perfect-fourth:   1.333;
  --scale-augmented-fourth: 1.414;
  --scale-perfect-fifth:    1.5;
  --scale-golden:           1.618;

  /* ---------- raw values (single source of truth) ---------- */
  --border-radius: 1rem;
  --min-font-size: 16;       /* px — root font size at min viewport */
  --max-font-size: 18;       /* px — type uses this at max viewport */
  --max-font-size-large: 20; /* px — space uses this at max viewport */
  --min-viewport: 320;       /* px — start of fluid range */
  --max-viewport: 1440;      /* px — end of fluid range */
  --min-scale: var(--scale-major-second);
  --max-scale: var(--scale-minor-third);

  /* ---------- type scale (consumed by type-scales.css) ---------- */
  --type-min-font-size: var(--min-font-size);
  --type-max-font-size: var(--max-font-size);
  --type-min-viewport:  var(--min-viewport);
  --type-max-viewport:  var(--max-viewport);
  --type-min-scale:     var(--min-scale);
  --type-max-scale:     var(--max-scale);

  /* ---------- space scale (consumed by space.css) ---------- */
  --space: 1rem;
  --space-min-font-size: var(--min-font-size);
  --space-max-font-size: var(--max-font-size-large);
  --space-min-viewport:  var(--min-viewport);
  --space-max-viewport:  var(--max-viewport);

  /* t-shirt multipliers (space sizes are these * --space) */
  --m-3xs: 0.25;
  --m-2xs: 0.5;
  --m-xs:  0.75;
  --m-s:   1;
  --m-m:   1.5;
  --m-l:   2;
  --m-xl:  3;
  --m-2xl: 4;
  --m-3xl: 5;

  /* ---------- color palette (consumed by colors.css) ---------- */
  --neutral-hue: 220;
  --neutral-chroma: 0.01;
  --dark-chroma-mul: 0.8;

  /* four hue seeds; 12 color slots cycle through them (slot N uses seed N mod 4).
     each seed has: chroma, hue, base light-mode lightness, dark-mode lightness delta. */
  --seed-0-c: 0.28; --seed-0-h: 220; --seed-0-l: 45%; --seed-0-ld: 10%;
  --seed-1-c: 0.26; --seed-1-h: 195; --seed-1-l: 50%; --seed-1-ld:  0%;
  --seed-2-c: 0.27; --seed-2-h: 245; --seed-2-l: 52%; --seed-2-ld: -4%;
  --seed-3-c: 0.25; --seed-3-h:  30; --seed-3-l: 50%; --seed-3-ld:  0%;

  /* lightness bump applied per tier of 4 slots (slots 0-3, 4-7, 8-11) */
  --tier-0: 0%;
  --tier-1: 5%;
  --tier-2: 5%;

  /* neutral lightness scale (light mode uses 1→10, dark mode uses 10→1) */
  --n-1:  1%;  --n-2: 20%; --n-3: 30%; --n-4: 40%; --n-5:  50%;
  --n-6: 60%;  --n-7: 70%; --n-8: 85%; --n-9: 95%; --n-10: 99%;

  /* ---------- grid layout (consumed by grids.css) ---------- */
  --content-max-width: 80rem;
  --content-wide-1-width: 2rem;
  --content-wide-2-width: 4rem;
}
```

### fonts.css

```css
:root {
  --font-family-0: 'Madimi One', 'Rubik';
  --font-family-1: 'Rubik';
}

@font-face {
  font-family: 'Madimi One';
  font-style: normal;
  font-stretch: normal;
  font-weight: 400;
  src: url("https://fonts.gstatic.com/s/madimione/v1/2V0YKIEADpA8U6RygDnZZFEoBoHMd2U.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 300;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-WYi1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 300;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8sDE0U1dYPFkJ1O.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 400;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-B4i1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 400;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8tdE0U1dYPFkJ1O.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 500;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-NYi1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 500;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8tvE0U1dYPFkJ1O.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 600;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-2Y-1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 600;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8uDFEU1dYPFkJ1O.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 700;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-4I-1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 700;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8u6FEU1dYPFkJ1O.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 800;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-h4-1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 800;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8vdFEU1dYPFkJ1O.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: normal;
  font-stretch: normal;
  font-weight: 900;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWZBXyIfDnIV5PNhY1KTN7Z-Yh-ro-1VU80V4bVkA.woff2") format("woff2");
}

@font-face {
  font-family: 'Rubik';
  font-style: italic;
  font-stretch: normal;
  font-weight: 900;
  src: url("https://fonts.gstatic.com/s/rubik/v28/iJWbBXyIfDnIV7nEt3KSJbVDV49rz8v0FEU1dYPFkJ1O.woff2") format("woff2");
}
```

### colors.css

```css
:root {
  /* inputs come from variables.css.
     ---------- derived: light accents ---------- */
  /* L = seed-l + tier,  C = seed-c,  H = seed-h */
  --color-light-color-1:  oklch(calc(var(--seed-0-l) + var(--tier-0)) var(--seed-0-c) var(--seed-0-h));
  --color-light-color-2:  oklch(calc(var(--seed-1-l) + var(--tier-0)) var(--seed-1-c) var(--seed-1-h));
  --color-light-color-3:  oklch(calc(var(--seed-2-l) + var(--tier-0)) var(--seed-2-c) var(--seed-2-h));
  --color-light-color-4:  oklch(calc(var(--seed-3-l) + var(--tier-0)) var(--seed-3-c) var(--seed-3-h));
  --color-light-color-5:  oklch(calc(var(--seed-0-l) + var(--tier-1)) var(--seed-0-c) var(--seed-0-h));
  --color-light-color-6:  oklch(calc(var(--seed-1-l) + var(--tier-1)) var(--seed-1-c) var(--seed-1-h));
  --color-light-color-7:  oklch(calc(var(--seed-2-l) + var(--tier-1)) var(--seed-2-c) var(--seed-2-h));
  --color-light-color-8:  oklch(calc(var(--seed-3-l) + var(--tier-1)) var(--seed-3-c) var(--seed-3-h));
  --color-light-color-9:  oklch(calc(var(--seed-0-l) + var(--tier-2)) var(--seed-0-c) var(--seed-0-h));
  --color-light-color-10:  oklch(calc(var(--seed-1-l) + var(--tier-2)) var(--seed-1-c) var(--seed-1-h));
  --color-light-color-11: oklch(calc(var(--seed-2-l) + var(--tier-2)) var(--seed-2-c) var(--seed-2-h));
  --color-light-color-12: oklch(calc(var(--seed-3-l) + var(--tier-2)) var(--seed-3-c) var(--seed-3-h));

  /* ---------- derived: dark accents ---------- */
  /* L = seed-l + seed-ld + tier,  C = seed-c * dark-chroma-mul,  H = seed-h */
  --color-dark-color-1:  oklch(calc(var(--seed-0-l) + var(--seed-0-ld) + var(--tier-0)) calc(var(--seed-0-c) * var(--dark-chroma-mul)) var(--seed-0-h));
  --color-dark-color-2:  oklch(calc(var(--seed-1-l) + var(--seed-1-ld) + var(--tier-0)) calc(var(--seed-1-c) * var(--dark-chroma-mul)) var(--seed-1-h));
  --color-dark-color-3:  oklch(calc(var(--seed-2-l) + var(--seed-2-ld) + var(--tier-0)) calc(var(--seed-2-c) * var(--dark-chroma-mul)) var(--seed-2-h));
  --color-dark-color-4:  oklch(calc(var(--seed-3-l) + var(--seed-3-ld) + var(--tier-0)) calc(var(--seed-3-c) * var(--dark-chroma-mul)) var(--seed-3-h));
  --color-dark-color-5:  oklch(calc(var(--seed-0-l) + var(--seed-0-ld) + var(--tier-1)) calc(var(--seed-0-c) * var(--dark-chroma-mul)) var(--seed-0-h));
  --color-dark-color-6:  oklch(calc(var(--seed-1-l) + var(--seed-1-ld) + var(--tier-1)) calc(var(--seed-1-c) * var(--dark-chroma-mul)) var(--seed-1-h));
  --color-dark-color-7:  oklch(calc(var(--seed-2-l) + var(--seed-2-ld) + var(--tier-1)) calc(var(--seed-2-c) * var(--dark-chroma-mul)) var(--seed-2-h));
  --color-dark-color-8:  oklch(calc(var(--seed-3-l) + var(--seed-3-ld) + var(--tier-1)) calc(var(--seed-3-c) * var(--dark-chroma-mul)) var(--seed-3-h));
  --color-dark-color-9:  oklch(calc(var(--seed-0-l) + var(--seed-0-ld) + var(--tier-2)) calc(var(--seed-0-c) * var(--dark-chroma-mul)) var(--seed-0-h));
  --color-dark-color-10:  oklch(calc(var(--seed-1-l) + var(--seed-1-ld) + var(--tier-2)) calc(var(--seed-1-c) * var(--dark-chroma-mul)) var(--seed-1-h));
  --color-dark-color-11: oklch(calc(var(--seed-2-l) + var(--seed-2-ld) + var(--tier-2)) calc(var(--seed-2-c) * var(--dark-chroma-mul)) var(--seed-2-h));
  --color-dark-color-12: oklch(calc(var(--seed-3-l) + var(--seed-3-ld) + var(--tier-2)) calc(var(--seed-3-c) * var(--dark-chroma-mul)) var(--seed-3-h));

  /* ---------- derived: neutrals ---------- */
  --color-light-neutral-1:  oklch(var(--n-1)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-2:  oklch(var(--n-2)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-3:  oklch(var(--n-3)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-4:  oklch(var(--n-4)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-5:  oklch(var(--n-5)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-6:  oklch(var(--n-6)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-7:  oklch(var(--n-7)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-8:  oklch(var(--n-8)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-9:  oklch(var(--n-9)  var(--neutral-chroma) var(--neutral-hue));
  --color-light-neutral-10: oklch(var(--n-10) var(--neutral-chroma) var(--neutral-hue));

  --color-dark-neutral-1:  oklch(var(--n-10) var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-2:  oklch(var(--n-9)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-3:  oklch(var(--n-8)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-4:  oklch(var(--n-7)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-5:  oklch(var(--n-6)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-6:  oklch(var(--n-5)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-7:  oklch(var(--n-4)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-8:  oklch(var(--n-3)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-9:  oklch(var(--n-2)  var(--neutral-chroma) var(--neutral-hue));
  --color-dark-neutral-10: oklch(var(--n-1)  var(--neutral-chroma) var(--neutral-hue));

  --shadow-color-light: oklch(7% 0.1  var(--neutral-hue));
  --shadow-color-dark:  oklch(3% 0.03 var(--neutral-hue));
}

/* ---------- scheme mapping ---------- */

:root,
.light,
[color-scheme=light] {
  color-scheme: light;
  --color-color-1:  var(--color-light-color-1);
  --color-color-2:  var(--color-light-color-2);
  --color-color-3:  var(--color-light-color-3);
  --color-color-4:  var(--color-light-color-4);
  --color-color-5:  var(--color-light-color-5);
  --color-color-6:  var(--color-light-color-6);
  --color-color-7:  var(--color-light-color-7);
  --color-color-8:  var(--color-light-color-8);
  --color-color-9:  var(--color-light-color-9);
  --color-color-10:  var(--color-light-color-10);
  --color-color-11: var(--color-light-color-11);
  --color-color-12: var(--color-light-color-12);
  --color-neutral-1:  var(--color-light-neutral-1);
  --color-neutral-2:  var(--color-light-neutral-2);
  --color-neutral-3:  var(--color-light-neutral-3);
  --color-neutral-4:  var(--color-light-neutral-4);
  --color-neutral-5:  var(--color-light-neutral-5);
  --color-neutral-6:  var(--color-light-neutral-6);
  --color-neutral-7:  var(--color-light-neutral-7);
  --color-neutral-8:  var(--color-light-neutral-8);
  --color-neutral-9:  var(--color-light-neutral-9);
  --color-neutral-10: var(--color-light-neutral-10);
  --shadow-color: var(--shadow-color-light);
  --contrast-color: black;
  --contrast-color-mix-percent: 10%;
}

@media (prefers-color-scheme: dark) {
  :root {
    color-scheme: dark;
    --color-color-1:  var(--color-dark-color-1);
    --color-color-2:  var(--color-dark-color-2);
    --color-color-3:  var(--color-dark-color-3);
    --color-color-4:  var(--color-dark-color-4);
    --color-color-5:  var(--color-dark-color-5);
    --color-color-6:  var(--color-dark-color-6);
    --color-color-7:  var(--color-dark-color-7);
    --color-color-8:  var(--color-dark-color-8);
    --color-color-9:  var(--color-dark-color-9);
    --color-color-10:  var(--color-dark-color-10);
    --color-color-11: var(--color-dark-color-11);
    --color-color-12: var(--color-dark-color-12);
    --color-neutral-1:  var(--color-dark-neutral-1);
    --color-neutral-2:  var(--color-dark-neutral-2);
    --color-neutral-3:  var(--color-dark-neutral-3);
    --color-neutral-4:  var(--color-dark-neutral-4);
    --color-neutral-5:  var(--color-dark-neutral-5);
    --color-neutral-6:  var(--color-dark-neutral-6);
    --color-neutral-7:  var(--color-dark-neutral-7);
    --color-neutral-8:  var(--color-dark-neutral-8);
    --color-neutral-9:  var(--color-dark-neutral-9);
    --color-neutral-10: var(--color-dark-neutral-10);
    --shadow-color: var(--shadow-color-dark);
    --contrast-color: white;
    --contrast-color-mix-percent: 5%;
  }
}

.dark,
[color-scheme=dark] {
  color-scheme: dark;
  --color-color-1:  var(--color-dark-color-1);
  --color-color-2:  var(--color-dark-color-2);
  --color-color-3:  var(--color-dark-color-3);
  --color-color-4:  var(--color-dark-color-4);
  --color-color-5:  var(--color-dark-color-5);
  --color-color-6:  var(--color-dark-color-6);
  --color-color-7:  var(--color-dark-color-7);
  --color-color-8:  var(--color-dark-color-8);
  --color-color-9:  var(--color-dark-color-9);
  --color-color-10:  var(--color-dark-color-10);
  --color-color-11: var(--color-dark-color-11);
  --color-color-12: var(--color-dark-color-12);
  --color-neutral-1:  var(--color-dark-neutral-1);
  --color-neutral-2:  var(--color-dark-neutral-2);
  --color-neutral-3:  var(--color-dark-neutral-3);
  --color-neutral-4:  var(--color-dark-neutral-4);
  --color-neutral-5:  var(--color-dark-neutral-5);
  --color-neutral-6:  var(--color-dark-neutral-6);
  --color-neutral-7:  var(--color-dark-neutral-7);
  --color-neutral-8:  var(--color-dark-neutral-8);
  --color-neutral-9:  var(--color-dark-neutral-9);
  --color-neutral-10: var(--color-dark-neutral-10);
  --shadow-color: var(--shadow-color-dark);
  --contrast-color: white;
  --contrast-color-mix-percent: 5%;
}
```

### type-scales.css

```css
:root {
  /* inputs come from variables.css; see GenerateTypeScales in naina for the formula.
     ---------- derived (unitless, in rem units conceptually) ---------- */
  --type-min-width-rem: calc(var(--min-viewport) / 16);
  --type-max-width-rem: calc(var(--type-max-viewport) / 16);
  --type-vw-span: calc(var(--type-max-width-rem) - var(--type-min-width-rem));

  /* naina boosts every max-viewport size by MaxFontSize/MinFontSize — this is what makes
     large steps grow faster at wide viewports than simple ratio^N scaling would give. */
  --type-max-boost: calc(var(--type-max-font-size) / var(--min-font-size));
  --type-base-min: calc(var(--min-font-size) / 16);
  --type-base-max: calc(var(--type-max-font-size) / 16 * var(--type-max-boost));

  /* ---------- per-step bounds (unitless rem) ----------
     step N:  min = base-min * type-min-scale^N,  max = base-max * type-max-scale^N  */
  --fs--2-min: calc(var(--type-base-min) * pow(var(--type-min-scale), -2));
  --fs--2-max: calc(var(--type-base-max) * pow(var(--type-max-scale), -2));
  --fs--1-min: calc(var(--type-base-min) * pow(var(--type-min-scale), -1));
  --fs--1-max: calc(var(--type-base-max) * pow(var(--type-max-scale), -1));
  --fs-0-min:  var(--type-base-min);
  --fs-0-max:  var(--type-base-max);
  --fs-1-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  1));
  --fs-1-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  1));
  --fs-2-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  2));
  --fs-2-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  2));
  --fs-3-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  3));
  --fs-3-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  3));
  --fs-4-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  4));
  --fs-4-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  4));
  --fs-5-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  5));
  --fs-5-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  5));
  --fs-6-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  6));
  --fs-6-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  6));
  --fs-7-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  7));
  --fs-7-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  7));
  --fs-8-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  8));
  --fs-8-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  8));
  --fs-9-min:  calc(var(--type-base-min) * pow(var(--type-min-scale),  9));
  --fs-9-max:  calc(var(--type-base-max) * pow(var(--type-max-scale),  9));
  --fs-10-min: calc(var(--type-base-min) * pow(var(--type-min-scale), 10));
  --fs-10-max: calc(var(--type-base-max) * pow(var(--type-max-scale), 10));

  /* ---------- fluid clamp ----------
     slope     = (max - min) / vw-span             (unitless)
     yint      = min - min-width-rem * slope       (unitless, rem)
     value     = clamp(min rem, yint rem + slope * 100vw, max rem)  */
  --fs--2-slope: calc((var(--fs--2-max) - var(--fs--2-min)) / var(--type-vw-span));
  --fs--1-slope: calc((var(--fs--1-max) - var(--fs--1-min)) / var(--type-vw-span));
  --fs-0-slope:  calc((var(--fs-0-max)  - var(--fs-0-min))  / var(--type-vw-span));
  --fs-1-slope:  calc((var(--fs-1-max)  - var(--fs-1-min))  / var(--type-vw-span));
  --fs-2-slope:  calc((var(--fs-2-max)  - var(--fs-2-min))  / var(--type-vw-span));
  --fs-3-slope:  calc((var(--fs-3-max)  - var(--fs-3-min))  / var(--type-vw-span));
  --fs-4-slope:  calc((var(--fs-4-max)  - var(--fs-4-min))  / var(--type-vw-span));
  --fs-5-slope:  calc((var(--fs-5-max)  - var(--fs-5-min))  / var(--type-vw-span));
  --fs-6-slope:  calc((var(--fs-6-max)  - var(--fs-6-min))  / var(--type-vw-span));
  --fs-7-slope:  calc((var(--fs-7-max)  - var(--fs-7-min))  / var(--type-vw-span));
  --fs-8-slope:  calc((var(--fs-8-max)  - var(--fs-8-min))  / var(--type-vw-span));
  --fs-9-slope:  calc((var(--fs-9-max)  - var(--fs-9-min))  / var(--type-vw-span));
  --fs-10-slope: calc((var(--fs-10-max) - var(--fs-10-min)) / var(--type-vw-span));

  --fs--2-yint: calc(var(--fs--2-min) - var(--type-min-width-rem) * var(--fs--2-slope));
  --fs--1-yint: calc(var(--fs--1-min) - var(--type-min-width-rem) * var(--fs--1-slope));
  --fs-0-yint:  calc(var(--fs-0-min)  - var(--type-min-width-rem) * var(--fs-0-slope));
  --fs-1-yint:  calc(var(--fs-1-min)  - var(--type-min-width-rem) * var(--fs-1-slope));
  --fs-2-yint:  calc(var(--fs-2-min)  - var(--type-min-width-rem) * var(--fs-2-slope));
  --fs-3-yint:  calc(var(--fs-3-min)  - var(--type-min-width-rem) * var(--fs-3-slope));
  --fs-4-yint:  calc(var(--fs-4-min)  - var(--type-min-width-rem) * var(--fs-4-slope));
  --fs-5-yint:  calc(var(--fs-5-min)  - var(--type-min-width-rem) * var(--fs-5-slope));
  --fs-6-yint:  calc(var(--fs-6-min)  - var(--type-min-width-rem) * var(--fs-6-slope));
  --fs-7-yint:  calc(var(--fs-7-min)  - var(--type-min-width-rem) * var(--fs-7-slope));
  --fs-8-yint:  calc(var(--fs-8-min)  - var(--type-min-width-rem) * var(--fs-8-slope));
  --fs-9-yint:  calc(var(--fs-9-min)  - var(--type-min-width-rem) * var(--fs-9-slope));
  --fs-10-yint: calc(var(--fs-10-min) - var(--type-min-width-rem) * var(--fs-10-slope));

  --font-size--2: clamp(calc(var(--fs--2-min) * 1rem), calc(var(--fs--2-yint) * 1rem + var(--fs--2-slope) * 100vw), calc(var(--fs--2-max) * 1rem));
  --font-size--1: clamp(calc(var(--fs--1-min) * 1rem), calc(var(--fs--1-yint) * 1rem + var(--fs--1-slope) * 100vw), calc(var(--fs--1-max) * 1rem));
  --font-size-0:  clamp(calc(var(--fs-0-min)  * 1rem), calc(var(--fs-0-yint)  * 1rem + var(--fs-0-slope)  * 100vw), calc(var(--fs-0-max)  * 1rem));
  --font-size-1:  clamp(calc(var(--fs-1-min)  * 1rem), calc(var(--fs-1-yint)  * 1rem + var(--fs-1-slope)  * 100vw), calc(var(--fs-1-max)  * 1rem));
  --font-size-2:  clamp(calc(var(--fs-2-min)  * 1rem), calc(var(--fs-2-yint)  * 1rem + var(--fs-2-slope)  * 100vw), calc(var(--fs-2-max)  * 1rem));
  --font-size-3:  clamp(calc(var(--fs-3-min)  * 1rem), calc(var(--fs-3-yint)  * 1rem + var(--fs-3-slope)  * 100vw), calc(var(--fs-3-max)  * 1rem));
  --font-size-4:  clamp(calc(var(--fs-4-min)  * 1rem), calc(var(--fs-4-yint)  * 1rem + var(--fs-4-slope)  * 100vw), calc(var(--fs-4-max)  * 1rem));
  --font-size-5:  clamp(calc(var(--fs-5-min)  * 1rem), calc(var(--fs-5-yint)  * 1rem + var(--fs-5-slope)  * 100vw), calc(var(--fs-5-max)  * 1rem));
  --font-size-6:  clamp(calc(var(--fs-6-min)  * 1rem), calc(var(--fs-6-yint)  * 1rem + var(--fs-6-slope)  * 100vw), calc(var(--fs-6-max)  * 1rem));
  --font-size-7:  clamp(calc(var(--fs-7-min)  * 1rem), calc(var(--fs-7-yint)  * 1rem + var(--fs-7-slope)  * 100vw), calc(var(--fs-7-max)  * 1rem));
  --font-size-8:  clamp(calc(var(--fs-8-min)  * 1rem), calc(var(--fs-8-yint)  * 1rem + var(--fs-8-slope)  * 100vw), calc(var(--fs-8-max)  * 1rem));
  --font-size-9:  clamp(calc(var(--fs-9-min)  * 1rem), calc(var(--fs-9-yint)  * 1rem + var(--fs-9-slope)  * 100vw), calc(var(--fs-9-max)  * 1rem));
  --font-size-10: clamp(calc(var(--fs-10-min) * 1rem), calc(var(--fs-10-yint) * 1rem + var(--fs-10-slope) * 100vw), calc(var(--fs-10-max) * 1rem));
}
```

### space.css

```css
:root {
  /* inputs come from variables.css; see GenerateSpaceScales in naina for the formula.
     ---------- derived (all unitless) ---------- */
  --space-min-width-rem: calc(var(--min-viewport) / 16);
  --space-max-width-rem: calc(var(--space-max-viewport) / 16);
  --space-vw-span: calc(var(--space-max-width-rem) - var(--space-min-width-rem));

  /* same MaxFontSize/MinFontSize boost applied to every max-viewport size */
  --space-max-boost: calc(var(--space-max-font-size) / var(--min-font-size));
  --space-min-base: calc(var(--min-font-size) / 16);
  --space-max-base: calc(var(--space-max-font-size) / 16 * var(--space-max-boost));

  /* per-size bounds (unitless rem) */
  --sp-3xs-min: calc(var(--space-min-base) * var(--m-3xs));
  --sp-3xs-max: calc(var(--space-max-base) * var(--m-3xs));
  --sp-2xs-min: calc(var(--space-min-base) * var(--m-2xs));
  --sp-2xs-max: calc(var(--space-max-base) * var(--m-2xs));
  --sp-xs-min:  calc(var(--space-min-base) * var(--m-xs));
  --sp-xs-max:  calc(var(--space-max-base) * var(--m-xs));
  --sp-s-min:   calc(var(--space-min-base) * var(--m-s));
  --sp-s-max:   calc(var(--space-max-base) * var(--m-s));
  --sp-m-min:   calc(var(--space-min-base) * var(--m-m));
  --sp-m-max:   calc(var(--space-max-base) * var(--m-m));
  --sp-l-min:   calc(var(--space-min-base) * var(--m-l));
  --sp-l-max:   calc(var(--space-max-base) * var(--m-l));
  --sp-xl-min:  calc(var(--space-min-base) * var(--m-xl));
  --sp-xl-max:  calc(var(--space-max-base) * var(--m-xl));
  --sp-2xl-min: calc(var(--space-min-base) * var(--m-2xl));
  --sp-2xl-max: calc(var(--space-max-base) * var(--m-2xl));
  --sp-3xl-min: calc(var(--space-min-base) * var(--m-3xl));
  --sp-3xl-max: calc(var(--space-max-base) * var(--m-3xl));

  /* ---------- fluid clamps ----------
     For a pair going from size A at min-viewport to size B at max-viewport:
       slope = (B-max - A-min) / vw-span
       yint  = A-min - min-width-rem * slope
       value = clamp(A-min rem, yint rem + slope * 100vw, B-max rem)
     Singles are just the degenerate case A == B.  */

  /* singles */
  --space-3xs: clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-3xs-max) - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xs-max) - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xs-max) * 1rem));
  --space-2xs: clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-2xs-max) - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xs-max) - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xs-max) * 1rem));
  --space-xs:  clamp(calc(var(--sp-xs-min)  * 1rem), calc((var(--sp-xs-min)  - var(--space-min-width-rem) * (var(--sp-xs-max)  - var(--sp-xs-min))  / var(--space-vw-span)) * 1rem + (var(--sp-xs-max)  - var(--sp-xs-min))  / var(--space-vw-span) * 100vw), calc(var(--sp-xs-max)  * 1rem));
  --space-s:   clamp(calc(var(--sp-s-min)   * 1rem), calc((var(--sp-s-min)   - var(--space-min-width-rem) * (var(--sp-s-max)   - var(--sp-s-min))   / var(--space-vw-span)) * 1rem + (var(--sp-s-max)   - var(--sp-s-min))   / var(--space-vw-span) * 100vw), calc(var(--sp-s-max)   * 1rem));
  --space-m:   clamp(calc(var(--sp-m-min)   * 1rem), calc((var(--sp-m-min)   - var(--space-min-width-rem) * (var(--sp-m-max)   - var(--sp-m-min))   / var(--space-vw-span)) * 1rem + (var(--sp-m-max)   - var(--sp-m-min))   / var(--space-vw-span) * 100vw), calc(var(--sp-m-max)   * 1rem));
  --space-l:   clamp(calc(var(--sp-l-min)   * 1rem), calc((var(--sp-l-min)   - var(--space-min-width-rem) * (var(--sp-l-max)   - var(--sp-l-min))   / var(--space-vw-span)) * 1rem + (var(--sp-l-max)   - var(--sp-l-min))   / var(--space-vw-span) * 100vw), calc(var(--sp-l-max)   * 1rem));
  --space-xl:  clamp(calc(var(--sp-xl-min)  * 1rem), calc((var(--sp-xl-min)  - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-xl-min))  / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-xl-min))  / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-2xl: clamp(calc(var(--sp-2xl-min) * 1rem), calc((var(--sp-2xl-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-2xl-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-2xl-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-3xl: clamp(calc(var(--sp-3xl-min) * 1rem), calc((var(--sp-3xl-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-3xl-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-3xl-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from 3xs */
  --space-3xs-2xs: clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-2xs-max) - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xs-max) - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xs-max) * 1rem));
  --space-3xs-xs:  clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-xs-max)  - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xs-max)  - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xs-max)  * 1rem));
  --space-3xs-s:   clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-s-max)   - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-s-max)   - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-s-max)   * 1rem));
  --space-3xs-m:   clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-m-max)   - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-m-max)   - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-m-max)   * 1rem));
  --space-3xs-l:   clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-l-max)   - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-l-max)   - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-l-max)   * 1rem));
  --space-3xs-xl:  clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-3xs-2xl: clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-3xs-3xl: clamp(calc(var(--sp-3xs-min) * 1rem), calc((var(--sp-3xs-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-3xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-3xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from 2xs */
  --space-2xs-xs:  clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-xs-max)  - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xs-max)  - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xs-max)  * 1rem));
  --space-2xs-s:   clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-s-max)   - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-s-max)   - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-s-max)   * 1rem));
  --space-2xs-m:   clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-m-max)   - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-m-max)   - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-m-max)   * 1rem));
  --space-2xs-l:   clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-l-max)   - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-l-max)   - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-l-max)   * 1rem));
  --space-2xs-xl:  clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-2xs-2xl: clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-2xs-3xl: clamp(calc(var(--sp-2xs-min) * 1rem), calc((var(--sp-2xs-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-2xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-2xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from xs */
  --space-xs-s:   clamp(calc(var(--sp-xs-min) * 1rem), calc((var(--sp-xs-min) - var(--space-min-width-rem) * (var(--sp-s-max)   - var(--sp-xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-s-max)   - var(--sp-xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-s-max)   * 1rem));
  --space-xs-m:   clamp(calc(var(--sp-xs-min) * 1rem), calc((var(--sp-xs-min) - var(--space-min-width-rem) * (var(--sp-m-max)   - var(--sp-xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-m-max)   - var(--sp-xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-m-max)   * 1rem));
  --space-xs-l:   clamp(calc(var(--sp-xs-min) * 1rem), calc((var(--sp-xs-min) - var(--space-min-width-rem) * (var(--sp-l-max)   - var(--sp-xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-l-max)   - var(--sp-xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-l-max)   * 1rem));
  --space-xs-xl:  clamp(calc(var(--sp-xs-min) * 1rem), calc((var(--sp-xs-min) - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-xs-2xl: clamp(calc(var(--sp-xs-min) * 1rem), calc((var(--sp-xs-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-xs-3xl: clamp(calc(var(--sp-xs-min) * 1rem), calc((var(--sp-xs-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-xs-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-xs-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from s */
  --space-s-m:   clamp(calc(var(--sp-s-min) * 1rem), calc((var(--sp-s-min) - var(--space-min-width-rem) * (var(--sp-m-max)   - var(--sp-s-min)) / var(--space-vw-span)) * 1rem + (var(--sp-m-max)   - var(--sp-s-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-m-max)   * 1rem));
  --space-s-l:   clamp(calc(var(--sp-s-min) * 1rem), calc((var(--sp-s-min) - var(--space-min-width-rem) * (var(--sp-l-max)   - var(--sp-s-min)) / var(--space-vw-span)) * 1rem + (var(--sp-l-max)   - var(--sp-s-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-l-max)   * 1rem));
  --space-s-xl:  clamp(calc(var(--sp-s-min) * 1rem), calc((var(--sp-s-min) - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-s-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-s-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-s-2xl: clamp(calc(var(--sp-s-min) * 1rem), calc((var(--sp-s-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-s-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-s-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-s-3xl: clamp(calc(var(--sp-s-min) * 1rem), calc((var(--sp-s-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-s-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-s-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from m */
  --space-m-l:   clamp(calc(var(--sp-m-min) * 1rem), calc((var(--sp-m-min) - var(--space-min-width-rem) * (var(--sp-l-max)   - var(--sp-m-min)) / var(--space-vw-span)) * 1rem + (var(--sp-l-max)   - var(--sp-m-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-l-max)   * 1rem));
  --space-m-xl:  clamp(calc(var(--sp-m-min) * 1rem), calc((var(--sp-m-min) - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-m-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-m-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-m-2xl: clamp(calc(var(--sp-m-min) * 1rem), calc((var(--sp-m-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-m-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-m-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-m-3xl: clamp(calc(var(--sp-m-min) * 1rem), calc((var(--sp-m-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-m-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-m-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from l */
  --space-l-xl:  clamp(calc(var(--sp-l-min) * 1rem), calc((var(--sp-l-min) - var(--space-min-width-rem) * (var(--sp-xl-max)  - var(--sp-l-min)) / var(--space-vw-span)) * 1rem + (var(--sp-xl-max)  - var(--sp-l-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-xl-max)  * 1rem));
  --space-l-2xl: clamp(calc(var(--sp-l-min) * 1rem), calc((var(--sp-l-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-l-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-l-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-l-3xl: clamp(calc(var(--sp-l-min) * 1rem), calc((var(--sp-l-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-l-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-l-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from xl */
  --space-xl-2xl: clamp(calc(var(--sp-xl-min) * 1rem), calc((var(--sp-xl-min) - var(--space-min-width-rem) * (var(--sp-2xl-max) - var(--sp-xl-min)) / var(--space-vw-span)) * 1rem + (var(--sp-2xl-max) - var(--sp-xl-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-2xl-max) * 1rem));
  --space-xl-3xl: clamp(calc(var(--sp-xl-min) * 1rem), calc((var(--sp-xl-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-xl-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-xl-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));

  /* pairs from 2xl */
  --space-2xl-3xl: clamp(calc(var(--sp-2xl-min) * 1rem), calc((var(--sp-2xl-min) - var(--space-min-width-rem) * (var(--sp-3xl-max) - var(--sp-2xl-min)) / var(--space-vw-span)) * 1rem + (var(--sp-3xl-max) - var(--sp-2xl-min)) / var(--space-vw-span) * 100vw), calc(var(--sp-3xl-max) * 1rem));
}
```

### grids.css

```css
.grid {
  --grid-gap: clamp(1rem, 6vw, 3rem);
  --standard: min(var(--content-max-width), 100% - var(--grid-gap) * 2);
  --wide-1: minmax(0, var(--content-wide-1-width));
  --wide-2: minmax(0, var(--content-wide-2-width));
  --full: minmax(var(--grid-gap), 1fr);
  display: grid;
  grid-template-columns:
    [full-start] var(--full)
    [wide-2-start] var(--wide-2)
    [wide-1-start] var(--wide-1)
    [standard-start] var(--standard) [standard-end]
    var(--wide-1) [wide-1-end]
    var(--wide-2) [wide-2-end]
    var(--full) [full-end];
}
.grid > * { grid-column: standard; }
.grid .wide-1 { grid-column: wide-1; }
.grid .wide-2 { grid-column: wide-2; }
.grid .full { grid-column: full; }
.hol-wrapping-row {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(calc(var(--content-max-width) / 3 - 5rem), 1fr));
  gap: var(--space-m); justify-content: center; justify-items: center; align-items: start; padding: 0; width: 100%;
}
.hol-column {
  display: grid; grid-template-columns: minmax(auto, 1fr); justify-items: center; width: 100%; padding: 0; gap: var(--space-s);
}
.spaced > * + * { margin-block-start: var(--row-space, 1em); }
```

### elements.css

```css
html {
  block-size: 100%;
}

body {
  background-color: var(--color-neutral-10);
  color: var(--color-neutral-2);
  font-family: var(--font-family-1);
  font-feature-settings: "liga" 1, "dlig" 1, "hlig" 1, "clig" 1, "calt" 1;
  font-size: var(--font-size-0);
  font-variant-ligatures: normal;
  line-height: 1.4;
  print-color-adjust: exact
}

:focus-visible {
  outline: 2px solid var(--color-color-1);
  outline-offset: 2px;
}

::selection {
  background-color: var(--color-color-1);
  color: var(--color-light-neutral-9)
}

::marker {
  color: var(--color-color-1)
}

audio {
  display: flex;
}

img {
  height: auto;
  width: 100%;
  object-fit: cover;
  shape-margin: 1rem;
  vertical-align: middle;
  border-radius: var(--border-radius);
  font-style: italic;
}

video {
  display: flex;
  box-sizing: border-box;
  width: 100%;
}

figure {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-2xs);
  align-items: center;
}

.primary {
  background-color: var(--color-color-1);
  border-color: var(--color-color-1);
  border-style: solid;
  border-width: 1px;
  border-radius: 0.3em;
  color: var(--color-light-neutral-9);
  color: contrast-color(var(--color-color-1));
  cursor: pointer;
  display: inline-flex;
  font-family: var(--font-family-1);
  font-size: var(--font-size-0);
  line-height: 0.9;
  padding: 0.5em 1em;
  width: fit-content;
  text-decoration: none;
}

.primary:visited {
  color: var(--color-light-neutral-9);
  color: contrast-color(var(--color-color-1));
}

.primary:hover {
  background-color: var(--color-color-9);
  text-decoration: none;
  color: var(--color-light-neutral-9);
  color: contrast-color(var(--color-color-1));
}

.primary:active {
  background-color: var(--color-color-9);
  transform: scale(0.99) translateY(1px);
}

.primary:disabled {
  cursor: not-allowed;
  opacity: .54;
}

.secondary {
  border-color: var(--color-color-6);
  border-style: solid;
  border-width: 1px;
  border-radius: 0.3em;
  background-color: var(--color-neutral-9);
  color: var(--color-color-6);
  color: contrast-color(var(--color-neutral-9));
  cursor: pointer;
  display: inline-flex;
  font-family: var(--font-family-1);
  font-size: var(--font-size-0);
  line-height: 0.9;
  padding: 0.5em 1em;
  width: fit-content;
  text-decoration: none;
}

.secondary:visited {
  color: var(--color-color-6);
  color: contrast-color(var(--color-neutral-9));
}

.secondary:hover {
  border-color: var(--color-color-10);
  color: var(--color-color-10);
  color: contrast-color(var(--color-neutral-9));
  text-decoration: none;
}

.secondary:active {
  border-color: var(--color-color-10);
  color: var(--color-color-10);
  color: contrast-color(var(--color-neutral-9));
  transform: scale(0.99) translateY(1px);
}

.secondary:disabled {
  cursor: not-allowed;
  opacity: .54;
}

.tertiary {
  background-color: transparent;
  border-style: none;
  border-color: transparent;
  border-width: 0;
  color: var(--color-color-7);
  cursor: pointer;
  display: inline-flex;
  font-family: var(--font-family-1);
  font-size: var(--font-size-0);
  line-height: 1;
  padding: 0.5em 0;
  width: fit-content;
  text-decoration: underline;
  text-decoration-thickness: .1em;
  text-underline-offset: .15em;
}

.tertiary:visited {
  color: var(--color-color-7);
}

.tertiary:hover {
  color: var(--color-color-11);
}

.tertiary:disabled {
  cursor: not-allowed;
  opacity: .54;
}

.tertiary:active {
  color: var(--color-color-11);
  transform: scale(0.99) translateY(1px);
}

.delete {
  background-color: var(--color-neutral-10);
  border-style: solid;
  border-color: var(--color-color-4);
  border-width: 1px;
  border-radius: 0.3em;
  color: var(--color-light-neutral-9);
  color: contrast-color(var(--color-neutral-10));
  cursor: pointer;
  display: inline-flex;
  font-family: var(--font-family-1);
  font-size: var(--font-size-0);
  line-height: 0.9;
  padding: 0.5em 1em;
  width: fit-content;
  text-decoration: none;
}

.delete:visited {
  color: var(--color-light-neutral-9);
  color: contrast-color(var(--color-neutral-10));
}

.delete:hover {
  background-color: var(--color-color-4);
  text-decoration: none;
  color: var(--color-light-neutral-9);
  color: contrast-color(var(--color-light-neutral-2));
}

.delete:active {
  border-color: var(--color-color-12);
  transform: scale(0.99) translateY(1px);
}

.delete:disabled {
  cursor: not-allowed;
  opacity: .54;
}

input[type="text"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="search"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="tel"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="email"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="number"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="password"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="radio"] {
  cursor: pointer;
  height: var(--space-s);
  width: var(--space-s);
  border: 2px solid transparent;
}

input[type="radio"]:hover {
  accent-color: var(--color-color-1);
}

input[type="radio"]:checked {
  accent-color: var(--color-color-5);
}

input[type="range"] {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.4em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

progress {
  font-size: var(--font-size-0);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  line-height: 1;
  padding: 0.2em 0.4em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

select {
  padding: 0.3em 0.6em;
  background-color: var(--color-neutral-10);
  border: 2px solid var(--color-neutral-7);
  font-size: var(--font-size-0);
  line-height: 1;
  accent-color: var(--color-color-3);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  font-family: var(--font-family-1);
}

textarea {
  font-size: var(--font-size-0);
  border: 2px solid var(--color-neutral-7);
  font-family: var(--font-family-1);
  border-radius: var(--border-radius);
  color: var(--color-neutral-1);
  line-height: initial;
  padding: 0.3em 0.6em;
  accent-color: var(--color-color-3);
  background-color: var(--color-neutral-10);
}

input[type="checkbox"] {
  height: var(--space-s);
  width: var(--space-s)
}

input[type="checkbox"]:hover {
  accent-color: var(--color-color-5)
}

input[type="checkbox"]:checked {
  accent-color: var(--color-color-1)
}

input[type="checkbox"]:checked:hover {
  filter: brightness(1.1)
}

label {
  line-height: 1;
  align-items: center;
  color: var(--color-neutral-1);
  display: flex;
  font-family: var(--font-family-1);
  font-size: var(--font-size--1);
  gap: var(--space-3xs);
}

a {
  color: var(--color-color-5);
  text-decoration: none;
  text-decoration-thickness: 0.1em;
  text-underline-offset: 0.15em;
}

a:hover {
  color: var(--color-color-9);
  text-decoration: underline;
  text-decoration-thickness: 0.1em;
  text-underline-offset: 0.15em;
}

a:visited {
  color: var(--color-neutral-1);
}

blockquote {
  margin: 0;
  padding: var(--space-m) var(--space-l);
  display: flex;
  flex-direction: column;
  font-size: var(--font-size-2);
  font-weight: bold;
  font-family: var(--font-family-1);
}

h1 {
  font-family: var(--font-family-1);
  font-size: var(--font-size-6);
  font-weight: normal;
  line-height: 1.2;
  text-wrap: balance;
}

h2 {
  font-family: var(--font-family-1);
  font-size: var(--font-size-5);
  font-weight: normal;
  line-height: 1.2;
  text-wrap: balance;
}

h3 {
  font-family: var(--font-family-1);
  font-size: var(--font-size-4);
  font-weight: normal;
  line-height: 1.2;
  text-wrap: balance;
}

h4 {
  font-family: var(--font-family-1);
  font-size: var(--font-size-3);
  font-weight: normal;
  line-height: 1.2;
  text-wrap: balance;
}

h5 {
  font-family: var(--font-family-1);
  font-size: var(--font-size-2);
  font-weight: normal;
  line-height: 1.2;
  text-wrap: balance;
}

h6 {
  font-family: var(--font-family-1);
  font-size: var(--font-size-1);
  font-weight: normal;
  line-height: 1.2;
  text-wrap: balance;
}

p {
  font-family: var(--font-family-1);
  font-size: var(--font-size-0);
  font-weight: normal;
  line-height: 1.4;
}

span {
  font-family: var(--font-family-1);
  font-size: var(--font-size--1);
  font-weight: thin;
}

time {
  background-color: var(--color-neutral-9);
  display: flex;
  width: fit-content;
  color: var(--color-neutral-4);
  font-size: var(--font-size--1);
  font-weight: 200;
}

ul {
  padding: 0;
  margin: 0;
}

ol {
  padding: 0;
  margin: 0;
}
```

### dashboard.css

```css
.dashboard {
	padding-block: var(--space-l-xl);

	> div {
		display: flex;
		flex-direction: column;
		gap: var(--space-m-l);

		> header {
			display: flex;
			justify-content: space-between;
			align-items: baseline;
			gap: var(--space-s);

			> div {
				display: flex;
				align-items: baseline;
				gap: var(--space-s);

				> span {
					color: var(--color-neutral-5);
				}
			}
		}

		> article {
			display: flex;
			flex-direction: column;
			gap: var(--space-s);
			padding: var(--space-s-m);
			border: 1px solid var(--color-neutral-3);
			border-radius: var(--border-radius);

			&.invite-link {
				border-color: var(--color-color-6);

				> input {
					width: 100%;
					font-family: monospace;
				}
			}

			> table {
				width: 100%;
				border-collapse: collapse;

				& th {
					text-align: left;
					color: var(--color-neutral-5);
					padding-block: var(--space-3xs);
				}

				& td {
					padding-block: var(--space-3xs);
					border-top: 1px solid var(--color-neutral-2);
				}
			}

			& .pending {
				color: var(--color-neutral-5);
			}

			> form {
				display: flex;
				align-items: end;
				gap: var(--space-s);

				> label {
					flex: 1;
					display: flex;
					flex-direction: column;
					gap: var(--space-3xs);
				}
			}
		}
	}
}
```

### join.css

```css
.join {
	padding-block: var(--space-xl-2xl);

	> div {
		display: grid;
		place-items: center;

		> .hol-column {
			max-width: 32rem;
			text-align: center;
			gap: var(--space-s);

			> input {
				width: 100%;
				text-align: center;
				font-family: monospace;
			}
		}
	}
}
```

### login.css

```css
/* login */

.login {
	padding: var(--space-l-2xl) 0;
}

.login > div > .hol-column {
	justify-items: start;
	gap: var(--space-m);
	max-width: 26rem;

	& > p {
		color: var(--color-neutral-5);
	}

	& > ul {
		display: flex;
		flex-direction: column;
		gap: var(--space-s);
		list-style: none;
		width: 100%;

		& > li > a {
			display: block;
			text-align: center;
			text-transform: capitalize;
		}
	}
}
```

### download.css

```css
.download {
	padding-block: var(--space-xl-2xl);

	> div {
		display: grid;
		place-items: center;

		> .hol-column {
			max-width: 32rem;
			gap: var(--space-s);
		}
	}
}
```

