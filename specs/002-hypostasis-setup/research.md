# Research: `local-machine hypostasis`

All facts below were observed on Linux-Alien on 2026-11-01 unless stated. Each item:
Decision / Rationale / Alternatives considered.

## R1. What hypostasis already does (boundary)

- **Observed**: `hypostasis install` installs every component in `order.install` at its pin
  (or editable under `mode: dev`) into the authority's `venv`, using the authority's
  `installer` (`uv`), **and renders every component config**. `hypostasis apply` only
  re-renders. `hypostasis status` reports component drift, service reachability, and render
  drift, and exits 0 only when every row passes.
- **Decision**: Setup runs `<venv>/bin/hypostasis install --config <authority>` once, as the
  last software step, and treats a non-zero exit as a failed piece (FR-008), printing
  hypostasis's own output. Setup never runs `apply` (install already renders). Status runs no
  hypostasis command; it prints a pointer line to `hypostasis status` (FR-020).
- **"Component install done"** is observed, not assumed. Reading hypostasis's
  `status --json` would pull its pin/drift checks into ours (FR-020), so instead the
  component-install piece is present when every component in `order.install` is installed
  in the venv, i.e. `uv pip show --python <venv> <name>` succeeds for each. This is a
  presence check only; pins and drift stay hypostasis's.
- **Alternatives**: re-implementing install (rejected: FR-008, duplicate authority);
  calling `hypostasis status` from our status (rejected: FR-020).

## R2. Values whose home is hypostasis's authority file (FR-022)

