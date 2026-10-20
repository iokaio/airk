# AIRK 1.2.1

Version 1.2.1 refreshes the documentation shipped in the VS Code extension and the repository, including one obsolete storage-budget message. It does not change AIRK's indexing, retrieval, storage, or MCP behavior from 1.2.0.

The VSIX Details and Marketplace description now lead with the 1.2 current-index model, progressive discovery and semantic readiness, the Agent Lean/Rich Structure/Search Only choices, and optional retained-object compression. Installation instructions name the current stable package instead of old 1.1 candidates. Retired history, checkpoint, and generation-trimming controls are no longer presented as available.

The repository documentation index and release workflow instructions now point to the 1.2 behavior and the checked-in version. Historical 1.1 release records remain available as dated records. Contributor guidance now requires documentation alignment before each commit, push, or PR creation cycle. Native and extension versions and both lockfiles advance together to 1.2.1 so the corrected description can be included in a new VSIX; the Marketplace does not replace a published version's packaged README in place.

Release qualification and publication require successful CI for the exact main commit, followed by the repository's explicit GitHub Release and Marketplace workflows. This page records the intended documentation change and does not itself establish that those checks or publications have completed.
