# Ramen Design Document

## 1. Product Purpose & Targets

**Ramen** is an ultra-fast native desktop file search utility (an Everything replacement powered by the [`goz`](https://github.com/mustafaahci/goz) NTFS MFT + USN journal daemon `v0.7.1`), built on **Native Shell (NSH 0.6.1)** and the shared **`Bento`** UI/desktop toolkit (`bento`).

- **Primary target**: Windows desktop (`minSize: 820x540`, native hidden `48px` title bar, system tray, optional global summon hotkey, pointer + keyboard-first workflow).
- **Secondary compatibility**: macOS and Linux desktop builds compile cleanly and display a clear platform-support state explaining that NTFS MFT indexing via `gozd` is Windows-only.
- **Visual family**: Shares the `Bento` / `Wonton` / `Sundae` desktop family design language — quiet, high-contrast Light & Dark semantic tokens (`theme.json`), `0.5rem` (`8px`) base radius, crisp hairlines, stable alignment spines, and virtualized result rows (`virtual_list`) for zero-jank browsing across tens of thousands of instant file matches.

---

## 2. Design System & Semantic Tokens

All colors in presentation code are consumed strictly via `nsh_base::theme()` and `bento::ui` helpers. Zero raw `rgb(0x...)` or hex literals belong in `src/`.

| Role | Light (`theme.json`) | Dark (`theme.json`) | Usage |
|---|---|---|---|
| `background` / `foreground` | `#f8fafd` / `#1f1f1f` | `#0e1117` / `#e3e3e3` | Window root surface & primary copy |
| `sidebar` / `sidebar-foreground` | `#f0f4f9` / `#1f1f1f` | `#111620` / `#e3e3e3` | Title bar chrome & bottom status bar surface |
| `card` / `card-foreground` | `#ffffff` / `#1f1f1f` | `#151b26` / `#e3e3e3` | Search container, service onboarding card, list container |
| `popover` / `popover-foreground` | `#ffffff` / `#1f1f1f` | `#1b2230` / `#e3e3e3` | Modal dialogs, sort/syntax popovers, context menus, toast |
| `primary` / `primary-foreground` | `#1a73e8` / `#ffffff` | `#8ab4f8` / `#062e6f` | Primary CTA (Install/Start Service, Confirm), selected chip |
| `secondary` / `secondary-foreground` | `#e8eef7` / `#1f1f1f` | `#232d3f` / `#e3e3e3` | Secondary actions, hotkey keycap badges |
| `muted` / `muted-foreground` | `#f0f4f9` / `#444746` | `#1b2230` / `#9aa8be` | Subtle containers, parent directory paths, file metadata |
| `accent` / `accent-foreground` | `#d3e3fd` / `#041e49` | `#25344d` / `#d3e3fd` | Active result row selection, hover washes |
| `destructive` / `destructive-foreground` | `#b3261e` / `#ffffff` | `#f28b82` / `#0e1117` | Uninstall Service action, protocol/daemon errors |
| `success` | `#146c2e` | `#6dd58c` | Live daemon status, healthy indexed volume badge |
| `warning` | `#b06000` | `#fdd663` | Bootstrapping/rescanning progress, incomplete volume honesty notice |
| `info` | `#0b57d0` | `#7cacf8` | Informational hints and version indicators |
| `border` / `input` / `ring` | `#dbe3ef` / `#c4cfe0` / `#1a73e8` | `#2c374b` / `#36445c` / `#8ab4f8` | Hairlines, search input border, keyboard focus-visible rings |
| `scrim` | `#041e4970` | `#000000cc` | Modal dialog backdrop |

### Typography, Spacing & Alignment Scale
- **Hierarchy**:
  - Window title & brand lockup: `text_sm` Semibold (`theme().foreground`) + version pill (`text_xs`, `tabular_nums`)
  - Onboarding / Dialog title: `text_base` / `text_lg` Semibold
  - Search input & result filename: `text_sm` (`FontWeight::MEDIUM` for filename)
  - Directory path, sizes, timestamps, status bar: `text_xs`, `tabular_nums` for aligned sizes, dates, and entry counts, monospace for query syntax hints and global shortcut keycaps
- **Spacing**: 4pt grid (`gap_1`=4pt, `gap_2`=8pt, `gap_3`=12pt, `gap_4`=16pt, `p_5`=20pt). Sibling workspace bands (search bar, filter strip, table header, result rows, status bar) share a consistent `16pt` horizontal leading/trailing content inset spine (`px_4`).
- **Radii**: `theme().radius` (`8px` / `0.5rem`) on inputs, buttons, and menus; `radius + 4px` (`12px`) on onboarding/settings cards and dialogs; `999px` (`rounded_full`) exclusively on filter chip pills and status dots.
- **Density**: Compact desktop density (`32px`–`36px` virtualized result rows, `40px` hero search input, `28px` status bar, `32px` toolbar buttons).

---

## 3. Window & Screen Architecture

```text
+-----------------------------------------------------------------------------------+
| [logo-mark.svg] Ramen [v1.0.0]   [● Live · 1.24M items]   [⚙][ⓘ] | [S/L/D] [_][O][X] |  48px Hidden Title Bar
+-----------------------------------------------------------------------------------+
| [ 🔍 Search files, folders, or goz syntax (ext:rs size:>1mb)... ] [Aa] [Sort ▾]  |  Search Toolbar
| (All) (Folders) (Documents) (Images) (Videos) (Audio) (Archives) (Code) [Scope ×] |  Filter Chips
+-----------------------------------------------------------------------------------+
| Name                                      Path                    Size   Modified |  Column Header
+-----------------------------------------------------------------------------------+
| [📄] main.rs                              C:\my\AI\nsh\Ramen\src  4.2 KB 11/23 13 |
| [📁] assets                               C:\my\AI\nsh\Ramen         --  11/23 13 |  virtual_list
| [🖼] icon.png                             C:\my\AI\nsh\Ramen\ass 44.6 KB 11/23 13 |  (0-jank rows)
|  ...                                                                              |
+-----------------------------------------------------------------------------------+
| [●] Ready · C:\ D:\ · 1,248,910 indexed      Showing 2,000 of 14,320   Alt+Space |  28px Status Bar
+-----------------------------------------------------------------------------------+
  Alternate Workspace (when daemon unavailable):
    Service Onboarding Card (NotInstalled / Stopped / Disabled / VersionMismatch / Untrusted / Non-Windows)
  Overlays:
    Settings Dialog (Hotkey, Close Behavior, Service Mgmt) | Confirm Uninstall Service
    About / Update Dialog | Confirm Close Dialog | Context Menu | Notice Toast
```

### Core Surface Specifications

1. **Title Bar (`48px` hidden native bar via `bento::shell::title_bar_shell`)**:
   - Left: `logo-mark.svg` (`22px`), `Ramen`, clickable `v1.0.0` badge opening About.
   - Center: Compact daemon health pill (`Live`, `Bootstrapping`, or `Offline`) showing live state via icon/dot + explicit text (never color alone).
   - Right: `bento::shell::title_bar_right_with_theme` hosting Settings icon button, About icon button, `toolbar_separator()`, and `System / Light / Dark` theme segmented switcher at the far right before native window captions.

2. **Search Bar**:
   - Prominent single-line `search_input` (`40px` height) with auto-focus on window open or global hotkey summon (`Alt+Space` when enabled).
   - Trailing inline controls inside or aligned beside the input:
     - Clear (`×`) button when query is non-empty.
     - Case-sensitivity toggle (`Aa` — `match_case`), with persistent `selected` fill.
     - Sort mode menu/cycle (`Name`, `Path`, `Size`, `Date Modified` + `Asc`/`Desc` direction).
     - Subtle syntax help trigger (`?`) opening an anchored popover documenting `goz` query syntax (`ext:`, `size:>10mb`, `dm:today`, `folder:`, `path:`, quotes, `!` negation).
   - Keystroke updates bump an atomic query generation counter and dispatch via `SearchEngine` on a dedicated worker thread, cancelling obsolete pending requests so typing stays instantaneous.

3. **Filter Chips Row**:
   - Single horizontal row of `bento::ui::filter_chip` pills for `FileCategory`:
     `All`, `Folders`, `Documents`, `Images`, `Videos`, `Audio`, `Archives`, `Code`.
   - Active chip uses persistent `selected` semantic styling (`theme().primary` / `theme().primary_foreground` or `accent`); pressing the active non-`All` chip toggles back to `All`.
   - Optional active folder scope pill (`in C:\Projects ×`) when scoped, allowing one-click scope clearing.

4. **Result Rows (`virtual_list`)**:
   - Uses NSH `virtual_list` with fixed row height (`34px`) and zero per-row local signals so rendering 2,000+ streamed rows causes zero frame drops.
   - Four aligned lanes matching the column header spine:
     1. **Name lane** (`flex_2`, `min_w_0`): Fixed `20px` icon slot (`FileIconKind` mapped to `bento::ui::icons`) + single-line truncated filename.
     2. **Path lane** (`flex_3`, `min_w_0`): Single-line truncated parent directory path in `theme().muted_foreground`.
     3. **Size lane** (`w_24`, right-aligned, `tabular_nums`): Human-readable byte size (`B`, `KB`, `MB`, `GB`) or `—` when unavailable/folder unindexed.
     4. **Modified lane** (`w_36`, right-aligned, `tabular_nums`): Formatted local timestamp from Windows `FILETIME` (`mtime_ft`).
   - Row states:
     - **Rest**: Transparent on `theme().background` / `card`.
     - **Hover**: Subtle `theme().muted` wash across full row bounds (rows never scale).
     - **Pressed**: Slightly deeper `theme().accent` wash.
     - **Selected**: Persistent `theme().accent` fill with `theme().accent_foreground` (navigable with `Up`/`Down`/`PageUp`/`PageDown`/`Home`/`End`).
   - Interactions: Double-click or `Enter` launches the file/folder via `bento::shell::open_local_file` / `open_directory`; right-click opens a native `ContextMenu` (`Open`, `Reveal in Explorer`, `Copy Full Path`, `Copy Parent Path`, `Scope to Folder`).

5. **Service Onboarding / Recovery Card**:
   - Shown automatically in place of empty results whenever `gozd` is not ready (`NotInstalled`, `Stopped`, `Disabled`, `VersionMismatch`, `Untrusted`, or non-Windows OS).
   - Centered `bento::ui::card` (`max-w-[480px]`, `p-6`, `gap-4`):
     - Header icon + clear state title (e.g., `"Install Search Service"`, `"Search Service Stopped"`, `"Upgrade Search Service to v0.7.1"`).
     - Concise 1–2 sentence explanation of why elevation/service is needed (NTFS MFT + USN journal access via `gozd v0.7.1` installed to `Program Files\goz\gozd.exe`, verified by SHA-256).
     - Primary button (`Install & Start Service` / `Start Service` / `Upgrade Service`) disabled with inline progress label while downloading, verifying SHA-256, or awaiting UAC elevation.
     - Retry / Refresh Status secondary button and clear inline error text if UAC is cancelled or network download fails.

6. **Dialogs**:
   - **Settings Dialog** (`modal_dialog`, `width: 480px`):
     - Global Hotkey section: Toggle switch + shortcut input field + live `HotkeyStatus` badge (`Registered`, `Conflict`, `Invalid`, `Disabled`).
     - Window & Tray section: Close behavior radio group (`Ask`, `Minimize to Tray`, `Exit`).
     - Background Index Service (`gozd v0.7.1`) section: Service state badge, `Start` / `Install or Repair` button, and destructive `Uninstall Service…` button (opens `Confirm Uninstall` dialog).
   - **Confirm Uninstall Service** (`confirm_dialog`): Names `goz` Windows service and `Program Files\goz\gozd.exe`; `Cancel` + destructive `Uninstall`.
   - **Confirm Close** (`bento::shell::confirm_close_dialog`) & **About / Update** (`modal_dialog`).

7. **Bottom Status Bar (`28px` height)**:
   - Full-width footer strip (`theme().sidebar` background, `1px` top `theme().border`, `px_4`).
   - Left: Service & volume state (`Live · C:\, D:\ · 1,248,910 entries`), or warning pill (`Indexing in progress — results may be incomplete`) whenever `QueryResults::volumes_incomplete` or `DaemonStatus::volumes_incomplete` is `true` (honesty bit).
   - Right: Match count (`tabular_nums`, e.g., `1,420 items` or `Top 2,000 of 18,900`), `metadata_pending` indicator if sizes/dates are still enriching, and active global hotkey hint (`Alt+Space`).

---

## 4. Motion Tokens & Motion Inventory

### Motion Tokens
Matches `bento::ui` shared motion constants aligned with NSH Design Guides:
- `instant`: `120ms` (`ease-out`) — hover background washes, border tints, row highlight transitions.
- `fast`: `150ms` (`cubic-bezier(0.22, 1, 0.36, 1)` soft settle) — button & filter chip press scale (`0.97`, icon buttons `0.95`), dialog/popover exit.
- `base`: `240ms` (`ease-in-out`) — segmented theme switcher fill, toggle switch thumb/track transitions, notice toast entrance.
- `slow`: `360ms` (`ease-out`) — modal dialog surface entrance (rise `8pt` from `0.97` scale + scrim fade).
- **Reduced Motion**: Evaluated via `reduce_motion()`. When enabled, all transforms/scales/slides collapse to instant state swaps and no continuous animation runs.

### Motion Inventory (All Screens & States)

| Surface / Screen | State Change | Tool & Tokens | Reduced Motion | Verified |
|---|---|---|---|---|
| **Global Controls (`bento::ui`)** | Button, filter chip, and toggle hover / press | Native `.hover()` wash (`instant`, ease-out) + `.active(\|s\| s.scale(0.97))` (`fast`, soft settle) | Hover wash instant; press scale disabled (`1.0`) | [ ] |
| **Icon Actions (`bento::ui`)** | Settings, About, Clear Search, Match Case, Theme segmented press | Native `.hover()` wash (`instant`) + `.active(\|s\| s.scale(0.95))` (`fast`) | Instant color swap, no scale | [ ] |
| **Keyboard Focus Ring** | Tab / arrow focus enters interactive control | Native `.focus_visible()` outward `2px` ring (`theme().ring`), never clipped | Unchanged (always visible) | [ ] |
| **Title Bar (`shell.rs`)** | Theme mode segmented switch (`System` / `Light` / `Dark`) | `bento::shell::theme_mode_segmented` selected fill transition (`base`, ease-in-out) | Instant swap | [ ] |
| **Title Bar & Status Bar** | Daemon status label / indexed count / match count update | **No motion** on count/status ticks (`tabular_nums` prevents layout jitter; frequent live updates must not pulse or bounce) | N/A (always instant) | [ ] |
| **Search Bar (`views/search.rs`)** | Focus border highlight & Clear (`×`) button appear/disappear | Border tint (`instant`, ease-out); Clear button conditional mount (`when`) with instant visibility | Instant | [ ] |
| **Filter Chips (`views/search.rs`)** | Category chip selection switch (`All` / `Folders` / `Documents` / ...) | Selected surface fill + foreground transition (`fast`, ease-in-out); chip press scale `0.97` (`fast`) | Instant fill swap, no scale | [ ] |
| **Result Rows (`virtual_list`)** | Row hover, press, and keyboard selection change | `.hover()` wash (`instant`) + `.active()` / `.selected()` fill (`instant`). Rows **never scale** | Instant fill swap | [ ] |
| **Result Rows (`virtual_list`)** | Search typing, sorting, live Watch refresh, cell recycling | Intentionally **no row enter/exit wave** on search updates so rapid keystrokes across 10,000+ rows remain zero-jank | N/A (always instant) | [ ] |
| **Popovers & Context Menu** | Sort menu, syntax help popover, row right-click `ContextMenu` | `popupStartingStyle` / `popupEndingStyle`: scale `0.97 -> 1.0` + fade (`fast` open, `instant` close) anchored to trigger | Instant appear/dismiss | [ ] |
| **Service Onboarding Card** | Appear / swap with result list when service state transitions | Conditional mount (`when`) + button disabled/loading state label swap during download/UAC | Instant swap; static progress label | [ ] |
| **Modal Dialogs (`dialogs.rs`)** | Settings, Confirm Uninstall, Confirm Close, About | `bento::ui::modal_dialog` scrim opacity fade + dialog rise `8pt` (`slow` enter, `fast` exit) | Instant appear/dismiss | [ ] |
| **Notice Toast (`shell.rs`)** | File path copied / Service started / Error toast (3s / 6s) | `bento::ui::notice_toast` bottom-edge floating reveal (`base`, ease-out) | Instant appear/dismiss | [ ] |
| **Loading / Indexing** | Volume bootstrapping, MFT scan, or `gozd.exe` download | Title/status bar text + warning/info badge (`Bootstrapping…`, `Downloading gozd…`); no spinning CPU shader loop | Static status label | [ ] |
| **Drag / Swipe / Shaders** | None on Ramen screens | N/A (Ramen has no drag-reorder lists, swipe gestures, or continuous GPU shaders) | N/A | [ ] |

---

## 5. Accessibility & Keyboard Contract

- Every interactive control uses semantic `bento::ui` wrappers built on `nsh_base` (`button()`, `input()`, `checkbox()`, `switch()`, `radio_option()`, `menu()`, `context_menu()`).
- Every icon-only button (`Settings`, `About`, `Theme System/Light/Dark`, `Clear Search`, `Syntax Help`, `Close Dialog`) carries an explicit `accessibility_label`.
- Complete keyboard operation without mouse:
  - **Global hotkey** (configurable via `bento::hotkey`, default disabled or `Alt+Space` when enabled): Summons/focuses Ramen window and focuses search input.
  - `Ctrl+F` / `/`: Focus search input and select current query text.
  - `Down` / `Up` / `PageDown` / `PageUp`: Move selected result row inside `virtual_list` (keeping selection visible in scroll viewport).
  - `Enter`: Open currently selected file or folder (when focus is in search box or results list).
  - `Ctrl+Enter` / `Ctrl+Shift+E`: Reveal currently selected item in Windows Explorer (`bento::shell::reveal_in_file_manager`).
  - `Ctrl+C`: Copy selected item's full path to clipboard (when not selecting text inside input) and show brief `NoticeToast`.
  - `Ctrl+,`: Open Settings dialog.
  - `Escape`: Dismiss topmost open dialog/popover -> clear search query if non-empty -> hide window to tray (if tray close behavior active).
- Status is never conveyed by color alone: every daemon/volume state pairs semantic color with explicit readable text (`Live`, `Bootstrapping`, `Rescanning`, `Stopped`, `Not Installed`, `Untrusted`, `Incomplete index`).
