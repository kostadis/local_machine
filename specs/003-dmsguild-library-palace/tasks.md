---
description: "Task list for 003-dmsguild-library-palace"
---

# Tasks: `local-machine hypostasis` — Bring Up the DMsGuild Library Palace

**Input**: `specs/003-dmsguild-library-palace/` — plan.md, spec.md, research.md (R1–R13),
data-model.md, contracts/cli.md, quickstart.md

**Tests**: Included. Constitution VI requires tests asserting `FakeSystem::prompted()` on
every action path, and quickstart.md lists the FakeSystem scenarios that must pass. Write
each story's tests first and see them fail.

**Conventions**: tests use `FakeSystem` (`src/system/fake.rs`: `respond`, `respond_prefix`,
`respond_fn`, `set_file`, `set_dir`, `calls`, `prompted`). Integration tests go in the
existing `tests/hypostasis_*.rs` files, sharing `tests/support/`. The `library` binary in
tests is `/home/kostadis/.venvs/main/bin/library` (fixture venv). Line formats and exit
codes are in `contracts/cli.md`; do not invent new wording.

## Execution model

Per plan.md "Execution Model": Opus orchestrates and commits; every implementation task
below is done by a Sonnet subagent with a self-contained brief; one `/code-review` pass at
the end (T038–T039); anything after that becomes a GitHub issue (T040), not a fix here.

## Phase 1: Setup

- [ ] T001 Add `serde_json` (same major as the ecosystem default, `1`) to `[dependencies]` in Cargo.toml; `cargo build` passes
- [ ] T002 [P] Extend tests/fixtures/authority.yaml with the `env:` block (the four `MEMPALACE_*` keys from `~/src/hypostasis-config/hypostasis.yaml`), `data_roots.mempalace: ~/.mempalace`, and a `library:` block per CampaignGenerator 044 `authority-config.md` (`palace: ~/.local/share/hypostasis/library/palace`, `work_dir`, `source_root: "/mnt/g/My Drive/DriveThru/Dungeon Masters Guild"`, `selection: {book_ids: [2443, 2475]}`); existing tests still pass
- [ ] T003 [P] Create tests/fixtures/authority-no-library.yaml: a copy of tests/fixtures/authority.yaml without the `library:` block

## Phase 2: Foundational (blocks all stories)

