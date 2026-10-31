---

description: "Task list for local-machine export"
---

# Tasks: `local-machine export`

**Input**: Design documents from `specs/001-home-share/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/cli.md](contracts/cli.md),
[contracts/export-config.md](contracts/export-config.md), [quickstart.md](quickstart.md)

**Tests**: The plan's testing strategy (plan.md → Technical Context → Testing) calls for
`cargo test` unit tests, `assert_cmd` CLI tests, and step tests against a fake command
runner, so those are included. Real-system behaviour is validated manually with
quickstart.md scenarios V1–V15.

**Organization**: Tasks are grouped by user story (spec.md): US1 Windows editing (P1),
US2 Unix VM clients (P2), US3 status (P3), US4 stop (P4).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story the task belongs to
- Paths are relative to the repository root (`/home/kostadis/src/local_machine`)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the `local-machine` Rust crate

- [X] T001 Create `Cargo.toml`: package `local-machine`, edition 2024, one binary `local-machine` (`src/main.rs`); dependencies `clap` (features `derive`), `serde` (features `derive`), `serde_yaml_ng`, `ipnet`, `rpassword`, `anyhow`, `thiserror`; dev-dependencies `assert_cmd`, `predicates`, `tempfile`
- [X] T002 Create the module skeleton from plan.md → Source Code as empty, compiling modules: `src/main.rs`, `src/cli.rs`, `src/config.rs`, `src/system/{mod.rs,files.rs,steps.rs}`, `src/export/{mod.rs,config.rs,samba.rs,state.rs,setup.rs,start.rs,status.rs,stop.rs,connect.rs,symlinks.rs}`; `cargo build` must succeed
- [X] T003 [P] Add `/target` to the root `.gitignore` and add `rustfmt.toml` (edition 2024, defaults otherwise)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared CLI, config, command-running and step machinery used by every action

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 Implement the clap command tree in `src/cli.rs` per contracts/cli.md → Grammar: global `--config-dir <DIR>` (accepted before or after the subcommand; default `$HOME/.config/local-machine`), subcommand `export` with required action enum `setup|start|status|stop`; `export` with no action prints help and exits 2; unknown action/flag exits 2
- [X] T005 [P] Implement shared config loading in `src/config.rs`: `fn load<T: DeserializeOwned>(config_dir: &Path, subcommand: &str) -> Result<T, ConfigError>` reading `<config_dir>/<subcommand>.yaml` (constitution Principle II, one file per subcommand); `ConfigError` (thiserror) variants `Missing { path }`, `Invalid { path, key: Option<String>, reason }`, each Display matching the messages in contracts/export-config.md → Errors
- [X] T006 [P] Implement command running in `src/system/mod.rs`: trait `CommandRunner { fn run(&self, cmd: &Cmd) -> Result<Output> }` where `Cmd { program, args, stdin: Option<String>, root: bool }`; `RealRunner` prefixes `sudo` when `root` is true and never logs `stdin`; `FakeRunner` (public, `pub mod fake`) records every `Cmd` and returns scripted outputs keyed by program+args, for use by `tests/`; `Cmd` also has `root_noninteractive: bool` → `RealRunner` uses `sudo -n` (never prompts); `fn sudo_preflight(runner, steps: &[&str])` prints `needs administrator privileges for:` plus one indented line per step, then runs `sudo -v`
- [X] T007 [P] Implement atomic managed-file writes in `src/system/files.rs`: `read(path) -> Option<String>`; `replace(runner, path, content) -> Result<Backup>` writes content to a temp file (via `tempfile`), copies the current file (if any) to `<path>.local-machine.bak` with `sudo cp -p`, then `sudo install -m 0644 <tmp> <path>`; `Backup::restore(runner)` puts the previous file back (or removes the new file if there was none)
- [X] T008 Implement the step engine in `src/system/steps.rs`: trait `Step { fn name(&self) -> &str; fn needs_root(&self) -> bool; fn check(&self, ctx) -> Result<Check>; fn apply(&mut self, ctx) -> Result<()>; fn rollback(&mut self, ctx) -> Result<()> { Ok(()) } }` with `enum Check { Done, NeedsChange(String) }`; `fn run_steps(steps, ctx)`: calls `sudo_preflight` **first, before any check**, listing every step whose `needs_root()` is true (FR-009: say so before doing anything), then checks all steps, then applies in order printing `ok: <name>` / `changed: <name>`; on the first failure prints `error: <name>: <reason>`, rolls back applied steps in reverse order, returns error (exit 1) — depends on T006, T007
- [X] T009 Implement error-to-exit-code mapping in `src/main.rs` per contracts/cli.md → Exit codes: define `ExitError { code: u8, message }` with constructors `failed` (1), `usage` (2), `not_configured` (3), `not_set_up` (4), `not_available` (5); print errors to stderr prefixed `error:` — depends on T004
- [X] T010 [P] Implement `ExportConfig` in `src/export/config.rs` per data-model.md → ExportConfig: `#[serde(deny_unknown_fields)]`; `allowed_network` required (`ipnet::IpNet`); defaults `share_name = "home"`, `owner` = current user (`$USER`, verified with `getent passwd`), `path` = owner's home from `getent passwd`, `exclude = [".ssh", ".gnupg"]`; validation: share_name `[A-Za-z0-9_-]{1,80}`, path is an existing directory owned by owner, owner exists and is not `root`, each exclude entry non-empty, no `/`, not `.`/`..`; a `password` key yields the dedicated message from contracts/export-config.md; unit tests in the same file for: minimal valid file, full valid file, unknown key, `password` key, bad CIDR, missing `allowed_network`, exclude entry with `/`
- [X] T011 [P] Implement pure Samba rendering in `src/export/samba.rs` (no I/O): `render_globals(cfg) -> String` for `/etc/samba/local-machine-export.conf` containing the header `# Managed by local-machine export — do not edit`, then a `[global]` section header on its own line **before any setting** (research R8: the include is appended after `[print$]` in the packaged smb.conf, so without it every setting would land in that share), then `map to guest = never`, `restrict anonymous = 2`, `server min protocol = SMB3`, `disable netbios = yes`, `hosts allow = <allowed_network> 127.0.0.1`, `hosts deny = ALL`, `follow symlinks = yes`, `wide links = no`, `allow insecure wide links = no`, `load printers = no`, `printing = bsd`, `printcap name = /dev/null`, `disable spoolss = yes` (research R5), and finally `include = /etc/samba/local-machine-export-share.conf`; `render_share(cfg) -> String` for the share file containing the header and one `[<share_name>]` stanza with `path`, `valid users = <owner>`, `force user = <owner>`, `read only = no`, `browseable = yes`, `veto files = /<e1>/<e2>/`, `delete veto files = no` (research R3–R7); unit tests asserting exact output for the minimal and full configs in contracts/export-config.md, that the first non-comment line of `render_globals` is `[global]`, and that neither file contains `change notify = no` or `kernel change notify = no` (FR-013)
- [X] T012 Implement setup-component checks and state detection in `src/export/state.rs` per data-model.md → SetupComponent and ShareState: each check returns `Present | Missing | Unknown`; root-only checks run with `root_noninteractive` (`sudo -n`) and return `Unknown` when sudo refuses — never `Missing`, never a prompt; one check fn per component (`samba-package` via `dpkg-query -W -f='${Status}' samba`; `global-config` compares file with `render_globals`; `include-line` counts `include = /etc/samba/local-machine-export.conf` lines in `/etc/samba/smb.conf` == 1; `samba-user` via `sudo -n pdbedit -L` contains owner; `firewall` via `sudo -n ufw status` → inactive, or exactly one `Samba` rule and it is for `allowed_network` (a rule for another network = `Missing`, so a changed network requires setup); `nmbd-disabled` via `systemctl is-enabled nmbd` not `enabled`); `fn share_state(cfg, runner) -> ShareState { NotSetUp(missing: Vec<&str>), SetUp, Available }` plus `unknown: Vec<&str>`; `NotSetUp` when any component is `Missing` (a `global-config` mismatch caused by a changed `allowed_network`/`owner` carries the reason `configuration changed`); Available = no component missing + share file equals `render_share` + `systemctl is-enabled smbd` and `is-active smbd` both succeed — depends on T006, T007, T010, T011
- [X] T013 Write CLI shape tests in `tests/cli.rs` with `assert_cmd`: `local-machine export` exits 2 and lists the four actions; `local-machine export bogus` exits 2; `--config-dir` accepted before and after `export`; with an empty temp config dir all four actions exit 3 and print `no config at <dir>/export.yaml` — depends on T004, T005, T009, T010

