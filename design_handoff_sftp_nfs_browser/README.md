# Handoff: SFTP/NFS Browser for Tablets

## Overview
A touch-first tablet client for browsing remote filesystems over SFTP and NFS. Five screens are specified: saved connections, add/edit connection, file browser (with multi-select), transfer queue, and settings. Audience is sysadmins/devops, media professionals pulling from NAS shares, and prosumers with home servers — so protocol detail (mode bits, ports, auth method, mount state, host key) is shown rather than hidden.

Target: tablet landscape, iPad and Android. Canvas is **1194 × 834** CSS px. Touch only — no hover-dependent affordances, no right-click, no keyboard shortcuts assumed.

## About the Design Files
`SFTP Browser.dc.html` in this bundle is a **design reference created in HTML** — a prototype showing intended look, layout, and content, not production code to copy. The task is to **recreate these designs in the target codebase's existing environment** (React, Vue, SwiftUI, Compose, etc.) using its established components, layout primitives, and theming. If no environment exists yet, pick the framework appropriate to the platform and implement there.

The file contains two visual treatments side by side. **Treatment `1a` "Warm Slate" is the approved direction.** Treatment `1b` "Ledger" is retained for reference only — do not implement it. In the HTML, `1a` is the block with `id="1a"`; each screen inside it is preceded by a caption (`01 · Connections`, `02 · Add connection`, `03 · File browser`, `04 · Transfers`, `05 · Settings`).

## Fidelity
**High-fidelity.** Colors, type sizes, weights, row heights, paddings, and radii are final and listed below. Recreate pixel-accurately using the codebase's existing libraries where they can match these values; where the codebase has an equivalent token, prefer the token.

Two caveats: icons are intentionally **placeholders** — file types are rendered as 3-letter monospace labels (`DIR`, `MOV`, `WAV`, `CFG`, `JPG`, `LOG`, `LNK`) in a 44px tile, and toolbar glyphs are text stand-ins (`‹`, `Q` for search, `‖` for pause). Substitute the codebase's real icon set, keeping the tile geometry. Second, the status strip (`9:41 / wifi / 100%`) stands in for the OS status bar; the real app uses safe-area insets, not this strip.

## Design Tokens

### Color (warm dark, single theme — dark only)
| Role | Hex |
|---|---|
| App background | `#171512` |
| Sidebar / footer bar background | `#1B1814` |
| Panel / card surface | `#1F1C18` |
| Raised control (file-type tile, stepper) | `#262119` |
| Border, strong (panel outline, control outline) | `#353028` |
| Border, medium (row divider inside panels) | `#2A251F` |
| Border, faint (file list row divider) | `#221E19` |
| Text primary | `#EDE8E0` |
| Text secondary | `#A69D90` |
| Text tertiary / mono metadata | `#756C60` |
| Accent (primary action, selection, SFTP) | `#3B6FE0` |
| Accent wash (selected row, tinted badge) | `rgba(59,111,224,.10)` row / `rgba(59,111,224,.14)` badge |
| Accent border (selected sidebar item) | `rgba(59,111,224,.35)` |
| NFS / warning | `#C9803A`, wash `rgba(201,128,58,.14)` |
| Error text | `#D08A6A`, dot/marker `#C9603A` |
| Success text | `#8FBF9A`, dot `#6FAE7C` |
| Inactive dot / toggle knob-off | `#4F4840` / `#8A8175` |
| Toggle track off | `#332E27` |
| Progress track | `#2A251F` |

### Typography
- UI: **Source Sans 3** — 400 / 500 / 600. Fall back to the platform humanist sans.
- Data: **IBM Plex Mono** — 400 / 500 / 600. Used for all paths, hosts, sizes, mode bits, speeds, section labels, and status strings.
- Scale in use: page title 30–32/600 (letter-spacing −.01em); section title 28/600; card title 22/600; primary row text 17/600; body 16–17/400–500; secondary 15/400; mono metadata 13–15/400; column headers and section labels 11/600 mono, uppercase, letter-spacing .08–.10em.

### Spacing, radius, shape
- Screen gutters: 40px (content pages), 24px (browser main column), 20px (sidebar sections), 10px (sidebar list inset).
- Radii: panels/cards 14px (12px on the compacted transfer rows), controls 10–12px, badges 5–6px, checkbox 7px, dots/toggles fully round.
- Elevation: `0 1px 2px rgba(0,0,0,.35)` on the two mounted connection cards only. Everything else is flat with a 1px border.
- Selection marker: `box-shadow: inset 3px 0 0 #3B6FE0` plus the accent wash background.
- Layout uses flex/grid with `gap` throughout; no margin-based spacing between siblings.

