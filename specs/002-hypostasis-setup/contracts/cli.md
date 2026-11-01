# CLI Contract: `local-machine hypostasis`

## Grammar

```text
local-machine [--config-dir DIR] hypostasis <action>

<action> := setup | start | status | stop
```

- `--config-dir` is the shared parameter (Principle II/III). Reads `DIR/machine.yaml` and
  `DIR/hypostasis.yaml`, plus hypostasis's authority file at the path `hypostasis.yaml`
  names. No other parameters.
- `hypostasis` with no action prints help listing the four actions, exit 2.

## Common behaviour

- Output on stdout, one line per piece or service: `ok: <name>`, `ok: <name> (<note>)`,
  `adding: <name>` (printed before a piece is put in place), then `changed: <name>` or
  `error: <name>: <reason>`. Status adds `missing:`, `unknown:`,
  `note:`. Errors on stderr are prefixed `error:`.
- Every action first validates config, the owner (invoking user = `machine.yaml` owner, not
  root), and readability of what it needs. Failure → exit 3 naming the file/key, before any
  change and without a password prompt.
- Root work only in `setup`, through the shared runner (Principle VI): one notice listing
  exactly the root steps with work to do (`install packages: sqlite3 python3-venv`,
  `enable linger for kostadis`, `allow port 8000/tcp from 192.168.1.0/24 (rpg-lib)`), then
  at most one password prompt. Nothing to do → no notice, no prompt.
- `start`, `stop`, `status` never need root and never prompt.

## Exit codes (same table as `export`)

| Code | Meaning |
|---|---|
| 0 | Success (status: every piece present and every service running) |
| 1 | Action failed (a piece or service failed; privilege refused) |
| 2 | Usage error |
| 3 | Not configured (config/authority missing or invalid, run as root, wrong user) |
| 4 | Not set up (start: a service not ready; status: a piece missing) |
| 5 | status only: set up, but a service is not running under this tool's control |

## `hypostasis setup`

- Puts each missing piece in place in data-model order; reports each `ok:`/`changed:`.
- Software pieces: stop at the first failure — `error: <piece>: <reason>`, then
  `done: <n> pieces` / `remaining: <names>`, exit 1 (FR-016).
- Occupied checkout path: `error: checkout <path>: occupied (<found>); not touched`, exit 1.
- Restores: each missing one is attempted; failures listed at the end, exit 1.
- Never starts, stops or restarts a service (FR-009); rewriting a unit file of a running
  service does not restart it (status shows `note: unit changed; run stop then start`).
- On success ends with:

  ```text
  next: local-machine hypostasis start
  then: mneme up <campaign>
  ```

## `hypostasis start`

- Readiness gate (FR-023): if any service is not ready → `error: not set up — run
  'local-machine hypostasis setup'` plus `not ready: <service> (<reason>)` lines; starts
  nothing; exit 4.
- Per service: already running → `ok: <service> (running at <url>)`; foreign →
  `error: <service>: answering at <url> but not started by local-machine; not starting a
  second copy`; port blocked → `error: <service>: port <p> held by <holder>`; else enable
  + start, wait for answer → `changed: <service> (running at <url>)`.
- Failure (timeout, crash): unit disabled again, `error: <service>: <cause>`, then the last
  20 log lines indented and `log: <path>`. Other services still start. Exit 1 if any failed.
- `<url>` is `http://<LAN address>:<port>` (the address on `allowed_network`; falls back to
  `127.0.0.1` when none).

## `hypostasis stop`

- Per service: running under control → disable + stop, wait until silent →
  `changed: <service> (stopped)`; already stopped → `ok: <service> (stopped)`; foreign →
  `error: <service>: answering but not started by local-machine; not stopping it` (exit 1).

## `hypostasis status`

```text
hypostasis: set up; 2 of 3 services running
ok: packages
ok: uv
ok: linger
ok: checkout /home/kostadis/src/mneme
...
ok: firewall rpg-lib
ok: restore palace obelisk
running: rpg-lib        http://192.168.1.85:8000
running: flexai-social  http://192.168.1.85:5105
stopped: flexai-combat  http://192.168.1.85:5106 — run 'local-machine hypostasis start'
  | <last 20 log lines, only for failed/stopped-after-crash services>
  log: /home/kostadis/.local/state/local-machine/hypostasis/flexai-combat.log
see also: hypostasis status   (component pins, rendered configs, external services)
```

- Missing piece → `missing: <name> (<reason>) — run 'local-machine hypostasis setup'`,
  exit 4. Root-only check without cached sudo → `unknown: firewall <service> (run
  'sudo -v' first to check)`; unknowns alone don't fail status.
- A needed backup folder unreachable while its restore is missing → the fix reads
  `mount <folder>, then run 'local-machine hypostasis setup'`.
- Service not running → exit 5 (if no piece missing). Log tail shown when the unit is in
  `failed` state or the service is enabled but not answering (FR-029a).
