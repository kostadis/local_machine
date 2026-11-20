# CLI Contract: library palace in `local-machine hypostasis`

Extends `specs/002-hypostasis-setup/contracts/cli.md`. Grammar, parameters, line prefixes
and the owner gate are unchanged. No action needs root for the library; no new prompt.

The downstream contract (CampaignGenerator `library` CLI and authority keys) is
CampaignGenerator `specs/044-library-passage-search/contracts/cli.md` and
`authority-config.md`.

## Every `library` call

- Program: `<authority venv>/bin/library`.
- Arguments: `<subcommand...> --config <authority>`, where `<authority>` is the path of the
  authority file local-machine read (the `authority:` path in its config), on every call.
- Environment: the parent environment, plus every `env:` key from the authority set on the
  child (overriding the shell's value).
- Captured output, except `library build`, which inherits the terminal.

## `setup`

The piece `library palace` is the last data step, after the restores.

```text
ok: library palace (1,284 books)                       # already built and ready
ok: library palace (present; see 'local-machine hypostasis status')   # present, not ready
adding: library palace
note: library palace: building from /mnt/g/My Drive/DriveThru/Dungeon Masters Guild; this can take a while
...CampaignGenerator progress...
note: library palace: 1284 books
changed: library palace
error: library palace: not configured (no library: block in ~/.config/hypostasis/hypostasis.yaml) — add it there and to ~/src/hypostasis-config/hypostasis.yaml
error: library palace: embedder http://192.168.56.1:11434 not answering — start it, then run 'local-machine hypostasis setup'
error: library palace: ~/.local/share/hypostasis/library/palace occupied (not a built library); not touched
error: library palace: build failed (exit 1); removed the partial palace
  <last 20 lines are already on the terminal>
```

Exit: as spec 002 (a failed library piece, like a failed restore, makes setup exit 1).

## `status`

One line for the library, after the restore lines:

```text
ok: library palace (1,284 books, 46 MB)
note: library palace: 3 books stale — run '~/.venvs/main/bin/library refresh'
missing: library palace (not built) — run 'local-machine hypostasis setup'
missing: library palace (not configured: <CG message>) — <CG fix>
error: library palace: embedder mismatch (<CG message>) — <CG fix>
error: library palace: overlaps a live palace (<CG message>) — change library.palace in <authority>
error: library palace: library status failed (exit <n>): <message>
```

Exit precedence: any other piece missing → 4; else library → 3 / 4 / 1 (3 and 4 passed
through, 5, 6 and others → 1); else services not all running → 5; else 0.

## `start`

Services are started first, unchanged. Then:

```text
ok: library palace (available; embedder http://192.168.56.1:11434 answering)
error: library palace: unavailable — not built — run 'local-machine hypostasis setup'
error: library palace: unavailable — embedder http://192.168.56.1:11434 not answering
```

Exit: any service failed → 1; else library mapped code (embedder down → 1); else 0.

## `stop`

No library line, no change.
