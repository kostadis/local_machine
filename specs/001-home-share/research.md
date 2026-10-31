# Research: `local-machine export`

Environment observed on the target machine (2026-10-30): Ubuntu 26.04 LTS, Rust/cargo 1.98,
`ufw` installed, Samba not installed (candidate 4.23.6), `cifs-utils` 7.4 available, no
`~/.config/local-machine` yet. These observations inform choices; none are baked into code.

## R1. Sharing protocol

- **Decision**: SMB 3 served by Samba (`smbd`), one share.
- **Rationale**:
  - Windows 10/11 has an SMB client built in (FR-010); NFS needs the optional "Client for
    NFS" feature, absent on Windows Home editions.
  - SMB authenticates every connection with a username and password (FR-015). NFS v3/v4
    with `sec=sys` trusts the client's claimed UID, so a password cannot be required
    without Kerberos — too heavy for a one-user home network.
  - Linux clients mount SMB natively (`mount -t cifs`, from `cifs-utils`), so one protocol
    serves both client kinds.
  - Samba sends change notifications, which Obsidian's file watcher relies on (FR-013).
- **Alternatives considered**: NFS v4 (no password auth without Kerberos, weak Windows
  support); running both SMB and NFS (two access paths to secure and keep consistent, and
  the NFS path would violate FR-015).

## R2. Faithful Unix permissions for Unix clients (FR-017)

- **Decision**: Enable SMB 3.1.1 POSIX extensions on the share
  (`smb3 unix extensions = yes`, a per-share parameter — verified against the Samba 4.23
  `smb.conf(5)` shipped in Ubuntu 26.04, where it defaults to `yes`; set explicitly so the
  intent is visible). Unix clients mount with the `posix` option.
- **Rationale**: Samba 4.23 (Ubuntu 26.04's version) supports them. Unix clients then see
  real mode bits, the executable bit, and symlinks; Windows clients are unaffected.
- **Alternatives considered**: Fixed `create mask`/`force create mode` (would rewrite
  permissions on edit, violating FR-017); NFS (rejected in R1).
- **Risk**: verify in quickstart step V6 that a Unix-client edit leaves mode bits intact.

## R3. File ownership (FR-012)

- **Decision**: The only valid Samba user is the owner (`valid users = <owner>`), and the
  share sets `force user = <owner>`.
- **Rationale**: Every authenticated session already maps to the owner's Unix account;
  `force user` makes that explicit so no client identity (Windows SID, Unix UID) can leak
  into file ownership.

## R4. Credentials (FR-015)

- **Decision**: `export setup` prompts for the share password (no echo, entered twice) and
  registers it with Samba's own password database (`smbpasswd -s -a <owner>`, fed via
  stdin). Global settings: `map to guest = never`, `restrict anonymous = 2`,
  `server min protocol = SMB3`. The password is never written to configuration or logs.
- **Rationale**: Samba keeps its passwords separate from the Unix login password; storing
  them in Samba's database is what Samba requires and keeps them out of YAML.
- **Re-running setup**: if the owner already has a Samba password, setup does not prompt
  again (idempotent). Changing it is outside this feature (`sudo smbpasswd <owner>`).

## R5. Network restriction (FR-016)

- **Decision**: Two layers, both derived from `allowed_network` in configuration:
  1. Samba: `hosts allow = <allowed_network> 127.0.0.1`, `hosts deny = ALL`.
  2. Firewall: if `ufw` is active, `setup` adds `allow from <allowed_network> to any app
     Samba`. If `ufw` is inactive, it changes nothing and says so.
- **Rationale**: Samba's check refuses connections even with the firewall off; the
  firewall rule keeps other networks (e.g. Tailscale, host-only adapters) from reaching
  the port at all.
- **Also**: `disable netbios = yes` (and `nmbd` stays disabled) — Windows and Linux both
  connect by address over TCP 445; NetBIOS name service is unnecessary attack surface.
- **Printing off**: globals set `load printers = no`, `printing = bsd`,
  `printcap name = /dev/null`, `disable spoolss = yes`, so no printers are loaded or served.
  **Correction (V18, 2026-10-31)**: the packaged `smb.conf` defines `[printers]` and
  `[print$]` *before* our include, and Samba fixes a share's settings when it reads its
  section, so these globals do not remove them: both shares remain (`printing = cups`).
  **Owner decision (2026-10-31): accepted.** Both need the owner's Samba password
  (`guest ok = no`) and are covered by the global `hosts allow`/`hosts deny`, which
  smb.conf(5) applies to every share. `[print$]` is a read-only view of
  `/var/lib/samba/printers`; `[printers]` accepts print jobs into `/var/tmp` that never
  print (no printers). Removing them would mean editing more of Ubuntu's `smb.conf`.
- **Network changes (owner decision 2026-10-30)**: changing `allowed_network` requires
  re-running `export setup`, which rewrites the globals and replaces the firewall rule
  (removing any `Samba` rule for a different network). `start` and `status` detect the
  mismatch and point to `setup`.

## R6. Exclusion list (FR-014)

- **Decision**: Implement with Samba `veto files`. Each entry is a **name** (no `/`),
  default `.ssh` and `.gnupg`. `delete veto files = no`.
- **Rationale**: Vetoed names are invisible, cannot be opened, and cannot be created
  through the share — exactly FR-014 and the "create an excluded path" edge case.
- **Consequence (accepted by owner 2026-10-30)**: `veto files` matches a name at any depth, so a
  directory named `.ssh` inside a project folder would also be hidden. Path-specific
  exclusion (e.g. `Documents/private`) is not supported by this mechanism.