**Checkpoint**: `cargo test` passes; the CLI parses, loads config, and can detect state

---

## Phase 3: User Story 1 - Edit home-directory files from Windows desktop tools (Priority: P1) 🎯 MVP

**Goal**: `export setup` then `export start` makes the home directory usable read-write from
Windows (Obsidian), persistent across reboots, password-protected, home network only,
`.ssh`/`.gnupg` hidden.

**Independent Test**: quickstart.md V2, V3, V4 (start output only), V5, V7 (Windows part),
V9, V10, V13, V16, V17, V18.

- [X] T014 [US1] Implement `export setup` in `src/export/setup.rs` as `Step`s run by `run_steps`, in this order: (1) `samba-package`: record whether `smbd` is enabled/active **before** installing, then `sudo apt-get install -y samba`, then restore that prior smbd state (`sudo systemctl disable --now smbd` if it was not enabled) because the package auto-starts smbd and setup MUST NOT change availability (FR-004); (2) `global-config`: `files::replace` with `render_globals`, then create an empty-stanza share file (header only) if absent; validate with `testparm -s` and roll back on failure; (3) `include-line`: append `include = /etc/samba/local-machine-export.conf` to `/etc/samba/smb.conf` if absent (via `files::replace` of the whole file, so it rolls back); (4) `samba-user`: only if owner is not in `pdbedit -L`, prompt with `rpassword` twice (`share password for <owner>:` / `again:`), refuse on mismatch or empty, run `smbpasswd -s -a <owner>` as root with stdin `"<pw>\n<pw>\n"`; never print or store the password; (5) `firewall`: if `ufw` active, delete every `Samba` rule whose source is not `allowed_network` (`sudo ufw delete allow from <old> to any app Samba`), then add `sudo ufw allow from <allowed_network> to any app Samba` if absent; if inactive print `ok: firewall (ufw inactive; Samba hosts allow still applies)`; (6) `nmbd-disabled`: `sudo systemctl disable --now nmbd` if enabled. Use the checks from `src/export/state.rs` as each step's `check()`
- [X] T015 [US1] Implement Windows connection info in `src/export/connect.rs`: find the local address inside `allowed_network` by parsing `ip -o addr show` (IPv4 and IPv6); `fn windows_path(cfg, addr) -> String` = `\\<addr>\<share_name>`; return `None` (and a reason) when no local address is inside the network
- [X] T016 [US1] Implement `export start` in `src/export/start.rs`: exit 3 if config invalid; run `sudo_preflight` first (before `share_state`, so its `sudo -n` checks have cached credentials); exit 4 with `export: not set up — run 'local-machine export setup'` plus missing components if `share_state` is `NotSetUp`, or `export: configuration changed — run 'local-machine export setup'` when the reason is a config change (change nothing); otherwise steps: `share-stanza` (`files::replace` with `render_share`, `testparm -s`, roll back on failure), `smbd-enabled` (`sudo systemctl enable --now smbd` if not enabled+active), `reload` (`sudo smbcontrol smbd reload-config` when the stanza changed); afterwards print `export: available` and the `windows:` line from `connect.rs`
- [X] T017 [US1] Wire `setup` and `start` dispatch in `src/export/mod.rs` and `src/main.rs`, passing `RealRunner` and the loaded `ExportConfig`
- [X] T018 [P] [US1] Write step tests in `tests/export_steps.rs` using `FakeRunner`: fresh machine → setup issues apt install, writes both managed files, appends include line, calls smbpasswd, and disables smbd again when it was disabled before; second setup run → only `ok:` lines and no smbpasswd call; start before setup → exit 4 and no root commands issued; start with `testparm` failing → share file restored from backup and exit 1; start twice → second run issues no `systemctl enable`; `render_globals` output begins with `[global]`; changing `allowed_network` then running start → exit 4 with `configuration changed`; setup after a network change deletes the old ufw rule and adds the new one; `sudo -v` is the first root command issued by setup and start
- [ ] T019 [US1] Validate on the real machine: quickstart.md V2, V3, V4, V5, V7 (Windows), V9, V10, V13, V16, V17, V18; record results in the commit message