- [ ] T004 Add `pub env: Vec<(String, String)>` to `Cmd` in src/system/mod.rs, default empty in `Cmd::new`, builder `pub fn env(mut self, k: &str, v: &str) -> Self`, and `pub fn envs(self, pairs: &[(String, String)]) -> Self`; in `RealSystem::run` call `command.env(k, v)` for each pair (research R2). `Cmd::line()` stays unchanged (env is not shown)
- [ ] T005 Add `fn http_status_at(&self, host: &str, port: u16, path: &str, timeout: Duration) -> Option<u16>` to the `System` trait in src/system/mod.rs; implement it in `RealSystem` by resolving `host:port` with `ToSocketAddrs` and reusing the existing HTTP/1.0 request code (Host header = `host:port`); make `http_status` call it with `"127.0.0.1"` (research R3)
- [ ] T006 Add `fn canonical(&self, path: &Path) -> Option<PathBuf>` to the `System` trait in src/system/mod.rs; `RealSystem` uses `std::fs::canonicalize(path).ok()` (research R7). Implement it, and forward `http_status_at`, in the `NoPrompt` wrapper in src/system/action.rs
- [ ] T007 In src/system/fake.rs: record each `Cmd`'s env alongside its line (`pub fn env_of(&self, line_prefix: &str) -> Option<Vec<(String, String)>>` returns the env of the last call starting with the prefix); add `set_http_at(host, port, answers)` mirroring `set_http`; implement `canonical` as identity for paths that exist in the fake (files or dirs), else `None`, with `set_canonical(from, to)` to script links
- [ ] T008 Unit tests in src/system/mod.rs (module tests) for `Cmd::env`/`envs` building, and in src/system/fake.rs for `env_of`, `set_http_at`, `canonical`
- [ ] T009 In src/hypostasis/authority.rs add to `Authority`: `pub env: Vec<(String, String)>` (from `env:`, order preserved, missing → empty, non-string value → `invalid("env.<KEY> must be a string")`), `pub mempalace_root: PathBuf` (`data_roots.mempalace`, `~` expanded, default `<home>/.mempalace`), and `pub library: Result<LibraryKeys, String>` where `pub struct LibraryKeys { pub palace: PathBuf, pub source_root: PathBuf }` (`~` expanded, must be absolute). `Err` reasons: `"no library: block"`, `"library.palace is missing"`, `"library.source_root is missing"`, `"library.<key> must be an absolute path"`. A bad library block MUST NOT fail the whole parse (data-model "Authority additions")
- [ ] T010 Unit tests in src/hypostasis/authority.rs: fixture parses env in order and library keys; authority-no-library.yaml gives `library == Err("no library: block")`; `library.palace: relative` gives the absolute-path error; `env: {X: 1}` fails the parse; missing `data_roots.mempalace` defaults to `/home/kostadis/.mempalace`
- [ ] T011 Create src/hypostasis/library.rs and add `pub mod library;` to src/hypostasis/mod.rs. Implement: `pub enum LibraryState { Ready { books: u64, stale: usize, index_bytes: u64 }, NotConfigured(String), NotBuilt(String), EmbedderMismatch(String), LivePalace(String), Failed(String) }`; `pub fn exit_code(&self) -> u8` (Ready 0, NotConfigured 3, NotBuilt 4, others 1 — spec clarification); `pub fn library_cmd(a: &Authority, sub: &[&str]) -> Cmd` = `a.bin("library")` + args + `envs(&a.env)`; `pub fn observe(sys, a) -> LibraryState` running `library status --json`, mapping exit 0/3/4/5/6/other and taking the message from stderr (trimmed), else stdout; on 0 parse JSON `books`, `stale` (array length), `index_bytes` with serde_json, a parse error → `Failed("library status --json: <err>")`; binary missing (`path_kind` Missing) → `Failed("needs the library command from CampaignGenerator")` without running it (research R1, R4)
- [ ] T012 In src/hypostasis/library.rs implement `pub fn guard(sys, ctx, a, keys) -> Result<(), String>` (research R7): resolve the palace via `canonical`, or for a missing path the nearest existing ancestor's canonical form joined with the remainder; resolve each protected root the same way: `a.mempalace_root`, `<home>/src/campaigns`, every `a.campaigns` entry. Refuse with `"<palace> overlaps live palace <root>"` if palace == root, palace starts_with root, or root starts_with palace
- [ ] T013 In src/hypostasis/library.rs implement `pub fn embedder(a) -> Option<(String /*url*/, String /*host*/, u16, &'static str /*path*/)>` parsing `MEMPALACE_EMBEDDING_ENDPOINT` from `a.env` (`http://host:port`, default port 80; `https://` → None meaning unknown), path `/api/tags` when `MEMPALACE_EMBEDDING_PROVIDER` is `ollama`, else `/`; and `pub fn embedder_answers(sys, a) -> Option<bool>` (None = cannot probe) using `http_status_at` with a 3 s timeout; ollama requires 2xx, others any status
- [ ] T014 Unit tests in src/hypostasis/library.rs for T011–T013: each exit code maps to its variant and `exit_code`; JSON parse incl. `stale: ["a","b"]` → 2; every `observe` call's recorded env equals the authority env (`env_of`); guard refuses palace inside `~/.mempalace/palaces/x`, equal to a campaign, containing `~/src/campaigns`, and through a scripted symlink (`set_canonical`); guard passes the default palace; embedder URL parsing incl. https → None; probe 200 → Some(true), no answer → Some(false)

**Checkpoint**: foundation builds, `cargo test` green; no behaviour change visible to the CLI yet.

## Phase 3: User Story 1 — Setup leaves a usable library palace (P1) 🎯 MVP

**Goal**: setup builds a missing library with the authority's embedder and never touches an existing one.

**Independent test**: FakeSystem setup run with no palace → one `library build` call with the authority env, then `changed: library palace (N books)`; second run → no build call, `ok:`.

### Tests (write first, see them fail)

- [ ] T015 [P] [US1] In tests/hypostasis_setup.rs: missing library (status exit 4, palace path absent, source_root dir present, rpg_library_db file present, embedder `set_http_at` 200) → calls contain `library build` (interactive) then `library status --json`; output contains `note: library palace: 2 books` then `changed: library palace`; `env_of("…/library build")` equals the fixture env even when the test process env has `MEMPALACE_BACKEND=chroma` (SC-003); `prompted()` false
- [ ] T016 [P] [US1] In tests/hypostasis_setup.rs: existing library (status 0) → no `library build` call, `ok: library palace (2 books)`; status 5 and status 6 → no build, `ok: library palace (present; see 'local-machine hypostasis status')` (FR-004, SC-002)
- [ ] T017 [P] [US1] In tests/hypostasis_setup.rs, one test per precondition P1–P6 (data-model table): each refuses with its reason and fix from contracts/cli.md, makes no `library build` call, still runs the other restores, and setup exits 1. Plus: status 4 with the palace path present → `occupied (not a built library); not touched`, no build, nothing removed
- [ ] T018 [P] [US1] In tests/hypostasis_setup.rs: build exits 1 after creating the palace dir → the dir is removed and the line reads `build failed (exit 1); removed the partial palace`; build exits 0 but the follow-up status is 4 → dir removed, failed; a palace path that existed before the run is never removed (assert no `remove_dir_all` on it in any failure test)

