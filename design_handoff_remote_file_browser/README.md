# Handoff: Remote File Browser (SFTP / NFS / SMB) — tablet + phone

## Overview
UI design for a remote-filesystem browser app for iOS and Android. Users register hosts
(SFTP, NFS, SMB, plus local storage), browse remote directories, inspect and transfer files,
manage a transfer queue, and manage SSH keys / saved credentials / known hosts.

Audience the design targets: sysadmins & DevOps (terminal-adjacent, wants permissions, rates,
error codes) and media hoarders browsing a NAS. Density is deliberately **compact/pro**.

The primary, approved direction is the **tablet "console" layout** (turn 2 in the design file:
options `2a`–`2d`). Phone explorations (`1a`–`1c`) are included as secondary reference; `1c`
is the phone expression of the same console language.

## About the Design Files
The files in this bundle are **design references created in HTML** — prototypes that show
intended look, layout, density, and content. They are **not production code to copy**.
The task is to **recreate these designs in the target codebase's own environment** (React
Native, SwiftUI, Jetpack Compose, Flutter, web, etc.) using its established components,
navigation, and theming. If no environment exists yet, pick the framework most appropriate
for a cross-platform iOS/Android tablet app and implement the designs there.

The HTML uses inline styles and a small runtime purely so the mockups render as a single
scrollable review canvas. Ignore that scaffolding; take the visual spec below.

## Fidelity
**High fidelity.** Colors, type sizes, weights, hairline values, column widths, row heights,
and all copy are final-intent and specified exactly below. Recreate pixel-closely, but map to
the codebase's existing primitives (list rows, dialogs, switches) where they exist.

Device sizes used: iPad-class **1194 × 834** landscape (`2a`, `2b`, `2c`), Android tablet
**1280 × 800** landscape (`2d`), phones **402 × 874** (iOS) and **412 × 892** (Android).

---

## Design Tokens

### Color
| Token | Value | Use |
|---|---|---|
| `bg` | `#0b0c0c` | Tablet screen background (console screens) |
| `bg-alt` | `#0e100f` | Phone screens `1a`/`1b`, dialog surface, queue strip `#0d0f0e` |
| `text` | `#eef1f0` | Primary text |
| `text-70` | `rgba(238,241,240,.70)` | Secondary values |
| `text-60` | `rgba(238,241,240,.60)` | Data values (sizes, rates) |
| `text-45` | `rgba(238,241,240,.45)` | Tertiary / inactive names |
| `text-40` | `rgba(238,241,240,.40)` | Paths, host subtitles |
| `text-35` | `rgba(238,241,240,.35)` | mtime, owner, mode |
| `text-30` | `rgba(238,241,240,.30)` | Column headers, disabled |
| `text-25` | `rgba(238,241,240,.25)` | Hidden files, completed rows |
| `accent` | `#00c48f` | Live state, active tab, one primary action per screen |
| `accent-on` | `#0b0f0e` | Text/knob on accent fill |
| `accent-wash` | `rgba(0,196,143,.05–.08)` | Selected row background |
| `warn` | `#e0a83c` | Stale mount, passphrase-only key, paused-on-cellular |
| `error` | `#e07a6a` | Failed transfer, changed fingerprint, destructive |
| `hairline` | `rgba(255,255,255,.07)` | Structural dividers (panes, headers, footers) |
| `hairline-soft` | `rgba(255,255,255,.04–.05)` | Row separators inside tables |
| `border-ctl` | `rgba(255,255,255,.12)` | Outlined buttons, inputs, chips |
| `bezel` | `#1b1d1c` (iPad) / `#232525` (Android) | Device frame |

No gradients, no elevation cards, no filled panels: **chrome is hairlines + whitespace only.**
Accent is reserved — never used for decoration.

### Typography
- **UI sans**: system (`-apple-system / SF Pro` on iOS, `Roboto` on Android).
- **Mono**: `ui-monospace, "SF Mono", Menlo, monospace` — used for *all* data: paths, sizes,
  rates, permissions, fingerprints, timestamps, column headers, chips, footer actions.
