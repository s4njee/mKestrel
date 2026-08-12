# Backend spikes — written findings (E4-S5 NFS, E4-S4 SMB)

Date: 2026-08-12. Status: **decided** (both). These drive the v1 backend scope
in `mk-vfs`. Full candidate survey is in the appendix below.

## NFS (E4-S5) — decision: implement in v1, NFSv3 only

Neither iOS nor Android lets an app mount NFS, so a **userspace client** is
required. Pure-Rust clients exist and are viable.

**Decision:** v1 ships NFSv3 via a **vendored/forked `nfs-rs`
(https://crates.io/crates/nfs-rs)** — full ONC RPC + portmapper + MOUNT + all
22 NFSv3 procedures, async on Tokio, Apache-2.0. **NFSv4 is deferred.**

- Effort: ~2–3 engineer-weeks — cross-compile to `aarch64-apple-ios` and
  `aarch64-linux-android` (cargo-ndk), force `?noresvport=true` (mobile can't
  bind privileged source ports), reconnect/keepalive layer for mobile
  networks, integration harness against 2–3 real NAS exports.
- Fallback: `nfs3_client` (Unlicense, 11k downloads, NFSv3-only) if `nfs-rs`
  fails mobile interop. Low swap cost — same protocol surface.
- Do **not** use `libnfs` bindings (`libnfs-sys`): the Rust wrapper is stale
  (Sep 2022, wraps ~v4-era headers) and building libnfs 7 for iOS/Android is
  hand-rolled autotools/CMake per target — a permanent cross-compile tax.
- NFSv4/v4.1, rpcsec_gss/Kerberos, NFS-over-TLS: post-v1.
- If schedule forces deferral of all NFS: `nfs4`/`nfs3` hosts still render in
  the rail (proto/free/status) but offer no mount — the documented exit path.

## SMB (E4-S4) — decision: pure-Rust `smb2`, no C bindings

**Decision:** use **`smb2` (https://crates.io/crates/smb2, v0.18+)**, pure
Rust, SMB2.x–3.1.1, NTLMv2/Kerberos/SPNEGO. Fallback: `smb`
(https://crates.io/crates/smb).

- `smb2` covers every needed flow: tree connect, create, read/write (streaming
  readers/writers), query directory, stat, rename/delete/mkdir. Pure-Rust
  dependency tree cross-compiles cleanly to iOS/Android.
- **Do not** use `pavao`/`libsmbclient`: Samba's waf build has no Android
  target and no maintained iOS path; Google archived its `samba-documents-
  provider` (the only reference libsmbclient-on-Android port) in 2022.
- Effort: 1–2 day spike to verify guest (empty-credential NTLM) + list/stat/
  read/write against Samba + Windows, then ~1–2 weeks to integrate behind the
  `VfsBackend` trait with saved credentials from `mk-secrets`.
- Pin `smb2` — it releases fast (26 releases in ~4 months).
- Deferral path: render discovered SMB hosts ("discovery only") rather than a
  half-tested mount.

## Appendix — candidate survey

### NFS crates

| Crate | Pure Rust? | Protocols | Notes |
|---|---|---|---|
| `nfs-rs` 0.3.1 | yes | v3 + v4.1 | Full stack, Tokio, CI vs physical NFS lab. <1.0 API churn. |
| `nfs3_client` 0.9.0 | yes | v3 only | Unlicense, 11k dl, runtime-agnostic, lower-level API. |
| NFSWolf stack | yes | v2/v3, v4 partial | Brand new (Aug 2026), pentest-oriented, v4 read-only. |
| `nfs-crust` | yes | v4.1 partial | Too early. |
| `libnfs-sys` 0.2.3 | no (C) | v3+v4 | Stale bindings; needs hand-built libnfs per mobile target. |

From-scratch NFSv3 estimate: ~15–20 engineer-days (3–4+ weeks with interop
quirks). Not worth it given `nfs-rs`.

### SMB crates

| Crate | Pure Rust? | Dialects | Notes |
|---|---|---|---|
| `smb2` 0.18.1 | yes | 2.x–3.1.1 | Full NTLMv2/Kerberos; async; active. Guest undocumented — spike it. |
| `smb` | yes | 2.x–3.x | Heavier build (sspi + aws-lc-rs); younger. |
| `pavao` 0.2.16 | no (libsmbclient) | SMB2/3 | Mature API but vendored Samba build is cross-compile-hostile on mobile. |
| `smb2-client` | yes | SMB2 | Read-only, no query-dir. Insufficient. |
