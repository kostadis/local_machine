# Data Model: Bring Up the DMsGuild Library Palace

## Authority additions (read-only; `src/hypostasis/authority.rs`)

`Authority` gains two optional fields. Absence is not a parse error: it is the library's
"not configured" state, reported by the library piece only, so a machine without the block
still passes every other piece.

| Field | Source key | Type | Rules |
|---|---|---|---|
| `env` | `env:` | ordered list of (name, value) strings | Missing → empty. Non-string values → invalid authority (exit 3). |
| `library` | `library:` | `Option<LibraryKeys>` | Missing → `None`. |
| `mempalace_root` | `data_roots.mempalace` | path | Optional; default `~/.mempalace`. Used only by the guard. |

`LibraryKeys` (only what local-machine needs; selection stays CampaignGenerator's to
validate):

| Field | Key | Rules |
|---|---|---|
| `palace` | `library.palace` | Required in the block; `~` expanded; absolute. |
| `source_root` | `library.source_root` | Required in the block; `~` expanded; absolute. |

A block without `palace` or `source_root` → `LibraryKeys` absent with a reason naming the
key (still "not configured", never a whole-authority failure).

## LibraryState (`src/hypostasis/library.rs`)

What `library status --json` says, run with the authority env.

| Variant | From | Carries | Mapped exit |
|---|---|---|---|
| `Ready` | exit 0 | `books`, `stale` (count), `index_bytes` | 0 |
| `NotConfigured` | exit 3 | CG message | 3 |
| `NotBuilt` | exit 4 | CG message | 4 |
| `EmbedderMismatch` | exit 5 | CG message | 1 |
| `LivePalace` | exit 6 | CG message | 1 |
| `Failed` | any other, or the command can't run | message / exit code | 1 |

`Ready` with `stale > 0` is still ready (exit 0); status prints a `note:` naming
`library refresh`.

## Preconditions (checked in this order; first failure wins)

| # | Precondition | Failure reason | Fix shown |
|---|---|---|---|
| P1 | Authority has a `library:` block with `palace` and `source_root` | `not configured (no library: block)` | add the block to the authority and to `authority_source` |
| P2 | `<venv>/bin/library` exists | `needs the library command from CampaignGenerator` | `local-machine hypostasis setup` (the library palace step reinstalls the components via `hypostasis install --config <authority>`, then re-checks; still missing is a failure: `'hypostasis install' did not provide it — check that CampaignGenerator declares the library script`). `status`/`start` only report it. |
| P3 | `rpg_library_db` exists | `needs the rpg library DB` | `local-machine hypostasis setup` (restores it) |
| P4 | `source_root` is a readable directory | `source folder <p> not readable` | `mount <p>, then run setup` |
| P5 | palace path passes the live-palace guard | `<palace> overlaps live palace <q>` | change `library.palace` in the authority |
| P6 | embedder answers | `embedder <url> not answering` | start the embedder, then run setup |

P1–P5 run in setup's `check()` (look only). P6 runs in `apply()` just before the build
(a network probe; status and start run it themselves).

## Library piece lifecycle (setup)

```text
           observe()
              │
   ┌──────────┼───────────────┬──────────────────────┐
 Ready /   NotBuilt &       NotBuilt &           NotConfigured / LivePalace /
 Mismatch    path missing    path exists          P1–P5 fail
 (any other) │                 │                  │
   │         ▼                 ▼                  ▼
   ▼      preconditions     refuse: occupied   refuse (failed piece)
 ok: present  │ ok
              ▼
          build (interactive) ──fail──► remove palace path (created this run) ► failed
             │ ok
          observe() ──not Ready──► remove palace path ► failed
             │ Ready
          changed: library palace (N books)
```

Setup never moves an existing library to another state (FR-004). A mismatched
library is reported `ok: library palace (present; see status)` by setup, and status reports
the problem. A live-palace overlap (exit 6) is a refusal, not "present": setup fails the
piece with `overlaps a live palace (<m>) — change library.palace in <authority>` (#9).

## System trait additions (`src/system/mod.rs`)

| Addition | Purpose |
|---|---|
| `Cmd.env: Vec<(String, String)>` + `Cmd::env(k, v)` | authority env on every `library` call (R2) |
| `System::http_status_at(host, port, path, timeout)` | remote embedder probe (R3); `http_status` becomes a wrapper |
| `System::canonical(path) -> Option<PathBuf>` | live-palace guard path resolution (R7) |

`FakeSystem` implements all three: records `env` with the command, and takes scripted
answers for probes and canonical paths.
