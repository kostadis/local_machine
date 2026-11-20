# Feature Specification: `local-machine hypostasis` — Bring Up the DMsGuild Library Palace

**Feature Branch**: `003-dmsguild-library-palace`

**Created**: 2026-11-20

**Status**: Draft

**Input**: User description: "https://github.com/kostadis/local_machine/issues/1 — hypostasis: bring up the DMsGuild library palace in setup/start/status" (the hypostasis side of kostadis/CampaignGenerator#664, including the GM ruling of 2026-11-20 in the issue's comments)

## Overview

CampaignGenerator spec 044, "Library passage search", adds search inside the owner's
converted DMsGuild books. The searchable store is the **library palace**: a semantic-search
store kept apart from every campaign palace. Spec 044 builds the palace, but bringing it up
on a machine and keeping it available is out of its scope. This feature does that work, as
an extension of the existing `local-machine hypostasis setup | start | status` actions
(spec 002).

**Who owns what (GM ruling, 2026-11-20, "option A"):**

| Concern | Owner |
|---------|-------|
| The `library build`, `library refresh` and `library status` commands, and what they do | CampaignGenerator (spec 044) |
| The library's keys (location, selection, source folder, cache, embedding setup) | hypostasis's authority file |
| *When* to build, and reporting whether the library is ready | `local-machine hypostasis` (this feature) |
| Refreshing after a book's converted text is fixed | the GM, running `library refresh` directly |

The contract this feature relies on, from spec 044:

- **FR-017 (044)**: the library's location, its selection declaration and the embedding
  setup come from hypostasis's authority file.
- **FR-018 (044)**: build, refresh and status are non-interactive and share one exit table
  (CampaignGenerator `specs/044-library-passage-search/contracts/cli.md`): **0** ok,
  **1** failed, **2** usage error, **3** not configured, **4** library not built,
  **5** embedder mismatch, **6** refused because the palace path resolves into a live palace.
  Every refusal message names the cause and the exact fix (an authority key or a command).
- `library status --json` reports `configured`, `built`, `books`, `stale` (the list of
  stale book ids), `embedder_match`, `palace` and `index_bytes`.
- The authority-file contract (`contracts/authority-config.md`) says CampaignGenerator only
  reads the file and **hypostasis seeds and owns it**. The keys are a new `library:` block
  (`palace`, `work_dir`, optional `cache_dir`, `source_root`, `selection` in exactly one of
  three forms, optional `exclude_book_ids`) plus the embedding settings in the existing
  `env:` block. CampaignGenerator has no defaults; the suggested locations are under
  `~/.local/share/hypostasis/library/`.
- CampaignGenerator merges the `env:` block into the build/search environment **without
  overriding variables already set**. A shell that already has `MEMPALACE_*` set therefore
  wins over the authority file unless the caller sets those variables itself.

## Clarifications

### Session 2026-11-20

- Q: `library status` exits 5 (embedder mismatch) and 6 (live-palace refusal) collide with
  local-machine's exit table. How should `hypostasis status` exit for them? → A: Map both to
  1 (failed); keep local-machine's table unchanged; the message names the condition.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Setup leaves a usable library palace on a new machine (Priority: P1)

The owner runs `local-machine hypostasis setup` on a machine where the library palace does
not exist. Once the authority file, CampaignGenerator and the embedder are in place, setup
has CampaignGenerator build the library from the configured source folder, using the
configured embedding setup, and reports the library as added. If a library already exists
at the configured location, setup leaves it alone.

**Why this priority**: Without a built library, passage search has nothing to search. This
is the piece that is done by hand today and, as the pilot showed, is easy to get wrong:
a shell missing the embedding settings silently built the library with the wrong embedder.

**Independent Test**: On a machine with no library at the configured location, run setup.
Then `library status` exits 0 and a passage search returns results from the converted
books. Run setup a second time: the library is reported as already present and is unchanged.

**Acceptance Scenarios**:

1. **Given** the authority file names the library's keys and no library exists at the
   configured location, **When** the owner runs setup, **Then** setup runs CampaignGenerator's
   `library build` with the embedding setup from the authority file, confirms the result with
   `library status`, and reports the library as added.
2. **Given** a library already exists at the configured location, **When** the owner runs
   setup, **Then** setup does not build, refresh, or replace it, and reports it as already
   present, even if it is stale or was built with a different embedder (status reports those).
3. **Given** the embedder endpoint does not answer, **When** setup reaches the library,
   **Then** setup fails the library piece, names the endpoint, leaves nothing half-built at
   the location, still attempts the other data pieces, and exits non-zero.
4. **Given** the source folder can't be read (e.g. the drive is not mounted), **When** setup
   reaches the library, **Then** setup fails the library piece and names the folder.
5. **Given** the owner's shell has no embedding settings, or different ones, **When** setup
   builds the library, **Then** the build still uses exactly the embedding setup from the
   authority file.

---

### User Story 2 - Status says whether the library is ready, and how to fix it if not (Priority: P1)

The owner runs `local-machine hypostasis status`. Alongside the existing pieces, it reports
the library: whether it is present, whether it was built with the embedder the authority
file now names, how many books it holds, and whether it is stale. Each problem comes with
the command that fixes it. Status changes nothing.

**Why this priority**: The library has failure modes that look like success (a library built
with the wrong embedder still answers searches, badly). The owner needs to see them.

**Independent Test**: Put the library in each state (ready, missing, built with another
embedder, keys missing from the authority file, location overlapping a live palace). Run
status in each and check the report, the fix, and the exit status.

**Acceptance Scenarios**:

1. **Given** a ready library, **When** the owner runs status, **Then** the library is
   reported present, built with the configured embedder, with its book count and staleness,
   and does not make status exit non-zero.
2. **Given** `library status` returns 3, 4, 5 or 6, **When** the owner runs status, **Then**
   status reports the matching condition (not configured, not built, embedder mismatch,
   refused live palace) with its fix, and exits 3, 4, 1 or 1 respectively when the
   library is the only problem.
3. **Given** the library is stale (`library status` lists stale book ids), **When** the
   owner runs status, **Then** status reports how many books are stale and names
   `library refresh` as the fix.
4. **Given** any library state, **When** the owner runs status, **Then** nothing is built,
   refreshed, or written, and no administrator rights are asked for.

---

### User Story 3 - Start confirms the library can be searched (Priority: P2)

The owner runs `local-machine hypostasis start`. The library is a file-backed store with no
service of its own, so there is nothing to start. Instead, start checks that the library is
ready and that the embedder endpoint answers (a search needs it to embed the query), and
reports the library as available or not, with the reason.

**Why this priority**: Useful as a one-command "is everything up" check, but the library
works without it once built.

**Independent Test**: Run start with a ready library and the embedder up: library reported
available. Stop the embedder, or remove the library, and run start again: library reported
unavailable with the reason, and the local services still start.

**Acceptance Scenarios**:

1. **Given** a ready library and an answering embedder, **When** the owner runs start,
   **Then** the library is reported available.
2. **Given** the library is not ready or the embedder does not answer, **When** the owner
   runs start, **Then** start reports the library unavailable with the reason and fix, still
   starts the local services, and exits non-zero.
3. **Given** any library state, **When** the owner runs start or stop, **Then** the library's
   contents are not changed. `stop` reports nothing for the library, since there is nothing
   running to stop.

---

### Edge Cases

- **Library location overlaps a live palace.** If the configured location is, is inside, or
  contains any campaign palace or anything under `~/.mempalace/palaces/`, setup refuses the
  library piece before building and status reports it (044 status 6). Setup never writes
  there.
- **Library keys missing from the authority file.** Setup does not edit the authority file
  (spec 002 FR-007). The library piece is refused as not configured (044 status 3), and the
  fix names the owner-kept authority source as where to add the keys.
- **Something unrelated occupies the location** (not a library). Setup refuses the piece and
  names the location, as for any occupied target (spec 002 FR-013).
- **Build interrupted or fails partway.** Nothing half-built is left at the location, so the
  next setup run builds again.
- **CampaignGenerator is not installed, or too old to have the `library` commands.** The
  library piece fails, naming the missing command; status reports it with the fix (re-run
  setup / update the pin).
- **Embedder changed in the authority file after build.** Status reports embedder mismatch
  (044 status 5) and names a rebuild as the fix. Setup does not rebuild on its own.
- **Large library.** The pilot built 50 books in about 3 minutes; the full library may take
  much longer. Setup shows that the build is in progress and does not time it out.

## Requirements *(mandatory)*

### Functional Requirements

**Setup**

- **FR-001**: Setup MUST treat the library palace as a data piece, alongside the restores of
  spec 002 FR-010a: attempted independently of the other data pieces, a failure reported
  without stopping them, and the overall exit non-zero when it fails.
- **FR-002**: Setup MUST bring up the library only after hypostasis's authority file and
  CampaignGenerator are in place (spec 002 FR-007, FR-008).
- **FR-003**: When no library exists at the configured location, setup MUST put one there
  by running CampaignGenerator's `library build`. Setup MUST NOT build the library itself.
- **FR-004**: Setup MUST NOT overwrite, refresh, rebuild, merge into, or delete an existing
  library, whatever its state.
- **FR-005**: Every `library` command setup, start or status runs MUST be given the
  embedding settings from the authority file's `env:` block explicitly, replacing any value
  the owner's shell already has. Because CampaignGenerator never overrides variables already
  set, this is the only thing that stops a stray shell setting from choosing the embedder.
- **FR-006**: Before building, setup MUST check that the library keys are present in the
  authority file, that the source folder can be read, that the embedder endpoint answers,
  and that the location does not overlap any live palace (FR-012). If any check fails, setup
  MUST refuse the piece, name the failed check and its fix, and not start the build.
- **FR-007**: After building, setup MUST confirm the result with `library status` before
  reporting the library as added. If the build or that check fails, setup MUST leave nothing
  at the location and report the failure with the build's own message.
- **FR-008**: Setup MUST NOT add or change the library's keys in the authority file (spec
  002 FR-007 still holds).

