# Architecture

mkestral is a Rust + Dioxus remote file browser. Crate boundaries enforce the
plan's dependency rule: **`mk-ui` depends on `mk-core` only**; backends are
injected as trait objects, so the entire UI is built and tested against
fixtures with no network.

```
┌────────────────────────────────────────────────────────────┐
│ app/   platform bootstrap · window size · CSS · `--demo`    │
│   │                                                        │
│   ▼                                                        │
│ mk-ui   Dioxus components + screens (rsx + CSS classes)    │
│   │        depends on: mk-core only                        │
│   ▼                                                        │
│ mk-core  domain types · formatting · fixtures (no I/O)     │
│                                                           │
│ mk-vfs      VfsBackend trait + local/sftp/smb/nfs impls    │
│ mk-transfer transfer engine · queue · rate history          │
│ mk-secrets  keychain / keystore / biometrics                │
└────────────────────────────────────────────────────────────┘
```

## Design-system path (E1)

- Single stylesheet: `assets/main.css` — every token as a CSS custom property,
  every primitive as a class. **No raw hex/rgba appears in component code.**
- Rust mirror: `mk_ui::tokens` (colors, type-scale class names) for anything
  inline styles or logic need.
- `mk_ui::components` holds the 14 primitives (E1-S2); screens compose them.
- `mk_ui::gallery` is the dev-only style gallery — the E14-S3 visual-diff
  baseline.
- Motion rules (E1-S4): 120–160ms dialog/sheet transitions, linear progress
  widths, `prefers-reduced-motion` kill-switch in CSS.

## State model (E3)

`mk-core` types mirror the handoff's state spec exactly:
`Host`/`Entry`/`Job`/`Settings`/`Credentials` (keys, passwords, known hosts),
all serde-round-trippable (property-tested). Fixtures in
`mk_core::fixtures::demo_state()` reproduce the `2a`/`2b`/`2d` content lists
verbatim — names, sizes, modes, owners, mtimes, percentages, error strings.

## Backends (E4)

All backends implement the async `VfsBackend` trait (E4-S1, defined in
`mk-vfs`), which `mk-ui` consumes via injected trait objects. The `MockBackend`
over the fixtures is the default until a real backend ships. Decisions from the
2026-08-12 spikes (see `docs/spikes.md`):

| Backend | v1 choice | Why not the alternative |
|---|---|---|
| SFTP | `russh` (pure Rust) | cross-compiles to iOS/Android |
| SMB | `smb2` v0.18+ (pure Rust) | libsmbclient/Samba is cross-compile-hostile on mobile |
| NFS | NFSv3 via vendored `nfs-rs` | libnfs bindings stale; NFSv4 deferred |
| local | std::fs + platform document pickers | — |

## Token → handoff map

`docs/dev-setup.md` records the build; the handoff README is the pixel spec.
Column widths, row heights, radii, and every string are defined there and
encoded in fixtures/tests.
