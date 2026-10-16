# UI design reference

## 1. Purpose and status

This is the reference the real Tauri and React UI is built from. It records the visual language, the layouts and the components that were settled during the mockup round, so the app does not have to be reverse-engineered from throwaway code. The mockups live on the branch `prototype/ui-mockups`, not on `main`. To look at them, check out that branch and open `apps/desktop/prototype-ui/index.html` in a browser. Add `?variant=A` to the address for the layout closest to the chosen design, and `&theme=light` or `&theme=dark` to force a theme. The prototype is not production code. Where this document and the prototype disagree, this document wins.

Decision from the review of the three variants: Main uses layout A ("Sidebar"). Inside A, the Review screen uses the segmented control (Original / Side by side / Result) and the page thumbnail strip from layout C. Setup and Settings are shared across all variants and stay as prototyped. Variants B and C are not built.

Product context is in [README.md](../README.md) and [architecture.md](architecture.md). Everything in this UI is read-only review plus conversion for v0.1. Editing in the Review screen is planned for v0.2.

## 2. Principles

- **Native, quiet utility.** The app should feel like a Finder, Preview or Windows 11 settings window, not a web dashboard.
- **One accent.** A single ink-teal accent marks the primary action, the current selection and progress. Status colours (green, amber, red) appear only on status.
- **Hairline borders, not cards.** Regions are separated by 1 px borders and a change of background. Panels are used for grouped settings and nothing else.
- **Compact desktop density.** 28 px controls, 30 px list rows, 13 px body text. No oversized padding or type.
- **Plain copy.** Short, specific, second person where needed, sentence case, no exclamation marks.
- **Keyboard first.** Every action is reachable without a mouse, and focus is always visible.
- **Both themes are first-class.** Light and dark are designed and checked together. Neither is derived from the other at the last minute.
- **Privacy is visible.** The sentence "Everything runs on this computer. Nothing is uploaded." appears where files are added and in setup.

Avoid:

- Gradients, glassmorphism, background blur, glowing or coloured shadows.
- Emoji, decorative icons, illustration blobs, mascots.
- A purple or indigo accent, neon in dark mode.
- Big rounded cards in a grid, hero sections, centred marketing copy.
- Oversized type (nothing above 20 px), pill-shaped buttons or tags.
- Product-hype words in copy: "magic", "seamless", "powerful", "smart".
- More than one icon set, or icons larger than 20 px.
- Animation beyond a 120 ms state change and a progress bar width.

## 3. Design tokens

Tokens are CSS custom properties on `:root`. The dark set is applied by `:root[data-theme="dark"]`. When no `data-theme` attribute is set, the app follows `prefers-color-scheme`, so the dark values are repeated under `@media (prefers-color-scheme: dark) { :root:not([data-theme]) { ... } }`. The theme choice in the app is System, Light or Dark; System removes the attribute. Set `color-scheme: light` or `dark` with each set so native controls and scrollbars match.

The prototype also defines `--desk` and `--proto-*` tokens for its own page chrome and floating control bar. They are not part of the app design and are omitted here.

### 3.1 Colour

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--bg` | `#F7F7F5` | `#1B1B1A` | Window background, title bar, column headers, status bar, page-canvas background behind scans |
| `--surface` | `#FFFFFF` | `#222221` | Content panes, action bar, panels, dialogs, popovers, form controls |
| `--sidebar` | `#F0EFEC` | `#181817` | Queue sidebar, Setup step list, Settings section list, thumbnail strip |
| `--hover` | `rgba(0,0,0,.04)` | `rgba(255,255,255,.05)` | Hover background on list rows, ghost and tertiary buttons |
| `--selected` | `#E3ECEF` | `#23353B` | Selected row in a list (current queue file, current settings section) |
| `--border` | `#E2E0DB` | `#33322F` | Hairlines between regions, panel borders, progress track |
| `--border-strong` | `#CFCCC5` | `#45433F` | Control borders (buttons, selects), dialog and popover borders, dashed drop target |
| `--text` | `#1D1D1B` | `#ECEAE5` | Primary text |
| `--text-2` | `#5F5D58` | `#AAA79F` | Secondary text, labels, metadata, inactive icons |
| `--text-3` | `#8E8B84` | `#7D7A73` | Decorative only: em-dash placeholders, empty-state glyphs. Never for text that must be read |
| `--accent` | `#1F6B85` | `#6BB4CC` | Primary button fill, links, selected toggle, progress fill, focus ring, converting status |
| `--accent-hover` | `#195A70` | `#86C4D8` | Primary button hover |
| `--accent-text` | `#FFFFFF` | `#0F1A1E` | Text and icons on an accent fill |
| `--accent-soft` | `#E3EEF2` | `#1F3239` | Accent tint: Recommended tag, pressed toggle button, highlighted result block, drag-over drop target |
| `--ok` | `#2F7A4B` | `#6CC08B` | Done status, check icons, "Runs well" |
| `--warn` | `#9A5B00` | `#E0A94F` | Needs attention, "Slow on this PC", Experimental tag |
| `--err` | `#B53A26` | `#E8826F` | Errors, "Won't fit", destructive button text, window close hover |
| `--ok-soft` | `rgba(47,122,75,.10)` | `rgba(108,192,139,.12)` | Background of the ok tag |
| `--warn-soft` | `rgba(154,91,0,.10)` | `rgba(224,169,79,.12)` | Background of the warn tag and the inline notice row |
| `--err-soft` | `rgba(181,58,38,.10)` | `rgba(232,130,111,.12)` | Background of the err tag |
| `--shadow` | `0 6px 24px rgba(0,0,0,.12)` | `0 6px 24px rgba(0,0,0,.45)` | Popovers, menus, dialogs, toasts. Nothing else |
| `--scrim` | `rgba(0,0,0,.28)` | `rgba(0,0,0,.5)` | Backdrop behind dialogs |