**Checkpoint**: MVP — Windows can edit home-directory files through the share

---

## Phase 4: User Story 2 - Use home-directory files from another Unix VM (Priority: P2)

**Goal**: The same share serves Linux VMs with faithful Unix permissions and owner-owned files.

**Independent Test**: quickstart.md V6 and V7 (Unix VM part).

- [X] T020 [US2] Add `smb3 unix extensions = yes` to `render_share` in `src/export/samba.rs` (research R2; it is a per-share parameter); add unit tests asserting it is present and that neither rendered file contains `create mask`, `force create mode`, `directory mask`, or `force directory mode` (FR-017)
- [X] T021 [US2] Add the Unix mount line to `src/export/connect.rs`: `fn unix_mount(cfg, addr) -> String` = `sudo mount -t cifs //<addr>/<share_name> /mnt/<share_name> -o username=<owner>,posix,uid=$(id -u),gid=$(id -g)`; print it as the `unix:` line after `windows:` in `export start` (`src/export/start.rs`)
- [ ] T022 [US2] Validate on the real machine with a Linux VM: quickstart.md V6 and V7 (Unix VM part)

**Checkpoint**: Windows and Unix VMs both use the share

---

## Phase 5: User Story 3 - Check status (Priority: P3)

**Goal**: `export status` reports state, what is shared, how to connect, and warnings, without privileges.

