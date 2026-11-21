# Linux GNU admission contract

This folder owns three admission rules for work in the campaign's WSL2
instance: the storage limits for GNU gates and packaging, the start of the
guest, and the device journey's expectation of service-wide GPU use. The
[README](README.md) gives the commands. Limits come from the recorded
authorization in the [current assignment](../../../../docs/agent-current-work.md)
(extension restart checkpoint of 20 November 2026). Nothing here raises one.

## Storage limits

Owner: `gnu_volume_policy.py`. Every number has one definition there.

| Limit | Constant | Bytes |
| --- | --- | --- |
| C: free-space baseline | `C_BASELINE` | 2,669,960,904,704 |
| C: growth allowance | `C_GROWTH_ALLOWANCE` | 120,000,000,000 |
| D: free-space baseline | `D_BASELINE` | 1,233,246,265,344 |
| D: growth budget | `D_GROWTH_BUDGET` | 195,421,011,968 |
| Free floor on C:, D: and the guest | `FREE_FLOOR` | 107,374,182,400 |
| Guest free-space baseline | `GUEST_FREE_BASELINE` | 967,378,755,584 |
| Guest free-space growth cap | `GUEST_GROWTH_CAP` | 25,769,803,776 |
| VHD length baseline | `VHD_BASELINE` | 60,649,635,840 |
| VHD growth cap | `VHD_GROWTH_CAP` | 25,769,803,776 |
| Swap reserve charged to C: | `SWAP_RESERVE` | 17,179,869,184 |

Whether the guest cap or the VHD cap may rise inside the C: allowance is an
open user decision. Until it is recorded both keep their retained values. A
recorded decision changes one constant and its row above.

**Binding.** A run supplies three values together: the path of the active
host watcher log, the SHA-256 of that log's first row without its line
ending, and the time the run must end by, with its UTC offset. The log must
be a regular file named `host-volume-watch-r<N>.jsonl` directly in the
campaign root, reached without a link and owned by the verified user: UID
1000 in the guest, and the human SID on Windows, where the host observer
checks it. No log name, digest or end time is compiled in.

**Watcher rows.** The first row and the last complete row must carry the
baselines, budgets and floor above, the original alert log digest and the
original 24 GiB C: budget as `previous_backing_budget`, with
`backing_writes_held` false, `within_budget` true and `has_headroom` true.
The last row must be at most 45 seconds old and not from the future. The run
must end within one day of the first row. A row still being written is
ignored.

**Admission.** With `additional` equal to the job's reservation at admission
and zero afterwards, a sample is refused when any of these holds:

- guest growth plus `additional` exceeds the guest cap;
- VHD growth plus `additional` exceeds the VHD cap;
- C: growth plus the swap reserve plus `additional` exceeds the C: allowance;
- D: growth exceeds the D: budget;
- free space on C:, D: or the guest is below the floor plus the unused part
  of that volume's limit;
- the run has grown the guest or the VHD by more than its reservation.

C: growth is the larger of the fall in free space and the VHD growth. C:, D:
and VHD readings take the less favourable of the guest's sample and the
watcher's row. An observer that reserves nothing passes no reservation; every
total still applies to it. A refusal at admission raises an error naming the
limits. A refusal during a run stops the owned process and fails the gate.

**Where it applies.** `maintained_gate.py` accepts the binding for every gate.
It checks admission before the source census, runs Cargo under the watchdog
for signed and unsigned sources, and adds `volume_policy` to the Clippy and
Nextest receipts. The `inspect` document is unchanged. `volume_policy_entry.py`
activates the binding for `shipping_build.py`, `shipping_archive.py` and
`shipping_service.py`, whose guard is `shipping_build.monitored` or the
service observer.

**Predecessors.** Without a binding every entry behaves as before.
`shipping_build.capacity` keeps the original 24 GiB limit on C: and refuses
today. `r7_volume_budget.py` is bound to `host-volume-watch-r4.jsonl` and
to a cutoff that has passed. `shipping_notice_cache.py`, `overlay_target.py`
and `target_overlay.py` still call the original guard directly.

## Guest start

Owner: `wsl_boot_admission.py` with the read-only `wsl_boot_host.ps1`.

The wrapper starts three command shapes and no other:

1. `pwsh -NoProfile -NonInteractive -File wsl_boot_host.ps1 ...`, which runs
   the human-execution guard and reads Windows facts;
2. `C:\Windows\System32\wsl.exe --list --running --quiet`;
3. `C:\Windows\System32\wsl.exe --distribution Ubuntu --user tyler [--cd <absolute guest directory>] --exec <absolute guest program> ...`.

It refuses a bare `wsl`, another executable path, the short options, a missing
or different distribution or user, every management verb, and any argument
that names `sudo`, `su`, `doas`, `pkexec`, `runuser`, `apt` or `apt-get`. It
never stops or terminates a distribution.

Before the guest command it requires: the expected human SID as user and
default owner, not elevated; one registered `Ubuntu` with default UID 1000,
WSL version 2 and its virtual disk on C:; the keep-alive task `Disabled`; no
Docker Desktop backend; at most one swap file, on C: and within the swap
reserve; Windows physical and commit memory above 12,884,901,888 bytes plus
the stated other reservations; a bound, fresh, healthy watcher log; the host
storage limits with the stated reservation; and no running distribution other
than `Ubuntu`. A wrong Windows identity ends the run before any file exists.

After the guest command it requires: exit code 0; `uid=1000(tyler)` and
`gid=1000(tyler)`; the WSL2 kernel; UID and GID 1000 without group or other
write access on the home, build and target directories; guest swap within the
reserve; `nproc` equal to the `lscpu` rows and at least the required number
of distinct cores; no Cargo, rustc or Nextest process; only `Ubuntu` running;
and the full storage admission with the guest's free space. A guest Ollama
process is reported and not refused.

The receipt `airk.private.wsl-boot-admission/1` is written once, also for a
refusal. It keeps the fields of the retained boot receipts (`sid`,
`registered_uid`, `before`, `after`, `guest`, `exit_code`, `vhd_path`,
`swap_path`, the three reservations, `post_boot_physical`,
`post_boot_virtual`, `monitor`) and adds the parsed guest facts, running
distributions, keep-alive state, headroom, `refusals` and `admitted`.

## Device journey

Owner: `gpu_opt_in_workflow.py`. It follows the Core decision
[Service-wide GPU use is opt-in](../../../../crates/airk-core/src/arbiter/CONTRACT.md#service-wide-gpu-use-is-opt-in-decided-20-november-2026-americadenver).

- A fresh configuration reports `allow_gpu: false` and no `gpu_opt_in` field
  after a stage selects Auto with FP16.
- With `stored_enabled_default=True` the reopened configuration must report
  `allow_gpu: false` with `gpu_opt_in: "default_reset"`.
- `set_gpu_allowed` with `false` reports off with no record, and with `true`
  reports on with `gpu_opt_in: "user_choice"`. A read returns the same pair.
  A null record is refused.
- Neither answer changes the stage configuration. Activation of an
  unqualified stage is still refused with `arbiter_unavailable`, explicit CPU
  selection keeps the model, and no profile is active.

`device_policy_workflow.py` is the predecessor. A retained Windows receipt
seals its bytes, so it is unchanged and expects the earlier enabled default.
