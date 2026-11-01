# Feature Specification: `local-machine hypostasis` — Set Up Hypostasis on a New Machine and Run Its Local Services

**Feature Branch**: `002-hypostasis-setup`

**Created**: 2026-11-01

**Status**: Draft

**Input**: User description: "installation and setup of hypostasis on a new machine - requires the bring up of a collection of services and the configuration of some systems. I want a command to setup hypostasis and all of the pieces it depends on. Start up is left as a command of hypostasis."

**Clarified** (owner, 2026-11-01):

- "Start up is left as a command of hypostasis" means the `local-machine hypostasis`
  subcommand has the start action.
- Start brings up rpg-lib, flexai-social, and flexai-combat.
- Started services last across reboots until `stop`.

## Overview

Hypostasis (from the `mneme` repository) configures the campaign/DGX environment from one
hand-edited authority file. Before hypostasis can run on a new machine, a lot has to be in
place: system prerequisites, a set of source checkouts and data trees at the locations
hypostasis's authority names, an isolated runtime environment, hypostasis itself, its
authority file, the local services, and the restored data those services and the
semantic-search stores need. Today that is done by hand and from memory, and the local
services are also started by hand.

The **local services** are the three services that run on this machine as part of the GM
toolset:

| Service | What it is | Who else relies on it |
|---------|------------|-----------------------|
| rpg-lib | TTRPG library index service | hypostasis health-checks it, and `mneme up` refuses to start a campaign without it |
| flexai-social | GM web tool for FlexAI social encounters | the owner, in a browser |
| flexai-combat | GM web tool for FlexAI combat encounters | the owner, in a browser |

`hypostasis` is a new top-level subcommand of `local-machine` that does all of this. It
follows the shared lifecycle verbs:

| Action | Purpose |
|--------|---------|
| `local-machine hypostasis setup` | Install and configure hypostasis, the local services, and everything they depend on (one time; re-runnable) |
| `local-machine hypostasis start` | Start the local services |
| `local-machine hypostasis status` | Report which pieces are in place and which local services are running, without changing anything |
| `local-machine hypostasis stop` | Stop the local services |

Boundaries with the other tools:

- **Hypostasis** (`hypostasis install` / `apply` / `status`) still installs its own
  components at their pins and renders their configs. Setup calls it and does not
  re-implement it.
- **`mneme up <campaign>`** still starts a campaign's CampaignGenerator. It health-gates on
  rpg-lib, which `local-machine hypostasis start` provides.
- **External services** (the DGX inference endpoint and the embedding endpoint on it) run on
  other machines. Nothing here installs, starts, or stops them.

## Clarifications

### Session 2026-11-01

- Q: Which machines should be able to reach the three local services (rpg-lib, flexai-social,
  flexai-combat) once `start` brings them up? → A: All three are reachable from the home
  network, with the firewall limited to it (C).
- Q: If the backup location can't be reached when setup runs, should setup stop there, or
  finish everything else and leave the restores for a later run? → A: Software pieces stop
  at the first failure; the restores are each attempted, failures are reported, and setup
  exits non-zero (B).
- Q: When a local service fails to come up or dies later, should `start` and `status` show
  its recent log output? → A: Yes, only when it failed or isn't answering: its last ~20 log
  lines and where the full log is (B).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Bring a fresh machine to "ready" with one command (Priority: P1)

On a newly built Ubuntu machine the owner writes this subcommand's configuration (which
repositories, where, which runtime environment, where the backups are), then runs
`local-machine hypostasis setup`. The command installs the missing prerequisites, obtains
every source checkout and data tree, creates the runtime environment, installs hypostasis
into it, seeds hypostasis's authority file from the owner's kept copy, has hypostasis install
its own components, installs and configures the three local services, and restores the data
they and the palaces need from backup. At the end the owner can run
`local-machine hypostasis start` and then `mneme up` with no further manual preparation.

**Why this priority**: This is the feature. Without it, a new machine is prepared by hand
and from memory, which is the problem.

**Independent Test**: On a fresh Ubuntu machine with only this tool and its configuration
present, run `hypostasis setup`, then run hypostasis's own status command and
`local-machine hypostasis status`. Every component row reports installed, and every setup
piece is present. Only the local services (not started yet) and the external services can
be unreachable.

**Acceptance Scenarios**:

1. **Given** a fresh machine with this subcommand's configuration present, **When** the owner
   runs `hypostasis setup`, **Then** every prerequisite, checkout, data tree, environment,
   hypostasis install, local-service install, and restore listed in the configuration is in
   place, and hypostasis's component install has completed successfully.
