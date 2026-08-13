# mkestral — feature backlog

What a serious SFTP/NFS client is expected to do, measured against what
mkestral does today (as of `d106f79`, 2026-08-12).

`plan.md` is the *build* plan — it tracks the design handoff to pixel fidelity
and is nearly feature-complete against the mockups. This file is the *product*
backlog: it covers the protocol- and file-manager-level capability that the
mockups never drew, plus the correctness gaps the current implementation
carries. Where an item is already an epic/story in `plan.md`, that story is
cited; where it is net-new, it is marked **new**.

## Status legend

| Mark | Meaning |
|---|---|
| ✅ | Implemented and exercised against a real host |
| 🟡 | Partially implemented, or implemented against fixtures only |
| ⬜ | Not implemented |

## Priority legend

- **P0** — blocks calling the app safe or correct; ship-stoppers.
- **P1** — expected by anyone who has used FileZilla / Cyberduck / Solid Explorer; absence is felt immediately.
- **P2** — depth that separates a good client from a demo.
- **P3** — differentiators, nice-to-have, or long-tail protocol coverage.

---

## 0. Where the code stands today

| Area | State |
|---|---|
| `VfsBackend` trait (connect/stat/list/read/write/mkdir/rename/chmod/remove/statfs/probe) | ✅ `crates/mk-vfs/src/lib.rs` |
| SFTP backend (russh + russh-sftp) | ✅ list/stat/read/write/rename/chmod/remove/statfs against real hosts |
| NFSv3 backend (nfs-rs, userspace, `noresvport`) | ✅ single fixed export per host |
| Local backend | ✅ incl. true ranged reads |
| SMB backend | ⬜ routes to `UnsupportedBackend` (`app/src/main.rs:296`) |
| Transfer engine (streaming, token-bucket cap, cancel, progress) | ✅ single-file only |
| Scheduler (parallelism, cap scaling, backoff/retry) | ✅ pure + unit-tested |
| Connection pool + idle reaper | ✅ `crates/mk-vfs/src/pool.rs` |
| Media streaming (loopback HTTP + Range → VLC) | 🟡 desktop only |
| Persistence (hosts/settings/queue JSON store) | ✅ |
| Secure credential storage (`mk-secrets`) | ⬜ `pub fn placeholder()` |
| Host-key verification | ⬜ **accepts every key** (`crates/mk-vfs/src/sftp.rs:42`) |

---

## 1. Correctness and safety (P0)

These are gaps in code that already exists. Each one is a way the app can
currently lose data, leak credentials, or lie to the user.

### B-1 — Real host-key verification ⬜ P0
`ClientHandler::check_server_key` returns `Ok(true)` unconditionally
(`crates/mk-vfs/src/sftp.rs:42`), so every SFTP connection is trivially
MITM-able. The UI already has a full known-hosts model
(`mk_core::credentials::KnownHost`, the `2d` review flow, `strict host key
checking`) — none of it is wired to the transport. Extends `plan.md` E4-S3 /
E9-S4.
- Backend reads and writes the persisted known-hosts store; TOFU only on first
  contact, and only with an explicit user accept.
- A changed fingerprint refuses the connection and raises the connect-time
  trust sheet showing old vs. new, with the `REVIEW` path from `2d`.
- `strict host key checking` in settings is honoured by the transport, not just
  displayed.
- Support `@cert-authority` and `@revoked` markers, hashed (`|1|`) entries, and
  non-default ports (`[host]:2222`).

### B-2 — Real secure storage for keys and passwords ⬜ P0
`mk-secrets` is a stub. Passwords live in an in-process
`Arc<Mutex<HashMap<String,String>>>` (`crates/mk-vfs/src/sftp.rs:23`) and the
private key is whatever sits at `~/.ssh/id_ed25519` (`app/src/main.rs:233`) —
`host.key_id` is ignored entirely. Implements `plan.md` E12-S1.
- iOS Keychain / Secure Enclave and Android Keystore behind one trait; the JSON
  store keeps opaque IDs only.