- Scale actually used:
  - Screen title (phone): 26–27px / 600 / letter-spacing −.5px
  - Pane title mono: 13px / 600
  - File name (tablet table): 12.5px mono / 400 (folders 500)
  - List row title (phone): 14.5–15.5px / 500
  - Data cells: 10–10.5px mono / 400
  - Column headers: 9.5px mono / 500 / letter-spacing .10em / uppercase
  - Section labels: 9.5–10px mono / 500 / letter-spacing .12–.16em / uppercase
  - Permission mode: 9.5px mono
  - Inspector metric label 10.5px mono, value 10.5px mono
  - Big numeric readout (throughput): 24px mono / 600

### Metrics
- Tablet table row: `padding: 5px 16px`, ~22px tall; queue rows with progress `7px 16px`.
- Phone rows: `padding: 9–12px 18–20px`.
- Column header rows: `padding: 6px 16px`.
- Pane widths: hosts rail **252px**, inspector **296px**, Android settings nav **236px**,
  dialog **820px** wide, dialog right column **300px**.
- Gaps between table cells: **12px** (tablet), 8px (hosts rail), 10px (phone `1c`).
- Radius: device screen 13px (iPad) / 9px (Android); dialog 10px; buttons/chips 4–5px;
  progress bars square (0); switch pill 9px; status dot 50%.
- Progress bar: 3px tall, two flex children (`flex: pct` accent, `flex: 100-pct`
  `rgba(255,255,255,.08)`), 1px gap. Phone variant 2px tall.
- Switch: 32–34 × 17–18px, knob 13–14px, on = `accent` fill + knob `#0b0f0e`,
  off = `rgba(255,255,255,.14)` + knob `rgba(238,241,240,.5)`.
- Selected row: `accent-wash` background + **2px** left border in `accent`.
- Column widths (2a file table): checkbox/indicator 14, MODE 66, NAME flex, SIZE 58 (right),
  MTIME 82 (right), OWNER 70 (right).
- Column widths (2b queue): DIR 18, FILE flex, HOST·PATH 150, RATE 58, ETA 54, PROGRESS 70.
- Column widths (2d keys): TYPE 74, NAME·FINGERPRINT flex, STORAGE 130, HOSTS 70, LAST USED 82.

---

## Screens / Views

### 1. Browser — three-pane (`2a`, primary screen; covers "connection list" + "file browser")
**Purpose:** pick a host, walk the tree, inspect and transfer files.

**Layout (top → bottom):**
1. **Status strip** — 5px 16px, mono 10.5px `text-40`: `9:41 Tue 12 Aug` · spacer ·
   `wifi 10.0.1.4` · `vpn` · `92%`. Bottom hairline. (Real app: platform status bar.)
2. **Path bar** — 9px 16px, hairline bottom. Left: `media-nas:/export/media/films` where host
   is `accent`, middle segments `text-45`, current segment `text`. Right cluster (mono 10–11px):
   `filter…` field (1px `border-ctl`, radius 4, padding `4px 26px 4px 8px`), `SORT name↑`,
   `LIST` (active `text-50`), `GRID` (`text-30`), `QUEUE 2` in `accent`.