Derived colours used by controls, computed with `color-mix` against the surface:

| State | Value |
| --- | --- |
| Secondary button hover | `color-mix(in srgb, var(--text) 5%, var(--surface))` |
| Secondary button pressed | `color-mix(in srgb, var(--text) 9%, var(--surface))` |
| Option row or table row hover | `color-mix(in srgb, var(--text) 3%, var(--surface))` |
| Block overlay border | `--accent` at 80%, fill `--accent` at 7% |
| Block overlay, highlighted | border `--accent`, fill `--accent` at 20% |

### 3.2 Typography

| Property | Value |
| --- | --- |
| `--font-ui` | `-apple-system, BlinkMacSystemFont, "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif` |
| `--font-mono` | `ui-monospace, "Cascadia Mono", "SF Mono", Consolas, monospace` |
| Numbers | `font-variant-numeric: tabular-nums` on the whole app, so sizes, times, counts and percentages do not jitter |
| Weights | 400 regular, 500 medium (buttons, tags, emphasis), 600 semibold (titles, file names). No other weights |
| Case | Sentence case everywhere. No all-caps labels |
| Smoothing | `-webkit-font-smoothing: antialiased` |

| Role | Size | Weight | Line height | Where |
| --- | --- | --- | --- | --- |
| Meta | 12 px | 400 or 500 | 1.45 | Labels above fields, tags, table headers, sizes, hints, status bar, title bar text |
| Body | 13 px | 400 | 1.45 | Default text, buttons, selects, queue rows |
| Emphasis | 14 px | 400 or 600 | 1.45 | Primary line in a settings row, lede paragraphs, file names in detail views, status headline |
| Section title | 16 px | 600 | 1.35 | `h2`: file name in the detail view, settings subsections, empty-state heading (weight 500) |
| Page title | 20 px | 600 | 1.3 | `h1`: Setup and Settings page titles. Used once per view |
| Result document | 14 px | 400 | 1.55 | Text in the Review result pane, max width 640 px. Table text in the result is 12 px |
| Mono | 12 px | 400 | 1.45 | Paths, output file names, block coordinates |

### 3.3 Spacing

Scale: **4, 8, 12, 16, 24, 32** px. Setup page gutters use 48 px. The prototype also uses 2 px (gap between list rows and inside segmented controls) and 6 px (gap between an icon and its label inside a button). Keep those as the only exceptions.

| Context | Value |
| --- | --- |
| Gap between buttons in a group | 8 px |
| Gap between a label and its control | 8 px (inline), 4 px (stacked) |
| Row padding in a panel | 12 px vertical, 16 px horizontal |
| Page padding, detail view | 24 px vertical, 32 px horizontal |
| Page padding, Settings | 24 px top, 32 px sides and bottom |
| Page padding, Setup | 32 px vertical, 48 px horizontal |
| Space between sections | 24 px |

### 3.4 Radii

| Token | Value | Use |
| --- | --- | --- |
| `--r-control` | 5 px | Buttons, selects, list rows, notices |
| `--r-panel` | 7 px | Panels, dialogs, popovers, menus, drop target, toasts |
| (fixed) | 4 px | Tags |
| (fixed) | 3 px | Active segment inside the segmented control |
| (fixed) | 2 px | Progress bar, block overlays, scanned page image |
| (fixed) | 9 px and 50% | Switch track and thumb, step number circles |

### 3.5 Control heights and fixed sizes

| Element | Size |
| --- | --- |
| Title bar | 36 px high, window buttons 46 px wide |
| Control (button, select, icon button) | 28 px |
| Primary button | 30 px high, 16 px horizontal padding (14 px in the action bar) |
| Small button | 24 px, 12 px text |
| Sidebar, step and settings rows | 30 px |
| Queue sidebar | 240 px wide, header 40 px |
| Action bar | 56 px |
| Review toolbar | 40 px minimum |
| Pane header (Original, Result) | 28 px |
| Page thumbnail | 64 x 88 px image |
| Switch | 32 x 18 px |
| Checkbox, radio | 14 px |
| Progress bar | 4 px |
| Icons | 16 px default, 14 px small, 12 px in carets and step marks, 20 px maximum (page-level glyphs only) |

### 3.6 Shadows and borders

- Shadow is `--shadow` on popovers, menus, dialogs and toasts only. Panes, panels and the action bar have none.
- Borders are 1 px. `--border` for structure, `--border-strong` for anything the user can interact with or that floats.
- The segmented control's active segment has a 1 px `--border-strong` ring in place of a shadow.

### 3.7 Focus ring

`:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }`. Plain `:focus` gets no outline, so mouse clicks show nothing. The ring is never removed from a control. Inside the dark floating elements it still uses `--accent`.

### 3.8 Motion

