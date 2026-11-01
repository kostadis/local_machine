# Config Contract: `<config-dir>/hypostasis.yaml`

Owned by `local-machine`. **Not** hypostasis's authority file (that one is
`~/.config/hypostasis/hypostasis.yaml`); values whose home is the authority (venv, pins,
rpg-lib address/port, library DB location, env, DGX) are not allowed here (FR-022).
Shared values (`allowed_network`, `owner`) live in [machine.yaml](machine-config.md).

## Example (this machine)

```yaml
# local-machine hypostasis setup — NOT hypostasis's authority file.
authority: ~/.config/hypostasis/hypostasis.yaml          # default; may be omitted
authority_source: ~/src/campaigns-private/hypostasis.yaml # owner-kept seed copy (example)
backup_root: "/mnt/g/My Drive/hypostasis/backups"          # Google Drive mount
backups:                  # optional; each defaults to a folder under backup_root
  # rpg_library:   <backup_root>/rpg-lib
  # flexai_social: <backup_root>/flexai-social
  # flexai_combat: <backup_root>/flexai-combat
  # palaces:       <backup_root>/palaces      (<campaign>/<stamp>/, mneme's layout)

prerequisites:            # optional; replaces the default list when present
  apt: [git, build-essential, python3-dev, python3-venv, pipx, sqlite3, nodejs, npm, ufw]

checkouts:
  - { path: ~/src/mneme,             repo: https://github.com/kostadis/mneme.git,             branch: main }
  - { path: ~/src/dgx-fun,           repo: https://github.com/kostadis/dgx-fun.git,           branch: main }
  - { path: ~/src/turbovecdb,        repo: https://github.com/kostadis/turbovecdb.git,        branch: main }
  - { path: ~/src/Mempalace,         repo: https://github.com/kostadis/Mempalace.git,         branch: kostadis-dev }
  - { path: ~/src/CampaignGenerator, repo: https://github.com/kostadis/CampaignGenerator.git, branch: main }
  - { path: ~/src/mytools,           repo: https://github.com/kostadis/mytools.git,           branch: main }
  - { path: ~/src/5e-tools-kostadis, repo: https://github.com/kostadis/5e-tools-kostadis.git, branch: kostadis-dev }
  - { path: ~/src/homebrew-private,  repo: https://github.com/kostadis/homebrew-private.git,  branch: master }
  - path: ~/obelisk
    repo: https://github.com/kostadis/campaigns
    branch: main
    sparse: [obelisk, gm-assistant, scripts, tools, .claude]
  # ~/phandalin, ~/out-of-the-abyss, ~/toee likewise

hypostasis_source: ~/src/mneme  # where hypostasis lives; must be one of the checkout paths

services:
  venv: ~/.venvs/services
  mytools: ~/src/mytools        # must be one of the checkout paths

flexai_social:
  port: 5105
  data_dir: ~/data/flexai/social
  env:                          # optional; feature disabled when unset (mytools#96)
    CLEF_URL: http://192.168.1.121:8002/v1
    CHAT_URL: http://192.168.1.121:8003/v1

flexai_combat:
  port: 5106
  data_dir: ~/data/flexai/combat
```

## Validation (exit 3, message names file and key)

- Unknown key; `allowed_network`/`owner` present (→ "moved to machine.yaml"); any authority
  key (`venv`, `rpg_lib`, `rpg_library_db`, …) present (→ "lives in <authority>").
- `authority_source`, `backup_root`, `checkouts`, `hypostasis_source`, `services.venv`,
  `services.mytools`,
  `flexai_social`, `flexai_combat` required.
- Paths: `~` expands to the owner's home; must be absolute afterwards.
- Checkouts: unique `path`s, none nested in another; `repo` non-empty; `branch` non-empty;
  `sparse` non-empty list of plain names when present.
- `services.mytools` and `hypostasis_source` each equal a checkout `path`.
- `backups` keys limited to `rpg_library`, `flexai_social`, `flexai_combat`, `palaces`.
- Ports 1–65535, distinct from each other and from the authority's rpg-lib port (checked
  once the authority is readable).
- `env` only under `flexai_social`; keys limited to `CLEF_URL`, `CHAT_URL`, `CLEF_MODEL`,
  `CHAT_MODEL`.
- No secrets: a key named `password`/`token` anywhere is an error.