**Independent Test**: quickstart.md V1, V4 (status), V8, V12.

- [X] T023 [P] [US3] Implement the symlink scan in `src/export/symlinks.rs`: walk `cfg.path` without following symlinked directories, skipping directories whose name is in `exclude`, to a maximum depth of 4 below `path`; for each symlink, canonicalize its target and report it if any path component of the target equals an excluded name; return `(warnings, depth_limit_hit: bool)`
- [X] T024 [US3] Implement `export status` in `src/export/status.rs` per contracts/cli.md → `export status`: never modifies anything and never runs `sudo -v`; uses `share_state` (whose root-only checks use `sudo -n`) and prints a `warning:` per `Unknown` component suggesting `sudo -v` first; output lines `export:`, `shared:` (path + excluded names), `network:` (CIDR + reachable yes/no + local address), `windows:`, `unix:`; `warning:` lines for drift (share file exists but differs from `render_share` → `run 'local-machine export start'`), for a setup-level config change (`configuration changed — run 'local-machine export setup'`), each symlink from `symlinks.rs`, and a note if the depth limit was hit; exit 0 available, 3 not configured, 4 not set up (list missing components), 5 set up but not available
- [X] T025 [US3] Wire `status` dispatch in `src/export/mod.rs`; add `FakeRunner` tests to `tests/export_steps.rs` for each exit code (0, 3, 4, 5) and for the drift warning; assert status issues no `sudo -v` and every root command it issues is `sudo -n`; with sudo refusing, status reports `unknown` components and still exits by the remaining state
- [ ] T026 [US3] Validate on the real machine: quickstart.md V1, V4, V8, V12

**Checkpoint**: Status works in every state

---

## Phase 6: User Story 4 - Stop sharing (Priority: P4)

**Goal**: `export stop` makes the share unavailable, persistently, idempotently.

**Independent Test**: quickstart.md V11.

- [X] T027 [US4] Implement `export stop` in `src/export/stop.rs`: exit 3 if config invalid; run `sudo_preflight` before `share_state`; exit 4 if `NotSetUp` (never install anything); if share file is header-only and smbd is disabled+inactive, print `export: nothing was shared` and exit 0; otherwise steps: `share-stanza-removed` (`files::replace` share file with header only, `sudo smbcontrol smbd reload-config` if smbd active), `smbd-disabled` (`sudo systemctl disable --now smbd`)
- [X] T028 [US4] Wire `stop` dispatch in `src/export/mod.rs`; add `FakeRunner` tests to `tests/export_steps.rs`: stop when available → share file emptied and smbd disabled; second stop → `nothing was shared`, no root commands; stop before setup → exit 4, no apt calls
- [ ] T029 [US4] Validate on the real machine: quickstart.md V11

