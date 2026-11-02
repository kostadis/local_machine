# Quickstart & Validation: `local-machine hypostasis`

Run as the owner (not root). Contracts: [cli](contracts/cli.md),
[hypostasis.yaml](contracts/hypostasis-config.md), [machine.yaml](contracts/machine-config.md).
Automated tests (`cargo test`) cover logic against `FakeSystem`; the checks below are the
real-machine validation. "This machine" is Linux-Alien, which already runs the services by
hand; any other machine is a fresh machine (V13).

## 0. Prerequisites

- Ubuntu with sudo, and `build-essential` (the Rust linker needs `cc`).
- Rust, through rustup:
  `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`, then
  `. "$HOME/.cargo/env"` (new shells pick it up from `~/.profile`).
- Git credentials for `https://github.com/kostadis/*`. The config uses `https://` URLs, so
  an SSH key alone is not enough: use a credential helper (`gh auth setup-git`), or
  `git config --global url."git@github.com:".insteadOf "https://github.com/"`. Otherwise
  each private clone asks for a username and password, and V13's "one prompt" fails.
- The Google Drive mount up, with backups under `/mnt/g/My Drive/hypostasis/backups/`
  (`rpg-lib/`, `flexai-social/`, `flexai-combat/`, `palaces/`), or overrides in config.
