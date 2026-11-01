# Implementation Plan: `local-machine hypostasis`

**Branch**: `002-hypostasis-setup` | **Date**: 2026-11-01 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/002-hypostasis-setup/spec.md`

## Summary

Add the `hypostasis` subcommand (`setup` / `start` / `status` / `stop`).

- **`setup`** brings a fresh Ubuntu machine to the point where `mneme up` works. In order:
  1. apt prerequisites, `uv`, and linger;
  2. git checkouts and sparse campaign trees (created only when missing, never touched
     afterwards);
  3. seeds hypostasis's authority file from the owner's copy;
  4. opens the three services' ports to the home network in ufw — the last root step,
     placed before the slow steps so sudo's cached credentials are still valid (analysis I1);
  5. creates the venv the authority names and installs hypostasis into it;
  6. runs `hypostasis install`;
  7. installs the three local services (rpg-lib, flexai-social, flexai-combat) into their
     own venv, with systemd **user** units;
  8. restores the library DB, the FlexAI workbooks and the palaces from folders under a
     backup root on the Google Drive mount. Each restore runs only when the item is
     missing, and palaces go through `mneme mp restore`.
- **`start` / `stop`** enable or disable the user units and wait until each service answers
  (or stops answering) over HTTP.
- **`status`** reports what is observed and never prompts.

Root is needed only for apt, linger and ufw, all inside one run of the shared runner.

This feature also creates the first shared config file, `machine.yaml` (`allowed_network`,
`owner`), and refactors `export` to read those two values from it (constitution Principle
II).

## Technical Context

**Language/Version**: Rust (stable), edition 2024. Same crate as `export`.

**Primary Dependencies**:
- Crates: `clap`, `serde_yaml_ng`, `ipnet`, `anyhow`, `thiserror`, `tempfile`. No new crates;
  the HTTP probe is a minimal HTTP/1.0 GET over `std::net::TcpStream`.
- System: `apt-get`, `dpkg-query`, `pipx`, `uv`, `git`, `systemctl --user`, `loginctl`,
  `ufw`, `ss`, `sqlite3`, `npm`.
- Our tools: `hypostasis` and `mneme` (installed into the authority's venv).

**Storage**:
- Our config: `<config-dir>/machine.yaml` and `<config-dir>/hypostasis.yaml`.
- Read-only: hypostasis's authority file.
- Written: user units in `~/.config/systemd/user/` and logs in
  `~/.local/state/local-machine/hypostasis/`.
- No state file of our own: everything is observed on each run.

**Testing**:
- `cargo test`: unit tests plus scripted `FakeSystem` tests for every action, including
  privilege order via `FakeSystem::prompted()` (Principle VI).
- Real-machine validation: [quickstart.md](quickstart.md) V1–V18.

**Target Platform**: Ubuntu 26.04 LTS with systemd (Linux-Alien; also a fresh VM).

**Project Type**: CLI (single binary `local-machine`).

**Performance Goals**:
- No time target (SC-002's time bound was dropped, analysis G2). A setup re-run on a
  set-up machine only runs checks, with no network except the HTTP probes of local
  services.
- `start` waits up to 30 s per service; `stop` up to 15 s.

**Constraints**:
- Run as the owner, never as root.
- No machine-specific literals in code.
- Never modify an existing checkout or existing data.
- Ask for the password at most once per run, and only for root work.
- Status never prompts.

**Scale/Scope**:
- About 12 checkouts, 3 services, about 4 palaces.
- New code is roughly a second subcommand of `export`'s size.

## Constitution Check

*Gate before Phase 0 and again after Phase 1.*

| Principle | Status | Evidence |
|---|---|---|
| I. Single entry point, grammar | ✅ | `local-machine hypostasis <setup\|start\|status\|stop>`; shared verbs; `setup` is the only action that installs |
| II. Config not hardcoding; per-subcommand file; sharing review | ✅ | `hypostasis.yaml`; sharing review in research R3 → `machine.yaml` created, `export` refactored in this feature, Principle II amended to 1.4.1 (PATCH) on this branch to record the file name and schema, and to allow reading — never writing — another tool's own config at a path named in our config (analysis D1); authority values not duplicated (FR-022, R2) |
| III. Shared parameter names | ✅ | only `--config-dir`; config keys `allowed_network`/`owner` keep `export`'s names |
| IV. Rust | ✅ | all logic in Rust; external programs invoked from Rust (hypostasis/mneme/uv/git/systemctl); no shell scripts |
| V. Ubuntu | ✅ | apt, systemd user units, ufw, loginctl |
| VI. Privileged actions | ✅ | apt/linger/ufw are runner steps; checks use `sudo -n`; notice lists only root work; gate refuses before any prompt; start/stop/status need no root; tests use `prompted()`. The runner gains an opt-in `continue_on_failure` for independent steps (restores). The rule is unchanged |
| Source control | ✅ | all work on `002-hypostasis-setup`, merged to `main` |

**Post-design re-check (after Phase 1)**: still ✅. One deviation from the spec text was
corrected rather than justified: hypostasis's runtime environment comes from the authority,
not from our config (R2; FR-021/FR-022 and Key Entities updated). No Complexity Tracking
entries.

## Project Structure

### Documentation (this feature)

```text
specs/002-hypostasis-setup/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/
│   ├── cli.md
│   ├── hypostasis-config.md
│   └── machine-config.md
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks (not created here)
```

### Source Code (repository root)

```text
src/
├── cli.rs                  # + Hypostasis subcommand and its four actions
├── config.rs               # + `~` expansion helper; optional-file read
├── machine.rs              # NEW: MachineConfig (machine.yaml): allowed_network, owner
├── export/
│   └── config.rs           # refactor: allowed_network/owner from machine.yaml; leftovers are errors
├── hypostasis/             # NEW
│   ├── mod.rs              # dispatch
│   ├── config.rs           # HypostasisConfig parse/validate
│   ├── authority.rs        # read-only view of hypostasis's authority file
│   ├── pieces.rs           # every SetupPiece as an action::Step (check/apply)
│   ├── checkout.rs         # clone / sparse clone / origin match / occupied
│   ├── restore.rs          # DB, FlexAI workbooks, palaces (via mneme)
│   ├── services.rs         # LocalService table, unit rendering, probe, port holder, log tail
│   ├── firewall.rs         # ufw rules by port with comment; parse shared with export
│   ├── setup.rs
│   ├── start.rs
│   ├── stop.rs
│   └── status.rs
└── system/
    ├── action.rs           # + per-step continue_on_failure; collected failures
    ├── mod.rs              # System: + http_status, path_kind, write_user_file, rename, copy_file, make_dir
    └── fake.rs             # + fake HTTP answers, filesystem kinds

