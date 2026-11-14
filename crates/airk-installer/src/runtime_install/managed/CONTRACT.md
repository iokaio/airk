# Managed runtime metadata and engine ownership

Owner: `airk-installer::runtime_install::managed` and its catalog admission
adapter. Native payload metadata is owned by `airk-host::runtime_pair` and
Core `runtime_metadata`; managed catalog origin formats stay owned by Store.

- Legacy metadata schema 1 describes the service plus an external embedding
  helper. Managed origins remain schema 1 with the exact helper identity.
- Metadata schema 2 describes one in-process engine. Managed origins remain
  schema 2 without a helper field.
- Split metadata schema 3 describes the service and its stateless CLI. It
  still has one in-process engine and uses the unchanged schema-2 managed
  origin, with no helper identity or new persisted format. Packaging schema
  numbers are not catalog-origin schema numbers.
- Schema-3 admission requires the approved inventory's CLI entry at the
  Core-owned relative path to match the metadata digest. The actual CLI bytes
  are checked by Host engine identity verification, alongside the service.
  Modified/missing CLI files refuse; no origin is rewritten to accept them.
- Catalog predecessor qualification remains version-independent from the
  current installer image, but validates the native metadata target and hashes
  both declared service/CLI images (or the legacy helper for schema 1) before
  framing the complete canonical metadata under the existing engine domain.
  No metadata-only structural read is claimed to verify executable bytes.
- Existing schema-1/2 bytes, managed origin identities, field order and digest
  framing are preserved. The inert fixtures test native metadata/ownership
  boundaries; they do not qualify execution of a packaged runtime/container.

```console
cargo nextest run --profile ci -p airk-installer --no-default-features --lib --locked --timings -E 'test(split_schema3_) | test(modern_managed_origin_binds_exact_main_without_a_helper)'
```

This selection contains three tests and must report a nonzero selected count.
Actual managed/container startup and package evidence remain separate gates.