- `RPG_LIB_BACKUP_DIR` in your shell profile names the same folder as
  `<backup_root>/rpg-lib` (rpg-lib's backup scripts write there).
- ufw active (`sudo ufw status` → `Status: active`) for V12. With ufw off, setup adds no
  rules and reports `ok: firewall … (ufw inactive; …)`.

## 1. Migrate the shared config (this machine, once)

Capture the baseline with the **old** binary, before installing the new one: once the new
one is on `PATH`, `export` refuses the old `export.yaml` (exit 3, no fallback).

```bash
local-machine export status > ~/export-status.before; echo "exit $?" >> ~/export-status.before
cargo install --path .               # the new binary, into ~/.cargo/bin
printf 'allowed_network: 192.168.1.0/24\n' > ~/.config/local-machine/machine.yaml
sed -i '/^allowed_network:/d; /^owner:/d' ~/.config/local-machine/export.yaml
# if export.yaml had an owner: line, add it to machine.yaml (otherwise the invoking user)
local-machine export status > ~/export-status.after; echo "exit $?" >> ~/export-status.after
diff ~/export-status.before ~/export-status.after    # V1: no difference
```

## 2. Write `hypostasis.yaml`

```bash
cp specs/002-hypostasis-setup/hypostasis.example.yaml ~/.config/local-machine/hypostasis.yaml
```

Resolve both `TODO(owner)` lines first. Until then they are not absolute paths, and every
action exits 3 naming the key:

- `authority_source`: commit a seed copy of `~/.config/hypostasis/hypostasis.yaml` to a
  checkout listed under `checkouts`, and name it here.
- `flexai_social.data_dir`, `flexai_combat.data_dir`: local folders, with copies of the
  workbooks in `<backup_root>/flexai-social` and `-combat` so setup can restore them.

Then:

```bash
local-machine hypostasis status      # V2: lists pieces; no password prompt; exit 0/4/5
```

## Validation scenarios

`<url>` below is what `start`/`status` print (`http://<address on allowed_network>:<port>`).

| # | Scenario | Steps | Expected |
|---|---|---|---|
| V1 | Export after migration | step 1 | `diff` prints nothing |
| V2 | Status never prompts | `sudo -k; local-machine hypostasis status` | no prompt; with ufw active, firewall rows `unknown: firewall <service> (run 'sudo -v' first to check)` |
| V3 | Already set up (this machine) | `hypostasis setup` | only real gaps (e.g. `packages`: sqlite3, python3-venv; `services-venv`; units; firewall) are changed; one notice naming only root work; checkouts all `ok:`, untouched |
| V4 | Idempotent re-run | `sudo -v; hypostasis setup` | every line `ok:`, no prompt (SC-002) |
| V4b | Re-run, sudo not cached | `sudo -k; hypostasis setup` | ufw active: one notice whose only lines are `check ufw; allow port <p>/tcp …` (one per service), one prompt, then every line `ok:`. ufw off: no notice, no prompt (SC-002) |
| V5 | Checkouts untouched | after V4: make an uncommitted change + local branch in `~/src/mytools`; `setup` | `git status`/`git branch` identical before and after (SC-003) |
| V6 | Occupied path | after V4: `mkdir /tmp/x`; add checkout `path: /tmp/x`; `setup` | stderr `error: checkout /tmp/x: occupied (not a git repository); not touched`; `/tmp/x` unchanged; exit 1 |
| V6b | Occupied path refused before asking | keep `/tmp/x`; `sudo apt remove sqlite3; sudo -k; setup` | the same error; **no** notice, **no** prompt; sqlite3 still not installed; exit 1 (FR-014) |
| V7 | Hand-started rpg-lib (today's state) | with the hand-run `library_server.py` up: `status`, `start` | status: `foreign: rpg-lib`; start refuses rpg-lib, `changed:` for the FlexAI pair; exit 1. Then kill the hand copy and `start` again: `changed: rpg-lib`, `ok: flexai-social (running at …)`, `ok: flexai-combat (running at …)`; exit 0 |
| V8 | Start/stop | `start`; from Windows browse each `<url>` it printed for 5105 and 5106; `mneme up <campaign> --dry-run`; `stop` | all answer; rpg-lib gate passes; after stop all three silent |
| V9 | Reboot survival | `start`; `sudo reboot`; after boot, without logging in to a desktop: `curl` each `<url>` from another LAN host | all three answer (SC-009) |
| V10 | Stop survives reboot | `stop`; reboot | none answers |
| V11a | Port blocked | with services stopped: `python3 -c 'import socket,time; s=socket.socket(); s.bind(("",5106)); s.listen(); time.sleep(1e6)' &`; `start`; then `kill %1` | combat: `error: flexai-combat: port 5106 held by python3`; rpg-lib and social start; exit 1. (The listener must not answer HTTP: `python3 -m http.server` answers, so it reads as `foreign`, not blocked.) |
| V11b | Crash + log tail | with services stopped: `chmod 000` the rpg-lib DB (it still exists, so the readiness gate passes); `start`; then `chmod 644` it back | rpg-lib: unit disabled again, `error: rpg-lib: …`, its last 20 log lines and the `log:` path; the FlexAI pair start; exit 1. First confirm `library_server.py` exits when its DB is unreadable; if it serves pages anyway, this scenario does not crash it and needs another fault |
| V12 | Firewall scope | ufw active; from a LAN host: `curl <url>` for 8000; from a host outside 192.168.1.0/24 (e.g. a phone off Wi-Fi via a tunnel, or another subnet): the same; `sudo ufw status` | LAN answers, outside does not; one ALLOW rule each for 8000/tcp, 5105/tcp, 5106/tcp from 192.168.1.0/24 and no other source |
| V13 | Fresh machine (SC-001) | new Ubuntu VM: prerequisites, `cargo install --path .`, write both configs; `start` (before setup); `setup`, `start`, `mneme up <campaign>` | the first `start`: `error: not set up …` with `not ready:` lines, exit 4. Then one password prompt; no manual step; FlexAI opens in a browser |
| V14 | Backup unmounted | on the fresh VM, `setup` with the Google Drive mount down | software pieces done; each restore `error:` naming the backup; exit 1. Mount, re-run: only restores run (SC-004) |
| V15 | Data never overwritten | on a set-up machine: `sha256sum` the DB; put a newer DB in the backup; `setup`; `sha256sum` again | `ok: restore rpg-library`; checksums equal (SC-007) |
| V16 | Palace search parity | on the fresh VM, run the same `mneme` search as on Linux-Alien | identical results, no re-embedding (SC-006) |
| V17 | Root refused | `sudo "$(command -v local-machine)" hypostasis status` (plain `sudo local-machine` is "command not found": sudo's PATH lacks `~/.cargo/bin`) | exit 3, `hypostasis: must not run as root; run it as the owner` |
| V18 | Second machine, different paths | V13 with different checkout paths | works with config change only (SC-008) |
| V19 | Unit change never restarts | services running: note each PID (`systemctl --user show -p MainPID local-machine-flexai-social`); add or change an `env:` value under `flexai_social`; `setup`; `status` | setup: `changed: unit flexai-social` and `note: flexai-social: unit changed; run stop then start`; PIDs unchanged (FR-009). Then `stop`, `start`: the new value is live |
| V20 | Stop leaves a foreign copy alone | services stopped; hand-start `library_server.py`; `stop` | `error: rpg-lib: answering but not started by local-machine; not stopping it`; the hand copy still answers; exit 1 |
| V21 | Wrong user | `machine.yaml` names `owner:` (without it the owner is whoever runs the tool); as another account: `local-machine --config-dir ~owner/.config/local-machine hypostasis status` | exit 3, `hypostasis: run as <owner> (the owner in …/machine.yaml), not <user>` |
| V22 | Prompt refused | with root work pending (as V6b, without `/tmp/x`): `sudo -k; setup`; enter a wrong password three times (Ctrl-C also kills `local-machine`: exit 130) | `error: administrator privileges refused`; exit 1; nothing changed (`packages` still missing in `status`) |
| V23 | No action | `local-machine hypostasis` | help listing setup, start, status, stop; exit 2 |