3. **Body** — flex row, 3 panes divided by hairlines:
   - **Hosts rail (252px)**: column header `HOST | PROTO | FREE`. Rows 7px 14px: host name
     12.5px mono 500, proto 9.5px mono (`accent` when mounted, `text-35` idle, `warn` stale),
     free space right-aligned. Selected host row gets accent wash + 2px accent left border, and
     directly under it an inline tree (children indented 10/20px, 11px mono/19px line-height;
     current dir in `text`). Hosts shown: `media-nas nfs4 4.2T`, `edge-01 sftp 38G`,
     `backup-tank smb3 —`, `plex-scratch nfs3 stale` (warn), `localhost file 41G`,
     `nas-photos nfs4 1.1T`, `vault sftp 512G`, `pi-relay sftp 12G`, `build-cache smb3 96G`.
     Then `RECENT` section (5 paths, 10.5px mono) and `PINNED OFFLINE · 3` (name + size).
     Footer (hairline top, 9px 14px, 10px mono 600): `[+] host` in accent, `keys`, `prefs`.
   - **File table (flex)**: header `MODE | NAME | SIZE | MTIME | OWNER`. First row is `..`
     parent (`../media`, `text-45`). Directories: 14×11px outlined rect indicator, name 500 with
     trailing `/`, SIZE column shows item count. Files: no indicator, name 400. Symlink row shows
     `latest → 4K_HDR` with the target in `text-40` and SIZE `link`. Dotfiles use `text-45` name,
     mode `-rw-------` at `text-25`. One row is selected (accent wash + 2px accent left border,
     mode and size rendered in accent). Contents used: `4K_HDR/ 31 items`,
     `Documentaries/ 64 items`, `Arrival.2016.2160p.mkv 18.4G`, `Arrival.2016.en.srt 84K`,
     `BladeRunner2049.2017.2160p.mkv 24.1G` (selected/downloading), `Dune.Part.Two.2024.mkv 31.7G`,
     `Heat.1995.remux.mkv 41.2G`, `Sicario.2015.mkv 12.9G`, `Sicario.2015.en.srt 61K`,
     `latest → 4K_HDR`, `.metadata/ 12 items`, `Nope.2022.2160p.mkv 22.8G`,
     `Oppenheimer.2023.2160p.mkv 46.3G`, `Prisoners.2013.mkv 14.1G`,
     `Sicario.Day.of.the.Soldado.mkv 13.4G`, `Stalker.1979.criterion.mkv 28.9G`,
     `TheThing.1982.remux.mkv 33.6G`, `Tinker.Tailor.2011.mkv 16.2G`, `Whiplash.2014.mkv 9.8G`,
     `Zodiac.2007.remux.mkv 37.4G`, `checksums.sha256 812K`, `films.nfo 4.1K`,
     `poster-cache.db 128M`, `.rsync-partial/ 3 items`. Owner column `media:media`
     (`root:root` for symlink/dotfile). Pane footer (hairline top, 10px mono `text-32`):
     `142 entries · 1.9T` · `1 selected · 24.1G` · spacer · `mkdir` `rename` `chmod` ·
     `put ↑` `get ↓` (accent).
   - **Inspector (296px)**: header `INSPECTOR`. 166px preview placeholder —
     `repeating-linear-gradient(45deg,#141615 0 6px,#101211 6px 12px)` with centered mono
     caption `frame preview · 4 MB read` (replace with a real decoded frame/thumbnail).
     File name 12.5px mono, `word-break: break-all`. Metric rows (label 74px `text-32`,
     value `text-70`, 10.5px mono): size `24.1G / 25,884,266,496 B`, mode `0644 -rw-r--r--`,
     owner `media:media 1000:1000`, mtime `2026-05-02 11:20:44`, codec
     `hevc 3840×2160 · dts-hd`, inode `8419772`. Then live transfer block:
     `downloading` / `61% · 8.4M/s`, 3px bar, `→ localhost:/Downloads · eta 19m`.
     Action chips (wrap, 7px gap, 10px mono 600): `GET ↓` (accent fill), `PIN OFFLINE`,
     `CHMOD`, `RENAME`, `SHA256` (outlined), `RM` (error text + `rgba(224,122,106,.35)` border).
     Footer 9.5px mono `text-28`: `nfs4 · rsize 1M wsize 1M · hard,intr` / `mounted 4h 12m · 0 retrans`.
4. **Queue strip** — persistent, `#0d0f0e`, hairline top, 8px 16px: label `QUEUE`, then two
   compact jobs (name ellipsized, pct in accent, 64px 3px bar), spacer,
   `3 queued · 1 failed`, `12.1M/s`, `pause all`. Tapping opens screen 2.