- 120 ms `ease-out` on `background-color`, `border-color`, `color`, `width` and `transform`, applied to buttons, list rows, option rows, block overlays, thumbnails, switches and progress fill.
- No other animation: no spinners that spin on an idle element, no stripes, no slides, no fades on screens.
- Everything above is wrapped in `@media (prefers-reduced-motion: no-preference)`. With reduced motion, state changes are instant.

## 4. Layout

### 4.1 Window structure

```
+----------------------------------------------------------------------+
| [icon] OpenDoc Reader                                  [-]  [ ]  [x] |  36 px, --bg
+----------------------------------------------------------------------+
|                                                                      |
|  app area: one of Setup, Main, Review (inside Main) or Settings      |
|                                                                      |
+----------------------------------------------------------------------+
```

- A single window. There is no browser-style navigation bar and no tab strip.
- The title bar is 36 px, `--bg`, with a 1 px bottom border. Left: the app glyph in `--accent` and "OpenDoc Reader" in 12 px `--text-2`. Right: flat minimize, maximize and close buttons, 46 px wide, 10 px glyphs drawn as 1 px strokes. Hover is `--hover`. Close hover is `--err` with `--accent-text`.
- Screens: Setup (first run only, shown until the download finishes), Main, Settings. Review is a state of Main. Settings opens in place of Main and a Back button returns to it. If Settings was opened during Setup, Back returns to Setup.
- Dialogs sit over the window with a `--scrim` backdrop. Toasts appear at the bottom left of the app area.
- The window is sized for 1280 x 800 and has been checked at 1200 px wide.

### 4.2 Main (layout A, "Sidebar")

```
+----------------------+-----------------------------------------------------+
| Queue  5 files       |                                                     |
|        [Clear done][+]|  +----------+  Statistical abstract 1920.pdf       |
+----------------------+  | preview  |  48 pages . 41.8 MB                   |
| [pdf] Library sc.. ok |  | (220 px) |                                       |
| [pdf] Statistical 64% |  |          |  Converting page 31 of 48            |
| [img] calculus-no.. . |  |          |  [=========-----------]              |
| [pdf] Invoice 202.. ! |  +----------+  About 2 min 10 s left               |
| [img] receipt.heic .  |               [Pause all] [Cancel all]              |
|                       |               Type     PDF document                 |
|                       |               Pages    48                           |
|                       |               Size     41.8 MB                      |
|                       |               Model    Accurate                     |
+----------------------+-----------------------------------------------------+
| [gear] Settings      | Format [x]Word [x]Markdown  Model [Accurate v]       |
|                      | Save to [Next to the original v]  [Pause all]        |
|                      | [Cancel all]                       [Convert 3 files] |
+----------------------+-----------------------------------------------------+
```

The sidebar is 240 px wide on `--sidebar` with a right hairline. The action bar is 56 px and spans the main area only, not the sidebar.

**Sidebar**

- Header (40 px): "Queue" in 12 px semibold `--text-2`, the file count in 12 px, then the tertiary small button "Clear done" (disabled when nothing is done) and an icon button "+" with `aria-label="Add files"`.
- Rows (30 px, 8 px horizontal padding): file-type icon (`file-pdf` or `file-image`) in `--text-2`, the file name truncated with an ellipsis, and a status indicator pinned right. A converting file shows its percentage in `--accent`. A done file shows a green check. Other states show their icon (see 5.7). The current file has `--selected` background and `aria-current="true"`.
- Footer: a row with a gear icon and "Settings", above a top hairline.
- Empty queue: "No files in the queue." in 12 px `--text-2`.
- Single click selects a file. Double click on a done file opens Review. Enter on a focused done file opens Review.

**Main area, with a file selected**

- A scrolling detail view with 24 px / 32 px padding. Left: the first-page preview, 220 px wide, 1 px `--border`, `--bg` behind it. Right (max 560 px wide, 16 px vertical gaps): file name (`h2`, 16 px semibold), then "N pages . size" in `--text-2`, then the status block, then a key-value list (Type, Pages, Size, Model; labels 12 px `--text-2` in an 88 px column).
- The status block depends on state:
  - Converting: "Converting page 31 of 48" in 14 px semibold `--accent`, a 4 px progress bar, "About 2 min 10 s left" in 12 px `--text-2`, then "Pause all" and "Cancel all".
  - Done: "Finished in 14 s" in green with a check, "Saved next to the original:" and the output names in mono, then "Review" (columns icon) and "Show in folder".
  - Waiting: "Waiting in the queue" and "It starts after the files above it are done."
  - Needs attention: the inline notice (5.8) with "Use existing text" and "Run OCR anyway".
  - Cancelled: "Cancelled" and an "Add again" button.

**Main area, empty queue**

The drop target fills the main area (5.10). The action bar stays, with Convert disabled.

**Action bar**

Left to right, 12 px padding, 12 px between groups: Format (label "Format", checkboxes Word and Markdown, at least one stays checked), Model (select; models that are not installed are listed as "Fast (not installed)" and disabled), Save to (select: "Next to the original", the chosen folder, "Choose folder..."). A flexible spacer, then "Pause all" and "Cancel all" (secondary, with icons, shown while something is converting; "Pause all" becomes "Resume all" while paused), then the primary button "Convert N files" ("Convert" when nothing is waiting, disabled when no file is waiting). The primary button is the only one on screen.

