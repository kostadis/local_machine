---

description: "Task list for local-machine hypostasis"
---

# Tasks: `local-machine hypostasis`

**Input**: Design documents from `specs/002-hypostasis-setup/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md),
[data-model.md](data-model.md), [contracts/cli.md](contracts/cli.md),
[contracts/hypostasis-config.md](contracts/hypostasis-config.md),
[contracts/machine-config.md](contracts/machine-config.md), [quickstart.md](quickstart.md)

**Tests**: The tests are included because the plan's testing strategy (plan.md → Technical
Context → Testing) and constitution Principle VI call for them:
- `cargo test` unit tests;
- `FakeSystem` scenario tests for every action;
- the privilege order asserted with `FakeSystem::prompted()`.

Real-machine behaviour is checked by hand with quickstart.md V1–V23.

**Organization**: Tasks are grouped by user story (spec.md):
- US1: fresh machine to ready (P1)
- US2: safe re-run (P1)
- US3: start/stop the local services (P1)
- US4: status (P2)

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: the user story the task belongs to
- Paths are relative to the repository root (`/home/kostadis/src/local_machine`)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Skeleton for the new subcommand.

- [X] T001 Create the module skeleton from plan.md → Source Code as empty, compiling
  modules. New files: `src/machine.rs` and
  `src/hypostasis/{mod.rs,config.rs,authority.rs,pieces.rs,checkout.rs,restore.rs,services.rs,firewall.rs,setup.rs,start.rs,stop.rs,status.rs}`.
  Declare them in `src/lib.rs`. `cargo build` must pass.
- [X] T002 Add the `hypostasis` subcommand to the clap tree in `src/cli.rs`, with the
  required actions `setup|start|status|stop` (contracts/cli.md → Grammar). Each action
  dispatches to a stub in `src/hypostasis/mod.rs` that returns exit 1 "not implemented".
  `local-machine hypostasis` with no action prints help and exits 2. Add a test to
  `tests/cli.rs`: `local-machine --help` lists `hypostasis`, and a bare
  `local-machine hypostasis` exits 2 and lists the four actions.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared config, system and runner extensions, and the config and authority
readers that every action needs.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

### Shared config (`machine.yaml`) and the `export` refactor (research R3)

- [X] T003 Implement `MachineConfig` in `src/machine.rs` per contracts/machine-config.md.
  - `load(config_dir, sys) -> Result<MachineConfig, ConfigError>` reads
    `<config_dir>/machine.yaml`.
  - `allowed_network` is required: a CIDR, truncated to its network.
  - `owner` is optional; it defaults to `sys.current_user()`, must not be `root`, and must
    exist via `getent passwd`.
  - Unknown keys are errors. A missing file gives
    `no config at <dir>/machine.yaml (need at least allowed_network)`.
  - Move `passwd_entry` and `PasswdEntry` from `src/export/config.rs` here, as `pub`, so
    both subcommands use them.
  - Unit tests: minimal file, owner set, owner `root`, unknown owner, bad CIDR, unknown key,
    missing file.
- [X] T004 Refactor `src/export/config.rs` to take `allowed_network` and `owner` from
  `MachineConfig`.
  - `KEYS` becomes `share_name`, `path`, `exclude`.
  - `allowed_network` or `owner` in `export.yaml` is a `ConfigError::InvalidKey` with the
    reason `moved to <dir>/machine.yaml` (no fallback).
  - A missing `export.yaml` is valid and means all defaults: `config::read_mapping` gains an
    optional-file variant in `src/config.rs`.
  - `load(config_dir, sys)` now loads `MachineConfig` first. `ExportConfig` keeps its
    fields, so the rest of `export` is unchanged.
  - Update the unit tests in `src/export/config.rs`: leftover key → moved error; empty or
    missing `export.yaml` → defaults.
  - Update feature 001's docs to the new behaviour (analysis G1):
    `specs/001-home-share/contracts/export-config.md` (keys, the moved-key error, a missing
    file is now valid), `contracts/cli.md` (exit-3 messages), `data-model.md`
    (`ExportConfig` sources) and the `quickstart.md` steps that write `export.yaml`. Put a
    dated note at the top of each changed file: "Updated 2026-11-01 by 002: shared values
    moved to `machine.yaml`".
  - Depends on T003.
- [X] T005 Update the `export` integration tests (`tests/export_*.rs`, `tests/cli.rs`) so
  their temp config dirs write `machine.yaml` with `allowed_network`, and expect the new
  `adding: <step>` lines from T008 wherever they match exact output. Add one test where
  `export.yaml` still holds `allowed_network` → exit 3 naming `machine.yaml`. The whole
  suite (48 tests) must still pass. Depends on T004.
- [X] T006 Amend the constitution in `.specify/memory/constitution.md`, Principle II
  (owner decisions D1 and R3, 2026-11-01):
  - Record the shared file `machine.yaml` and its schema (`allowed_network` required,
    `owner` optional, never root).
  - Add: "A subcommand MAY read, and MUST NOT write, another tool's own configuration
    file, at a path named in its own config, for values that tool owns. Those values MUST
    NOT be copied into `local-machine` config." This permits reading hypostasis's
    authority file (FR-022).
  - Version 1.4.0 → 1.4.1. The owner chose PATCH: both changes clarify Principle II's
    intent (one home per value).
  - Add a Sync Impact Report entry, and replace "Deferred: shared config file name/schema"
    with the resolution.
  - Update the matching line in `CLAUDE.md` in the same change (Governance sync rule).

### System and runner extensions

- [X] T007 [P] Extend the `System` trait in `src/system/mod.rs` with owner-level
  operations, all of which run without root:
  - `path_kind(&Path) -> PathKind {Missing, File, Dir, Other}`
  - `make_dir_all(&Path)`
  - `write_user_file(&Path, &str)`, atomic: temp file in the same directory, then rename
  - `rename(&Path, &Path)`
  - `copy_file(&Path, &Path)`
  - `remove_dir_all(&Path)`
  - `http_status(port: u16, path: &str, timeout: Duration) -> Option<u16>`: a minimal
    HTTP/1.0 GET to `127.0.0.1:<port>` over `std::net::TcpStream`, parsing the status line;
    `None` on connect failure or timeout
  - `effective_uid() -> u32`
  - `home_dir() -> Option<PathBuf>`

  Implement them for `RealSystem`. In `src/system/fake.rs`, implement them over the
  in-memory `files`/`dirs` maps, plus `set_http(port, path, Option<u16>)`, a scriptable
  sequence of answers per port so wait loops can be tested, and `set_euid`. Record every
  mutating call in `calls`. Add unit tests for the HTTP status-line parser, using canned
  bytes.
- [X] T008 [P] Add per-step failure tolerance to the runner in `src/system/action.rs`
  (research R13).
  - `Step` gains `fn continue_on_failure(&self) -> bool { false }`.
  - In `run_steps`, a failing step that returns true prints `error: <name>: <reason>`, is
    recorded, and the loop goes on.
  - After the loop, if any were recorded, return an error that lists them, without rolling
    back the steps that succeeded.
  - A failing step that returns false behaves as today.
  - Unit tests: two tolerant failures plus one success → error naming both, success kept;
    an intolerant failure after a tolerant one → stops at once.
  - Also (analysis C1, spec US1 AC3): before applying a step whose check says it needs
    work, `run_steps` prints `adding: <name>`; `changed:`/`error:` follow as now. Steps
    already done still print only `ok:`. This applies to every action, `export` included.
    Unit test: the output order for one done step and one changed step is `ok: a`,
    `adding: b`, `changed: b`.
- [X] T009 [P] Move the ufw parsing out of `src/export/state.rs` into a shared module,
  `src/system/ufw.rs`.
  - `parse_status(stdout) -> Option<Vec<UfwRule>>` returns `None` when inactive, where
    `UfwRule { to: String, from: String, comment: Option<String> }` covers both app rules
    (`Samba`) and port rules (`8000/tcp`) and parses the `# comment` suffix.
  - Keep `export` working by deriving the old `parse_ufw` result from it.
  - Move the existing ufw unit tests over, and add port-rule and comment cases.