- `host.key_id` resolves through the vault; no hardcoded paths.
- Encrypted-at-rest passphrase cache with a configurable lifetime.
- Passwords, key material and passphrases never reach logs, panics, or the
  exported config.

### B-3 — SHA-256 verification actually verifies ⬜ P0
`app/src/transfer.rs:156` computes the local digest and discards it
(`let _ = sha256_file(...)`), so `verify sha256 on finish` shows `verified` on
a file nothing checked. Completes `plan.md` E7-S4.
- Compare against a server-side digest where obtainable (`sha256sum` over an
  SSH exec channel for SFTP, or a full read-back otherwise) and record which
  method was used.
- A mismatch fails the job with a distinct message and does not mark the file
  verified.
- Where no digest is obtainable, the UI says `size-checked`, not `verified`.

### B-4 — Atomic writes and partial-file cleanup ⬜ P0 **new**
Both `download` and `upload` write straight to the destination path. A failed
or cancelled transfer leaves a truncated file that looks complete.
- Download to `<name>.mkpart`, `fsync`, then rename into place.
- Upload the same way where the backend supports rename-over.
- Cancel/failure removes the partial unless resume is enabled and the partial
  is recorded in the queue.
- On relaunch, orphaned `.mkpart` files are offered for resume or cleanup.

### B-5 — Resume from offset ⬜ P0
`store.rs:1172` restarts every resumed job from byte 0, and download opens the
destination with `File::create` (truncating). `resume interrupted` is a setting
that does nothing. Completes `plan.md` E7-S4.
- Resume seeks the remote read to the partial length and appends locally
  (`ReadStream::seek` already exists).
- Upload resumes by seeking the remote write handle where the backend permits.
- Length/mtime sanity check before resuming; restart with an explicit notice
  when the remote changed.

### B-6 — Overwrite / conflict policy ⬜ P0 **new**
Transfers silently clobber existing files at the destination.
- Per-transfer and default policy: overwrite, skip, resume, rename, or
  newer-only.
- A conflict prompt on single transfers, and an apply-to-all path for batches.
- Same policy applies to move/copy within a host.

### B-7 — Free-space and quota preflight ⬜ P1 **new**
`statfs` exists on every backend but nothing consults it before enqueueing.
- Downloads check local free space against the job's total; uploads check the
  remote's.
- Insufficient space blocks the enqueue with the shortfall, rather than failing
  mid-copy.
- Aggregate the check across a queued batch, not per job.

### B-8 — Preserve timestamps and permissions ⬜ P1 **new**
Transfers reproduce bytes only; mtime and mode are lost.
- `preserve mtime` and `preserve mode` settings, honoured in both directions
  (SFTP `setstat`, NFS `setattr` — both already reachable).
- Uploaded files land with a configurable umask when preservation is off.

### B-9 — Recursive directory transfer ⬜ P0
`GET ↓` / `PUT ↑` handle one file. Directories cannot be transferred at all —
the single most conspicuous absence for a file manager.
- Recursive walk producing a single grouped queue entry with an aggregate
  progress row and per-file drill-down.
- Directory structure recreated on the destination, empty dirs included.
- Symlink policy: skip, follow, or recreate.
- Cycle detection on followed symlinks.
- Per-file failures do not abort the group; the group reports `n of m failed`.

### B-10 — Recursive delete ⬜ P1
`remove` unlinks a file, else removes an *empty* directory
(`crates/mk-vfs/src/sftp.rs:338`). Deleting a populated tree fails halfway.
- Recursive delete with a preflight count (`delete 1,204 items · 24.1G?`).
- Progress and cancel for long deletes; report what was already removed on
  cancel.

---

## 2. Core file management (P1)

The verbs a file manager is judged on. Currently implemented: `mkdir`,
`rename`, `chmod`, `remove`, `get`, `put`.