2. **Given** setup has completed, **When** the owner runs hypostasis's own commands,
   **Then** they run from the configured runtime environment without any further
   installation step.
3. **Given** a piece is missing on the machine, **When** setup runs, **Then** the output
   names each piece it is adding before adding it, and confirms each one afterwards.
4. **Given** setup finishes, **When** the owner reads its output, **Then** it ends with the
   next step: `local-machine hypostasis start`, then `mneme up <campaign>`. Setup itself
   starts no local service.
5. **Given** a step that needs administrator rights (e.g. installing system packages),
   **When** setup runs, **Then** the owner is asked for the password only if that step has
   work to do, and is told exactly what that work is first.

---

### User Story 2 - Re-run setup safely on a machine that is already (partly) set up (Priority: P1)

The owner re-runs `hypostasis setup` after an interruption, after adding a repository to the
configuration, or on a machine that is already fully set up. Setup finishes what is missing
and leaves everything that is already in place alone. In particular it never discards or
rewrites work in an existing source checkout, and never overwrites existing data.

**Why this priority**: Setup on a real machine gets interrupted (network, password prompt,
a failing build). A setup that can't be re-run, or that damages existing checkouts or data on
a re-run, is unsafe on the owner's working machine. That is as important as working once.

**Independent Test**: Run setup to completion, make an uncommitted change and a local
branch in one of the checkouts, then run setup again. It reports every piece already in
place, asks for no password, and the uncommitted change and the branch are untouched.

**Acceptance Scenarios**:

1. **Given** everything is already in place, **When** the owner runs `hypostasis setup`,
   **Then** it reports each piece as already done, changes nothing, and does not ask for a
   password.
2. **Given** a configured checkout already exists with local changes, a different branch, or
   unpushed commits, **When** setup runs, **Then** that checkout is left exactly as it was.
3. **Given** a previous run stopped partway, **When** setup is re-run, **Then** it picks up
   with the pieces still missing and finishes them.
4. **Given** a path where a checkout should go is already taken by something that isn't the
   expected repository, **When** setup runs, **Then** it refuses for that piece, names the
   path and what it found, and changes nothing there.
5. **Given** local services are running, **When** setup is re-run, **Then** they keep
   running undisturbed.

---

### User Story 3 - Start and stop the local services (Priority: P1)

After setup, the owner runs `local-machine hypostasis start`. rpg-lib, flexai-social, and
flexai-combat come up, and each answers at its address. `hypostasis status` shows rpg-lib
reachable, `mneme up <campaign>` passes its rpg-lib health gate, and the two FlexAI tools
open in a browser. Later the owner runs `local-machine hypostasis stop` and none of them is
running.

**Why this priority**: `mneme up` refuses to start a campaign while rpg-lib is down, and the
FlexAI tools are used at the table. Without start, a set-up machine still needs manual steps
before it is usable.

**Independent Test**: On a set-up machine, run `hypostasis start`. Confirm that
hypostasis's own status reports rpg-lib reachable, `mneme up --dry-run` passes its rpg-lib
gate, and both FlexAI tools load in a browser. Run `local-machine hypostasis stop` and
confirm all three are unreachable.

**Acceptance Scenarios**:

1. **Given** setup is complete and no local service is running, **When** the owner runs
   `hypostasis start`, **Then** all three are running and answering at their addresses, and
   start reports each one started only after it has seen that service answer.
2. **Given** setup has not been run, or a local service is not ready (not installed, or its
   data is missing), **When** the owner runs `hypostasis start`, **Then** it refuses before
   starting anything, names the services that aren't ready, and tells the owner to run
   `hypostasis setup`.
3. **Given** some local services are already running, **When** the owner runs
   `hypostasis start`, **Then** it reports those as already running, leaves them alone, and
   starts only the others.
4. **Given** local services are running, **When** the owner runs `hypostasis stop`, **Then**
   each one stops, and stop reports each stopped only after it no longer answers.
5. **Given** no local service is running, **When** the owner runs `hypostasis stop`,
   **Then** it reports each as already stopped and changes nothing.
6. **Given** the local services were started, **When** the machine reboots, **Then** they
   are running again with no action from the owner, until the owner runs `hypostasis stop`.
7. **Given** one service fails to come up (e.g. its port is taken), **When** the owner runs
   `hypostasis start`, **Then** the others still start, start names the failed service and
   the cause it observed, exits non-zero, and doesn't leave the failed one half-started
   claiming to be up.

