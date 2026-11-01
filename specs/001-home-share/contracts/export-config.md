# Config Contract: `<config-dir>/export.yaml`

> Updated 2026-11-01 by 002: shared values moved to `machine.yaml`.

Default location: `~/.config/local-machine/export.yaml`. Field rules: see
[data-model.md](../data-model.md#exportconfig-owner-edited-config-direxportyaml).

`allowed_network` and `owner` live in the shared `<config-dir>/machine.yaml`
([002 contract](../../002-hypostasis-setup/contracts/machine-config.md)), not here.

## Minimal

`export.yaml` may be absent or empty: every key has a default. `machine.yaml` needs:

```yaml
allowed_network: 192.168.1.0/24
```

## Full

```yaml
share_name: home                  # name Windows/Unix clients use: \\<host>\home
path: /home/kostadis              # default: owner's home directory
exclude:                          # names hidden and blocked at any depth
  - .ssh
  - .gnupg
```

## Errors (exit 3)

| Condition | Message |
|-----------|---------|
| `machine.yaml` missing | `error: no config at <dir>/machine.yaml (need at least allowed_network)` |
| `allowed_network` missing/invalid | `error: <dir>/machine.yaml: allowed_network: <reason>` |
| `allowed_network` or `owner` in `export.yaml` | `error: <dir>/export.yaml: <key>: moved to <dir>/machine.yaml` |
| `export.yaml` missing | not an error: all defaults |
| Unknown key | `error: <dir>/export.yaml: unknown key '<key>'` |
| `password` key present | `error: <dir>/export.yaml: passwords are not stored in config; run 'local-machine export setup'` |
| `exclude` entry contains `/` | `error: <dir>/export.yaml: exclude: '<entry>' must be a name, not a path` |