### B-11 — Copy and move ⬜ P1 **new**
Only same-directory `rename` exists. There is no copy at all, and no move
across directories.
- Copy/move within a host, server-side where the protocol allows
  (SFTP `copy-data` extension when advertised, otherwise stream through the
  device with a warning about the round trip).
- Cut/copy/paste buffer that survives navigation and host switching.
- Move across hosts falls back to copy-then-delete, transactional per file.

### B-12 — Host-to-host transfer ⬜ P2
Streams through the device (`plan.md` E15-S3, design-blocked).
- Two-pane or queue-based host→host copy with one progress row.
- Detect and use server-side copy when both endpoints are the same host.

### B-13 — Multi-select bulk actions ⬜ P1
Selection model exists; the action bar does not (`plan.md` E15-S1).
- Select-all, range select, invert, select-by-pattern.
- Bulk get/put/delete/chmod/move with counts and byte totals.

### B-14 — Remote search / find ⬜ P1 **new**
`filter…` filters the loaded page of the current directory only.
- Recursive server-side search where available (SFTP over an exec channel,
  guarded by a setting since it runs a remote command), client-side walk
  otherwise.
- Filter by name glob/regex, size range, mtime range, and type.
- Streaming results with a cancel, not a blocking wait.

### B-15 — Ownership, links, and the rest of the POSIX surface ⬜ P2 **new**
`chmod` is the only metadata write. Both SFTP and NFSv3 expose more.
- `chown`/`chgrp` (SFTP `setstat` uid/gid; NFS `setattr`).
- Create symlinks and hardlinks; read and repoint existing symlinks.
- `touch` / set mtime explicitly; `truncate`.
- Recursive chmod/chown with a separate directory mask, as `chmod -R` needs.
- Extended attributes and ACLs where the server advertises them (P3).

### B-16 — Paged and virtualized listings ⬜ P1
`VfsBackend::list` takes a `page` argument that every backend ignores
(`_page`), so a 100k-entry directory is one blocking round trip into one
`Vec`. Completes `plan.md` E13-S1.
- Backends stream/page `readdir`; the first page paints before the rest lands.
- Sorting and filtering operate over the full set off the render thread.
- Cancel the outstanding listing when the user navigates away.

### B-17 — Listing cache and manual refresh ⬜ P1 **new**
Every navigation re-lists over the wire; there is no way to force a refresh
either.
- Per-directory cache with a TTL, invalidated by any local mutation.
- Explicit pull-to-refresh / `REFRESH` action.
- Back navigation restores the cached listing plus scroll and selection
  (`plan.md` E5-S5 assumes this).

### B-18 — Trash / undo for destructive actions ⬜ P2 **new**
`RM` is immediate and irreversible.
- Optional per-host trash directory that delete moves into.
- Undo window for rename/move/delete within the session.
- Deletes are recorded in the audit log (B-53) regardless.

### B-19 — Directory sizes and disk usage ⬜ P2 **new**
The SIZE column shows item counts for directories (per the mockups) but there
is no way to learn what a tree actually weighs.
- On-demand recursive size calculation with progress and cancel.
- A `du`-style breakdown view of the largest children.

---

## 3. Connection and protocol depth

### SFTP

### B-20 — SSH agent, keyboard-interactive, and MFA ⬜ P1
`AuthMethod::Agent` and `AuthMethod::KbdInt` are modelled in `mk-core` but both
collapse to key-file or password auth at the backend
(`app/src/main.rs:263-275`).
- Real agent protocol support (`SSH_AUTH_SOCK` on desktop; an in-app agent on
  mobile).
- Genuine keyboard-interactive: render server prompts, including TOTP/2FA and
  password-expiry challenges.
- Auth-method fallback chain in the order the server advertises, with each
  attempt visible in the probe log.

### B-21 — Jump hosts / bastion / ProxyJump ⬜ P1 **new**
Very common in the environments SFTP is used in, and entirely absent.
- `ProxyJump`-equivalent, chained through one or more bastions.
- `ProxyCommand` on desktop.
- SOCKS/HTTP proxy support for reaching hosts through a corporate proxy.

