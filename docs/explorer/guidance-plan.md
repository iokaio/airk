# AIRK Guidance — Implementation Plan v4

**Status:** forward plan, not current behavior. Nothing described here is implemented. The other files in `docs/explorer/` are the historical 1.4/1.5 Explorer design baseline (see `docs/explorer/README.md`); current shipped behavior is in `docs/vscode-extension.md` and `docs/unified-mcp.md`.
**Feature:** evidence-driven evaluation, generation and **user-chosen management** of agent-guidance artifacts from the AIRK index and the live source. The artifacts are instruction files (AGENTS.md, CLAUDE.md, engine-specific instruction files), agent **skills** (`SKILL.md` directories), agent **settings files** (for example `.claude/settings.json`, `.codex/config.toml`, `.gemini/settings.json`, `.mcp.json`, `.vscode/mcp.json`), decision records and glossaries.
**Surfaces:** new `guidance_*` tools and a `draft_guidance` proposal tool on the unified `airk` MCP server, `airk guidance` CLI commands, a **Guidance** workspace in AIRK Explorer, and plain-Markdown agent skill files.
**Target:** the first release after 1.6.0. The operator assigns the version per AGENTS.md. This plan builds on the shipped 1.5 Explorer (route manifest, Suggestions Inbox, guarded workspace saves, Management › Agents) and the unified `airk` MCP server. WP1–WP4 can ship earlier as a CLI/MCP-only increment if the Explorer work slips.
**Coding agent:** Codex, under AGENTS.md. This plan is a governing source; where it conflicts with AGENTS.md, CONTRIBUTING.md or `docs/architecture.md`, those win and the conflict is logged in WP0.

**Changes in v4:** the Guidance workspace can now, if the user chooses, update and manage **skills** and **agent settings files** as well as instruction files. It does so from recommendations grounded in indexed evidence (found with the AIRK tools) and re-verified against the live original source (§6.6–§6.9, §7 families GD-S and GD-A, §12, §18.3). The plan adds per-class management modes in the profile (§6.1), ownership tracking with three-way handling of user edits, a sensitive-settings class, and invariants INV-12 to INV-15. Agent and bridge recommendations enter through the Suggestions Inbox (`draft_guidance`) and are never written by their originator. §18.3 now limits the optional bridge to producing recommendations. It also folds in repository-evaluation corrections (§0) and adds new work package WP6b and decisions D11–D16.
**Changes in v3:** §2 rewritten as *Sources, credit and attribution* with a per-concept source table (repository, file path, license), the attribution obligations that follow from each, and a mandatory attribution ADR in WP0 and NOTICE/THIRD_PARTY work in WP8. A *References* section (§19) lists every external source by URL. No functional change from v2.
**Changes in v2:** added §3 *Runtime dependency policy — no Node*, invariant INV-10, agent skill files as pure Markdown (§8.3), an explicitly deferred and isolated *optional Node bridge* (§18), decision D8, a no-Node qualification gate in WP4/WP8, and the corresponding risk row.

---

## 0. Repository evaluation (v4, 28 October 2026)

v3 was checked against the working tree on `feat/semindexint8`: HEAD 1.5.12, with a 1.6.0 working-tree version. The AIRK retrieval tools were not exposed to the evaluating session, so these observations come from direct reads of live files. WP0 must re-confirm them against an indexed publication.

| v3 assumption | Observed | Correction in v4 |
|---|---|---|
| Target "AIRK 1.5 on the 1.4 architecture" | Explorer shipped in 1.5; the workspace is now at 1.6.0 | Target is the first release after 1.6.0 (header) |
| Explorer has "Search/Code/Evidence, Review, Strategies, Reports & Packs, Management"; Guidance is the eighth workspace | `packages/vscode/src/explorerRoutes.ts`: Overview, Search, Code, Documents, Evidence, Review, Strategies, Management (pinned), Help (drawer). There is no Reports & Packs | Guidance is a new `main` workspace in the route manifest (§12) |
| Feature flag `airk.explorer.guidance` | Routes are gated by `explorerFeatures.ts` (`implemented` / `prototype_only`); a route becomes `implemented` only in the commit that ships it together with its help topic | Use the route availability model, not a separate flag (§12) |
| MCP tools named `airk_guidance_*` in an opt-in group | `docs/unified-mcp.md`: one `airk` server, 26 unprefixed tools; `airk_*` names are compatibility aliases hidden from discovery; proposals go through `draft_*` / `suggest_view` | Tools become `guidance_*` plus a `draft_guidance` proposal tool. Whether the unified server can hold an opt-in group is decision D14 (§10) |
| "Rust setup code that already writes agent configuration without overwriting user settings" (§3.3, §8.3) | `airk setup --client …` only **prints** a configuration (`crates/airk-cli/src/main.rs`, `Command::Setup`). Writes happen in the extension: `agentSetup.ts` and `agentConfig.ts` insert only missing servers, keep comments and formatting, refuse conflicting entries and check the document version before `applyEdit` | Skill and settings writes go through the Explorer apply path or an explicit CLI `--apply` (§8.3, §11) |
| Explorer apply via `workspace.applyEdit` only | `explorerWorkspaceIO.ts` `savePlanInWorkspace` already provides a guarded write: workspace containment, refusal of symlinks and junctions on every ancestor, refusal of dirty buffers, a snapshot hash before and after a modal confirmation, an exclusive create, and read-back hash verification | Reuse that guarded write for every Guidance file write (§12.3) |
| D4 "carrier for agent briefs" open | The Suggestions Inbox exists; proposals carry `publication`, citations with `content_hash`, `idempotency_key`, `workspace_binding` and a 16 KiB body limit | D4 is narrowed to "Inbox item vs Planner task" for briefs; guidance changes always use the Inbox (§9, §10) |
| Instruction files unknown to Explorer | `explorerContracts.ts` already defines `AGENT_INSTRUCTION_FILES` (Claude Code, Codex, Copilot) and an `AIRK_AGENT_INSTRUCTIONS` block, and Explorer opens those files for manual editing | Inventory and the GD-V04 draft reuse these definitions instead of a second list (§6.6) |
| SKILL.md export | `AIRK_Explorer_Final_Design.md` §7.7 lists "export a template or plan as `SKILL.md`" as optional P3+ and not implemented | Guidance › Skills is that export's home (§8.3, §12) |
| Explorer plan file `AIRK_Explorer_Codex_Plan_v1_4.md`; ADR name `ADR-00xx-…` | The file is `docs/explorer/AIRK_Explorer_Codex_Implementation_Plan_v1.4.md`; ADRs use `docs/decisions/NNN-slug.md` (`001-local-retrieval-kernel.md`) | Fixed in §2.2 and §2.4 |
| Settings files are indexed evidence | `.mcp.json` is git-ignored, and `.claude/settings.local.json` is personal by convention, so these may be absent from the index | A *live-only* evidence class with redaction (§6.6, INV-14) |
| "Guidance" is a free name | The Explorer Help catalog already has an area named `guidance` (how-to topics) | WP0 confirms the workspace title, or renames the help area label (D13 note) |

---

## 1. Problem and goal

Every coding agent loads a project's instruction files on every session. Those files are hand-written, drift from the code, reference commands and paths that no longer exist, omit components that were added later, and are duplicated across engines (AGENTS.md, CLAUDE.md, GEMINI.md, `.github/copilot-instructions.md`, `.cursorrules`) with no mechanism keeping them aligned. The cost is paid twice: in tokens on every session and in agent mistakes caused by stale guidance.

AIRK already holds the evidence needed to catch this: the file inventory, outlines, structured facts from Markdown/TOML/JSON/YAML, code declarations and imports, indexed changes and revision identity. **Goal:** let AIRK evaluate guidance artifacts against that evidence, produce bounded findings with citations, generate or update the artifacts deterministically from templates plus evidence, and hand the result to the user (Explorer) or a coding agent (MCP) to apply.

### Success criteria

1. On a workspace with no guidance files, `airk guidance draft agents` produces an AGENTS.md whose component list, language list, verification commands and MCP setup block are all derived from indexed evidence, each with a citation.
2. On AIRK's own repository, `airk guidance evaluate` reports zero defects on a clean checkout and reports the correct finding when AGENTS.md and CLAUDE.md are made to differ by one byte, when a referenced path is deleted, or when a crate is added without a guidance mention.
3. A coding agent can call `guidance_evaluate` and `guidance_draft`, apply the draft with its own edit tools, re-run evaluate, and see the finding closed — with no AIRK component having written to the workspace.
4. The Explorer Guidance tab shows the same findings, previews the same diff, and applies it only after an explicit user click, through the VS Code workspace-edit API.
5. Same index generation → byte-identical evaluation and draft output (determinism test).
6. Every feature above runs on a machine with **no Node.js, npm or npx installed** (§3, INV-10).
7. With skill management enabled in the profile, the Skills panel lists every project skill with its origin (AIRK-shipped, AIRK-generated, user-authored, third-party) and drift state. Updating an AIRK-managed skill the user has edited shows a three-way comparison and writes nothing until the user chooses (INV-13).
8. With settings management enabled, a recommendation to change `.claude/settings.json` or `.codex/config.toml` cites the evidence that motivates it: an indexed path, a script, a CONTRIBUTING command or a live-only read of the settings file. It previews a format-preserving diff that keeps comments and unrelated keys. A sensitive key (permission grant, hook command, environment value, MCP server command) needs its own confirmation and is never part of a batch apply (INV-14).
9. A coding agent that searched with the AIRK tools can submit a recommendation through `draft_guidance`. That recommendation appears in Guidance › Recommendations labelled *agent*, with its citations re-checked against the live source. The agent cannot apply it.

### Non-goals

- No model inference inside AIRK. Rules are deterministic; the agent supplies reasoning. (AGENTS.md: "Do not add agent reasoning, orchestration…")
- No multi-agent review, peer debate, context succession or Orca integration.
- No writes to the workspace from the `airk` service or MCP transport (INV-1).
- No Node.js at user install or run time for anything AIRK ships (INV-10).
- No claim that a generated guidance file is "complete" or "correct"; outputs are evidence-bound drafts and findings.
- No change to the existing `airk_*` retrieval tools or their schemas.
- No management of user-level (home directory) skills or settings in v1 (D12), and no writes outside the selected project's workspace folder.
- No installation, update or removal of third-party skills, plugins or marketplace packages (for example the `peer` plugin). Guidance lists them as *third-party* and leaves them alone.
- No storage, generation or display of secrets. Settings values classified as secrets are redacted everywhere Guidance reports them.
- No automatic or background apply. Every write, in every management mode, follows an explicit user action.

---

## 2. Sources, credit and attribution

This feature is a synthesis. Its ideas come from identifiable places, and AIRK's own AGENTS.md requires that inspiration be recorded truthfully and that nothing be copied without its license being honored. This section is the record; WP0 turns it into an ADR and WP8 turns it into NOTICE entries.

