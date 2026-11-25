---
name: Grapplehook
description: A desktop front end for yt-dlp and ffmpeg where every video you hook gets a claim tag.
colors:
  tag: '#ff8a3d'
  tag-hover: '#ff9d5c'
  tag-ink: '#16120d'
  tag-ink-2: '#4a2a12'
  tag-ink-hover: '#2b2219'
  tag-well: 'rgb(22 18 13 / 0.1)'
  ground: '#121110'
  field: '#1e1c19'
  field-2: '#2a2723'
  line: '#3a352f'
  ink: '#efe7da'
  muted: '#a69c8f'
  ok: '#6fd18f'
  bad: '#f0625a'
typography:
  display:
    fontFamily: "'Barlow Condensed', system-ui, sans-serif"
    fontSize: '2.6rem'
    fontWeight: 700
    lineHeight: 1
    letterSpacing: '-0.02em'
    fontFeature: 'tnum'
  headline:
    fontFamily: "'Barlow Condensed', system-ui, sans-serif"
    fontSize: '1.86rem'
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: '-0.01em'
  title:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '1.07rem'
    fontWeight: 700
  body:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '14px'
    fontWeight: 400
    lineHeight: 1.45
  body-sm:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '0.857rem'
    fontWeight: 400
  label:
    fontFamily: "system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: '0.75rem'
    fontWeight: 600
    letterSpacing: '0.1em'
rounded:
  seg: '1px'
  sm: '4px'
  control: '6px'
  sheet: '8px'
spacing:
  xs: '4px'
  sm: '8px'
  md: '16px'
  lg: '24px'
  xl: '32px'
  xxl: '48px'
components:
  button:
    backgroundColor: '{colors.field-2}'
    textColor: '{colors.ink}'
    rounded: '{rounded.control}'
    padding: '0 16px'
    height: '40px'
  button-hover:
    backgroundColor: '{colors.line}'
  button-primary:
    backgroundColor: '{colors.tag}'
    textColor: '{colors.tag-ink}'
    rounded: '{rounded.control}'
    padding: '0 16px'
    height: '40px'
  button-primary-hover:
    backgroundColor: '{colors.tag-hover}'
  button-stub:
    backgroundColor: '{colors.tag-ink}'
    textColor: '{colors.tag}'
    rounded: '{rounded.control}'
    padding: '8px'
    height: '40px'
  button-stub-hover:
    backgroundColor: '{colors.tag-ink-hover}'
  button-pill:
    textColor: '{colors.muted}'
    typography: '{typography.body-sm}'
    rounded: '{rounded.control}'
    padding: '0 12px'
    height: '32px'
  button-pill-hover:
    backgroundColor: '{colors.field-2}'
    textColor: '{colors.ink}'
  input:
    backgroundColor: '{colors.field}'
    textColor: '{colors.ink}'
    rounded: '{rounded.control}'
    padding: '0 16px'
    height: '40px'
  tag-option-value:
    textColor: '{colors.tag-ink}'
    typography: '{typography.title}'
    rounded: '{rounded.sm}'
    padding: '0 8px'
    height: '32px'
  badge:
    backgroundColor: '{colors.field-2}'
    textColor: '{colors.muted}'
    typography: '{typography.label}'
    rounded: '{rounded.sm}'
    padding: '2px 8px'
  stub:
    width: '144px'
    padding: '16px'
---

# Design System: Grapplehook

## Overview

**Creative North Star: "The Claim Tag"**

Every video you hook gets a tag; you collect the file with its stub. The window is a dark, warm counter: a near-black ground with solid, unbordered fields on top of it. Against that ground the current video comes out as a rope-orange tag printed in near-black ink. It has an eyelet notch on its left edge, its options are printed on the stock in dashed-divided segments, and a perforated stub on the right carries a claim number and the action. Downloads become dark tags with the same stub, and their routing strip fills as the work moves.

