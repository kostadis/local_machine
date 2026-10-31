# Feature Specification: `local-machine export` — Share Home Directory on the Home Network

**Feature Branch**: `001-home-share`

**Created**: 2026-10-30

**Status**: Draft

**Input**: User description: "i want a sub command that will export my local home directory on my local netowrk so I can use my windows desktop to view data. The critical use case is using the windows desktop tools like obsidian or other editors. Either CIFS or NFS is acceptable. The only users on my system are me. This is a home private network. And only people i trust are on the network."

## Overview

`export` is a top-level subcommand of `local-machine`. It makes the owner's home directory
available on the home network so that other machines can work on those files directly: the
Windows desktop (Obsidian first, other editors too) and other Unix VMs. Following the constitution's command grammar, it
offers four actions:

| Action | Purpose |
|--------|---------|
| `local-machine export setup` | Prepare the machine so it can share (one time; re-runnable) |
| `local-machine export start` | Make the home directory available on the network |
| `local-machine export status` | Report whether it is available and how to connect from each kind of client |
| `local-machine export stop` | Make it unavailable |

This spec defines what `export` must achieve and what each action guarantees to the owner.
How each action accomplishes it is left to planning.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Edit home-directory files from Windows desktop tools (Priority: P1)

On the Ubuntu machine the owner runs `export setup` once, then `export start`. On the
Windows desktop they connect to the share and open a folder in it as an Obsidian vault, or
open files in any Windows editor. They read, edit, create, rename, and delete files, and the
changes appear on the Ubuntu machine immediately.

**Why this priority**: This is the stated critical use case; without it `export` has no value.

**Independent Test**: On a fresh Ubuntu machine with configuration present, run
`export setup` and `export start`, connect from Windows, open a folder as an Obsidian vault,
create and edit a note, then confirm the note exists with the new content in the home
directory on the Ubuntu machine.

**Acceptance Scenarios**:

1. **Given** a fresh machine with configuration present, **When** the owner runs
   `export setup`, **Then** the machine is ready to share and the share is not yet available.
2. **Given** setup has not been run, **When** the owner runs `export start`, **Then** it
   refuses, changes nothing, and tells the owner to run `export setup`.
3. **Given** setup has been run, **When** the owner runs `export start`, **Then** the home
   directory is reachable from the Windows desktop with only what Windows provides built in.
4. **Given** the share is available, **When** the owner edits and saves a file in a Windows
   editor, **Then** the change is visible on the Ubuntu machine and the file belongs to the
   owner's Ubuntu account.
5. **Given** a folder in the share is open as an Obsidian vault, **When** a file in it changes
   on the Ubuntu side, **Then** Obsidian shows the change without the vault being reopened.
6. **Given** the share is available, **When** the Ubuntu machine reboots, **Then** the share
   is available again with no action from the owner.

---

### User Story 2 - Use home-directory files from another Unix VM (Priority: P2)

From another Unix VM on the home network, the owner connects to the share and works on the
same files with ordinary command-line and desktop tools: reading, editing, creating,
renaming, and deleting, with changes visible on the Ubuntu machine and on Windows.

**Why this priority**: Extends the same share to the owner's other machines; the Windows
editing use case is still the critical one.

**Independent Test**: With the share available, connect from another Linux VM, create and
edit a file, then confirm it is on the Ubuntu machine, owned by the owner, and visible from
Windows.

**Acceptance Scenarios**:

1. **Given** the share is available, **When** the owner connects from a Unix VM using the
   address `export status` reports, **Then** the home directory is reachable using the
   client tools that VM's operating system normally provides.
2. **Given** the share is connected on a Unix VM, **When** the owner creates or edits a file
   there, **Then** the change is visible on the Ubuntu machine and the file belongs to the
   owner's Ubuntu account, whatever the user ID on the Unix VM.
3. **Given** the same folder is open on Windows and on a Unix VM, **When** a file changes on
   either, **Then** the other sees the change without reconnecting.
4. **Given** a file's permissions and executable bit on the Ubuntu machine, **When** it is
   viewed from a Unix VM, **Then** they are shown faithfully and editing through the share
   does not change them.