### 2.1 Primary design source

**`corrabora/peer-skills-and-workflows`** — "Peer skills and orchestrated workflows for Claude and Codex", by GitHub user **corrabora**, MIT License.
Repository: https://github.com/corrabora/peer-skills-and-workflows
License file: https://github.com/corrabora/peer-skills-and-workflows/blob/main/LICENSE
Examined revision: `main` as of 2026-10-28 (2 commits). Record the exact commit hash in the attribution ADR when WP0 runs.

The repository's own stance on reuse is instructive and is adopted here: it treats quality requirements as governing the package and not repositories that install it, and it states that installation establishes definitions, not trust. AIRK likewise takes the *shapes* and *disciplines* and reimplements them; it does not vendor the package.

### 2.2 Per-concept source table

| Concept in this plan | Where it comes from | Source file(s) | How it is used here | Attribution obligation |
|---|---|---|---|---|
| Guidance inventory: discover repository guidance, verification requirements and quality selection; record a source map | peer-skills `bind-agent-workflow` skill and its binding contract | `skills/bind-agent-workflow/SKILL.md`, `workflow/BINDING.md` | Reimplemented in Rust over the AIRK index (§6.2) | Credit in ADR and `docs/guidance.md`; no text reproduced |
| Finding contract: requirement → observed issue → consequence → support → recommended action; classification into *demonstrated defect / missing evidence / unresolved decision / preference* | peer-skills `peer-review` skill, "Assess independently" section; `review-adjudicator`; `workflow/REVIEW.md` severity rules | `skills/peer-review/SKILL.md`, `skills/review-adjudicator/SKILL.md`, `workflow/REVIEW.md`, `workflow/ADJUDICATION.md` | Adopted as a Rust data shape (§6.4); the four classification names are reused verbatim | Credit in ADR, `docs/guidance-rules.md` and the `Finding` type's doc comment; the four class names are short functional labels, but credit anyway |
| Agent brief shape: self-contained task, scope, authority, evidence, observable completion, report | peer-skills `write-for-agents` skill and shared writing guidance | `skills/write-for-agents/SKILL.md`, `docs/writing-guidance.md` | Principles re-expressed in the brief generator (§9) and in the shipped `airk-guidance` skill text (§8.3) | Credit in ADR and in the skill file's header comment |
| Writing guidance for prose agents produce | peer-skills `write-for-humans`, `clarify-artifact` | `skills/write-for-humans/SKILL.md`, `skills/clarify-artifact/SKILL.md` | Informs the tone rules for generated templates only | Credit in ADR |
| ADR format and revision rules | peer-skills `record-decision` skill and its ADR format guidance — which in turn derive from **Michael Nygard, "Documenting Architecture Decisions" (2011)** | `skills/record-decision/SKILL.md`, `docs/guidance/adr-format.md`, `docs/guidance/record-format-adoption.md`; Nygard's article | AIRK's `adr.md` template (§8.1) and rules GD-R\* (§7) use the Nygard section set (Context, Decision, Consequences) plus peer-skills' additions (Alternatives, Status lifecycle, no invented acceptance) | Credit both in ADR and template header; if peer-skills' wording is reproduced in `adr.md`, add MIT notice to THIRD_PARTY_NOTICES.md |
| Glossary format and "consequential vocabulary ambiguity" trigger | peer-skills `maintain-glossary` and glossary format guidance | `skills/maintain-glossary/SKILL.md`, `docs/guidance/glossary-format.md` | `glossary.md` template (§8.1), rule GD-P04 | Credit in ADR and template header; MIT notice if wording reproduced |
| Issue and PR writing with observable completion criteria | peer-skills `write-issue`, `write-pr` | `skills/write-issue/SKILL.md`, `skills/write-pr/SKILL.md` | "Observable completion" line in briefs (§9); increment-report discipline in §13 | Credit in ADR |
| Per-repository quality profile with explicit selection and loud failure on unknown keys | peer-skills `defaults.yaml`, `.quality/config.json`, resolver contract in `workflow/ORCA.md` §Configuration, and the restricted YAML reader | `defaults.yaml`, `.quality/config.json`, `workflow/ORCA.md`, `tools/lib/yaml.mjs` | Simplified to one `guidance.toml` (§6.1); the "unknown sections fail resolution rather than being ignored" rule is adopted | Credit in ADR |
| Skill distribution as plain `SKILL.md` directories discoverable by Claude Code and Codex | peer-skills `tools/install.mjs` and `docs/installation.md` §Skill links; the **Agent Skills `SKILL.md` convention** used by Claude Code and Codex | `tools/install.mjs`, `docs/installation.md` | The Guidance apply path writes plain copies natively (§8.3); the format is the engines' own convention, not peer-skills' | Credit peer-skills for the install pattern; the format itself is an engine convention and needs no license |
| Token-cost awareness of instruction files (GD-Q01) | AIRK's own usage-reporting tokenizer calibration (BPE/WordPiece/Unigram/WordLevel pipelines) | `docs/usage-reporting.md` in iokaio/airk | Reused directly | None (internal) |
| Explorer invariants, evidence classes, proposal admission through the Assistance MCP, Suggestions inbox, Planner | AIRK Explorer plan v1.4 and its review/spec (Ioka) | `docs/explorer/AIRK_Explorer_Codex_Implementation_Plan_v1.4.md`, `AIRK_Explorer_Final_Design.md` and the Explorer implementation spec | §9 and §12 route through these existing contracts | None (internal) |
| Insert-only, comment-preserving agent configuration merges; guarded workspace saves; Inbox proposal admission | AIRK extension (iokaio/airk) | `packages/vscode/src/agentConfig.ts`, `unifiedAgentConfig.ts`, `agentSetup.ts`, `explorerWorkspaceIO.ts`, `explorerSuggestions.ts` | Skill and settings apply path (§12.3), structured settings edits (§6.8) | None (internal) |
| Read-only kernel, evidence-or-nothing, honest reporting, no invented approval, AGENTS.md/CLAUDE.md byte-identical, "inspiration only — do not copy implementation" | AIRK AGENTS.md (iokaio/airk) | `AGENTS.md` | INV-1, INV-2, INV-8, INV-9; rule GD-C01; this section's existence | None (internal) |
| Orca orchestration (blocking ask/reply, task dispatch, worker_done, worktrees) | **`stablyai/orca`** and its `orchestration` skill | https://github.com/stablyai/orca, `skills/orchestration/SKILL.md` | **Not used.** Named only in §18.1 as something a user may install themselves | None; mention by name in `docs/guidance-peer-recipe.md` with link |
| MCP tool shape, stdio transport | Model Context Protocol official SDK (already an AIRK dependency) | existing `airk-mcp` | Reused | Already covered in THIRD_PARTY_NOTICES.md |

### 2.3 What is explicitly *not* taken

Orca, `peer-review` (as a protocol), `peer-decision-making`, `peer-implementation`, `successor` and its three hooks, `plan-coordinator`, `plan-responsible-party`, `plan-reviewer`, the review-seat rotation in `workflow/REVIEW.md`, the adjudication barrier in `workflow/ORCA.md`, and every file under `tools/`. These are orchestration and model-driven review, outside AIRK's scope by AGENTS.md, and they are the only parts that require Node.js (§3.3).

### 2.4 Attribution obligations (binding on WP0 and WP8)

1. **Attribution ADR (WP0).** `docs/decisions/NNN-guidance-design-sources.md` (next free number; existing convention `001-local-retrieval-kernel.md`), status *accepted* once the operator approves, listing every row of §2.2 with the peer-skills commit hash examined, the MIT license text location, and the statement that no implementation code was copied. This mirrors how AGENTS.md already treats Munarium and VCP as inspiration only.
2. **THIRD_PARTY_NOTICES.md and `third_party/licenses/` (WP8).** Add `corrabora/peer-skills-and-workflows` (MIT) **if and only if** any template, skill file, rule description or generated text reproduces its wording. The WP3 snapshot review decides this; the default intent is to re-express in AIRK's own words so the entry is unnecessary, but the decision is recorded either way in the ADR.
3. **In-code credit.** Doc comments on `Finding`, `Classification`, the brief generator and the ADR/glossary templates name the source concept and link to the ADR. Template and skill files carry a one-line header comment: `<!-- Design informed by corrabora/peer-skills-and-workflows (MIT); see docs/decisions/NNN-guidance-design-sources.md -->`.
4. **User-facing docs.** `docs/guidance.md` has a short *Credits* section naming peer-skills (MIT) and Nygard's ADRs, and stating that Orca is a separate product by stablyai that AIRK does not bundle.
5. **Release notes.** The notes for the first Guidance release credit the design sources in one sentence with links.
6. **Trademarks.** "Orca" and "peer" are referred to only descriptively, with links; AIRK ships nothing under those names.

---

## 3. Runtime dependency policy — no Node

### 3.1 Statement

Nothing in this plan requires Node.js, npm or npx on the user's machine, at install time or at run time. This is a hard requirement (INV-10), not a preference, because AIRK's value proposition is a single native binary plus an editor extension with no runtime, port, service account, API key or language runtime.

### 3.2 Where each component runs

| Component | Implementation | Runtime on the user's machine |
|---|---|---|
| `airk-guidance` crate (profile, inventory, rules, generators, diff, brief, report) | Rust | Inside the `airk` binary |
| `airk guidance …` CLI | Rust | `airk` binary |
| `guidance_*` and `draft_guidance` MCP tools | Rust, official SDK stdio transport | `airk` binary (same helper the retrieval tools use) |
| Templates (AGENTS, CLAUDE, pointers, ADR, glossary, profile) | Markdown/TOML embedded with `include_str!` | None — compiled in |
| Token counting for GD-Q01 | Existing Rust tokenizer calibration pipeline | `airk` binary |
| Explorer Guidance tab | TypeScript, like the rest of `packages/vscode` | VS Code's **extension host**, which VS Code ships itself. No user-installed Node. Identical to today's extension. |
| Agent skill files (`SKILL.md`) | Markdown | None. Plain files written into project `.claude/skills/` and `.agents/skills/` by the Guidance apply path (Explorer, or CLI `--apply`) |
| Skill and settings management (registry, ownership manifest, change sets, three-way merge, structured settings edits, redaction) | Rust in `airk-guidance` (`manage/`); guarded writes in the extension host | `airk` binary + VS Code extension host |
| Evaluate-on-session hooks (optional, §8.4) | Hook command is `airk guidance evaluate …` | `airk` binary; no script interpreter |

Contributor build tooling for `packages/vscode` (TypeScript compiler, `vsce` packaging) is unchanged and remains build-time only, as it is today.

### 3.3 Why peer-skills needs Node and this plan does not

peer-skills' Node requirement comes entirely from its `tools/` directory, not from its skills. Each piece has a native equivalent here or is unnecessary:

