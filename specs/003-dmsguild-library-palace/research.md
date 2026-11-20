# Research: Bring Up the DMsGuild Library Palace

Phase 0 for `specs/003-dmsguild-library-palace/plan.md`. Every unknown from Technical
Context is resolved here.

## R1. Where the `library` command comes from

- **Decision**: Run `<authority venv>/bin/library` (`Authority::bin("library")`). It is
  installed by hypostasis's `Components` piece, which installs CampaignGenerator into the
  authority venv. If the file is missing, the library piece reports "needs the `library`
  command from CampaignGenerator" and the fix is `local-machine hypostasis setup` (the
  Components piece reinstalls at the pin).
- **Rationale**: CampaignGenerator's `pyproject.toml` already declares
  `library = "pipelines.library.cli:main"`, but `~/.venvs/main/bin` does not contain it yet
  (checked 2026-11-20). An editable install only adds the script when it is reinstalled, so a
  missing binary is a real, expected state.
- **Alternatives**: `python -m pipelines.library.cli` (rejected: bypasses the declared entry
  point, and depends on CampaignGenerator's internal layout); `PATH` lookup (rejected: the
  shell may have another venv first).

## R2. Making the authority's embedder win over the shell (FR-005)

- **Decision**: Add `env: Vec<(String, String)>` to `system::Cmd`. `RealSystem::run` calls
  `Command::env` for each entry. Every `library` call is given every key of the authority's
  `env:` block. `FakeSystem` records the env so tests can assert it.
- **Rationale**: The 044 contract merges `env:` into the build environment "never overriding
  vars already set". If local-machine sets the variables on the child process, they are
  already set to the authority's values when CampaignGenerator reads them, so a stray
  `MEMPALACE_*` in the owner's shell can't get through. That is the pilot's failure mode
  (silent fallback to chroma + minilm).
- **Alternatives**: `env_clear()` and pass only the authority env (rejected: loses `PATH`,
  `HOME`, locale; too blunt); `env -i ...` wrapper (rejected: same, and shells out).

## R3. Probing the embedder endpoint (FR-006, FR-009)

- **Decision**: Add `System::http_status_at(host, port, path, timeout) -> Option<u16>`,
  generalising the existing localhost-only `http_status` (which becomes a thin wrapper).
  Probe `GET /api/tags` on the host and port of `MEMPALACE_EMBEDDING_ENDPOINT`, with a 3 s
  timeout. Any 2xx means it answers. When `MEMPALACE_EMBEDDING_PROVIDER` is not `ollama`,
  probe `GET /` and accept any HTTP response.
- **Rationale**: The endpoint (`http://192.168.56.1:11434`) is on another machine; the
  current probe hard-codes `127.0.0.1`. Ollama answers `/api/tags` cheaply without loading a
  model. Same hand-rolled HTTP/1.0 approach as today, so no new crate.
