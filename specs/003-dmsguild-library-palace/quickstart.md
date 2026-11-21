# Quickstart: validate the library palace bring-up

Line formats and exit codes: `contracts/cli.md`. States and preconditions: `data-model.md`.

## Prerequisites

- Spec 002 setup has run on this machine (`local-machine hypostasis status` shows the
  software pieces `ok:`).
- CampaignGenerator at a commit that has the `library` command (spec 044), reinstalled so
  `~/.venvs/main/bin/library` exists.
- A `library:` block in `~/src/hypostasis-config/hypostasis.yaml` and in
  `~/.config/hypostasis/hypostasis.yaml` (defaults from 044's `authority-config.md`). For a
  quick run, use a small selection (`book_ids: [...]`, a few books).
- Google Drive mounted at `/mnt/g`, and the Ollama embedder up.

## Automated

```bash
cargo test                     # unit tests incl. FakeSystem scenarios below
cargo clippy -- -D warnings
```

FakeSystem scenarios that must pass: build when missing; existing library untouched (no
`library build` call recorded); shell `MEMPALACE_*` overridden (recorded `Cmd.env`); each
precondition P1–P6 refuses with its fix; build failure removes only the palace path;
live-palace guard refuses before any build; status exit precedence; start starts services
even when the library is down; no prompt in any library path (`FakeSystem::prompted()`).

## Manual, on the machine

1. **Build** (US1): with no palace at `library.palace`, run
   `MEMPALACE_BACKEND=chroma local-machine hypostasis setup`. Expect `changed: library
   palace (N books)`. Then `~/.venvs/main/bin/library status --json` shows
   `embedder_match: true` (the shell value did not win, SC-003).
2. **Idempotent** (SC-002): record `find <palace> -type f -exec sha256sum {} + | sort`,
   re-run setup, expect `ok: library palace (N books)` and identical checksums.
3. **Status** (US2): `local-machine hypostasis status; echo $?` → `ok: library palace` and
   0 (if services run). Touch one source book's JSON → `note: ... 1 books stale`.
4. **Embedder down** (US3): stop Ollama, run `local-machine hypostasis start; echo $?` →
   services `ok:`, library `unavailable — embedder ... not answering`, exit 1.
5. **Not configured**: on a copy of the authority without `library:` (point
   `hypostasis.yaml` `authority` at it via a scratch `--config-dir`), status prints
   `missing: library palace (not configured ...)` and exits 3 when nothing else is missing.
6. **Live-palace guard** (SC-006): in the scratch authority set
   `library.palace: ~/.mempalace/palaces/x`; setup refuses before building and nothing is
   created under `~/.mempalace`.

## Results

- 2026-11-20 — Automated: `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --check` all pass.
- 2026-11-20 — Manual steps 1–6: **not run.** `~/.venvs/main/bin/library` does not exist
  yet (CampaignGenerator must be reinstalled at a commit with the 044 `library` command),
  and the live authority `~/.config/hypostasis/hypostasis.yaml` has no `library:` block
  (added to `hypostasis-config` on branch `003-library-block`, not yet merged or copied).
- 2026-11-20 — Review: one pass (re-run once, scoped, after the first run was cut off). No
  confirmed findings. Follow-ups filed: local_machine #2–#6, CampaignGenerator #671.
- 2026-11-20 — Manual step 1 (build): first build with a narrow selection, 322 books /
  6,359 passages in 2,760 s, `embedder_match: true` with `MEMPALACE_BACKEND=chroma` in the
  shell (SC-003). Step 2: identical checksums on re-run (SC-002). Step 3: status exit 0.
  Step 4 skipped (the embedder is shared and in use). Step 5: exit 3.
- 2026-11-21 — Selection widened to `all_converted: true` (owner's decision). `library
  refresh` added 287 books: 609 books / 17,340 passages, 0 failed, 2,297 s. No duplicate
  drawers (no repeated source_file + chunk_index). `local-machine hypostasis status` →
  `ok: library palace (609 books, 1224 MB)`, no stale note; `--config` passed (#8 verified).
  The first refresh attempt ran at ~26 files/min: every per-file `where=source_file`
  lookup in turbovecdb was a full table scan. Fixed in turbovecdb #126 (expression
  index), after which mining ran at ~359 files/min.
- 2026-11-21 — Step 4 (embedder down), on a scratch authority with
  `MEMPALACE_EMBEDDING_ENDPOINT: http://127.0.0.1:1`, so the live ollama is never stopped:
  `start` → services `ok:`, `error: library palace: unavailable — embedder
  http://127.0.0.1:1 not answering`, exit 1. **Pass.**
- 2026-11-21 — Step 6 (live-palace guard), on a scratch authority with
  `library.palace: ~/.mempalace/palaces/x`: nothing built, nothing created under
  `~/.mempalace` (SC-006 holds); `status` → `error: ... overlaps a live palace`, exit 1.
  **But setup** printed `ok: library palace (present; ...)` and exited 0 instead of
  refusing: CampaignGenerator's exit 6 is read as an existing library. Filed as #9.