**Checkpoint**: Full lifecycle works

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T030 [P] Write `README.md` at the repository root: build/install (`cargo install --path .`), the minimal and full `export.yaml` from contracts/export-config.md, the four `export` actions, and how to connect from Windows and Linux
- [X] T031 Run `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`; fix all findings
- [ ] T032 Run the remaining quickstart.md scenarios V14 (setup→vault under 5 minutes) and V15 (note save under 1 second); then re-run V1–V18 end to end on a clean state
- [X] T033 Review against the constitution Governance checklist (Principles I–V, including the config-sharing review in research.md R12); record any deviation in plan.md → Complexity Tracking

---

## Real-Machine Validation Progress (2026-10-31)

Quickstart checks run by the owner on the real machine. T019, T022, T026, T029 and T032 stay
open until every check they list has passed.

| Check | Result |
|-------|--------|
| V1 status before setup | pass (exit 4) |
| V2 start before setup | pass (exit 4, no prompt, nothing changed) |
| V3 setup twice | pass (second run only `ok:`, no share-password prompt) |
| V4 start, status | pass (available, exit 0; smbd enabled+active) |
| V5 Windows + Obsidian | pass |
| V7 excluded names (Windows) | pass |
| V9 wrong password, guest | pass; outside-network half not tested (no host outside 192.168.1.0/24) |
| V13 no password in config | pass |
| V15 ~500 KB note save | pass |
| V17 Windows-illegal file name | pass (mangled name visible via `dir`; Ubuntu file unchanged) |
| V18 globals applied | pass, with Ubuntu's `[printers]`/`[print$]` shares accepted (research R5) |

**Not yet run**: V6 and V7 (Unix VM), V8, V10 (reboot), V11, V12, V14, V16, then the full
V1–V18 re-run (T032).

**Open question**: whether to add `smb ports = 445` so smbd stops listening on port 139
(still covered by `hosts allow` and the password). Owner has not decided.

**Future idea (owner, 2026-10-31)**: allow access over Tailscale — `allowed_network` would
become a list; its own spec when wanted.

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: none
- **Foundational (Phase 2)**: after Setup; blocks all stories
- **US1 (Phase 3)**: after Foundational
- **US2 (Phase 4)**: after US1 (it extends `samba.rs`, `connect.rs`, and `start.rs`, and needs a running share to validate)
- **US3 (Phase 5)**: after Foundational for T023–T025; T026 validation needs US1. Uses `connect.rs` (US1, and US2 for the `unix:` line)
- **US4 (Phase 6)**: after Foundational; T029 validation needs US1
- **Polish (Phase 7)**: after all stories

### Within Each Story

- Rendering/state (Foundational) → action implementation → wiring → FakeRunner tests → real-machine validation

### Parallel Opportunities

- Phase 1: T003 alongside T001/T002
- Phase 2: T005, T006, T007, T010, T011 in parallel (different files); then T008, T009, T012, T013
- After US1: US3 (T023–T025) and US4 (T027–T028) can proceed in parallel — different files except the one-line dispatch in `src/export/mod.rs`
- T018 can be written alongside T014–T016 once the step names are fixed

---

## Parallel Example: Phase 2

```text
Task: "Implement shared config loading in src/config.rs"            (T005)
Task: "Implement command running in src/system/mod.rs"               (T006)
Task: "Implement atomic managed-file writes in src/system/files.rs"  (T007)
Task: "Implement ExportConfig in src/export/config.rs"               (T010)
Task: "Implement pure Samba rendering in src/export/samba.rs"        (T011)
```

## Parallel Example: after US1

```text
Task: "Implement the symlink scan in src/export/symlinks.rs"         (T023, US3)
Task: "Implement export stop in src/export/stop.rs"                  (T027, US4)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Phase 1 → Phase 2 (`cargo test` green)
2. Phase 3 (US1) → validate V2–V5, V7, V9, V10, V13 on the real machine with Windows
3. **STOP**: Obsidian on Windows works against the home directory

### Incremental Delivery

1. US1 → Windows editing (MVP)
2. US2 → Linux VMs
3. US3 → status
4. US4 → stop
5. Polish → README, lint, full quickstart

---

## Notes

- Every action that changes the system goes through `run_steps`, so idempotence (FR-002)
  and rollback (FR-009) come from one place.
- Never write the share password to disk, logs, or config (FR-015); `RealRunner` must not
  log `stdin`.
- Commit after each phase checkpoint.