- **Alternatives**: `curl` subprocess (rejected: adds a package dependency and a subprocess
  for one GET); embedding a test string (rejected: slow, loads the model, and is
  CampaignGenerator's job).
- **Limit**: An `https://` endpoint can't be probed this way. It is reported as `unknown:`,
  not failed, and the build goes ahead (CampaignGenerator will fail clearly if it is down).

## R4. Reading library state

- **Decision**: One function, `library::observe(ctx, sys, authority) -> LibraryState`, runs
  `library status --json` with the authority env and maps the exit code:
  0 → `Ready{books, stale, index_bytes}`; 3 → `NotConfigured(msg)`; 4 → `NotBuilt(msg)`;
  5 → `EmbedderMismatch(msg)`; 6 → `LivePalace(msg)`; anything else → `Failed(msg)`.
  `msg` is CampaignGenerator's refusal text (stderr, else stdout), which already names the
  fix. On exit 0 the JSON gives `books`, `stale` (list → count), `embedder_match`,
  `index_bytes`. Setup, start and status all use it.
- **Rationale**: One reading of the contract in one place. The refusal messages are
  CampaignGenerator's to word (cli.md "Refusal messages"), so local-machine shows them
  rather than inventing its own.
- **Alternatives**: Each action calling the command itself (rejected: three places to drift).

## R5. "Missing" vs "occupied" before a build

- **Decision**: The library piece needs building only when `library status` returns 4 **and**
  the resolved palace path does not exist. Returns 4 with something at the path → refuse as
  occupied (spec 002 FR-013), naming the path; setup never builds over it.
- **Rationale**: FR-004 forbids touching an existing library, and a half-built or unrelated
  directory is not ours to delete. Only a path setup itself just created may be removed.

## R6. No half-built library (FR-007)

- **Decision**: The step records that the palace path was missing before the build. If
  `library build` exits non-zero, or the follow-up `library status` is not 0, the step
  removes the palace path (only that path, only because it did not exist before this run)
  and reports the build's exit code and the last 20 lines of its output.
- **Rationale**: CampaignGenerator builds in place; it has no `.partial` rename like the DB
  restores. Removing a path this run created is the same promise as `restore_db`.
- **Guard**: Removal is refused unless the path passed the live-palace guard (R7) before the
  build, so a mis-set key can never make local-machine delete a campaign palace.
- **Alternatives**: Building into a temp path and renaming (rejected: the palace path is
  read from the authority by CampaignGenerator; there is no flag to redirect it, and adding
  one is a 044 contract change).

## R7. The live-palace guard (FR-012)

- **Decision**: Resolve the palace path the way 044 does (`~` expanded, links resolved; for
  a path that does not exist yet, resolve its nearest existing ancestor and re-append the
  rest). Refuse if it equals, is inside, or contains any of: `data_roots.mempalace` from the
  authority (default `~/.mempalace` when absent), `~/src/campaigns`, and every
  `data_roots.campaigns` entry. Add `System::canonical(path) -> Option<PathBuf>`
  (`std::fs::canonicalize`) for this.
- **Rationale**: 044 refuses with exit 6, but only when its own command runs. Checking first
  lets setup name the overlap and refuse before any build (FR-006), and protects the
  rollback in R6. `data_roots.mempalace` exists in the live authority (`~/.mempalace`) and
  is the more precise version of the hard-coded path in the spec.

## R8. Build output and duration

- **Decision**: Run `library build` with `Cmd::interactive()`, so CampaignGenerator's
  progress goes straight to the owner's terminal, with no timeout. Print
  `adding: library palace (building <N> books; this can take a while)` first, where N is not
  known in advance, so the line just says "building from <source_root>".
- **Rationale**: The pilot took 186 s for 50 books; the full library is much larger. A
  silent multi-minute step looks hung. Interactive mode loses captured output, but
  CampaignGenerator already printed it to the terminal, so the error line only needs the
  exit code.
- **Then**: `library status --json` (captured) confirms the result (FR-007).

## R9. Exit codes

- **Decision** (spec clarification 2026-11-20): library 3 → 3, 4 → 4, everything else → 1.
  - **status**: other pieces missing → 4 (unchanged); else a library problem → its mapped
    code; else services not all running → 5; else 0. A library problem outranks stopped
    services because `start` can't fix it.
  - **start**: services start first and are never blocked by the library (FR-009). Then the
    library check. Exit: any service failed → 1; else library mapped code; else 0.
  - **setup**: the library is an independent data step (`continue_on_failure`), so a failure
    makes setup exit 1, the same as a failed restore.
- **Note**: Exit 3 from status for a library-only problem is new. Until now status never
  returned 3 after its gate passed. It means the same thing ("not configured"), so it is
  consistent with the table.

## R10. Preconditions and ordering in setup

- **Decision**: The library step goes last in `restore_steps` (after the rpg-library DB and
  palace restores), because the build reads `rpg_library_db`. Its `apply` checks, in order,
  and refuses naming the first that fails: library keys present (authority has a `library:`
  block); `library` binary present; `rpg_library_db` exists; `source_root` is a readable
  directory; live-palace guard; embedder answers. Only then build.
- **Rationale**: Each refusal names one fix. These checks only look, so they are safe in
  `check()` too, which lets `status` give the same reason.

## R11. Where the keys live (FR-008)

- **Decision**: local-machine never writes the `library:` block. The owner adds it to the
  authority source (`~/src/hypostasis-config/hypostasis.yaml`, which `authority_source` in
  `hypostasis.yaml` names). Setup only copies that file when the authority is missing (spec
  002 FR-007). When the block is missing from an existing authority, the fix line names
  both: "add a `library:` block to <authority>, (and to <authority_source> so new machines
  get it)".
- **Rationale**: The 044 contract's "hypostasis seeds and owns it" is satisfied by
  hypostasis-config, which is hypostasis's own repo. Editing YAML would mean setup rewrites
  a hand-edited file, which spec 002 forbids.
- **Dependency outside this repo**: a commit to `hypostasis-config` adding the block with
  the defaults from 044's `authority-config.md`. Tracked as a task, done on a branch there.

## R12. Config-sharing review (Constitution II)

- **New local-machine config values**: none. Every library value (palace, work_dir,
  cache_dir, source_root, selection, exclude_book_ids, embedding env) is owned by
  hypostasis's authority file and is only read, never copied (spec FR-014, constitution II
  last bullet).
- **Compared with `export.yaml`, `machine.yaml`, `hypostasis.yaml`**: no overlap; nothing to
  move to `machine.yaml`. `backups:` gets no library entry, since there is no restore (spec
  Assumptions).
- **Result**: no sharing opportunity; no refactor.

## R13. Parameters (Constitution III)

- No new flags. `setup | start | status` keep only `--config-dir`.
