# AIRK 1.5.2 development record

Status: release candidate prepared on 27 October 2026; publication requires protected-main CI and both release workflows.

Native workspace packages, extension manifest and both lockfiles are synchronized at 1.5.2. This patch carries forward the [1.5.1 Explorer presentation and usability changes](release-1.5.1.md) and the Inspector guidance follow-up. The service namespace remains `service-1.4`; no index-format change or reindex is required.

- AIRK Explorer replaces the Service Console with Search, Code, Documents, Evidence, Review, Strategies and Management workspaces. Explorer Assistance provides fourteen extension-owned help/draft tools, separate from native read-only Retrieval.
- Responsive AIRK/VS Code themes, the activity-bar entry point, contextual Help and guarded nested plan/strategy/search file libraries carry forward from the 1.5.1 development record. Review prioritizes source changes and test directories and discloses bounded textual evidence.
- Agent setup uses a version-free universal launcher for recognized default installations and preserves custom configuration. Existing registered project/index ownership is unchanged.
- Evidence › Inspector loads the selected source file when no citation is selected. **Inspect selected file** refreshes that record or returns from a citation.
- Visible instructions explain Search → result Inspect → indexed record → Open source, with a **Search for evidence** shortcut and labels distinguishing the header file from a selected citation.
- Search explains that its query and path scope are separate from the source-file header. Bundled Help and extension guidance describe both entry points.

Local 1.5.2 validation: Rust formatting and workspace Clippy passed; extension compilation/tests passed (286 passed, twelve skipped); Python contracts passed (172 total, four skipped). Ten actual-bundle browser scenarios passed, including Inspector citation/file switching, Search guidance, project reset, responsive themes and saved-file libraries. Repository, version synchronization and whitespace checks passed. Dependency notice generation covered 411 packages with the existing three separate-license-text review notices. The staged debug runtime pair reports 1.5.2 and passed all seven native Review tests and the two-project service/client check (sixteen retrieval tools, real offline model, saved source bytes preserved). Full Rust workspace tests are recorded below when complete. The packaged 1.5.1 VSIX and earlier qualification receipts remain historical; 1.5.2 packaging, installed-editor acceptance and hosted platform checks require their own evidence. Publication status must be checked in GitHub Actions and the public outputs.
