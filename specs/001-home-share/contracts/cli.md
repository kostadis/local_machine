# CLI Contract: `local-machine export`

> Updated 2026-11-01 by 002: shared values moved to `machine.yaml`.

## Grammar

```text
local-machine [--config-dir DIR] export <action>
local-machine export <action> [--config-dir DIR]

<action> := setup | start | status | stop
```

- `--config-dir DIR` is a global parameter shared by every subcommand (constitution
  Principle II/III). Default `~/.config/local-machine`. `export` reads `DIR/machine.yaml` (shared:
  `allowed_network`, `owner`) and `DIR/export.yaml` (optional: `share_name`, `path`,
  `exclude`). Exit 3 messages name the file: `no config at DIR/machine.yaml (need at least
  allowed_network)`, or `DIR/export.yaml: allowed_network: moved to DIR/machine.yaml`.
- `export` with no action prints help listing the four actions and exits 2.
- No other parameters. Any future parameter shared with another subcommand MUST reuse that
  subcommand's name.

## Common behaviour

- Output: human-readable lines on stdout; errors on stderr prefixed `error:`.
- `setup`, `start`, and `stop` begin — before any check — by printing
  `needs administrator privileges for:` followed by the privileged steps they may take,
  then call `sudo -v` once (FR-009). `status` never does this and never prompts.
- Each change is printed as `adding: <step>` before it is made, then `changed: <step>`;
  each already-correct step as `ok: <step>` (since 002).
- A failing step prints `error: <step>: <reason>` and the action exits non-zero, having
  restored any file it replaced.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success (for `status`: share is available) |
| 1 | Action failed (step error, privilege refused) |
| 2 | Usage error (unknown action/flag) |
| 3 | Not configured (config missing/invalid) |
| 4 | Not set up (run `export setup`) |
| 5 | `status` only: set up but not available |

## Actions

### `export setup`

- Requires config. Ensures every SetupComponent (see data-model) is present.
- Prompts for the share password only when the owner has no Samba password yet (no echo,
  asked twice, must match). Never prints or stores it.
- Re-running after `allowed_network` changes rewrites the globals and replaces the firewall
  rule (any `Samba` rule for another network is deleted).
- Does not change whether the share is available.
- Exit 0 when all components are present afterwards.

### `export start`

- Requires config and complete setup (else exit 3 / 4 with a message naming the fix).
- Ensures the share stanza matches config and `smbd` is enabled and active.
- Re-running with a changed `share_name`, `path`, or `exclude` applies the change.
- A changed `allowed_network` or `owner` makes setup incomplete: `start` exits 4 with
  `export: configuration changed — run 'local-machine export setup'`.
- On success prints the connect lines (same as `status`).

### `export status`

- Needs no privileges; never changes anything; never prompts. Root-only checks run with
  `sudo -n`; if that fails the component shows as `unknown` (a `warning:` line suggesting
  `sudo -v` first), never as missing.
- Example (available):

  ```text
  export: available
  shared:  /home/kostadis  (excluding .ssh, .gnupg)
  network: 192.168.1.0/24  (reachable: yes, via 192.168.1.85)
  windows: \\192.168.1.85\home
  unix:    sudo mount -t cifs //192.168.1.85/home /mnt/home -o username=kostadis,posix,uid=$(id -u),gid=$(id -g)
  ```

- Not available: `export: set up, not available` (exit 5); not set up:
  `export: not set up — run 'local-machine export setup'` plus the missing components
  (exit 4); not configured: names the file and missing key (exit 3).
- Also prints `warning:` lines for config drift, for symlinks into excluded names (scan
  limited to 4 levels below the shared path; a note says when the limit was hit), and for
  a setup-level config change (`configuration changed — run 'local-machine export setup'`).
  The `network:` line uses `yes`/`no` for reachability and `unknown` for an unread firewall.

### `export stop`

- Requires config. If setup is incomplete, says so (exit 4) — never installs anything.
- Ensures the share stanza is absent and `smbd` is disabled and stopped.
- When already stopped: prints `export: nothing was shared` and exits 0.
