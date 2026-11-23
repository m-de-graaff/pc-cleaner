# PC Cleaner

A Windows desktop app that finds what's using your disk, explains each item, and removes only what you approve. Built with Tauri 2 (Rust) and React.

## What it cleans

- **Junk & temp files**: your temp folder and Windows Temp, crash dumps, error reports, the thumbnail cache, Windows Update downloads, browser caches (Chrome, Edge, Brave, Firefox), app caches (VS Code, Discord, Slack, Teams, Spotify, JetBrains), shader caches and the Recycle Bin.
- **Developer caches**: npm, the pnpm store, Yarn, Bun, Deno, Cargo, rustup, pip, uv, NuGet, Gradle, Maven, Go, Playwright and Puppeteer browsers, Electron downloads.
- **Dev tools**:
  - Docker build cache, unused images, stopped containers and unused volumes
  - cleanup inside WSL distros: the apt cache, `~/.cache` and old system logs
  - compacting WSL and Docker virtual disks so Windows gets the space back
  - old Rust toolchains and Android emulators

## How it keeps you safe

- Nothing is removed without a review list and a confirmation. Only items marked "Safe" are pre-checked.
- Every path is checked against a hard deny-list before anything is deleted: Windows, Program Files, and your Documents, Desktop, Pictures and other personal folders.
- Dry run shows what would be removed without touching anything.
- External tools run only through a fixed set of commands that the app builds itself.
- History records every action.
- It never removes WSL distros or installed apps.

## Run it

Requirements: Windows 10/11 x64, Node 22+, pnpm, Rust (stable), and the Visual Studio C++ build tools with the Windows 11 SDK.

```powershell
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # installer in src-tauri/target/release/bundle/nsis
```

Some system locations, and disk compaction, need administrator rights. Use the **Restart as admin** button in the installed app.

## Tests

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test
```

Design and plans are in [`docs/`](docs/).
