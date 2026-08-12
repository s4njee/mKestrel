# mkestral — Build Plan

Remote file browser (SFTP / NFS / SMB / local) for iOS + Android tablets and phones,
built in **Rust with Dioxus**.

Design source of truth: [`design_handoff_remote_file_browser/README.md`](design_handoff_remote_file_browser/README.md)
and `Remote Browser - Mockups.dc.html` (screens `2a`–`2d` tablet console = primary,
`1a`–`1d` phone = secondary, `1c` is the phone expression to build).

---

## 1. Target stack

| Concern | Choice | Notes |
|---|---|---|
| UI framework | Dioxus (0.7.x line) | RSX + CSS. Confirm the current release at kickoff and pin it. |
| Mobile runtime | `dioxus-mobile` (wry/WebView) | The design is pure type + hairlines + flex bars — it maps 1:1 to CSS, so WebView rendering is the low-risk path. `dioxus-native`/Blitz is a later evaluation, not a v1 dependency. |
| Dev loop | `dx serve --platform desktop` for 95% of UI work; `--platform ios` / `--platform android` for device passes | Desktop window sized 1194×834 / 1280×800 reproduces the tablet mockups exactly. |
| State | Dioxus signals + a `Store` of domain signals in context | See Epic 3. |
| Async | `tokio` (multi-thread) via `dioxus::spawn` / `use_resource` | Transfers and probes are long-lived tasks, not component-scoped. |
| Layout language | CSS (single `assets/main.css` + per-screen sheets), design tokens as CSS custom properties | Mirrors the token table in the handoff. |

### Workspace layout

```
mkestral/
  Cargo.toml                # workspace
  crates/
    mk-core/                # domain types, no I/O: Host, Entry, Job, Settings, Credential
    mk-vfs/                 # VfsBackend trait + local/sftp/smb/nfs impls
    mk-transfer/            # transfer engine, queue, scheduler, rate history
    mk-secrets/             # Keychain / Keystore / biometric bindings
    mk-ui/                  # Dioxus components + screens (all rendering)
  app/                      # thin binary: platform bootstrap + Dioxus launch
  assets/                   # css, fonts
  design_handoff_remote_file_browser/
```

Rule: `mk-ui` depends on `mk-core` only. Backends are injected as trait objects so the UI
can be developed and tested entirely against `MockBackend`.

---

## 2. Milestones

| M | Name | Epics | Exit condition |
|---|---|---|---|
| M0 | Skeleton | E0, E1 | `dx serve` renders a token-accurate style gallery on desktop + one device. |
| M1 | Browsable (mock) | E2, E3, E5, E6 | `2a` pixel-close, fully interactive, backed by fixture data. |
| M2 | Real transport | E4, E7 | Real SFTP + local browsing; queue moves real bytes end-to-end (`2b`). |
| M3 | Full feature set | E8, E9, E12 | Add/edit host with live probe (`2c`), keys/known-hosts/settings (`2d`). |
| M4 | Ship-ready | E10, E11, E13, E14 | Phone layouts, all empty/error states, virtualized lists, CI + signed builds. |
| M5 | Backlog | E15 | Open questions from the handoff, scoped separately. |

Dependency notes: E4 (backends) can run in parallel with E5/E6 from day one because both
sides code against the `VfsBackend` trait defined in E4-S1. E7 needs E4-S1 + E3.

---

## Epic 0 — Project foundation

### E0-S1 — Workspace scaffold
Create the Cargo workspace and crate skeletons above; `app/` launches an empty Dioxus app.
**AC**
- `cargo build --workspace` succeeds on macOS host.
- `dx serve --platform desktop` opens a window rendering "mkestral".
- Crate dependency direction is enforced: `mk-ui` has no dependency on `mk-vfs`/`mk-transfer`.
- `rust-toolchain.toml` pins the toolchain; `rustfmt.toml` and `clippy.toml` committed.

### E0-S2 — Mobile targets build and run
**AC**
- `dx serve --platform ios` installs and launches on an iPad simulator (1194×834 class).
- `dx serve --platform android` installs and launches on an Android tablet emulator (1280×800).
- Both render the same placeholder screen; documented in `docs/dev-setup.md` with exact
  SDK/NDK versions and any `cargo-ndk` / codesigning steps.
- Known-good build recorded per platform (target triples, min OS versions) in the same doc.

### E0-S3 — Fixture data from the mockups
Encode every literal from the handoff (9 hosts, 24 directory entries, 6 queue jobs,
3 keys, 2 passwords, 4 known hosts, 60s rate history) as `mk-core` fixtures.
**AC**
- `mk_core::fixtures::demo_state()` returns a state matching the `2a`/`2b`/`2d` content lists
  exactly — names, sizes, modes, owners, mtimes, percentages, error strings.
- A `--demo` flag (and a debug-menu toggle) boots the app on fixtures with no network.
- Unit test asserts entry count = 142 summary / 24 visible rows and total `1.9T` footer math.