### hypostasis config and authority readers

- [X] T010 [P] Implement `HypostasisConfig` in `src/hypostasis/config.rs` per
  contracts/hypostasis-config.md and data-model.md → HypostasisConfig, Checkout,
  FlexAIService.
  - Keys and nesting are as in the contract, and `~` expands to the owner's home.
  - Defaults: `authority` → `~/.config/hypostasis/hypostasis.yaml`; each `backups.<kind>`
    → `<backup_root>/<folder>` (`rpg-lib`, `flexai-social`, `flexai-combat`, `palaces`);
    `prerequisites.apt` → the research R5 list, plus `pipx`.
  - Every rule under "Validation" maps to a `ConfigError`. That includes: the
    "moved to machine.yaml" rule; the "lives in <authority>" rule for authority keys
    (`venv`, `installer`, `rpg_lib`, `rpg_library_db`, `services.rpg_lib`, `env` at top
    level, `dgx`); unique and non-nested checkout paths; `services.mytools` must be a
    checkout path, and so must `hypostasis_source` (analysis U1); distinct ports; the `env` key allow-list; `password`/`token` keys
    forbidden.
  - Unit tests: one per validation rule, plus a full valid example (the contract's).
- [X] T011 [P] Implement the read-only authority view in `src/hypostasis/authority.rs` per
  data-model.md → Authority.
  - `load(path) -> Result<Authority, AuthorityError>` reads only `venv`, `installer`,
    `services.rpg_lib.{url,port,health.path}` (health path defaults to `/`),
    `data_roots.rpg_library_db`, `data_roots.campaigns`, `components.*.source.path` and
    `order.install`, expanding `~`. Everything else is ignored.
  - An `installer` other than `uv` is an error naming it.
  - `services.rpg_lib.url` (analysis I3): its port must equal `port`, and its host must be
    this machine — `localhost`, `127.0.0.1`, `::1`, `hostname`, or one of the addresses from
    `ip -o addr show` (passed in, so the check is testable). Otherwise exit 3 naming both
    values ("rpg-lib must run on this machine"). Unit tests: `localhost`, the LAN address,
    another host, and a port mismatch.
  - A missing or invalid authority file is an error naming the full path, and the message
    says it is hypostasis's authority file (research R11).
  - Never writes.
  - Unit tests use a copy of this machine's authority file as a fixture in
    `tests/fixtures/authority.yaml` (with the `mneme.id` value replaced by a placeholder).
- [X] T012 Implement the owner gate in `src/hypostasis/mod.rs`, shared by all four actions.
  - Load `MachineConfig` and `HypostasisConfig`.
  - Refuse with exit 3 when `effective_uid() == 0` (`must not run as root`) or when the
    invoking user differs from `owner`.
  - Return a `Context { machine, cfg, sys }`. The authority is loaded lazily, because on a
    fresh machine it doesn't exist until setup seeds it.
  - Test in `tests/hypostasis_config.rs`: all four actions exit 3 with the right message for
    a missing `machine.yaml`, a missing `hypostasis.yaml`, root, and the wrong user, and
    none of them calls `sudo`.
  - Depends on T003, T010.
- [X] T013 Implement the local-service table in `src/hypostasis/services.rs` per
  data-model.md → LocalService and research R6.
  - `LocalService {name, port, health_path, unit_name, log_path, app}` for rpg-lib (port
    and path from the authority) and the two FlexAI services (from config).
  - `render_unit(svc, cfg, authority) -> String` produces the systemd user unit:
    `Description`, `WorkingDirectory`, `ExecStart` exactly as in research R6, with the
    `--host 0.0.0.0` flag; `Environment=` lines from `flexai_social.env`, sorted;
    `Restart=on-failure`; `StandardOutput=append:<log>` and `StandardError=append:<log>`;
    `[Install] WantedBy=default.target`. Values containing spaces are quoted.
  - `probe(sys, svc) -> bool`: an HTTP status below 500 within 2 s.
  - `unit_active` / `unit_enabled` via `systemctl --user is-active|is-enabled`.
  - `port_holder(sys, port) -> Option<String>`, parsing `ss -ltnpH sport = :<port>`.
  - `log_tail(sys, svc, 20) -> Vec<String>`.
  - Unit tests: the exact unit text for each service; `ss` output with and without a
    process name; a log shorter than 20 lines.
  - Depends on T007, T010, T011.

**Checkpoint**: `cargo test` passes. `export` reads `machine.yaml`. The `hypostasis`
config, authority and service table parse, and all four actions refuse correctly.

---

## Phase 3: User Story 1 - Bring a fresh machine to "ready" with one command (Priority: P1) 🎯 MVP

**Goal**: On a fresh Ubuntu machine with both config files, `local-machine hypostasis
setup` puts every piece in place, asks for the password at most once and only for root
work, starts nothing, and ends with the next steps.

**Independent Test**: a `FakeSystem` fresh-machine scenario (T024); quickstart.md V13 on a
new VM.

Each piece below is an `action::Step` in `src/hypostasis/pieces.rs` with the name from
data-model.md → SetupPiece. `check()` follows the "Present when" column and returns
`NeedsChange` (not an error) when its inputs don't exist yet (research R13). `notice()`
returns `Some` only for root steps. `rollback()` keeps the default no-op, so completed
pieces survive a later failure.

- [X] T014 [P] [US1] `packages` step.
  - Check: `dpkg-query -W -f='${Status}' <pkg>` for each package in `prerequisites.apt`,
    counting `node`/`npm` as present when the commands resolve (research R5).
  - Notice: `install packages: <missing…>`.
  - Apply: `sudo apt-get install -y <missing…>`.
- [X] T015 [P] [US1] `uv` step.
  - Check: `uv` on `PATH` or at `~/.local/bin/uv`.
  - Apply: `pipx install uv`, as the owner, with no notice.
- [X] T016 [P] [US1] `linger` step.
  - Check: `loginctl show-user <owner> -p Linger` → `Linger=yes`.
  - Notice: `enable linger for <owner>`.
  - Apply: `sudo loginctl enable-linger <owner>`.
- [X] T017 [US1] Checkouts in `src/hypostasis/checkout.rs`, per research R4.
  - `normalize_remote(url)`: drop a trailing `.git` and `/`; treat `git@github.com:x` the
    same as `https://github.com/x`.
  - `inspect(sys, checkout) -> {Present, Missing, Occupied(found)}`, where `found` is one
    of `a file`, `not a git repository`, `origin is <url>`.
  - `create(sys, checkout)`: plain `git clone --branch <b> <repo> <path>`, or the sparse
    sequence (`clone --no-checkout --branch`, `sparse-checkout init --cone`,
    `sparse-checkout set <dirs…>`, `checkout <b>`). On failure it removes only the
    directory it created.
  - One `checkout <path>` step per configured entry. An `Occupied` result fails the step
    with `occupied (<found>); not touched`.
  - Unit tests for `normalize_remote` and for each `inspect` outcome.
  - Depends on T007.
- [X] T018 [US1] `authority` step (FR-007).
  - Check: the authority file exists.
  - Apply: `authority_source` must be readable (otherwise fail naming it); copy it to
    `authority` with `copy_file`, creating the parent directory.
  - It never edits the content or replaces a file that is already there.
- [X] T019 [US1] `component-paths` step: a check-only step after `authority` (research R4).
  Every `components.*.source.path` in the authority must equal a checkout path, otherwise
  the step fails naming the uncovered paths. Its `apply` re-checks and fails the same way,
  so nothing is ever changed. Depends on T011, T017.
- [X] T020 [US1] `venv`, `hypostasis` and `components` steps (FR-006, FR-008, research R1).
  - `venv`:
    - check: `<venv>/bin/python --version` succeeds;
    - apply: `uv venv <venv>`.
  - `hypostasis`:
    - check: `<venv>/bin/hypostasis --help` exits 0;
    - apply: `uv pip install --python <venv>/bin/python -e <hypostasis_source>` (the
      config key from T010; analysis U1).
  - `components`:
    - check: `uv pip show --python <venv>/bin/python <name>` succeeds for each name in
      `order.install`;
    - apply: `<venv>/bin/hypostasis install --config <authority>`, printing hypostasis's
      output. A non-zero exit fails the step and shows hypostasis's own stderr.
- [X] T021 [US1] Service install steps, in `src/hypostasis/pieces.rs` using `services.rs`.
  - `services-venv`:
    - check: `<svenv>/bin/python` runs;
    - apply: `uv venv <svenv>`.
  - `install rpg-lib`:
    - check: `uv pip show --python <svenv>/bin/python rpg-lib claudelib` succeeds, and
      `<mytools>/rpg-lib/frontend/dist/index.html` exists;
    - apply: `uv pip install -e <mytools>/claudelib -e <mytools>/rpg-lib`, then, when
      `dist` is missing, `npm ci` and `npm run build` in `rpg-lib/frontend`.
  - `install flexai-social` / `install flexai-combat`:
    - check: flask and openpyxl are importable in the services venv;
    - apply: `uv pip install -r <app>/requirements.txt`.
  - `unit <service>`:
    - check: the unit file equals `render_unit`;
    - apply: `write_user_file` into `~/.config/systemd/user/`, create the log directory
      `~/.local/state/local-machine/hypostasis/`, then `systemctl --user daemon-reload`;
    - it never starts, stops or restarts a unit (FR-009). When the unit is active and its
      content changed, print `note: unit changed; run stop then start`.
- [X] T022 [US1] `firewall <service>` steps in `src/hypostasis/firewall.rs`, using
  `system::ufw` (research R8). They are placed right after the `authority` step and before
  `component-paths`, so every root step runs before the slow steps (analysis I1). The
  rpg-lib port comes from the authority; when the authority doesn't exist yet at the
  up-front check, return `NeedsChange` with the notice `allow rpg-lib's port (from
  <authority>)/tcp from <net>`. For each service port:
  - check: `sudo -n ufw status`:
    - sudo refused → `Check::Unknown`;
    - ufw inactive → `DoneWith("ufw inactive; services reachable from any network")`;
    - an ALLOW `<port>/tcp` rule from exactly `allowed_network` → done;
    - a rule with the comment `local-machine hypostasis <service>` from another network,
      or for a stale port → `NeedsChange` (config changed).
  - Notice: `allow port <p>/tcp from <net> (<service>)`.
  - Apply: delete our stale rules, then `sudo ufw allow from <net> to any port <p>
    proto tcp comment 'local-machine hypostasis <service>'`.
- [X] T023 [US1] Restore steps in `src/hypostasis/restore.rs` (research R7). Each returns
  `continue_on_failure() == true`.
  - Backup-folder errors name the folder; when it is unreachable the message says
    `mount <folder>, then run setup`.
  - `restore rpg-library`:
    - present: `rpg_library_db` exists;
    - apply: take the newest `rpg_library-*.db` in `backups.rpg_library` (or an undated
      `rpg_library.db`), copy it to `<db>.partial`, run `sqlite3 <partial> 'PRAGMA
      quick_check'` (must print `ok`), rename into place, and do the same for `user_data`
      when it is in the backup;
    - on any failure, remove `.partial`.
  - `restore flexai-social data` / `restore flexai-combat data` (**blocked on
    mytools#101**, the apps' `--check` flag; research R7):
    - present: `<svenv>/bin/python <mytools>/<app>/app.py --data-dir <data_dir> --check` exits 0;
    - apply: for each `missing: <file>` line, copy the file from the backup folder to
      `<data_dir>/<file>.partial`, then rename it into place (create `data_dir` if
      needed). Never touch an existing file. Then run `--check` again;
    - on failure (a file not in the backup, or `--check` still failing), remove only the
      files this restore added, and show `--check`'s lines;
    - an `unreadable:` file that already exists is reported and left alone (FR-010b).
  - `restore palace <campaign>`, one per authority campaign:
    - present: `<venv>/bin/mneme mp status <campaign>` reports a store;
    - apply: the newest snapshot directory under `backups.palaces/<campaign>/` (holding
      `.mneme-backup`), then `<venv>/bin/mneme mp restore <campaign> --from <dir>`. It runs
      only when the store is missing, so mneme never replaces a present store (FR-010b);
    - no snapshot → fail naming the folder.
- [X] T024 [US1] Implement `setup` in `src/hypostasis/setup.rs`.
  - Build one `action::Action`: the gate is the owner gate from T012, and the steps come in
    data-model order (root steps before the slow steps; analysis I1).
  - The runner asks for root once for `packages`, `linger` and the firewall steps.
    Software steps stop at the first failure; restores are tolerant (T008).
  - On failure, print `done:` and `remaining:` lists (FR-016). On success, print
    `next: local-machine hypostasis start` and `then: mneme up <campaign>` (FR-017).
  - Map errors to exit codes per contracts/cli.md.
  - Depends on T014–T023.
- [X] T025 [P] [US1] Write `tests/hypostasis_setup.rs`, fresh-machine scenarios on
  `FakeSystem`:
  - a fully missing machine:
    - the steps come in data-model order;
    - `prompted()` is true;
    - the notice lists only the packages, linger and firewall lines;
    - no root command runs before the notice;
    - every `sudo` call (apt, linger, ufw) comes before the first `uv venv` call
      (analysis I1);
    - every `sudo` call is `apt-get install`, `loginctl enable-linger`, `ufw` or the
      privilege notice's `sudo -v`/`sudo -n` checks, so no clone, install, copy or
      `chown` runs as root (FR-015; analysis G3);
    - no `systemctl --user start|enable|restart` is issued;
    - setup ends with the two `next:` lines and exit 0;
  - the authority seeded from `authority_source`, with byte-identical content;
  - an unreadable `authority_source` → exit 1 naming it, and venv untouched;
  - an uncovered component path → fails before `uv venv`;
  - `hypostasis install` failing → exit 1 with its stderr shown, and no restore attempted;
  - an unreachable backup folder → every restore fails and is named, all software pieces
    are done, exit 1;
  - a corrupt DB backup (`quick_check` not `ok`) → no `.partial` left and the target is
    absent;
  - a palace with no snapshot → named, and the other restores still run.
- [ ] T026 [US1] Real-machine validation (by the owner): quickstart.md V3 (this machine,
  already mostly set up) and V13 (fresh VM). Record the results in `tasks.md` →
  Real-Machine Validation Progress.

**Checkpoint**: MVP. `setup` brings a machine to ready.

---

## Phase 4: User Story 2 - Re-run setup safely on a machine that is already (partly) set up (Priority: P1)

**Goal**: Re-running changes only what is missing. It never touches existing checkouts or
data, and asks for no password when nothing needs root.

**Independent Test**: T027 scenarios; quickstart.md V4, V5, V6, V15.

- [X] T027 [P] [US2] Add re-run scenarios to `tests/hypostasis_setup.rs`:
  - fully set up → only `ok:` lines, `prompted()` false, and no mutating call recorded
    (SC-002);
  - an existing checkout with a different branch or a dirty tree → only read-only git
    commands were issued for it, with no `checkout`, `pull`, `reset`, `clean` or `fetch`
    (FR-012);
  - an occupied path (a file, a non-git directory, the wrong origin) → `error: checkout
    <p>: occupied (<found>); not touched`, exit 1, nothing written there (FR-013);
  - an interrupted run (a fake failure on the 5th step, then a second run) → the second run
    applies only the remaining steps (SC-004);
  - only a firewall rule missing → the notice lists only that rule;
  - existing data while the backup holds newer files → no copy and no `mneme mp restore`
    issued (FR-010b, SC-007);
  - an authority file that differs from its source → untouched;
  - services active during setup → no `systemctl --user stop|restart`.
- [X] T028 [US2] Fix whatever T027 exposes in `src/hypostasis/{pieces,checkout,restore}.rs`
  (expected: none, if T014–T023 follow the check rules). Depends on T027.
- [ ] T029 [US2] Real-machine validation (by the owner): quickstart.md V4, V4b, V5, V6, V6b, V15, V22.

**Checkpoint**: setup is safe to re-run on the owner's working machine.

---

## Phase 5: User Story 3 - Start and stop the local services (Priority: P1)

**Goal**: `start` brings up rpg-lib, flexai-social and flexai-combat as owner-run user
units that survive logout and reboot, reporting each one only once it answers. `stop`
takes them down and keeps them down. Neither action ever prompts.

**Independent Test**: T032 scenarios; quickstart.md V7, V8, V9, V10, V11a, V11b, V12.

- [X] T030 [US3] Implement `start` in `src/hypostasis/start.rs` per contracts/cli.md →
  start.
  - Readiness gate (FR-023): every service's install piece, unit piece and data piece must
    be present. For FlexAI the data piece is the app's `--check` (**blocked on
    mytools#101**). Otherwise exit 4 listing `not ready: <service> (<reason>)` before touching
    any unit.
  - Then, per service:
    - active and answering → `ok:`;
    - answering but unit inactive → foreign error;
    - port held (`port_holder`) → blocked error naming the holder;
    - otherwise `systemctl --user enable --now <unit>`, then poll `probe` every 250 ms for
      up to 30 s → `changed: <service> (running at <url>)`.
  - On timeout or a failed unit: `systemctl --user disable --now <unit>`, then
    `error: <service>: <cause>` (the cause comes from `systemctl --user is-failed` or
    "did not answer within 30 s"), then 20 log lines and the `log:` path (FR-029a).
  - Keep going with the other services, and exit 1 if any failed.
  - `<url>` uses the machine's address inside `allowed_network`. Move `connect::local_address`
    from `src/export/connect.rs` into a shared place so both subcommands can use it.
- [X] T031 [US3] Implement `stop` in `src/hypostasis/stop.rs` per contracts/cli.md → stop.
  Per service:
  - unit active → `systemctl --user disable --now`, then wait up to 15 s for `probe` to
    fail → `changed: <service> (stopped)`;
  - inactive and silent → `ok: <service> (stopped)`;
  - answering with the unit inactive → foreign error, exit 1.

  When the unit is inactive but still enabled, disable it so it stays down after a reboot
  (FR-026).
- [X] T032 [P] [US3] Write `tests/hypostasis_services.rs` on `FakeSystem`:
  - start with all services ready → three `enable --now` calls, each `changed` only after
    the scripted HTTP answer turns on;
  - not ready → exit 4, no `systemctl --user` calls;
  - one already running → `ok:`, and no enable for it;
  - foreign rpg-lib (answering, unit inactive) → error, the FlexAI pair still starts,
    exit 1;
  - port blocked → names the holder, and no enable for it;
  - timeout → `disable --now` issued, 20 log lines and the `log:` path printed, exit 1;
  - stop of running services → `disable --now`, `changed` only after silence;
  - stop when everything is stopped → `ok:` and no calls;
  - stop of a foreign service → error, and no kill or disable issued;
  - every case → `prompted()` false and no `sudo` call at all.
- [X] T033 [US3] Wire `start` and `stop` dispatch in `src/hypostasis/mod.rs`. Depends on
  T030, T031.
- [ ] T034 [US3] Real-machine validation (by the owner): quickstart.md V7 (stop the
  hand-run rpg-lib first), V8, V9, V10, V11a, V11b, V12, V19, V20.

**Checkpoint**: `mneme up` passes its rpg-lib gate, and both FlexAI tools open from
Windows.

---

## Phase 6: User Story 4 - See what is in place, what is missing, and what is running (Priority: P2)

**Goal**: `status` reports every setup piece and every service as observed. It never
changes anything or prompts, names the fix for each problem, and points to `hypostasis
status` for everything else.

**Independent Test**: T036 scenarios; quickstart.md V2, V17.

- [X] T035 [US4] Implement `status` in `src/hypostasis/status.rs` per contracts/cli.md →
  status.
  - Run each piece's `check()` through `NoPrompt`. Make `NoPrompt` `pub(crate)` in
    `action.rs` so status can reuse it.
  - Print `ok:`, `missing: <name> (<reason>) — run 'local-machine hypostasis setup'`,
    `unknown: <name> (run 'sudo -v' first to check)`, or `note:`.
  - Then, per service: `running:`, `stopped:`, `failed:`, `foreign:` or `blocked:`, with
    its URL and the fix. Show a 20-line log tail when the unit is failed, or enabled but
    not answering.
  - For an unreachable backup folder whose restore is missing, the fix reads
    `mount <folder>, then run …`.
  - The headline is `hypostasis: <not set up|set up>; <n> of 3 services running`. The last
    line is `see also: hypostasis status   (component pins, rendered configs, external
    services)`.
  - Exit codes: 0 all good; 4 any piece missing; otherwise 5 if any service is not running.
    Unknowns alone don't fail.
  - A piece whose inputs don't exist yet (e.g. no authority file) reports `missing` with
    the reason "needs <earlier piece>".
- [X] T036 [P] [US4] Write `tests/hypostasis_status.rs`:
  - fully set up and running → exit 0 and three `running:` lines;
  - one checkout missing → `missing: checkout <p>`, exit 4, no mutating call;
  - a service stopped → exit 5 and `run 'local-machine hypostasis start'`;
  - sudo not cached → firewall `unknown:`, exit unaffected, `prompted()` false;
  - a failed unit → log tail printed;
  - a foreign service → `foreign:`;
  - the `see also:` line is always printed;
  - root → exit 3.
- [ ] T037 [US4] Wire `status` dispatch in `src/hypostasis/mod.rs`, and run quickstart.md
  V2 and V17 on the real machine (by the owner). *(Dispatch wired; V2 smoke-tested with a
  scratch config dir, see below. V17 is the owner's.)*

**Checkpoint**: all four actions work.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T038 [P] Add a `local-machine hypostasis` section to `README.md` (if present) or to
  `CLAUDE.md`'s command list. It should cover the actions, the two config files, the
  `RPG_LIB_BACKUP_DIR` note from research R7, and that start/stop never prompt.
- [X] T039 [P] Put the example config from contracts/hypostasis-config.md into
  `specs/002-hypostasis-setup/` as `hypostasis.example.yaml`, with this machine's real
  values, so it can be copied for quickstart step 2.
- [X] T040 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test`: all clean and passing.
- [X] T041 Owner migration on Linux-Alien (quickstart step 1). Capture `local-machine
  export status` with the old binary, install the new one with `cargo install --path .`,
  create `machine.yaml`, move `allowed_network` (and any `owner`) out of `export.yaml`,
  then check that `export status` is unchanged (V1).
  Done 2026-11-01: `export.yaml` had only `allowed_network` and is now empty (all
  defaults); `export status` identical before and after (`export: available`, exit 0).
  V1 passes. The binary installed then was built from a61ce19, before 003's fixes.
- [ ] T042 Full quickstart V1–V23 pass on the real machine (V13, V14, V16 and V18 need a
  fresh VM). Record the results below, as feature 001 did.

---

## Dependencies & Execution Order

- **Phase 1** (T001–T002) → **Phase 2** (T003–T013) → user stories.
- Phase 2 internal order:
  - T003 → T004 → T005 (T005 also needs T008);
  - T006 can run any time;
  - T007, T008, T009, T010, T011 in parallel;
  - T012 needs T003 and T010;
  - T013 needs T007, T010 and T011.
- **External**: mytools#101 (FlexAI `--check`) must be merged before the FlexAI part of
  T023, and before T030.
- **US1** (T014–T026) is the MVP and is needed by US2, US3 and US4: US2 tests US1's steps,
  US3's readiness gate reuses US1's piece checks, and US4 reports US1's pieces.
- **US2** (T027–T029) needs US1.
- **US3** (T030–T034) needs T013 and US1's install, unit and data pieces. It can be built
  in parallel with US2.
- **US4** (T035–T037) needs US1's pieces and T013. It can be built in parallel with US2 and
  US3.
- **Polish** (T038–T042) comes after the stories you are shipping.

### Parallel examples

```text
Phase 2:  T007 system ext  |  T008 runner tolerance  |  T009 ufw module  |  T010 config  |  T011 authority
US1:      T014 packages  |  T015 uv  |  T016 linger        (then T017 → T018 → T022 → T019 → T020 → T021 → T023 → T024)
After US1: US2 (T027)  |  US3 (T030–T032)  |  US4 (T035–T036)
```

## Implementation Strategy

1. **MVP**: Phases 1–3. `setup` works on a fresh VM and on this machine (V3, V13).
2. **Safety**: Phase 4. Re-runs are proven harmless (V4–V6, V15). Only after this does
   setup run routinely on Linux-Alien.
3. **Services**: Phase 5. This retires the hand-run rpg-lib (V7–V12).
4. **Status**: Phase 6, then Polish and the full validation pass.

Commit after each task or logical group, on `002-hypostasis-setup` only. `main` changes
only by merging this branch (constitution → Source Control).

## Real-Machine Validation Progress

- **2026-11-01, V2 smoke test (by Claude, read-only)**: `local-machine hypostasis status
  --config-dir <scratch>` on Linux-Alien, with `machine.yaml` and a copy of
  `hypostasis.example.yaml` in a scratch directory (the two TODO(owner) values filled with
  the authority file itself and the DriveThru FlexAI folder). `~/.config/local-machine` was
  not touched. Result: no prompt, no change; firewall rows `unknown:`; exit 4. Reported
  missing: `packages` (python3-dev python3-venv pipx), `services-venv`, the three
  installs and units, both FlexAI data pieces (no services venv yet). rpg-lib reported
  `foreign` (the hand-run copy, pid 1098703). Took about 60 s, almost all of it four
  `mneme mp status` calls.
- **Found and fixed by that run**: palace campaign names. `data_roots.campaigns` entries
  are search roots; the campaign is the folder inside holding `.mneme`
  (`~/phandalin/Phandalin` → `Phandalin`). The first version used the root's basename and
  reported `restore palace phandalin` as missing.