### Implementation

- [ ] T019 [US1] In src/hypostasis/library.rs implement `pub struct LibraryPalace<'a> { ctx: &'a Context, created: bool }` implementing `Step` (src/system/action.rs): `name()` = `"library palace"`; `notice()` = None; `continue_on_failure()` = true; `check()` loads the authority (None → `NeedsChange("needs the authority file")`), then P1 (keys) → `NeedsChange("not configured (<reason>)")`, P2 binary, `observe()`: Ready → `DoneWith("<books> books")`, EmbedderMismatch/LivePalace → `DoneWith("present; see 'local-machine hypostasis status'")`, NotBuilt → P3–P5 then `NeedsChange("not built")`, NotConfigured/Failed → `NeedsChange(msg)`
- [ ] T020 [US1] In `LibraryPalace::apply` (src/hypostasis/library.rs): re-run P1–P5 and bail with the first failure's reason + fix (contracts/cli.md wording; P1 fix names `ctx.cfg.authority` and `ctx.cfg.authority_source`); if the palace path exists → bail `"<palace> occupied (not a built library); not touched"`; P6 `embedder_answers` Some(false) → bail naming the URL, None → `say!("unknown: embedder <url> (cannot probe https); building anyway")`; `say!("note: library palace: building from <source_root>; this can take a while")` (the runner has already printed `adding: library palace`); set `created = true`; run `library_cmd(a, &["build"]).interactive()`; non-zero → remove the palace path if it exists and bail `"build failed (exit <n>); removed the partial palace"`; then `observe()` must be Ready, else remove and bail `"built but library status says: <msg>; removed the partial palace"`; on success `say!("note: library palace: <books> books")` (the runner then prints `changed: library palace`; it takes no note, src/system/action.rs:187)
- [ ] T021 [US1] Implement `rollback()` on `LibraryPalace` in src/hypostasis/library.rs: remove the palace path only when `self.created` is true and the guard passed; never otherwise
- [ ] T022 [US1] In src/hypostasis/setup.rs append `Box::new(LibraryPalace::new(ctx))` as the last entry of `restore_steps` (after the palace restores, research R10); update the module doc comment
- [ ] T023 [US1] Make T015–T018 pass; `cargo test` and `cargo clippy -- -D warnings` green

**Checkpoint**: US1 complete and demonstrable with FakeSystem; status/start unchanged apart from status now listing the step via `restore_steps` (fixed properly in US2).

## Phase 4: User Story 2 — Status reports readiness and the fix (P1)

**Goal**: status prints one library line from `library status --json` and exits per research R9.

**Independent test**: put the fake in each `LibraryState`; check the line and exit code; nothing changes, no prompt.

### Tests

- [ ] T024 [P] [US2] In tests/hypostasis_status.rs, one test per state with every other piece present and all services running: Ready → `ok: library palace (2 books, 46 MB)`, exit 0; Ready with 3 stale → plus `note: library palace: 3 books stale — run '<venv>/bin/library refresh'`, exit 0; NotConfigured → exit 3; NotBuilt → exit 4; EmbedderMismatch → exit 1; LivePalace → exit 1; other exit 7 → exit 1. Each asserts `prompted()` false and no call other than `library status --json` to the library binary
- [ ] T025 [P] [US2] In tests/hypostasis_status.rs, precedence: another piece missing + library NotConfigured → exit 4; library EmbedderMismatch + services stopped → exit 1; library Ready + services stopped → exit 5; authority missing → no library call, existing `missing: restores` line unchanged

### Implementation

- [ ] T026 [US2] In src/hypostasis/status.rs stop reporting `LibraryPalace` through the generic `Pieces::report` (filter it out of `restore_steps` output, or have `restore_steps` take a flag), and add a library section after the restore lines that calls `library::observe` (via `NoPrompt`) and prints the lines from contracts/cli.md `## status`; `index_bytes` shown as MB rounded; P1/P2 failures reported as `missing:` without running the binary
- [ ] T027 [US2] In src/hypostasis/status.rs compute the exit per research R9: `pieces.missing > 0` → 4; else library `exit_code()` if non-zero; else services < 3 → 5; else 0. Update the header line so a library-only problem reads `hypostasis: set up; library palace not ready; N of 3 services running`
- [ ] T028 [US2] Make T024–T025 pass; `cargo test`, clippy green

**Checkpoint**: US1 + US2 make the feature usable end to end (build, then see it).

## Phase 5: User Story 3 — Start confirms the library can be searched (P2)