### 2. Transfer queue (`2b`)
Header: `queue` + `2 running · 3 waiting · 1 failed · 14 done today`, right actions
`PAUSE ALL`, `CLEAR DONE`, `LIMIT 20M/s` (accent).
Table columns `DIR | FILE | HOST · PATH | RATE | ETA | PROGRESS`.
- Active rows: accent wash, direction arrow `↓`/`↑` in accent, rate `8.4M/s`, ETA `19m 04s`,
  progress `14.7/24.1G` in accent, 3px bar under the row.
- Waiting rows: dim (`text-55` name), RATE `waiting` (or `host down` in `warn`), ETA `—`,
  progress `0/31.7G`.
- Failed row: arrow + progress in `error`; RATE cell becomes `retry`, ETA cell `skip`
  (both act as buttons); second line, indented 30px, 9.5px mono `error`:
  `EACCES · /etc/nginx/nginx.conf not writable by deploy · attempt 2 of 3 at 09:44`.
- `COMPLETED · TODAY` group: rows at `text-42` / `text-25`, PROGRESS column reads `verified`.
- Pane footer: `session 4h 12m` · `up 2.1G · down 38.4G` · `0 retrans · 0 timeouts`.
Right pane (296px): `THROUGHPUT · 60s` — 24px mono readout `12.1` + `MB/s · cap 20`,
16-bar histogram (56px tall, 2px gap, older bars `rgba(0,196,143,.3)`, recent `.45`, latest
solid accent), axis labels `-60s` / `now`. Then setting rows: `parallel jobs 2 of 3`,
`chunk size 1 MiB`, `on cellular paused` (warn), `verify sha256 on finish`. Then `PER HOST`
rows (name, rate, 52px mini bar). Footer note: `transfers continue in background / for 10 min
after app is closed`.

### 3. New / edit connection (`2c`)
Modal dialog, 820px wide, centered over the dimmed browser (browser at `opacity: .28` plus a
`rgba(6,8,7,.62)` scrim). Dialog: `#0e100f`, 1px `rgba(255,255,255,.12)`, radius 10,
shadow `0 40px 90px rgba(0,0,0,.6)`.
- Title bar: `new host` (13px mono 600), right `IMPORT ~/.ssh/config`, `✕`.
- Protocol chips row: `SFTP` (accent fill) `NFS` `SMB` `LOCAL` (outlined); right side hint
  `openssh 9.6 detected`.
- Left column (flex): underline text fields — label 9px mono, letter-spacing .14em, `text-32`;
  value 13.5px mono. Focused field's underline and label are accent, value shows a caret `|`.
  Fields: `NAME edge-02`, `GROUP prod`, `HOST edge-02.fsn1.internal` (focused), `PORT 22`,
  `USER deploy`. `AUTH` chips: `KEY` (accent outline, selected) `PASSWORD` `AGENT` `KBD-INT`.
  Selected key card (1px `border-ctl`, radius 6): `id_ed25519` +
  `SHA256:v8Kx7dR…q2Lp · Secure Enclave · Face ID`, trailing `CHANGE`.
  Then `INITIAL PATH /srv/www`, `KEEPALIVE 30s`.
- Right column (300px): `OPTIONS` switch rows — compression **on**, follow symlinks off,
  read-only mount off, show hidden **on**. Then `PROBE` log box (`#0a0b0b`, 1px
  `rgba(255,255,255,.07)`, radius 5, 9.5px/16px mono): resolve → IP, `tcp 22 open · 24 ms`,
  banner, `host key not in known_hosts` (warn), fingerprint, `auth ed25519 accepted · /srv/www
  readable` (accent). Buttons: `CANCEL` (outlined) and `TRUST & SAVE` (accent fill), equal width.
  NB: primary CTA copy changes with probe state — `TEST` before a successful probe,
  `TRUST & SAVE` when an unknown host key must be accepted, `SAVE` otherwise.