### Touch sizing (hard minimums — do not compact further)
Primary buttons 56px tall (48px on the Transfers screen header), file list rows 70px, connection/place sidebar rows 56–60px, form rows 72–88px, settings rows 84px, checkbox hit area ≥44px around a 26px box, pause control 44 × 44px, toggle 60 × 34px with a 28px knob.

## Screens / Views

### 01 · Connections
**Purpose:** pick a saved endpoint, see which are mounted, add a new one.

Column layout: 34px status strip → header → content.
Header (`padding: 14px 40px 26px`) has title `Connections` (32/600) with subtitle `6 saved · 2 mounted` (15/400, secondary) at left; at right a row of two 56px buttons, gap 12: secondary `Import from .ssh/config` (panel fill, 1px `#353028`, radius 12) and primary `New connection` (accent fill, white 16/600).

Body is a 2-column grid, gap 18, gutters 40. Each card: panel fill, 1px `#353028`, radius 14, padding 22.
- Top row, space-between: protocol badge — mono 11/600, letter-spacing .1em, padding 5px 9px, radius 6, `SFTP` accent-on-wash or `NFS` amber-on-wash — and state: 8px dot + 14px label (`Mounted` green, `Idle` tertiary, `Unreachable` `#D08A6A`).
- Name 22/600, margin-top 16.
- Endpoint mono 14/400 secondary, margin-top 6: `editor@10.0.4.12:22 · /volume1/media`, `nfs://archive.local:/export/cold`, `root@192.168.1.20:2222 · /etc`, `nfs://10.0.9.3:/scratch`.
- Footer: 1px `#2A251F` top rule, 16px above, 20px gap; left tertiary 14px (`Opened 12 min ago`, `Opened yesterday`, `Key: id_ed25519`, `Timed out after 8s`), right accent 14/600 `Open` — or `Retry` on the unreachable card.

Only mounted cards carry the shadow. Four cards shown; the list scrolls.

### 02 · Add connection
**Purpose:** create or edit an endpoint, verify it before saving.

Header (`padding: 10px 40px 22px`), three-part: `Cancel` 17/400 secondary · title `New connection` 20/600 centered · 52px accent `Save`.
Body centers a 760px column, flex, gap 16.

1. **Protocol segmented control** — panel fill, 1px border, radius 14, padding 6, two 56px halves gap 6. Active half: accent fill, white 17/600, label `SFTP` plus a 12px mono sublabel `ssh` at 75% opacity. Inactive: transparent, secondary 17/500, sublabel `v3 / v4`.
2. **Identity panel** — radius 14, rows divided by 1px `#2A251F`:
   - `Name` / `Studio NAS` — grid `200px 1fr`, height 72.
   - `Host` / mono `10.0.4.12` / right-aligned mono `port 22` — grid `200px 1fr 140px`, height 72.
   - `Username` / mono `editor` — height 72.
   - `Authentication` — height 88; three 52px chips, gap 10: selected `SSH key` (accent wash, 1px accent, 16/600), then `Password`, `Agent` (1px `#353028`, secondary).
   Labels are 16/400 secondary; values 17/500, mono where they are machine strings.
3. **Transport panel** — same construction:
   - `Private key` / mono `id_ed25519 · imported` (the ` · imported` in tertiary) / 48px `Replace` outline button — height 80.
   - `Remote path` / mono `/volume1/media` — height 72.
   - `Store credentials in keychain` with 14px tertiary sub-line `Face ID required to mount` / toggle ON — height 80.
4. **Verify row** — 56px `Test connection` outline button + mono 15/400 success text `ok · handshake 240 ms · sftp v3`, gap 14.

Empty-state and error-state equivalents of the verify row are needed in implementation: unverified (tertiary hint), in-progress (spinner + `testing…`), failed (`#D08A6A` reason string). `Save` should be disabled until host, user, and auth are present.

### 03 · File browser
**Purpose:** navigate a mount, select files, act on the selection. Shown with 2 items selected.

Two columns: 296px sidebar (background `#1B1814`, 1px right border `#353028`) and a fluid main column.