**Goal**: start reports the library available or not after starting services, never blocking them.

**Independent test**: ready library + embedder up → `ok: library palace (available; …)`, exit 0; embedder down → services still start, library error, exit 1.

### Tests

- [ ] T029 [P] [US3] In tests/hypostasis_services.rs: Ready + embedder 200 → services started, `ok: library palace (available; embedder <url> answering)`, exit 0; Ready + embedder no answer → services started, `error: library palace: unavailable — embedder <url> not answering`, exit 1; NotBuilt → services started, exit 4; a service fails + library NotBuilt → exit 1; library absent from the authority → exit 3 after services start; `prompted()` false; no `library build`/`refresh` call ever
- [ ] T030 [P] [US3] In tests/hypostasis_services.rs: `stop` makes no call to the library binary and prints no library line

### Implementation

- [ ] T031 [US3] In src/hypostasis/start.rs, after the service loop in `run`, add `library_line(ctx, sys, &authority) -> u8` (in src/hypostasis/library.rs) that checks P1, `observe`, and `embedder_answers`, prints per contracts/cli.md `## start`, and returns the mapped code (embedder down → 1, https → `unknown:` line and treated as answering). `run` returns: any service failed → existing FAILED error; else non-zero library code → `ExitError::new(code, "start: library palace unavailable")`; else 0. Services are never skipped because of the library (FR-009)
- [ ] T032 [US3] Make T029–T030 pass; `cargo test`, clippy green

## Phase 6: Polish & Cross-Cutting

- [ ] T033 [P] Add `library:` block (defaults from CampaignGenerator 044 `contracts/authority-config.md`, `selection.product_types: [adventure, setting, gm_aid]`) to `~/src/hypostasis-config/hypostasis.yaml` on a new branch `003-library-block` in that repo; commit there (research R11). Do not edit `~/.config/hypostasis/hypostasis.yaml` from code; the owner copies the block by hand or re-seeds
- [ ] T034 [P] Update specs/002-hypostasis-setup/contracts/cli.md `## Exit codes` with a pointer: "status may also exit 3 for a library-only problem (spec 003)"; and README/`--help` text for `hypostasis status` in src/cli.rs if it lists what status covers
- [ ] T035 Run the full quickstart.md "Automated" section: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`
- [ ] T036 Manual quickstart.md steps 1–6 on the machine, once CampaignGenerator 044 is merged and `~/.venvs/main/bin/library` exists; record results (pass/fail per step) at the bottom of specs/003-dmsguild-library-palace/quickstart.md. Blocked on 044 — skip and say so if it is not merged
- [ ] T037 Re-check contracts/cli.md and research.md against CampaignGenerator 044's committed `contracts/cli.md` and `authority-config.md`; note any drift in research.md

## Phase 7: Review & Close

- [ ] T038 Run `/code-review` once on the branch diff against `main` (Opus); list the confirmed findings
- [ ] T039 Fix the confirmed findings from T038 in one pass, by Sonnet subagent; `cargo test`, clippy, fmt green; no second review pass
- [ ] T040 File every remaining finding, nit or follow-up (including any T037 drift and a skipped T036) as a GitHub issue on kostadis/local_machine via the GitHub MCP tools, each linking issue #1; then merge the branch into `main`

## Dependencies & Execution Order

- Phase 1 → Phase 2 → stories. T004–T007 before T008; T009 before T010–T013; T011–T013 before T014.
- **US1** (Phase 3) depends on Phase 2 only.
- **US2** depends on Phase 2; T026 touches the `restore_steps` list US1 extends, so do US2 after T022.
- **US3** depends on Phase 2 only (uses `observe`, `embedder_answers`); independent of US1/US2 files except src/hypostasis/library.rs (append-only).
- Polish after the stories it documents; T033 can be done any time (other repo); T036 blocked on 044.

## Parallel Examples

- Phase 1: T002 and T003 together.
- Phase 2: T005 and T006 touch the same file (src/system/mod.rs), so serial; T009 (authority.rs) can run in parallel with T004–T007.
- US1: T015, T016, T017, T018 written in parallel (same file, separate test fns — coordinate or append).
- US2 tests T024/T025 and US3 tests T029/T030 can be written in parallel with each other.
- T033 and T034 in parallel with anything.

## Implementation Strategy

1. **MVP = Phases 1–3 (US1)**: setup can build the library safely. Stop and validate with FakeSystem.
2. Add **US2** so the owner can see the library's state and exit codes — the feature is complete for daily use.
3. Add **US3** for the one-command "is everything up" check.
4. Polish, then the manual run once CampaignGenerator 044 is merged.
5. One `/code-review` pass and its fixes (T038–T039); everything later is an issue (T040).