### 4.3 Review (inside layout A)

Review replaces the main area. The queue sidebar stays, with the reviewed file highlighted, so another done file can be opened with one click. Selecting a file that is not done leaves Review. The control set is C's: a segmented control and a page thumbnail strip.

```
+----------------------+-----------------------------------------------------+
| Queue  5 files       | [<- Queue] Library scans.pdf  [Original|Side|Result]|
|                      |  <  Page 2 of 3  >  [-] 100% [+]  [Blocks]          |
| [pdf] Library sc.. ok|                  [Show in folder][Export Markdown]  |
| [pdf] Statistical 64%|                                      [Export Word]  |
| ...                  +-----------------------------------------------------+
|                      | [p1] [p2] [p3]                    thumbnail strip   |
|                      +--------------------------+--------------------------+
|                      | Original                 | Result . Accurate (...)  |
|                      | +----------------------+ |  No. 2                   |
|                      | | scan with block      | |  Paragraph text ...      |
|                      | | overlays             | |  +--------+--------+     |
|                      | |                      | |  | table  | table  |     |
|                      | +----------------------+ |  Formula  x^2 + ...  (4) |
+----------------------+--------------------------+--------------------------+
| [gear] Settings      |
+----------------------+
```

At 1280 px the toolbar does not fit in one row, so it wraps to two rows (view and page controls first, export actions right-aligned). The toolbar has a 40 px minimum height and `--surface` background.

**Toolbar, left to right**

1. "Queue" ghost button with an arrow-left icon: returns to the Main detail view.
2. File name, semibold, truncated.
3. Segmented control: **Original / Side by side / Result**. Side by side is the default on opening. Side by side and Result are disabled, with a tooltip "Available when the file is done", for files that are not done.
4. Pager: previous and next icon buttons (`aria-label` "Previous page", "Next page") around the label "Page 2 of 3". Buttons disable at the ends.
5. Zoom: zoom-out and zoom-in icon buttons around the value, 50 to 200 percent in 25 percent steps, default 100. Zoom applies to the original page only, so it is disabled in Result mode.
6. "Blocks" toggle (ghost button with a selection icon, `aria-pressed`): shows or hides the block overlays on the original and the block-type labels in the result. On by default.
7. Spacer, then "Show in folder" (tertiary), "Export Markdown" (secondary) and "Export Word" (primary, the only primary on the screen).

**Page thumbnail strip**

A horizontal strip above the panes, `--sidebar` background, 12 px padding and 12 px gaps, with a bottom hairline, scrolling horizontally. Each thumbnail is the 64 x 88 px page image, cropped from the top, with the page number below in 12 px. The current page has a 2 px `--accent` outline and a semibold number. Each is a button with `aria-label="Page N"`. For long documents the strip scrolls and keeps the current page in view. The prototype caps the strip at 12 pages and does not handle more (see section 8).

**Panes**

- Side by side: two equal columns separated by a hairline. Each has a 28 px header on `--bg`: "Original", and "Result" followed by a dot and the model, for example "Accurate (PaddleOCR-VL 1.6)".
- Original pane: `--bg` background, 16 px padding, the page image centred at the zoom width with a 1 px border. Block overlays are positioned in percentages of the image so they survive zoom.
- Result pane: `--surface` background, 24 px / 32 px padding, the document at 14 px / 1.55 and max 640 px wide.
- Original mode shows only the original pane, Result mode only the result pane, each full width.
- Panes scroll in sync by proportion: scrolling either pane sets the other to the same fraction of its scroll height. Programmatic jumps (below) suspend syncing for 300 ms so they do not fight it.

**Block overlay and hover**

- Each detected block is a box on the original: 1 px `--accent` border at 80%, 7% fill, 2 px radius. Page furniture (running head, page number) uses a dashed border and no fill.
- Hovering a block in either pane highlights its counterpart in both: the overlay gets a 20% fill and a solid border, the result block gets an `--accent-soft` background.
- Clicking an overlay scrolls the result pane to that block, centred. Clicking a result block scrolls the original to its box.
- With Blocks on, running heads and page numbers in the result carry a small semibold label ("Running head", "Page number") and a tooltip: "Left out of the export." With Blocks off, the labels and overlays disappear and the text remains.
- A block's tooltip or info shows its type and, for tables, the row count.

**Result blocks**

Headings and titles at 16 px semibold, paragraphs at 14 px, captions and notes at 12 px `--text-2`, figures as an image cropped from the page with a 1 px border, formulas rendered with KaTeX and centred with their equation number on the right, tables as real tables with merged cells (1 px `--border-strong` grid, 12 px text, header cells on `--bg`, numbers right-aligned, last row semibold) inside a horizontally scrolling wrapper so wide tables never stretch the pane.

**Export actions**

"Export Word" and "Export Markdown" write the files and show a toast, for example "Saved Library scans.docx next to the original." Existing files are never overwritten (a numeric suffix is added). "Show in folder" opens the output folder in the OS file manager.

### 4.4 Setup

Four steps, shown once at first run. A quiet shell, not a large wizard.

