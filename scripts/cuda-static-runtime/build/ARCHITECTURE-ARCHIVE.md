# Private CUDA architecture archive handoff

This follows the [Windows architecture compile candidate](ARCHITECTURES.md).
Aggregate only after the original configure, compile and CUDA code
inventory all settle. The architecture aggregate recipe uses the existing
aggregate fields and owners with these additions and substitutions.

| Field | Required value |
| --- | --- |
| `schema` | `airk.private.cuda-fresh-architecture-aggregate-recipe/1` |
| `compile_receipt` | Exact cu17 `fresh-compile.json` FilePin |
| `cuda_code_receipt` | Exact sibling `fresh-cuda-code.json` FilePin |
| `owners.driver` | FilePin of the reviewed `fresh_architecture_archive.py` |
| `owners.compile`, `owners.projection`, `owners.archive`, `owners.pin_reader` | Current exact stock helper pins |
| `archiver` | Exact Windows `lib.exe` FilePin in the reviewed configure tool census |

The additive archive owner authenticates the existing reviewed architecture
source and exact original configure recipe. The code receipt must bind
the exact compile receipt, generated CMake/Ninja architecture evidence,
staged CUDA provider FilePin, pinned inspector, and both actual owned
cuobjdump command receipts under the original lineage clock. It reparses
the bounded stdout for the required SASS/PTX population.

The emitted private architecture build proof includes the configure and
compile closure, original architecture owner and code receipt pin. The
aggregate plan binds that proof by its exact canonical SHA. The original
300-second aggregate operation and 10,800-second build lineage windows
still apply. The reviewed stock archive selection excludes provider import
libraries, checks the precise `lib.exe` argv and response file, requires
natural Job settlement, inspects every selected input's COFF membership,
and checks the resulting archive and collector exports. Final rechecks
cover the code, inspector, and provider pins. The candidate aggregate
receipt retains its private proof and exact archive path/bytes/SHA.
It carries no runtime activation, GPU validation or public provenance.

```text
python -B -S scripts/cuda-static-runtime/build/fresh_architecture_archive.py
  --recipe REVIEWED_PRIVATE_AGGREGATE_RECIPE --recipe-sha256 SHA
```

The existing SM86 aggregate and environment/6 consumers retain their exact
schemas and owner identities. A future candidate handoff reopens these new
private receipts explicitly before any Rust metadata projection.