Read from `~/.config/hypostasis/hypostasis.yaml` (path: hypostasis's default; see R11):

| Value | Authority key | Used by |
|---|---|---|
| runtime environment | `venv` | setup (create, install hypostasis), status |
| installer | `installer` | setup (`uv` → needs the `uv` prerequisite) |
| rpg-lib address/port | `services.rpg_lib.url`, `.port` | start/stop/status health check, unit, firewall |
| library database | `data_roots.rpg_library_db` | restore target, start readiness |
| palace root, campaigns | `data_roots.mempalace`, `data_roots.campaigns` | palace restore list |
| palace backups root | `data_roots.backups` (mneme's `backups_root`, default `~/.mneme/backups`) | see R7 |
| component checkout paths | `components.*.source.path` | cross-check only (R4) |

- **Decision (spec correction)**: the runtime-environment location is **not** a
  `hypostasis.yaml` (ours) key. FR-021 lists it, but the authority already holds `venv`,
  and FR-022's one-home rule wins. The spec's FR-021 list is corrected in the same branch.
- **Ordering consequence**: setup must seed the authority file (FR-007) before it can
  create the venv (FR-006). The authority's source file normally lives in a checkout (e.g. a
  private repo), so the order is: prerequisites → checkouts → authority file → venv →
  hypostasis → component install → services → firewall → restores.
- **Alternatives**: copy `venv` into our config (rejected: two homes, drift).

## R3. Config-sharing review (constitution Principle II)

Existing subcommand: `export` (`export.yaml`: `allowed_network`, `share_name`, `path`,
`owner`, `exclude`). New subcommand values compared:

| hypostasis value | export value | Same meaning? | Result |
|---|---|---|---|
| home network services are reachable from (FR-030) | `allowed_network` | **yes** — the trusted home LAN | **share** |
| owner account | `owner` (default: current user) | **yes** — the one human account | **share** |
| checkouts, backups, FlexAI addresses/data, env | — | no counterpart | stay in `hypostasis.yaml` |

- **Decision**: create the first shared file, **`<config-dir>/machine.yaml`**, with keys
  `allowed_network` (required) and `owner` (optional, default the invoking user, never
  root). Names are kept from `export.yaml` so nothing is renamed (Principle III spirit).
  `export` is refactored in this feature to read both from `machine.yaml`; `export.yaml`
  keeps `share_name`, `path`, `exclude`. A leftover `allowed_network`/`owner` in
  `export.yaml` is a config error that names `machine.yaml` as the new home (no silent
  precedence — one home). The constitution's Principle II is amended (PATCH) to record the
  shared file's name and schema, on this branch.
- **Migration on this machine**: move `allowed_network: 192.168.1.0/24` from
  `export.yaml` to `machine.yaml` (quickstart step).
- **Owner checkpoint**: the file name `machine.yaml` and the hard error on leftovers are
  plan decisions the owner confirms at plan review.
- **Alternatives**: `shared.yaml` / `common.yaml` (less descriptive: these are facts about
  *the machine*); keep duplicates (forbidden); fall back to `export.yaml` when
  `machine.yaml` is missing (two homes during a migration that has one user — rejected).

## R4. Checkouts and data trees

- **Observed**: all repos are `https://github.com/kostadis/<repo>` with these branches:
  mneme `feat/85-mcp-search` (a feature branch — the config names the initial branch, e.g.
  `main`), dgx-fun/turbovecdb/CampaignGenerator/mytools `main`, Mempalace and
  5e-tools-kostadis `kostadis-dev`, homebrew-private `master`. Campaign trees `~/phandalin`,
  `~/out-of-the-abyss`, `~/obelisk`, `~/toee` are separate clones of `kostadis/campaigns`,
  cone sparse-checkouts of `{<campaign dir>, gm-assistant, scripts, .claude}` (+`tools` for
  obelisk). Sparse config is per clone and not committed (MACHINE.md).
- **Decision**: one config list, `checkouts`, entries `{path, repo, branch, sparse?}`;
  data trees are the same shape (FR-010). Create = `git clone --branch <b> <repo> <path>`;
  sparse = `git clone --no-checkout --branch <b>`, `git sparse-checkout init --cone`,
  `git sparse-checkout set <dirs…>`, `git checkout <b>`. All as the owner, no root.
- **Present** = `<path>/.git` exists and `git -C <path> remote get-url origin` equals the
  configured repo after normalisation (trailing `.git` and `/` ignored; `https://github.com/x`
  ≡ `git@github.com:x`). Branch, dirtiness, HEAD and sparse set are never checked or
  changed (FR-012). **Occupied** = path exists and is not that (FR-013): refuse, naming path
  and what was found (`not a git repository`, `origin is <url>`, `a file`).
- A failed clone removes only the directory it created (it did not exist before).
- **Cross-check**: every `components.*.source.path` in the authority must be covered by a
  configured checkout. On a fresh machine the authority only exists after the checkouts and
  the seed step, so this is a check-only step placed right after the authority piece: when a
  path is uncovered it fails, naming the path, before the venv or any install is touched
  (otherwise `hypostasis install` would fail late).
- **Alternatives**: `git worktree` (rejected — MACHINE.md: separate clones by design);
  pulling existing checkouts (forbidden by FR-012).

## R5. Prerequisites

- **Observed**: present — `git`, `build-essential`, `ufw`, python 3.14, `uv` at
  `~/.local/bin/uv` (not from apt; Ubuntu 26.04 has no `uv` package), `node`/`npm` at
  `/usr/local/bin` (hand-installed). Missing — `sqlite3`, `python3-venv`, apt `nodejs`/`npm`.
- **Decision**: prerequisites are a configured list in two kinds:
  - `apt` packages (default list shipped as non-machine-specific defaults:
    `git build-essential python3-dev python3-venv sqlite3 nodejs npm ufw`; config may
    replace it). Present = `dpkg-query -W -f='${Status}'` says `install ok installed`.
    Installed together by one privileged step `apt-get install -y <missing…>` (runner,
    Principle VI; one notice, one prompt — SC-001).
  - `uv`: present = `uv` resolvable on the owner's `PATH` or at `~/.local/bin/uv`. Installed
    without root by `pipx install uv` (adds `pipx` to the apt list). No `curl | sh`.
- `node`/`npm` count as present when the commands resolve, so a hand-installed node (this
  machine) is not replaced by apt's.
- **Why node**: rpg-lib's web UI (`rpg-lib/frontend`) is not committed built; setup runs
  `npm ci && npm run build` there (R6). The 5etools MCP index (`mcp/index.js`) is
  committed and needs node at run time.
- **Alternatives**: astral's installer script (network script execution, rejected);
  snap (adds snapd dependency).

## R6. How the local services run (installed, configured, under control)

- **Observed**: rpg-lib runs today by hand: `~/.venv/bin/python ./library_server.py --db
  ./rpg_library.db --port 8000` (pid 1098703), host default `127.0.0.1`. flexai-social and
  flexai-combat are Flask apps (`app.py --data-dir D --host H --port P`), host default
  `127.0.0.1`, ports 5105/5106; combat has a hand `service.sh` (nohup + pid file). Linger is
  already enabled for `kostadis`.
- **Decision — runtime**: a separate **services venv** (config `services.venv`, e.g.
  `~/.venvs/services`), created with `uv venv`; rpg-lib installed with
  `uv pip install -e <mytools>/claudelib -e <mytools>/rpg-lib`, FlexAI with
  `uv pip install -r <app>/requirements.txt`. Not hypostasis's venv: the services are not
  hypostasis components, and mixing their deps (FastAPI, Flask, claudelib) into the
  component venv would make hypostasis's install/drift results depend on our installs.
- **Decision — control**: one **systemd user unit** per service,
  `~/.config/systemd/user/local-machine-<service>.service`, written by setup (no root),
  `Restart=on-failure`, `WantedBy=default.target`, `StandardOutput=append:<log>`,
  `StandardError=append:<log>`. `start` = `systemctl --user enable --now`; `stop` =
  `systemctl --user disable --now`. Reboot survival (FR-025) needs **linger**:
  `loginctl enable-linger <owner>`, a setup step that needs root, run through the runner
  (present = `loginctl show-user <owner> -p Linger` → `yes`). Services run as the owner
  (FR-029).
- Unit `ExecStart`s (all bind `0.0.0.0`, FR-030):
  - rpg-lib: `<svenv>/bin/python <mytools>/rpg-lib/library_server.py --db <rpg_library_db>
    --host 0.0.0.0 --port <authority port>`, `WorkingDirectory=<mytools>/rpg-lib`.
  - flexai-*: `<svenv>/bin/python <app>/app.py --data-dir <data_dir> --host 0.0.0.0
    --port <port>`; flexai-social also gets `Environment=` lines from config `env`
    (CLEF_URL, CHAT_URL, models) when set (R9).
- rpg-lib's web UI: setup runs `npm ci && npm run build` in `rpg-lib/frontend` when
  `frontend/dist/index.html` is missing (part of the rpg-lib install piece).
- **"Under this tool's control"** = the unit is active. **Answering** = HTTP GET on
  `127.0.0.1:<port><health path>` returns any HTTP status below 500 within 2 s (rpg-lib
  path `/` from the authority's `health.path`; FlexAI `/`). A service answering while its
  unit is inactive was started by hand (rpg-lib today): status says so; start refuses that
  one (no second copy), naming the holder; stop reports it cannot stop it (FR-028).
- **Port holder** = `ss -ltnpH sport = :<port>`; process names are visible for the owner's
  own processes without root, otherwise "another process".
- **Readiness for start** (FR-023): unit file present and matching, services venv present
  with the service's install, data present (DB file / data dir with the workbooks).
- **Wait for answer**: start polls every 250 ms up to 30 s per service; stop waits up to
  15 s for the port to stop answering. On timeout the unit is stopped again
  (`systemctl --user stop`, left enabled? no — `disable --now`) so nothing half-started
  claims to be up (US3-7), and the last 20 log lines are shown (FR-029a).
- **No root for start/stop**: user units need no sudo, so start/stop never prompt.
  Principle VI still holds (no root work, no notice).
- **Alternatives**: system units with `User=` (needs root for every start/stop — every start
  would prompt); nohup + pid files (no reboot survival); running from `~/.venv` (an
  unmanaged venv).

## R7. Backups and restores

- **Observed**: rpg-lib's `backup_db.sh` writes dated `rpg_library-YYYY-MM-DD.db` and
  `user_data-YYYY-MM-DD.db` via `sqlite3 .backup`. Palaces: `mneme mp backup <campaign>`
  writes snapshot dirs (marked `.mneme-backup`) under mneme's backups root
  `<root>/<campaign>/<stamp>`; `mneme mp restore <campaign> --from <dir>` **replaces** the
  store (moves an existing one aside), never re-embeds, prechecks the embedder (it probes
  the embedding endpoint), then verifies with mneme's conform rows. FlexAI: social needs a
  directory with **two** workbooks (`FlexAI_Social_*.xlsx`), combat one
  (`FlexAI_Combat_2021_01_04.xlsx`).
- **Decision — locations** (owner, plan review 2026-11-01): a required `backup_root` in
  config — on this machine the Google Drive mount, `/mnt/g/My Drive/hypostasis/backups`
  (machine-specific, so config, never a code default) — with one folder per kind of data
  under it by default, each overridable under `backups:`:

  ```text
  <backup_root>/rpg-lib/        rpg_library[-YYYY-MM-DD].db, user_data[-YYYY-MM-DD].db
  <backup_root>/flexai-social/  *.xlsx
  <backup_root>/flexai-combat/  *.xlsx
  <backup_root>/palaces/<campaign>/<stamp>/   (mneme snapshot dirs)
  ```

  Newest dated file wins; an undated file is used only when no dated one exists. Setup only
  reads these folders; the owning tools write them:
  - rpg-lib (since mytools#99 / PR #100): `backup_db.sh`, `setup_library_mcp.sh` and
    `refresh_library.sh` use `$RPG_LIB_BACKUP_DIR`, with no built-in default. The owner sets it
    in their shell profile to `<backup_root>/rpg-lib` (done on Linux-Alien, 2026-11-01:
    `/mnt/g/My Drive/hypostasis/backups/rpg-lib`). **Decision (owner, 2026-11-01)**:
    `local-machine hypostasis` does not set or check that variable. Only the hand-run backup
    scripts read it, not the rpg-lib service, and setup gets the folder from its own config.
    The quickstart notes that the two must name the same folder.
  - mneme: palace snapshots go to its backups root (authority `data_roots.backups`); the
    owner points it at `<backup_root>/palaces` or overrides `backups.palaces` in config.
- **Library DB**: present = target file exists. Restore = copy to `<target>.partial`, run
  `sqlite3 <partial> 'PRAGMA quick_check'` → `ok`, then rename into place (atomic, no partial
  at target on failure, FR-010c). `user_data.db` restored alongside when in the backup (the
  server creates an empty one otherwise).
- **FlexAI data** (analysis I2, owner decision 2026-11-01: ask the app; depends on
  mytools#101): each app gains `<svenv>/bin/python <mytools>/<app>/app.py --data-dir <data_dir> --check`. It loads the workbooks through the app's own
  startup path, exits 0 when usable, and otherwise exits non-zero with one line per problem:
  `missing: <file>` or `unreadable: <file>: <reason>`. The workbook names stay constants in
  the apps; `local-machine` never names them.
  - Present = `--check` exits 0.
  - Restore: for each `missing:` file, copy it from the backup folder to
    `<data_dir>/<file>.partial` and rename it into place, never touching files already
    there. Then run `--check` again (FR-010c). If it still fails, remove only the files this
    restore added, and fail the piece with `--check`'s lines. An `unreadable:` file that
    already exists is reported, never replaced (FR-010b).
  - `start`'s readiness gate uses the same check.
- **Palaces**: list = campaigns in the authority's `data_roots.campaigns`; present =
  mneme's store for that campaign exists (`mneme mp status <campaign>` reports a store).
  Restore = `<venv>/bin/mneme mp restore <campaign> --from <newest snapshot>` **only when
  the store is missing**, because mneme's restore replaces a present store (FR-010b would be
  violated otherwise). mneme verifies readability itself (FR-010c). A campaign with no
  snapshot in the backup is a failed restore piece, named.
- Restores run last (FR-010d) and each is independent: setup attempts all missing ones and
  then exits non-zero if any failed (FR-016 as clarified).
- Restored files are created by the owner, so ownership is the owner's (FR-015).
- **Risk**: mneme's embedder precheck needs the embedding endpoint reachable; when it is
  down, palace restores fail with mneme's message. Accepted: reported as that piece's
  failure, re-run later.
- **Alternatives**: copying `store.sqlite3` directly (forbidden by FR-010a); per-item backup
  only, with no default root (more config to write on every machine); a fixed tree with no
  overrides (forces copying where a tool already writes elsewhere).

## R8. Firewall (FR-030)

- **Decision**: one setup step per service port, `ufw allow from <allowed_network> to any
  port <port> proto tcp comment 'local-machine hypostasis <service>'`, through the runner
  (root). Present = `sudo -n ufw status` shows an ALLOW rule for that port from exactly
  `allowed_network`; a rule with that comment from another network = config changed →
  replaced (delete then add) like `export`'s Samba rule. ufw inactive → note
  (`ufw inactive; services reachable from any network`), not a missing piece. `sudo -n`
  failing → `unknown` in status (never prompts).
- rpg-lib's port comes from the authority; a changed port leaves a stale rule — status
  reports rules carrying our comment for ports no longer configured as `config changed`, and
  setup deletes them.
- **Alternatives**: binding only to the LAN address (breaks when DHCP changes; ufw already
  the pattern from `export`).

## R9. flexai-social's model endpoints (spec assumption, mytools#96)

- **Observed**: mytools#96 has landed: `situational.py` now reads `CLEF_URL`/`CHAT_URL`
  (required) and `CLEF_MODEL`/`CHAT_MODEL` from the environment and disables the feature
  when unset — no LAN defaults remain. These are a decision model and a chat model, **not**
  the authority's `machines.dgx.endpoint` (Gemma on a different port).
- **Decision**: they are different values, so no duplication: optional `env` map under
  `flexai_social` in `hypostasis.yaml`, passed as `Environment=` in its unit. Unset → the
  service still starts; the feature reports itself not configured (spec edge case).
- **Alternatives**: derive from the authority's dgx endpoint (wrong model/port).

## R10. Status, exit codes, output

- **Decision**: reuse `export`'s exit codes (`ExitError`): 0 ok, 1 failed, 2 usage, 3 not
  configured, 4 not set up (a setup piece missing), 5 set up but a service not answering /
  not under control. Lines `ok:` / `changed:` / `error:` / `missing:` / `unknown:` /
  `note:` as in `export`. Status `unknown` for root-only checks (ufw) without cached sudo.
- Status also probes each needed backup folder's reachability only to word the fix
  (`mount <folder>, then run setup`) — never a missing piece by itself.

## R11. Name confusion (spec assumption)

- Ours: `~/.config/local-machine/hypostasis.yaml` (key `authority:` points at theirs).
  Theirs: `~/.config/hypostasis/hypostasis.yaml`. Every message names the full path, and our
  file's header comment says it is not hypostasis's authority file.
- **Decision**: keep the names (constitution: `<subcommand>.yaml`); the authority path is a
  config value with hypostasis's default, `~/.config/hypostasis/hypostasis.yaml`.

## R12. Root / owner

- Every action refuses when run as root (euid 0) or when the invoking user differs from the
  shared `owner` (exit 3, naming the reason). Checkouts, venvs, units, logs and restores are
  created by the invoking (owner) process, so they are owner-owned (FR-015) with no chown.

## R13. Privileged steps in this feature (Principle VI)

Only three things need root, all in `setup`: `apt-get install` (missing packages),
`loginctl enable-linger`, and the ufw rules. They are steps of one `action::run` call with a
gate (config valid, not root, invoking user is the owner) so a fully set-up machine never
prompts and the password is asked at most once (SC-001). Every other setup piece is an
owner-level step. All three root steps are ordered before the slow owner-level steps
(venv, `hypostasis install`, npm build, restores), so they run while sudo's cached
credentials are still valid. The firewall steps go right after the `authority` step, the
first point at which rpg-lib's port is known (analysis I1, owner decision 2026-11-01). Only
the clones run between the prompt and the firewall steps; if they ever take longer than
sudo's 15-minute cache, sudo would prompt again — accepted. `start`, `stop`, `status` need
no root. Tests assert order with `FakeSystem::prompted()`.

- **Runner fit**: the existing runner asks once for all root steps, then applies steps in
  order. Owner-level steps implement the same `Step` trait with `notice() -> None`, so they
  appear in the same ordered list and the notice lists only root work. Restores need
  "try all, then fail" (FR-016): the runner gets an opt-in per-step `continue_on_failure`
  (default false); failures are collected and the action fails at the end without rolling
  back independent successes. This is a runner change made in this feature, with unit tests.
- **Runner facts that shape setup** (`src/system/action.rs`):
  - All step checks run once up front (to build the notice), then again just before each
    apply. A check for a later piece whose inputs don't exist yet (e.g. the venv, before
    the authority file is seeded) MUST return `NeedsChange`, not an error.
  - On a failure the runner rolls back already-applied steps. Setup's steps keep the
    default no-op `rollback`, so completed pieces (a clone, a venv) survive a later failure
    and a re-run picks up where it stopped (SC-004). A step cleans up only its own partial
    work inside `apply` (a half-made clone, a `.partial` file).
