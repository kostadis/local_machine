# Quickstart & Validation: `local-machine export`

Run on the Ubuntu machine unless marked **[Windows]** or **[Unix VM]**. Commands, outputs
and exit codes: [contracts/cli.md](contracts/cli.md). Config: [contracts/export-config.md](contracts/export-config.md).

## Prerequisites

- `local-machine` built and on `PATH` (`cargo install --path .`).
- Owner can `sudo`.
- `~/.config/local-machine/export.yaml` with at least `allowed_network` set to the home LAN.
- A Windows 10/11 desktop and one Linux VM on that network; the VM has `cifs-utils`.

## Validation scenarios

| ID | Steps | Expected | Spec |
|----|-------|----------|------|
| V1 | `local-machine export status` before setup | `not set up`, exit 4 | US3, FR-005 |
| V2 | `local-machine export start` before setup | Refuses, names `export setup`, exit 4, nothing changed | US1-2, FR-005 |
| V3 | `local-machine export setup`; enter password; run it again | First run: `changed:` lines, share still not available. Second run: only `ok:` lines, no prompt | US1-1, FR-002, FR-004 |
| V4 | `local-machine export start`; `export status` | `available`, exit 0, prints Windows and Unix addresses | US1-3, US3-1, FR-007 |
| V5 | **[Windows]** Map the printed `\\host\share` with the owner's credentials; open a folder as an Obsidian vault; create, edit, rename, delete a note | All changes appear on Ubuntu; `ls -l` shows owner; Obsidian picks up a file touched on Ubuntu without reopening | US1-4/5, FR-011–013, SC-002 |
| V6 | **[Unix VM]** Mount with the printed command; create a file; `chmod +x` a script on Ubuntu, edit it from the VM | File owned by owner on Ubuntu; mode bits shown on VM; edit leaves `+x` intact | US2, FR-012, FR-017, SC-007 |
| V7 | **[Windows]** and **[Unix VM]** try to list/open/create `.ssh`, and `projects/x/.ssh` | Not listed; open/create refused; `~/.ssh` unchanged | FR-014, SC-006 |
| V8 | On Ubuntu `ln -s .ssh ~/keys-link`; `export status` | Status prints a symlink warning; remove the link afterwards | Edge case (symlinks) |
| V9 | Connect with a wrong password; connect from a host outside `allowed_network` (e.g. over Tailscale) | Both refused; no guest access | FR-015, FR-016, SC-004 |
| V10 | Reboot Ubuntu; `export status`; reconnect from Windows | Available with no action | FR-006, SC-005 |
| V11 | `local-machine export stop` twice; `export status` | Second run says nothing was shared; status `set up, not available`, exit 5; Windows and VM cannot connect; still off after reboot | US4, FR-008, SC-005 |
| V12 | Remove `allowed_network` from config; run each action | Each stops with exit 3 naming the key and file | Edge case (config) |
| V13 | `grep -ri password ~/.config/local-machine` | No match | FR-015 |
| V14 | Time V3→V5 on a fresh machine | Under 5 minutes | SC-001 |
| V15 | **[Windows]** Open and save a ~500 KB note | Under 1 second | SC-003 |
| V16 | With the share available, change `allowed_network` in `export.yaml`; run `export start`; `export status`; then `export setup`, `export start` | `start` exits 4 and `status` warns "configuration changed — run 'local-machine export setup'"; after setup + start the share is available, old firewall rule gone (`sudo ufw status`) | Edge case (config change), FR-016 |
| V17 | On Ubuntu `touch ~/'a:b?.txt'`; **[Windows]** browse that folder; on Ubuntu `ls -l ~/'a:b?.txt'` | Windows shows a mangled name or hides it; the Ubuntu file is unchanged | Edge case (file names) |
| V18 | `sudo testparm -s 2>/dev/null \| sed -n '/^\[global\]/,/^\[/p'`; then `testparm -s --parameter-name="map to guest"` | `[global]` shows `server min protocol = SMB3`, `restrict anonymous = 2`, `hosts allow = <allowed_network> 127.0.0.1`, `hosts deny = ALL`; `map to guest` prints `Never` (a default, so the first command hides it). Ubuntu's `[printers]` and `[print$]` shares remain (accepted, research R5) | FR-015, FR-016 (globals applied) |

## Automated checks

- `cargo test` — config validation, Samba config rendering, step planning with a fake
  command runner, CLI shape and exit codes.