---

### User Story 4 - See what is in place, what is missing, and what is running (Priority: P2)

Before or after setup, the owner runs `local-machine hypostasis status` to see each piece
setup is responsible for and whether it is present, plus whether each local service is
running and answering. Those pieces are the prerequisites, checkouts and data trees, runtime
environment, hypostasis install, authority file, component install, local-service installs,
library database, FlexAI data files, palaces, and the firewall rules for the local
services. Status never changes the machine and
never asks for a password.

**Why this priority**: Useful for diagnosing a half-built machine and confirming a finished
one, but setup and start already report as they run.

**Independent Test**: On a machine where one configured checkout is missing, run
`hypostasis status`. It lists that checkout as missing, lists the rest as present, exits
non-zero, and nothing on the machine changes.

**Acceptance Scenarios**:

1. **Given** setup has completed and all local services are running, **When** the owner runs
   `hypostasis status`, **Then** every piece is reported present, each service is reported
   running and answering with its address, and the command exits successfully.
2. **Given** any piece is missing, **When** the owner runs `hypostasis status`, **Then** it
   names each missing piece and the action that fixes it (`setup`), and exits non-zero.
3. **Given** setup is complete but a local service is stopped, **When** the owner runs
   status, **Then** it reports that service stopped, names `hypostasis start` as the fix, and
   exits non-zero.
4. **Given** any state, **When** status runs, **Then** it asks for no password. If something
   can only be checked with administrator rights, it says it could not check it.
5. **Given** setup is complete, **When** the owner runs status, **Then** it points to
   hypostasis's own status command for component pins, rendered-config drift, and the
   external services. It does not repeat those checks as its own.

---

### Edge Cases

- **Configuration missing or invalid**: every action refuses before changing anything, names
  the configuration file and the problem, and does not ask for a password.
- **A source repository can't be fetched** (no network, no access): setup stops at that
  piece, names it, and reports which pieces are done and which are still missing. It never
  reports success.
- **Hypostasis's own component install fails** (e.g. a native build error): setup reports
  the failure as hypostasis reported it, exits non-zero, and does not hide the cause.
- **External services unreachable** (e.g. the DGX or the embedding endpoint): not a failure
  of any action here. Hypostasis's status reports them. flexai-social still starts and
  answers when the DGX is down; only its situational-decision feature is unavailable.
- **A checkout exists but at a commit hypostasis's authority pins differently**: setup
  leaves it alone. Pin enforcement is hypostasis's job, and hypostasis reports it.
- **Interrupted during the administrator-rights step**: a re-run detects which prerequisites
  are now present and asks again only for the remaining work.
- **Data already present** (a palace, the library database, or a FlexAI data file): setup
  leaves it alone, even if the backup is newer. Replacing live data is a deliberate owner
  action, not a setup step.
