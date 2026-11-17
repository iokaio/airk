# Incremental derivation controls

The [validator](../incremental.rs) authenticates build proof/3 and raw incremental receipt/1, the existing checkpoint parent, one changed translation unit/object, 39 unchanged session objects, 95 unchanged libraries, provider identity, original clocks, retained process settlement and archive-member closure. It produces descriptive ancestry only. The [owning contract](../../CONTRACT.md) requires a fresh child producer/aggregate identity and forbids qualification claims.

[tests.rs](tests.rs) contains ten pure literal controls for clock renewal, retained HANDLE exits, suspended identity/Job limits, forced or foreign processes, exact replacement rosters, archive deduplication bounds, field rosters and pins. They do not launch processes, load DLLs or write fixtures. Public ancestry controls live in [the parent tests](../../tests.rs). These boundary controls are not a full actual-producer integration pass.

After scoped Clippy from the [parent README](../../README.md), run from the repository root:

```powershell
cargo nextest run --profile ci -p airk-retrieval --all-features --locked --timings -E 'test(arbiter::runtime::cuda::provenance::projection::incremental::)'
```

Expect ten selected controls; zero selected is failure. At introduction these controls are unrun. The validator's recipe authority deliberately fails closed until the genuine maintained producer recipe is frozen and reviewed. Neither authored controls nor a future metadata pass qualifies a runtime or model.