**Sidebar**, top to bottom:
- `MOUNTS` label (mono 11/600, letter-spacing .1em, tertiary) in a 22/20/16 padded block with a 1px `#2A251F` bottom rule.
- Mount rows, 60px, inset 10px, radius 11, gap 12: 8px state dot, then name (16/600 active, 16/500 otherwise) over mono 12/400 `sftp · 10.0.4.12`. The active row is accent wash + 1px `rgba(59,111,224,.35)`. Unmounted rows use secondary name text and a `#4F4840` dot.
- `PLACES` label, then 56px rows: `Recent`; `Bookmarks` with a mono 13/500 tertiary count `7`; `Transfers` with an accent pill badge (mono 12/600 white on `#3B6FE0`, padding 4px 9px, radius 9) reading `3`.
- Pinned to the bottom, above a 1px `#2A251F` rule: mono 13/400 tertiary `1.8 TB free of 12 TB` with a 6px capacity bar (track `#2A251F`, fill `#4F4840` at 85%).

**Main column:**
- Status strip, then a 52px-tall toolbar row (`padding: 8px 24px 18px`, gap 14): 52px square back button (`‹`); flexible breadcrumb field (panel fill, 1px border, radius 11, mono 15/400) where ancestors are secondary, separators `#4F4840`, and the current folder `2026_shoots` is primary 500; 52px search button; and a 52px `Sort: Modified` pill.
- Column header, 40px, mono 11/600 tertiary, letter-spacing .08em, 1px `#2A251F` bottom rule: `NAME · SIZE · MODIFIED · MODE`.
- Rows: 70px, grid `28px 44px 1fr 110px 170px 120px`, gap 16, gutters 24, divider 1px `#221E19`.
  - Checkbox: 26px, radius 7, 1.5px `#4F4840` when empty; accent fill with a white check when selected.
  - Type tile: 44px, radius 10, `#262119` fill, 1px `#353028`, mono 10/600 letter-spacing .06em — `DIR` in accent, `CFG` in amber, other types secondary, `LNK` tertiary.
  - Name 17/600; symlinks render `latest → 2026_shoots` in secondary.
  - Size mono 15/400 secondary (`—` tertiary for directories), modified 15/400 secondary, mode mono 13/400 tertiary (`drwxr-xr-x`, `-rw-r--r--`, `lrwxrwxrwx`).
  - Selected rows: accent wash + `inset 3px 0 0 #3B6FE0`.
- Selection action bar, pinned bottom, `#1B1814`, 1px `#353028` top border, padding 14/24, gap 12: `2 selected` 16/600 with mono 14/400 secondary `23.9 GB`; then right-aligned 56px controls — primary `Download` (accent), outline `Copy to…`, `Rename`, and `Delete` with `#D08A6A` label.

The bar appears only while a selection exists; with none, the same slot is empty and the list extends. Long-press or the leading checkbox enters selection; tapping a row with a live selection toggles it rather than navigating.

### 04 · Transfers
**Purpose:** monitor and control in-flight transfers. This screen is deliberately denser than the others (the request was to compact it), while keeping the 44px pause target.

Header (`padding: 8px 40px 18px`): baseline row of title `Transfers` 30/600 and mono 14/400 secondary `2 active · 118 MB/s · 6 min left`, gap 16; at right two 48px buttons gap 10 — `Pause all` (16/500... 15/500) and `Clear finished` (secondary).
Content column, gutters 40, gap 8.

**Active rows** — panel fill, 1px `#353028`, radius 12, padding 14/18:
- Top line, gap 12: direction+protocol badge (mono 11/600, padding 4px 8px, radius 5 — `↓ SFTP` accent-on-wash, `↑ NFS` amber-on-wash), filename 17/600 flexing, mono 14/400 secondary `8.9 / 14.2 GB · 118 MB/s`, then a 44px outline pause control (`‖`) with 10px left margin.
- Progress bar 6px, radius 3, track `#2A251F`, fill accent (SFTP) or amber (NFS), margin-top 10.
- Foot line, mono 12.5/400 tertiary, space-between, margin-top 7: route `Studio NAS → On device / Ingest` and `63% · 46s left`.

**Section labels** `QUEUED · 1`, `FINISHED · TODAY` — mono 11/600, letter-spacing .1em, tertiary, 10px above.

**Queued row** — 60px, panel fill, radius 12, gutters 18: muted badge (`#262119` fill, tertiary text), filename 16/500, mono 14/400 tertiary `9.7 GB · waiting`.

**Finished rows** — 56px, `#1B1814` fill, 1px `#2A251F`, radius 12: leading mono marker (`✓` `#6FAE7C`, or `!` `#C9603A`), name 16/500 secondary, and either mono 14/400 tertiary `3.1 KB · 09:12` or, for failures, an inline `#D08A6A` mono reason (`permission denied`) with an accent 15/600 `Retry` at right.

### 05 · Settings
**Purpose:** transfer, editor, and security preferences.