The system is dense and calm, built for short, occasional sessions. Hierarchy comes from weight and ink, not from many sizes: there is a single tracked-caps label size, and every size works within one 8px module. Colour carries state, not decoration. Orange marks what is live, green marks done, red marks failure, and everything else is warm neutral.

The world is literal about its materials. Perforations are dashed 2×8px rules, the eyelet is a hole punched to the ground colour, and claim numbers are tabular. On a finished download the stub turns green and becomes Show in folder.

**Key Characteristics:**

- Warm near-black ground, solid fields, no bordered cards in the content flow.
- Rope-orange stock only for the one live item; ink printing on it.
- Perforated stub with a tabular claim number on every tag.
- A four-segment routing strip shows progress (prepare, download, convert, saved).
- 8px module; one label size; hierarchy by weight.

## Colors

The palette is a warm charcoal ramp with one brand orange and two state colours.

### Primary

- **Rope Orange** (`tag`): the stock of the live tag (fetched video, first-run setup), the primary button, the progress fill, running claim numbers and stage text, links, focus rings, the text caret, selection and the hook mark. It is a brand commitment.
- **Warm Rope Hover** (`tag-hover`): hover state for the primary button only.
- **Print Ink** (`tag-ink`): everything printed on orange stock: text, the perforation, the dashed segment dividers, the 2px rule under the tag head, the stub button fill, checkbox accent and focus outlines inside the tag.
- **Faded Print** (`tag-ink-2`): secondary print on orange (labels, fine print, placeholders, the Change marker).
- **Stub Button Hover** (`tag-ink-hover`): hover for ink-filled stub buttons.
- **Ink Well** (`tag-well`): a 10% ink tint for hover wells on orange and for the empty track of a strip printed on orange.

### Neutral

- **Ground** (`ground`): the window background. It also shows through the eyelet hole and through the perforation of dark tags.
- **Field** (`field`): the top bar, inputs, dark download tags, the Tools sheet and the About dialog.
- **Raised Field** (`field-2`): default buttons, badges, empty route segments, pill hover.
- **Line** (`line`): button hover, overlay borders, the blank tag's dashed outline, scrollbars.
- **Bone** (`ink`): primary text.
- **Dust** (`muted`): secondary text, labels, stats, placeholders, idle pills.

### State

- **Done Green** (`ok`): the done stage, the filled routing strip and the green stub of a finished download, Tools ready, and Managed or System badges.
- **Failed Red** (`bad`): failed stage, strip and claim number, error text, Tools missing, the Missing badge, and the danger button (18% mix into `field-2`).

### Named Rules

**The Live Stock Rule.** A solid orange fill means the one live item: the fetched video's tag, the setup tag, or the primary button. Once the tag is out, Fetch info drops back to a default button until the URL changes. Orange as text or stroke (running stage text, progress fill, links, focus, the update pill, the hook mark) marks something active, and is never used as a decorative fill.

**The Printed-On Rule.** Inside orange stock nothing uses neutral colours. Text, rules, outlines and controls print in `tag-ink` / `tag-ink-2`, and hover wells use `tag-well`.

## Typography

**Display Font:** Barlow Condensed 700/800 (SIL OFL 1.1), self-hosted from `src/assets/fonts/`, used for claim numbers, tag titles, the wordmark and the About title
**Body Font:** system-ui (with -apple-system, Segoe UI, sans-serif)

**Character:** A condensed grotesque with tag-print character for everything printed on a tag (titles, claim numbers, the wordmark), over a plain system sans for reading and controls.

### Hierarchy

