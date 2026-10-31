<!--
Sync Impact Report
- Version change: 1.3.0 → 1.4.0 (MINOR: new principle added; Source Control expanded)
- Added principles:
  - VI. Privileged Actions: Look, Then Ask — every action that needs root goes through the
    shared runner `system::action::run`; checks only look; the notice names only steps that
    need root; a refusing gate or nothing-to-do never prompts; status-type actions never
    prompt; tests assert the order with `FakeSystem::prompted()`
- Modified principles: none
- Source Control: `main` changes only by merging a branch; no direct commits to `main`
- Governance: "Principles I–V" → "I–VI"; subcommand review now checks privileged actions (VI)
- Added/removed sections: none
- CLAUDE.md updated in the same change (Governance sync rule)
- Deferred: none

Previous report (1.3.0):
- Version change: 1.2.1 → 1.3.0 (MINOR: materially expanded Principle II)
- Modified principles:
  - II. Configuration, Not Hardcoding: per-subcommand config files by default; mandatory
    config-sharing review in each plan's research; shared values refactored into a shared
    file with every user updated in the same feature
- Governance: subcommand review now checks the config-sharing review
- Added/removed sections: none
- CLAUDE.md updated in the same change (Governance sync rule)
- Deferred: shared config file name/schema, to be fixed by the first feature that needs it

Previous report (1.2.1):
- Version change: 1.2.0 → 1.2.1 (PATCH: Principle I lists `setup` as a shared verb)
- Modified principles: I. Single Entry Point (shared-verb list; no rule change)
- Added/removed sections: none

Previous report (1.2.0):
- Version change: 1.1.0 → 1.2.0 (MINOR: materially expanded Principle I)
- Modified principles:
  - I. Single Entry Point: adds command grammar `local-machine <subcommand> <action>`,
    shared action verbs, and the requirement that specs name their subcommand and actions
- Added sections: none
- Removed sections: none
- CLAUDE.md updated in the same change (Governance sync rule)

Previous report (1.1.0):
- Version change: 1.0.0 → 1.1.0 (MINOR: materially expanded Principle II)
- Modified principles:
  - II. Configuration, Not Hardcoding (YAML): adds config storage location
    (`~/.config/local-machine` default, overridable via shared `--config-dir` parameter)
- Added sections: none
- Removed sections: none
- CLAUDE.md updated in the same change (Governance sync rule)

Previous report (1.0.0):
- Version change: (unversioned template) → 1.0.0
- Source: derived from /home/kostadis/src/local_machine/CLAUDE.md
- Principles (all new):
  - I. Single Entry Point: `local-machine`
  - II. Configuration, Not Hardcoding (YAML)
  - III. Shared Parameter Vocabulary
  - IV. Rust Implementation
  - V. Ubuntu Linux Target
- Added sections: Scope & Machine Boundary; Source Control; Governance
- Removed sections: none
- Templates: none modified (dependent templates read the constitution at runtime)
- Deferred TODOs: none. Note: "There will be a local repo" in CLAUDE.md is interpreted as
  "the tool's source lives in a local git repository"; no remote is assumed.
-->

# local-machine Constitution

## Core Principles

### I. Single Entry Point: `local-machine`

All scripts and tools in this project MUST be reachable as subcommands of one top-level
command named `local-machine`. No standalone binaries or loose scripts may be added as a
separate user-facing entry point. Every subcommand MUST be discoverable from
`local-machine --help` (or the help of its parent subcommand).

The command grammar is `local-machine <subcommand> <action> [parameters]`:

- A **subcommand** names one capability (e.g. `export`) and is a group, not a single verb.
- An **action** is a verb under that subcommand (e.g. `setup`, `start`, `status`, `stop`). The
  operations on a capability MUST be actions of its one subcommand, not sibling
  subcommands (no `unexport`, `export-status`) and not mode-selecting flags
  (no `export --stop`).
- Actions that mean the same thing across subcommands MUST use the same verb. Shared
  verbs so far:
  - `setup` — install and configure everything the capability needs (may need sudo);
    the only action that installs software; idempotent.
  - `start` / `status` / `stop` — lifecycle of a running capability.
- Every feature spec MUST name its subcommand and list its actions.

Rationale: one command makes the full toolset discoverable without remembering where each
tool lives.

### II. Configuration, Not Hardcoding (YAML)

Machine-specific facts — IP addresses, hostnames, paths, credentials references, and any
other value that describes this particular virtual machine — MUST live in configuration
files, never in source code. Configuration files MUST be YAML. Code MAY ship defaults only
for values that are not machine-specific.

Configuration MUST be stored in a configuration directory whose default is
`~/.config/local-machine`. The location is a parameter, not a constant: every subcommand
that reads configuration MUST accept the same `--config-dir` parameter (Principle III) to
override the default, and MUST NOT read configuration from any other location.

Configuration is **isolated by default, shared by refactoring**:

- Each subcommand reads its own file, `<config-dir>/<subcommand>.yaml` (e.g. `export.yaml`).
  A broken or missing file affects only its own subcommand.
