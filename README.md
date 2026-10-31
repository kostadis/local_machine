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

## `export` — share the home directory

Shares your home directory over SMB 3 (Samba) so the Windows desktop (Obsidian, editors)
and other Linux VMs on the home network can read and write it. Access needs your Samba
password and is limited to the configured network. Spec: [specs/001-home-share](specs/001-home-share/spec.md).

`~/.config/local-machine/export.yaml` — minimal:

```yaml
allowed_network: 192.168.1.0/24
```

Full:

```yaml
allowed_network: 192.168.1.0/24   # required; the home network allowed to connect
share_name: home                  # \\<host>\home
path: /home/you                   # default: your home directory
owner: you                        # default: the user running local-machine
exclude:                          # names hidden and blocked at any depth
  - .ssh
  - .gnupg
```

The share password is never stored in this file; `export setup` asks for it.

| Command | What it does |
|---------|--------------|
| `local-machine export setup` | Installs Samba, writes its settings, sets the share password, opens the firewall for `allowed_network`. Needs sudo. Safe to re-run; re-run after changing `allowed_network` or `owner`. |
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

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

System access goes through the `System` trait (`src/system/mod.rs`); tests use
`system::fake::FakeSystem`, so `cargo test` never touches the machine.