- **Display** (Barlow Condensed 800, 2.6rem, 1, -0.02em, tabular): the claim number on the live tag's stub. On dark download stubs it drops to 1.86rem.
- **Headline** (Barlow Condensed 700, 1.86rem, 1.05, balanced wrap): the tag title (video title, setup heading, empty-state heading, About title).
- **Title** (700, 1.07rem system-ui): option values printed on the tag (quality, folder, filename). The wordmark is Barlow Condensed 700 at 1.36rem.
- **Body** (400, 14px root, 1.45): all running text, task titles (600) and buttons (600).
- **Body small** (0.857rem): task stats, stage text (600), pills (600), Tools sheet text and paths. Errors are 0.929rem.
- **Label** (600, 0.75rem, 0.1em, uppercase): field labels on the tag, the "Claim no." stub label, the Downloads heading, and the Change marker. Badges use the same size with 0.04em tracking and no uppercase.

### Named Rules

**The One Label Rule.** There is exactly one tracked-caps size (0.75rem/600/0.1em). It names a field or a count and is never used decoratively above a headline.

**The Tabular Numbers Rule.** Claim numbers, the version and task stats use tabular figures, so counts don't jitter while they update.

### Font loading

The app's CSP allows only self-hosted fonts, so the display face ships as woff2 files in `src/assets/fonts/` (weights 700 and 800, Latin subset, `font-display: swap`) with its licence in `OFL.txt`. Use it through `var(--display)`. Don't add hosted font stylesheets.

## Layout

Everything uses an 8px module: 4, 8, 16, 24, 32 and 48px, with 4px only for tight internal gaps. A single centred column (max 760px) sits under a 48px top bar that is also the window drag region. Main padding is 24px, with 48px at the bottom, and blocks are 16px apart. The URL row is a flexible input plus buttons with 8px gaps. Tags are full-width rows: a flexible body (live tag 16/24/24/32px padding, with 32px on the left to clear the eyelet; download tag 16/24px) and a fixed 144px stub. Downloads stack 8px apart. Below 640px (the window minimum is 560) stubs narrow to 112px and the thumbnail to 120px. Options sit in a wrapping row of segments (min 150px each). The overlays are the Tools sheet (popover, max 440px, anchored 56px from the top and 16px from the right) and the About dialog (max 460px).

## Elevation & Depth

Content is flat. Depth comes from tonal steps (`ground` → `field` → `field-2`) and from the bright orange stock, not from shadows. Only floating overlays lift.

### Shadow Vocabulary

- **Overlay lift** (`box-shadow: 0 16px 40px rgb(0 0 0 / 0.5)`): the Tools sheet and the About dialog only. The About backdrop is `rgb(8 7 6 / 0.7)`.

### Named Rules

**The Solid Field Rule.** In-flow surfaces are solid fills without borders or shadows. Only overlays get a 1px `line` border and the overlay lift.

## Shapes

Corners are small and practical: controls 6px, small wells and chips 4px, overlays 8px, route segments 1px. Tags are asymmetric (8px 4px 4px 8px for the live tag, 6px 4px 4px 6px for download tags), with the softer corners on the eyelet side. The eyelet is a 16px circle at the left edge, filled with the ground colour (or `line` on the blank tag). Perforations are a repeating 2×8px half-filled vertical gradient on the stub's left edge, in `tag-ink` on orange, `ground` on dark tags, and `line` on the blank tag. Inside the tag, dashed 1px ink rules divide option segments, and a solid 2px ink rule separates the tag head from its options. The pre-video blank tag is an unprinted outline: a 2px dashed `line` outline inset 2px on a transparent fill. Status dots are 8px circles in `currentColor`.

## Components

### Buttons

Quiet, solid and squat.

- **Shape:** 6px corners, 40px tall, 0 16px padding, 600 weight, no wrap.
- **Default:** `field-2` fill with bone text. Hover is `line`, with a 120ms ease-out background transition.
- **Primary:** rope-orange fill with ink text; hover is `tag-hover`. Used for Fetch info until the tag is out, and for Install Tools.
- **Stub button:** ink fill with orange text, full stub width, wraps, sticky 16px from the bottom. On a done download it is ink with green text.
- **Danger:** `bad` mixed 18% into `field-2`, with red text (Delete Tools).
- **Pill:** 32px, transparent, muted, for top-bar items. Hover adds `field-2` and bone text. Variants: ok (green), bad (red), update (orange). It carries an 8px status dot.
- **Focus:** 2px `tag` outline offset 2px. Inside orange stock the outline is `tag-ink`.
- **Disabled:** opacity 0.45, not-allowed cursor.