- Left rail, 240 px, `--sidebar`: the app glyph and name, then the four steps as 30 px rows, each with a small numbered circle (18 px) and a label: Welcome, Your computer, Choose a model, Download. The current step is semibold with an accent-filled circle. Completed steps show a check on an `--accent-soft` circle and can be clicked to go back. Upcoming steps are disabled. During the download, going back is disabled.
- Right pane on `--surface`: a scrolling body with 32 px / 48 px padding and content limited to 640 px, and a fixed footer with a top hairline holding the buttons. The body starts with "Step 2 of 4" in 12 px `--text-2`, then the `h1`, then a 14 px `--text-2` lede.
- Footer: Back (tertiary) at the left, the primary button at the right. Only one primary per step.

| Step | Content | Primary button |
| --- | --- | --- |
| 1. Welcome | Title "Welcome to OpenDoc Reader", one sentence on what the app does, the privacy sentence, and a numbered list of the three things that happen next | Get started |
| 2. Your computer | A panel with four rows (Graphics, Memory, Processor, Free space), each with an icon, a label, the detected value and a green check, then the summary "This computer can run every model." | Continue |
| 3. Choose a model | Option rows for Fast, Accurate, Max, with the suggester's pick marked Recommended (accent border; the mockup shows Accurate, but on a GPU with 8 GB or more the manifest rules pick Max), each with a checkbox, name and model, best-for line, download size and speed on this PC, a fit tag and a "Model details" button. Accurate is pre-checked. An "Advanced models" disclosure holds DeepSeek-OCR 2 (Experimental, Community conversion) and a legend that explains the three fit tags. A storage row: "Models are stored in `G:\OpenDoc Reader\models`" with "Change..." | Download 2.6 GB, with the breakdown "Model 1.8 GB, runtime and engine 0.8 GB" beside it (the total changes with the selection, and the button is disabled with no model selected) |
| 4. Download | One row per component (runtime, document engine, each model) with name, speed, a 4 px progress bar, "done / total GB" and time left. "Pause" and "Resume". Then the calibration line: "Testing speed on a sample page..." changing to "About 4.8 s per page on this computer." | Start using OpenDoc Reader (disabled until calibration finishes) |

The recommended option explains itself in one line, for example "Best balance of accuracy and speed on this computer." Build that line from the probe result and the measured speed table rather than fixed text, and do not repeat the speed already shown in the meta line.

### 4.5 Settings

Opened from the gear row at the bottom of the queue sidebar. Layout: a 220 px `--sidebar` list at the left with a Back ghost button at the top and four 30 px section rows (icon and label, current row on `--selected`), and a `--surface` content area at the right with 24 px / 32 px padding and content limited to 720 px. Each section has an `h1`. Settings are grouped in panels of 44 px minimum rows: label at 14 px, optional description at 12 px `--text-2`, control at the right. Changes apply immediately. There is no Save button.

| Section | Contents |
| --- | --- |
| General | Default format (Word and Markdown checkboxes). Default model (select). Save location: radio "Next to the original" or "Always this folder: `path`", with "Change...". "Check for new versions once a month" switch, on by default, with the note "Only asks GitHub whether a newer version exists. Nothing about you is sent." |
| Models | **Installed:** table (Name, Tier tag, Size, License, Remove). **Available:** the same table with Install. **Storage:** models folder path with "Change...", and "Import model from folder..." with the note "For computers that can't download. Files are checked before use." **Advanced models:** DeepSeek-OCR 2 with its Experimental and Community conversion tags, "3.6 GB, Apache-2.0. Needs about 7 GB of graphics memory. No quality advantage over Max was measured." and an Install or Remove button. Remove asks for confirmation in a dialog. |
| Privacy and logs | "OpenDoc Reader has no telemetry." with a short explanation. Logs: folder path and "Open logs folder". Diagnostics: "Copy diagnostics", which shows a quiet inline "Copied" with a check for about 2.5 s. |
| About | Name, "Version 0.1.0 (pre-alpha)", link to the source code, licenses note: the app is MIT, models keep their own licenses (Apache-2.0 and MIT), with a "View licenses" link. |

## 5. Components

### 5.1 Buttons

| Kind | Look | Use |
| --- | --- | --- |
| Primary | `--accent` fill, `--accent-text`, no border, 30 px high, hover `--accent-hover` | The one main action in a view: Convert, Export Word, Get started, Continue, Download, Start using OpenDoc Reader |
| Secondary | `--surface` fill, 1 px `--border-strong`, `--text`, 28 px high, weight 500. Hover 5% and pressed 9% tint | Other actions: Pause all, Cancel all, Export Markdown, Review, Pause, Change... |
| Tertiary | Transparent, `--accent` text, 8 px padding, hover `--hover` | Low-weight actions and links in context: Back, Clear done, Show in folder, Model details |
| Ghost | Transparent, `--text`, hover `--hover` | Toolbar actions that are not links: Queue (back), pager and zoom icons, Blocks |
| Icon | 28 px square, no label | Add files, pager, zoom, close. Always has `aria-label` and a `title` |
| Danger | Secondary look with `--err` text | Remove |
| Small | 24 px high, 12 px text | Inside tables and rows |

Rules:

- One primary button per view. If two actions seem to compete, one is secondary.
- A toggle button (Blocks) uses `aria-pressed`. Pressed is `--accent-soft` fill, `--accent` border and text.
- Disabled is 50% opacity. Keep disabled buttons visible so the screen layout does not move, and say why in a tooltip when it is not obvious.
- Icons are Phosphor regular, 16 px, placed before the label with a 6 px gap. Not every button needs one.

