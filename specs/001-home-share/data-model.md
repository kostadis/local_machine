# Data Model: `local-machine export`

## ExportConfig (owner-edited, `<config-dir>/export.yaml`)

| Field | Type | Required | Default | Validation |
|-------|------|----------|---------|------------|
| `allowed_network` | CIDR string | **yes** | — (machine-specific, never guessed) | Parses as IPv4/IPv6 CIDR |
| `share_name` | string | no | `home` | 1–80 chars, `[A-Za-z0-9_-]` |
| `path` | absolute path | no | owner's home directory | Exists, is a directory, owned by `owner` |
| `owner` | Unix username | no | user running the tool | Exists in passwd; must not be `root` |
| `exclude` | list of names | no | `[".ssh", ".gnupg"]` | Each non-empty, no `/`, not `.` or `..`; list may be empty only if written explicitly |

Rules:
- Unknown keys are an error (catches typos such as `exlcude`).
- A missing file or missing `allowed_network` stops every action except `status`, which
  reports "not configured", naming `<config-dir>/export.yaml` and the missing key.
- No password field exists; a `password` key is rejected with a message pointing to
  `export setup` (FR-015).

Full schema and example: [contracts/export-config.md](contracts/export-config.md).

## ShareState (derived at runtime, never stored)

```text
            setup               start
NotConfigured ──► (config ok) NotSetUp ──────► SetUp ──────► Available
                                  ▲             ▲  ◄──────── │
                                  │             │    stop     │
                                  └── software removed / include line removed
```

| State | Determined by |
|-------|---------------|
| `NotConfigured` | `export.yaml` missing or invalid |
| `NotSetUp` | Any setup component missing (see SetupComponent) |
| `SetUp` | All setup components present; share stanza absent or `smbd` not enabled+active |
| `Available` | All setup components present; share stanza present and matches config; `smbd` enabled and active |

Status additionally reports, without affecting the state:
- `reachable_from_network`: whether any local interface has an address inside
  `allowed_network`.
- `drift`: share stanza present but different from what the current config would render
  (fix: `export start`).
- `symlink_warnings`: symlinks under `path` that resolve into an excluded name.

## SetupComponent (checked and applied by `export setup`)

Each component has `check()` and `apply()`; setup applies only those whose check fails.

| Component | Present when |
|-----------|--------------|
| `samba-package` | `samba` installed (dpkg status `install ok installed`) |
| `global-config` | `/etc/samba/local-machine-export.conf` matches rendered globals |
| `include-line` | `/etc/samba/smb.conf` contains exactly one `include = /etc/samba/local-machine-export.conf` |
| `samba-user` | `owner` exists in Samba's password database (`pdbedit -L`) |
| `firewall` | `ufw` inactive, **or** a rule allows `allowed_network` to `Samba` |
| `nmbd-disabled` | `nmbd` not enabled |

## Managed system files

| File | Written by | Contents |
|------|-----------|----------|
| `/etc/samba/local-machine-export.conf` | `setup` | Global hardening (R4, R5, R2, R7) and `include` of the share file |
| `/etc/samba/local-machine-export-share.conf` | `start` (write), `stop` (empty) | The single `[<share_name>]` stanza: `path`, `valid users`, `force user`, `read only = no`, `veto files`, `delete veto files = no` |

Both files start with a header comment: `# Managed by local-machine export — do not edit`.
