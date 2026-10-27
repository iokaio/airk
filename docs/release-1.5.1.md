# AIRK 1.5.1 development record

Status: unreleased development version, prepared on 27 October 2026.

Native, extension and lockfiles are synchronized at 1.5.1. The service namespace remains `service-1.4`; these editor changes introduce no index-format change and require no reindex.

- Explorer restores prototype-inspired panels, typography, spacing and navigation. The persistent upper-right **Themed** toggle chooses AIRK colors or VS Code colors; both share responsive layouts and preserve high-contrast colors. Management is beside the main navigation; content scrolling no longer displaces it.
- Project cards replace the wide Projects table, preserving cached/unknown observations and exposing publication details separately.
- A monochrome A› activity icon follows the website/extension mark. Clicking it reveals Explorer; the old sidebar and project tree are fallback views if Explorer cannot load. Missing assets fail explicitly and a later attempt retries. Service readiness and trust remain separate from UI availability.
- Planner, Studio and Saved Searches browse saved files in their .airk folders, including nested directories, root selection, breadcrumbs, bounded paging, Load buttons and double-click loading. Imports retain schema/byte limits and confirmation/race checks. The search Save button and Saved Searches tab use plain labels. Recipe JSON import/export preserves workspace searches and never starts a search; conflicting saved recipes are not overwritten.
- Source selection offers open editor tabs and another-file browsing, keeps a per-project target across webview focus, and displays the chosen file. File-based actions use indexed evidence; source links and plan documents open beside Explorer.
- The single upper-right Help button opens a native quick pick for the current screen or general guidance. Missing Outline selection is reported as a missing source file, not an invalid protocol message.

The [1.5.0 release record](release-1.5.0.md) and its installer receipts retain their original scope. No 1.5.1 installer or publication is implied by this source change. Browser previews use the actual bundled UI with fictional host observations; they do not qualify a native runtime or installed editor.

Validation receipts are recorded in the [Explorer action plan](explorer/code-review-action-plan.md). Full native rebuilds, installed-editor acceptance and Linux/macOS/remote qualification remain separate.


Review follow-up: changed source paths are prioritized ahead of documentation, declaration samples are spread across files, and test directories are searched before applying reference-hit limits. Changed test files appear even without indexed declarations and are labeled as Git observations. Native reference replies retain their nested publication identity; mismatched publications still fail closed. Reports disclose inspected-path/declaration limits, incomplete inventory and Git availability. Review considers uncommitted changes and textual candidates, not resolved callers or executed test coverage.

Source result text uses normal weight and a thin outline across the shared fact, excerpt and Review views. Metadata stays separate from quoted source text.

Inspector follow-up: Evidence › Inspector automatically loads the selected source file when no citation is selected and provides **Inspect selected file** to refresh it or return from a cited result. The empty state explains where result-level Inspect actions live. Selected-file reads require an indexed hash/publication and report absent records explicitly.

Inspector includes visible file/citation instructions and a Search for evidence shortcut. Search explains its independent scope and result actions. This follow-up is source-validated; the earlier local VSIX receipt predates it.