### E0-S4 — Debug/inspection affordances
**AC**
- A hidden dev drawer (long-press the status strip in debug builds) can switch backend
  (mock/real), force host states (`mounted|idle|stale|unreachable`), and inject a failed job.
- Not compiled into release builds (`#[cfg(debug_assertions)]`).

---

## Epic 1 — Design system

### E1-S1 — Design tokens
Port the handoff token table into CSS custom properties + a mirrored Rust `tokens` module
for anything needed in logic (e.g. progress-bar colours).
**AC**
- Every token in the handoff exists with the exact value: `bg #0b0c0c`, `bg-alt #0e100f`,
  `text #eef1f0`, the six `text-*` alphas, `accent #00c48f`, `accent-on #0b0f0e`,
  `accent-wash`, `warn #e0a83c`, `error #e07a6a`, `hairline`, `hairline-soft`,
  `border-ctl`, both `bezel` values.
- No raw hex/rgba literal appears anywhere in component CSS — CI greps for violations.
- Type scale defined as named classes: screen title, pane title mono, table name, list row
  title, data cell, column header, section label, mode, metric, big readout.
- Mono stack is `ui-monospace, "SF Mono", Menlo, monospace`; sans is system per platform.
- Verified: no gradients, no shadows (except the dialog's), no filled panels.

### E1-S2 — Primitive components
`Hairline`, `Row`, `ColumnHeader`, `DataCell`, `MonoChip`, `OutlineButton`, `AccentButton`,
`DangerButton`, `Switch`, `ProgressBar`, `StatusDot`, `UnderlineField`, `Histogram`.
**AC**
- `ProgressBar`: 3px tall, two flex children (`flex: pct` accent / `flex: 100-pct`
  `rgba(255,255,255,.08)`), 1px gap, square corners; a `compact` variant is 2px.
- `Switch`: 32–34 × 17–18px, knob 13–14px, on = accent fill + `#0b0f0e` knob,
  off = `rgba(255,255,255,.14)` + `rgba(238,241,240,.5)` knob; pill radius 9px.
- `Row` supports a `selected` state = accent wash background + 2px accent left border.
- Pressed state on rows = `rgba(255,255,255,.04)`; on chips = `rgba(255,255,255,.06)`.
- Field focus = 1px accent underline, accent label, caret `|` in the value.
- `Histogram` renders N bars with per-bar opacity ramp (older `.3`, recent `.45`,
  latest solid accent), 2px gaps, configurable height.

### E1-S3 — Style gallery screen
A dev-only route rendering every primitive in every state.
**AC**
- Reachable from the dev drawer; shows each primitive × each state side by side.
- Used as the visual-diff baseline in E14-S3.

### E1-S4 — Motion + interaction rules
**AC**
- Dialog and sheet transitions are 120–160ms opacity/position, no bounce, no spring.
- Progress bar width animates linearly with data, never eased.
- A `prefers-reduced-motion` path disables all transitions.
- No hover-only affordance exists anywhere (tablet-first).

---

## Epic 2 — App shell, navigation, responsive

### E2-S1 — Root shell and routing
Routes: `Browser`, `Queue`, `NewHost` (modal over Browser), `Settings/{section}`.
**AC**
- Dioxus router configured; deep link `mkestral://queue` opens the queue screen.
- Back/`←` behaviour matches platform convention (Android back button, iOS swipe/`←` chip).
- Modal routes render over the browser without unmounting it (state preserved).

### E2-S2 — Status strip
**AC**
- Renders `9:41 Tue 12 Aug` · spacer · `wifi 10.0.1.4` · `vpn` · `92%`, mono 10.5px at
  `text-40`, padding 5px 16px, bottom hairline.
- On device it reflects real clock, connectivity type + local IP, VPN flag, battery.
- Under the real platform status bar it is suppressed and safe-area insets are applied instead.

### E2-S3 — Responsive breakpoints
**AC**
- ≥1100px: three panes (rail 252 / table flex / inspector 296).
- ~900–1100px: inspector collapses into a right-side sheet, opened by row selection.
- Portrait tablet: rail + table, inspector as a bottom sheet.
- <768px: phone stacked navigation per `1c`.
- Rotating a device re-lays out with no state loss and no re-fetch of the current listing.

### E2-S4 — Persistent queue strip
**AC**
- Present on the browser screen at all breakpoints: `#0d0f0e`, hairline top, 8px 16px.
- Shows label `QUEUE`, two compact jobs (ellipsized name, pct in accent, 64px 3px bar),
  spacer, `3 queued · 1 failed`, aggregate `12.1M/s`, `pause all`.
- Tapping it navigates to the queue screen; `pause all` pauses without navigating.
- Hidden when the queue is empty and no job finished in the last 60s.

---

## Epic 3 — Domain state

### E3-S1 — Core types
**AC**
- `Host { id, name, group, protocol: Sftp|Nfs4|Nfs3|Smb3|File, address, port, user,
  auth: Key|Password|Agent|KbdInt, key_id, initial_path, options{compression,
  follow_symlinks, read_only, show_hidden}, status: Mounted|Idle|Stale|Unreachable,
  free_bytes, rtt_ms, mounted_at, retrans }`.
- `Entry { name, is_dir, is_symlink, target, size_bytes, mode, uid, gid, owner_label,
  mtime, is_hidden, inode }`.
- `Job { id, direction, name, host_id, remote_path, local_path, bytes_done, bytes_total,
  rate, eta_seconds, state: Running|Waiting|Paused|Failed|Done, attempt, errno, message,
  finished_at, verified }`.
- `Settings` (transfers/browsing/security groups) and `Credentials { keys, passwords,
  known_hosts }` per the handoff.
- All types are `serde`-round-trippable; property test proves round-trip stability.

### E3-S2 — Store, selectors, derived state
**AC**
- A `Store` in context exposes signals: `hosts`, `selected_host_id`, `cwd`, `listing`,
  `sort_key`/`sort_dir`, `filter_text`, `view_mode`, `selection`, `inspector`, `queue`,
  `settings`, `credentials`.
- Derived (memoized) values: aggregate rate, per-state job counts, per-host rate,
  60s rate ring buffer, footer totals (`142 entries · 1.9T`, `1 selected · 24.1G`).
- Changing `filter_text` re-renders only the file table, verified by a render-count test.

### E3-S3 — Persistence
**AC**
- Hosts, settings, known hosts and queue state persist across app restarts (SQLite or
  a versioned JSON store under the platform app-support dir).
- Schema is versioned with a migration hook; a corrupt store is quarantined, not fatal.
- Secrets are **never** written here — only key/credential IDs (see E12).
- Queue restored on launch: `running` jobs come back as `waiting`, not silently dropped.

---

## Epic 4 — VFS backends

### E4-S1 — `VfsBackend` trait
**AC**
- Async trait: `connect`, `disconnect`, `stat`, `list(path, page)`, `read_range`,
  `open_read`, `open_write`, `mkdir`, `rename`, `chmod`, `remove`, `statfs`, `probe`.
- Errors are a typed enum carrying an errno-equivalent and the resolved remote path
  (so `EACCES · /etc/nginx/nginx.conf not writable by deploy` is reconstructible).
- `MockBackend` implements it over E0-S3 fixtures with configurable latency and failures.
- Trait is object-safe and injected through context; swapping backends needs no UI change.

### E4-S2 — Local backend
**AC**
- Lists, reads, writes, renames, deletes on device-local storage.
- Android: SAF/scoped-storage document tree; iOS: app sandbox + `UIDocumentPicker` folder.
- `statfs` reports free space (`localhost file 41G` shape).
- Downloads land in the user's Downloads/Files location and are visible to the OS file app.

### E4-S3 — SFTP backend
**AC**
- Pure-Rust SSH (russh + SFTP layer) so it cross-compiles to iOS/Android without C toolchain
  pain; if a C library is used instead, the build doc records the NDK/iOS build recipe.
- Auth: publickey (ed25519, rsa, ecdsa), password, keyboard-interactive, agent where present.
- Host key verification against the known-hosts store; unknown/changed key surfaces the
  handoff's trust flow rather than connecting.
- Supports resumed reads/writes via offsets (needed by E7-S4).
- Keepalive interval honours the host's `KEEPALIVE 30s` setting.
- Integration test runs against a containerized `openssh-server`.

### E4-S4 — SMB backend
**AC**
- SMB3 list/stat/read/write against a Samba container.
- Auth with saved credentials from `mk-secrets`; guest/anonymous supported.
- Free-space reporting maps to the FREE column.
- Dependency choice (bindings vs pure Rust) documented with its mobile build steps.

### E4-S5 — NFS spike + backend
**Spike first — this is the highest-risk item in the plan.** Neither iOS nor Android lets an
app mount NFS; a userspace NFSv3/v4 client is required.
**AC (spike)**
- Written finding: viable crate or protocol-implementation scope, effort estimate, and
  whether NFSv4 or NFSv3-only is realistic for v1.
- Decision recorded: implement, defer, or drop NFS from v1 (and how the UI degrades if
  deferred — the `nfs4`/`nfs3` hosts must still render, just not mount).
**AC (implementation, if greenlit)**
- Userspace mount/list/read/write, `rsize`/`wsize` reported to the inspector footer
  (`nfs4 · rsize 1M wsize 1M · hard,intr`), retrans counter surfaced.
- `Stale` status detected and offered as a remount (the `plex-scratch` case).

### E4-S6 — Connection lifecycle
**AC**
- A connection pool keeps at most N live sessions; idle sessions close after a timeout.
- Host status transitions (`idle → mounted → stale → unreachable`) are driven by real probe
  and error signals and are reflected in the rail within 1s.
- Network loss/regain is detected and re-establishes affected sessions without user action.
- App background/foreground does not corrupt in-flight sessions.

---

## Epic 5 — Browser screen (`2a`)

### E5-S1 — Path bar
**AC**
- Left: `media-nas:/export/media/films` — host in accent, middle segments `text-45`,
  current segment `text`; ancestor segments are tappable and navigate.
- Right cluster, mono 10–11px: `filter…` field (1px `border-ctl`, radius 4,
  padding `4px 26px 4px 8px`), `SORT name↑`, `LIST` (active `text-50`) / `GRID` (`text-30`),
  `QUEUE 2` in accent.
- Typing in `filter…` filters the current listing live (case-insensitive substring).
- `SORT` cycles key and direction and the header caret follows.
- `QUEUE n` badge equals active+waiting count and navigates to the queue.
- Padding 9px 16px, bottom hairline.

### E5-S2 — Hosts rail (252px)
**AC**
- Column header `HOST | PROTO | FREE` at 9.5px mono 500, `.10em`, uppercase, `text-30`.
- Rows 7px 14px: name 12.5px mono 500; proto 9.5px mono coloured accent when mounted,
  `text-35` idle, `warn` stale; free right-aligned, `—` when unreachable.
- All nine fixture hosts render with their exact proto/free values.
- Selected host: accent wash + 2px accent left border.
- Inline tree renders under the selected host: children indented 10/20px, 11px mono,
  19px line-height, current dir in `text`; expanding fetches children lazily.
- `RECENT` section (5 paths, 10.5px mono) and `PINNED OFFLINE · 3` (name + size).
- Footer above hairline, 9px 14px, 10px mono 600: `[+] host` (accent), `keys`, `prefs` —
  each navigating correctly.

### E5-S3 — File table
**AC**
- Header `MODE | NAME | SIZE | MTIME | OWNER`; column widths indicator 14, MODE 66,
  NAME flex, SIZE 58 right, MTIME 82 right, OWNER 70 right; cell gap 12px.
- Rows are `padding: 5px 16px` (~22px tall).
- First row is the `..` parent showing `../media` in `text-45`; tapping navigates up.
- Directories: 14×11px rounded-rect indicator (1px `rgba(238,241,240,.3)`), name weight 500
  with trailing `/`, SIZE column shows item count (`31 items`).
- Files: no indicator, name weight 400, SIZE humanized (`18.4G`, `84K`).
- Symlink row renders `latest → 4K_HDR` with target in `text-40` and SIZE `link`.
- Dotfiles: name `text-45`, mode at `text-25`; hidden per the `show hidden files` setting.
- Selected row: accent wash + 2px accent left border, with MODE and SIZE rendered in accent.
- A downloading row shows an inline progress line `↓ 61% · 8.4 MB/s` plus accent wash.
- All 24 fixture rows render in fixture order with exact strings.

### E5-S4 — File table footer
**AC**
- 10px mono `text-32`, hairline top: `142 entries · 1.9T` · `1 selected · 24.1G` · spacer ·
  `mkdir` `rename` `chmod` · `put ↑` `get ↓` (accent).
- Counts and sizes are computed from state, not hardcoded.
- `mkdir`/`rename`/`chmod` open their dialogs; verbs disable (at `text-30`) when the
  selection or host permissions make them invalid, or on a read-only mount.
- `get ↓` / `put ↑` enqueue immediately and the queue strip updates in the same frame.

### E5-S5 — Navigation and selection semantics
**AC**
- Single tap selects (inspector updates, footer shows `n selected · size`).
- Double-tap or chevron on a directory navigates into it.
- Long-press begins multi-select (selection model supports it now; the bulk-action bar is
  E15-S1 — until then long-press extends `selection[]` and updates the footer count).
- Navigation pushes history; back returns to the previous directory with scroll position
  and selection restored.
- Directory listing shows a skeleton, never a blank pane, while loading.

### E5-S6 — File operations
**AC**
- `mkdir`, `rename`, `chmod` dialogs match the dialog styling of `2c` (mono fields,
  underline inputs, accent primary).
- `chmod` accepts octal and shows the symbolic form live (`0644 -rw-r--r--`).
- `RM` requires confirmation and is styled with `error` text + `rgba(224,122,106,.35)` border.
- Failures surface the errno and resolved path inline; the listing refreshes on success.

---

## Epic 6 — Inspector (296px)

### E6-S1 — Layout and preview
**AC**
- Header `INSPECTOR`; 166px preview area using the striped placeholder
  `repeating-linear-gradient(45deg,#141615 0 6px,#101211 6px 12px)` with mono caption
  `frame preview · 4 MB read` until a real thumbnail resolves.
- Real thumbnails/frames replace the placeholder when available and when the
  `thumbnails over remote` setting is on; caption reports actual bytes read.
- File name at 12.5px mono with `word-break: break-all`.

### E6-S2 — Metric rows
**AC**
- Label column 74px `text-32`, value `text-70`, both 10.5px mono.
- Rows: size (`24.1G / 25,884,266,496 B`), mode (`0644 -rw-r--r--`), owner
  (`media:media 1000:1000`), mtime (`2026-05-02 11:20:44`), codec
  (`hevc 3840×2160 · dts-hd`), inode (`8419772`).
- Codec/dimension data is fetched lazily by partial read and is omitted (not blank-labelled)
  when unavailable; `SHA256` is computed only on demand.

### E6-S3 — Live transfer block + actions
**AC**
- When the focused entry is transferring: `downloading` / `61% · 8.4M/s`, 3px bar,
  `→ localhost:/Downloads · eta 19m`, updating ~1×/s.
- Action chips wrap with 7px gap, 10px mono 600: `GET ↓` (accent fill), `PIN OFFLINE`,
  `CHMOD`, `RENAME`, `SHA256` (outlined), `RM` (error).
- `GET ↓` becomes `PUT ↑` when the focused pane is local.
- Chips disable correctly for directories, symlinks, and read-only mounts.
- Footer 9.5px mono `text-28`: `nfs4 · rsize 1M wsize 1M · hard,intr` /
  `mounted 4h 12m · 0 retrans`, generated from the live session.

---

## Epic 7 — Transfer engine + queue (`2b`)

### E7-S1 — Transfer engine
**AC**
- Jobs run in a background task pool honouring `parallel transfers` (default 3).
- Per-job progress events (bytes, rate, ETA) emit at ~1Hz and are coalesced so the UI
  re-renders at most once per tick.
- Global bandwidth limit (`20 MB/s` default) is enforced across all jobs.
- Chunk size configurable (default 1 MiB).
- Pause/resume/cancel per job and `pause all` globally, all effective within one tick.

### E7-S2 — Queue screen chrome
**AC**
- Header `queue` + `2 running · 3 waiting · 1 failed · 14 done today` (live counts),
  right actions `PAUSE ALL`, `CLEAR DONE`, `LIMIT 20M/s` (accent, opens the limit editor).
- Table columns `DIR | FILE | HOST · PATH | RATE | ETA | PROGRESS` with widths
  18 / flex / 150 / 58 / 54 / 70; rows `7px 16px`.
- Pane footer: `session 4h 12m` · `up 2.1G · down 38.4G` · `0 retrans · 0 timeouts`.

### E7-S3 — Queue row states
**AC**
- Active: accent wash, `↓`/`↑` in accent, rate `8.4M/s`, ETA `19m 04s`, progress
  `14.7/24.1G` in accent, 3px bar under the row.
- Waiting: `text-55` name, RATE `waiting` — or `host down` in `warn` — ETA `—`,
  progress `0/31.7G`.
- Failed: arrow and progress in `error`; RATE cell reads `retry`, ETA cell reads `skip`,
  both tappable; a second line indented 30px, 9.5px mono `error`, reads
  `EACCES · /etc/nginx/nginx.conf not writable by deploy · attempt 2 of 3 at 09:44`.
- `COMPLETED · TODAY` group: rows at `text-42`/`text-25`, PROGRESS column reads `verified`.
- `CLEAR DONE` removes only the completed group.

### E7-S4 — Retry, resume, verification
**AC**
- Failed jobs auto-retry up to 3 attempts with backoff; the attempt counter and timestamp
  appear in the error line.
- `retry` restarts immediately and resets the counter; `skip` marks the job skipped and
  advances the queue.
- Interrupted transfers resume from the last byte when `resume interrupted` is on and the
  backend supports ranged access; otherwise they restart and say so.
- With `verify sha256 on finish` on, completed jobs are hashed and show `verified`;
  a mismatch fails the job with a distinct message.

### E7-S5 — Throughput panel (296px)
**AC**
- `THROUGHPUT · 60s` header; 24px mono 600 readout `12.1` plus `MB/s · cap 20`.
- 16-bar histogram, 56px tall, 2px gaps, older bars `rgba(0,196,143,.3)`, recent `.45`,
  latest solid accent; shifts left once per second; axis labels `-60s` / `now`.
- Setting rows: `parallel jobs 2 of 3`, `chunk size 1 MiB`, `on cellular paused` (warn),
  `verify sha256 on finish` — all live-bound to settings.
- `PER HOST` rows: name, rate, 52px mini bar, sorted by rate.
- Footer note: `transfers continue in background / for 10 min after app is closed`.

### E7-S6 — Background execution
**AC**
- Android: a foreground service with a progress notification keeps transfers alive.
- iOS: background URLSession-equivalent or background task assertion; the ~10 min claim in
  the design is either honoured or the footer copy is corrected to what the platform allows.
- `transfer on cellular = off` pauses jobs when the connection becomes cellular and shows
  `paused on cellular` in `warn`; jobs auto-resume on wifi.
- Killing and relaunching the app restores queue state (E3-S3) and offers to resume.

---

## Epic 8 — New / edit connection (`2c`)

### E8-S1 — Dialog shell
**AC**
- 820px wide, centred; background browser at `opacity: .28` under a `rgba(6,8,7,.62)` scrim.
- Surface `#0e100f`, 1px `rgba(255,255,255,.12)`, radius 10,
  shadow `0 40px 90px rgba(0,0,0,.6)`.
- Title bar: `new host` (13px mono 600) — `edit host` in edit mode — right `IMPORT ~/.ssh/config`
  and `✕`; `✕` and scrim tap prompt before discarding unsaved edits.
- Enter/Escape map to the primary/cancel actions; opens in 120–160ms with no bounce.
- Below 900px the dialog becomes a full-screen sheet with the same content order.

### E8-S2 — Protocol + form fields
**AC**
- Chip row `SFTP` (accent fill when selected) `NFS` `SMB` `LOCAL` (outlined); right hint
  `openssh 9.6 detected` derived from the real environment or hidden.
- Switching protocol swaps the field set (port default, auth options, mount options) without
  losing shared values.
- Underline fields: label 9px mono `.14em` `text-32`, value 13.5px mono; focused field's
  underline and label go accent and the value shows a caret.
- Fields `NAME`, `GROUP`, `HOST`, `PORT`, `USER`, `INITIAL PATH`, `KEEPALIVE` with the
  fixture defaults; validation errors render inline in `error` without shifting layout.
- `AUTH` chips `KEY` (accent outline when selected) `PASSWORD` `AGENT` `KBD-INT`.
- Selected key card (1px `border-ctl`, radius 6) shows `id_ed25519`,
  `SHA256:v8Kx7dR…q2Lp · Secure Enclave · Face ID`, trailing `CHANGE` opening the key picker.
- `IMPORT ~/.ssh/config` parses Host blocks and pre-fills a chosen entry.

### E8-S3 — Options + probe
**AC**
- Right column (300px) `OPTIONS` switch rows: compression **on**, follow symlinks off,
  read-only mount off, show hidden **on** — flipping persists per host with no save button.
- `PROBE` log box (`#0a0b0b`, 1px `rgba(255,255,255,.07)`, radius 5, 9.5px/16px mono)
  appends one line per real step: DNS resolve → IP, `tcp 22 open · 24 ms`, server banner,
  `host key not in known_hosts` (warn), fingerprint, and
  `auth ed25519 accepted · /srv/www readable` (accent).
- Probe runs on field blur and on `TEST`; it is cancellable and never blocks the UI.
- Failures append a `error`-coloured line with the underlying cause.

### E8-S4 — Save semantics
**AC**
- Primary CTA copy is state-driven: `TEST` before a successful probe, `TRUST & SAVE` when an
  unknown/changed host key must be accepted, `SAVE` otherwise.
- `CANCEL` (outlined) and the primary (accent fill) are equal width.
- Save is blocked while an unknown or changed host key is unaccepted.
- Accepting writes the fingerprint to known hosts (E9-S3) with a timestamp.
- Saved host appears in the rail immediately, in its group, with status `idle`.

---

## Epic 9 — Settings, keys, known hosts (`2d`)

### E9-S1 — Settings shell
**AC**
- Top bar: `←`, `prefs`, right `build 2026.08.3 · libssh2 1.11 · nfs-client 4.2` generated
  from the real build and dependency versions.
- Left nav 236px with hairline right: `transfers`, `browsing`, `keys & credentials`,
  `known hosts`, `cache & offline`, `appearance`, `about`; selected item = accent wash +
  2px accent left border, `text` at weight 500.
- Nav footer note: `3 keys · 2 passwords / 4 known hosts · 1 changed` (live counts).
- On phone the nav collapses to a list that pushes the section (per `1d`).

### E9-S2 — Settings sections
**AC**
- TRANSFERS: transfer on cellular (off), parallel transfers (3), bandwidth limit (20 MB/s),
  resume interrupted (on).
- BROWSING: show hidden files (on), thumbnails over remote (off, sub-label
  `reads first 4 MB per file`), default sort (`name ↑`), cache (`1.8 GB of 4 GB used` +
  `CLEAR`).
- SECURITY: unlock with biometrics (on), strict host key checking (on),
  keys & credentials (`3 keys ›`).
- ABOUT: build string, licenses, open-source notices.
- Every control writes through to `Settings` immediately and is observably honoured by the
  engine (e.g. lowering the limit throttles a running job within one tick).
- `CLEAR` cache shows freed bytes and cannot delete pinned-offline files.

### E9-S3 — Keys & credentials
**AC**
- Table `TYPE | NAME · FINGERPRINT | STORAGE | HOSTS | LAST USED`, widths 74 / flex /
  130 / 70 / 82.
- Fixture rows render exactly: `ed25519 · id_ed25519 · SHA256:v8Kx…q2Lp · Keystore ·
  biometric · 2 · 4 min ago` (selected); `rsa 4096 · id_rsa_legacy · passphrase only`
  in `warn` · `1 · 8 mo ago`; `ecdsa p256 · ci-runner · Keystore · 1 · 2 d ago`.
- Action row: `GENERATE ed25519` (accent fill), `IMPORT FILE`, `PASTE PEM` (outlined),
  right-aligned `REVOKE UNUSED` (error outline).
- Generating creates a real keypair in platform secure storage; the public key is
  copyable/exportable, the private key is never displayed.
- `IMPORT FILE` / `PASTE PEM` accept OpenSSH and PEM, prompt for passphrase when needed.
- `REVOKE UNUSED` lists what will be deleted and requires confirmation.
- `SAVED PASSWORDS · 2` rows show proto, account, storage, `REVEAL`, last used; `REVEAL`
  requires biometric/passcode re-auth.

### E9-S4 — Known hosts
**AC**
- `KNOWN HOSTS · 4` rows; verified hosts show a 7px accent dot.
- The changed host renders fully in `error` with sub-line
  `fingerprint changed since 04 Aug · connection refused until reviewed` and a `REVIEW` action.
- With strict checking on, a changed fingerprint blocks connection until reviewed — surfaced
  both here and as a connect-time sheet showing old vs new fingerprint.
- Reviewing lets the user accept the new key or delete the entry; the action is logged.
- Pane footer: `strict host key checking on` · `biometric unlock on` · `export config` ·
  `wipe all credentials` (error, double-confirmed).

---

## Epic 10 — Phone build (`1c`)

### E10-S1 — Phone browser (console expression)
**AC**
- Table columns `MODE | NAME | SIZE | MTIME`, rows `9–12px 18–20px`, cell gap 10px.
- Footer verbs `mkdir rename chmod put↑ get↓`.
- Renders correctly at 402×874 (iOS) and 412×892 (Android) with safe-area insets.
- Host picker and inspector are pushed screens / bottom sheets rather than panes.

### E10-S2 — Phone queue, add-host, settings
**AC**
- Queue, add-connection and settings screens reflow to a single column with identical
  content and the same 2px-underline mono tab treatment where tabs appear.
- Progress bars use the 2px compact variant.
- Android gesture pill area (110×3px, `rgba(238,241,240,.35)`) is never overlapped.

### E10-S3 — Platform conventions
**AC**
- Android hardware/gesture back navigates up one directory, then one screen, then exits.
- iOS edge-swipe back is enabled on pushed screens and disabled inside modal dialogs.
- Text-size accessibility settings scale UI text without breaking column alignment
  (mono data columns may cap their scale — documented decision).

---

## Epic 11 — Empty, error, offline states

### E11-S1 — Empty and error states
**AC**
- Designed and implemented for: empty directory, no hosts configured, host unreachable,
  permission denied on listing, offline with pinned files only.
- Each uses the same language as the rest of the app: mono text, hairlines, at most one
  accent action; no illustrations.
- Permission-denied shows the errno and the resolved path.
- "No hosts configured" offers `[+] host` as its single accent action.

### E11-S2 — Connectivity and degradation
**AC**
- Going offline marks affected hosts `unreachable` (FREE `—`) within 5s and pauses jobs
  with a queue banner rather than failing them.
- Pinned-offline files remain browsable and openable while offline.
- Regaining connectivity re-probes hosts and resumes paused jobs automatically.
- Stale mounts (`plex-scratch`) render `stale` in `warn` and offer remount on tap.

---

## Epic 12 — Platform security & storage

### E12-S1 — Secure storage
**AC**
- `mk-secrets` stores private keys and passwords in iOS Keychain / Secure Enclave and
  Android Keystore; the app database stores only opaque IDs.
- Storage backing is reported to the UI exactly as designed: `Secure Enclave · Face ID`,
  `Keystore · biometric`, `passphrase only` (warn).
- Secrets never appear in logs, crash reports, or exported config.

### E12-S2 — Biometric unlock
**AC**
- With `unlock with biometrics` on, using a key or revealing a password requires Face ID /
  Touch ID / Android biometric prompt.
- Fallback to device passcode; repeated failure blocks the action with a clear message.
- Backgrounding the app for longer than a configured interval re-locks credentials.

### E12-S3 — Config export / wipe
**AC**
- `export config` produces a shareable file containing hosts, settings and known hosts and
  **no** secret material; the file states this in a header comment.
- `wipe all credentials` removes every key, password and known host from secure storage and
  the database, requires double confirmation, and is irreversible by design.

---

## Epic 13 — Performance

### E13-S1 — Virtualized listings
**AC**
- The file table virtualizes rows; a 100k-entry directory scrolls at ≥55fps on a mid-tier
  Android tablet with memory flat in scroll.
- Listings are fetched paged/incrementally; the first page paints before the rest arrives.
- Sorting and filtering 100k entries stay under 100ms (computed off the render thread).

### E13-S2 — Update budget
**AC**
- 1Hz transfer ticks re-render only the queue rows, strip, inspector block and histogram —
  proven by a render-count instrumentation test.
- Thumbnail/metadata fetches are cancelled when the selection changes.
- Idle CPU with an active transfer stays under an agreed budget (measured, recorded in
  `docs/perf.md`).

### E13-S3 — Startup and binary size
**AC**
- Cold start to interactive browser under 1.5s on reference devices.
- Release binary size recorded per platform with a regression check in CI.

---

## Epic 14 — Quality, CI, release

### E14-S1 — Unit and integration tests
**AC**
- `mk-core` formatting helpers (sizes, rates, ETA, modes, timestamps) covered by tests that
  assert the exact strings from the mockups (`18.4G`, `8.4M/s`, `19m 04s`, `-rw-r--r--`).
- `mk-transfer` scheduler tested for parallelism limits, bandwidth capping, retry/backoff,
  resume, and verification, with a simulated clock.
- `mk-vfs` backends tested against containerized SFTP and SMB servers.

### E14-S2 — Component and interaction tests
**AC**
- Each screen has tests over the mock backend covering the interactions in the handoff:
  host tap/mount, row select, directory navigate, filter, enqueue, retry/skip, probe flow,
  fingerprint-change block.
- Tests assert rendered text and state classes, not internal implementation.

### E14-S3 — Visual regression
**AC**
- Screenshot baselines for `2a`, `2b`, `2c`, `2d`, `1c` at the design device sizes, taken
  from the fixture state.
- Baselines are diffed in CI; a threshold failure blocks merge.
- A one-page comparison doc puts each baseline next to the mockup for sign-off.

### E14-S4 — CI pipeline
**AC**
- CI runs fmt, clippy (`-D warnings`), tests, and builds desktop + iOS + Android on every PR.
- Backend integration tests run against service containers.
- Build artifacts (`.ipa`/`.apk`) are produced on main.

### E14-S5 — Release readiness
**AC**
- Signed builds distributed to TestFlight and an Android internal track.
- Crash reporting and opt-in diagnostics wired, with secrets redacted.
- Store metadata, privacy disclosures (network access, local storage) and licenses prepared.
- `docs/` covers dev setup, architecture, backend recipes and the token map to the handoff.

---

## Epic 15 — Backlog (handoff open questions)

Each of these is unresolved in the design and needs a design pass before implementation.

### E15-S1 — Multi-select bulk action bar
Copy/move/delete across hosts. Selection model already exists (E5-S5).
**AC** — Design approved; bar appears on multi-select with counts and byte totals; bulk ops
enqueue as grouped jobs with a single progress row.

### E15-S2 — Grid / thumbnail view
`GRID` exists in the toolbar but is undrawn.
**AC** — Design approved; toggling `GRID` renders a thumbnail grid honouring the
`thumbnails over remote` setting; selection and inspector behave identically to list mode.

### E15-S3 — Two-host split-pane copy
**AC** — Design approved; two host panes side by side; a drag between them enqueues a
host-to-host transfer (streamed through the device, or server-side where supported).

### E15-S4 — File preview
Video / image / text viewer.
**AC** — Design approved; preview streams rather than downloading whole files; text viewer
handles large files by ranged reads.

---

## 3. Definition of done (every story)

1. Matches the handoff spec — tokens, metrics and copy verified against the mockup.
2. Works on desktop, iPad-class tablet, Android tablet, and both phone sizes where in scope.
3. No hardcoded colours, sizes, or strings that belong in tokens/fixtures/i18n.
4. Tests at the level the story implies (unit / integration / visual), all green in CI.
5. Loading, empty and error paths handled — never a blank pane, never a silent failure.
6. `cargo clippy -D warnings` and `cargo fmt --check` clean.
7. Secrets and paths absent from logs.

## 4. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| No usable NFS client for mobile | `nfs4`/`nfs3` hosts — a third of the design — can't mount | E4-S5 spike **first**; decide implement/defer/drop before M2 ends |
| SMB library cross-compilation to iOS/Android | Blocks E4-S4 | Evaluate binding vs pure-Rust in the same spike window as NFS |
| Dioxus mobile maturity (WebView perf, background tasks, platform APIs) | Perf and background transfers | E13 budgets measured early on real devices; platform APIs behind thin FFI shims so they can be swapped |
| iOS background execution limits vs the "10 min after close" claim | Design copy may be unachievable | E7-S6 either honours it or corrects the copy — decided, not left ambiguous |
| Pixel-close fidelity at 9.5–10.5px mono across two platforms' font stacks | Visual drift | Token-only styling + E14-S3 screenshot baselines per device size |
| Large-directory performance over slow links | Unusable on real NAS | Paged listings + virtualization designed in from E5-S3, not retrofitted |

## 5. Immediate next steps

1. E0-S1 → E0-S3 (workspace, mobile targets, fixtures).
2. Start the E4-S5 NFS spike and the E4-S4 SMB dependency evaluation in parallel — their
   outcome changes v1 scope.
3. E1-S1/E1-S2 tokens and primitives, then the E1-S3 gallery as the fidelity checkpoint
   before any screen work begins.