### 5.2 Select

A native `select` with the system appearance removed, 28 px high, 10 px left padding, 28 px right padding, 120 px minimum width, 1 px `--border-strong`, 5 px radius, and a Phosphor caret-down (12 px, `--text-2`) over the right edge. Every select has an `aria-label` or a visible label. In the action bar, labels are 12 px `--text-2` text before the control.

### 5.3 Checkbox, radio, switch

- Checkbox and radio: native, 14 px, `accent-color: var(--accent)`, label to the right with a 6 to 8 px gap, 28 px row height.
- Switch: 32 x 18 px, `role="switch"` and `aria-checked`, label by `aria-labelledby`. Off is `--border-strong` track with a `--surface` thumb. On is `--accent` track with an `--accent-text` thumb moved 14 px. Use a switch for a setting that applies immediately (Settings only). Use checkboxes for choices that are submitted together (formats, models).

### 5.4 Segmented control

A group with `role="group"` and an `aria-label` ("View"), containing 22 px high buttons with `aria-pressed`, in a 1 px `--border-strong` frame with 2 px padding and 2 px gaps on `--bg`. Inactive segments are `--text-2`, weight 500. The active segment has `--surface` background, `--text`, and a 1 px `--border-strong` ring. Use it for mutually exclusive views, only in Review.

### 5.5 Progress bar

4 px high, 2 px radius, `--border` track, `--accent` fill, width driven by a custom property (`--p`). When complete the fill turns `--ok`. No stripes and no animation other than the 120 ms width change. Next to a bar, show the numbers in text ("31 of 48", "0.91 / 1.80 GB", "About 2 min left"), so the bar is never the only source of information. Bars carry `role="progressbar"` with `aria-valuenow`, `aria-valuemin` and `aria-valuemax`.

### 5.6 Option row (Setup)

A bordered row (1 px `--border`, 7 px radius, 12 px / 16 px padding, 8 px between rows) with a checkbox at the left, a body, and a right column. The body holds the name (14 px semibold) with the model name in 12 px `--text-2` and any tags, a best-for line in `--text-2`, a 12 px meta line (download size and speed on this PC, 16 px apart), and for the recommended option a reason line in `--accent`. The right column holds the fit tag and the "Model details" button. The recommended option has an `--accent` border. The whole name is a label for the checkbox.

### 5.7 Status indicators

Status is always an icon plus a word. In the narrow queue sidebar the word is replaced by a tooltip and a visually hidden text, and the icon and colour carry the state.

| State | Icon (Phosphor) | Colour | Text in detail view | Sidebar |
| --- | --- | --- | --- | --- |
| Done | `check-circle` | `--ok` | "Finished in 14 s" (list views: "Done") | Icon |
| Converting | `circle-notch` in lists, none in the detail view | `--accent` | "Converting page 31 of 48" | Percentage, for example "64%" |
| Waiting | `clock` | `--text-2` | "Waiting in the queue" | Icon |
| Needs attention | `warning` | `--warn` | The reason, for example "This PDF already contains text." | Icon |
| Cancelled | `prohibit` | `--text-2` | "Cancelled" | Icon |

### 5.8 Fit badges and tags

Tags are 18 px high, 6 px horizontal padding, 4 px radius, 12 px weight 500, no wrap.

| Tag | Style | Meaning |
| --- | --- | --- |
| Runs well | `ok`: `--ok-soft` fill, `--ok` text, with a small check icon | Fast enough for everyday use on this computer |
| Slow on this PC | `warn`: `--warn-soft` fill, `--warn` text | Works, but expect long waits per page |
| Won't fit | `err`: `--err-soft` fill, `--err` text | Not enough memory. Cannot be installed here |
| Recommended | `accent`: `--accent-soft` fill, `--accent` text | The suggester's pick. At most one per list |
| Experimental | `warn` | Model is not part of the default set |
| Community conversion | Neutral: transparent with a 1 px `--border-strong` border, `--text-2` text | Model files were not published by the original authors |
| Tier (Fast, Accurate, Max) | Neutral | Used in tables |

A "Won't fit" model keeps its row but its checkbox is disabled. The fit legend explains the three states; it is shown under Advanced models in Setup.

### 5.9 Inline notice row

A notice about one item that needs a decision. 12 px / 16 px padding, 1 px `--border`, a 3 px `--warn` left border, `--warn-soft` fill, 5 px radius. Content: a status line in semibold `--warn` with the warning icon (for example "This PDF already contains text."), a one-line explanation in `--text-2` ("Using the existing text is instant and exact. Running OCR reads the page images instead."), and the actions as secondary buttons ("Use existing text", "Run OCR anyway"). In list-style layouts the notice may be a single row with the same text and buttons side by side. Notices are not toasts and do not time out.

### 5.10 Empty state and drop target

A dashed 1 px `--border-strong` panel with 7 px radius on `--surface`, filling the main area with a 24 px margin. Centre: a 20 px upload glyph in `--text-2`, "Drop PDFs or images here" (`h2`, weight 500), a secondary button "or choose files...", and a 12 px `--text-2` line, max 420 px: "PDF, PNG, JPG, TIFF (multi-page), BMP, WebP and HEIC. Folders work too. Everything runs on this computer. Nothing is uploaded." While a file is dragged over, the border is `--accent` and the fill `--accent-soft`. The same component is the drop target in the app whenever the queue is empty.

