# Data Model: `local-machine hypostasis`

Everything here is either configuration (read) or observed machine state (derived on every
run). The tool keeps no state file of its own. See [research.md](research.md) for why.

## Configuration entities

### MachineConfig — `<config-dir>/machine.yaml` (shared, new)

The first shared config file (constitution Principle II). Read by `export` and
`hypostasis`. Contract: [contracts/machine-config.md](contracts/machine-config.md).

| Field | Type | Rule |
|---|---|---|
| `allowed_network` | CIDR | required; truncated to its network (`192.168.1.7/24` → `192.168.1.0/24`) |
| `owner` | user name | optional; default the invoking user; MUST NOT be `root`; MUST exist (`getent passwd`) |

### HypostasisConfig — `<config-dir>/hypostasis.yaml`

Contract: [contracts/hypostasis-config.md](contracts/hypostasis-config.md).

| Field | Type | Rule |
|---|---|---|
| `authority` | path | default `~/.config/hypostasis/hypostasis.yaml`; where hypostasis reads its authority |
| `authority_source` | path | required; owner-kept seed copy; read-only to us |
| `backup_root` | path | required; e.g. `/mnt/g/My Drive/hypostasis/backups`; read-only to us |
| `backups.rpg_library` | path | optional; default `<backup_root>/rpg-lib` |
| `backups.flexai_social` | path | optional; default `<backup_root>/flexai-social` |
| `backups.flexai_combat` | path | optional; default `<backup_root>/flexai-combat` |
| `backups.palaces` | path | optional; default `<backup_root>/palaces` (mneme snapshot layout) |
| `prerequisites.apt` | list of package names | optional; default list in research R5 |
| `checkouts` | list of Checkout | required, ≥1 |
| `hypostasis_source` | path | required; must equal a checkout `path`; installed editable into the authority's venv (analysis U1) |
| `services.venv` | path | required; the local services' venv |
| `services.mytools` | path | required; must equal a checkout `path` |
| `flexai_social` | FlexAIService | required |
| `flexai_combat` | FlexAIService | required |

`~` expands to the owner's home. All paths absolute after expansion. Unknown keys are
errors. `allowed_network` or `owner` here is an error naming `machine.yaml`.

### Checkout (also used for data trees)

| Field | Type | Rule |
|---|---|---|
| `path` | path | unique across checkouts; not nested inside another checkout |
| `repo` | git URL | required |
| `branch` | string | required; used only at creation |
| `sparse` | list of dir names | optional; non-empty when present; cone-mode directories |

### FlexAIService

| Field | Type | Rule |
|---|---|---|
| `port` | 1–65535 | required; distinct from every other service port |
| `data_dir` | path | required; where the workbooks live |
| `env` | map string→string | optional; `flexai_social` only (`CLEF_URL`, `CHAT_URL`, `CLEF_MODEL`, `CHAT_MODEL`) |

The app path is fixed relative to mytools (`<mytools>/flexai-social/app.py`,
`<mytools>/flexai-combat/app.py`), as the repo layout is not machine-specific.

### Authority (read-only view of hypostasis's file)

Only these keys are read; any other content is ignored and never written.

| Field | Source key | Required by |
|---|---|---|
| `venv` | `venv` | setup, status |
| `installer` | `installer` | setup (only `uv` supported; anything else → refuse naming it) |
| `rpg_lib_port`, `rpg_lib_health_path` | `services.rpg_lib.port`, `.health.path` (default `/`) | start/stop/status, unit, firewall |
| `rpg_lib_url` | `services.rpg_lib.url` | consistency check only (analysis I3): host must be this machine (`localhost`, `127.0.0.1`, `::1`, the hostname, or one of its own addresses); the URL's port must equal `port`. Probes always use `127.0.0.1:<port>` |
| `rpg_library_db` | `data_roots.rpg_library_db` | restore, readiness |
| `campaigns` | `data_roots.campaigns` | palace restores |
| `component_paths` | `components.*.source.path` | checkout cross-check |
| `component_names` | `order.install` | component-install presence |

## Derived entities (observed each run)

### SetupPiece

One row of setup/status output. `name` is stable and printed.

