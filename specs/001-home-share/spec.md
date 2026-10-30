# Feature Specification: Home Directory Network Share

**Feature Branch**: `001-home-share`

**Created**: 2026-10-30

**Status**: Draft

**Input**: User description: "i want a sub command that will export my local home directory on my local netowrk so I can use my windows desktop to view data. The critical use case is using the windows desktop tools like obsidian or other editors. Either CIFS or NFS is acceptable. The only users on my system are me. This is a home private network. And only people i trust are on the network."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Edit home-directory files from Windows desktop tools (Priority: P1)

The owner runs `local-machine export setup` once on the Ubuntu machine, which installs
and configures everything sharing needs, then runs `local-machine export start` to share
their home directory on the home network. On the Windows desktop they connect to the share (e.g. as a
mapped drive letter) and open a folder inside it as an Obsidian vault, or open files in any
Windows editor. They read, edit, create, rename, and delete files, and the changes appear
immediately on the Ubuntu machine.

**Why this priority**: This is the stated critical use case. Without read-write access that
Windows editors can use natively, the feature has no value.

**Independent Test**: On a fresh Ubuntu machine, run `local-machine export setup` then
`local-machine export start`, map the share from Windows, open a folder as
an Obsidian vault, create and edit a note, then confirm the file exists with the new content
in the home directory on the Ubuntu machine.

**Acceptance Scenarios**:

1. **Given** a fresh Ubuntu machine with configuration present, **When** the owner runs
   `local-machine export setup`, **Then** all software sharing needs is installed and
   configured, the owner's share credentials are established, and the share is not yet
   active.
2. **Given** setup has not been run, **When** the owner runs `local-machine export start`,
   **Then** it refuses, changes nothing, and tells the owner to run `export setup` first.
3. **Given** setup has been run, **When** the owner runs `local-machine export start`,
   **Then** the home directory becomes reachable from the Windows desktop using only the
   connection facilities built into Windows (no extra client software).
4. **Given** the share is active and mapped on Windows, **When** the owner edits and saves a
   file in a Windows editor, **Then** the change is visible on the Ubuntu machine and the
   file is owned by the owner's Ubuntu account.
5. **Given** a folder in the share is open as an Obsidian vault, **When** a file in it is
   changed on the Ubuntu side, **Then** Obsidian reflects the change without the vault being
   reopened.
6. **Given** the share is active, **When** the Ubuntu machine reboots, **Then** the share is
   available again without the owner re-running `export start`.

---

### User Story 2 - Check share status (Priority: P2)

The owner runs `local-machine export status` to see whether the share is active, and gets back the address
Windows should connect to, which directory is shared, and whether it is reachable.

**Why this priority**: Makes it easy to find the connection address and to diagnose why
Windows cannot connect, but the share works without it.

**Independent Test**: With the share active, run `local-machine export status` and use the address
it prints to connect from Windows successfully; with the share inactive, status reports
that clearly.

**Acceptance Scenarios**:

1. **Given** the share is active, **When** the owner runs status, **Then** it reports
   "active", the shared directory, and the exact address to enter on Windows.
2. **Given** the share is not set up, **When** the owner runs status, **Then** it reports
   "not active" and exits with a non-zero status.

---

### User Story 3 - Stop sharing (Priority: P3)

The owner runs `local-machine export stop` to stop sharing the home directory. Afterwards the share is no
longer reachable from the network and does not come back on reboot.

**Why this priority**: Needed for a clean lifecycle, but the owner's primary goal is
enabling the share.

**Independent Test**: With the share active and mapped on Windows, run `local-machine export stop`,
then confirm Windows can no longer reach the share, including after a reboot.

**Acceptance Scenarios**:

1. **Given** the share is active, **When** the owner stops it, **Then** Windows can no
   longer connect and the share stays off after reboot.
2. **Given** the share is not active, **When** the owner stops it, **Then** the command
   succeeds with a message that nothing was shared (idempotent).

---

### Edge Cases

- `export start` is run when the share is already active: it MUST succeed without
  creating a duplicate share and MUST apply any changed configuration.
- `export setup` is run again after a successful setup: it MUST succeed, re-applying
  configuration without reinstalling or duplicating anything, and MUST NOT change the
  active/not-active state of the share.
- `export setup` fails partway (e.g. a package download fails): it reports the failing step,
  and re-running it completes the setup.
- A host firewall is active: `export setup` opens file-sharing access for the configured
  network only, never for all networks.
- Required software is removed after setup: `export start` and `export status` report that
  setup is incomplete and point to `export setup`; neither installs anything.
- The configuration file is missing or lacks a required value (e.g. the allowed network):
  the action stops with a message naming the missing value and the config file path; it
  MUST NOT guess machine-specific values.
- The VM's network is not reachable from the home LAN (e.g. NAT-only networking): status
  reports that the share is active but not reachable from the configured network.
- Windows connects with the wrong credentials: access is refused; the share never falls
  back to anonymous access.
- A Windows editor writes a file with characters not allowed in Linux filenames or vice
  versa: names valid on Linux but invalid on Windows remain accessible (not silently hidden
  or corrupted).