tests/
├── hypostasis_config.rs
├── hypostasis_setup.rs     # FakeSystem scenarios: fresh, partial, idempotent, occupied, prompts
├── hypostasis_services.rs  # start/stop/status: running, foreign, blocked, timeout + log tail
└── export_*.rs             # updated for machine.yaml
```

**Structure decision**: same single-crate layout as `export`. `hypostasis/` mirrors
`export/`. The shared machine config goes in `src/machine.rs`, next to `src/config.rs`.
The ufw output parser moves out of `export/state.rs` into a shared module, and both
subcommands use it.

## Phase 0 / Phase 1 outputs

- [research.md](research.md), sections R1–R13. All unknowns are resolved; none are left as
  NEEDS CLARIFICATION.
- [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md).

## Decisions confirmed by the owner at plan review (2026-11-01)

1. **Shared file**: `machine.yaml` holds `allowed_network` and `owner` (R3). A leftover key
   in `export.yaml` is a hard error with no fallback.
2. **One services venv**, named in our config (e.g. `~/.venvs/services`), separate from
   hypostasis's venv (R6). rpg-lib moves off the unmanaged `~/.venv`.
3. **systemd user units + linger** (R6). Start and stop never prompt.
4. **Backups**: a required `backup_root`, on this machine
   `/mnt/g/My Drive/hypostasis/backups`. Each kind of data defaults to a folder under it
   (`rpg-lib/`, `flexai-social/`, `flexai-combat/`, `palaces/`), and each folder can be
   overridden under `backups:` (R7).
5. **`uv` via `pipx`**, never `curl | sh` (R5). An existing `uv` on `PATH` is left alone.

## Complexity Tracking

None. No constitution violations.