### 4. Settings + keys, Android tablet (`2d`)
Top bar: `←`, `prefs`, right `build 2026.08.3 · libssh2 1.11 · nfs-client 4.2`.
Left nav (236px, hairline right): `transfers`, `browsing`, `keys & credentials` (selected:
accent wash + 2px accent left border, `text` at 500), `known hosts`, `cache & offline`,
`appearance`, `about`; footer note `3 keys · 2 passwords / 4 known hosts · 1 changed`.
Right pane: table `TYPE | NAME · FINGERPRINT | STORAGE | HOSTS | LAST USED`.
- `ed25519 / id_ed25519 / SHA256:v8Kx…q2Lp / Keystore · biometric / 2 / 4 min ago` (selected row).
- `rsa 4096 / id_rsa_legacy / … / passphrase only` (warn) `/ 1 / 8 mo ago`.
- `ecdsa p256 / ci-runner / … / Keystore / 1 / 2 d ago`.
Action row: `GENERATE ed25519` (accent fill), `IMPORT FILE`, `PASTE PEM` (outlined),
right-aligned `REVOKE UNUSED` (error outline).
`SAVED PASSWORDS · 2` rows (proto, account, storage, `REVEAL`, last used).
`KNOWN HOSTS · 4` rows: verified hosts show a 7px accent dot; the changed one is fully in
`error` with sub-line `fingerprint changed since 04 Aug · connection refused until reviewed`
and a `REVIEW` action.
Pane footer: `strict host key checking on` · `biometric unlock on` · `export config` ·
`wipe all credentials` (error). Bottom: Android gesture pill (110×3px, `rgba(238,241,240,.35)`).

Settings items to implement (from the phone settings screen `1d`, same content model):
TRANSFERS — transfer on cellular (off), parallel transfers (3), bandwidth limit (20 MB/s),
resume interrupted (on). BROWSING — show hidden files (on), thumbnails over remote (off,
sub-label `reads first 4 MB per file`), default sort (`name ↑`), cache (`1.8 GB of 4 GB used`
+ `CLEAR`). SECURITY — unlock with biometrics (on), strict host key checking (on),
keys & credentials (`3 keys ›`). ABOUT — build string.

### 5. Phone references (`1a`, `1b`, `1c`, `1d`)
Same content at phone width. `1a` hairline/minimal (status dot + one accent action),
`1b` "path rail" (mono section headers `MOUNTED`/`IDLE`, 38px left proto gutter, crumb path bar,
chip filters `ALL/VIDEO/PINNED/.HIDDEN`, bottom mono tabs `REMOTES QUEUE KEYS PREFS` with a 2px
accent underline on the active tab), `1c` console (phone version of the tablet table:
`MODE | NAME | SIZE | MTIME`, footer verbs `mkdir rename chmod put↑ get↓`).
`1d` is Android phone: add-connection form, keys & credentials, settings.
If the phone build follows the tablet, use `1c`.

---

## Interactions & Behavior
- **Host tap** in rail: mount if idle (row proto goes `accent`, tree expands under it);
  stale mount (`plex-scratch`) offers remount; unreachable host shows `—` for FREE.
- **Row tap** selects (inspector updates, footer shows `n selected · size`); **double-tap /
  chevron** on a directory navigates; long-press starts multi-select (not yet designed —
  bulk-action bar is a known gap, see Open questions).
- **Path bar** segments are tappable ancestors; `filter…` filters the current listing live.
- **`get ↓` / `put ↑`** enqueue and immediately reflect in the queue strip and, for the affected
  row, an inline progress line (`↓ 61% · 8.4 MB/s`) plus accent wash in the file table.
- **Transfers** update ~1×/s: rate, ETA, bytes, bar width, and the 60s histogram (shift left,
  newest bar solid accent). Aggregate rate in strip and header stay in sync.
- **Failed job**: row turns `error`, RATE/ETA cells become `retry` / `skip`; auto-retry up to
  3 attempts, message names the errno (`EACCES`) and the resolved remote path.