| peer-skills Node tool | Purpose there | Here |
|---|---|---|
| `tools/resolve.mjs` + vendored YAML/Ajv | Layer `defaults.yaml` → repo → plan → phase → operator; validate shape | Single `guidance.toml` with `serde` + `toml` and explicit validation errors (§6.1). Layering beyond file-plus-defaults is not needed for v1. |
| `tools/install.mjs` | Symlink/junction skills into `~/.claude/skills` and `~/.agents/skills`; merge successor hooks into `settings.json` / `hooks.json` | Guidance › Skills and `airk guidance draft skill … --apply` write plain copies into **project** skill directories (no links; no junction rules; user-level deferred, D12). Hook entries are a reviewed, sensitive settings change (§8.4, INV-14). `airk setup` itself still only prints MCP configuration; v3's statement that it writes configuration was wrong (§0) |
| `tools/peer-records.mjs` | Resolve record/scratch roots for Orca workers | Not needed; no workers |
| `tools/verify.mjs`, `validate.mjs`, `generate-*.mjs`, `install-hooks.mjs`, `commit-verify.mjs` | Package self-checks and commit gate | AIRK's existing `cargo` checks and `scripts/check-repo.ps1` |
| successor `SessionStart`/`UserPromptSubmit`/`Stop` hook scripts | Context succession | Not adopted. If AIRK ever offers a hook, the hook command is the `airk` binary itself. |

The skills in peer-skills are plain `SKILL.md` text; only their *distribution* was Node. AIRK distributes skill text natively.

### 3.4 Enforcement

- **INV-10** (below) plus a CI qualification step: run the WP4 smoke test (`airk setup`, `airk guidance inventory|evaluate|draft`, MCP stdio round-trip) on a runner with `node`, `npm` and `npx` removed from `PATH`, on all three platforms.
- `scripts/check-repo.ps1` gains a check that no release artifact, setup output, generated agent config or skill file references `node`, `npm` or `npx` as a required command.
- `docs/agent-integration.md` and the README state the no-Node guarantee next to the existing "no Python bridge, port, service account or API key" sentence.

---

## 4. Invariants

- **INV-1 Read-only kernel.** `airk-guidance`, the `airk` service and `airk-mcp` never create, modify or delete workspace files. Drafts, change sets and diffs are returned as data. Only three apply paths exist: (a) the coding agent's own edit tools; (b) Explorer's guarded workspace write (§12.3) after explicit user confirmation; (c) the CLI when the user passes `--apply` (CLI runs as the user, like `airk index`). A recommendation submitted through `draft_guidance` or by the optional bridge (§18) is inert data until the user applies it through (b) or (c).
- **INV-2 Evidence or nothing.** Every finding and every generated section carries at least one citation: path, content hash, original location, indexed revision id. A rule that cannot cite evidence reports *missing evidence*, not a pass and not a defect.
- **INV-3 Determinism.** Identical published generation + identical profile → identical inventory, findings and drafts. No wall-clock, environment or ordering nondeterminism in output (timestamps live in a separate `run` envelope, not in content).
- **INV-4 Freshness is explicit.** Outputs name the generation they were computed from. If the live file differs from the indexed hash at apply time, Explorer and the CLI refuse to apply and report the mismatch; the MCP response includes the indexed hash so the agent can check before editing.
- **INV-5 Tool cost isolation.** Guidance MCP tools are registered in a separate, opt-in tool group so agents that only need retrieval do not pay schema tokens for them.
- **INV-6 Existing tools unchanged.** The current `airk_*` retrieval tool schemas, names and semantics are not modified.
- **INV-7 Advisory never blocks.** Rules classified *preference* or *heuristic* can never produce a `defect`; they produce `advisory` at most and are off by default where noisy.
- **INV-8 No invented approval.** Generated files never contain sign-offs, "reviewed by", "approved", or version claims not present in evidence. ADR drafts are created with status `proposed`.
- **INV-9 Honest reporting.** Rule results state what was checked, what could not be checked (unindexed paths, unsupported formats) and why.
- **INV-10 No Node at user install or run time.** No feature shipped in the `airk` binary, the VSIX, the generated agent configurations or the shipped skill files requires Node.js, npm or npx. The VS Code extension host is not a user-installed Node. Any future Node-dependent integration is a separately distributed, separately installed, runtime-detected optional package (§18) and never a dependency of the core.
- **INV-11 Credit travels with the code.** Every type, template or skill whose design derives from §2.2 carries the in-code credit described in §2.4; a reviewer can trace any adopted concept to its ADR row without leaving the repository.
- **INV-12 Management is the user's choice.** Each managed class (instructions, skills, settings, records) has a mode in the profile: `off`, `report`, `suggest` or `apply` (§6.1). The defaults are `suggest` for instructions and records, `report` for skills and `off` for settings. Only `apply` enables an Apply button, and the button still needs a click. Changing a mode is itself a reviewed profile edit. No mode writes in the background, on index change or on session start.
- **INV-13 User edits win.** Guidance records the hash of what it last applied to each managed file or section (ownership manifest, §6.9). If the live content differs from that hash, an update is a three-way comparison (last applied / live / proposed), and nothing is written until the user picks a resolution. Files and sections Guidance did not create are *user-authored*; they get proposals but are never deleted, and they are never adopted as managed without an explicit *Adopt*.
- **INV-14 Sensitive settings.** Settings keys that grant permissions, run commands (hooks, MCP server `command`/`args`, status-line commands, API-key helpers), set environment values or select model/provider endpoints are *sensitive*. A sensitive change is shown with its exact text, needs its own confirmation, cannot be batch-applied and is never a default recommendation. Recommendations never broaden a permission without citing the evidence that requires it. Values that look like secrets are redacted in every MCP, CLI and report output. Explorer shows them only in the local diff of the user's own file, and only when the user expands it.
- **INV-15 Live source is re-verified.** A recommendation is grounded in indexed evidence (publication + content hashes) and is re-checked against the live original source before it can be applied. If any cited file is `changed_since_index`, `not_saved` or `missing` (the existing `LiveCheck` states), the recommendation is marked stale and Apply is disabled until it is re-evaluated. Settings files that are not indexed use the *live-only* evidence class: Explorer reads the live file locally, hashes it and labels it as such.

---

## 5. Architecture

### 5.1 Placement

New crate **`crates/airk-guidance/`** with an independent purpose (rule engine + generators + profile), usable from CLI, MCP and the service. Dependency direction (must be reflected in `docs/architecture.md`):

```
airk-core ──► airk-guidance ◄── airk-kernel (evidence queries, collection scope)
                   ▲
       airk-cli ───┤
       airk-mcp ───┘        packages/vscode (Explorer Guidance tab, via service API)
```

`airk-guidance` depends on `airk-core` contracts and reads evidence through `airk-kernel` services only. It has no dependency on `airk-store` or `airk-retrieval` directly, no filesystem writes, and no process spawning.

**Decision D1 (WP0):** if `airk-kernel` already owns a query-driven rule engine for Explorer Review (fingerprinted findings), reuse that engine's finding/fingerprint types and put guidance rules behind the same trait rather than inventing a second engine. If it does not exist yet, `airk-guidance` defines the trait and Explorer Review adopts it later.

### 5.2 Modules

```
crates/airk-guidance/src/
  lib.rs
  profile/        guidance.toml schema, defaults, validation
  inventory/      artifact discovery, verification-command discovery, component discovery
  evidence/       typed queries over airk-kernel (files, outlines, doc facts, code facts, manifests)
  rules/          Rule trait, registry, catalog GD-* (one file per family)
  findings/       Finding, Citation, Fingerprint, Classification, Severity   (credit: §2.2 row 2)
  generate/       templates (AGENTS, CLAUDE, GEMINI, copilot, cursor, ADR, glossary), section fillers
  diff/           unified diff against indexed content; apply-preconditions (hash match)
  brief/          agent task-brief generator                                   (credit: §2.2 row 3)
  manage/         managed-artifact registry (engines × kinds × paths, data-driven), ownership
                  manifest, change sets, three-way merge, structured settings ops (JSON/JSONC/TOML,
                  comment- and format-preserving), sensitive-key classifier, secret redaction,
                  recommendation admission checks for agent/bridge proposals
  report/         JSON / Markdown report writers (same shapes as CLI and MCP)
crates/airk-guidance/templates/   Markdown/TOML templates, include_str! at build (headers per §2.4.3)
crates/airk-guidance/skills/      SKILL.md files shipped verbatim through the Guidance apply path (headers per §2.4.3)
```

### 5.3 Data flow

```
index (published generation)
   │
   ▼
inventory ──► rules ──► findings ──► report  ──┐
   │                                            ├─► MCP (guidance_*)
   └──► generators ──► draft + diff + manifest ─┤    CLI (airk guidance …)
                                                │    Service API → Explorer Guidance tab
live-only reads (Explorer host, settings files) ┤
                                                ▼
coding agent ── search/read_file/… ── draft_guidance ──► Suggestions Inbox ──┐
optional bridge (§18) ── review report ── draft_guidance ──────────────────┤
native findings ── recommended_action ──────────────────────────────────────┤
                                                                            ▼
                          Guidance › Recommendations (origin: rule | agent | bridge)
                                                                            │  re-verify live source (INV-15)
                                                                            ▼
                          change set preview (three-way if user-edited, INV-13)
                                                                            │  user click; per-item confirm if sensitive (INV-14)
                                                                            ▼
                          guarded write (§12.3) ──► ownership manifest ──► reindex request ──► re-evaluate
```

Recommendations from every origin converge on one change-set contract and one apply path. Origin affects labelling and gating only. Agent and bridge recommendations are built from untrusted indexed text, so they are always validated (§6.7) and never pre-selected for apply.

---

## 6. Contracts

### 6.1 Guidance profile — `.airk/guidance.toml` (tracked)

```toml
schema = 1

[artifacts]
primary   = "AGENTS.md"                 # canonical instruction file
mirrors   = ["CLAUDE.md"]               # must be byte-identical to primary
pointers  = ["GEMINI.md", ".github/copilot-instructions.md"]  # short files that point to primary
engines   = ["codex", "claude", "gemini", "copilot"]
decisions = "docs/decisions"            # ADR directory
glossary  = "docs/glossary.md"

[budget]
instruction_tokens = 4000               # per-engine instruction set, measured with calibrated tokenizer
tokenizer = "cl100k_base"               # must exist in AIRK tokenizer calibration catalog

[rules]
disable  = ["GD-V04"]
severity = { "GD-Q02" = "advisory" }

[verification]
sources = ["CONTRIBUTING.md", "scripts/", ".github/workflows/"]

[manage]                                # INV-12; modes: off | report | suggest | apply
instructions = "suggest"
records      = "suggest"
skills       = "report"
settings     = "off"

[manage.skills]
engines   = ["claude", "codex"]         # project dirs .claude/skills/, .agents/skills/
mirror    = true                        # same skill in both engine dirs must be identical (GD-S04)
shipped   = ["airk-guidance", "airk-evidence", "airk-record-decision"]
generated = ["verify", "release"]       # evidence-derived project skills (§8.3)

[manage.settings]
files     = [".claude/settings.json", ".codex/config.toml"]   # tracked, shared settings only
personal  = "report"                    # .claude/settings.local.json etc.: at most report
sensitive = "off"                       # off | suggest; "apply" is not accepted (INV-14)
mcp       = "delegate"                  # MCP server entries are owned by Management › Agents
```