### B-22 — Real `~/.ssh/config` import ⬜ P1
`ssh_import` pre-fills from a hardcoded fixture (`crates/mk-ui/src/dialogs.rs:430`).
Completes `plan.md` E8-S2.
- Parse real `Host` blocks including `Include`, wildcards, `HostName`, `Port`,
  `User`, `IdentityFile`, `ProxyJump`, `Compression`.
- Multi-select import; show what maps to which mkestral field and what was
  dropped.
- Import `~/.ssh/known_hosts` into the known-hosts store (pairs with B-1).

### B-23 — Algorithm, cipher, and compatibility controls ⬜ P2 **new**
- Per-host cipher/KEX/MAC/host-key-algorithm preference lists for old or
  hardened servers.
- Compression toggle actually reaches the transport (the `compression` host
  option is currently stored and unused).
- Surface the negotiated cipher/KEX in the inspector footer and the probe log.
- Server-quirk handling: servers that lack `statvfs@openssh.com` (already
  detected), `posix-rename`, or `hardlink@openssh.com`.

### B-24 — Concurrent SFTP streams and window tuning ⬜ P2 **new**
Throughput on high-latency links is dominated by SFTP window/request pipelining.
- Multiple in-flight read/write requests per file, tunable.
- Optionally split one large file across parallel channels.
- Expose the read/write chunk size per host, not just globally.

### B-25 — SSH exec side-channel ⬜ P3 **new**
russh already gives us a session; a guarded exec channel unlocks several items
above (B-3 remote digests, B-14 remote find, B-19 `du`).
- Off by default, per-host opt-in, with the exact command shown before it runs.
- Never interpolate unescaped user input into a remote command line.

### NFS

### B-26 — Export discovery ⬜ P1 **new**
A host is pinned to one export at creation (`for_export(&host.address,
&host.initial_path)`, `app/src/main.rs:287`), so browsing above the export root
is impossible and the user must know the export path in advance.
- `showmount -e` / MOUNT `EXPORT` enumeration in the host dialog, listing
  available exports and their allowed clients.
- Multiple exports per host, switchable without editing the host.
- Clear error when the export is not permitted to this client.

### B-27 — NFS identity mapping ⬜ P1 **new**
NFSv3 AUTH_UNIX sends whatever uid/gid the client picks; mkestral never asks
and never shows what it sent. Files land owned by a surprise uid.
- Configurable uid/gid/auxiliary GIDs per host, defaulted and explained.
- Show the effective identity in the inspector footer next to `rsize/wsize`.
- Detect and explain root-squash and all-squash behaviour on permission errors.

### B-28 — NFSv4 ⬜ P2
Deferred at the spike (`docs/spikes.md`), and the host model already has an
`Nfs4` protocol that silently runs the v3 client.
- NFSv4.1 client: compound ops, sessions, delegation-free operation.
- v4 ACLs surfaced in the inspector.
- Until then, an `nfs4` host must say it is connecting over v3, not pretend.

### B-29 — Mount options and tuning ⬜ P2 **new**
`rsize`/`wsize` are hardcoded to 32 KiB in the URL, while the design footer
advertises `rsize 1M wsize 1M`.
- Per-host `rsize`/`wsize`, `hard`/`soft`, `intr`, timeout, and retrans limits.
- Real retrans/timeout counters feeding the inspector and queue footers (both
  currently render zeros).
- `stale` detection driving the remount affordance (`plan.md` E11-S2).

### B-30 — Kerberos / `sec=krb5` ⬜ P3 **new**
The only credible authentication story for NFS in an enterprise.
- `sec=krb5`, `krb5i`, `krb5p` with a keytab or a ticket obtained in-app.

### SMB

### B-31 — SMB3 backend ⬜ P1
Spiked and decided (`smb2` crate, `docs/spikes.md`) but unimplemented; SMB
hosts hit `UnsupportedBackend`. `plan.md` E4-S4.
- List/stat/read/write/rename/delete/mkdir against Samba and Windows.
- NTLMv2 and guest; Kerberos where available.
- Share enumeration, DFS referral handling, free space to the FREE column.