---

### User Story 3 - Check status (Priority: P3)

The owner runs `export status` to learn whether the share is available, what is shared, and
the address to use from Windows and from a Unix VM.

**Why this priority**: Makes connecting and troubleshooting easy, but sharing works without it.

**Independent Test**: With the share available, run `export status` and use the addresses it
prints to connect from Windows and from a Unix VM; with the share unavailable, status says so.

**Acceptance Scenarios**:

1. **Given** the share is available, **When** the owner runs `export status`, **Then** it
   reports available, the shared directory, and the exact address to use from Windows and
   from a Unix VM.
2. **Given** the share is not available, **When** the owner runs `export status`, **Then**
   it reports not available and exits with a non-zero status.
3. **Given** setup is incomplete, **When** the owner runs `export status`, **Then** it says
   so and names `export setup` as the fix.

---

### User Story 4 - Stop sharing (Priority: P4)

The owner runs `export stop`. The home directory is no longer reachable from the network and
stays unreachable after reboot.

**Why this priority**: Completes the lifecycle; the owner's main goal is enabling the share.

**Independent Test**: With the share available and connected from Windows and a Unix VM,
run `export stop`, then confirm neither can reach it, including after a reboot.

**Acceptance Scenarios**:

1. **Given** the share is available, **When** the owner runs `export stop`, **Then** no
   client can connect and the share stays unavailable after reboot.
2. **Given** the share is not available, **When** the owner runs `export stop`, **Then** it
   succeeds and reports that nothing was shared.

---

### Edge Cases

- Any action is run twice in a row: the second run has no further effect and succeeds.
- `export setup` is run while the share is available: it does not make the share
  unavailable, and does not make it available if it was not.
- An action fails partway: it reports which step failed and leaves the machine no worse than
  before; re-running the action completes it.
- Configuration is missing or lacks a required value: the action stops, naming the missing
  value and the config file; it never guesses machine-specific values.
- The configuration changes while the share is available: running `export start` again
  applies the new configuration.
- The Ubuntu machine is not reachable from the home network: `export status` reports the
  share as available on the machine but not reachable from the configured network.
- Someone connects with wrong credentials, or from outside the configured network: access is
  refused; there is never anonymous access.
- A client tries to open or create a path on the exclusion list (e.g. `.ssh`): refused; the
  excluded path is never created, read, or overwritten through the share.
- A file name is valid on Linux but not on Windows: the file stays intact on the Ubuntu
  side and is never corrupted or silently deleted through the share.
- A Unix VM's user has a different user ID or name than the owner on the Ubuntu machine:
  files still belong to the owner on the Ubuntu machine, and access still requires the
  owner's credentials.
- A symbolic link in the home directory points outside it: it is not followed to expose
  anything outside the shared directory.
- A symbolic link inside the home directory points at an excluded item (e.g.
  `keys -> .ssh`): `export status` warns about it. Blocking it is not required — the
  exclusion list is a convenience on a trusted network, not a security boundary.
- The same file is written from two clients, or from a client and Ubuntu, at once: last write wins; there are
  no locking guarantees beyond what the sharing method provides.

## Requirements *(mandatory)*

### Functional Requirements

**Command shape**

- **FR-001**: `export` MUST be a top-level subcommand of `local-machine` with exactly the
  actions `setup`, `start`, `status`, and `stop` (constitution Principle I, shared verbs).
- **FR-002**: All four actions MUST be idempotent: running one twice has the same result as
  running it once.
- **FR-003**: Every action MUST read configuration only from the config directory (default
  `~/.config/local-machine`, overridable with `--config-dir`), in YAML (Principle II), and
  MUST reuse existing parameter names for any parameter shared with other subcommands
  (Principle III).

**Action guarantees**

- **FR-004**: `export setup` MUST leave the machine ready to share, including everything
  that must be installed, and MUST NOT change whether the share is available.
- **FR-005**: `export setup` MUST be the only action that installs software. When setup is
  incomplete, `start`, `status`, and `stop` MUST say so and name `export setup` as the fix.
