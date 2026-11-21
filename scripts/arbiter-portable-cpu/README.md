# Portable Context graph on the static CPU runtime

`run-selector.ps1` runs the ignored `portable_cpu` selectors of `airk-inference`
on Windows. The selectors load and score the portable Context graph on AIRK's
own static ONNX Runtime CPU build, at the 8-core class named in the
[Arbiter plan](../../docs/arbiter/final-plan-fp16.md#10-engineering-state-carried-forward).
The portable graph stores its learned weights as FP16 and computes in FP32.

The Rust owner is
`crates/airk-inference/src/inference_engine/pool/arbiter/qualification/portable_cpu.rs`.
It is test-only code. Everything it produces is unqualified development
evidence: no profile it emits carries a qualification, a qualified target or a
download, and nothing here adds a catalog row.

## Selectors

| Step | Selector | What it does |
| --- | --- | --- |
| `emit` | `portable_cpu_emit_unqualified_profiles` | Builds two private profiles with the existing builder, both at `cpu_fit` 8 cores and 8 threads: `full` at 8,192 tokens and `fast` at 2,048. Loads no model. |
| `load` | `portable_cpu_load_only_records_resources_with_zero_forwards` | Opens the scorer once and drops it. Records load time, memory before and after the load and after the drop, the execution identity and `forwards: 0`. |
| `score` | `portable_cpu_score_references_timing_and_cast_profile` | Scores the retained cases that fit the tier and compares them with their retained FP32 scores. Then runs one warmup and 20 timed forwards at the tier ceiling. Then opens the scorer once more with profiling to see whether the weight Casts ran in the forward. |
| `timing` | `portable_cpu_timing_only` | Loads, then runs the warmup and the 20 timed forwards only. |

The timed forwards use the widest input the tier's own compiler admits, which
is exactly the tier ceiling with eight options. The series goes through Core
`ResourceSeries`, so it stops at the first forward over 120 seconds or over the
profile's declared memory reservation and then reports no quantiles. The
receipt gives the median as the mean of the two central samples and the p95 by
nearest rank.

`load`, `score` and `timing` refuse to run unless the process can use exactly
eight physical cores. Each records the affinity mask it observed.

The score comparison applies a development screen: the winner must agree and
the largest raw-score difference must be at most 0.007. The selector records
every case before it fails on the screen. The screen is not a qualification
gate.

## Inputs

All inputs are private files named by environment variables, each with a
SHA-256 pin in a second variable. `run-selector.ps1` sets them.

- `emit` reads `AIRK_PORTABLE_CPU_REQUEST` and `AIRK_PORTABLE_CPU_REQUEST_SHA256`.
  The request is a JSON document you write by hand:

  ```json
  {
    "schema": "airk.private.portable-cpu-emit-request/1",
    "model": "arbiter-context-qwen3-17b",
    "profile_id": "private-portable-compact-cpu8-r1",
    "memory_reservation_bytes": 17179869184,
    "peak_disk_bytes": 4294967296,
    "assets": [
      {"asset": {"name": "model.onnx", "role": "model", "bytes": 0, "sha256": "<hex>"}, "path": "<absolute path>"}
    ],
    "cases": [
      {"name": "control-t32", "path": "<absolute path to execution.json>", "sha256": "<hex>"}
    ]
  }
  ```

  `assets` needs the graph (`model`), its weight file (`external_weights`,
  named as the graph's external-data location names it), the tokenizer
  (`tokenizer`), a notice (`notice`) and a fixture (`fixture`). The builder
  writes the compiler and calibration assets itself. Each case is a retained
  reference receipt: `result.compiled` holds the input and
  `result.original_binding.original_fp32` the reference scores. The emitter
  appends `-full` and `-fast` to `profile_id`.
- `load`, `score` and `timing` read `AIRK_PORTABLE_CPU_INPUT` and
  `AIRK_PORTABLE_CPU_INPUT_SHA256`: one `tier-input.private.json` written by
  `emit`. Its pin is `tier_input_sha256` in `emit.json`.
- Every selector writes to `AIRK_PORTABLE_CPU_OUTPUT`, which must be a new path
  under `artifacts/`.

The weight file is read in place. Nothing copies it.

## Outputs

`run-selector.ps1 -Root <root> -Name <name>` creates `<root>\<name>` with:

- the selector's own output: `profiles\` for `emit` (`emit.json`, and per tier
  the profile, compiler, calibration and `tier-input.private.json`), otherwise
  `load.json`, `score.json` or `timing.json`;
- for `score`, the ONNX Runtime profile of the one profiled forward,
  `score.ort-profile_<time>.json`;
- the receipt runner's files: `<name>.json`, `<name>-start.json`,
  `<name>.log`, the Cargo timing report and the nextest JUnit report;
- `launcher.json`: the command, the affinity mask requested and read back, the
  processor model, the digests of the static runtime environment file, of its
  aggregate receipt and of `runtime.rs`, free space on the volume, and the
  Cargo, compiler, Python and AIRK processes running before and after.

These files hold absolute paths. Keep them under `artifacts/` and never stage
them.

## Use

Build the test binary and run the unit tests first, without an affinity cap.
Set the five variables under `environment` in
`artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json` and
`AIRK_BUNDLED_MODEL_DIR`, then:

```powershell
cargo nextest run --profile ci -p airk-inference --no-default-features --features local-inference,test-support --locked --offline --timings -E 'test(/portable_cpu/)'
```

Eight unit tests run. The four selectors are ignored and do not run here. Then
run the selectors. Pass `-BundledModelDir` with the same string as
`AIRK_BUNDLED_MODEL_DIR` had in the build, or Cargo rebuilds under the cap.

```powershell
$sid = '<SID of the logged-in human>'
$root = '<existing private folder under artifacts>'
$mask = '<hex mask covering both logical processors of eight physical cores>'
pwsh -NoProfile -File scripts/arbiter-portable-cpu/run-selector.ps1 -Step emit -Name emit-r1 -Root $root -InputPath <request.json> -InputSha256 <hex> -ExpectedSid $sid
pwsh -NoProfile -File scripts/arbiter-portable-cpu/run-selector.ps1 -Step load -Name full-load-r1 -Root $root -InputPath $root\emit-r1\profiles\full\tier-input.private.json -InputSha256 <hex> -ExpectedSid $sid -AffinityMask $mask
pwsh -NoProfile -File scripts/arbiter-portable-cpu/run-selector.ps1 -Step score -Name full-score-r1 -Root $root -InputPath $root\emit-r1\profiles\full\tier-input.private.json -InputSha256 <hex> -ExpectedSid $sid -AffinityMask $mask
```

To measure timing again on a quiet machine, run only the `timing` step with a
new name. Use `fast` in place of `full` for the 2,048-token tier.

The script caps its own process, so always start it as a separate `pwsh`
process. One Cargo owner at a time may use the target directory. On Linux or
macOS, cap the shell to eight physical cores yourself (for example with
`taskset`), set the variables above and run the Cargo command with
`--run-ignored only -E 'test(=inference_engine::pool::arbiter::qualification::portable_cpu::<selector>)'`.

## What the results do and do not show

A passing `load` shows that the static CPU build loads the graph. A passing
`score` shows agreement with the retained FP32 scores on the retained cases
within the development screen. Timing figures describe one machine under the
load recorded in `launcher.json`; quote a p95 for the CPU class only from a run
on a quiet machine. None of this is model fit, calibration, task quality,
installed-service behaviour or qualification on another platform.