### B-32 — Additional protocols ⬜ P3 **new**
Once three backends exist, the trait cost of a fourth is low, and these are
what users ask for next.
- WebDAV / WebDAV over HTTPS.
- FTP / FTPS (explicit TLS) — still ubiquitous on appliances and hosting.
- S3-compatible object storage.
- rsync-over-SSH as a transfer strategy, not a browse backend.

---

## 4. Discovery and connectivity (P2)

### B-33 — Network discovery ⬜ P2 **new**
Adding a host means typing an address by hand.
- mDNS/Bonjour for `_sftp-ssh._tcp`, `_nfs._tcp`, `_smb._tcp`.
- NetBIOS/WS-Discovery for SMB.
- Subnet scan on the local network, gated behind an explicit user action.
- Discovered hosts become one-tap host creations.

### B-34 — IPv6, link-local, and multi-homing ⬜ P2 **new**
- Bracketed IPv6 literals and link-local addresses with a zone index.
- Happy-eyeballs across resolved addresses rather than first-A-record-wins.
- Honour the platform's VPN/interface selection; the status strip already
  claims to show a VPN flag.

### B-35 — Reconnection and network-change resilience 🟡 P1
The pool reaps idle sessions and the store re-probes on connectivity change,
but an in-flight transfer across a Wi-Fi→cellular handoff is untested and has
no dedicated path. Completes `plan.md` E4-S6.
- Transparent reconnect with transfer resume across interface changes.
- Exponential reconnect backoff, distinct from job retry backoff.
- Explicit `reconnecting…` host state rather than a jump to `unreachable`.

### B-36 — Wake-on-LAN ⬜ P3 **new**
A NAS-focused client that can't wake the NAS is half a feature. Store a MAC per
host and send the magic packet from the unreachable state.

---

## 5. Transfer engine depth (P2)

### B-37 — Queue management ⬜ P1 **new**
The queue runs; it cannot be curated.
- Reorder, prioritise, move-to-top.
- Per-host concurrency limits in addition to the global parallel limit.
- Schedule a transfer for later; a quiet-hours window.
- Per-host bandwidth caps, not just the single global cap.

### B-38 — Background execution ⬜ P1
The queue stops when the app is backgrounded; the design footer promises 10
minutes. `plan.md` E7-S6.
- Android foreground service with a progress notification and pause/cancel
  actions.
- iOS background task assertion, with the footer copy corrected to whatever the
  platform actually grants.
- `transfer on cellular = off` enforced by the engine, not just stored.
- Completion notifications, including failure notifications.

### B-39 — Sparse and large-file handling ⬜ P2 **new**
- Files >4 GiB verified end to end on every backend (NFSv3 offsets are 64-bit;
  confirm the crate path).
- Sparse-file awareness so a 1 TB sparse image does not transfer as 1 TB.
- Zero-length and >255-byte-name edge cases covered by tests.

### B-40 — Filename encoding and cross-platform sanitisation ⬜ P1 **new**
Everything currently assumes UTF-8 and POSIX-legal names.
- Non-UTF-8 remote names (SFTP names are byte strings) render losslessly and
  round-trip on rename.
- Names illegal on the local platform (`:`/`?`/`*`, trailing dots, reserved
  Windows names) are sanitised on download, with the mapping shown.
- Unicode normalisation differences between macOS (NFD) and Linux (NFC) do not
  produce phantom duplicates.

### B-41 — Transfer integrity reporting ⬜ P2 **new**
- Per-job record: bytes, duration, average/peak rate, retries, verification
  method and result.
- Exportable transfer history beyond the `COMPLETED · TODAY` group.
- A failed-job report that can be copied for a bug report, with paths and
  errnos, secrets redacted.

---

## 6. Content: preview, streaming, offline (P2)

