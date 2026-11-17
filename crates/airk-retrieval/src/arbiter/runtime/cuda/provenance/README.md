# CUDA provenance

The [Rust owner](../provenance.rs) validates the path-free public runtime provenance record. [CONTRACT.md](CONTRACT.md) defines its descriptive authority, exact package roster and historical compatibility. [tests.rs](tests.rs) contains public decoding, refusal and ancestry controls. The [projection owner](projection.rs) authenticates private metadata before producing the public record; its [submodules](projection/README.md) preserve checkpoint and incremental ancestry separately.

Run from the repository root with the provisioned static runtime and a single Cargo owner:

```powershell
python -B scripts/dev-check.py -p airk-retrieval --all-features
cargo nextest run --profile ci -p airk-retrieval --all-features --locked --timings -E 'test(arbiter::runtime::cuda::provenance::)'
```

Report the actual selected count. These checks cover descriptive validation, not runtime/model qualification. The incremental implementation awaits combined review, compilation and regression execution. Its exact reviewed recipe authority remains deliberately fail-closed until the genuine maintained producer recipe is frozen; historical checkpoint evidence retains its original identity.
