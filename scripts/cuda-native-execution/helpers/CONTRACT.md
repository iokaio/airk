# Maintained ownership helper source contract

This structural migration retains exact source bytes and existing callable
behavior. Original files remain under the private independent-numeric campaign
root; no receipt, model, label or historical result is rewritten.

| File | SHA256 |
| --- | --- |
| reference_owned.py | 2d7a31a64eaba745e32ed83adfbeedb75983ab6f9b5c4cc82714e9371c1895a1 |
| reference_parent_no_site_r2.py | fe4cc990649d92e041b79cccb3fd7ef1129979a26432b7f02b2d8d28b586ed8d |
| reference_worker.py | 9c04be28ef06c0a06b8e2cffd4c274348ff141046114cc493d725ce527b694c7 |

The CUDA native loader authenticates both declared retained source paths and
these maintained copies before importing code. It explicitly supplies the
maintained worker during the parent's sibling import, checks object identity,
canonical path and exact hash, and restores prior module state on success or
failure. It does not add artifact directories to the import search path.
Fresh plans seal maintained paths. Old paths remain authenticated historical
evidence only; their bytes are never executed by this loader.

Suspended launch, user/image checks, Job/resource limits, retained HANDLE
settlement and cleanup behavior are unchanged. Relocated legacy CLI entry points
are unsupported because their original path assumptions remain. No process,
model, conversion, native build or qualification follows from this migration.
Future behavior changes require separate review and tests.