### B-42 — In-app file preview ⬜ P1
`mk-media` already serves seekable ranged HTTP on loopback — the hard part is
done, and nothing in the app consumes it but the VLC handoff. `plan.md` E15-S4.
- Image viewer over ranged reads (no whole-file download).
- Text/code viewer with paging for large files and encoding detection.
- Video/audio playback in-app via the existing media server.
- PDF and archive listing (P3).

### B-43 — Mobile media handoff ⬜ P1
`open_vlc` logs `not implemented` on iOS/Android (`app/src/main.rs:339`), which
is where this feature matters most.
- Android `ACTION_VIEW` intent to the user's player of choice.
- iOS `vlc-x-callback://` / document interaction.
- Keep the app alive while serving; explain when the platform will not allow it.

### B-44 — Offline pinning and cache ⬜ P1
`PINNED OFFLINE · 3` renders from fixtures; `CLEAR` cache zeroes a number. No
file is actually pinned or cached. `plan.md` E11-S2, E9-S2.
- Pin a file or directory for offline use; re-sync on reconnect.
- Real LRU cache with the size budget from settings, honoured by the engine.
- Pinned content is browsable and openable while every host is unreachable.
- `CLEAR` reports actually-freed bytes and never touches pinned files.

### B-45 — Remote text editing ⬜ P2 **new**
The classic "why I opened an SFTP client" task: fix one line in a config file.
- Edit-in-place: download to a temp file, edit, upload on save, with a
  conflict check against the remote mtime.
- Watch a locally-opened file and re-upload on external editor save (desktop).

---

## 7. Sync and automation (P2–P3)

### B-46 — Directory compare and one-way sync ⬜ P2 **new**
- Compare local and remote trees by name/size/mtime (and hash on demand).
- A reviewable diff before anything transfers.
- Mirror and update modes, with a delete-extraneous option behind an explicit
  confirmation.

### B-47 — Two-way sync with conflict resolution ⬜ P3 **new**
- Change journal per synced pair; detect both-sides-changed and offer both
  copies rather than picking.
- Explicit sync profiles, run on demand or on a schedule.

### B-48 — Scheduled and triggered transfers ⬜ P3 **new**
- Run a saved transfer or sync on a schedule, or on connecting to a named
  network.
- Camera-roll / Downloads auto-upload to a chosen host path.

---

## 8. Security and trust (P1)

### B-49 — Biometric gating ⬜ P1
`unlock with biometrics` is a toggle with no enforcement. `plan.md` E12-S2.
- Face ID / Touch ID / Android biometric before key use or password reveal.
- Passcode fallback; lockout after repeated failure.
- Re-lock after a configurable background interval.

### B-50 — Key lifecycle ⬜ P1
The `2d` keys table is fixture-only. `plan.md` E9-S3.
- Generate ed25519 (and ECDSA P-256) keypairs into secure storage.
- Import OpenSSH and PEM keys, prompting for the passphrase; never display the
  private key.
- Export/copy the public key, and offer to install it into a host's
  `authorized_keys` over an existing password session.
- `REVOKE UNUSED` with a preview of exactly what will be deleted.

### B-51 — Config export and wipe ⬜ P1
`plan.md` E12-S3.
- Export hosts/settings/known-hosts with a header asserting no secret material,
  and verify that assertion in a test.
- Optional encrypted backup that *does* include secrets, passphrase-protected.
- `wipe all credentials`, double-confirmed and complete.

### B-52 — Read-only and safety rails ⬜ P2 **new**
The `read_only` host option exists in the model and is never enforced.
- Read-only mounts reject every mutating call at the backend, not just by
  disabling buttons.
- Optional per-host "confirm destructive actions" and a protected-path list
  (e.g. never delete under `/etc`).

### B-53 — Audit log ⬜ P3 **new**
- Local, append-only record of connections, host-key decisions, and destructive
  operations, with export.

---

## 9. Observability and diagnostics (P2)

### B-54 — Connection diagnostics ⬜ P2 **new**
The `2c` probe log is a strong start; it ends at connect.
- Persistent per-host session log: negotiated algorithms, latency, retrans,
  reconnects, server banner.