- During research for every new feature (`/speckit-plan` Phase 0), the plan MUST compare the
  new subcommand's config values with every existing subcommand's and record the result:
  values with the same meaning (e.g. the home network, the owner account) are sharing
  opportunities.
- When a sharing opportunity exists, the shared value MUST move to a shared file in the same
  config directory, and every subcommand that uses it MUST be refactored to read it from
  there, in the same feature. Each value has exactly one home; a value MUST NOT be
  duplicated across subcommand files.
- The shared file's name and schema are fixed by the first feature that needs one, and
  recorded in this principle.

Rationale: the tools describe *how* to manage a machine; the config describes *which*
machine. Keeping them apart lets the same binary manage a rebuilt or replacement VM by
editing YAML alone. Isolation keeps subcommands independent; sharing is added only when
research shows a value really is common, so one fact is set in one place.

### III. Shared Parameter Vocabulary

When two or more subcommands take the same logical parameter, they MUST use the same
parameter name (same long flag, same short flag if any, same meaning, same value format).
A new subcommand MUST reuse an existing parameter name before inventing a new one; a new
name is permitted only for a concept no existing parameter covers.

Rationale: users learn a flag once and it works everywhere; divergent names for one concept
are a discoverability and correctness hazard.

### IV. Rust Implementation

The `local-machine` tool and all of its subcommands MUST be written in Rust. Shell or other
languages MUST NOT be used to implement subcommand logic; invoking external system
programs from Rust is permitted where that is the natural way to manage the system.

Rationale: a single implementation language keeps the toolset one coherent codebase with
one build and one dependency story.

### V. Ubuntu Linux Target

The tools MUST assume an Ubuntu Linux system (e.g. `apt`, `systemd`, Ubuntu filesystem
layout). Portability to other distributions or operating systems is not a goal and MUST NOT
add complexity to the code.

Rationale: the tool manages one known kind of machine; designing for others is cost without
benefit.

### VI. Privileged Actions: Look, Then Ask

Every action, in any subcommand, that needs root MUST run through the shared
privileged-action runner (`src/system/action.rs`, `action::run`). An action is an optional
gate (what must already be true) and a list of steps (what to make true). Actions MUST NOT
call `sudo_preflight` or run `sudo` commands outside the runner's steps and hooks.

- **Checks only look.** Gate and step checks MAY only read and use `sudo -n`; they MUST NOT
  prompt or change the machine. The runner enforces this by giving checks a `NoPrompt`
  system.
- **Ask only when needed.** The privilege notice MUST list only the steps that need root to
  change something or to find out, and MUST precede every root command.
- **Refuse before asking.** When the gate refuses (e.g. not set up), the action MUST exit
  with the gate's code before any notice or prompt.
- **Nothing to do, no prompt.** When every step is already in place, the action MUST print
  its `ok:` lines without a notice or prompt.
- **Reports never prompt.** Read-only actions such as `status` MUST NOT prompt; they report
  what they cannot check without root instead.
- **Tested.** Tests of every privileged action MUST assert the privilege order with
  `FakeSystem::prompted()`.

Rationale: a sudo prompt is an interruption and a trust decision. Asking only when there is
root work to do, and saying exactly what that work is, keeps the prompt meaningful; one
shared runner keeps the rule the same for every subcommand instead of re-solved in each.

## Scope & Machine Boundary

This project contains scripts and tools for managing this particular virtual machine (see
`~/src/CLAUDE.md` for the surrounding workspace). Features MUST serve management of that
machine. The identity of the machine is supplied by YAML configuration (Principle II), not
by assumptions in code.

## Source Control

The project's source lives in a local git repository. Changes are made and committed
locally; no remote hosting is assumed by this constitution.

`main` changes only by merging a branch; nothing, including a constitution amendment, is
committed directly to `main`. Each change is committed on a branch (a feature branch, or a
short branch of its own) together with anything it depends on, so `main` always holds a
consistent set of rules and code.

## Governance

- This constitution is derived from the project's `CLAUDE.md`. The two MUST stay
  consistent: an amendment to either requires updating the other in the same change.
- This constitution supersedes conflicting practice. Specs, plans, and tasks produced by
  Spec Kit MUST be checked against Principles I–VI; any deviation MUST be justified
  explicitly in the plan's complexity tracking.
- Amendments are made via `/speckit-constitution` on a branch (see Source Control),
  recorded with a Sync Impact Report, and versioned semantically: MAJOR for removed or
  redefined principles, MINOR for added principles or sections or materially expanded
  guidance, PATCH for clarifications.
- Every review of a new subcommand MUST verify: it hangs off `local-machine` and follows the
  `<subcommand> <action>` grammar (I), contains no
  machine-specific literals, reads config only from `--config-dir`, and its plan records the
  config-sharing review (II), reuses existing parameter names (III), is Rust (IV),
  targets Ubuntu only (V), and runs every action that needs root through `action::run`,
  with tests asserting the privilege order (VI).

**Version**: 1.4.0 | **Ratified**: 2026-10-30 | **Last Amended**: 2026-10-31