### 5.11 Popover, menu, dialog, toast

- **Popover:** 340 px wide, 12 px / 16 px padding, 1 px `--border-strong`, 7 px radius, `--shadow`. Used for "Model details": a key-value list with Model, License, Source (link) and Download. Opens on click, closes on outside click or Escape, `role="dialog"` with an `aria-label`, and the opening button has `aria-expanded`.
- **Menu:** 190 px minimum, 4 px padding, same border, radius and shadow as the popover, 28 px items, `--hover` on hover.
- **Dialog:** 400 px wide, 20 px padding, centred horizontally and 30% from the top, over a `--scrim`. A title (`h2`) and one or two sentences, with buttons right-aligned 20 px below, Cancel first and the action last. Used only for destructive confirmations, for example "Remove Accurate?" with "This frees 1.8 GB on G:. You can install it again later." and the buttons "Cancel" and "Remove". Focus is trapped and Escape cancels.
- **Toast:** bottom left, 16 px from the edges, 420 px maximum, `--surface` with 1 px `--border-strong`, `--shadow`, a green check and one line of text, `role="status"`, dismissed after about 3 seconds. For confirmations only, never for errors or decisions.

### 5.12 Panels and rows

A panel is `--surface` with 1 px `--border` and 7 px radius, holding rows separated by hairlines. A row has a flexible text block (14 px label, 12 px `--text-2` description) and its control at the right. Panels are used in Setup step 2 and 4 and in Settings. They are not used for the queue or for Review.

## 6. Copy guidelines

- Plain, short, specific. Say what the app is doing or what the user can do.
- Sentence case for all labels, titles and buttons. Proper names keep their case (Word, Markdown, PDF, HEIC).
- Second person ("You can install it again later") or no pronoun. No "we".
- No exclamation marks, no emoji, no hype words.
- Buttons are verbs or short noun phrases: "Convert 3 files", "Export Word", "Show in folder", "Use existing text". An ellipsis (three dots in the real app, a single "..." character is acceptable) means the action opens a picker or asks for more input: "Change...", "or choose files...".
- Numbers and units: "1.8 GB", "about 5 s per page", "Page 2 of 3", "48 pages". Use "about" for estimates. Do not hide estimates as precise values.
- Say what privacy means where it matters: no cloud, no telemetry, nothing sent.
- Errors say what happened and what to do next. Do not blame the user, and do not show raw exception text as the message.
- Plural forms are handled by the string, not the layout: "1 file" and "3 files".

Strings taken from the prototype, as examples:

| Place | String |
| --- | --- |
| Drop target | "Drop PDFs or images here", "or choose files..." |
| Privacy | "Everything runs on this computer. Nothing is uploaded." |
| Progress | "Converting page 31 of 48", "About 2 min 10 s left", "Waiting in the queue" |
| Done | "Finished in 14 s", "Saved next to the original:" |
| Notice | "This PDF already contains text.", "Use existing text", "Run OCR anyway" |
| Queue | "Convert 3 files", "Pause all", "Resume all", "Cancel all", "Clear done", "No files in the queue." |
| Setup | "This computer can run every model.", "Testing speed on a sample page...", "About 4.8 s per page on this computer.", "Start using OpenDoc Reader" |
| Models | "Best balance of accuracy and speed on this computer.", "Models are stored in G:\OpenDoc Reader\models", "Needs about 7 GB of graphics memory." |
| Fit | "Runs well", "Slow on this PC", "Won't fit" |
| Review | "Page 2 of 3", "Export Word", "Export Markdown", "Left out of the export." |
| Settings | "Only asks GitHub whether a newer version exists. Nothing about you is sent.", "OpenDoc Reader has no telemetry.", "Copied" |
| Toasts | "Saved Library scans.docx next to the original.", "Used the text already in the PDF.", "Accurate is installed." |
| Dialog | "Remove Accurate?", "This frees 1.8 GB on G:. You can install it again later." |

## 7. Accessibility

Target: WCAG 2.2 level AA.

**Contrast.** Measured from the token values. Normal text needs 4.5:1, large text and meaningful non-text elements 3:1.

| Pair | Light | Dark |
| --- | --- | --- |
| `--text` on `--surface` | 16.9 | 13.2 |
| `--text-2` on `--surface` | 6.6 | 6.6 |
| `--text-2` on `--sidebar` | 5.7 | 7.4 |
| `--text-2` on `--selected` | 5.5 | 5.3 |
| `--accent` on `--surface` | 6.0 | 6.8 |
| `--accent` on `--accent-soft` | 5.1 | 5.7 |
| `--accent-text` on `--accent` | 6.0 | 7.6 |
| `--ok` on `--ok-soft` (over surface) | 4.6 | 5.8 |
| `--warn` on `--warn-soft` (over surface) | 4.8 | 6.0 |
| `--err` on `--err-soft` (over surface) | 5.0 | 4.9 |
| `--text-3` on `--surface` | 3.4 (fails) | 3.7 (fails) |

- `--text-3` fails AA for text in both themes. Use it only for decoration (placeholder dashes, empty glyphs). Never for a label, value, hint or disabled state that has to be read.
- Status text on the sidebar background (`--ok` is 4.6, `--warn` 4.7) is close to the limit. Do not use a lighter shade of any status colour.
- If a token value changes, re-check this table in both themes.