- **New host**: probe runs on field blur / `TEST`; log appends a line per step; an unknown or
  changed host key blocks save until the fingerprint is accepted (`TRUST & SAVE`).
- **Changed known-host fingerprint** blocks connection until reviewed — surfaced both in
  settings and as a connect-time sheet.
- **Switches** flip immediately (no save button); settings persist per host where host-scoped.
- Transitions: keep them short and non-decorative — 120–160ms opacity/position for dialog and
  sheet, no bounce; progress bars animate width linearly with data, never eased.
- Hover is irrelevant on tablet; **pressed** state = row background `rgba(255,255,255,.04)`,
  chips darken to `rgba(255,255,255,.06)`. Focus ring: 1px accent underline on fields.
- **Empty / error states needed** (designed only implicitly so far): empty directory,
  no hosts configured, host unreachable, permission denied on listing, offline with pinned
  files only.
- **Responsive**: three panes at ≥1100px; at ~900px collapse the inspector into a right sheet;
  in portrait tablet show rail + table with the inspector as a bottom sheet; phone = stacked
  navigation per `1c`.

## State Management
- `hosts[]`: id, name, protocol (`sftp|nfs4|nfs3|smb3|file`), address, port, user, authMethod,
  keyId, initialPath, options (compression, followSymlinks, readOnly, showHidden), status
  (`mounted|idle|stale|unreachable`), freeBytes, rttMs, mountedAt, retrans.
- `selectedHostId`, `cwd`, `listing[]` (name, isDir, isSymlink, target, sizeBytes, mode,
  uid/gid, ownerLabel, mtime, isHidden, inode), `sortKey`/`sortDir`, `filterText`,
  `viewMode` (`list|grid`), `selection[]`.
- `inspector`: focused entry + lazily fetched extras (codec/dimensions from a partial read,
  sha256 on demand).
- `queue[]`: id, direction, name, hostId, remotePath, localPath, bytesDone, bytesTotal, rate,
  etaSeconds, state (`running|waiting|paused|failed|done`), attempt, errno, message,
  finishedAt, verified. Derived: aggregate rate, counts per state, 60s rate history ring buffer.
- `settings`: transfer + browsing + security groups above; `credentials`: keys[], passwords[],
  knownHosts[] (host, keyType, fingerprint, verifiedAt, changed).
- Data fetching: directory listings paged/virtualized (tables are long — virtualize rows);
  metadata and thumbnails fetched lazily per selection; transfers run in a background service
  (continues ~10 min after app close, per the design's footnote).

## Assets
No image or icon assets. Everything is type, hairlines, rectangles, and CSS bars:
- Folder indicator: 14×11px rounded rect, 1px `rgba(238,241,240,.3)` border.
- File preview: striped placeholder (see token above) — **replace with a real thumbnail**.
- Arrows/glyphs are text: `↓ ↑ ← ‹ › ✕ → ↑↓`. Status dots are 7px circles.
- Throughput histogram and progress bars are flex `<div>`s, no chart library required.
- If the codebase has an icon set, icons may replace the `[+] host`, `✕`, and direction arrows —
  but keep the mono-text treatment of data columns.

## Files
- `Remote Browser — Mockups.dc.html` — all mockups. Turn 2 (top of the page) = tablet console
  direction `2a`–`2d`; turn 1 (below) = phone directions `1a`–`1d`.
- `ios-frame.jsx`, `android-frame.jsx` — device bezel/status-bar scaffolding used by the phone
  mockups only (not part of the product design).
- `support.js` — runtime that renders the mockup file. Not product code.
Open the HTML file in a browser to review; the review canvas is pan/zoom.

## Open questions / known gaps
1. Multi-select + bulk action bar (copy/move/delete across hosts) is not designed yet.
2. Grid / thumbnail view for media folders is referenced in the toolbar (`GRID`) but not drawn.
3. Two-host split-pane copy (drag between remotes) discussed but not designed.
4. Empty, offline, and permission-denied states need a pass.
5. File preview (video/image/text viewer) not designed.
