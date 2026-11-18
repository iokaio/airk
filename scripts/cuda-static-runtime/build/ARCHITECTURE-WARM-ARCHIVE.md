# Private cu17 warm archive

Use only after [the direct warm continuation](ARCHITECTURE-WARM.md) passes.
The warm archive consumer reopens the original configure and failed stage,
the actual direct Ninja and cuobjdump logs and owned results, and the exact
audited library/provider census. It checks the current CMake/Ninja identity
and reparses the actual SASS/PTX reports.

The reviewed JSON recipe has exactly these fields.

| Field | Value |
| --- | --- |
| `schema` | `airk.private.cuda-architecture-warm-aggregate-recipe/1` |
| `driver` | FilePin of the reviewed `fresh_architecture_warm_archive.py` |
| `warm_compile` | Exact passed `warm-continuation-*/warm-compile.json` FilePin |
| `archiver` | Original configure tool census FilePin for MSVC `lib.exe` |
| `output` | Fresh absolute direct `warm-aggregate-*` child of cu17 |
| `wall_seconds` | Positive integer up to 300 |
| `execution_authorized` | `true` |
| `qualification` | `false` |

```text
python -B -S scripts/cuda-static-runtime/build/fresh_architecture_warm_archive.py
  --recipe REVIEWED_PRIVATE_RECIPE --recipe-sha256 SHA
```

Run as the verified interactive Windows human, with the original pinned
Python interpreter. The original 10,800-second lineage bounds preflight
and execution. The archive operation uses its separately clipped
300-second cap. It calls the stock archive selection and COFF checks on
the observed graph and library list, then runs the actual `lib.exe` under
direct suspended Job ownership. The private plan and receipt identify
the warm continuation, preserve the old evidence, and record both actual
collector exports and archive FilePin. The explicit warm plan and receipt
schemas cannot stand in for historical fresh evidence. The output grants no
runtime selection, GPU qualification or public provenance.