**Start / stop**

- **FR-009**: Start MUST check that the library is ready (`library status` returns 0) and
  that the embedder endpoint answers, and report the library as available or unavailable
  with the reason and its fix. A library problem MUST NOT stop the local services from
  starting, but MUST make start exit non-zero.
- **FR-010**: Start and stop MUST NOT change the library. Stop MUST NOT report on it.

**Status**

- **FR-011**: Status MUST run CampaignGenerator's `library status` and report: library
  present, built with the configured embedder, book count, and number of stale books, each
  problem with its fix (CampaignGenerator's own refusal message). Codes 3 and 4 mean the same
  as local-machine's "not configured" and "not set up" and MUST be passed through.
  Codes 5 (embedder mismatch) and 6 (live-palace refusal) MUST be mapped to 1 (failed),
  because local-machine already uses 5 for "a service is not running" and does not use 6;
  the printed message says which condition it is. Any other non-zero code also maps to 1.
  When other pieces also fail, status exits as spec 002 FR-019 already defines.

**Isolation**

- **FR-012**: The library's palace path, after `~` expansion and resolving links, MUST NOT
  be, be inside, or contain `~/.mempalace`, `~/src/campaigns`, or any campaign palace
  location named in the authority file. CampaignGenerator enforces this too (exit 6); setup
  MUST check it before calling the build so it can name the overlap, and status MUST report
  it.

**Shared rules (from spec 002, restated for the new piece)**

- **FR-013**: The library MUST be owned by, and every `library` command MUST run as, the
  owner account, never root. No library step needs administrator rights.
- **FR-014**: All library facts (location, selection, source folder, cache, embedding setup)
  MUST be read from hypostasis's authority file. `local-machine` configuration MUST NOT copy
  them (spec 002 FR-022).
- **FR-015**: Setup MUST report the library as already present, added, or failed, with the
  reason and fix on failure, in the same report as the other pieces.

### Key Entities

- **Library palace**: the semantic-search store of passages from the owner's converted
  DMsGuild books. File-backed, no running service. Kept apart from all campaign palaces.
  Its state, as `library status` reports it: ready, not configured, not built, embedder
  mismatch, refused (overlaps a live palace), plus book count and staleness.
- **Library keys** (in hypostasis's authority file): the `library:` block — `palace`,
  `work_dir`, optional `cache_dir`, `source_root` (currently the DMsGuild folder on the
  mounted Google Drive), `selection` (exactly one of `product_types`, `book_ids`,
  `all_converted`), optional `exclude_book_ids` — and the embedding settings in the `env:`
  block (currently the turbovec backend with Ollama `qwen3-embedding:0.6b`). The build also
  reads the existing top-level `rpg_library_db` key.
- **Embedder endpoint**: the external embedding service the library is built and searched
  with. Runs on another machine; nothing here starts or stops it.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a machine with no library, one setup run leaves a library that
  `library status` reports ready, with no manual step beyond what setup already requires.
- **SC-002**: Re-running setup on a machine with any existing library leaves it
  byte-for-byte unchanged, in 100% of runs.
- **SC-003**: A library built by setup always uses the authority file's embedder, even when
  setup is run from a shell with no or conflicting embedding settings.
- **SC-004**: For each of the four failing `library status` outcomes, status names the
  condition and a fix, and exits 3 (not configured),
  4 (not built) or 1 (embedder mismatch, live-palace refusal) when it is the only problem.
- **SC-005**: Status and start never change the library, and never ask for administrator
  rights.
- **SC-006**: No library is ever written at a location that overlaps a live palace.

## Assumptions

- CampaignGenerator spec 044's `library build`, `library refresh` and `library status`
  commands exist, with the exit codes listed in the Overview, and CampaignGenerator is
  installed at a pin that has them. The contract was read from CampaignGenerator's
  `specs/044-library-passage-search/contracts/cli.md` and `contracts/authority-config.md`,
  which on 2026-11-20 were uncommitted files in its working tree. Planning must re-check
  them once they are committed.
- The library's keys are seeded in the owner-kept authority source (the
  `hypostasis-config` repository), not by setup. That is how "hypostasis seeds and owns it"
  is read here, and it keeps spec 002 FR-007 (setup never edits the authority file) intact.
  The suggested default locations from the 044 contract are adopted there.
- Setup builds a missing library; it does not restore one from backup. The ruling says
  "setup runs `library build` when no library exists", and a rebuild is cheap compared with
  keeping a backup in step with the embedder. A backup restore can be added later as its own
  data piece.
- `library status` works out staleness and book count; this feature only shows them.
- Spec 044 also lists "restored from backup" as hypostasis's choice; this spec chooses
  build only (see above).
- The source folder and the embedder endpoint are reachable from this machine when setup
  runs. Mounting the drive and running the embedder are out of scope.
- Refreshing the library is the GM's job (`library refresh`), not setup's or start's.
