# Arbiter GPU memory tools

Purpose: observe the GPU memory a private CUDA measurement uses on Windows,
and summarize the measurement receipts of the plan 12.5.2 CUDA preparation
(L5). These are evaluation tools. They grant no admission and qualify nothing.

| File | Purpose |
| --- | --- |
| `../arbiter-gpu-memory.py` | Library: sampled per-process D3DKMT LOCAL and NON_LOCAL usage. See [CONTRACT.md](CONTRACT.md). |
| `launch_sampled.py` | Launches one child (normally the `cuda_l5a_diagnostics` test image) and samples its GPU memory on every named adapter LUID. |
| `l5a_summary.py` | Turns `l5a_gpu` and `l5a_cutlass` receipts plus their sampler summaries into one measurements table. |

## launch_sampled.py

- **Inputs.**
  - The child command, after `--`.
  - One `--luid high:low` per adapter to sample, in hexadecimal, physical index 0. Every named adapter is reported separately; nothing is guessed.
  - `--env KEY=VALUE` additions to the inherited environment.
  - `--interval` (seconds) and `--deadline-s`.
- **Outputs**, in a fresh `--output-dir`:
  - `child.log`, the child's stdout and stderr;
  - `samples.jsonl`, every sample;
  - `summary.json`, with the sampled maxima, the exit code and the image SHA-256.
- **Use.**
  - Run with `python -I` as the verified human.
  - Only the child process is sampled. The child must therefore be the test image `.exe` itself; the launcher refuses a cargo or nextest wrapper and records the image SHA-256.
  - It takes one final sample at exit; the observer marks that sample invalid once the process is gone.
  - Samples are sampled current usage, never a lifetime peak.

## l5a_summary.py

- **Inputs.** `--runs DIR` holding `g*.json` receipts and their `<receipt>.mem` directories; `--luid` names the adapter to read.
- **Outputs.** `measurements.json` and `measurements.md` in a fresh `--output` directory.
- **Use.**
  - The device column is the CUDA free-byte drop from the pre-load observation to the observation after each width. It is cumulative, because the CUDA arena does not shrink.
  - The process column is the sampled LOCAL maximum over the child lifetime.
