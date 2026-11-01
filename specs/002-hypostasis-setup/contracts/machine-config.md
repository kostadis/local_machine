# Config Contract: `<config-dir>/machine.yaml` (shared)

The first shared config file (constitution Principle II: "the shared file's name and schema
are fixed by the first feature that needs one"). It holds facts about the machine that more
than one subcommand uses. Each value lives here and nowhere else.

```yaml
allowed_network: 192.168.1.0/24   # required: the trusted home network
owner: kostadis                   # optional: default is the invoking user; never root
```

| Key | Used by | Meaning |
|---|---|---|
| `allowed_network` | `export` (Samba `hosts allow`, firewall), `hypostasis` (service firewall) | the home network allowed to reach this machine's shares and services |
| `owner` | `export` (share owner), `hypostasis` (account that owns checkouts and runs services) | the one human account |

## Rules

- Missing file → exit 3 for every subcommand that needs it: `no config at
  <dir>/machine.yaml (need at least allowed_network)`.
- Unknown keys are errors (a key added here is a constitution-recorded schema change).
- `allowed_network` parses as a CIDR and is truncated to the network.
- `owner` must exist and must not be `root`.

## Migration (this feature)

- `export` reads `allowed_network` and `owner` from `machine.yaml`. Its own file keeps
  `share_name`, `path`, `exclude`.
- `allowed_network` or `owner` left in `export.yaml` is an error:
  `<dir>/export.yaml: allowed_network: moved to <dir>/machine.yaml`. No fallback.
- On this machine: create `machine.yaml` with `allowed_network: 192.168.1.0/24` and remove
  that line from `export.yaml` (an `export.yaml` with no keys left is valid: all defaults).
- Constitution Principle II gains the file name and schema (PATCH amendment, this branch).
