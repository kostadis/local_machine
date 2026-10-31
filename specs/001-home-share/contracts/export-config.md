# Config Contract: `<config-dir>/export.yaml`

Default location: `~/.config/local-machine/export.yaml`. Field rules: see
[data-model.md](../data-model.md#exportconfig-owner-edited-config-direxportyaml).

## Minimal

```yaml
allowed_network: 192.168.1.0/24
```

## Full

```yaml
allowed_network: 192.168.1.0/24   # required; the home network allowed to connect
share_name: home                  # name Windows/Unix clients use: \\<host>\home
path: /home/kostadis              # default: owner's home directory
owner: kostadis                   # default: user running local-machine
exclude:                          # names hidden and blocked at any depth
  - .ssh
  - .gnupg
```

## Errors (exit 3)

| Condition | Message |
|-----------|---------|
| File missing | `error: no config at <dir>/export.yaml (need at least allowed_network)` |
| `allowed_network` missing/invalid | `error: <dir>/export.yaml: allowed_network: <reason>` |
| Unknown key | `error: <dir>/export.yaml: unknown key '<key>'` |
| `password` key present | `error: <dir>/export.yaml: passwords are not stored in config; run 'local-machine export setup'` |
| `exclude` entry contains `/` | `error: <dir>/export.yaml: exclude: '<entry>' must be a name, not a path` |