- Missing file → built-in defaults (primary AGENTS.md, mirror CLAUDE.md, no ADR dir, no glossary, the `[manage]` defaults above) and a finding `GD-P00 profile absent` at `advisory`.
- `[manage]` is read only from the tracked profile. A per-user override that may only *lower* a mode lives in VS Code workspace state, never in a file. A teammate's profile therefore cannot raise another user's settings mode to `apply` without that user's own choice: raising any mode to `apply` requires a confirmation in the user's own Explorer, recorded per workspace binding.
- `mcp = "delegate"`: findings about MCP server entries (GD-A01) open Management › Agents (`suggest_view` intent `agents.setup`), which keeps sole ownership of insert-only MCP merges. Guidance never edits the `mcpServers`, `servers`, `mcp` or `[mcp_servers.*]` keys.
- Unknown keys fail validation loudly: report, do not ignore. (Discipline adopted from peer-skills' configuration contract, §2.2.)
- **Decision D2 (WP0):** confirm `.airk/` is appropriate for a tracked file given `.airk/collections/` is ignored state; alternative is `airk.guidance.toml` at root.

### 6.2 Inventory

```jsonc
{
  "generation": "<published generation id>",
  "profile": { "path": ".airk/guidance.toml", "hash": "…", "source": "file|defaults" },
  "artifacts": [
    { "role": "primary", "path": "AGENTS.md", "hash": "…", "exists": true, "tokens": { "count": 1412, "tokenizer": "cl100k_base" } },
    { "role": "mirror",  "path": "CLAUDE.md", "hash": "…", "exists": true, "identical_to_primary": true }
  ],
  "components": [ { "kind": "cargo-crate", "name": "airk-core", "path": "crates/airk-core", "manifest": "crates/airk-core/Cargo.toml", "hash": "…" } ],
  "languages": [ { "name": "rust", "files": 312 }, { "name": "typescript", "files": 58 } ],
  "verification": [
    { "command": "cargo clippy --workspace --all-targets --locked -- -D warnings", "source": "CONTRIBUTING.md#L41", "hash": "…" },
    { "command": "pwsh -NoProfile -File scripts/check-repo.ps1", "source": "CONTRIBUTING.md#L44", "hash": "…" }
  ],
  "ci_gates": [ { "workflow": ".github/workflows/ci.yml", "jobs": ["fmt", "clippy", "test"] } ],
  "decisions": { "dir": "docs/decisions", "exists": false, "records": [] },
  "glossary":  { "path": "docs/glossary.md", "exists": false },
  "unindexed": [ "temp/", "private/" ],
  "limits": [ "PDF guidance not evaluated", "…" ]
}
```

Component discovery is adapter-based: Cargo workspace members, `package.json` workspaces, `.csproj`/`.sln`, `go.mod`, `pyproject.toml`. Each adapter lives in `inventory/components/` and reports what it could not read. Adapters parse manifests from the index; they never execute a package manager.

### 6.3 Evidence queries (`evidence/`)

Typed wrappers over existing kernel services; no new index structures in v1:

- `files_matching(glob)`, `file_exists(path)`, `file_hash(path)`
- `outline(path)` → headings with line ranges
- `doc_facts(path, category)` → requirements, decision language, dates, numeric bounds
- `manifest_facts(path)` → TOML/JSON/YAML key paths (version, workspace members, scripts)
- `declarations(component)` → counts and public symbol names (for coverage rules)
- `changes_since(generation)` → indexed change list
- `tokens(path, tokenizer)` → via usage-reporting calibration pipeline

**Decision D3 (WP0):** verify which of these are already exposed by `airk-kernel` for the existing MCP tools; add only thin adapters, never duplicate query logic.

### 6.4 Finding

Shape adopted from peer-skills' peer-review assessment structure (§2.2 row 2).

```jsonc
{
  "id": "GD-C02",
  "fingerprint": "sha256(rule id + normalized subject + evidence hashes)",
  "classification": "defect | missing_evidence | unresolved_decision | preference",
  "severity": "defect | warning | advisory",
  "subject": { "path": "AGENTS.md", "range": { "start_line": 17, "end_line": 17 } },
  "requirement": "Referenced paths must exist in the indexed workspace",
  "observation": "AGENTS.md references `scripts/check-repo.sh`; no such path is indexed",
  "consequence": "Agents will attempt a command that cannot run",
  "support": [ { "path": "AGENTS.md", "hash": "…", "line": 17 }, { "query": "file_exists(scripts/check-repo.sh)", "result": false, "generation": "…" } ],
  "recommended_action": { "kind": "replace", "path": "AGENTS.md", "draft_ref": "drafts/agents.md#section=verification" },
  "limits": [ "Shell aliases and PATH binaries are not indexed and were not checked" ]
}
```

Fingerprints are stable across re-runs so Explorer can mark findings dismissed/acknowledged and the CLI can diff finding sets between generations.

### 6.5 Draft envelope

```jsonc
{
  "generation": "…",
  "artifact": { "role": "primary", "path": "AGENTS.md", "indexed_hash": "…|null" },
  "content": "…full proposed file…",
  "diff": "…unified diff vs indexed content, or null when creating…",
  "sections": [ { "id": "components", "evidence": [ …citations… ], "generated": true },
                { "id": "project-rules", "evidence": [], "generated": false, "preserved_from": "AGENTS.md#L8-L30" } ],
  "apply_preconditions": { "live_hash_must_equal": "…" },
  "warnings": [ "Section 'release' preserved verbatim; could not validate against evidence" ]
}
```

Generation is **merge-preserving**: hand-written sections are kept verbatim and marked `preserved_from`; only evidence-backed sections (components, languages, verification, MCP setup, mirror/pointer blocks) are regenerated. Section boundaries are detected by a marker convention (`<!-- airk:section components -->` … `<!-- /airk:section -->`); files without markers get a single generated block appended and a finding recommending marker adoption.

### 6.6 Managed-artifact registry

The registry is data embedded in the binary; adding an engine means adding rows, not code. It extends the existing `AGENT_INSTRUCTION_FILES` in `explorerContracts.ts` and does not duplicate it: WP0 moves that list into a shared contract generated from, or tested against, this registry.

| Kind | Engine | Project path | Format | Evidence class | Default ownership |
|---|---|---|---|---|---|
| instructions | codex / claude / copilot / gemini / cursor | `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md`, `GEMINI.md`, `.cursorrules` | Markdown | indexed | user-authored unless created by Guidance |
| skill | claude | `.claude/skills/<name>/SKILL.md` (+ files in that dir) | Markdown + YAML front matter | indexed if not ignored, else live-only | per manifest |
| skill | codex | `.agents/skills/<name>/SKILL.md` (+ files) | same | same | per manifest |
| settings (shared) | claude | `.claude/settings.json` | JSON | indexed if tracked, else live-only | user-authored |
| settings (personal) | claude | `.claude/settings.local.json` | JSON | live-only | user-authored; at most `report` |
| settings (shared) | codex | `.codex/config.toml` | TOML | indexed if tracked | user-authored |
| settings (shared) | gemini | `.gemini/settings.json` | JSON | indexed if tracked | user-authored |
| MCP config | claude / vscode / cursor / … | `.mcp.json`, `.vscode/mcp.json`, … | JSON | live-only | **owned by Management › Agents** (report only here) |

- **Inventory additions** (§6.2): `skills[]` with `{engine, name, dir, files[{path, hash}], front_matter{name, description}, origin: shipped|generated|user|third_party, managed_version, drift: none|outdated|user_edited|diverged}` and `settings[]` with `{engine, path, format, tracked, evidence_class, hash, keys_checked[], sensitive_keys[], redacted: true}`. Sensitive *values* never appear; only key paths and their classification do.
- **Origin detection** is deterministic: *shipped* means the name and the content hash match a shipped skill version; *generated* means a manifest entry exists; *third_party* means a known plugin marker or a path outside both (listed, never managed); everything else is *user*.
- **Live-only reads** happen only in the Explorer extension host (or the user's CLI process). They are bounded at 1 MiB, the same limit as `agentConfig.ts`, and reject symlinks or junctions. Live bytes go to the service only as a request body for a render/evaluate call, are not persisted and are not exposed through MCP.

### 6.7 Recommendation

A recommendation is a Finding (§6.4) or an admitted proposal, plus a change set:

```jsonc
{
  "id": "rec-…", "origin": "rule|agent|bridge",
  "finding": "GD-A03 | null",                 // agent/bridge proposals may cite a rule or stand alone
  "title": "Add CONTRIBUTING verification commands to the verify skill",
  "rationale": "…",                           // agent/bridge text is shown as untrusted, never as AIRK's claim
  "evidence": { "publication": "…", "citations": [ { "path": "CONTRIBUTING.md", "start_line": 41, "content_hash": "…" } ],
                "live": [ { "path": ".claude/settings.json", "sha256": "…", "class": "live-only" } ] },
  "live_check": "matches_index|changed_since_index|not_saved|missing",   // recomputed at preview (INV-15)
  "change_set": "cs-…",
  "sensitive": false,
  "applicable": true, "blocked_by": []        // e.g. ["mode:settings=off", "stale:CONTRIBUTING.md"]
}
```

Admission checks for `agent` and `bridge` origins, run by `manage/` before the item appears:

- Every target path is in the registry and inside the bound project root.
- The change set validates against §6.8.
- Every citation exists in the named publication with the stated hash.
- The body does not touch MCP keys (delegated) or user-level paths (D12).
- Items that touch sensitive keys are flagged and admitted only if `[manage.settings] sensitive` is not `off`.

Rejected proposals keep the existing Inbox machine-readable reasons (`proposal_status`).

### 6.8 Change set

```jsonc
{
  "id": "cs-…", "generation": "…", "binding": "<workspace binding>",
  "files": [
    { "path": ".claude/skills/verify/SKILL.md", "op": "create|replace|section|structured|delete",
      "precondition": { "live_sha256": "…|absent", "last_applied_sha256": "…|null" },
      "content": "…",                                   // create/replace/section
      "edits": [ { "pointer": "/permissions/allow/-", "action": "insert_if_missing", "value": "Bash(cargo test --workspace)",
                   "sensitive": true, "reason_ref": "rec-…" } ],      // structured (JSON/JSONC/TOML)
      "diff": "…unified diff, rendered against the live bytes…" }
  ],
  "order": [0, 1], "atomic": false
}
```

- **Structured edits** address keys by JSON Pointer (TOML: dotted key path). The allowed actions are `insert_if_missing`, `set` and `remove`. Rendering preserves comments, ordering, indentation and EOL, which is the behaviour `agentConfig.ts` and `unifiedAgentConfig.ts` already guarantee for MCP entries. **Decision D11:** one Rust renderer in `manage/` that receives the live bytes (Explorer and CLI then show the same diff), or reuse the extension's `jsonc-parser` editors with a cross-implementation byte-equality test.
- **`delete`** is allowed only for files whose live hash equals `last_applied_sha256` in the manifest, which means Guidance created them and the user has not changed them. Everything else gets an *Open in editor* action instead.
- Duplicate keys, invalid JSON/TOML or a file over 1 MiB stop the change set for that file with the existing "fix it before installing" style messages. Nothing is half-edited.

### 6.9 Ownership manifest

`.airk/guidance-managed.toml` records, for each file or `airk:section` that Guidance wrote: path, section id, kind, origin (`shipped:<skill>@<version>` / `generated:<generator>` / `rule:<id>` / `agent:<proposal id>` / `bridge:<run id>`), `last_applied_sha256`, generation and evidence fingerprint. The manifest contains no timestamps (INV-3). The applying party writes it in the same guarded write as the change set. If it is missing or invalid, Guidance treats every file as user-authored, which is the safe direction. **Decision D16:** whether the manifest is tracked (shared ownership across the team) or ignored (per-clone), and therefore whether it lives beside `guidance.toml` (D2).

---

## 7. Rule catalog v1

| ID | Family | Check | Evidence | Default severity | Fix |
|---|---|---|---|---|---|
| GD-P00 | Presence | Guidance profile absent | profile load | advisory | draft `guidance.toml` |
| GD-P01 | Presence | No primary instruction file at root | `file_exists` | defect | draft AGENTS.md |
| GD-P02 | Presence | Declared mirror/pointer missing | profile + `file_exists` | warning | draft mirror/pointer |
| GD-P03 | Presence | ADR dir declared but absent | profile + `file_exists` | warning | create dir + README template |
| GD-P04 | Presence | Glossary declared but absent | profile + `file_exists` | advisory | draft glossary |
| GD-C01 | Consistency | Mirror not byte-identical to primary | hashes | defect | regenerate mirror |
| GD-C02 | Consistency | Referenced path not indexed | regex on backticked/linked paths + `file_exists` | defect | list + replacement |
| GD-C03 | Consistency | Verification commands in guidance ≠ CONTRIBUTING/scripts/CI | inventory.verification vs doc facts | warning | regenerate verification section |
| GD-C04 | Consistency | Version strings in guidance ≠ manifest versions | regex semver + `manifest_facts(version)` | warning | replace |
| GD-C05 | Consistency | Pointer file does not reference primary | outline + link scan | warning | regenerate pointer |
| GD-C06 | Consistency | Guidance or skill text requires a runtime not evidenced in the workspace (e.g. `npx …` with no `package.json`) | command scan vs manifests | advisory | report |
| GD-V01 | Coverage | Workspace component never mentioned in guidance | inventory.components vs text | warning | add components section |
| GD-V02 | Coverage | Language with ≥N files has no conventions mention | inventory.languages | advisory | add languages section |
| GD-V03 | Coverage | No verification gate stated | inventory.verification vs guidance | defect | add verification section |
| GD-V04 | Coverage | No AIRK MCP usage instruction while a collection exists for this root | collection config | advisory (off by default) | add MCP block |
| GD-Q01 | Quality | Instruction set exceeds token budget | `tokens()` | warning | report largest sections |
| GD-Q02 | Quality | Dated/historical content inside current guidance | doc facts: dates in imperative sections | advisory | suggest move to release notes |
| GD-Q03 | Quality | Same topic, conflicting directives across engine files | heading/keyword overlap | advisory | report pairs only |
| GD-R01 | Records | Decision language in docs with no ADR referencing the subject | doc facts: decision cues vs ADR index | advisory | draft ADR (status proposed) |
| GD-R02 | Records | ADR missing required sections | outline vs template | warning | insert missing sections |
| GD-R03 | Records | ADR status missing or not in allowed set | doc facts | warning | insert status |
| GD-S01 | Skills | Skill front matter missing, unparsable, or `name` ≠ directory name | outline + front-matter parse | warning | section edit of front matter |
| GD-S02 | Skills | Skill references a path, script or command that is not indexed or not evidenced (GD-C02/GD-C06 applied to skills) | `file_exists`, manifests, verification inventory | warning | replace or remove reference |
| GD-S03 | Skills | AIRK-shipped skill older than the binary's shipped version | manifest + shipped hash table | advisory | update (three-way if user-edited) |
| GD-S04 | Skills | Same skill name in `.claude/skills/` and `.agents/skills/` with different content while `mirror = true` | hashes | warning | regenerate mirror copy |
| GD-S05 | Skills | Generated project skill (e.g. `verify`) drifted from its evidence: verification commands or component list changed since last apply | manifest evidence fingerprint vs inventory | warning | regenerate generated sections |
| GD-S06 | Skills | Skill description over budget or skill body over token budget | `tokens()` | advisory | report largest sections |
| GD-S07 | Skills | Recommended shipped skill not installed (replaces the install half of GD-V04) | collection config + inventory | advisory (off by default, D6) | create from shipped skill |
| GD-A01 | Settings | MCP entry for this project missing or not matching the registered launcher | live-only read + registration | warning | **delegate**: open Management › Agents |
| GD-A02 | Settings | Settings file invalid, duplicate keys, or over the 1 MiB edit limit | parse | defect | report only (user fixes) |
| GD-A03 | Settings | Permission rule names a command, script or path that does not exist in the indexed workspace | permission key paths vs `file_exists` / verification inventory | warning | `remove` (sensitive) |
| GD-A04 | Settings | Hook or status-line command references a missing executable path or a runtime not evidenced in the workspace | hook commands vs `file_exists` / manifests | warning | report; `remove` (sensitive) |
| GD-A05 | Settings | Value that looks like a secret in a **tracked** shared settings file | pattern + git-tracked flag; value never emitted | defect | recommend moving to personal settings; no automatic edit |
| GD-A06 | Settings | Shared and personal settings disagree on the same key (personal silently overrides team policy) | key paths across both files | advisory | report pairs only |
| GD-A07 | Settings | Verification commands from CONTRIBUTING are not allowed in shared permission rules (agents will prompt on every gate) | verification inventory vs permission rules | advisory, *preference* (off by default) | `insert_if_missing` (sensitive) |

Rules must each declare: evidence they need, what they cannot check, and whether they are heuristic. GD-Q\*, GD-C06, GD-R01, GD-S06, GD-A06 and GD-A07 are heuristic and bound by INV-7. GD-R\* section and status requirements follow the ADR format credited in §2.2. GD-A\* rules read settings through the live-only class when the file is not indexed, and report which class they used. A GD-A fix touching a sensitive key carries `sensitive: true` and inherits INV-14 whatever the rule's severity. Settings rules never propose broadening permissions by wildcard; GD-A07 proposes only exact command strings copied from cited evidence.

---

## 8. Generators, templates and shipped skills

### 8.1 Templates

Templates live in `crates/airk-guidance/templates/` as Markdown with `{{section}}` slots and the `airk:section` markers, embedded with `include_str!`. Each carries the credit header from §2.4.3. Minimum set:

- `agents.md` — Purpose, Components (from inventory), Languages & conventions (slot), Verification (from inventory), Documentation rules (slot), MCP evidence access (from collection config: actual helper path, root, collection), Boundaries (slot).
- `claude.md` — mirror: exact copy of primary (mirror policy) or a pointer stub (pointer policy).
- `gemini.md`, `copilot-instructions.md`, `cursorrules` — pointer stubs by default; full mirror if profile says so.
- `adr.md` — Title, Status (proposed), Date (inserted at apply time by the applying party, not by the generator — INV-3), Context, Decision, Alternatives, Consequences, Evidence. Section set per Nygard (2011) with peer-skills' additions; see §2.2.
- `adr-README.md` — index of records, regenerated from the `decisions/` directory.
- `glossary.md` — Term, Meaning, Where defined (citation), Aliases.
- `guidance.toml` — defaults with comments, including `[manage]` at its INV-12 defaults.
- `skill-verify.md`, `skill-release.md`, `skill-export.md` — project-skill templates with YAML front matter (`name`, `description`) and `airk:section` markers (§8.3).
- No settings-file templates: settings changes are always structured edits on the live file (§6.8), never whole-file templates.

### 8.2 Generators

Pure functions `(inventory, profile, existing_content) -> Draft`. No I/O, no process spawning.

### 8.3 Shipped agent skills (pure Markdown)

This is the piece of peer-skills most worth porting (credit: §2.2, "skill distribution" row), and it needs no runtime at all. AIRK ships `SKILL.md` files under `crates/airk-guidance/skills/`, embedded in the binary with a version and content hash. They are offered as `create` change sets into the **project** skill directories (`.claude/skills/`, `.agents/skills/`) through Guidance › Skills or `airk guidance draft skill <name> --apply`. User-level directories are deferred (D12). An existing skill of the same name is never overwritten: if it is user-authored, the collision is reported with *Open*, *Install under another name* and *Adopt* (INV-13); if it is an older shipped copy, a three-way update is offered.

- `airk-guidance/SKILL.md` — when to call `guidance_inventory|evaluate|draft|explain` and `draft_guidance`, how to apply a draft (check `indexed_hash` against the live file first), how to verify (re-run evaluate, cite the closed fingerprint), what the agent must never claim (completeness, approval). Brief discipline per §2.2 row 3.
- `airk-evidence/SKILL.md` — the retrieval-first discipline already stated in AIRK's AGENTS.md ("use AIRK MCP first…bounded reads…verify live source before edits"), expressed as a reusable skill any project can install instead of hand-copying the paragraph.
- `airk-record-decision/SKILL.md` — how to draft an ADR with `guidance_draft` (`artifact: "adr"`), fill Context/Decision from evidence, and leave status `proposed`. Credits peer-skills `record-decision` and Nygard in its header.

The `airk-guidance` skill is also what GD-V04/GD-S07 recommend adding, so the recommendation and the installable artifact are the same thing.

**Evidence-generated project skills.** Besides the shipped skills, generators produce project-specific skills whose generated sections are filled from the inventory, each section with citations (INV-2), and whose hand-written sections are preserved by `airk:section` markers:

- `verify/SKILL.md`: the verification gates found in CONTRIBUTING, `scripts/` and CI (§6.2 `verification`), in their documented order, with the source line for each.
- `release/SKILL.md`: version files and lockfiles that must agree (from `manifest_facts(version)`), plus the documentation set the repository's own guidance says to update before a release. Generated only when that evidence exists.
- **Strategy/plan export** (Final Design §7.7, P3+): a Studio strategy or Planner plan saved as a skill. The exported skill describes the AIRK tool calls the strategy uses; it does not run them.

GD-S05 keeps generated skills aligned with their evidence. A generated skill the user edited outside its generated sections is updated section by section; a user edit inside a generated section triggers the INV-13 three-way comparison.

### 8.4 Optional session hooks (no interpreter)

If a user wants guidance drift surfaced at session start, Guidance › Agent settings can offer a `SessionStart` hook entry whose command is `airk guidance evaluate --collection <c> --severity defect --md`. The hook command is the native binary, resolved through the stable launcher that Management › Agents already uses. The entry is a **sensitive structured change** (INV-14): opt-in, previewed with its exact command text and confirmed on its own. The CLI equivalent is `airk guidance draft settings --hook session-start [--apply]`. **Decision D9:** include in the first Guidance release or defer.

---

## 9. Agent handoff (Explorer → coding agent)

For actions that need judgment (writing the "Boundaries" section, resolving GD-Q03 conflicts, filling an ADR's Context), Explorer does not guess. It produces an **agent brief** in the shape credited in §2.2 row 3 and routes it through the existing Explorer Assistance MCP proposal path / Suggestions inbox from the Explorer plan:

```
# Task: Resolve GD-Q03 conflict between AGENTS.md and CLAUDE.md on test policy
Scope: AGENTS.md L40-L52, CLAUDE.md L38-L50 only.
Authority: edit those ranges; do not change other sections; do not bump versions.
Evidence: <citations with hashes and generation>
Observable completion: `airk guidance evaluate --rule GD-Q03` reports no finding for fingerprint <fp>; CLAUDE.md byte-identical to AGENTS.md (GD-C01 clean).
Report: paste evaluate output; state what was not verified.
```

**Decision D4 (WP0):** the Suggestions Inbox admission contract exists (publication-bound citations, idempotency key, workspace binding, 16 KiB body, machine-readable rejection reasons). The remaining choice is whether a *brief* is carried as an Inbox item or as a Planner task (`draft_plan`). *Recommendations with change sets* always use `draft_guidance` and the Inbox.

The reverse direction is the management loop. The agent does the reasoning; AIRK keeps it grounded:

1. The agent reads `guidance_evaluate` findings and searches with the evidence tools (`search`, `read_file`, `document_facts`, `outline`, `changes`).
2. It composes a change set (for example, add a missing gate to `verify/SKILL.md`, or remove a permission for a deleted script).
3. It submits the change set with `draft_guidance`, citing the publication and content hashes.
4. The user reviews and applies it in Guidance › Recommendations.
5. The agent may poll `proposal_status`; *applied* means written by the user's action and does not mean verified. After reindex, `guidance_evaluate` shows whether the finding closed.

---

## 10. MCP tools (opt-in group `guidance` on the unified `airk` server)

Names follow the unified catalog convention (`docs/unified-mcp.md`): unprefixed, with the verb carrying the distinction. All are read-only or inert-proposal; all return the `generation` they used. Settings data returned over MCP is limited to key paths and classifications; values are redacted (INV-14), and live-only files appear only as `{path, evidence_class, sha256}` unless the user's Explorer supplied them for that call.

| Tool | Input | Output |
|---|---|---|
| `guidance_inventory` | `{ kinds?: ("instructions"\|"skills"\|"settings"\|"records")[] }` | Inventory (§6.2, §6.6) |
| `guidance_evaluate` | `{ rules?: string[], paths?: string[], severity_min?: string }` | `{ findings: Finding[], checked: string[], skipped: {rule, reason}[] }` |
| `guidance_draft` | `{ artifact: "agents"\|"claude"\|"pointer"\|"adr"\|"glossary"\|"profile"\|"skill"\|"settings", path?: string, name?: string, options?: {...} }` | Draft envelope (§6.5) or change set (§6.8) |
| `guidance_explain` | `{ rule: string }` | Requirement text, evidence used, limits, fix kind, sensitivity, and the source credit for the rule's design where one applies |
| `draft_guidance` | `{ title, rationale, change_set \| draft_ref, evidence: {publication, citations[]}, finding?, idempotency_key, workspace_binding }` | Inbox receipt; admission per §6.7. Never writes; `proposal_status` reports the outcome |

`draft_ref` lets an agent submit a change set that `guidance_draft` produced natively instead of inlining file content, which keeps within the 16 KiB proposal body limit (**Decision D15:** raise the limit for `draft_guidance` or require `draft_ref` for bodies above it).

Registration: the Management › Agents connection flow and `airk setup` expose a *Guidance tools* choice; the default remains evidence + help + existing draft tools (INV-5). **Decision D14 (WP0):** the unified server currently presents a single 26-tool catalog. Confirm whether an opt-in group can be negotiated per connection or needs a configuration argument on the launcher. If neither is possible, ship the guidance tools behind a launcher flag rather than adding their schemas to every connection. Tool schemas go through the same official-SDK stdio transport and schema tests as the existing tools.

---

## 11. CLI

```
airk guidance inventory   --collection <c> [--kind instructions,skills,settings,records] [--json|--md]
airk guidance evaluate    --collection <c> [--rule GD-C01,…] [--severity warning] [--json|--md] [--since <generation>]
airk guidance draft agents|claude|pointer|adr|glossary|profile --collection <c> [--out <file>] [--apply]
airk guidance draft brief --finding <fingerprint> --collection <c> [--out <file>]   # §9 brief; never applied
airk guidance draft skill <name>|--shipped <name> --engine claude,codex --collection <c> [--out <file>] [--apply]
airk guidance draft settings --engine claude|codex|gemini [--rule GD-A03] [--hook session-start] --collection <c> [--out <file>] [--apply] [--confirm-sensitive <pointer>]…
airk guidance skills      --collection <c> [--json|--md]          # origin and drift per skill (read-only)
airk guidance apply <change-set.json> --collection <c>             # applies a reviewed change set; same guards as --apply
airk guidance explain GD-C02
airk guidance credits                      # prints §2.2 as text with links
```

- `--apply` writes only when `apply_preconditions.live_hash_must_equal` (or each file's `precondition`) matches the live file (INV-4); otherwise exits non-zero with both hashes. It refuses a user-edited managed file (INV-13) and prints the three-way diff instead; the user resolves it in an editor or in Explorer.
- `--apply` honours `[manage]` modes exactly as Explorer does: a class below `apply` is refused with the mode named. A sensitive edit is applied only if its JSON Pointer is listed in a `--confirm-sensitive` argument; there is no wildcard form (INV-14).
- `airk setup` keeps its current behaviour: it prints MCP configuration and writes nothing. v3's `airk setup --skills/--hooks` is replaced by the `guidance draft` commands above.
- Exit codes: 0 clean, 2 warnings, 3 defects, 4 precondition failed (hash mismatch or user-edited managed file), 5 evidence unavailable (index missing/stale), 6 refused by management mode or unconfirmed sensitive edit.
- `--since` diffs finding fingerprints between two generations for CI-style "did guidance regress" checks.

---

## 12. Explorer — Guidance workspace

A new `main` workspace `guidance` in `explorerRoutes.ts`, placed after Strategies and before the pinned Management workspace. It uses the same status pill (service / embed), the same evidence-class badges (plus *live-only*), the same honest counts and the same project selector binding. It runs in the VS Code extension host like the rest of the extension and calls the local `airk` service and nothing else.

### 12.1 Routes (tabs)

| Route | Tab | Purpose | Prototype |
|---|---|---|---|
| `guidance/overview` | Overview | Management modes per class, counts by severity and origin, publication/generation, stale-evidence banner | `images/guidance-overview.png` |
| `guidance/recommendations` | Recommendations | Findings and admitted proposals in one list, filterable by class, severity, origin (rule / agent / bridge), sensitivity and live check; actions *Preview*, *Dismiss (fingerprint)*, *Ask agent* (brief), *Re-evaluate* | `images/guidance-recommendations.png` |
| `guidance/preview` | Change preview | Multi-file change set: per-file diff or three-way view, evidence per hunk, per-item sensitive confirmations, *Apply selected* | `images/guidance-change-preview.png` |
| `guidance/instructions` | Instructions | The v3 *Artifacts* table: role, path, exists, identical-to-primary, tokens vs budget, indexed hash, live check | `images/guidance-instructions.png` |
| `guidance/skills` | Skills | Project skills by engine with origin, version, drift, mirror state and token size; *Install shipped*, *Generate*, *Update*, *Adopt*, *Open*, *Remove (managed + unchanged only)* | `images/guidance-skills.png` |
| `guidance/settings` | Agent settings | Shared and personal settings per engine: evidence class, key-path tree with sensitivity badges and redacted values, findings inline; MCP rows show *Managed in Management › Agents* | `images/guidance-agent-settings.png` |
| `guidance/records` | Records | ADR index and glossary with *New ADR* (status proposed) and *Regenerate index* | — |
| `guidance/profile` | Profile | Form over `guidance.toml` including `[manage]` modes; validation from the crate; *About / credits* link to `docs/guidance.md#credits` | — |

Each route is `prototype_only` in `explorerFeatures.ts` until the commit that ships it, and that commit adds its help topic, as the existing comment there requires. `suggest_view` navigation gains the `guidance/*` routes so agents can point the user at a recommendation without being able to apply it. The PNG files under `docs/explorer/images/` are static design prototypes for review, not screenshots of shipped behaviour.

### 12.2 Management by user choice

- The Overview shows each class's mode (INV-12) and a *Change…* button that opens Profile; a mode change is a reviewed profile edit. Raising `settings` to `apply` or `sensitive` to `suggest` shows a modal naming the keys that become editable.
- In `report` mode a class shows findings without Preview; in `suggest` it shows Preview and *Ask agent*; only in `apply` is *Apply selected* enabled.
- Recommendations are never pre-selected. Rule-origin, non-sensitive items in an `apply` class may be multi-selected; agent- and bridge-origin items are selected one at a time; sensitive items are always confirmed one at a time with their exact text (INV-14).
- Dismissals are per fingerprint and per workspace binding, stored in workspace state (not a file), and can be listed and restored.

### 12.3 Apply path

1. Re-run the live check for every cited and targeted file (INV-15). Stale items are disabled with *Re-evaluate*.
2. Validate every file's precondition before writing any file (live hash, last-applied hash, no dirty buffer, not a symlink/junction, inside the bound workspace folder, ≤ 1 MiB).
3. One modal confirmation lists every file and action; sensitive items have already been confirmed individually.
4. Write each file with the guarded write from `explorerWorkspaceIO.ts` (`savePlanInWorkspace`'s protections: ancestor checks, snapshot before and after confirmation, exclusive create or no-follow open, fsync, read-back hash), or with `workspace.applyEdit` plus a document-version check when the file is open, as `agentSetup.ts` does.
5. Update the ownership manifest (§6.9) in the same sequence.
6. Writes are per-file atomic and the set is ordered, not transactional. On failure, stop and report which files were written (with new hashes) and which were not.
7. Keep the pre-apply bytes in extension global storage (never in the workspace) for one *Undo*. Undo is available only while each file's live hash still equals the post-apply hash.
8. Request durable native indexing through the existing coalescing path, then re-evaluate and show which fingerprints closed and which did not.

**Rules for the tab (inherit Explorer invariants):** zero backend calls other than the local service; no model calls; every number labelled with its generation; nothing applied without a click; apply refusals show both hashes; agent and bridge text is rendered as untrusted content (no links followed, no commands run, no Markdown HTML); writes never leave the bound workspace folder.

Availability follows the route model above; the Marketplace description mentions Guidance only when at least Overview, Recommendations and Instructions are `implemented` (WP8).

---

## 13. Work packages and test gates

| WP | Deliverable | Gate |
|---|---|---|
| **WP0 Discovery + attribution** | Confirm 1.4 service API surface, kernel evidence services available (D3), Explorer Review rule-engine types (D1), Assistance MCP proposal contract (D4), `.airk/` tracking (D2), exact current MCP tool count/names. Confirm no crate in the proposed dependency set spawns processes or requires a runtime. Re-confirm every §0 observation against an indexed publication. Decide D11–D16. Confirm whether the service accepts bounded live bytes in a request body (needed by D11 and live-only evaluation). Write `docs/guidance/discrepancy-log.md`. **Write the attribution ADR (§2.4.1) with the examined peer-skills commit hash.** | Log reviewed by operator; D1–D4, D11–D16 and the attribution ADR recorded (proposed) |
| **WP1 Profile + inventory** | `airk-guidance` crate skeleton; profile schema/validation; inventory with Cargo + npm-manifest component adapters (parsing only); verification-command discovery from CONTRIBUTING/scripts/CI. | Unit tests; inventory snapshot on AIRK repo and three fixtures; determinism test |
| **WP2 Rules P/C** | Rule trait, registry, fingerprinting; GD-P\*, GD-C\*. Doc comments carry credits per §2.4.3. | Fixture-driven tests: each rule has a positive and a negative fixture; INV-2 enforced by a test that rejects citation-less findings |
| **WP3 Generators + skills** | Templates, merge-preserving generation, diff, apply-preconditions; the three shipped `SKILL.md` files and the `verify`/`release` project-skill generators; credit headers. **Snapshot review decides whether any peer-skills wording is reproduced (§2.4.2).** | Snapshot tests for every template and skill; round-trip test (draft → apply in temp dir → evaluate = clean); marker-less file test; reproduction decision recorded in the ADR |
| **WP4 CLI + MCP** | `airk guidance …` incl. `credits`, `skills`, `draft skill`, `apply`; `guidance_*` and `draft_guidance` behind the D14 opt-in mechanism; schema tests. `airk setup` unchanged. | MCP schema tests; stdio smoke test; a default connection shows no guidance schemas; **no-Node gate** on Windows/macOS/Linux runners with `node`, `npm`, `npx` removed from `PATH` |
| **WP5 Rules V/Q/R** | GD-V\*, GD-Q\* (token budget via calibration pipeline), GD-R\*; ADR/glossary templates. | Fixtures incl. drifted versions, oversized guidance, missing ADR sections; heuristic rules verified to never emit `defect` |
| **WP6 Explorer tab** | `guidance` workspace in the route manifest; Overview, Recommendations, Change preview, Instructions, Records, Profile; guarded apply path (§12.3) for instruction and record files; help topics per route; credits link. | Webview tests; Playwright flow: findings → preview → apply refused on hash mismatch → apply succeeds → reindex → finding closed; route availability test (`prototype_only` until shipped) |
| **WP6b Skills and agent settings management** | `manage/` module (registry, ownership manifest, change sets, three-way, structured edits per D11, sensitive classifier, redaction); GD-S\* and GD-A\* rules; live-only evidence class; Skills and Agent settings tabs; `[manage]` modes and the per-user lower-only override; `draft_guidance` admission (§6.7); Undo. Depends on WP3, WP4 and WP6. | Fixtures per §14 (user-edited managed skill, mirrored skill drift, tracked secret, stale permission, hook missing executable, invalid/duplicate-key settings, MCP-entry delegation); tests prove: no write in `off`/`report`/`suggest`; sensitive edits never batch-applied; secret values absent from every MCP/CLI/report output; MCP keys never edited; `delete` refused for user-authored files; Windows junction and symlink targets refused; CRLF and comments preserved byte-for-byte outside edited keys |
| **WP7 Agent handoff + hooks** | Brief generator; Suggestions/Planner routing; `guidance_explain`; agent → `draft_guidance` → Inbox → apply loop; optional SessionStart hook as a sensitive settings change (if D9 = include). | Brief snapshot tests; manual run with Codex and Claude Code: agent searches with AIRK tools, submits a skill and a settings recommendation, user applies, finding closes (recorded in qualification doc) |
| **WP8 Docs, release, qualification, notices** | `docs/guidance.md` with *Credits*, `docs/guidance-rules.md`, `docs/guidance-peer-recipe.md`, architecture update, README section incl. the no-Node statement, release notes with source credit, extension description, `scripts/check-repo.ps1` no-runtime check, `docs/scale-qualification.md` entry; **THIRD_PARTY_NOTICES.md / `third_party/licenses/` per the WP3 decision; attribution ADR moved to *accepted*;** version bump reminder per AGENTS.md. | `check-repo.ps1` clean; README/docs/VSIX agree on version; no-Node gate green on all platforms; route availability (`implemented` vs `prototype_only`) per route recorded; a reviewer can trace every §2.2 row to an in-repo credit (INV-11) |

Each WP ends with the standard increment report: what passed, failed, was not run, remains unknown.

---

## 14. Testing

- **Fixture workspaces** under `examples/workspaces/guidance/`: `empty` (no guidance), `clean` (profile + aligned files), `drifted-mirror`, `stale-paths`, `uncovered-crate`, `oversized`, `adr-incomplete`, `node-free-rust` (no `package.json` anywhere; exercises GD-C06), `skills-mixed` (shipped, generated, user-authored and third-party skills; a mirrored pair that diverged; a managed skill edited by the user), `settings-claude` (tracked `.claude/settings.json` with a stale permission, a hook pointing at a missing script, custom indentation and CRLF; an untracked `settings.local.json` overriding a shared key), `settings-codex` (`.codex/config.toml` with comments and an `[mcp_servers.airk]` table that must stay untouched), `settings-secret` (a fake token-shaped value in a tracked file; the test asserts the value never appears in any output). Tiny, original content only — no text from peer-skills fixtures or scenarios, and no real credentials.
- **Mode enforcement:** for each class and mode, a test asserts which actions are offered and that no write occurs below `apply` (INV-12).
- **Three-way:** applying an update to a user-edited managed file writes nothing and returns the three-way view; resolving it writes once and updates the manifest (INV-13).
- **Untrusted proposals:** `draft_guidance` bodies with paths outside the root, MCP keys, user-level paths, unknown citation hashes, or HTML/links in rationale are rejected or rendered inert (§6.7).
- **Determinism:** index each fixture twice, assert byte-equal inventory/findings/drafts (INV-3).
- **Citation enforcement:** a test wrapper fails any finding or generated section with empty `support` (INV-2).
- **Write-isolation:** a filesystem watcher in tests asserts `airk-guidance`, service and MCP paths perform zero writes to the fixture root (INV-1).
- **No-process-spawn:** clippy `disallowed-methods` for `std::process::Command` in `airk-guidance`.
- **No-Node gate:** CI job on all three platforms with Node removed from `PATH` runs the WP4 smoke (INV-10).
- **Credit presence:** a `check-repo.ps1` step asserts every file under `templates/` and `skills/` carries the §2.4.3 header and that `docs/guidance.md` has a *Credits* heading (INV-11).
- **Self-check:** AIRK's own repo is a fixture; CI runs `airk guidance evaluate --severity defect` against it and fails on defects (this also enforces AIRK's existing "AGENTS.md and CLAUDE.md byte-identical" rule mechanically via GD-C01).
- **Cross-platform:** paths with backslashes, CRLF guidance files, skill directories on Windows without junctions, case-folded path comparison on Windows and macOS (as `explorerSource.ts` `caseFold` does), and settings files with a BOM.
- **Routine CI stays lean** per AGENTS.md; full fixture matrix is opt-in.

---

## 15. Documentation and release

- `docs/guidance.md` (user guide: profile, management modes, rules, CLI, MCP, Explorer tab and each route, skills, agent settings management, sensitive keys and redaction, ownership manifest, Undo limits; *Credits* section per §2.4.4); Explorer help topics for each `guidance/*` route in the bundled catalog; `docs/guidance-rules.md` (catalog with limits and design credit per rule where applicable), `docs/guidance-peer-recipe.md` (§18.1; names Orca by stablyai and peer-skills by corrabora with links), ADRs for D1–D4, D8 and the attribution record, architecture diagram update, README "Guidance" section with the no-Node statement, `docs/agent-integration.md` skills section, extension content guide entry, release notes with upgrade note ("guidance tools and skills are opt-in; existing agent configs unchanged; no new runtime requirements") and a one-sentence source credit.
- THIRD_PARTY_NOTICES.md and `third_party/licenses/`: per §2.4.2.
- Marketplace description: mention Guidance only once its core routes are `implemented` (§12), and say that skill and settings management is opt-in and project-scoped.

---

## 16. Risks and mitigations

| Risk | Mitigation |
|---|---|
| A convenient Rust crate for YAML/JSON-schema or templating pulls in a runtime shim, or a contributor reaches for `npx` in a script that ships | INV-10; WP0 dependency audit; `check-repo.ps1` runtime-reference check; no-Node CI gate |
| A template or skill ends up paraphrasing peer-skills closely enough that credit without a license notice is inadequate | WP3 snapshot review with the explicit reproduction decision; default to re-expression; MIT notice added when in doubt (§2.4.2) |
| Heuristic rules (GD-Q\*, GD-C06, GD-R01) produce noise and erode trust | advisory-only, off by default where noisy, per-rule limits text, dismiss-by-fingerprint |
| Generated AGENTS.md overwrites hand-written nuance | merge-preserving sections with markers; marker-less files get append-only + finding |
| Guidance tool schemas inflate every agent's context | opt-in tool group (INV-5) |
| Drafts applied against a file that changed since indexing | hash precondition (INV-4) on every apply path |
| Engine instruction-file or skill-directory conventions change | engine list and skill paths are data in the profile + template dir; adding an engine is a template + one profile default |
| Token counts mislead when tokenizer ≠ agent's actual tokenizer | report tokenizer name beside every count; use calibration pipeline's framing warnings |
| Scope creep toward "AI writes your docs" | INV-1 and non-goals; judgment-requiring actions always go through the agent brief, never through AIRK |
| Users later want peer-skills' orchestrated review and expect AIRK to bundle it | §18: documented recipe first; any shipped bridge is a separate, runtime-detected optional package |
| A settings recommendation grants a dangerous permission or installs a command-running hook | Sensitive class (INV-14): off by default, exact text, per-item confirmation, no batch, no wildcard broadening, CLI requires a named `--confirm-sensitive` pointer |
| Prompt injection: indexed text (or an agent/bridge that read it) steers a recommendation toward a malicious hook, permission or skill instruction | Agent/bridge origin labelled and never pre-selected; admission checks (§6.7); rationale rendered inert; sensitive gating; rule-origin corroboration shown when present; skills are text the user reviews before apply |
| Secrets in settings files leak through MCP, reports or logs | Values redacted at the `manage/` boundary; outputs carry key paths only; GD-A05 never echoes the value; test fixture asserts absence |
| Guidance overwrites a user's skill or settings edits | Ownership manifest + three-way (INV-13); `delete` only for unchanged managed files; Undo from pre-apply bytes |
| Two owners for MCP configuration (Guidance and Management › Agents) drift apart | `mcp = "delegate"`; Guidance never edits MCP keys; GD-A01 opens Management › Agents |
| A teammate's tracked profile silently raises another user's management mode | `apply` requires a per-user confirmation recorded per binding; the per-user override can only lower modes (§6.1) |
| Engine settings schemas change (new hook or permission syntax) | Sensitive-key classifier and registry are data; unknown keys in a settings file are left untouched and reported as *not evaluated*, never guessed |

---

## 17. Open decisions for the operator

- **D1** Reuse Explorer Review rule engine vs new trait in `airk-guidance`.
- **D2** Tracked profile location: `.airk/guidance.toml` vs `airk.guidance.toml`.
- **D3** Which evidence queries exist in `airk-kernel` today.
- **D4** Carrier for agent briefs (Suggestions inbox vs Planner queue item).
- **D5** Ship WP1–WP4 as an earlier CLI/MCP-only increment, or hold everything for the release that carries the Explorer tab.
- **D6** GD-V04 (recommend AIRK MCP usage) — keep as opt-in advisory, or drop to avoid self-promotion in a quality tool.
- **D7** Whether `airk guidance evaluate` should be added to AIRK's own CI as a required gate immediately (WP8) or after one release of dogfooding.
- **D8** Whether an optional Node bridge (§18) is ever shipped by Ioka, or remains a documented recipe only. Recommendation: recipe only through 1.5; revisit on demand.
- **D9** Include the native-binary SessionStart hook (a sensitive settings change, §8.4) in the first Guidance release or defer.
- **D10** Whether to contact the peer-skills author to note the derived work, beyond the public credit in the repository. Not required by MIT; a courtesy the operator may choose.
- **D11** Structured settings rendering: a single Rust renderer in `manage/` fed bounded live bytes (recommended: one implementation, identical CLI/Explorer diffs) or reuse of the extension's `jsonc-parser` editors with a cross-implementation byte-equality test.
- **D12** User-level skills and settings (`~/.claude/…`, `~/.agents/…`, `~/.codex/…`): out of scope for v1 (recommended), read-only inventory, or managed behind a separate opt-in.
- **D13** Workspace naming: keep the title *Guidance* and relabel the Help catalog's existing `guidance` area, or choose another workspace title (for example *Agent Guidance*).
- **D14** How the unified `airk` server exposes an opt-in guidance tool group (per-connection negotiation vs launcher flag) without adding schemas to every connection (INV-5).
- **D15** `draft_guidance` body size: raise the 16 KiB Inbox limit for this tool, or require `draft_ref` to a natively generated change set above it.
- **D16** Ownership manifest tracked (team-shared) or ignored (per clone), and its location relative to D2.

---

## 18. Optional Node-dependent integrations — isolated and deferred

Everything above is Node-free. This section exists so that if a user wants the parts of peer-skills that genuinely need Node (Orca-supervised peer review, succession hooks, their `tools/*.mjs`), the boundary is already drawn.

### 18.1 Default: a recipe, not a product

The first and recommended form is documentation only. `docs/guidance-peer-recipe.md` describes how a user who has installed **Orca (stablyai/orca)** and the **`peer` plugin (corrabora/peer-skills-and-workflows)** themselves can point a peer reviewer at AIRK: the companion's brief says "use the `airk` evidence tools, cite path/hash/generation, and return recommendations through `draft_guidance`", and `airk guidance draft brief` produces a brief in the shape their `write-for-agents` skill expects. AIRK ships nothing extra, installs nothing, and the user owns the Node install. The recipe links to both projects' own installation docs rather than restating them.

### 18.2 If a shipped bridge is ever justified (D8)

- **Separate artifact.** A distinct package (`airk-peer-bridge`, published separately — not in the `airk` binary, not in the VSIX, not in `crates/`). Its own version, changelog, license notices and release workflow.
- **Runtime-detected, never bundled.** The core probes for `node --version` only when the user opens the bridge's settings page; the result is a capability flag `integrations.node.available` shown in Explorer Management under *Optional integrations* with the detected version and the bridge's minimum. AIRK never downloads, installs or prompts to install Node; it links to the bridge's own documentation.
- **Hidden when absent.** Bridge-dependent actions do not appear in Explorer or the CLI when the flag is false.
- **Data contract only.** The bridge consumes the same JSON the CLI and MCP emit (inventory, findings, drafts, change sets, briefs) and returns results only through `draft_guidance` (§18.3). If the bridge needs something the CLI cannot emit, the CLI gets the field first, Node-free.
- **Evidence stays native.** The bridge never reads the index directly; it calls the `airk` binary or MCP like any other client.
- **Opt-in at three levels:** install the bridge, enable it in the profile (`[integrations] node_bridge = true`), and enable each feature it adds.
- **Qualification stays separate.** The core's no-Node CI gate runs without the bridge; the bridge has its own CI. The core's README never lists Node, even as optional, in the installation section.

### 18.3 What the bridge would and would not do

Skill and settings management is a **core, Node-free** capability (§6.6–§6.9, §12, WP6b). It does not depend on the bridge, and the bridge gains no write path of its own. In management terms the bridge is one more *recommendation source*, equivalent to a coding agent.

**Would:**

- Translate AIRK findings into a peer-review brief and write the brief where the user's Orca/peer setup expects it. That location belongs to the bridge's own installation, not the workspace.
- Ask the peer reviewers to assess guidance artifacts (instructions, skills, settings) against AIRK evidence. The brief tells them to use the `airk` evidence tools (`search`, `read_file`, `document_facts`, `outline`, `changes`) and the `guidance_*` tools, and to cite publication, path, line and content hash.
- Read back the review report and submit each actionable item through `draft_guidance`, as a change set or a `draft_ref` to a `guidance_draft` result, with origin `bridge` and the run id. Items then pass the same admission checks (§6.7), live re-verification (INV-15), three-way handling (INV-13) and sensitive gating (INV-14) as agent proposals. The user applies them in Guidance › Recommendations or not at all.
- Submit review conclusions that cannot be expressed as a change set (preference disagreements, unresolved decisions) as findings classified `unresolved_decision` or `preference`, with no change set. Explorer shows them and offers *Ask agent* or *New ADR*.

**Would not:**

- Write, update, adopt or delete any instruction file, skill, settings file, hook or ownership-manifest entry. All such writes go through the core apply path after a user click.
- Raise a management mode, confirm a sensitive item or batch-select its own recommendations.
- Run Orca, launch agents, or install or update the `peer` plugin, Orca, or their skills and hooks into Claude or Codex settings. Guidance lists those as *third-party* and leaves them alone (§1 non-goals). They remain the user's installation, under the user's own trust decisions, exactly as peer-skills' own docs require.
- Vendor peer-skills' tools.
- Read the index or the live settings files directly. The bridge sees only what the CLI and MCP emit, with values redacted.

The recipe in §18.1 achieves the same loop without the bridge: the user's own peer setup calls `draft_guidance` over MCP. The shipped bridge (D8) would only automate the brief and report plumbing.

---

## 19. References

**External**

- corrabora, *peer-skills-and-workflows* (MIT). https://github.com/corrabora/peer-skills-and-workflows
  - README — https://github.com/corrabora/peer-skills-and-workflows/blob/main/README.md
  - LICENSE — https://github.com/corrabora/peer-skills-and-workflows/blob/main/LICENSE
  - `skills/peer-review/SKILL.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/skills/peer-review/SKILL.md
  - `skills/bind-agent-workflow/SKILL.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/skills/bind-agent-workflow/SKILL.md
  - `skills/write-for-agents/SKILL.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/skills/write-for-agents/SKILL.md
  - `skills/record-decision/SKILL.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/skills/record-decision/SKILL.md
  - `skills/maintain-glossary/SKILL.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/skills/maintain-glossary/SKILL.md
  - `skills/write-issue/SKILL.md`, `skills/write-pr/SKILL.md` — https://github.com/corrabora/peer-skills-and-workflows/tree/main/skills
  - `docs/writing-guidance.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/docs/writing-guidance.md
  - `docs/guidance/adr-format.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/docs/guidance/adr-format.md
  - `docs/guidance/glossary-format.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/docs/guidance/glossary-format.md
  - `docs/guidance/record-format-adoption.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/docs/guidance/record-format-adoption.md
  - `docs/installation.md` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/docs/installation.md
  - `workflow/ORCA.md`, `workflow/BINDING.md`, `workflow/REVIEW.md`, `workflow/ADJUDICATION.md`, `workflow/RECORDS.md` — https://github.com/corrabora/peer-skills-and-workflows/tree/main/workflow
  - `defaults.yaml` — https://github.com/corrabora/peer-skills-and-workflows/blob/main/defaults.yaml
  - `tools/` (not used; listed for the §3.3 comparison) — https://github.com/corrabora/peer-skills-and-workflows/tree/main/tools
- stablyai, *Orca* and its `orchestration` skill (not used; §18.1 only). https://github.com/stablyai/orca
- Michael Nygard, "Documenting Architecture Decisions", 2011. https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions
- Agent Skills `SKILL.md` convention as used by Claude Code (`~/.claude/skills`) and Codex (`~/.agents/skills`); see each engine's documentation.
- Model Context Protocol — https://modelcontextprotocol.io (already an AIRK dependency).

**Internal (iokaio/airk)**

- `AGENTS.md` — https://github.com/iokaio/airk/blob/main/AGENTS.md
- `README.md` — https://github.com/iokaio/airk
- `docs/usage-reporting.md`, `docs/architecture.md`, `docs/agent-integration.md`, `docs/scale-qualification.md`
- AIRK Explorer plan v1.4, its review, and the Explorer implementation spec (Ioka, October 2026).