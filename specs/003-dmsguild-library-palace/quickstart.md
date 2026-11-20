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
