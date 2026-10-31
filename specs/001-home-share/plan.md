# Implementation Plan: `local-machine export`

**Branch**: `001-home-share` | **Date**: 2026-10-30 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/001-home-share/spec.md`

## Summary

Add the first `local-machine` subcommand, `export`, with actions `setup`, `start`, `status`,
`stop`. It shares the owner's home directory over SMB 3 using Samba, so Windows (built-in
client) and Linux VMs (`mount -t cifs`, POSIX extensions) can both use it read-write.
Access is limited to the owner's password and the configured home network; `.ssh` and
`.gnupg` are hidden via `veto files`. Every action is an idempotent list of check/apply
steps that run privileged commands through `sudo` and roll back files on failure. This
feature also creates the `local-machine` Rust crate and its shared `--config-dir` handling.

## Technical Context

**Language/Version**: Rust (stable; cargo 1.98 on the target machine), edition 2024

**Primary Dependencies**: `clap` (derive), `serde`, `serde_yaml_ng`, `ipnet`, `rpassword`,
`anyhow`, `thiserror`; system: Samba 4.23 (`smbd`, `smbpasswd`, `pdbedit`, `testparm`,
`smbcontrol`), `apt-get`, `systemctl`, `ufw`, `sudo`

**Storage**: YAML config `<config-dir>/export.yaml`; managed files under `/etc/samba/`;
password in Samba's own database (never in config)

**Testing**: `cargo test` (unit + `assert_cmd` CLI tests with a fake command runner);
end-to-end on the real machine via [quickstart.md](quickstart.md)

**Target Platform**: Ubuntu 26.04 LTS (VM), systemd; clients Windows 10/11 and Linux VMs

**Project Type**: CLI (single binary `local-machine`)

**Performance Goals**: Note open/save from Windows < 1 s on LAN (SC-003); setup→vault < 5
min (SC-001)

**Constraints**: Run as the owner, not root (so `~` resolves correctly); no machine-specific
literals in code; no secrets in config; idempotent and roll-back-on-failure actions

**Scale/Scope**: One owner, one share, a handful of clients

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check | Pre-research | Post-design |
|-----------|-------|--------------|-------------|
| I. Single entry point & grammar | `local-machine export {setup,start,status,stop}`; shared verbs `setup`/`start`/`status`/`stop` used with their constitutional meaning; no sibling subcommands or mode flags | PASS | PASS ([contracts/cli.md](contracts/cli.md)) |
| II. Config, not hardcoding | All machine facts (network, share name, path, owner, excludes) in `<config-dir>/export.yaml`, default `~/.config/local-machine`, `--config-dir` override; only non-machine defaults in code (`home`, `.ssh`/`.gnupg`, current user's home); config-sharing review recorded (research R12: first subcommand, nothing to share) | PASS | PASS ([contracts/export-config.md](contracts/export-config.md)) |
| III. Shared parameter vocabulary | Only parameter is the global `--config-dir` | PASS | PASS |
| IV. Rust | Whole tool in Rust; system programs invoked via `std::process::Command` (permitted) | PASS | PASS |
| V. Ubuntu target | apt, systemd, ufw, Ubuntu Samba paths; no portability layer | PASS | PASS |

No violations; Complexity Tracking is empty.

## Project Structure

### Documentation (this feature)

```text
specs/001-home-share/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1–R12
├── data-model.md        # Phase 1: ExportConfig, ShareState, SetupComponent, managed files
├── quickstart.md        # Phase 1: validation scenarios V1–V15
├── contracts/
│   ├── cli.md           # Command grammar, output, exit codes
│   └── export-config.md # YAML schema and errors
└── tasks.md             # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
Cargo.toml                    # crate `local-machine`: library + binary `local-machine`
src/
├── main.rs                   # parse CLI, run, print error, exit code
├── lib.rs                    # run(), ExitError, exit-code mapping (lib so tests/ can use FakeSystem)
├── cli.rs                    # clap tree: global --config-dir, `export <action>`
├── config.rs                 # shared: resolve config dir, load <name>.yaml (any subcommand)
├── system/
│   ├── mod.rs                # System trait + RealSystem, Cmd, sudo preflight
│   ├── action.rs             # shared privileged-action runner: Gate, Step, look-then-act, NoPrompt
│   ├── fake.rs               # FakeSystem for tests
│   └── files.rs              # replace with in-memory backup/restore
└── export/
    ├── mod.rs                # action dispatch
    ├── config.rs             # ExportConfig + validation
    ├── samba.rs              # pure rendering of global + share config text
    ├── state.rs              # ShareState detection; SetUp gate for start/stop
    ├── setup.rs              # SetupComponent steps
    ├── start.rs
    ├── status.rs             # report, reachability, drift, symlink warnings
    ├── stop.rs
    ├── connect.rs            # local address in allowed_network; Windows path, Unix mount line
    └── symlinks.rs           # depth-limited scan for symlinks into excluded names
tests/
├── cli.rs                    # assert_cmd: grammar, help, exit codes, config errors
└── export_steps.rs           # step plans against FakeRunner (idempotence, rollback)
```

**Structure Decision**: Single binary crate. `config.rs` and `system/` are shared
infrastructure for future subcommands (e.g. `hypostasis`); each subcommand lives in its own
module directory, mirroring the command tree.

## Owner Decisions (2026-10-30)

1. **Exclusions are names, at any depth (R6)** — accepted; spec FR-014 and SC-006 amended.
2. **One config file per subcommand (R12)** — accepted: `<config-dir>/export.yaml`, later
   `<config-dir>/hypostasis.yaml`.
3. **Symlinks into excluded names: warn only (R7)** — the exclusion list is a convenience,
   not a security boundary. `follow symlinks = yes`, `wide links = no`; `status` warns.
   Spec edge case amended.
4. **Network/owner changes require `export setup` again (R5)** — `start` and `status` detect
   the mismatch and say so; spec edge case amended.

## Implementation Notes (2026-10-31)

- `System` (one trait for commands, managed files, directory ownership, password prompt)
  replaces the planned `CommandRunner`, so file effects are faked in tests too.
- Backups for rollback are kept in memory for the duration of an action instead of
  `<path>.local-machine.bak` files; managed files are small and rollback happens in-process.
- Privileged actions run through `system::action::run`, shared by every subcommand. It
  checks the gate and every step first through `NoPrompt` (reads and `sudo -n` only; anything
  that could prompt or change the machine is an error), then:
  - gate refuses (e.g. not set up) → exit with its code, no notice, no prompt;
  - everything already in place → `ok:` lines, no notice, no prompt;
  - otherwise → a privilege notice naming only the steps that need root to change or to
    check, `sudo -v`, the gate re-checked as root, then check→apply→rollback per step.
  Each `Step` supplies its own notice line (`None` when it needs no root). This supersedes
  the "preflight first, before any check" order in T008/T016/T027 (FR-009 still holds:
  the notice precedes every root command). `FakeSystem::prompted()` asserts that order in
  tests.
- `smb3 unix extensions = yes` is a share parameter (research R2), rendered in the share
  file rather than the globals.

## Complexity Tracking

No constitution violations (re-checked after implementation, T033).