- **Alternatives considered**: Per-path ACLs (would change permissions on the owner's own
  files, also seen locally); bind-mounting a filtered view (complex, needs root-maintained
  mounts).

## R7. Symlinks (edge case)

- **Decision**: `follow symlinks = yes`, `wide links = no`, `allow insecure wide links = no`.
- **Rationale**: Samba then refuses to follow any symlink that resolves outside the share
  path. For Unix clients with POSIX extensions, symlinks are returned as symlinks and
  resolved client-side, so they never reach outside the share either.
- **Accepted gap (owner 2026-10-30: exclusions are a convenience, warn is sufficient)**: a symlink *inside* home pointing *to* `~/.ssh` (e.g. `keys -> .ssh`) is
  within the share path, and `veto files` checks the requested name, not the target. Plan:
  `export status` warns if any symlink under home resolves into an excluded path.
  Quickstart V8 verifies.
- **Scan depth**: the warning scan walks at most 4 directory levels below the shared path,
  skipping excluded directories and not following symlinked directories, so `status` stays
  fast on a large home directory. When the limit is reached, status says the scan was
  partial. A deeper link is not detected; acceptable because exclusions are a convenience.

## R8. Making the share available / unavailable and persistent (FR-006, FR-008)

- **Decision**:
  - `setup` installs `samba`, writes the global settings file
    `/etc/samba/local-machine-export.conf`, adds one `include =` line for it to the end of
    `/etc/samba/smb.conf`, registers the password, configures the firewall, and **leaves
    `smbd` in whatever state it was** (FR-004).
  - `start` writes the share stanza to `/etc/samba/local-machine-export-share.conf`
    (included from the global file), validates with `testparm -s`, then
    `systemctl enable --now smbd` and `smbcontrol smbd reload-config`.
  - `stop` removes the share stanza, reloads, then `systemctl disable --now smbd`.
- **Include placement**: Samba treats an `include` as part of whatever section precedes it,
  and Ubuntu's packaged `smb.conf` ends with the `[print$]` share. The global settings file
  therefore MUST begin with its own `[global]` header (Samba merges repeated `[global]`
  sections); otherwise every hardening setting would be applied to `[print$]` and ignored.
  The share file begins with its `[<share_name>]` header, so its include is safe.
- **Installing samba starts smbd**: the Ubuntu package enables and starts `smbd` on install.
  `setup` records smbd's state before installing and restores it afterwards (FR-004).
- **Rationale**: systemd `enable` gives reboot persistence; the share's presence is a file,
  so state is inspectable and changes are atomic (write temp file, `testparm`, rename).
- **Alternatives considered**: Editing `smb.conf` in place (fragile, clashes with the
  packaged file); `usershares` (no `hosts allow`/`force user` control).

## R9. Privileges

- **Decision**: The tool runs as the owner (so `~` and the config directory resolve to the
  owner) and invokes `sudo` for each privileged command. Before the first change, an action
  prints the privileged steps it will take and runs `sudo -v` once.
- **Rationale**: Running the whole tool under `sudo` would make `~` resolve to `/root`.
- **Ordering (FR-009)**: `setup`, `start`, and `stop` print the privileged steps they may
  take and run `sudo -v` **before any check**, so no check can trigger a sudo prompt ahead of
  that notice.
- **Checks never prompt**: every check that needs root (`pdbedit -L`, `ufw status`) runs as
  `sudo -n`. With credentials cached by the preflight it succeeds; without (as in `status`,
  which never runs `sudo -v`) the component is reported as `unknown`, never as missing.
  `status` therefore needs no privileges and never prompts.

## R10. Atomicity and idempotence (FR-002, FR-009)

- **Decision**: Every action is a list of steps, each `check() -> Done | NeedsChange` then
  `apply()`. Files are written to a temp file and renamed into place only after `testparm`
  accepts the result; on failure the previous file is restored and the step is reported.
- **Rationale**: Re-running converges; partial failure leaves prior state.

## R11. Rust crates

- **Decision**:
  - `clap` (derive) — command tree `local-machine <subcommand> <action>`, global
    `--config-dir`.
  - `serde` + `serde_yaml_ng` — YAML config (maintained fork of the archived `serde_yaml`).
  - `ipnet` — parse and validate `allowed_network` as a CIDR.
  - `rpassword` — no-echo password prompt.
  - `anyhow` — error context for user-facing messages; `thiserror` for typed config errors.
  - `std::process::Command` for system programs (apt, systemctl, smbpasswd, testparm, ufw).
- **Testing**: `cargo test` for pure logic (config parsing/validation, Samba config
  rendering, step planning against a fake command runner); `assert_cmd` for CLI shape and
  exit codes. End-to-end behaviour is verified on the real machine via `quickstart.md`.

## R12. Configuration layout (cross-subcommand convention)

- **Decision**: One YAML file per subcommand: `<config-dir>/export.yaml`. Future
  subcommands (e.g. `hypostasis`) get `<config-dir>/hypostasis.yaml`.
- **Rationale**: Subcommands stay independent; a malformed file for one does not break
  another. `--config-dir` (Principle II) stays the only location parameter.
- **Alternative considered**: A single `config.yaml` with one section per subcommand.
- **Note**: This sets a convention for all subcommands; accepted by owner 2026-10-30.
- **Config-sharing review (constitution v1.3.0, Principle II)**: `export` is the first
  subcommand, so there is nothing to compare against and nothing is shared. Values likely
  to be shared by later subcommands: `allowed_network` (the home network) and `owner`.
  The next feature's research MUST check these.
