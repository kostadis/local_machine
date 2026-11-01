# local-machine

Tools for managing this machine (Ubuntu). Every tool is a subcommand:
`local-machine <subcommand> <action>`. Principles: [.specify/memory/constitution.md](.specify/memory/constitution.md).

## Install

```sh
cargo install --path .
```

## Configuration

Each subcommand reads `<config-dir>/<subcommand>.yaml`. The config directory defaults to
`~/.config/local-machine` and can be changed on any command with `--config-dir DIR`.

Facts more than one subcommand uses live in the shared `<config-dir>/machine.yaml`:

```yaml
allowed_network: 192.168.1.0/24   # required; the trusted home network
owner: you                        # optional; default the invoking user; never root
```

## `export` — share the home directory

Shares your home directory over SMB 3 (Samba) so the Windows desktop (Obsidian, editors)
and other Linux VMs on the home network can read and write it. Access needs your Samba
password and is limited to the configured network. Spec: [specs/001-home-share](specs/001-home-share/spec.md).

Needs `machine.yaml` (above). `~/.config/local-machine/export.yaml` is optional; every key
has a default:

```yaml
share_name: home                  # \\<host>\home
path: /home/you                   # default: your home directory
exclude:                          # names hidden and blocked at any depth
  - .ssh
  - .gnupg
```

The share password is never stored in this file; `export setup` asks for it.

| Command | What it does |
|---------|--------------|
| `local-machine export setup` | Installs Samba, writes its settings, sets the share password, opens the firewall for `allowed_network`. Needs sudo. Safe to re-run; re-run after changing `allowed_network` or `owner` in `machine.yaml`. |
| `local-machine export start` | Shares the home directory; stays shared across reboots. |
| `local-machine export status` | Shows whether it is shared and how to connect. Never prompts. |
| `local-machine export stop` | Stops sharing; stays stopped across reboots. |

Exit codes: 0 ok, 1 failed, 2 usage, 3 not configured, 4 not set up, 5 (`status`) set up but
not shared.

### Connecting

`export status` prints both lines, for example:

- **Windows**: map `\\192.168.1.85\home` (File Explorer → Map network drive), log in with
  your Ubuntu username and the share password.
- **Linux VM** (needs `cifs-utils`):
  `sudo mount -t cifs //192.168.1.85/home /mnt/home -o username=you,posix,uid=$(id -u),gid=$(id -g)`

## `hypostasis` — set up mneme's machine and run its local services

Brings a fresh Ubuntu machine to the point where `mneme up` works, then runs the three
local services (rpg-lib, flexai-social, flexai-combat) as systemd user units reachable
from the home network. Spec: [specs/002-hypostasis-setup](specs/002-hypostasis-setup/spec.md).

Config:

- `machine.yaml` (above): the home network the services' firewall rules allow, and the
  owner, who must be the one running every action.
- `~/.config/local-machine/hypostasis.yaml`: checkouts, the backup folders, the services
  venv, FlexAI ports and data folders. This is **not** hypostasis's own authority file
  (`~/.config/hypostasis/hypostasis.yaml`), which keeps the venv, pins, rpg-lib's port and
  the library DB path; `local-machine` only reads it. Example:
  [hypostasis.example.yaml](specs/002-hypostasis-setup/hypostasis.example.yaml); schema:
  [contracts/hypostasis-config.md](specs/002-hypostasis-setup/contracts/hypostasis-config.md).

| Command | What it does |
|---------|--------------|
| `local-machine hypostasis setup` | Installs packages and `uv`, enables linger, clones missing checkouts (never touches existing ones), seeds the authority file, opens the service ports to `allowed_network`, installs hypostasis and its components, installs the services and their units, and restores missing data from the backups. Asks for sudo once, only for apt, linger and ufw. Starts nothing. Safe to re-run. |
| `local-machine hypostasis start` | Starts the three services and waits until each answers; they stay up across logout and reboot. Never prompts. |
| `local-machine hypostasis status` | Reports every piece and service, and the fix for each problem. Never prompts. |
| `local-machine hypostasis stop` | Stops the services; they stay stopped across reboots. Never prompts. |

Exit codes: 0 ok, 1 failed, 2 usage, 3 not configured (or run as root / not the owner),
4 not set up, 5 (`status`) set up but a service is not running.

Backups: rpg-lib's own backup scripts write to `$RPG_LIB_BACKUP_DIR`, set in your shell
profile. Point it at the same folder as `<backup_root>/rpg-lib` (or `backups.rpg_library`);
`local-machine` reads its folder from its own config and never sets that variable.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

System access goes through the `System` trait (`src/system/mod.rs`); tests use
`system::fake::FakeSystem`, so `cargo test` never touches the machine.