- Windows tries to create a file or folder whose name matches an excluded path (e.g.
  `.ssh`): the operation is refused; it MUST NOT create or overwrite the excluded path.
- Two programs (Windows editor and an Ubuntu process) write the same file: last write wins;
  no locking guarantees beyond what the sharing protocol provides.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The feature MUST be the `export` subcommand of `local-machine`, with four
  actions (constitution Principle I):
  - `local-machine export setup` — install and configure everything sharing needs
    (User Story 1)
  - `local-machine export start` — start sharing (User Story 1)
  - `local-machine export status` — report share status (User Story 2)
  - `local-machine export stop` — stop sharing (User Story 3)
- **FR-002**: Sharing MUST use a network file-sharing protocol that Windows supports out of
  the box, so the Windows desktop needs no additional client software. (CIFS/SMB and NFS
  are both acceptable to the owner.)
- **FR-003**: The share MUST provide read-write access, so Windows editors (Obsidian
  included) can create, edit, rename, and delete files.
- **FR-004**: Files created or modified through the share MUST be owned by the owner's
  Ubuntu account, with the same effect as if the owner had edited them locally.
- **FR-005**: The shared directory MUST be the owner's home directory by default, minus a
  configurable exclusion list. Excluded paths MUST be neither visible nor reachable through
  the share. The default list is `.ssh` and `.gnupg`; the owner MAY extend or replace it in
  the YAML configuration. All other content, including other hidden files and folders such
  as an Obsidian vault's `.obsidian`, MUST be shared.
- **FR-006**: Access MUST require the owner's credentials; anonymous/guest access MUST NOT
  be enabled.
- **FR-007**: Access MUST be restricted to the home network specified in configuration;
  connections from other networks MUST be refused.
- **FR-008**: All machine-specific values (allowed network, share name, shared path, account
  name) MUST come from YAML configuration in the config directory, default
  `~/.config/local-machine`, overridable with `--config-dir` (constitution Principle II).
- **FR-009**: Any parameter that also exists on another subcommand MUST use that existing
  parameter name (constitution Principle III).
- **FR-010**: Once started, the share MUST stay available across reboots until explicitly
  stopped.
- **FR-011**: Setup, start, and stop MUST be idempotent: running any of them twice has the same result as
  running it once.
- **FR-012**: Status MUST report whether the share is active, the shared directory, and the
  address to enter on Windows, and exit non-zero when the share is not active.
- **FR-013**: Changes made on either side MUST be visible on the other side without
  remounting or reopening (needed for Obsidian's file watching).
- **FR-014**: Operations requiring administrator privileges MUST say so up front and MUST NOT
  leave a half-configured share if they fail partway.
- **FR-015**: `export setup` MUST install every piece of software the share needs from
  Ubuntu's standard package sources, configure the sharing service and the host firewall
  (allowing the configured network only), and establish the credentials Windows will use.
  It MUST prompt the owner for the share password and MUST NOT write that password into
  configuration files. Setup MUST NOT start sharing.
- **FR-016**: Only `export setup` installs software. `export start`, `status`, and `stop`
  MUST NOT install anything; when setup is incomplete they MUST say so and name
  `export setup` as the fix.

### Key Entities

- **Share configuration**: The YAML settings for this feature — shared path, share name,
  allowed network, owning account, exclusion list.
- **Share**: The live, network-visible export of the shared directory; states are active or
  not active.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: From a fresh Ubuntu machine with configuration present, the owner can go from
  running `export setup` and `export start` to having a folder open as an Obsidian vault on Windows in under
  5 minutes.
- **SC-002**: 100% of file create/edit/rename/delete operations done in Obsidian on Windows
  are reflected correctly on the Ubuntu machine, with correct ownership.
- **SC-003**: Opening and saving a typical note (under 1 MB) from Windows completes in under
  1 second on the home network.
- **SC-004**: A connection attempt from outside the configured network, or with wrong
  credentials, fails 100% of the time.
- **SC-005**: After a reboot, the share is available again with no manual action, in 100% of
  reboots.
- **SC-006**: Paths on the exclusion list (by default `.ssh` and `.gnupg`) cannot be listed,
  read, or written from Windows, 100% of the time.

## Assumptions

- The Ubuntu machine has exactly one human user (the owner); multi-user access control is
  out of scope.
- The home network is private and everyone on it is trusted; protection against hostile
  hosts on the LAN is limited to requiring credentials and restricting to the configured
  network. Encryption in transit beyond the protocol's defaults is out of scope.
- The Windows desktop is a current Windows 10/11 machine; recent Windows versions refuse
  anonymous/guest shares by default, which is why credentials are required (FR-006).
- The VM is reachable from the home LAN (bridged or equivalent networking). Configuring the
  hypervisor's networking is out of scope.
- The owner can grant administrator privileges (sudo) for `export setup`, `start`, and
  `stop`.
- The Ubuntu machine can reach Ubuntu's package sources when `export setup` runs.
- Connecting from Windows (mapping the drive, entering credentials) is done by the owner by
  hand; automating the Windows side is out of scope.
- Sharing directories other than the home directory, and sharing to non-Windows clients, are
  out of scope for this feature, though nothing prevents them working.