### Inputs / Fields

- **Style:** `field` fill, no border, 6px corners, 40px tall, muted placeholder. Read-only fields use muted text.
- **On the tag:** printed values are transparent, ink, 1.07rem/700, 32px tall, and pulled 8px left so the text aligns with the label. Hover and focus show a `tag-well`. Text inputs show a resting dashed ink fill-in line along the bottom. The folder picker is a value button with an underlined tracked-caps "Change" marker. Checkboxes are 18px with an ink accent.

### Badges and chips

- **Badge:** 0.75rem/600 with 0.04em tracking, 2px 8px padding, 4px corners, `field-2` fill and muted text. Ok and bad variants tint 14% of the state colour into `field`.
- **Tag chip ("optional"):** a 0.75rem chip with 0 6px padding and a `tag-well` fill on orange, or `field-2` with muted text in the Tools sheet.
- **Missing marker (setup):** an ink well with orange text and 4px corners.

### Navigation

The top bar is 48px of `field` with 24px side padding. It holds the wordmark (an 18px orange stroke hook mark, the lowercase "grapplehook" at 1.07rem/700, and a muted tabular version) on the left, and pills (update notice, Tools status, About) with 8px gaps on the right.

### The Claim Tag (signature)

- **Live tag:** orange stock with an eyelet. The body holds the 160px 16:9 thumbnail (4px corners), the headline title and the fine subtitle. Below the 2px ink rule is the option segment row. The stub carries the "Claim no." label, the 2rem claim number, and Download at the bottom.
- **Setup tag:** the same stock. A three-column list (9rem / 1fr / auto) of Tool, purpose and state. The stub shows the download size label and Install Tools.
- **Blank tag:** a dashed outline with a `line` eyelet and perforation, the headline "No video yet", muted instruction text, and the next claim number.
- **Download tag:** `field` fill. The body has a task title with stage text, the routing strip and stats. The stub has a 1.43rem claim number (orange while running) and Cancel or Show in folder. Done turns the stub green, failed turns the stage, strip and number red, and cancelled mutes them.

### Routing strip

Four equal 6px segments with 4px gaps and 1px corners. Empty segments are `field-2`, full segments are `tag`, and the current segment fills by percent (200ms ease-out width). An indeterminate segment runs a 30% bar across in 1.2s with `cubic-bezier(0.22, 1, 0.36, 1)`. Reduced motion stops the animation and shows a full bar at 0.4 opacity. On orange stock (Tool install) the track is `tag-well` and the fill is ink.

### Overlays

The Tools sheet and the About dialog use a `field` fill, a 1px `line` border, 8px corners and the overlay lift, with 16px and 24px padding respectively. Rows are compact grids, with 1px `line` dividers in the Tools sheet.

## Do's and Don'ts

### Do:

- **Do** keep every gap, padding and size on the 8px module (4px for tight internal gaps only).
- **Do** give every new item tag the stub: perforated left edge, tabular claim number, and its one action.
- **Do** print in `tag-ink` / `tag-ink-2` on orange stock, and switch focus outlines to ink there.
- **Do** show progress with the four-segment routing strip, and respect reduced motion.
- **Do** step the primary button back to default once its live item exists.
- **Do** use the 0.75rem tracked-caps label only to name a field or a count.

### Don't:

- **Don't** put a solid orange fill on anything but the one live item.
- **Don't** draw borders or shadows on in-flow surfaces. Borders and the overlay lift belong to the Tools sheet and the About dialog.
- **Don't** add a second label size or decorative caps above headlines.
- **Don't** use neutral greys on orange stock.
- **Don't** set running text, controls or task titles in the display face; it is for printed tag matter only.