- **A backup is missing, unreadable, or fails to restore** (including a backup location
  that isn't reachable yet, e.g. a share not mounted): setup fails that piece, names the
  backup, removes anything half-restored at the target, still attempts the other restores,
  and exits non-zero. A later re-run restores only what is still missing.
- **The authority file exists but differs from the owner-kept source**: setup leaves it
  alone. Hypostasis's file on the machine is the authority; setup only seeds it once.
- **rpg-lib's declared address changes in the authority file**: its host may be any name
  or address of this machine (`localhost`, the hostname, or the home-network address, so
  the same URL is right when viewed from other machines). After a port change, the next
  `setup` updates the unit and the firewall rule, and `start`/`status` use the new port. A
  host that is another machine, or a URL whose port disagrees with the declared port, is a
  configuration error: every action refuses and names it. The address is never copied
  into this subcommand's config.
- **A service's port is already in use by something else**: start doesn't start that
  service, names the port and what holds it if it can see that, and doesn't stop the other
  process.
- **A connection from outside the home network** to any local service is refused by the
  firewall. A connection from the home network reaches all three.
- **A local service was started by hand, outside this tool**: status reports it answering
  but not under this tool's control. Start doesn't start a second copy, and stop says it
  can't stop what it didn't start.
- **Run as root, or without the configured owner account**: every action refuses and names
  the reason. Checkouts, the runtime environment, and the running services belong to the
  owner, not to root.

## Requirements *(mandatory)*

### Functional Requirements

**Command shape**

- **FR-001**: The capability MUST be the `hypostasis` subcommand of `local-machine`, with
  exactly the actions `setup`, `start`, `status`, and `stop`, all discoverable from
  `local-machine --help`.
- **FR-002**: `start` and `stop` MUST manage exactly the three local services (rpg-lib,
  flexai-social, flexai-combat), all together. They MUST NOT start or stop CampaignGenerator
  instances (that is `mneme up`/`down`), hypostasis's component installs, or the external
  services.
- **FR-003**: Every action MUST read its configuration only from
  `<config-dir>/hypostasis.yaml`, using the shared `--config-dir` parameter, and MUST reuse
  existing parameter names for any concept an existing subcommand already names.

**What setup puts in place**

- **FR-004**: Setup MUST install every system prerequisite needed to build and run
  hypostasis, its components, and the local services (including the toolchains needed to
  build components that have native code) when it is missing.
- **FR-005**: Setup MUST obtain every configured source checkout at its configured location
  and initial branch, when it is missing. These are hypostasis's own repository, each
  component hypostasis installs from a local path, the repositories holding the local
  services, and the external tool repositories hypostasis's authority references.
- **FR-006**: Setup MUST create the configured runtime environment when it is missing, and
  install hypostasis into it so that hypostasis's commands can be run.
- **FR-007**: Setup MUST make sure hypostasis's authority file is present at the location
  hypostasis reads it from. When it is missing, setup MUST copy it from the owner-kept source
  file named in the setup configuration. When it is already present, setup MUST leave it
  unchanged, even if it differs from the source file. When it is missing and the source file
  can't be read, setup MUST refuse that piece and name the source file. Setup MUST NOT
  generate or edit the authority file's contents.
- **FR-008**: Setup MUST then have hypostasis install its components, through hypostasis's
  own install command, and MUST treat a hypostasis failure as a setup failure.
- **FR-009**: Setup MUST install and configure each local service so that `start` can run
  it. Setup MUST NOT start any local service, and MUST NOT disturb one that is already
  running.
- **FR-010**: Setup MUST obtain each configured git-hosted data tree (campaign trees, the
  5etools data, homebrew) when it is missing, with the same rules as source checkouts
  (FR-005, FR-012, FR-013). A tree configured as a partial checkout MUST be created with
  exactly its configured set of directories.
- **FR-010a**: Setup MUST restore the following from the backup locations named in the
  setup configuration (one per kind of data, defaulting to folders under one backup root), each only when it is missing on the machine: the TTRPG library database,
  the semantic-search stores (palaces), and the FlexAI data files (flexai-social's workbooks and
  flexai-combat's workbook; commercial content that is never in a repository). Where the owning tool
  has its own restore path (e.g. `mneme`'s palace restore), setup MUST use it rather than
  copying the store's internals directly.
- **FR-010b**: Setup MUST NOT overwrite, merge into, or delete existing data. Data that is
  already present MUST be left alone and reported as present.
- **FR-010c**: Setup MUST confirm each restored item is readable by its owning tool before
  reporting it restored. When a configured backup is missing or unreadable, setup MUST fail
  that piece, name the backup, and leave no partial restore at the target.
- **FR-010d**: Setup MUST restore data only after the tools that own it are installed
  (FR-008, FR-009), and MUST NOT obtain any data the setup configuration does not list.

**Running the local services**

- **FR-023**: `start` MUST check every local service is ready (installed, configured, data
  present) before starting any. If any is not ready, it MUST refuse, start nothing, name each
  unready service, and name `setup` as the fix.
- **FR-024**: `start` MUST bring each local service up at its address and report it started
  only after it has answered there. rpg-lib's address MUST be read from hypostasis's authority
  file. The FlexAI services' addresses come from the setup configuration, because hypostasis's
  authority file doesn't describe them.
- **FR-025**: Once started, each local service MUST keep running after the owner logs out
  and come back after a reboot, until `stop` is run.
- **FR-026**: `stop` MUST stop each local service and keep it stopped across reboots, and
  MUST report each stopped only after it no longer answers.
- **FR-027**: `start` MUST leave an already-running service alone and say so. `stop` MUST do
  the same for an already-stopped one. When there is nothing to do, neither action may ask
  for a password.
- **FR-028**: When one service fails to start, `start` MUST still start the others, then
  name the failure and exit non-zero. A service whose port is held by another process MUST
  NOT be started, and that process MUST NOT be stopped. `stop` MUST NOT stop a service
  instance it did not start.
- **FR-029**: The local services MUST run as the owner account, never as root.
- **FR-029a**: Each local service's output MUST be kept in a log the owner can read without
  administrator rights. When `start` reports a service failed, or `status` reports a
  service not answering, it MUST show that service's last 20 log lines (or fewer if the log
  is shorter) and say where the full log is. Healthy services show no log output.
- **FR-030**: Each local service MUST be reachable from this machine and from the configured
  home network, and from nowhere else. Setup MUST let each service's port through the
  firewall from the home network only. The home network is a configuration value
  (FR-021). It is the same home network `export` uses, so planning's config-sharing review
  moves it to the shared `machine.yaml` (research R3). rpg-lib still answers at the address in
  hypostasis's authority file (e.g. `localhost`), and also answers on this machine's
  home-network address. When the firewall is inactive, setup MUST report that, and status
  MUST report it as a note, not as a missing piece.

**Safety and idempotence**

- **FR-011**: Setup MUST be idempotent. With unchanged configuration on a fully set-up
  machine it MUST change nothing, report every piece as already in place, and not ask for a
  password.
- **FR-012**: Setup MUST NOT modify an existing source checkout: no branch switch, pull,
  reset, clean, or overwrite of local changes or commits.
- **FR-013**: Setup MUST refuse a piece whose target location is occupied by something
  other than what is expected, name the location and what was found, and leave it unchanged.
- **FR-014**: Every step, in any action, that needs administrator rights MUST go through the
  shared privileged-action flow. Check first without asking. Ask only when there is root work
  to do, naming exactly that work. Refuse before asking when a precondition fails. Never ask
  when nothing needs to change.
- **FR-015**: Checkouts, the runtime environment, hypostasis's authority file, and restored
  data MUST belong to the owner account, never to root.
- **FR-016**: Setup MUST stop at the first non-restore piece it cannot put in place, name
  it, list what was completed and what remains, and exit non-zero. The data restores
  (FR-010a) are the exception: they are independent of each other, so setup MUST attempt
  every missing one, report each failure, and then exit non-zero. No action may report
  success on a partial result.

**Reporting**

- **FR-017**: Setup MUST report each piece as already present, added, or failed, and on
  success MUST end by naming `local-machine hypostasis start` and then `mneme up <campaign>`
  as the next steps.
- **FR-018**: Status MUST report, for each piece setup is responsible for, whether it is
  observed present on the machine (not whether the configuration says it should be), and
  MUST name the action that fixes each missing piece.
- **FR-018a**: For each local service, status MUST report whether it is running under this
  tool's control and answering at its address, show that address, and name `start` as the
  fix when it is not.
- **FR-019**: Status MUST exit successfully only when every piece is present and every local
  service is answering, and MUST NOT change the machine or ask for a password.
- **FR-020**: Status MUST NOT re-implement hypostasis's other checks (component pins,
  rendered-config drift, external services). It MUST point to hypostasis's status command
  for them.

**Configuration**

- **FR-021**: All machine-specific facts MUST come from configuration, not from the tool's
  code. These are checkout locations, repository addresses, branches, the location of the
  local services' runtime environment, the backup locations, the FlexAI services'
  addresses and data locations, the home network the local services are reachable from,
  and the owner account.
- **FR-022**: The configuration MUST NOT duplicate values whose authority is hypostasis's own
  authority file: component pins, hypostasis's runtime environment, rpg-lib's address and
  port, environment variables, the library database location, and the DGX endpoint. Each
  value has one home, and this subcommand reads those from the authority file.

### Key Entities

- **Setup configuration** (`<config-dir>/hypostasis.yaml`): the owner's description of what
  this machine needs. Owned by `local-machine`. It holds:
  - the source checkouts and data trees (location, repository address, initial branch,
    optional partial-checkout directory set)
  - the local services' runtime-environment location (hypostasis's own runtime
    environment is named in its authority file, FR-022)
  - the owner-kept source of hypostasis's authority file
  - the backup root and any per-kind backup locations
  - for each FlexAI service, its address and data location
  - the home network the local services are reachable from