- A latency/throughput test action per host.
- A copyable diagnostic bundle for support, with paths and secrets redacted.

### B-55 — Error taxonomy ⬜ P2
`VfsError` carries a kind and path; most backend errors collapse to
`VfsErrorKind::Io` with a stringified crate error, so `EACCES · /path` cannot
always be reconstructed as the design specifies. Strengthens `plan.md` E4-S1.
- Map protocol status codes to real errnos across all backends.
- Every user-visible error names the operation, the resolved path, and one
  concrete next step.

### B-56 — Crash and performance telemetry ⬜ P2
`plan.md` E14-S5, E13-S2. Opt-in, secrets redacted, with the measured budgets
recorded in `docs/perf.md`.

---

## 10. Platform integration (P2)

### B-57 — OS file-provider integration ⬜ P2 **new**
The difference between an app and a mount.
- Android Storage Access Framework `DocumentsProvider`, so every other app can
  open files from mkestral hosts.
- iOS File Provider extension for the Files app.
- Share-sheet target for uploading into a chosen host path.

### B-58 — Deep links and shortcuts ⬜ P3
`mkestral://queue` is planned (`plan.md` E2-S1); extend to
`sftp://user@host/path` handling, app shortcuts to a pinned host, and
Siri/Android quick actions for a saved sync.

### B-59 — Desktop parity ⬜ P3 **new**
The desktop build is a dev harness today but is a plausible product.
- Drag-and-drop to/from the OS file manager.
- Keyboard-first navigation and shortcuts.
- Multiple windows / tabs per host.

---

## 11. Accessibility, internationalisation, robustness (P2)

### B-60 — Accessibility ⬜ P2 **new**
The design is 9.5–10.5px mono throughout — the accessibility risk is real and
already flagged in `plan.md` E10-S3.
- Screen-reader labels on every control; the file table is navigable as a table.
- Dynamic Type / font-scale support with a documented cap on mono data columns.
- Contrast audit of the `text-25`/`text-28`/`text-30` tokens against WCAG AA.
- Larger touch targets available as a setting.

### B-61 — Internationalisation ⬜ P3 **new**
Strings are inline literals throughout `mk-ui`.
- Externalised strings; locale-aware dates, sizes, and number formatting.
- RTL layout support.

### B-62 — Backend conformance test suite ⬜ P1 **new**
`mk-vfs` has per-backend tests pointed at a real host (`freya.local`), which
will not run in CI. Strengthens `plan.md` E14-S1/E14-S4.
- One shared conformance suite every backend must pass, covering the edge cases
  above: unicode names, >4 GiB, sparse, symlinks, permission errors, mid-transfer
  disconnect, concurrent readers.
- Containerised `openssh-server`, a Samba container, and an NFS-server container
  as CI services.
- A fault-injection layer (latency, truncation, disconnect) over the trait.

---

## 12. Explicitly out of scope

Recorded so they are not re-litigated:

- Acting as an SFTP/NFS **server**.
- Kernel-level mounting on mobile (impossible; the userspace client is the
  answer — `docs/spikes.md`).
- Terminal / SSH shell emulation. Adjacent, frequently requested, and a
  different product; B-25 gives the narrow slice that file management needs.
- Cloud-storage backends (Drive, Dropbox, OneDrive) beyond the S3-compatible
  item in B-32.

---

## Suggested ordering

1. **Trust and data safety** — B-1, B-2, B-3, B-4, B-5, B-6. Nothing else
   matters if the app accepts any host key and cannot resume or verify.
2. **File-manager completeness** — B-9, B-10, B-11, B-13, B-16, B-17.
   Recursive transfer is the largest single gap between this and a usable
   client.
3. **Protocol credibility** — B-20, B-22, B-26, B-27, B-31, B-62.
4. **Platform behaviour** — B-38, B-43, B-44, B-49, B-50.
5. **Depth** — everything else, in priority order.