288–296px left nav on `#1B1814` with a 1px right border: title `Settings` 24/600 (padding 0 14 20), then 58px rows, inset 12, radius 11 — active row accent wash + 1px `rgba(59,111,224,.35)`, 16/600; others secondary 16/400. Items: `Transfers` (active), `General`, `Text editor`, `Security & keys`, `Storage`, `About`.

Right pane, padding 44/40, flex gap 22: section title `Transfers` 28/600, then grouped panels (radius 14, 1px `#353028`, rows divided by `#2A251F`):
- `Parallel transfers` + sub-line `Higher values saturate slow links` / a 3-stop stepper of 52px squares (`1`, `4`, `8`) — selected square is accent wash, 1px accent, mono 17/600. Row height 84.
- `Resume interrupted transfers` + `Uses partial-file offsets where the server allows it` / toggle ON.
- `Only transfer on Wi-Fi` + `Queue holds until Wi-Fi returns` / toggle OFF.
- Label `CONFLICTS`, then: `When a file already exists` / value `Ask every time` (17/400 secondary, opens a picker); `Preserve modification times` + `NFS mounts only` / toggle ON.

Row labels are 17/400 primary; sub-lines 14/400 tertiary, 3px below.

## Interactions & Behavior
- **Navigation:** connection card → mount (progress/handshake state, then browser at the saved remote path); folder row → push a path segment; breadcrumb segment → jump up; back button → pop. Sidebar switches mounts without losing each mount's path stack. `Transfers` place and the settings nav are top-level destinations.
- **Selection:** long-press a row or tap its checkbox to enter selection mode; subsequent taps toggle. The action bar slides up from the bottom (~180ms ease-out) and shows a live count and byte total. Exiting selection dismisses it.
- **Destructive:** `Delete` opens a confirm sheet naming the count; failures surface per-item in the transfer list rather than as a blocking alert.
- **Transfers:** per-item pause/resume and a global `Pause all`. Progress, rate, and ETA update ~1/s; animate bar width, not the numeric text. Failed items keep a `Retry`. `Clear finished` removes completed and failed entries.
- **Connection test:** `Test connection` runs a handshake and reports latency, protocol version, and host key; an unknown host key must prompt for trust before saving.
- **Error states to build:** unreachable host (card state + reason), permission denied (per-file, inline), stale/broken mount (sidebar dot goes inactive with a `reconnect` action), Wi-Fi lost while `Only transfer on Wi-Fi` is on (queue holds, header says why).
- **Responsive:** designed for 1194 × 834. Narrower landscape widths shrink the file list's `1fr` name column first, then drop the `MODE` column below ~980px content width. In portrait, collapse the sidebar into a drawer opened from the breadcrumb bar and keep the action bar pinned.
- No hover states are required; provide pressed states (surface one step lighter, ~120ms) and honor reduce-motion.

## State Management
- `connections[]` — id, name, protocol (`sftp` | `nfs`), host, port, user, authMethod (`key` | `password` | `agent`), keyRef, remotePath, storeInKeychain, lastOpenedAt, state (`mounted` | `idle` | `unreachable`), error.
- `activeConnectionId`, and per connection a `pathStack[]` plus `entries[]` (name, type, size, mtime, mode, target for symlinks) with `loading` / `error`.
- `selection: Set<entryId>` derived count and byte total; `selectionMode: boolean`.
- `sort: {key, direction}`, `filter/searchQuery`, `viewDensity`.
- `transfers[]` — id, direction, connectionId, srcPath, dstPath, bytesDone, bytesTotal, rate, etaSeconds, status (`active` | `queued` | `paused` | `done` | `failed`), error. Global `paused`, aggregate rate and ETA.
- `settings` — parallelTransfers (1|4|8), resumeInterrupted, wifiOnly, conflictPolicy, preserveMtimes, editor prefs.
- Data fetching: directory listings per path (cache per mount, revalidate on focus and after any write), transfer progress via a subscription/stream rather than polling the whole list, credentials read from the platform keychain behind a biometric gate.

## Assets
None shipped. Fonts are Google Fonts (Source Sans 3, IBM Plex Mono) — swap to the codebase's equivalents if it already ships a humanist sans and a mono. All file-type and toolbar icons are text placeholders to be replaced with the codebase's icon set; nothing in this design uses illustration or imagery.

## Files
- `SFTP Browser.dc.html` — both treatments. Implement the `id="1a"` block ("Warm Slate"); `id="1b"` ("Ledger") is reference only. The file also carries a `treatment` control that can hide one treatment while reviewing.