**Not by colour alone.** Every status has an icon and a word. Fit tags carry text. The recommended option has a tag as well as an accent border. Block overlays do not rely on colour only: page furniture uses a dashed border.

**Focus.** All interactive elements show the 2 px accent focus ring on `:focus-visible`, in both themes. Focus order follows reading order: sidebar, then main area, then action bar. Dialogs trap focus and return it to the opener. In a destructive dialog, initial focus goes to Cancel. (The prototype focuses the last button, which is the destructive one. Do not copy that.)

**Keyboard.**

- Everything in the prototype is reachable with Tab, Enter and Space. Escape closes a dialog, menu or popover.
- Enter on a done file in the queue opens Review.
- The queue should behave as one list: Up and Down move between files (roving tab index), Enter opens. The prototype tabs through each row instead.
- Proposed shortcuts, not in the prototype: add files (Ctrl or Cmd+O), open Settings (Ctrl or Cmd+comma), previous and next page in Review (Left and Right, or Page Up and Page Down), zoom (Ctrl or Cmd with plus and minus), Escape returns from Review to the queue.
- Block overlays and result blocks are mouse-only in the prototype. The real app must make blocks reachable by keyboard, for example by letting the result blocks take focus and highlighting their overlay, so the block-to-block link is not pointer-only.
- Drag and drop always has the "or choose files..." button as an equivalent.

**Names and roles.**

- Icon-only buttons have `aria-label` and `title`: "Add files", "Previous page", "Next page", "Zoom out", "Zoom in", "Close review", "Settings".
- Decorative icons are `aria-hidden`. Status icons have a visually hidden text.
- Setup steps are an ordered list with `aria-current="step"`. Settings sections use `aria-current="true"`. The segmented control and the Blocks toggle use `aria-pressed`. Switches use `role="switch"`. The popover and dialogs have an accessible name.
- Progress bars have `role="progressbar"` and a value. Changes in conversion status are announced politely (a `role="status"` region), without announcing every page.
- Live regions: toasts (`role="status"`), and the calibration line in Setup (`aria-live="polite"`).

**Motion and zoom.** `prefers-reduced-motion` removes all transitions. The layout must work with the OS text size increased and at 200% page zoom without loss of function, scrolling inside panes rather than clipping.

## 8. Open items

The prototype did not settle these. Each needs a decision before or during the build.

1. **Title bar on macOS and Windows.** The prototype draws Windows-style controls on the right. macOS needs the traffic lights on the left with the title centred or hidden. Decide whether to use native decorations on both platforms or a custom title bar with platform-specific control placement.
2. **Window minimum size.** The prototype assumed 1200 x 760 and was checked only down to 1200 px wide. Decide the real minimum, how the Review toolbar and the action bar collapse below it (the action bar is already tight), and whether the sidebar can collapse.
3. **Page furniture in exports.** The prototype labels running heads and page numbers as "Left out of the export." Decide whether that is the real rule, whether it is configurable, and how headers, footers and footnotes appear in Word and Markdown (omitted, kept as a section header and footer, or kept inline). Review should show the same rule.
4. **Theme setting.** The app follows the system theme, and the prototype switches theme with its own control. Decide where System, Light and Dark live in Settings (probably General).
5. **Fast tier language.** The architecture describes a language dropdown for the Fast tier. The prototype has no place for it. Decide whether it sits in the action bar when Fast is selected or in Settings.
6. **Errors and failure states.** Not designed: a file that fails to convert, a corrupt or password-protected PDF, a failed or interrupted download (with retry and a checksum mismatch), not enough disk space, the model failing to load, running out of graphics memory. The inline notice row is the starting point, but each needs copy and an action.
7. **Queue behaviour.** Removing a single file, reordering, retrying a failed file, selecting several files, what "Cancel all" confirms, and what a folder drop looks like (flatten into files, or a group row).
8. **Long documents in Review.** The prototype review is built on three sample pages. Decide how the thumbnail strip behaves with hundreds of pages (virtualisation, a page-number field, jumping to a page), and whether the result pane shows one page at a time or a continuous scroll.
9. **Vertical or horizontal thumbnail strip.** This document specifies C's horizontal strip. In A's two-pane split a vertical strip beside the original may use space better on tall windows. Confirm once the real pages are on screen.
10. **Zoom and fit.** Zoom is a percentage on the original only. Decide whether "Fit width" and "Fit page" are needed, and whether zoom should apply to both panes.
11. **Block information.** The prototype shows block type and coordinates in an inspector that only exists in variant C. Decide whether A needs a block info line (a status line under the toolbar, or a tooltip) or whether the hover highlight is enough.
12. **Original mode before conversion.** In Original mode for a file that has not finished, the prototype shows only the first-page preview. Decide whether every page of an unconverted file can be previewed, and whether to build the preview from PDFium at 200 DPI as in the pipeline.
13. **Review in v0.2.** Editing text in the result pane changes the model of the result pane (focus, selection, undo). Reserve space for it, but do not design it now.
14. **Fit and speed text.** The "about 5 s per page" figures come from a measured table, then from calibration. Decide how to show an estimate that is still unmeasured (for example Apple Silicon) without sounding precise.
15. **Localisation.** All copy is English. Allow about 30% extra width in buttons, tags and the action bar for translated strings.
