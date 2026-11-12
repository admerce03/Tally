# Tally design guidelines

Read this before changing anything in `app/ui`. It applies to every screen, desktop and (later) iOS.
The model is a native Apple app (Notes, Reminders, Settings), adapted from Apple's Human Interface
Guidelines. When this file and a habit disagree, this file wins. Update it when a decision changes.

## Principles

1. **Let the visual carry meaning; words only when a visual can't.** No explanatory sentences, no
   section titles that restate what the content already shows. Color, position, a bar or an arrow
   says it; a `title` tooltip holds the sentence for those who hover.
2. **One tool for each job.** Every task has exactly one place in the UI. Before adding a control,
   find where the job is already done. (Example: the target is edited only on its stat, never in
   Settings too. Settings has one entry point.)
3. **Content first, chrome last.** The grades are the interface. No boxes around boxes, no badges.
   Group with whitespace and hairline separators.
4. **Use the window.** Wide windows get a wide column whose groups flow into columns; narrow windows
   get one column. Never a thin strip of content in a sea of white.
5. **Edit in place.** Click a value to change it where it is; click a row to open its editor inside
   the row. Hypotheticals are typed in place too (what-if mode).
6. **Hierarchy through type, not decoration.** One large number per screen.
7. **Color means something.** Accent: interactive things, selection, and what-if values. Red:
   destructive actions, an unreachable target, a grade that went down (↓). Green: a secured
   target, a grade that went up (↑). Everything else is neutral. Color always pairs with a shape
   or symbol (arrow, chip), never alone.
8. **Parity.** Every number shown comes from a core `CommandOutput`. The UI formats and lays out;
   it never computes a grade, a delta or a GPA. (Layout ratios such as bar widths are fine.)

## Layout

- **Window:** sidebar (240px, `--bg-side`) + content. No native title bar: the top `--bar-h`
  (44px) of both columns is the title bar (`data-tauri-drag-region`: drag to move, double-click
  to maximize). The sidebar's strip is empty (room for macOS traffic lights); the content's strip
  is the **toolbar** (what-if control on the trailing side; on Windows, our own minimize /
  maximize / close buttons, 46px wide, flush right; close hovers `--win-close`). Window chrome is
  chosen in `lib/platform.ts`: Windows = frameless (`tauri.windows.conf.json`, decorations off),
  macOS = `titleBarStyle: Overlay` + `hiddenTitle`, browser/Linux = native, no controls drawn.
- **Content:** column `--content-max` (1180px) centered, side gutter `--gutter` (16-48px, scales
  with the window). Below 720px the sidebar stacks above the content.
- **Columns:** groups (categories, terms) sit in a `.cols`-style grid,
  `repeat(auto-fill, minmax(min(100%, 420px), 1fr))`: two columns on a desktop window, one on
  narrow ones. Component breakpoints use container queries on `.page`, not the viewport.
- **Page header:** Large Title and, on a course, one secondary line. No buttons: actions live in
  the toolbar, Settings in the sidebar.
- **Sidebar (source list):** GPA (cumulative trailing), What-if; then terms. Each term label is a
  disclosure (collapse its courses) with the term GPA trailing, aligned with the course letters
  (core `term_gpa`, through the open what-if). On hover/focus the GPA gives way to the "⋯" menu
  (Rename, Set year…, Delete…) in the same slot. Optional **Group terms by year** (Settings > All courses, per device): collapsible
  year headers, newest year first, "No year" last, latest-added term first within a year.
  Collapsed state is remembered per device. Footer: "+" menu (New course, New term) leading;
  trailing: the sync status (only while sync is on: dot + one word) and the one **Settings gear**
  at the right edge.
- **Term years** are real data (core `Term.year`, `set_term_year`), not parsed from names: names
  like "2026-27" or "Spring '27" can't be parsed reliably, and agents/CLI need the same grouping.
  The UI only *prefills* the year field from a 4-digit year in the name.

## Course page