- **Hypostasis authority file**: hypostasis's own hand-edited configuration (components,
  pins, services, data roots, environment, the DGX). Owned by hypostasis. `local-machine`
  makes sure it is in place, reads from it the values it owns, and never becomes a second
  authority for its contents.
- **Authority source file**: the owner-kept copy (e.g. in a private repository) that seeds
  hypostasis's authority file on a new machine. Setup reads it and never writes it.
- **Source checkout**: a working copy of one repository at a configured location. Once it
  exists it belongs to the owner's day-to-day work and setup never changes it.
- **Runtime environment**: the isolated environment hypostasis and its components are
  installed into.
- **System prerequisite**: an operating-system package or toolchain that hypostasis, a
  component, or a local service needs to build or run.
- **Local service**: rpg-lib, flexai-social, or flexai-combat. A service that runs on this
  machine. Setup installs and configures it. `start`/`stop` own whether it is running. Each
  has an address it answers at.
- **Data backup**: the configured locations holding backups of the TTRPG library database,
  the palaces, and the FlexAI data files. Setup reads it and never writes it.
- **Restorable data**: the TTRPG library database, each palace, and each FlexAI data file.
  Restored once, when missing, and owned by its tool from then on.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a fresh Ubuntu machine, the owner gets from "configuration written" to "a
  campaign can be started with `mneme up` and both FlexAI tools open in a browser" with two
  commands (`setup`, `start`) and no manual steps other than entering the administrator
  password at most once.
