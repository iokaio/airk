# Issue drafts for review

These are local drafts for the owner's review, based on source commit `c62c1e6936500801dbf41dffdd6e21453e48335d` (1.7.2 development candidate), inspected on 2026-10-30. Nothing has been posted. Each numbered document contains a proposed issue title, evidence and completion criteria; product proposals are not approved implementation requirements.

Start with the findings you want to raise. **01, 02 and 13 are marked for private security review**, following [SECURITY.md](../../../SECURITY.md). Do not paste those bodies into a public tracker without reviewing disclosure with the maintainer. If private vulnerability reporting is unavailable, the policy calls for requesting a private contact route without publishing the details.

| Draft | Kind | Suggested route |
|---|---|---|
| [01 — XML attribute processing can exhaust parser CPU](01-xml-parser-cpu-exhaustion.md) | Statically traced vulnerability; remediation | Private |
| [02 — Existing custom storage permissions](02-custom-storage-permissions.md) | Conditional security concern; investigation | Private |
| [03 — Automated dependency advisory checks](03-dependency-advisory-checks.md) | Maintenance-control proposal | Public |
| [04 — Outstanding dependency findings](04-dependency-findings-triage.md) | Dependency triage and remediation | Public; no exploit details |
| [05 — Review first-use installation and indexing effects](05-first-use-installation-scope.md) | Product-control proposal | Public |
| [06 — Native execution with automatic installation disabled](06-disable-native-execution.md) | Documentation and control proposal | Public |
| [07 — Sensitive-file capture and saved evidence](07-sensitive-data-retention.md) | Privacy-control proposal | Public |
| [08 — PATH and terminal wrapper survive uninstall cleanup](08-uninstall-shell-integration.md) | Installation-hygiene defect | Public |
| [09 — Optional purge of retained AIRK data](09-uninstall-retained-data.md) | Cleanup feature proposal | Public |
| [10 — Detached uninstall completion and recovery](10-uninstall-completion-visibility.md) | Observability and qualification proposal | Public |
| [11 — Verify an actual published package against source](11-release-verification-evidence.md) | Bounded evidence investigation | Public |
| [12 — Unresolved dependency license review](12-license-review-disposition.md) | Release-review disposition | Public |
| [13 — Windows Assistance IPC access controls](13-windows-assistance-ipc-review.md) | Unverified hardening concern; investigation | Private |
| [14 — Incorrect CLI setup/removal documentation](14-cli-removal-documentation.md) | Documentation defect | Public |

The drafts distinguish static evidence, dated advisory-audit results and verification still to be performed. AIRK was not installed, built or executed for this review, and no platform install/uninstall test is claimed. Source links identify the reviewed commit so the future tracker text does not depend on local review records. Recheck relevant facts against the version being reported before posting.

Related observations are grouped rather than turned into duplicate defects: shell-profile breadth and agent-entry migration are in 05; manual agent entries, residual logs/settings and custom storage are in 09/10/14; Workbench copies and external agent disclosure are in 07. Documented opt-in login startup, a library-only provider path with no established production caller, and the explicitly limited SBOM do not establish separate bugs. Intentional index retention is a feature-request premise, not a broken deletion promise.