| Kind | Name pattern | Present when (research ref) | Needs root |
|---|---|---|---|
| Prerequisite packages | `packages` | every listed apt package installed (R5) | yes |
| uv | `uv` | `uv` resolvable (R5) | no |
| Linger | `linger` | `Linger=yes` (R6) | yes |
| Checkout | `checkout <path>` | `.git` + matching origin (R4) | no |
| Authority file | `authority` | file exists at `authority` (FR-007) | no |
| Firewall | `firewall <service>` | ufw ALLOW `<port>/tcp` from `allowed_network` (R8); inactive → note | yes |
| Component coverage | `component-paths` | every component path is a checkout (R4) | no (check-only) |
| Venv | `venv` | `<venv>/bin/python` runs (FR-006) | no |
| Hypostasis install | `hypostasis` | `<venv>/bin/hypostasis --help` exits 0 | no |
| Component install | `components` | each `order.install` name installed in venv (R1) | no |
| Services venv | `services-venv` | `<svenv>/bin/python` runs | no |
| Service install | `install <service>` | packages installed in svenv; rpg-lib: `frontend/dist/index.html` exists | no |
| Service unit | `unit <service>` | unit file content equals the rendered unit | no |
| Library DB | `restore rpg-library` | `rpg_library_db` exists (R7) | no |
| FlexAI data | `restore <service> data` | the app's `--check` exits 0 (R7; mytools#101) | no |
| Palace | `restore palace <campaign>` | mneme store present (R7) | no |

Setup applies the pieces in table order. The runner asks for the password once, up front,
for every root piece with work to do. All root pieces (`packages`, `linger`, `firewall`)
come before the slow ones (venv, installs, npm build, restores), so sudo's cached
credentials (15 minutes) are still valid when they run (analysis I1). Only the clones sit
between the prompt and the firewall pieces; that gap is an accepted risk (owner decision,
2026-11-01). The firewall pieces need rpg-lib's port from the authority file, which exists
by then. On a fresh machine their up-front check can't read it yet, so it returns
`NeedsChange` and the notice says `allow rpg-lib's port (from <authority>)/tcp from <net>`.

Piece state: `present` | `present-with-note` | `missing(reason)` | `occupied(found)` |
`config-changed(reason)` | `unknown` (root-only check, no cached sudo).

### LocalService

| Field | rpg-lib | flexai-social | flexai-combat |
|---|---|---|---|
| port | authority | config | config |
| health path | authority (`/`) | `/` | `/` |
| unit | `local-machine-rpg-lib.service` | `local-machine-flexai-social.service` | `local-machine-flexai-combat.service` |
| log | `~/.local/state/local-machine/hypostasis/<service>.log` | same pattern | same pattern |
| ready (FR-023) | install + unit + DB present | install + unit + `--check` passes | install + unit + `--check` passes |

Runtime state, derived from `systemctl --user is-active/is-enabled` and an HTTP probe on
`127.0.0.1:<port>`:

```text
                 start (unit enable --now, wait ≤30 s for answer)
   stopped ────────────────────────────────────────────────► running
   (unit inactive,                                           (unit active,
    port silent)  ◄────────────────────────────────────────   answering)
                 stop (unit disable --now, wait ≤15 s silent)

   start timeout / crash → failed (unit disabled --now again, last 20 log lines shown)
   answering but unit inactive → foreign (started by hand): start skips it with an error,
                                  stop reports it cannot stop it
   port held by a non-matching process → blocked: start doesn't start it, names the holder
```

Status per service: `running` (exit contribution 0) | `stopped` | `failed` | `foreign` |
`blocked` | `not ready` (→ setup, exit 4).

## Relationships

- `HypostasisConfig.services.mytools` ⊂ `checkouts[].path`.
- `HypostasisConfig.hypostasis_source` ⊂ `checkouts[].path`.
- `Authority.component_paths` ⊆ `checkouts[].path` (cross-check piece).
- `authority_source` is normally inside a checkout; it must be readable after checkouts.
- `MachineConfig.allowed_network` → every `firewall <service>` piece; also `export`.
- `MachineConfig.owner` → the invoking user must equal it.
