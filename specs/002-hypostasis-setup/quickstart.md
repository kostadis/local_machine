# Quickstart & Validation: `local-machine hypostasis`

Run as the owner (not root). Contracts: [cli](contracts/cli.md),
[hypostasis.yaml](contracts/hypostasis-config.md), [machine.yaml](contracts/machine-config.md).
Automated tests (`cargo test`) cover logic against `FakeSystem`; the checks below are the
real-machine validation.

## 0. Prerequisites

- Ubuntu with sudo; `local-machine` built and on `PATH` (`cargo install --path .`).
- Git credentials for `github.com/kostadis/*` (SSH key or credential helper).
- The Google Drive mount up, with backups under `/mnt/g/My Drive/hypostasis/backups/`
  (`rpg-lib/`, `flexai-social/`, `flexai-combat/`, `palaces/`), or overrides in config.
- `RPG_LIB_BACKUP_DIR` in your shell profile names the same folder as
  `<backup_root>/rpg-lib` (rpg-lib's backup scripts write there; setup reads it).

## 1. Migrate the shared config (this machine, once)

```bash
printf 'allowed_network: 192.168.1.0/24\n' > ~/.config/local-machine/machine.yaml
sed -i '/^allowed_network:/d' ~/.config/local-machine/export.yaml
local-machine export status          # V1: unchanged result (exit 0 while the share is on)
```

## 2. Write `hypostasis.yaml`

Copy the example from the config contract and adjust paths. Then:

```bash
local-machine hypostasis status      # V2: lists pieces; no password prompt; exit 0/4/5
```

## Validation scenarios

| # | Scenario | Steps | Expected |
|---|---|---|---|
| V1 | Export after migration | step 1 | `export status` result unchanged |
| V2 | Status never prompts | `sudo -k; local-machine hypostasis status` | no prompt; firewall rows `unknown:` |
| V3 | Already set up (this machine) | `hypostasis setup` | only real gaps (e.g. `packages`: sqlite3, python3-venv; `services-venv`; units; firewall) are changed; one notice naming only root work; checkouts all `ok:`, untouched |
| V4 | Idempotent re-run | `hypostasis setup` again | every line `ok:`, no prompt (SC-002) |
| V5 | Checkouts untouched | make an uncommitted change + local branch in `~/src/mytools`; `setup` | `git status`/`git branch` identical before and after (SC-003) |
| V6 | Occupied path | `mkdir /tmp/x`; add checkout `path: /tmp/x`; `setup` | `error: checkout /tmp/x: occupied (not a git repository)`; `/tmp/x` unchanged; exit 1 |
| V7 | Hand-started rpg-lib (today's state) | with the hand-run `library_server.py` up: `status`, `start` | status: `foreign`; start refuses rpg-lib, starts the FlexAI pair; exit 1. Then kill the hand copy and `start` again: all three `changed:` |
| V8 | Start/stop | `start`; browse `http://192.168.1.85:5105`, `:5106` from Windows; `mneme up <campaign> --dry-run`; `stop` | all answer; rpg-lib gate passes; after stop all three silent |
| V9 | Reboot survival | `start`; `sudo reboot`; after boot, without logging in to a desktop: `curl` each port from another LAN host | all three answer (SC-009) |
| V10 | Stop survives reboot | `stop`; reboot | none answers |
| V11a | Port blocked | with services stopped: `python3 -m http.server 5106 &`; `start`; then kill the http.server | combat: `error: … port 5106 held by python3`; rpg-lib and social start; exit 1 |
| V11b | Crash + log tail | with services stopped: `chmod 000` the rpg-lib DB (it still exists, so the readiness gate passes); `start`; then `chmod 644` it back | rpg-lib: unit disabled again, `error: rpg-lib: …`, its last 20 log lines and the `log:` path; the FlexAI pair start; exit 1 |
| V12 | Firewall scope | from a LAN host: `curl :8000`; check `sudo ufw status` | ALLOW 8000,5105,5106/tcp from 192.168.1.0/24 only |
| V13 | Fresh machine (SC-001) | new Ubuntu VM: install `local-machine`, write both configs, `setup`, `start`, `mneme up <campaign>` | one password prompt; no manual step; FlexAI opens in a browser |
| V14 | Backup unmounted | on the fresh VM, `setup` with the Google Drive mount down | software pieces done; each restore `error:` naming the backup; exit 1. Mount, re-run: only restores run (SC-004) |
| V15 | Data never overwritten | on a set-up machine, put a newer DB in the backup; `setup` | `ok: restore rpg-library`; DB checksum unchanged (SC-007) |
| V16 | Palace search parity | on the fresh VM, run the same `mneme` search as on Linux-Alien | identical results, no re-embedding (SC-006) |
| V17 | Root refused | `sudo local-machine hypostasis status` | exit 3, "must not run as root" |
| V18 | Second machine, different paths | V13 with different checkout paths | works with config change only (SC-008) |
