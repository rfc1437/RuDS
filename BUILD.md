# RuDS — Build Prerequisites

## Native desktop packages

Install the pinned Cargo Packager CLI once:

```sh
cargo install cargo-packager --locked --version 0.11.8
```

Build packages on their native operating system from the repository root:

```sh
cargo bundle-macos    # Blogging Desktop Server.app and .dmg
cargo bundle-windows  # NSIS .exe installer
cargo bundle-linux    # .deb and .AppImage
```

Packages are written below `target/release`. The macOS bundle uses the ICNS icon, Windows embeds the ICO in the application executable and installer, and Linux packages install the 1024×1024 PNG with their desktop entry.

Windows packaging requires the MSVC Rust toolchain, Visual Studio Build Tools with C++ support, and the Windows SDK. Build each package on its target operating system; these commands do not cross-package installers.

## macOS system requirements

- macOS 13 (Ventura) or later
- Apple Silicon or Intel Mac with Metal or Vulkan support (required by Iced's wgpu backend)

## Linux system requirements (optional, for CI or cross-platform development)

- A Vulkan-capable GPU and driver, or software rendering via `WGPU_BACKEND=gl`
- System packages for GTK and related libraries (for muda and rfd):

```sh
# Debian/Ubuntu
sudo apt install build-essential cmake pkg-config libgtk-3-dev libxdo-dev libdbus-1-dev libwebkit2gtk-4.1-dev
```

## Install Homebrew packages (macOS)

```sh
brew install rust cmake pkg-config
```

| Package | Why |
|---|---|
| `rust` | Rust toolchain (alternatively install via `rustup`, see below) |
| `cmake` | Required by some native dependencies during `cargo build` |
| `pkg-config` | Locates system libraries during `cargo build` |

## Install Xcode Command Line Tools (macOS)

```sh
xcode-select --install
```

Required for the macOS SDK, Metal framework headers, and the Apple linker. A full Xcode install also works but is not required.

## Recommended: install Rust via rustup

If you prefer managing Rust versions explicitly (recommended for pinning toolchain versions across the team):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then set the stable toolchain:

```sh
rustup default stable
```

## Additional dependencies by milestone

### M0–M4 (core through rendering)

No additional system packages beyond the above. Key crates use bundled/vendored native code:

- `diesel` provides the typed SQLite query layer
- `diesel_migrations` embeds migrations; `libsqlite3-sys` bundles SQLite
- `iced` uses wgpu which links to Metal (macOS) or Vulkan (Linux/Windows) at runtime
- `muda` uses native platform menu APIs (NSMenu on macOS, GTK on Linux, Win32 on Windows)
- `rfd` uses native platform dialog APIs (NSOpenPanel on macOS, GTK on Linux, Win32 on Windows)
- `cosmic-text` bundles its own font shaping; uses system font discovery via `fontdb`
- `syntect` bundles syntax definitions; no system dependency
- `ropey` is pure Rust; no system dependency
- `image` crate is pure Rust for most codecs; no system dependency for JPEG/PNG/WEBP
- `pulldown-cmark`, `liquid`, `quick-xml`, `rayon` are pure Rust; no system dependencies
- `axum` + `tokio` are pure Rust; no system dependencies
- `ssh2` links to libssh2 (bundled via `ssh2` crate's default features)
- `objc2` / `objc2-app-kit` (macOS only, cfg-gated) links to system AppKit frameworks already available via Xcode CLI tools

### M5–M6 (Lua scripting)

When Lua support is added via the `mlua` crate:

```sh
brew install lua@5.4
```

Alternatively, use the `vendored` feature flag on `mlua` to compile Lua 5.4 from source and skip the system install entirely. The choice should be made when Wave 6 starts.

### Publishing (SSH/rsync)

The publish engine uses SSH and rsync. Both ship with macOS by default. No Homebrew install needed unless you want a newer rsync:

```sh
brew install rsync   # optional, for a newer version than the macOS default
```

## Verify the setup

After installing prerequisites:

```sh
# check Rust toolchain
rustc --version
cargo --version

# check native tooling
cmake --version
pkg-config --version
xcode-select -p          # macOS only

# clone and build
cargo build
```

## Environment notes

- The project is a Cargo workspace. Always run `cargo` commands from the repository root.
- Iced requires a GPU context (Metal, Vulkan, or OpenGL fallback). CI runners must support one of these or use headless test targets that do not create windows.
- The `bds-core` and `bds-cli` crates do not depend on Iced, muda, or rfd and can be built and tested without a display server.
- The `bds-editor` crate depends on Iced (for the custom widget trait) but its buffer, highlighting, and layout logic can be unit tested without a display server.