- **FR-006**: `export start` MUST make the share available and keep it available across
  reboots until `export stop` is run.
- **FR-007**: `export status` MUST report whether the share is available, the shared
  directory, and the address to use from Windows and from a Unix VM, and MUST exit non-zero when not available.
- **FR-008**: `export stop` MUST make the share unavailable, including after reboots.
- **FR-009**: Any action needing administrator privileges MUST say so before doing anything,
  and a failed action MUST NOT leave the machine half-configured.

**What is shared and to whom**

- **FR-010**: The share MUST be usable from two kinds of client at the same time:
  - the Windows desktop, with only what Windows provides built in (no additional client
    software);
  - other Unix VMs on the home network, with the client tools their operating system
    normally provides.

  Either CIFS/SMB or NFS is acceptable to the owner, provided both kinds of client are served.
- **FR-011**: The share MUST be read-write from every client, so editors (Obsidian included)
  and command-line tools can create, edit, rename, and delete files.
- **FR-012**: Files created or changed through the share, from any client, MUST belong to the
  owner's Ubuntu account, as if edited locally, regardless of the client's own user identity.
- **FR-013**: Changes made on the Ubuntu machine or any client MUST be visible on all others
  without reconnecting or reopening (needed for Obsidian's file watching).
- **FR-014**: The share MUST cover the owner's home directory minus a configurable exclusion
  list of names (default `.ssh` and `.gnupg`). An excluded name is hidden wherever it occurs
  in the shared directory, at any depth; entries are names, not paths. Excluded items MUST
  be neither visible nor reachable by that name.
  Everything else, including other hidden files and folders such as `.obsidian`, MUST be
  shared.
- **FR-015**: Access MUST require the owner's credentials; anonymous access MUST NOT be
  possible. The share password MUST NOT be stored in configuration files.
- **FR-016**: Access MUST be limited to the home network named in configuration.
- **FR-017**: Unix clients MUST see each file's Unix permissions faithfully, and edits made
  through the share MUST NOT change a file's permissions.

### Key Entities

- **Export configuration**: The owner's YAML settings for `export` — shared directory, share
  name, allowed network, owning account, exclusion list.
- **Share**: The home directory as seen from the network. States: not set up, set up but not
  available, available.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fresh machine with configuration present, the owner goes from running
  `export setup` to having a folder open as an Obsidian vault on Windows in under 5 minutes.
- **SC-002**: 100% of create/edit/rename/delete operations done in Obsidian on Windows are
  reflected correctly on the Ubuntu machine, with the owner as the file owner.
- **SC-003**: Opening and saving a typical note (under 1 MB) from Windows takes under
  1 second on the home network.
- **SC-004**: Connection attempts from outside the configured network, or with wrong
  credentials, fail 100% of the time.
- **SC-005**: After 100% of reboots, an available share is available again with no manual
  action, and a stopped share stays unavailable.
- **SC-006**: Excluded names (by default `.ssh` and `.gnupg`) cannot be listed, read, or
  written by that name from any client, 100% of the time.
- **SC-007**: From a Unix VM, 100% of create/edit/rename/delete operations are reflected
  correctly on the Ubuntu machine with the owner as file owner, and file permissions are
  unchanged by edits.

## Assumptions

- The Ubuntu machine has one human user, the owner; multi-user access is out of scope.
- The home network is private and everyone on it is trusted; protection is limited to
  requiring credentials and restricting to the configured network.
- The Windows desktop runs a current Windows 10/11.
- The other Unix VMs are Linux (or similar Unix) machines on the same home network, used by
  the owner; their users are not separate people needing separate access.
- The Ubuntu machine (a VM) is reachable from the home network; configuring the hypervisor's
  networking is out of scope.
- The owner can grant administrator privileges when an action needs them.
- Connecting from Windows or a Unix VM is done by the owner by hand; configuring the client
  side (including installing client tools on a Unix VM) is out of scope for `export`.
- Sharing anything other than the home directory is out of scope.