1. **Header:** course name; one secondary line built as one string ("3 credits · Fall 2026").
2. **Summary row:** hero grade with its letter (letter in `--label-2`, same size); in a what-if
   the real grade sits before it, small and faint, with an arrow ("91.2% A- → 91.5% A-"). Stats
   on the trailing side:
   - **Target**: the percent, green when secured, red when unreachable, neutral otherwise; no
     words. Click to edit in a popover. "Set" in accent when there is none. The tooltip says what
     is needed.
   - **Locked in**: the projected grade (the floor if you score 0 on what's left).
   - **GPA** (what-if only): cumulative GPA, "3.48 → 3.52" when it differs.
3. **Categories** (one inset group each, in columns): header = name + weight (and "drops N") in
   secondary, category average trailing in the score column. Click to edit in place.
   - Row: name, due date under it. Trailing score "90 / 100" (points possible in `--label-2`).
   - Graded rows: how much the grade moved when it was graded, "↑1.2" green / "↓0.8" red, under
     the score (core `impact`). Arrows, not +/−, so it never reads as bonus points; ~0 hidden.
   - Remaining rows: "85.4 needed" under "— / 100" in accent (red when above full marks), and a
     40px weight bar before the score (core `leverage`, width relative to the largest).
   - Dropped scores: struck through in `--label-3`. Missing / Excused: one secondary word.
   - Click the score to type in place (Enter saves, Esc cancels); click the row for the full editor.
   - "Add assignment" as the last row; "Add category" after the groups.

4. **Trend chart**, full width under the categories (always shown): running grade after each
   graded assignment (core `impact`, one evenly spaced point per assignment, chronological, so
   undated work still plots). Y axis in %, light grid, a tick per point. Dates label the x axis
   only when every point has one (thinned to fit); otherwise ticks alone, never half-labelled.
   Target as a dashed line labelled "Target 90%" in the right gutter, outside the plot. Hover: assignment name, date, value. No data: axes plus a
   faint "No graded work yet". In a what-if: real as a quiet `--label-3` line, what-if as an
   accent dashed line on top.

## What-if

- **A mode, entered from the toolbar.** The toolbar button "What if ▾" opens a menu shaped like
  the What-if page: "New what-if for <course>" (on a course page), the saved what-ifs (switch; a
  check marks the open one), Rename… / Discard… (while one is open), All what-ifs. While one is open the button becomes an accent-tinted pill with
  its name, next to a prominent "Done". No banner, no tinted window edge.
- **Values in place.** Type a score in a row to set a hypothetical one (core `add_override`
  set_score, which replaces an earlier one). A changed value is a quiet accent-tint chip with its
  own × (reverts every change to that assignment); the real score sits faintly beside it, struck
  through. Clearing the field also reverts. Added hypothetical assignments have an accent name.
  Nothing else turns blue.
- Done keeps the what-if (a new one with no changes is dropped). Discard deletes it (confirmation
  only for one saved earlier with changes).
- **What-if page:** header with "+ New what-if" (menu of courses: creates "<course> what-if" and
  opens that course in the mode). One row per what-if, readable at a glance:
  - line 1: name (accent while open); trailing actions Open · Rename · Delete, shown on hover or
    focus (always on touch).
  - line 2: what it changes, in short phrases ("Exam 3 → 96 · + Quiz 5 · Curve +5"), clamped to
    two lines; trailing: what that does, per course it touches and for GPA, as caption + "real →
    what-if" with the what-if value green ↑ or red ↓ (real faint). Unchanged values show alone.
  - Click the row to open it on its course. Rename is in place.
  - "Side by side" table only with 2+ what-ifs: Real plus each what-if, only rows where some
    what-if differs (cumulative GPA always); differing cells accent. All numbers from core
    `compare_scenarios`.

## Settings

One sheet, one entry point: the gear in the sidebar footer (or Ctrl/Cmd+,). Top-anchored so it
doesn't jump.
- On a course page it opens on that course, with a segmented control: [course name] | All courses.
- **Course:** name, code, credits; Curve (add points, lower cutoffs by); Letter cutoffs (two
  columns); GPA (status, official letter); "Delete course…". In a what-if, curve and GPA rows edit
  the what-if (accent on a tint); the rest is disabled.
- **All courses:** Curves (show grades with curves; count curves in needed scores), Sidebar (group
  terms by year), GPA scale (two columns), Sync. Display choices are saved per device.
- Fields commit on Enter or blur. One "Done".

## Visual system

All values are CSS custom properties in `app.css`. Never hard-code a color in a component.

**Type** (body is 14px rather than 13pt; Windows renders the same pixel size smaller):

| Token          | Size / line | Weight | Use                                     |
|----------------|-------------|--------|-----------------------------------------|
| `--t-large`    | 28 / 34     | 700    | Page title                              |
| `--t-hero`     | 44 / 48     | 600    | The one large number (tabular figures)  |
| `--t-title`    | 17 / 22     | 600    | Sheet titles                            |
| `--t-headline` | 14 / 20     | 600    | Group headers, section labels           |
| `--t-body`     | 14 / 20     | 400    | Default                                 |
| `--t-callout`  | 13 / 18     | 400    | Secondary lines                         |
| `--t-caption`  | 11 / 14     | 600    | Sidebar labels, stat labels (uppercase) |

Stat values are 22/28 weight 500. Font stack: `-apple-system, BlinkMacSystemFont, "Segoe UI
Variable Text", "Segoe UI", system-ui, sans-serif`. All numbers use tabular figures.

**Color** (semantic names, light / dark):

| Token            | Light                 | Dark                   |
|------------------|-----------------------|------------------------|
| `--bg`           | `#ffffff`             | `#1e1e1e`              |
| `--bg-side`      | `#f5f5f7`             | `#262628`              |
| `--bg-group`     | `#f5f5f7`             | `#2a2a2c`              |
| `--bg-hover`     | `rgba(0,0,0,.04)`     | `rgba(255,255,255,.06)`|
| `--label`        | `rgba(0,0,0,.88)`     | `rgba(255,255,255,.92)`|
| `--label-2`      | `rgba(0,0,0,.55)`     | `rgba(255,255,255,.55)`|
| `--label-3`      | `rgba(0,0,0,.28)`     | `rgba(255,255,255,.28)`|
| `--separator`    | `rgba(0,0,0,.09)`     | `rgba(255,255,255,.09)`|
| `--accent`       | `#007aff`             | `#0a84ff`              |
| `--accent-tint`  | `rgba(0,122,255,.12)` | `rgba(10,132,255,.22)` |
| `--red`          | `#ff3b30`             | `#ff453a`              |
| `--green`        | `#28a745`             | `#30d158`              |

**Shape and spacing:** 4px grid. Groups radius 10px, controls 6px, no borders on groups.
Separators 1px `--separator`, inset to the text. Row height 44px min. Shadows only on floating
things (popovers, sheets, menus).

**Controls:**
- *Prominent*: accent fill, white text, at most one per view (Done, Add).
- *Standard*: `--bg-group` fill (What if ▾, Cancel).
- *Borderless*: accent text (Add assignment, Set).
- *Quiet icon*: `--label-2` glyph, hover fill (gear, "⋯").
- *Destructive*: red text, borderless, behind a confirmation.
- *Segmented*: `--bg-group` track, selected segment `--bg` with a hairline shadow.
- *Menus and popovers* (`Popover.svelte`, `Menu.svelte`): the panel is moved to `<body>` and
  placed with `position: fixed` against its trigger, so no group (`overflow: hidden`) can clip it
  and nothing later in the page covers it. Opens below (or above with `up`), start- or end-aligned,
  flips when there is no room, stays 8px inside the window, follows scroll/resize, closes on Esc
  (anywhere) or a press outside. Menu sections are separated by hairlines; a check column marks
  the current choice.
- Text fields: `--bg-group` fill, no border, 3px accent focus ring. An in-place editor gets `--bg`
  fill and an accent ring.
- Icons (`Icon.svelte`): 16px grid, 1.3px stroke, round joins. The gear has eight square teeth and
  a hub so it can never read as a sun.
- Motion: 150ms ease-out; none with `prefers-reduced-motion`.

**Charts:** hand-drawn SVG sized to the real container width (text never scales). Accent line,
hollow points, light grid, labels in `--label-2` 11px, dashed reference lines with a halo-backed
label. Hover shows a small floating readout.

## Keyboard

Every action reachable by keyboard. Enter commits, Esc cancels and closes, Tab follows reading
order, Ctrl/Cmd+, toggles Settings. Visible focus ring on everything.

## Checklist before finishing UI work

- Could a visual (color, position, bar, arrow) replace a sentence or heading you added? Do that.
- Is there now more than one way to do this job? Remove one.
- Does the page use a wide window well and still work at phone width?
- Does every number come from a `CommandOutput`, with only formatting in the UI?
- Light and dark both checked; keyboard path works; `npm run check` clean.
