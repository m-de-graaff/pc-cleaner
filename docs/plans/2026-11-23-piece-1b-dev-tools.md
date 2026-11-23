# Piece 1b — Dev Tools: Implementation Plan

> **To execute:** use `executing-plans`. Spec: the "Piece 1b" section of [`docs/design/2026-11-23-pc-cleaner.md`](../design/2026-11-23-pc-cleaner.md).
> This plan names exact interfaces and test cases but does not repeat the code. It was written by the same session that executes it, straight away.

**Goal:** a Dev tools page that finds reclaimable Docker data, WSL disks that can be compacted, unused Rust toolchains and Android emulators, and cleans them through the existing guarded executor.

## Global constraints

- Everything in the piece 1 plan's Global Constraints still applies.
- External commands run only through `SystemOps::run(program, args) -> Result<String, String>`, with argument lists the backend builds. Tests never run real commands.
- Rust builds run inside `vcvars64.bat` from VS 2022 Build Tools until VS 18 has its C++ workload.

## Tasks

### 1. Tool runner, shared test fake, model

- `SystemOps::run(&self, program: &str, args: &[&str]) -> Result<String, String>`. `WindowsSys` runs it with `CREATE_NO_WINDOW`; it returns stdout on success, and stderr (or the exit code) as the error.
- `sys::fake::FakeSys` (`#[cfg(test)]`) replaces both existing test fakes. It holds:
  - the Recycle Bin size and count
  - canned responses keyed by `"program arg1 arg2"`
  - a log of calls
  - an `emptied` flag

  An unknown command returns `Err("not installed")`.
- `Category::DevTools` serialises as `"devtools"`.
- `DockerTarget { BuildCache, Images, Containers }`.
- New `Action` variants:
  - `DockerPrune { target }`
  - `DockerRemoveVolume { name }`
  - `CompactDisk { path }`
  - `RemoveToolchain { name }`
  - `DeletePaths { paths }`
- `Finding.upper_bound: bool` (camelCase `upperBound`). When true, the size is a maximum and is left out of totals.
- Tests: the new actions serialise with camelCase tags.

### 2. Dev tools scanner (`scanners/devtools.rs`)

- `parse_docker_size(&str) -> Option<u64>` uses decimal units (B, kB, MB, GB, TB) and ignores a trailing `" (NN%)"`.
  - Tests: `"0B"`, `"1.5kB"`, `"512MB"`, `"2.3GB (45%)"`, and garbage → `None`.
- `docker_findings(&dyn SystemOps) -> (Vec<Finding>, Vec<String>)`:
  - Uses `docker system df --format {{json .}}` for the reclaimable sizes.
  - Uses `docker volume ls -q --filter dangling=true` plus `docker system df -v --format {{json .}}` for per-volume sizes. An unknown size is 0, and the reason says the size is unknown.
  - Zero-size findings are skipped.
  - If `docker` fails, it returns no findings and one notice.
  - Tests: findings from fixture output; the not-running notice; dangling volumes come out as caution findings.
- `wsl_disk_findings(&KnownPaths) -> Vec<Finding>`, with names from folders:
  - `CanonicalGroupLimited.Ubuntu_xxx` → "Ubuntu"
  - `Docker\wsl\disk` → "Docker Desktop data"
  - `Docker\wsl\main` → "Docker Desktop system"
  - `wsl\<name>` → `<name>`

  Every finding has `upper_bound`, `requires_admin` and review risk. Test: discovery and names from a fake tree.
- `toolchain_findings(&KnownPaths, &dyn SystemOps)` parses `rustup toolchain list`, skips entries marked `(default)` or `(active)`, and sizes each from `RUSTUP_HOME\toolchains\<name>`. Tests: parsing, skipping, and no rustup → nothing.
- `avd_findings(&KnownPaths)` reads `ANDROID_AVD_HOME` or `~\.android\avd`. It creates one caution finding per `<n>.avd` directory, with `DeletePaths [dir, <n>.ini]`. Test: discovery.
- JetBrains: `profiles(local\JetBrains, "caches")` goes into `app_locations` in `junk.rs`. Test: found as safe.
- `scan_devtools(&KnownPaths, &dyn SystemOps) -> DevScan { findings, notices }`, sorted by score.

### 3. Executor

- `run()` executes `CompactDisk` findings last.
- `DockerPrune` and `DockerRemoveVolume` run the docker commands from the spec. Freed bytes are read from `Total reclaimed space:` in the output, or else taken from the finding's size.
- `CompactDisk` runs `wsl --shutdown`, then `diskpart /s <script>` with `select vdisk file="…"`, `attach vdisk readonly`, `compact vdisk` and `detach vdisk`. Freed bytes are the file size before minus after.
- `RemoveToolchain` runs `rustup toolchain uninstall <name>`.
- `DeletePaths` checks each path with the guard, then removes it (directories recursively).
- In a dry run, none of these run anything.
- Tests: the exact commands run; dry run runs nothing; compaction is ordered last; compaction is skipped when not elevated; `DeletePaths` respects the guard.

### 4. Command and UI

- The `scan_devtools` command stores findings under `Category::DevTools`, replacing only devtools entries, and returns `DevScan`.
- Frontend:
  - types and `api.scanDevtools`
  - `useJunkScan` is generalised to `useScan(fetch)`, with `notices`
  - `JunkPage` becomes `ReviewPage` with `title` and `intro`
  - a "Dev tools" sidebar entry
  - `FindingRow` shows "up to" for `upperBound`
- `selection.ts`: `selectedTotal` skips `upperBound`; `confirmNotes(findings, selected)` warns about WSL shutdown and anything permanent. Vitest covers both.

### 5. Verify

Run the full Rust and Vitest suites and `tsc`. Then run a read-only, ignored test (`devtools_this_machine`) that prints the real findings without cleaning anything. Commit after each task.