- **SC-002**: Re-running setup on a fully set-up machine changes nothing and asks for no
  password. (The earlier "under 1 minute" target was dropped by the owner, analysis G2,
  2026-11-01.)
- **SC-003**: Across repeated runs, 0 existing source checkouts lose uncommitted changes,
  local branches, or unpushed commits.
- **SC-004**: After an interrupted run, a single re-run completes setup without the owner
  cleaning anything up by hand.
- **SC-005**: For every missing piece, status names it and the fix. An owner can bring a
  half-built machine to ready by following status output alone.
- **SC-006**: After setup on a fresh machine, a search against a restored palace returns
  the same results it returned on the source machine, with no re-embedding.
- **SC-007**: Across repeated runs, 0 existing palaces, library databases, or FlexAI data
  files are modified.
- **SC-008**: The same tool and a different configuration file set up a second machine
  whose checkout locations differ from the first. No code change is needed.
- **SC-009**: Every time `start` reports a service started, that service answers at its
  address. After a reboot all started services answer again without owner action.

## Assumptions

- The machine runs Ubuntu, and the owner has a user account with administrator (sudo) rights.
- The owner can reach the source repositories over the network with credentials already set
  up on the machine (e.g. an SSH key or credential helper). Setting up those credentials is
  out of scope.
- External services (the DGX inference endpoint and the embedding endpoint on it) run on
  other machines. Nothing here installs, starts, or stops them, and hypostasis reports their
  reachability.
- Hypostasis's own install command (component install at pins, config rendering) is
  reused as-is. This subcommand prepares the ground for it and does not re-implement it.
- Hypostasis's authority file keeps listing rpg-lib as a service it health-checks but does
  not start (`managed: false`). `local-machine hypostasis` is the tool that starts it. No
  change to the `mneme` repository is required. (mneme GH #1 had planned for hypostasis
  itself to own rpg-lib's start/stop; the owner chose this subcommand instead.)
- flexai-social and flexai-combat are not hypostasis components or services today. They live
  in the `mytools` repository and are not in hypostasis's authority file, so their addresses
  and data locations live in this subcommand's configuration.
- flexai-social's situational-decision feature calls models on the DGX. Planning decides, in
  the config-sharing review, whether it reads the DGX endpoint from hypostasis's authority
  file (preferred: one home) or keeps its own defaults. flexai-social currently hard-codes
  LAN addresses as its endpoint defaults. That is filed as kostadis/mytools#96, and this
  feature MUST NOT rely on those defaults.
- The FlexAI data files are commercial content. They come only from the owner's backup
  location, never from a repository or the network.
- Existing checkouts are treated as the owner's working copies: setup creates missing ones
  and never updates existing ones. Keeping them current is the owner's (or hypostasis's) job.
- `gm-assistant` and other content that ships inside the campaign workspaces is not a separate
  piece. It arrives with the campaign trees.
- Creating and refreshing the data backups is out of scope. Setup only reads an existing
  backup. The backup locations are reachable from the new machine (e.g. a mounted share or a
  local copy) before setup runs.
- Restored palaces were built with the embedder hypostasis's authority file declares. A
  mismatch is reported by `mneme`, not by this subcommand.
- The setup configuration's file name (`hypostasis.yaml` in `local-machine`'s config dir) is
  distinct from hypostasis's authority file in hypostasis's own config location. Planning
  confirms the names don't confuse each other.
- The config-sharing review required by the constitution happens in planning (compare with
  `export.yaml`). The owner account and the home network (`export`'s `allowed_network`) are
  the expected sharing candidates.
