# Dev setup

How to build and run mkestral on this machine.

## Host prerequisites (macOS, Apple Silicon)

| Tool | Version | Notes |
|---|---|---|
| rustup toolchain | **1.97.1** (pinned in `rust-toolchain.toml`) | iOS/Android targets pre-installed (see below) |
| Dioxus CLI | **0.7.10** (`cargo install dioxus-cli --version 0.7.10 --locked`) | ⚠️ PATH note below |
| Xcode | 26.5 (`xcodebuild -version`) | for `--platform ios` |
| Android SDK | `$ANDROID_HOME=~/Library/Android/sdk` with NDK + platforms + system-images | for `--platform android` |

Installed rust targets (from `rustup target list --installed`):
`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`,
`aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`,
`x86_64-linux-android`, plus `wasm32-unknown-unknown`.

### ⚠️ `dx` on PATH resolves to Deno's `dx`, not Dioxus

`/opt/homebrew/bin/dx` is the **Deno** "execute a script" runner
(`deno x`). The Dioxus CLI installs to `~/.cargo/bin/dx`. Until `~/.cargo/bin`
precedes `/opt/homebrew/bin` on your PATH, invoke it as:

```sh
~/.cargo/bin/dx serve --platform desktop --package mkestral
```

(Or `alias dx=~/.cargo/bin/dx`.)

## Desktop dev loop (95% of UI work)

The window is sized 1194×834 — the iPad-class tablet mockup size (plan.md §1).

```sh
# one-shot, no hot reload
cargo run -p mkestral -- --demo

# dev loop with hot reload
~/.cargo/bin/dx serve --platform desktop --package mkestral
```

`--demo` boots on the `mk-core` fixtures (9 hosts, 142-entry listing, queue,
credentials) with no network.

## Workspace layout

```
mkestral/
  Cargo.toml          # workspace
  crates/mk-core      # domain types + formatting + fixtures (no I/O)
  crates/mk-vfs       # VfsBackend trait (E4) + local/sftp/smb/nfs impls
  crates/mk-transfer  # transfer engine / queue (E7)
  crates/mk-secrets   # keychain/keystore + biometrics (E12)
  crates/mk-ui        # Dioxus components + screens (depends on mk-core only)
  app/                # platform bootstrap + Dioxus launch
  assets/main.css     # design-system CSS (single source of styling truth)
```

## Quality gates (definition of done, plan.md §3)

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## iOS simulator (E0-S2)

**Targets for the pinned toolchain must be installed per-toolchain.** rustup
targets belong to the toolchain named in `rust-toolchain.toml`, so:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim --toolchain 1.97.1
```

### Dev loop

```sh
~/.cargo/bin/dx serve --platform ios --package mkestral
```

Bundle identifier comes from `Dioxus.toml` (`[bundle].identifier`, overridden
by `[ios].identifier`) — set to `com.mkestral.app`. Without it dx defaults to
`com.example.Mkestral`.

`dx` boots the most-recently-booted simulator (`xcrun simctl list devices`),
builds the bundle, installs, and launches. Boot the target device first, e.g.:

```sh
xcrun simctl boot 08C54105-001A-43E8-A270-6901BBA52A9D   # iPad Pro 11-inch (M5)
open -a Simulator
```

### Known-good manual recipe

`scripts/bundle-ios-sim.sh [udid]` builds (`--no-default-features --features
mobile`, target `aarch64-apple-ios-sim`), assembles `Mkestral.app` with an
`Info.plist`, ad-hoc signs (simulator needs no provisioning), installs via
`xcrun simctl install`, and launches. Verified 2026-08-12.

Recorded for the E0-S2 AC:

| Item | Value |
|---|---|
| Runtime | iOS 26.5 (`com.apple.CoreSimulator.SimRuntime.iOS-26-5`) |
| Reference device | iPad Pro 11-inch (M5) — 834×1194 pt (the 1194×834 landscape class) |
| Rust target | `aarch64-apple-ios-sim` |
| Min OS (Info.plist) | 15.0 |
| Signing | ad-hoc (`codesign --force --sign -`) — simulator only |
| Bundle id | `com.mkestral.app` |

The gallery renders on-device in either orientation; rotation/reflow behavior
is exercised by E2-S3.

## Android emulator (E0-S2)

**Prerequisites (all present on this machine):**
- `ANDROID_HOME` = `~/Library/Android/sdk`, NDK `28.2.13676358`
  (`ANDROID_NDK_HOME` must be exported for dx). The NDK clang ships as a
  **universal arm64+x86_64 binary** even though the folder is named
  `darwin-x86_64` — it runs natively on Apple Silicon.
- **JDK 17–25 is required.** Gradle/AGP cannot compile build scripts on
  Java 26 ("Unsupported class file major version 70"). Installed via
  `brew install openjdk@21`; export its `JAVA_HOME`.
- `cargo-ndk` (4.1.2) — dx mimics its env setup internally; used standalone
  to validate the Rust side compiles.

### Dev loop

```sh
export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/28.2.13676358
export JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home
~/.cargo/bin/dx serve --platform android --package mkestral
```

dx boots the `cb8_tablet` AVD, builds with gradle (generated project under
`target/dx/…/android/`), installs via adb, and launches. The APK gets
`applicationId`/`namespace` `com.mkestral.app` from `Dioxus.toml`.

Recorded for the E0-S2 AC:

| Item | Value |
|---|---|
| AVD | `cb8_tablet` — 2560×1600 @ 320dpi (logical **1280×800** landscape), API 36 google_apis arm64-v8a |
| Rust target | `aarch64-linux-android` |
| NDK | 28.2.13676358 |
| JDK | 21 (Temurin via `brew install openjdk@21`) |
| Gradle / AGP | system gradle 9.6.1 (dx-invoked); compileSdk 34, targetSdk 34, minSdk 24 (dx defaults) |
| applicationId | `com.mkestral.app` |

## Known-good build matrix

| Target | Triple | Notes |
|---|---|---|
| macOS desktop | `aarch64-apple-darwin` | verified; 1194×834 window |
| iOS simulator | `aarch64-apple-ios-sim` | verified E0-S2; iPad Pro 11-inch (M5), iOS 26.5 |
| iOS device | `aarch64-apple-ios` | toolchain present; device pass later (needs signing) |
| Android emulator | `aarch64-linux-android` | verified E0-S2; cb8_tablet 1280×800, API 36, JDK 21 required |
