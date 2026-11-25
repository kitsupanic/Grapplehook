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

<!-- impeccable:design-map -->
Name: Grapplehook
Colors: tag #ff8a3d, tag-hover #ff9d5c, tag-ink #16120d, tag-ink-2 #4a2a12, tag-ink-hover #2b2219, tag-well rgb(22 18 13 / 0.1), ground #121110, field #1e1c19, field-2 #2a2723, line #3a352f, ink #efe7da, muted #a69c8f, ok #6fd18f (+1 more in the frontmatter)
Fonts: Barlow Condensed (display, headline); system-ui (title, body, body-sm, label)
Spacing: xs 4px, sm 8px, md 16px, lg 24px, xl 32px, xxl 48px
Radius: seg 1px, sm 4px, control 6px, sheet 8px
North Star: "The Claim Tag"
Rules:
- The Live Stock Rule. A solid orange fill means the one live item: the fetched video's tag, the setup tag, or the primary button.
- The Printed-On Rule. Inside orange stock nothing uses neutral colours.
- The One Label Rule. There is exactly one tracked-caps size (0.75rem/600/0.1em).
- The Tabular Numbers Rule. Claim numbers, the version and task stats use tabular figures, so counts don't jitter while they update.
- The Solid Field Rule. In-flow surfaces are solid fills without borders or shadows.
Elevation:
- Overlay lift: 0 16px 40px rgb(0 0 0 / 0.5)
Don't:
- Don't put a solid orange fill on anything but the one live item.
- Don't draw borders or shadows on in-flow surfaces.
- Don't add a second label size or decorative caps above headlines.
- Don't use neutral greys on orange stock.
- Don't set running text, controls or task titles in the display face; it is for printed tag matter only.
Do:
- Do keep every gap, padding and size on the 8px module (4px for tight internal gaps only).
- Do give every new item tag the stub: perforated left edge, tabular claim number, and its one action.
- Do print in `tag-ink` / `tag-ink-2` on orange stock, and switch focus outlines to ink there.
- Do show progress with the four-segment routing strip, and respect reduced motion.
- Do step the primary button back to default once its live item exists.
- Do use the 0.75rem tracked-caps label only to name a field or a count.
Map: src/assets/fonts/
Full: .impeccable/design-full.md (read a section when you need its detail)
