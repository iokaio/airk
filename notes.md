# Windows CUDA checkout permissions — November 18, 2026

## What happened

The Windows CUDA static build completed and both AIRK test images linked against
its archive. The native CUDA manifest assembler then refused to open original
build inputs because their permissions allowed other accounts to modify them.
The checkout was already owned by TYAI\tdotn.

The D: drive root explicitly grants Authenticated Users inheritable Modify access.
The grant flowed through D:\code and D:\code\Github into the checkout. The checkout
root also had three explicit Modify grants for other principals that flowed into
its children. The owner check and the effective permissions check answer different
questions. The earlier preflight verified ownership and missed those grants. The
current ACL shows where the grants came from. Whether a previous repair changed
only ownership or whether the ACL was later restored is unknown.

The first permissions-helper preview incorrectly used PowerShell's default owner
for new objects as a condition for reading the checkout ACL. That caused a refusal
before any permission change. The corrected preview has no owner eligibility
check. Apply checks the actual checkout owner against the running account.

During the user-run repair, the helper printed the backup path at 8:26 a.m.
Mountain time and then waited inside Set-Acl. Windows was propagating the changes
through the existing tree. The PowerShell process was using CPU and the CMake,
Ninja and recipe paths adopted the new permissions at different times. The checkout
root was protected and the process had exited by 8:34 a.m. Mountain time. The
helper now reports when propagation starts, how long it took, and when an
already-protected root can skip the update.

The root is now protected from inheritance. A read-only audit verified the hashes
of 22 original pinned files and checked permissions on 32 distinct files and
parent directories. It found zero foreign write grants. Native CUDA manifest
assembly then passed against the original completed build in 24.5 seconds. This
verifies assembly. Windows model and platform qualification are still open.

## Prevention

1. Set a private permissions boundary at the checkout root before creating or
   cloning into it. Keep the root protected from drive-level inheritance. Let new
   files inherit from the private checkout. Use the helper's read-only preview to
   confirm which permissions are present after a move, restore, or new checkout.
2. Before starting the three-hour build clock, check the exact recipe, build-owner
   files, tools, and source patches and their immediate parent folders. Require
   the expected owners and reject foreign write, delete, and permission-changing
   grants. Ownership alone cannot clear this check. A protected build workspace
   provides a route when the working checkout must remain shared.
3. After any repair, recheck the original file hashes and access rules before
   reusing artifacts. Check the exact inputs because a child can have its own ACL.
   Resume native assembly on the existing build when these checks pass.
4. During a permission change, keep the backup and let the first invocation
   finish. Observe the running process and selected child ACLs if propagation
   appears stalled.

- [Checkout permissions helper](scripts/dev/protect-airk-checkout.ps1)
- [Windows CUDA build preflight](scripts/cuda-static-runtime/build/README.md)
- [Windows permission inheritance](https://learn.microsoft.com/en-us/windows/win32/secauthz/automatic-propagation-of-inheritable-aces)
