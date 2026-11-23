# PC Cleaner — Design

**Status:** approved 2026-11-23
**Goal:** a Windows desktop app that brings a PC back towards a "factory default" state. It scans the PC, recommends what to remove with a plain-English reason for each item, and removes only what the user approves, in a way that can be reversed wherever possible.

## Decisions

| Topic | Decision |
|---|---|
| Stack | Tauri 2 (Rust backend) + React 19 / TypeScript / Vite, Tailwind CSS v4, lucide-react icons |
| Platform | Windows 10/11 x64 only |
| Recommendations | Offline rules and scoring. No network calls, and no data leaves the PC. |
| Old dev projects | Default suggestion: strip build artifacts only and keep the source |
| Scope | Junk/temp files, old dev projects, large and duplicate files, apps and startup items, deep uninstall, registry recommendations |

## Architecture

### Backend (Rust, `src-tauri/src`)

- **Scanners**: one module per area. Each returns `Vec<Finding>`. A finding records:
  - what the item is and its size
  - when it was last used
  - its risk level: `safe`, `review` or `caution`
  - a score from 0 to 100
  - a plain-English reason
  - whether it needs admin rights, and whether removing it can be reversed
  - the exact action that would remove it

  Scanners never delete anything.
- **Scoring**: a base value per risk level, plus a size bonus. Later pieces add age and regenerability factors. The scanner writes the reason text, because only the scanner knows *why* the item is junk.
- **Executor**: the only code that deletes anything.
  - It only acts on findings from the most recent scan held in backend state. The frontend sends finding IDs, never paths.
  - Every target path goes through the `ProtectedSet` guard, a hard deny-list:
    - Folders that may never be deleted themselves, nor any folder that contains them: drive root, `C:\Windows`, Program Files, ProgramData, `C:\Users`, the user profile, AppData.
    - Folders that may not be touched at all, anywhere inside: System32, SysWOW64, WinSxS, servicing, Boot, Fonts, and the user's Documents, Desktop, Pictures, Videos, Music and OneDrive.
  - It continues past individual failures, such as files that are in use, and reports them.
  - It writes one journal entry per finding.
- **Journal**: a JSON Lines file in the app's local data folder. It backs the History screen.
- **System access** (`sys.rs`): the Recycle Bin size and empty, drive usage, the elevation check, and relaunching as admin. It sits behind the `SystemOps` trait so the executor can be tested without touching the real system.
- **Elevation**: the app runs as a normal user. Findings that need admin rights are flagged and disabled. A "Restart as admin" banner relaunches the app through a UAC prompt.

### Frontend (React, `src`)

- Sidebar navigation. Piece 1 has three screens: Overview, Junk & temp files, History.
- **Review list**: findings grouped by risk, each with a checkbox. Only `safe` findings that the current user is allowed to run are pre-checked. Each row shows its size, reason and badges. A footer shows the total selected, a dry-run toggle, and the Clean button. A confirmation dialog appears before anything is deleted.
- A result summary after each clean lists anything that was skipped or failed.

## Safety model

1. Nothing is deleted without a review screen and a confirmation.
2. The executor only acts on findings the backend produced itself. Every path passes the `ProtectedSet` guard: once for the target root, and again for every single file.
3. **Regenerable junk (temp files, caches, dumps) is deleted permanently.** Moving it to the Recycle Bin would free no space. Only files older than a minimum age are touched, so programs that are running are not affected.
4. **User-owned files (from piece 2 onwards) go to the Recycle Bin**, and History gains an Undo action for them.
5. **Registry changes and uninstalls (pieces 3–4)** are preceded by a `.reg` export of every key involved and a System Restore point.
6. Dry-run mode reports exactly what *would* be removed without removing it.

## Pieces

Each piece ships a working app and gets its own implementation plan.

### Piece 1 — Foundation + junk & temp files

Scan → review → clean → history, plus the Overview dashboard. It needs no other piece.

Junk locations:

| Location | Path | Min age | Risk | Admin |
|---|---|---|---|---|
| Your temporary files | `%TEMP%` | 24 h | safe | no |
| Windows temporary files | `%SystemRoot%\Temp` | 24 h | safe | yes |
| Crash dumps | `%LOCALAPPDATA%\CrashDumps` | 0 | safe | no |
| Windows error reports | `%ProgramData%\Microsoft\Windows\WER\ReportArchive` and `ReportQueue` | 0 | safe | yes |
| Thumbnail cache | `%LOCALAPPDATA%\Microsoft\Windows\Explorer\thumbcache_*.db` | 0 | safe | no |
| Windows Update downloads | `%SystemRoot%\SoftwareDistribution\Download` | 72 h | review | yes |
| Delivery Optimization cache | `%SystemRoot%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache` | 0 | safe | yes |
| Chrome / Edge / Brave cache | `<User Data>\<profile>\Cache` and `\Code Cache` | 0 | safe | no |
| Firefox cache | `%LOCALAPPDATA%\Mozilla\Firefox\Profiles\<profile>\cache2` | 0 | safe | no |
| Recycle Bin | all drives | — | review | no |

### Piece 2 — Old dev projects + large/duplicate files (needs piece 1)

- **Projects**: a folder counts as a project root if it contains `.git`, `package.json`, `*.csproj`/`*.sln`, `Cargo.toml`, `pyproject.toml`/`requirements.txt`, `pom.xml`/`build.gradle` or `go.mod`. A project is *stale* when its newest source file (artifacts excluded) is more than 6 months old. The default action strips artifacts: `node_modules`, `bin`/`obj`, `target`, `.venv`/`venv`, `dist`/`build`, `.next`, `__pycache__`, `.gradle`. The source is kept.
- **Large files**: files over 500 MB, plus `.exe`/`.msi`/`.iso`/`.zip` installers in Downloads older than 30 days.
- **Duplicates**: files are grouped by size first, then confirmed by a BLAKE3 hash. One copy is kept: the newest, or the one outside Downloads.
- Introduces Recycle Bin deletion and Undo in History.

### Piece 3 — Apps, startup items, deep uninstall (needs piece 1)

- **Installed apps**: read from the Uninstall registry keys (HKLM, HKLM WOW6432Node, HKCU). The list shows size and last used; "last used" comes from Prefetch or the newest file in the install folder.
- **Deep uninstall**:
  1. Create a restore point.
  2. Run the app's own `UninstallString`.
  3. Scan for leftovers: install folder, `%APPDATA%`/`%LOCALAPPDATA%`/`%ProgramData%` folders matching the publisher or product name, `HKCU`/`HKLM\Software\<Publisher>\<Product>`, Start Menu and Desktop shortcuts, services and scheduled tasks pointing into the install folder.
  4. The user reviews the leftovers; they are backed up (`.reg` and Recycle Bin) and then removed.
- **Startup items**: Run/RunOnce keys, the Startup folder, and scheduled tasks triggered at logon. They are *disabled* via the `StartupApproved` key, the same mechanism Task Manager uses, never deleted.

### Piece 4 — Registry recommendations (needs piece 3's registry backup)

Only orphans that can be proven:

- Uninstall entries whose install location or uninstaller no longer exists.
- App Paths, Run keys and services whose executable is missing.
- File associations whose handler is missing.
- `Software\<Publisher>\<Product>` keys left behind by apps that are no longer installed.

Every deletion is preceded by a `.reg` backup and a restore point.

## Biggest risk

Deleting something the user still needed. Mitigations: the `ProtectedSet` guard in the executor, nothing pre-checked except `safe` findings, the Recycle Bin for user files, `.reg` backups, restore points, dry-run mode, and the journal.

## Verification

- Rust unit tests for every scanner, the guard, the executor and the journal, using fixture folders in temp directories and a fake `SystemOps`.
- Vitest for the frontend's selection and formatting logic.
- A manual pass in `npm run tauri dev` per piece: a dry run first, then a real clean, with results checked against the History screen.
