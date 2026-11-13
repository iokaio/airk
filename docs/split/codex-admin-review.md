# AIRK Admin UX review and improvement plan

**13 November 2026 · Running service: 1.8.34 · Read-only UX survey**

The biggest improvement is to make each screen answer one practical question, with the next action visible immediately. The current interface exposes much of AIRK's administrative capability, but long stacked forms, expanded native JSON and internal identifiers make ordinary tasks feel like API work. Help adds a more serious problem: several live articles describe superseded setup and transport behavior.

**Recommended priorities:** correct Help's guidance; shorten the main pages by separating tasks; replace identifier entry with named selection; and keep state, limitations and safe next actions visible. Preserve the complete native data behind those readable views.

**Your scrolling constraint is a design requirement.** The proposed initial desktop screens fit within one 1440 × 1000 viewport, with advanced disclosures closed. The mobile Help proposal fits within 390 × 844. This is achieved by changing information hierarchy and separating tasks, not by shrinking text or pretending the complete data has disappeared.

## What was reviewed and delivered

- All **11 main navigation pages**: Overview, Index library, Activity, Agents, Storage, Recovery, Service, Logs, Usage, Search and Help.
- **40 Help topic screens** and **29 tool-reference screens**, opened individually in the running UI.
- Index and index-set details; setup form; source-change variants; folder picker; job detail; four client setup tabs; storage inventory and both cleanup reviews; loaded service reads; the actual log-read failure; usage/report export/settings; search catalog and a real bounded query; Help's no-match state and mobile layout; session entry/sign-out; and a browser-offline error state.
- **107 current PNG captures and 107 corresponding proposed PNG designs**, plus a machine-readable [capture manifest](images/admin/capture-manifest.json). The complete visual inventory at the end links every pair.

The proposed images are illustrative designs, visibly labelled **PROPOSED UX · NOT IMPLEMENTED**. They use sample or selectively retained observations to communicate layout; they are not service screenshots, runnable prototypes, complete specifications, or evidence that a new capability exists. Help article mockups show a compact reading overview; full procedures, schemas, advanced options and continuations must remain accessible in implementation.

No application code or other documents were changed. Existing unrelated working-tree edits were preserved. No index creation, source change, grant issuance/revocation, purge, recovery execution, startup setting, stop or restart was applied. Search selection affected only the audit browser session. Read-only inspection may renew that session and contribute normal usage observations.

### Main screen comparison index

| Screen | Current screenshot | Recommended design |
| --- | --- | --- |
| Overview | [Current](images/admin/current-01-overview.png) | [Proposed](images/admin/proposed-01-overview.png) |
| Index library | [Current](images/admin/current-02-index-library.png) | [Proposed](images/admin/proposed-02-index-library.png) |
| Activity | [Current](images/admin/current-03-activity.png) | [Proposed](images/admin/proposed-03-activity.png) |
| Agents | [Current](images/admin/current-04-agents.png) | [Proposed](images/admin/proposed-04-agents.png) |
| Storage | [Current](images/admin/current-05-storage.png) | [Proposed](images/admin/proposed-05-storage.png) |
| Recovery | [Current](images/admin/current-06-recovery.png) | [Proposed](images/admin/proposed-06-recovery.png) |
| Service | [Current](images/admin/current-07-service.png) | [Proposed](images/admin/proposed-07-service.png) |
| Logs | [Current](images/admin/current-08-logs.png) | [Proposed](images/admin/proposed-08-logs.png) |
| Usage | [Current](images/admin/current-09-usage.png) | [Proposed](images/admin/proposed-09-usage.png) |
| Search | [Current](images/admin/current-10-search.png) | [Proposed](images/admin/proposed-10-search.png) |
| Help | [Current](images/admin/current-11-help.png) | [Proposed](images/admin/proposed-11-help.png) |

## Evidence, boundaries and task ledger

| Item | Recorded evidence or boundary |
| --- | --- |
| Target | `https://127.0.0.1:62133/admin/#overview`; local Windows browser survey, light theme. |
| Installed identity | Health and native Help report **1.8.34**. The captured health result reports `build_receipt_missing_or_unverified`; this review does not resolve build provenance. |
| Source identity | Checkout HEAD observed as `13c8f371c905d7f4fdfc00d0cb3166d86ceb3a16`, with pre-existing uncommitted work. Working release material describes 1.8.35 and is not evidence of installed behavior. Source-based findings are labelled separately. |
| Repository navigation | AIRK MCP was not exposed in the available tools. `.codex/config.toml` contains agent configuration and `.mcp.json` has no server entries. Scoped local reads were used; configuration was not changed. |
| Browser access | The installed `airkctl admin url` supplied a normal one-time launch directly to an isolated browser context. Codes/tokens were not printed or saved to deliverables. Browser execution used the Windows service owner's account, as required by same-user admission. |
| Capture conditions | Desktop viewport 1440 × 1000; mobile Help 390 × 844. Current PNGs preserve full document/body height; Help's internally scrolling topic rail remains a viewport of that rail, while every topic/tool was separately opened. Capture timestamps and image identities are in the manifest. |
| TLS limit | The capture context allowed the local HTTPS certificate. The browser survey is not a TLS trust qualification; the separate service trust read reported the Windows observation. |
| Allowed observations | Navigation, reads, non-applied cleanup previews, one browser-only search selection/query, sign-out of the audit session, and offline simulation confined to the audit browser. The service itself was not stopped to create the error screen. |
| Not available/executed | No known repair plan or existing agent connection was selected for detailed inspection. No new plan/connection was created merely for screenshots. Mutation success/failure, actual expiry/restart, assistive-technology behavior, other themes/platforms and all viewport sizes were not exercised. |
| Initial checkpoint | The 11 main pages were captured first; the installed version and stale Help overview were established. The named remaining acceptance work was detail views, every Help entry, visual proposals and the report. |
| Capture-script failure | One source-mode selector timed out. Already completed captures and the successful search were retained. After inspecting the source selector, only remaining forms/mobile/sign-out were captured. This was a harness selector issue, not an application defect reproduction. |
| Actual observed refusal | `logs.recent` returned `diagnostic_logs_unavailable`, HTTP 409. Its cause was not investigated or repaired; the failed-read screen is preserved. |
| Actual search observation | A browser scope was granted and a lexical search for `Artifact Indexing`, limited to `README.md` with a 4,096-byte result budget, returned evidence. The response disclosed truncation and unavailable scoped freshness; saved-source verification reported a match. These are distinct observations, not proof of retrieval quality or full coverage. |
| Source review | Admin shell/pages/rendering, Help, Agents, Index library, folder picker, Activity, Usage, operations and session code; native Help topics; relevant README/CONTRIBUTING/architecture sections; Admin HTTP and contract ownership documents. Independent reviewers covered Help/Agents and the remaining workflows; the primary reviewer integrated the evidence and designs. |
| Validation scope | Image/link/manifest integrity, layout dimensions, manual visual inspection, whitespace and final changed-path scope. Native builds, formatting/Clippy, workspace tests, repository code gates and platform qualification were not run: this task changes review artifacts only and makes no production validation claim. |

This ledger is contained in the requested review. No existing task ledger was edited. Findings below distinguish observed UI behavior, source interpretation, and future acceptance criteria; there was no participant usability study.

## Reduce scrolling without losing information

### Layout rules

1. **One task per view.** Library lists, creation, index details, set details, grants and destructive reviews should not be appended to one another. Open a dedicated route or a focused panel, then return to the same list position.
2. **A useful first viewport.** Show page purpose, selected context, current state, the primary action and essential warnings together. On a 1366 × 768 laptop, those essentials must still fit without scrolling; full-page content is allowed to extend when a user deliberately opens details. The laptop target is a future implementation acceptance criterion, not a tested mockup viewport.
3. **Tabs for separate tasks, disclosures for supporting facts.** For example, index Overview / Sources / Settings / Advanced; Help Guides / Reference; Service Status / Certificates / Startup / Lifecycle. Do not create tabs solely to conceal required warnings or a failed outcome.
4. **Bound lists rather than the whole application.** Show a useful first page, count and Load more/older. Preserve the native revision-bound continuation. One scrolling folder list inside a stable dialog is acceptable; multiple competing long scroll panes are not.
5. **Readable summaries, complete evidence.** Put IDs, digests, raw JSON and advanced arguments behind clear controls. Preserve all native fields and full exports. Distinguish complete results from selected rows or currently loaded records.
6. **Help opens to the answer.** Show short task cards on the landing screen. Once a guide is selected, show its heading and next step immediately; collapse the catalog on small screens. Long references can have a compact contents list and explicit sections.
7. **Keep action and feedback together.** Validation should appear by its field; receipt state, uncertainty or failure should appear beside the action that produced it. A new result should not require hunting at the bottom of a tall page.
8. **Do not trade scrolling for unreadability.** Keep body text around 14–16 CSS pixels, comfortable line height and clear focus indicators. Reflow at zoom, retain full labels and allow justified scrolling rather than clipping information to meet a height target.

### Measured visual comparison

These are full capture/image heights, not measured task times or a claim that every opened disclosure fits on one screen. Proposed screenshots show the compact default view; current screenshots include the inline sections and expanded results the application actually displayed.

| View | Current image height | Proposed image height | Main change |
| --- | ---: | ---: | --- |
| Overview | 2,088 px | 1,000 px | Summary and named index table |
| Index library | 1,711 px | 1,000 px | Separate creation, sets and access |
| Agents | 2,415 px | 1,000 px | Choose one setup path |
| Index detail | 5,042 px | 1,000 px | Focused detail tabs |
| Index set detail | 3,094 px | 1,000 px | Membership view with contextual actions |
| Usage report | 2,296 px | 1,000 px | Loaded scope, summary and secondary details |
| Search results | 1,964 px | 1,000 px | Readable results and disclosed limits |
| Mobile Help | 3,333 px | 844 px | Quick answers and category picker |

Desktop Help's page height alone is misleading: its current long topic catalog scrolls inside its own rail. The proposed landing removes that catalog from the primary task area, while retaining a dedicated browse/reference path.

## Shared UX foundations

Use a consistent page title, one-sentence purpose, selected context and observed-at label. Group navigation into Workspace, Manage, Service and Support without changing capability boundaries. Keep ordinary navigation in browser history, preserve position and form context when appropriate, and make index/set/job/article details linkable with non-secret routes. Handle launch-code removal separately from normal Back/Forward behavior.

Use plain labels in routine paths: source folder, index storage, index set, search scope and connection access. Explain implementation identities in technical details. Keep **service running**, **index searchable**, **indexing active**, **source verification**, **freshness** and **agent connected** separate; no single success badge can establish all of them.

Adopt the stronger existing Activity/Usage pattern elsewhere: a readable native-result projection followed by complete details. Do not introduce reduced parallel responses. Any future read-contract change must audit canonical native handlers and CLI/MCP/Workbench inputs and complete results, including scope, provenance, readiness, coverage, continuation and retry information. Presentation-only work can reuse current result contracts; unsupported discovery capabilities require a separate contract decision.

Load cheap, bounded initial observations consistently; provide explicit refresh and observation age. Handle failed, empty, not-yet-read, partial, stale and unknown states differently. Announce copy success only after the clipboard operation succeeds. Give invalid fields a persistent reason and accessible association rather than a color-only outline or unexplained disabled button.

Preserve reviewed changes, exact target identities, native digest/revision checks, typed confirmation and receipt recovery. A clean UI must not automatically resubmit an uncertain mutation or hide the difference between accepted work and completed work.

## Review of the admin workflows

**Scrolling is a primary design constraint.** Put the page purpose, selected context, current state and main action in the first viewport. Use dedicated detail views, tabs and deliberate disclosure to reduce page length. Keep normal readable type and spacing; do not achieve compactness by shrinking content or replacing long pages with giant nested scroll areas. Paginate or explicitly load bounded tables, and preserve context and return position when moving between list and detail.

The following observations come from the captured live **1.8.34** interface unless marked as source review. Proposed screens are design concepts, not implemented behavior or verification that new controls exist. **P1** means a major barrier to a routine task or a material misunderstanding; **P2** means a useful clarity or efficiency improvement. Proposed changes must preserve complete native responses, selected scope, revision checks, failure details, and read-only source behavior.

### Overview — P1

**Reduce scrolling.** Keep service availability, the selected index and its most useful next action above the fold. Show only a short recent-work summary; use links to focused index/job views rather than embedding every worker and native record.

[Current screen](images/admin/current-01-overview.png) · [Proposed design](images/admin/proposed-01-overview.png)

**Observed.** Service health opens with a large JSON object. The useful version, uptime and acceptance state share the same visual weight as executable hashes, storage-format numbers and the Windows user identifier. The captured response includes `build_receipt_missing_or_unverified`; the screen does not explain its implication. Indexing progress shows the combination “Waiting to run Finished (finished),” a succeeded reconciliation under “Current job,” and waiting jobs beneath it. “Committed: 1 of 2,465 captured files” appears beside “Indexed files: 2,465 of 2,465.” Index listing and workers expose “After binding” and “Project bindings” inputs.

**Impact.** The landing page asks people to interpret service internals before answering the primary questions: Is AIRK available, can I search this index, and is anything waiting or failing? The captured progress labels combine a retained completed observation and subsequent queued work. The display can look contradictory even when each individual value has a legitimate meaning. The snapshot does not establish a worker defect.

**Recommended design.** Lead with service availability, observed time and version. Keep build provenance as its own accurately qualified status: for this observation, “Build provenance could not be verified,” with the native reason in details. Add a named index list with separate readiness, indexing activity and pause/watch information only where supported by the corresponding reads. Present “Last completed job” separately from “Next queued job”; label the denominator and phase of each progress measure. Do not render a single apparent completion percentage by combining differently scoped counts. Put workers, bindings, catalog clocks and hashes in advanced details. Offer direct navigation to Create index, Search and Agents.

**Acceptance.** A person can identify service availability and the selected index’s readiness without opening JSON; can explain completed work versus queued work; and can find the full original observation. Missing build evidence remains visible and is never converted into a verified badge. Pagination uses a normal Load more control while retaining its revision-bound cursor internally.

### Index library, setup and access — P1

**Reduce scrolling.** Keep the library as a bounded list with a visible Create index action. Move setup into its own focused flow, and sets/access into their own views; opening any item must not append a second full screen below the list.

[Current screen](images/admin/current-02-index-library.png) · [Proposed design](images/admin/proposed-02-index-library.png)

**Observed.** One page combines Indexes, Set up an index, Index sets, Create an index set and Grants. The captured `airk` index row shows its ID, three revision/import facts, policy state, two paths, collection and a long binding. The corresponding set also shows revisions and “default indexes: none.” The setup form exposes Primary root, Name, Collection, Data root and a multiline `ALIAS=PATH` entry. The page’s grant section asks for a 64-hex context key and set/index IDs. No grants existed in this snapshot; that does not by itself establish that no agents can connect.

**Impact.** First-time setup competes with several advanced administrative concepts. “Index,” “index set,” “collection,” “binding” and “grant” appear before their relationship is explained. A user who only wants to add a folder must distinguish storage decisions from search-source decisions and understand an alias syntax. The set’s default-index field is easy to mistake for a statement that it has no members.

**Recommended design.** Use three views within the library: **Indexes**, **Index sets**, and **Advanced access**. Make the default index table emphasize name, source folder, current activity/readiness and last observation. Put IDs and revisions behind Details. Explain index sets in one sentence: “An index set groups the indexes that a scope or connection can use.” Show membership separately from defaults.

Create an index through **Sources → Name and storage → Review**. The first step chooses a folder. Derive a suggested name, present the default storage location plainly, and keep the collection identifier under Advanced. Represent additional folders as rows with Browse and Remove controls; retain expert text entry where needed. Keep “Watch for changes” with a short explanation. The review should summarize exactly which source folders will be read, which index-storage folder will be created or reused, and whether watching is enabled. Preserve the complete service draft and explicit apply step. Source review confirms setup also creates a matching index set; explain this as the resulting access structure, without promising that every search representation is immediately ready.

Advanced access should explain the distinction between a connection and its access grant. Prefer named connection/context and index-set selection when authorized catalog reads support it. If the necessary context discovery is unavailable, retain a clearly documented advanced ID workflow; do not imply a new discovery capability already exists.

**Acceptance.** A user can prepare an index using only a folder choice and understandable defaults, then identify source versus storage before applying. No folder selection creates an index. Required technical controls remain available. Set membership and default selection are not conflated. Empty grants are not labelled “no agent access.” A stale review, expired review or unknown apply outcome leads to the appropriate re-review/receipt workflow rather than a fresh blind submission.

### Index detail, source changes, policy, split and retirement — P1

**Reduce scrolling.** A detail tab should expose one coherent task within the first viewport. Display a short summary and action at the top, with optional supporting evidence below; split and retirement need dedicated flows, not more sections appended to the index page.

[Current screen](images/admin/current-12-index-detail.png) · [Proposed design](images/admin/proposed-12-index-detail.png)

**Observed.** Opening an index inserts a very long detail panel into the existing library. It repeats the index facts and expanded JSON, followed by Sources, Index sets containing it, Make readable, Indexing progress, Rename, Change sources, Pending source change, Source policy, Split, Split status and recovery, and Retire. The original setup/set/grant sections remain below. Split asks for JSON selector arrays and set choices, four capacity ceilings in bytes, an operation ID, digests and a finalization revision. Pending source work has terse “Observe” and “Resolve” controls.

**Impact.** Common tasks such as finding sources or renaming an index compete with operations that need substantial preparation. The large page makes the consequence and context of each action difficult to retain. “Resolve” does not explain the action a user is about to request. The UI asks users to manually transfer identifiers between reads and forms.

**Recommended design.** Give each index a dedicated view with a breadcrumb and persistent title. Use **Overview**, **Sources**, **Settings**, and **Advanced**. Overview contains readable progress, membership and recent work. Sources uses a folder/exclusion table with contextual Add folder, Remove folder, Exclude path and Remove exclusion actions. Choose existing source roots and exclusion rules by descriptive names/paths; retain exact IDs in details. Explain pending changes with observed state and the specific next operation.

Keep split, policy recovery and retirement in Advanced. Split should have a staged selector, destination/storage review, affected-set access review, capacity budget review, and final confirmation. If a fully guided split cannot be implemented within existing contracts, label the current JSON workflow “Advanced split configuration” and link precise help instead of presenting it as ordinary setup. Keep all capacity, scope and digest checks. For retirement, describe removal from sets/grants and retained history; do not relabel it as deleting source files or assume physical cleanup semantics not established by the response.

**Acceptance.** Source changes are discoverable without scrolling past split forms. Users can state the target and consequences before applying. Pending state is distinguishable from completed state; an empty observation never becomes proof of completion. A direct link can reopen the same index. Native split/retirement previews and complete receipts remain inspectable, including unknown or refused outcomes.

### Index set detail — P1

**Reduce scrolling.** Keep set identity, member count, the first bounded member page and Add indexes visible together. Open add/rename/delete as focused panels; preserve membership-list position on return instead of rendering every form inline.

[Current screen](images/admin/current-13-set-detail.png) · [Proposed design](images/admin/proposed-13-set-detail.png)

**Observed.** Set detail is appended below the library’s setup and set list. It exposes member records, add-index controls, rename and delete. Source review shows adding a member requires choosing an index and then separately clicking “Read its sources” before the review becomes available. Removing membership and deleting the set retain review and typed confirmation. The deletion explanation correctly states that indexes and their data remain.

**Impact.** Membership is a basic organizing task but feels like manually constructing a catalog operation. The distinction between an index’s membership and the set’s default indexes is not obvious. A disabled review can leave users hunting for the prerequisite read.

**Recommended design.** Use a dedicated set view with a name, purpose description, member list and “Add indexes” action. When an index is selected, obtain its bounded source details and show the exact enabled-root scope to be added before review. Do not silently add a subset if the source result is incomplete. Present members by index name and source summary. Keep set deletion in a separate danger section with the existing preservation statement and required confirmation.

**Acceptance.** A user can explain which indexes belong to the set, what access their membership covers and what deleting the set preserves. Source truncation/incompleteness yields a clear supported continuation or CLI handoff. Adding/removing membership still uses the current service review and revision checks.

### Folder picker — P1 accessibility, P2 layout

**Reduce scrolling.** Keep a stable dialog header and selected-path/action footer visible. Use one bounded folder-list region and explicit continuation; avoid nesting another long scroll list inside a scrolling full-page form.

[Current dialog](images/admin/current-14-folder-picker.png) · [Proposed design](images/admin/proposed-14-folder-picker.png)

**Observed.** The captured dialog offers drives, Home and AIRK data as both start buttons and folder-list entries. It includes a typed Path, Go, selected-folder text, Cancel and Use this folder. A live keyboard observation confirmed that initial focus remains on the underlying **Browse…** button instead of moving into the dialog. Source review finds a declared input ref and intended focus call, but no attachment of that ref to the rendered Path input. Escape handling, a focus-loop handler and opener restoration are present in source; their complete keyboard behavior was not qualified by this review.

**Impact.** The initial focus mismatch makes the dialog’s opening unclear for keyboard and screen-reader users and can leave interaction outside the announced modal. Duplicate start affordances add clutter. A folder used as index storage needs to look different from a folder chosen as source content.

**Recommended design.** Move focus into the dialog on open and keep background content inert while it is active. Use a compact location rail, a breadcrumb/path field, and a folder list with clear marks for “Already indexed” and “Index storage.” Keep the selected full path and primary action visible in a footer. For storage selection, label a new subfolder as something setup will create; do not suggest that selecting it has already created it. Preserve link/junction restrictions, hidden-folder controls and incomplete-list/mark disclosures.

**Acceptance.** Opening places focus on the Path field or a meaningful dialog heading; Tab and Shift+Tab remain within the dialog, Escape closes it, and focus returns to the correct Browse button. Source and storage picker titles describe their distinct purposes. The chosen full path remains visible, and no file-system mutation occurs during selection.

### Activity and job details — P2

**Reduce scrolling.** Put index/event filters and the latest events in the first viewport, with explicit page size and Load older. Replace the appended job card and always-visible management forms with focused views that return to the same list position.

[Current activity](images/admin/current-03-activity.png) · [Proposed activity](images/admin/proposed-03-activity.png) · [Current job](images/admin/current-15-job-detail.png) · [Proposed job](images/admin/proposed-15-job-detail.png)

**Observed.** Activity already translates events into phrases such as “Vector build started” and “Reconciliation finished,” with local times, named indexes, status styling and clickable short job IDs. The snapshot shows 32 events and their sequence range, with Load older. Saved requests says none were waiting. A running vector job opens below the timeline with work, state, index, attempts, outcome, worker generation and a cancel confirmation. Pause/resume and manual job-ID lookup are additional sections.

**Impact.** This is one of the more readable screens, but a dense stream of background events can bury the event a user cares about. A job link labelled only with a short hash gives little indication of its destination. Opening job detail below a long timeline makes the response to the click easy to miss. Background maintenance frequency in the snapshot is an observation, not evidence that the scheduling is incorrect.

**Recommended design.** Keep the readable timeline and add filters for index and event/work category if supported. If filtering only the loaded page, say so. Label jobs by work and state, with the ID secondary. Open job detail in a clearly focused panel or dedicated route with a return link and concise current status. Explain Retry and Cancel using the actual eligibility/refusal result. Move routine pause/resume controls into index detail and link there from Activity. Retain Saved requests as the destination for uncertain outcomes, with an explanation of the supported next step. Place paste-an-ID lookup under Advanced.

**Acceptance.** A user can find the latest failed or running work, open its details and return to the same timeline position. Every filter states its scope; older retained-history gaps remain visible. Unknown requests are not treated as safe new retries. Source review notes job timestamps derive from loaded events rather than JobRecord, so missing history must remain labelled as unavailable.

### Storage inventory and cleanup — P1

**Reduce scrolling.** Fit the category overview and eligible cleanup actions in one compact table. Open one cleanup review at a time; do not stack inventory JSON, cache review and log review into one continuously growing page.

[Current initial screen](images/admin/current-05-storage.png) · [Proposed initial screen](images/admin/proposed-05-storage.png) · [Current inventory](images/admin/current-16-storage-loaded.png) · [Proposed inventory](images/admin/proposed-16-storage-loaded.png)

**Observed.** Initial Storage has a Load button and two cleanup sections. Loaded inventory is raw JSON containing program/data/state/cache/log locations, file counts, byte sizes, completeness, purge eligibility and explicit exclusions from coverage. The captured cache is empty. The data location is about 1.3 billion bytes, but the page requires the user to interpret the raw count. The inventory explicitly does not cover external index data roots, workspace/agent configuration, other exports/backups or content already sent to providers.

**Impact.** Users cannot quickly answer what consumes space or what can be removed. The coverage exclusions are valuable but buried in JSON. A misleading aggregate total would be worse than the current complete representation because this inventory is not a whole-machine accounting.

**Recommended design.** Load the bounded inventory automatically and show a table with category, purpose, readable size, full location and eligible action. Show “Known storage in these locations” with a coverage note rather than an unqualified total. Preserve incomplete/unknown size states. Link index-data management to Index library. Keep exact bytes, file list details and complete native inventory available.

**Acceptance.** Users can identify the largest covered category and distinguish protected index data from cleanup-eligible cache/logs. Covered locations and exclusions are visible without reading JSON. Unknown size is never rendered as zero. Inventory inspection does not delete or modify anything.

[Current cache review](images/admin/current-17-cache-review.png) · [Proposed cache review](images/admin/proposed-17-cache-review.png) · [Current closed-log review](images/admin/current-18-logs-cleanup-review.png) · [Proposed closed-log review](images/admin/proposed-18-logs-cleanup-review.png)

**Observed.** Both captured cleanup plans contain an empty `files` array and `truncated: false`; each nevertheless shows the path-typing confirmation and “Purge exactly this plan.” The inventory’s six log files do not imply six eligible closed logs, and this review did not investigate why the plan contains none.

**Impact.** A full destructive confirmation for an empty plan makes it hard to tell whether there is any useful work to do. The user must compare inventory and plan internals to understand eligibility.

**Recommended design.** For this state show “Nothing to clean up in this plan,” with the location and Refresh plan action. For a nonempty supported plan show exact planned file count/bytes when available, the distinction between cache and closed logs, and a clearly separated final confirmation. Keep the current typed full-path guard and exact-plan digest binding for actual deletion. Never manufacture a cleanup estimate from all inventory bytes.

**Acceptance.** Empty plans require no meaningless confirmation. Nonempty plans state exactly what will be affected and what stays. Truncation is disclosed and handled by the service’s supported review rules. A changed/stale plan cannot be silently replaced at apply time. Cancel leaves everything untouched.

### Recovery — P1

**Reduce scrolling.** Place the explanation, plan lookup and main next step together. Once loaded, show plan status and impact first; paginate items and open selected item detail separately rather than appending all raw plan data.

[Current screen](images/admin/current-06-recovery.png) · [Proposed design](images/admin/proposed-06-recovery.png)

**Observed.** The entire visible screen is a “Repair plan” panel with Plan id, Load plan, Items after binding, Limit and Load items. It gives no explanation of where a plan ID comes from or which situation calls for this screen. No real plan was loaded in this survey. Source review confirms the implemented page is a plan/items reader; it does not provide a plan discovery list or execution workflow.

**Impact.** The navigation label suggests a recovery center, but the opening screen is an expert lookup with no starting point. A first-time user may assume an empty screen means there are no problems, although no such check has occurred.

**Recommended design.** Within current capability, title it “Inspect a repair plan” and explain how a plan reaches this screen. Link from the actual supported plan-producing workflow and allow an explicit pasted-ID fallback. Display readable purpose, target, state and items when returned; use ordinary continuation controls. Keep a message that the screen has not checked for problems. A future recovery queue or repair button requires additional supported behavior and separate validation; do not depict it as currently available.

**Acceptance.** A user can tell whether they have a plan, how to obtain the correct ID, and whether this page only inspects or also acts. An unopened plan is not labelled healthy. Invalid/missing/stale plan states have clear next steps and preserve the underlying error. No execution is suggested when this version cannot perform it.

### Service and resources — P1

**Reduce scrolling.** Keep a concise status summary and applicable actions in the first viewport. Use Resources, Trust and Startup sections on demand, and a focused lifecycle confirmation, instead of stacking all JSON observations and command variants.

[Current initial screen](images/admin/current-07-service.png) · [Proposed initial screen](images/admin/proposed-07-service.png) · [Current loaded screen](images/admin/current-19-service-loaded.png) · [Proposed loaded screen](images/admin/proposed-19-service-loaded.png)

**Observed.** Resources, HTTPS issuer status and startup status are separate manual reads. The loaded screen exposes memory/CPU/thread/queue observations as JSON. Certificate trust is reported as trusted on Windows, rotation is absent, and sign-in startup is registered. The same trust install/remove commands and Turn on/Turn off buttons remain presented. Restart and Stop each have a permanently visible text-confirmation form.

**Impact.** Current state is harder to find than possible actions. Technical capacity numbers are difficult to distinguish from measured usage. Showing both trust installation and removal as equally prominent next steps creates unnecessary choice when the reported trust is already present. Lifecycle actions need a clearer explanation of the interruption they cause.

**Recommended design.** Lead with status cards for service, resource pressure, certificate trust and Windows sign-in startup. Distinguish configured limits, reservations and observed consumption with units and freshness. Unknown telemetry stays unknown. Present the applicable startup action beside the observed state; put trust maintenance commands in contextual details and preserve the local-only boundary. Group restart/stop under Service controls and open a consequence review before the existing typed confirmation.

Source contract clarification: `crates/airk-https/src/admin_plane/CONTRACT.md` states that restart drains and exits the service, which cannot start itself; restart completion depends on a starting client/login entry and a fresh Admin session. Use “Request restart” and explain the actual start/reopen path. Do not promise automatic reconnect or guaranteed restart solely because the request is accepted.

**Acceptance.** A user can tell whether trust/startup is already enabled without reading JSON; understand resource limits versus actual consumption; and explain what happens after Stop or Request restart. No OS-trust mutation is moved into HTTP. The session-ending consequence is shown before confirmation. Lifecycle acceptance remains distinct from successful restart completion.

### Logs — P1 failure guidance, P2 reader

**Reduce scrolling.** Show the read status and primary recovery action immediately. For successful reads use a bounded paginated table and a focused selected-entry panel; keep the full-log handoff near the header rather than beneath a long result.

[Current initial screen](images/admin/current-08-logs.png) · [Proposed initial screen](images/admin/proposed-08-logs.png) · [Current failed read](images/admin/current-20-logs-loaded.png) · [Proposed failed-read design](images/admin/proposed-20-logs-loaded.png)

**Observed.** The initial screen has Limit, Load and a local complete-log command. The actual read failed with `diagnostic_logs_unavailable` and HTTP 409. It presents that code and a small native JSON object, followed by `airkctl service logs --all`. This survey did not obtain successful recent log rows or establish the cause of that failure.

**Impact.** The user sees an implementation code with little explanation of whether retrying helps, whether other logs exist or what to inspect next. The failure must not be described as an empty log or as proof that log recording is disabled; Overview separately reported log availability, and the two observations need interpretation rather than speculative repair.

**Recommended design.** For the observed failure show “Recent diagnostic logs could not be read,” the timestamp, native code and an explicit local-log fallback. Include Retry only with accurate retry guidance. For successful reads, show bounded timestamp/severity/message rows, expansion for exact data, and a count/coverage label. If text/severity filters work only over loaded rows, state that scope. Exports must distinguish loaded entries from complete local logs.

**Acceptance.** Failure, empty result and partial result are visually distinct. The user can copy the exact diagnostic and find the supported local command. No root cause is invented. A successful reader retains full messages and complete response data, including bounds and continuation information.

### Usage, export and index settings — P2

**Reduce scrolling.** Keep the index/date selector, completeness note and headline measures together. Use Summary, Breakdown and Recording tabs; put export in the report toolbar and settings in index detail so neither adds another screenful below the report.

[Current initial screen](images/admin/current-09-usage.png) · [Proposed initial screen](images/admin/proposed-09-usage.png) · [Current report](images/admin/current-21-usage-report.png) · [Proposed report](images/admin/proposed-21-usage-report.png)

**Observed.** Usage has a named index and period selector, a clear privacy explanation, readable recording policy, report tiles, client/operation breakdowns, daily history, estimates and recording health. The captured report shows 588 recorded calls, one failure and two clients. It explicitly calls totals a lower bound and lifetime totals partial. Recording is on for daily aggregates. “Today” is a period choice, while the report explains that days are whole UTC days and displays boundaries in local time.

**Impact.** This screen is substantially more understandable than raw-result screens and provides a useful design precedent. Its long sequence of recording controls, report, estimates, exports and unrelated settings still obscures the main question. “Today” can be interpreted as the user’s local calendar day. The estimates section must not suggest measured provider cost savings when no eligible baseline exists.

**Recommended design.** Keep the named index/period controls and make the loaded report’s scope and exact date window persistent. Put recorded calls, failures and client usage first; make recording configuration and estimate methodology secondary sections. Label the day convention explicitly. Keep unknown recording loss, missing baselines, excluded observations and partial lifetime coverage visible near the figures they qualify. Use the existing readable tables as the foundation rather than replacing them with decorative charts.

**Acceptance.** Users can identify the report’s actual index, time window and completeness even after changing the form selection. “Today” has an unambiguous timezone. Zero comparisons is not presented as zero savings or measured savings. All raw policy/report data remains available.

[Current export](images/admin/current-22-usage-export.png) · [Proposed export](images/admin/proposed-22-usage-export.png)

**Observed.** “Read Markdown” adds document text and a JSON wrapper to the page. The captured Markdown contains the report, qualifications and detailed resource measures. It is useful content, but Export implies a file or directly copyable report rather than another result object.

**Recommended design.** Offer clearly labelled Copy Markdown and Download Markdown/CSV actions for the report text once that behavior is implemented. Bind export to the displayed scope/window or explicitly state that it will read the newly selected scope. Keep Complete result JSON separately. Show copy success/failure feedback. Label any export as recorded/partial when applicable.

**Acceptance.** An exported text document is usable without extracting a JSON string. Its index, window and qualifications agree with the displayed report. Changing the chooser cannot silently export a different scope than the user expects.

[Current settings document](images/admin/current-23-index-settings.png) · [Proposed settings design](images/admin/proposed-23-index-settings.png)

**Observed.** “Index settings documents” lives under Usage. Its dropdown includes roots, extraction policy, vector selection/status, scan limits and quota policy/status. The captured roots document is JSON with primary root, configured/indexed objects, generation and pending-index state.

**Recommended design.** Move this read-only inspection into the chosen index’s Settings view. Display a purpose-specific description and a structured summary for each supported document, keeping the complete document and unknown fields available. Where configured and indexed state differ, explain that they are distinct observations; do not treat an empty object as proof that no source exists when the primary root is present separately.

**Acceptance.** Settings are discoverable from the index being managed. Each view identifies its index and observed revision/state, preserves configuration versus indexed distinctions and exposes the full native document. Inspection does not imply an edit was applied.

### Search — P1

**Reduce scrolling.** Keep the selected-source summary, query and Search action in the first viewport. Choose scope in a focused picker, open advanced arguments on demand and paginate results. Place complete-response access in the results toolbar rather than an expanded JSON block after every result.

[Current screen](images/admin/current-10-search.png) · [Proposed design](images/admin/proposed-10-search.png)

**Observed.** Search begins with List catalog indexes, followed by Namespace, Expected catalog revision, Index ids and Project ids. The second panel asks for a Tool, Query, Limit, full/terse mode and optional JSON arguments. It correctly explains that this browser selection does not change an editor window’s scope or an agent connection’s grant. A subsequent bounded lexical query returned README evidence after the browser scope was selected. The result was displayed as a large native JSON response, with truncation and unavailable scoped freshness disclosed. Saved-source verification separately reported a match. This single observation does not establish retrieval quality or complete coverage. [Current results](images/admin/current-27-search-results.png) · [Proposed results](images/admin/proposed-27-search-results.png).

**Impact.** A page called Search requires knowledge of catalog and API identities before the user can search. Choosing a valid relationship between index and project IDs becomes the user’s burden. Free-text tool names and arbitrary JSON make basic querying look like an integration task.

**Recommended design.** Make the first step a named, membership-aware source picker with index-set groups and a visible selection summary. Resolve namespace/revision from the canonical catalog response and retain all authority/revision checks. Keep the statement “This browser only” beside the selector. Offer a common search form with examples and a plain query. Keep the complete supported tool catalog, full argument forms and raw JSON under Advanced; simplification must not remove supported inputs. A future readable result view should show source location, cited range, indexed revision, readiness/coverage and any truncation or verification outcome, with complete response and clearly scoped export.

Do not infer readiness from membership, silently expand access, or bypass the requirement that the selected indexes belong to the selected catalog scope. Empty selection, stale catalog, unavailable scope, not-ready index and genuine no matches need different messages and actions.

**Acceptance.** A person can choose an existing named source and construct a basic search without copying identifiers or typing JSON. They can explain which sources are in this browser’s scope and distinguish it from agent/editor scopes. The advanced tool/input catalog and full response retain parity with the canonical handlers. Selected-evidence copying is explicitly distinguished from complete-result export. Successful retrieval and response-state coverage require a later implementation validation; this survey makes no claim that they passed.

## Help: make it the place users can finish a task

[Current Help screen](images/admin/current-11-help.png) · [Recommended Help landing screen](images/admin/proposed-11-help.png)

Key article proposals: [Getting started](images/admin/proposed-help-topic-quickstart.png) · [MCP connections](images/admin/proposed-help-topic-mcp.png) · [Connect an agent](images/admin/proposed-help-topic-agent-setup.png) · [Search tool reference](images/admin/proposed-help-tool-search.png) · [No matching help results](images/admin/proposed-24-help-empty-filter.png).

Help has the highest-priority content problem in this review: the installed service reports **AIRK 1.8.34**, but several articles teach older installation, process and connection models. A clearer layout would make those instructions easier to follow without making them correct. Correct the shared guidance first, then reorganize its presentation.

The live landing screen shows a long topic list beside a wide, dense overview. The first several topics include Workbench, About, Explorer, role descriptions, indexing choices and CLI work management; Getting started appears farther down. The actual first-use questions—“How do I index a folder?”, “How do I connect my agent?” and “Why is search empty?”—are not prominent. The tool catalog is farther down the independently scrolling topic pane. The short article leaves substantial unused space, while the list requires continued scrolling.

Retain the existing strengths: offline guidance, safe text rendering, tool parameter tables, an explicit native version, complete native results, and disclosure of truncated articles. The redesign should make these capabilities easier to find without discarding technical detail.

### Correct the guidance before polishing it

The following discrepancies were observed in the captured live Help topics, with matching source in `crates/airk-mcp/src/help/topics.rs`. They are content findings, not assumptions based only on an unbuilt working tree.

| Priority | Article and live evidence | Why this matters | Recommended correction |
| --- | --- | --- | --- |
| P0 | [What AIRK does](images/admin/current-help-topic-overview.png) says trusted VSIX activation automates registration, model preparation and indexing. | A user may wait for automatic setup instead of explicitly creating an index. | Explain the current create/review/index workflow; distinguish installing AIRK, creating an index, choosing scope and connecting an agent. |
| P0 | [Getting started](images/admin/current-help-topic-quickstart.png) repeats automatic setup and refers to older registrations and command paths. | The first-use article cannot be relied on as a current procedure. | Replace its main procedure with version-matched UI steps; move genuinely useful migration information into a dated migration guide. |
| P0 | [MCP connections](images/admin/current-help-topic-mcp.png) says MCP uses stdio. | The installed Agents screen explicitly describes Streamable HTTP over HTTPS, with no stdio or SSE. | Describe the installed endpoint, client setup and trust prerequisites using the current native contract. |
| P0 | [Connecting a coding agent](images/admin/current-help-topic-agent-setup.png) describes a stable-launcher `--root SOURCE mcp` configuration. | A user following Help can create the wrong configuration for the installed service. | Point to the current Agents workflow and parser-backed `airkctl agents` guidance. Keep credential enrollment outside the Admin HTTP plane. |
| P1 | [Overview](images/admin/current-help-topic-overview.png) and [Terminology](images/admin/current-help-topic-glossary.png) teach separate worker/helper process descriptions. | Internal architecture vocabulary obscures the user's task and can mislead diagnosis. | Lead with one local service and independent indexes. Put only version-verified process details in a technical reference. |
| P1 | [CLI scope](images/admin/current-help-topic-cli.png) describes older `airk` command and selector usage. | A readable command is still harmful when it is the wrong entry point or uses obsolete arguments. | Verify each runnable example against the matching installed parser catalog; label historical commands as historical. |
| P1 | [Tool discovery](images/admin/current-help-topic-tools.png) hard-codes catalog counts and older alias descriptions. | Narrative becomes stale when the actual catalog changes. | Explain discovery through the live catalog; avoid manually maintained counts. Keep compatibility aliases in a separate reference note. |
| P2 | [About](images/admin/current-help-topic-about.png) opens with AIRK 1.7.0 despite its AIRK 1.8.34 metadata. | Users cannot tell current guidance from release history. | Lead with product purpose, current observed version and licensing. Put past releases under a clearly dated history heading. |

Do not repair these disagreements only inside the browser. Help is a shared native capability: update its owning content and shared examples so CLI, MCP and Admin agree. The live-version label describes the service delivering the article; it does not prove that every paragraph was reviewed for that version. Add an explicit content applicability/review policy during implementation rather than using a version badge as a substitute for accuracy.

### Proposed information architecture

Make the Help landing page task-oriented, with reference material one step away:

| Group | User-facing guides |
| --- | --- |
| Get started | Create your first index; Choose what to search; Connect a coding agent; Run a first search |
| Indexes and scope | Index vs index set; Add another folder; Keep an index current; Understand progress; Change scope safely |
| Agents | Connect Claude Code, Codex or VS Code; Verify a connection; Resolve a certificate problem; Revoke access |
| Troubleshooting | No search results; Changes are missing; Indexing is waiting; An action has an unknown outcome; The service is unavailable |
| Reference | Tools; CLI commands; Supported formats; Status meanings; Terms; Privacy and limits |

This is a proposed organization, not a claim that the installed interface has those guides or a new search API.

The landing page should contain a clear **Help & guides** heading, a short introduction, three prominent task cards, and the five guide groups. Suggested introduction: “Set up AIRK, find indexed evidence, and resolve common problems.” The first task cards should be **Create an index**, **Connect an agent**, and **Find missing results**. An optional installation summary can show the observed service version and links to Overview or Agents. It must not infer index readiness from the existence of a service or from help text.

Keep **Tool reference** and **CLI reference** visible but secondary. A new user should not need to know `query_scope`, a topic identifier or a continuation-line argument to find instructions.

### Findability and reading behavior

The existing control accurately calls itself **Filter topics and tools**. Source inspection confirms that it matches topic IDs/titles and tool names/descriptions, not the body of an article (`packages/admin/src/help.tsx:489–497`). A future **Search help** control should search the actual bundled content and aliases such as “folder,” “missing files” and common error codes. Results should include title, category, a matching excerpt and applicable version. If implementation retains metadata-only matching, retain the narrower label and tell users what it searches.

A no-results state should say what was searched, preserve the query, and offer relevant categories. Do not display a blank rail beside an unrelated article as if that were a search answer. Clearing the filter should restore the catalog without changing the selected article.

Article selection should have a stable, non-secret route, such as `#help?topic=quickstart`, and participate in browser Back/Forward navigation. The current selected topic/tool lives only in component state, and top-level page navigation replaces history. This prevents ordinary bookmarking and returning to a particular answer. A shareable article route can still require a new authenticated session; it must never carry a launch code, cookie or token.

Keep the task overview and next action in the first viewport. Put long reference details behind deliberate navigation or disclosure; do not stack the full catalog before the article. Use a consistent article layout:

1. Breadcrumb, clear title and a one-sentence outcome.
2. Prerequisites, only when needed.
3. Numbered actions using the exact labels in the applicable version.
4. “What should I see?” with an observable result.
5. Troubleshooting for the likely failure points.
6. Related guides and a relevant page-navigation action.
7. Applicable version and complete response details beneath the main explanation.

A navigation action such as **Open Index library** should only navigate or prefill a form; it must not create, index, revoke or delete anything. Avoid prose that silently turns guidance into authorization.

### Sample replacement guide for the observed 1.8.34 UI

The following is proposed copy based on the captured form and the matching Admin documentation. Creating a new index was not executed during this read-only review, so the apply behavior remains source/documentation evidence rather than a completed live journey.

> **Create an index for a folder**
>
> An index is AIRK's searchable copy of information extracted from your source folders. Creating an index does not edit those source files.
>
> **Before you start:** choose the folder you want AIRK to index and a separate location for its index data.
>
> 1. Open **Index library** and find **Set up an index**.
> 2. Beside **Primary root (the folder to index)**, choose **Browse…** and select your source folder.
> 3. Enter a recognizable **Name**. Review **Collection** and **Data root**. The data root is AIRK's index-storage folder, not the source folder; setup can create the suggested location.
> 4. If needed, use **Add a folder…** for additional source folders. Choose whether to keep the index current after edits using **watch sources**.
> 5. Choose **Review setup**. Check every source folder, the data location and the watching choice in the returned review.
> 6. If the review is correct, choose **Create index**. An accepted action is not proof that all indexing work has finished.
> 7. Open the index's details or **Overview → Indexing progress** to inspect its job and published state. In this version, setup also creates an index set named after the index. That makes the index available for scope selection; it does not mean every agent has selected it.
>
> **What next?** Choose the index or its set for the search or connection you intend to use. Admin's Search scope belongs only to this browser session; it does not change an editor window's scope or an agent connection's grant.
>
> **If files are missing:** check the selected scope, source folders, indexing progress, reported failures and exclusions. A running service alone does not prove that your files are searchable or current.

The proposed landing and guide designs can use friendlier field labels, but a guide for the existing 1.8.34 interface must keep its actual labels. Do not mix later working-tree behavior, such as newer open agent-connection defaults, into an article presented as an installed 1.8.34 procedure.

### Tool-reference pages

The existing native parameter tables are valuable. Improve their hierarchy rather than replacing them with an incomplete summary. For each tool, show purpose, “Use this when…”, one parser/schema-valid example, required inputs, commonly used optional inputs, and how to interpret the response. Place advanced budgets, continuity and verification options in clearly labeled sections while keeping the entire schema accessible.

**P1 schema-label accuracy:** the live Search tool reference renders optional enum fields as “one of … or any.” Source review explains the mismatch: `typeText` in `help.tsx` falls back to `any` for a null-only branch, while the native Search arguments use `Option<SearchMode>` and `Option<EvidenceKind>` with explicit enum choices (`crates/airk-mcp/src/lib.rs`). Optional/null does not mean an arbitrary value is accepted. The proposed Search reference shows the actual mode choices and an Optional badge. A future renderer fix should preserve native schema meaning and be checked against shared schema cases; no application fix was made in this review.

For example, a Search reference should distinguish the query and chosen scope from optional result filters and budgets. Explain that lexical, vector and hybrid behavior have different prerequisites and that an empty result does not establish why nothing was found. Keep provenance, readiness, freshness, coverage and truncation details available. Examples should use fictional identifiers and paths; never turn local IDs captured for this review into reusable instructions.

### Help implementation acceptance criteria

- A first-time user can find the correct index-creation and agent-connection guide from Help without knowing a tool name.
- Current Help has no unqualified stdio, automatic-indexing or obsolete setup claim for the installed HTTPS/manual-setup product.
- Every runnable command example is checked against the matching native parser/catalog; guide labels match the applicable UI version.
- Topic/tool links survive reload and browser Back/Forward without retaining secrets in the URL.
- At narrow widths, the selected article appears before a long catalog. Replace the current full stacked topic list with a collapsible topic picker or dedicated browse view.
- Keyboard users can reach search, results, categories and article links in a logical order. Moving between articles updates focus to a meaningful heading; use a short status announcement instead of putting the entire article in an unsolicited live region.
- Use readable paragraph width, visible headings, generous line spacing and meaningful link labels. Do not rely on hover tooltips for a tool's description.
- The renderer and content format agree. The current safe renderer supports only a limited Markdown subset; internal links, tables or nested procedures need deliberate support before authors depend on them.
- A continuation says **Load next section** or **Continue reading** unless the next response is guaranteed to contain everything remaining. Preserve loaded text on failure; show loaded/total line information and any remaining continuation.
- Complete native results and supported input data remain accessible. A simplified reading view must not become a reduced parallel response contract.

## Agents: guide setup before exposing protocol details

[Current Agents screen](images/admin/current-04-agents.png) · [Recommended Agents screen](images/admin/proposed-04-agents.png)

The live page contains useful, concrete material: an endpoint, certificate trust, client-specific configuration, connection lookup, verification and revocation. Its ordering makes the user read implementation detail before choosing a task. Setup instructions are followed by long manual snippets, then a connection-ID lookup and a separate revoke section. Users must discover prerequisites in multiple places.

Lead with **Connect an agent**, followed by a client selector and the recommended supported editor/terminal path. Place existing connections next. Use a compact status summary for endpoint reachability and certificate trust; move transport names and CA fingerprints into details. Credential creation still occurs through the authorized local client, not a new browser enrollment operation.

The automatic CLI example requires a reviewed selection file, but does not explain how to produce one. That prerequisite needs a linked guide and version-matched command help. The manual form labels its connection ID “optional,” then creates copyable snippets with `<connection-id>` and certificate-path placeholders. Label these **Configuration template — complete the highlighted values**, or withhold the ready-to-use claim until the required values are present. Distinguish **Copy template** from **Copy configuration**.

A future connection picker should show a friendly name/client, scope summary and last verified observation. This is a proposed capability that requires checking the available native list contract; the current page asks for a 32-hex ID and directs users to `airkctl agents list`. Do not display an invented connection inventory in an implementation without that supporting API.

Treat setup checks independently:

| Check | What success establishes |
| --- | --- |
| Endpoint ready | The service reports its agent endpoint as ready. |
| Certificate trusted | The observed store/client configuration trusts the issuer under the reported conditions. |
| Credential probe verified | The endpoint accepted this connection's MCP initialization probe. |
| Agent connected | The specific external agent loaded its configuration and successfully used AIRK; a browser probe alone cannot establish this. |
| Requested scope available | The selected indexes are admitted and have the required evidence/readiness. |

The current page already states that verification does not prove an external agent loaded its configuration. Preserve that distinction in a clearer checklist rather than reducing all checks to a single green **Connected** badge.

Put **Revoke access** inside the selected connection's details, beneath its scope and effect summary. Retain typed confirmation and explain that revoking access does not remove client configuration entries. Keep token retrieval as an advanced local command; never display or copy the raw bearer token from Admin.

Source review also found two implementation-level UX issues to address later: copy buttons announce “Copied” without awaiting clipboard success (`agents.tsx:110–126`), and the client tabs have ARIA tab roles without an explicit arrow-key/roving-focus interaction (`agents.tsx:689–705`). Acceptance should require truthful copy feedback, a selectable fallback on failure, and a complete keyboard tab pattern. These are source-backed recommendations, not results of an assistive-technology audit.

## Session entry: explain how to return safely

[Current session-entry screen](images/admin/current-00-session-entry.png) · [Recommended session-entry screen](images/admin/proposed-00-session-entry.png)

Opening the supplied URL without a valid session produces a very sparse screen: the product name and a terminal command. The command is correct and the refusal to accept pasted credentials is a useful boundary, but the user receives little explanation of why the familiar page will not open.

Recommended entry copy:

> **Open AIRK Admin securely**
>
> AIRK Admin manages the service running on this computer.
>
> Run this command in a terminal on this computer:
>
> `airkctl admin open` — **Copy command**
>
> A fresh launch opens an authenticated session. Admin never asks you to paste a sign-in code or token here.

An extension opening action can be described only using the installed extension's verified label. A short expandable **Why a fresh link?** can explain that sessions end after inactivity or a service restart. The captured running page's eight-character service instance should move to technical details; an observed version and an explicit local-service state are more useful in the header.

Preserve the existing distinct states for signed out, expired, already-used/expired link, stopped and restarted. Suggested expiry copy is **Your admin session ended. Run `airkctl admin open` to continue.** Do not imply unsaved form input will survive. Do not add a token input or a button that fabricates a session outside the native launch flow.

The source's restart message suggests waiting for the service to come back, while the native contract says the process exits and a client must start it again. The future UI should distinguish **restart requested**, **service unavailable**, and **service reachable again** only when those states have actually been observed. Give the matching start/open commands instead of promising automatic recovery. Restart, expiry and destructive connection behavior were not executed during this review; those recommendations come from source and contract inspection.



## Prioritized improvement plan

Priority describes user impact, not an authorization to start implementation. No application changes are included in this review.

| Order | Scope and deliverable | Dependencies and boundary | Acceptance evidence |
| --- | --- | --- | --- |
| 1 — P0 | Correct current Help setup, transport, CLI and agent-connection guidance; clearly date historical material. | Shared native Help owner; verify against the installed parser/catalog and current product contracts. Do not create a competing browser-only source of truth. | Walk the corrected first-index and agent setup instructions against the applicable build; compare supported command examples and labels. |
| 2 — P1 | Shared title/context/status pattern, consistent readable responses, truthful copy/validation, Back/deep links and dialog focus. | Reuse current canonical data. Keep unknown/partial/failure distinctions and complete exports. | Keyboard and history checks; complete-result comparisons; field errors and clipboard failure; first-viewport inspection. |
| 3 — P1 | Short Index library, separate create/detail/set workflows, named Search scope and readable evidence results. | Membership-aware catalog selection, native review/apply and scope checks remain authoritative. Guided controls must keep advanced supported inputs. | Create/index/search task walkthroughs; stale review/scope tests; compare complete canonical/transport results where reshaping adds observations. |
| 4 — P1 | Help task landing, article routes, compact mobile layout and required-first tool reference. | Correct content from step 1. Full-text Help search, internal links and rich rendering need explicit capability work if not currently supported. | Find task by ordinary wording/error code; open article without catalog scrolling; test keyboard, continuations and content-version applicability. |
| 5 — P1/P2 | Compact Agents setup, empty cleanup plans, contextual service/lifecycle review and honest Recovery entry. | Enrollment/trust remain local; no invented agent inventory or recovery execution API. Keep destructive reviews. | Client prerequisites clear; empty plans require no deletion confirmation; user can explain restart and Recovery's read-only role. |
| 6 — P2 | Activity job views, bounded logs, Usage breakdown/export and settings placement. | Preserve loaded scope/time window, UTC boundaries, partial-history facts and exact native errors. | Return to list position; filter/export scope clear; failed logs differ from empty logs; report/export context matches. |

### Component boundaries for a later implementation

Presentation work belongs in `packages/admin/src/`: shell/history (`app.tsx`), shared response and validation (`render.tsx`, `app.css`), Help (`help.tsx`), Index library/folder picker, Agents, Activity, Usage and the pages module. Native Help content remains owned by `crates/airk-mcp/src/help/`. The Admin operation inventory and HTTP boundary remain owned by their current native contracts. A shared wire-contract change must be reviewed separately before consumers adopt it; the visual plan does not approve a new contract, crate or persisted format.

Do not make a broad refactor a prerequisite for the UX repair. Keep changes scoped, with the repository's mandatory investigation/reproduction/review sequence and one build/test owner. Capture complete output baselines before moving response rendering. Presentation changes must not silently remove advanced inputs, full result fields, continuation or error/retry data.

### Concrete usability and accessibility validation

Use realistic task walkthroughs with people unfamiliar with AIRK terminology; these are proposed future checks, not completed research:

1. From Overview, explain service status versus the selected index's readiness and find the next useful action without opening JSON.
2. Prepare an index from a source folder using default storage, identify what will be created and what stays unchanged, and cancel at the review.
3. Explain index, index set and search scope, then select sources by name and run a bounded search without typing identifiers.
4. Find why expected files might be missing using Help; distinguish scope, not-ready state, freshness, verification and response limits.
5. Choose a supported coding agent, identify prerequisites, and distinguish endpoint verification from proof that the client actually connected.
6. Inspect a job and an unknown request outcome without automatically replaying work.
7. Preview cleanup, recognize an empty plan, and explain what a nonempty plan would preserve before confirming anything.
8. Explain what Request restart does and how to open a new session afterward; recognize that Recovery only inspects a known plan in this version.
9. Export a usage report and explain its loaded index, UTC window, partial totals and estimate limitations.

Record task completion, first wrong turn, internal IDs copied, scroll screens before the first useful action, recovery from errors and understanding of scope. Set a baseline before claiming improvement; image-height reductions are not substitutes for those outcomes.

For implementation acceptance, verify 1440 × 1000 and 1366 × 768 desktop views, 390-pixel mobile width, 200% zoom, keyboard-only operation, focus visibility/restoration, meaningful control names and short live announcements. Essential task/action/context should be visible without scrolling at the relevant desktop viewport. Help mobile should reach an opened article immediately. Important warnings must remain adjacent to their action; full details may require intentional expansion and scrolling. Automated accessibility checks supplement, rather than replace, those walkthroughs.

Exercise loading, empty, nonempty, refused, stale, partial, unavailable, session-ended and unknown-outcome cases at the cheapest useful component seam. Preserve complete-response fixtures and actual transport comparisons where responses are reshaped. Run relevant T0/T1 checks before a frozen-input browser/artifact confirmation; avoid broad native campaigns merely to judge layout. This plan does not claim those implementation checks have passed.

## Complete screenshot and design inventory

`current-…` files are live UI captures. `proposed-…` files are corresponding concept images. Main pages and secondary forms may share a proposed destination because the recommendation intentionally separates or consolidates the current long-page layout. No screenshot is presented as an unexecuted mutation success or a nonexistent recovery/connection detail.

### Main pages, detail states and responsive views

| Screen/state | Current | Proposed |
| --- | --- | --- |
| 00 session entry | [Capture](images/admin/current-00-session-entry.png) | [Design](images/admin/proposed-00-session-entry.png) |
| Overview | [Capture](images/admin/current-01-overview.png) | [Design](images/admin/proposed-01-overview.png) |
| Index library | [Capture](images/admin/current-02-index-library.png) | [Design](images/admin/proposed-02-index-library.png) |
| Activity | [Capture](images/admin/current-03-activity.png) | [Design](images/admin/proposed-03-activity.png) |
| Agents | [Capture](images/admin/current-04-agents.png) | [Design](images/admin/proposed-04-agents.png) |
| Storage initial | [Capture](images/admin/current-05-storage.png) | [Design](images/admin/proposed-05-storage.png) |
| Recovery | [Capture](images/admin/current-06-recovery.png) | [Design](images/admin/proposed-06-recovery.png) |
| Service initial | [Capture](images/admin/current-07-service.png) | [Design](images/admin/proposed-07-service.png) |
| Logs initial | [Capture](images/admin/current-08-logs.png) | [Design](images/admin/proposed-08-logs.png) |
| Usage initial | [Capture](images/admin/current-09-usage.png) | [Design](images/admin/proposed-09-usage.png) |
| Search | [Capture](images/admin/current-10-search.png) | [Design](images/admin/proposed-10-search.png) |
| Help home | [Capture](images/admin/current-11-help.png) | [Design](images/admin/proposed-11-help.png) |
| Index detail | [Capture](images/admin/current-12-index-detail.png) | [Design](images/admin/proposed-12-index-detail.png) |
| Index set detail | [Capture](images/admin/current-13-set-detail.png) | [Design](images/admin/proposed-13-set-detail.png) |
| Choose a source folder | [Capture](images/admin/current-14-folder-picker.png) | [Design](images/admin/proposed-14-folder-picker.png) |
| Job detail | [Capture](images/admin/current-15-job-detail.png) | [Design](images/admin/proposed-15-job-detail.png) |
| Manual setup: Claude Code | [Capture](images/admin/current-agents-claude-code.png) | [Design](images/admin/proposed-agents-claude-code.png) |
| Manual setup: Codex | [Capture](images/admin/current-agents-codex.png) | [Design](images/admin/proposed-agents-codex.png) |
| Manual setup: VS Code / Copilot | [Capture](images/admin/current-agents-vs-code-copilot.png) | [Design](images/admin/proposed-agents-vs-code-copilot.png) |
| Manual setup: Other MCP clients | [Capture](images/admin/current-agents-other-mcp-clients.png) | [Design](images/admin/proposed-agents-other-mcp-clients.png) |
| Storage inventory | [Capture](images/admin/current-16-storage-loaded.png) | [Design](images/admin/proposed-16-storage-loaded.png) |
| Cache cleanup review | [Capture](images/admin/current-17-cache-review.png) | [Design](images/admin/proposed-17-cache-review.png) |
| Closed-log cleanup review | [Capture](images/admin/current-18-logs-cleanup-review.png) | [Design](images/admin/proposed-18-logs-cleanup-review.png) |
| Service status | [Capture](images/admin/current-19-service-loaded.png) | [Design](images/admin/proposed-19-service-loaded.png) |
| Recent service logs | [Capture](images/admin/current-20-logs-loaded.png) | [Design](images/admin/proposed-20-logs-loaded.png) |
| Usage report | [Capture](images/admin/current-21-usage-report.png) | [Design](images/admin/proposed-21-usage-report.png) |
| Usage report export | [Capture](images/admin/current-22-usage-export.png) | [Design](images/admin/proposed-22-usage-export.png) |
| Index settings document | [Capture](images/admin/current-23-index-settings.png) | [Design](images/admin/proposed-23-index-settings.png) |
| Help filter: no matches | [Capture](images/admin/current-24-help-empty-filter.png) | [Design](images/admin/proposed-24-help-empty-filter.png) |
| Service unavailable in browser | [Capture](images/admin/current-25-service-unreachable.png) | [Design](images/admin/proposed-25-service-unreachable.png) |
| Search: catalog lookup | [Capture](images/admin/current-26-search-catalog.png) | [Design](images/admin/proposed-26-search-catalog.png) |
| Search: bounded lexical query | [Capture](images/admin/current-27-search-results.png) | [Design](images/admin/proposed-27-search-results.png) |
| Create an index form | [Capture](images/admin/current-28-create-index.png) | [Design](images/admin/proposed-28-create-index.png) |
| Source change: remove root | [Capture](images/admin/current-sources-remove_root.png) | [Design](images/admin/proposed-sources-remove_root.png) |
| Source change: trim path | [Capture](images/admin/current-sources-trim_path.png) | [Design](images/admin/proposed-sources-trim_path.png) |
| Source change: unblock path | [Capture](images/admin/current-sources-unblock_path.png) | [Design](images/admin/proposed-sources-unblock_path.png) |
| Help at 390 px | [Capture](images/admin/current-29-help-mobile.png) | [Design](images/admin/proposed-29-help-mobile.png) |
| Signed out | [Capture](images/admin/current-30-signed-out.png) | [Design](images/admin/proposed-30-signed-out.png) |

### Help topic-by-topic review map

Every topic was opened. This table gives the recommended leading task/message for each, with a dedicated design. The P0 content corrections above take precedence over visual rearrangement. The brief design copy is a proposed overview, not a replacement for all native guidance or an assertion that every current paragraph is wrong.

| Current topic | Proposed title / emphasis | Current | Proposed |
| --- | --- | --- | --- |
| Overview | **What AIRK helps you do** — Find bounded, cited evidence in local code and documents. | [Capture](images/admin/current-help-topic-overview.png) | [Design](images/admin/proposed-help-topic-overview.png) |
| Workbench and bounded evidence plans | **Explore evidence in Workbench** — Use the editor's evidence workspace for a bounded investigation. | [Capture](images/admin/current-help-topic-workbench.png) | [Design](images/admin/proposed-help-topic-workbench.png) |
| About AIRK, Ioka and the community | **About AIRK** — A local Artifact Indexing and Retrieval Kernel. | [Capture](images/admin/current-help-topic-about.png) | [Design](images/admin/proposed-help-topic-about.png) |
| Explorer UI and agent assistance | **Use AIRK from your editor** — Find evidence and inspect indexing from the AIRK Explorer. | [Capture](images/admin/current-help-topic-explorer.png) | [Design](images/admin/proposed-help-topic-explorer.png) |
| Developer, Tester, Architect and Writer | **Choose a workflow for your task** — Start from your question, then select the right evidence. | [Capture](images/admin/current-help-topic-roles.png) | [Design](images/admin/proposed-help-topic-roles.png) |
| Indexing choices by purpose and source | **Choose indexing settings** — Balance useful evidence with extraction and storage cost. | [Capture](images/admin/current-help-topic-indexing-choices.png) | [Design](images/admin/proposed-help-topic-indexing-choices.png) |
| CLI options for managing project work | **Manage indexing work** — Choose an action from the observed index and job state. | [Capture](images/admin/current-help-topic-job-management.png) | [Design](images/admin/proposed-help-topic-job-management.png) |
| Explaining workspace index health | **Understand index health** — Separate availability, searchable data and current work. | [Capture](images/admin/current-help-topic-index-health.png) | [Design](images/admin/proposed-help-topic-index-health.png) |
| Help topic catalog | **Browse Help by task** — Choose a guide, a troubleshooting path or technical reference. | [Capture](images/admin/current-help-topic-topics.png) | [Design](images/admin/proposed-help-topic-topics.png) |
| Getting started | **Create your first searchable index** — Review sources, create the index and confirm readiness. | [Capture](images/admin/current-help-topic-quickstart.png) | [Design](images/admin/proposed-help-topic-quickstart.png) |
| CLI scope and invocation | **Run the right CLI command** — airkctl is the command-line client for the installed service. | [Capture](images/admin/current-help-topic-cli.png) | [Design](images/admin/proposed-help-topic-cli.png) |
| Public CLI command catalog | **Find a CLI command** — Use the installed catalog as the authority for command syntax. | [Capture](images/admin/current-help-topic-commands.png) | [Design](images/admin/proposed-help-topic-commands.png) |
| MCP connections and response modes | **Understand MCP connections** — Agents connect to the local service over HTTPS. | [Capture](images/admin/current-help-topic-mcp.png) | [Design](images/admin/proposed-help-topic-mcp.png) |
| MCP tool discovery and arguments | **Read a tool's inputs** — The live native catalog describes available operations. | [Capture](images/admin/current-help-topic-tools.png) | [Design](images/admin/proposed-help-topic-tools.png) |
| Choosing an evidence tool | **Choose an evidence tool** — Match the operation to the information you need. | [Capture](images/admin/current-help-topic-tool-selection.png) | [Design](images/admin/proposed-help-topic-tool-selection.png) |
| Lexical, vector and hybrid search | **Choose a search mode** — Use lexical, vector or hybrid search with clear expectations. | [Capture](images/admin/current-help-topic-search.png) | [Design](images/admin/proposed-help-topic-search.png) |
| Indexing jobs and phases | **Read indexing progress** — Understand what a job is doing and has published. | [Capture](images/admin/current-help-topic-indexing.png) | [Design](images/admin/proposed-help-topic-indexing.png) |
| Ongoing jobs, queues and waits | **Understand jobs and queues** — Find the current job and distinguish waiting from failure. | [Capture](images/admin/current-help-topic-jobs.png) | [Design](images/admin/proposed-help-topic-jobs.png) |
| Published state and source freshness | **Understand source freshness** — Indexed evidence and live source are related but distinct. | [Capture](images/admin/current-help-topic-freshness.png) | [Design](images/admin/proposed-help-topic-freshness.png) |
| Code facts, references and relationships | **Interpret code evidence** — Use declarations and relationships as bounded investigation aids. | [Capture](images/admin/current-help-topic-code.png) | [Design](images/admin/proposed-help-topic-code.png) |
| Document facts and original locations | **Interpret document evidence** — Keep the original page, section or cell location with the text. | [Capture](images/admin/current-help-topic-documents.png) | [Design](images/admin/proposed-help-topic-documents.png) |
| Supported formats and adapters | **Check supported formats** — Distinguish an available adapter from successful extraction. | [Capture](images/admin/current-help-topic-formats.png) | [Design](images/admin/proposed-help-topic-formats.png) |
| Offline models and inference | **Understand semantic readiness** — Local models and indexed vectors are separate requirements. | [Capture](images/admin/current-help-topic-models.png) | [Design](images/admin/proposed-help-topic-models.png) |
| Native service and process ownership | **Understand the local service** — One service manages the user's independent indexes. | [Capture](images/admin/current-help-topic-service.png) | [Design](images/admin/proposed-help-topic-service.png) |
| Memory, workers and resource policy | **Read resource status** — Use current observations to explain capacity and waiting work. | [Capture](images/admin/current-help-topic-resources.png) | [Design](images/admin/proposed-help-topic-resources.png) |
| Installation and terminal command | **Check your installation** — Identify the service, client and editor extension. | [Capture](images/admin/current-help-topic-installation.png) | [Design](images/admin/proposed-help-topic-installation.png) |
| Replacement, rollback and legacy transition | **Review an upgrade safely** — Separate payload replacement, data retention and reconnection. | [Capture](images/admin/current-help-topic-upgrades.png) | [Design](images/admin/proposed-help-topic-upgrades.png) |
| Project configuration and guard checks | **Review index settings** — Understand configured policy and its published effect. | [Capture](images/admin/current-help-topic-configuration.png) | [Design](images/admin/proposed-help-topic-configuration.png) |
| Index storage, isolation and quotas | **Understand AIRK storage** — Separate original sources, derived indexes and service files. | [Capture](images/admin/current-help-topic-storage.png) | [Design](images/admin/proposed-help-topic-storage.png) |
| Repair, rebuild and deletion | **Inspect repair and removal plans** — Understand the effect before taking a maintenance action. | [Capture](images/admin/current-help-topic-maintenance.png) | [Design](images/admin/proposed-help-topic-maintenance.png) |
| Connecting a coding agent | **Connect a coding agent** — Review access, configure the client and verify the connection. | [Capture](images/admin/current-help-topic-agent-setup.png) | [Design](images/admin/proposed-help-topic-agent-setup.png) |
| Unknown outcomes and exact retries | **Recover an unknown outcome** — Find the original request before trying the action again. | [Capture](images/admin/current-help-topic-request-recovery.png) | [Design](images/admin/proposed-help-topic-request-recovery.png) |
| Diagnosing common symptoms | **Diagnose missing results** — Work from the symptom to the smallest useful observation. | [Capture](images/admin/current-help-topic-troubleshooting.png) | [Design](images/admin/proposed-help-topic-troubleshooting.png) |
| Privacy and read-only boundaries | **Understand privacy boundaries** — Local indexing and agent-provider handling are separate concerns. | [Capture](images/admin/current-help-topic-privacy.png) | [Design](images/admin/proposed-help-topic-privacy.png) |
| Budgets and interpretation limits | **Read evidence within its limits** — Bounded results support investigation without proving completeness. | [Capture](images/admin/current-help-topic-limits.png) | [Design](images/admin/proposed-help-topic-limits.png) |
| Usage reports and savings estimates | **Interpret usage reports** — Read counts and estimates with their recording limits. | [Capture](images/admin/current-help-topic-usage.png) | [Design](images/admin/proposed-help-topic-usage.png) |
| Versions, builds and runtime diagnostics | **Check versions and diagnostics** — Match observations to the runtime that produced them. | [Capture](images/admin/current-help-topic-versions.png) | [Design](images/admin/proposed-help-topic-versions.png) |
| Components and data flow | **Understand the data flow** — Follow sources through indexing to cited evidence. | [Capture](images/admin/current-help-topic-architecture.png) | [Design](images/admin/proposed-help-topic-architecture.png) |
| Common user workflows | **Follow an evidence workflow** — Keep each investigation small enough to verify. | [Capture](images/admin/current-help-topic-workflows.png) | [Design](images/admin/proposed-help-topic-workflows.png) |
| Terminology | **Understand AIRK terminology** — Three distinctions make everyday tasks easier. | [Capture](images/admin/current-help-topic-glossary.png) | [Design](images/admin/proposed-help-topic-glossary.png) |

### Tool-reference-by-tool review map

Each tool has a current screenshot and a recommended reference layout. The images prioritize required inputs, then a small set of common options; they explicitly identify this as a subset and keep the full schema available. Implementation must derive the full input catalog from its native owner, not hand-maintain a reduced mirror. Schema-valid examples and result/error sections are separate reference tabs so a long parameter list does not dominate the first viewport.

| Tool | Specific recommendation | Current | Proposed |
| --- | --- | --- | --- |
| `changes` | Lead with the compared revisions and explain what an incomplete change window omits. | [Capture](images/admin/current-help-tool-changes.png) | [Design](images/admin/proposed-help-tool-changes.png) |
| `context` | Explain the bounded context task and its selected scope before advanced expansion controls. | [Capture](images/admin/current-help-tool-context.png) | [Design](images/admin/proposed-help-tool-context.png) |
| `file_info` | Lead with original path, indexed identity and extraction/coverage interpretation. | [Capture](images/admin/current-help-tool-file_info.png) | [Design](images/admin/proposed-help-tool-file_info.png) |
| `parse` | Show preparation versus execution clearly; a proposed plan is not completed evidence. | [Capture](images/admin/current-help-tool-parse.png) | [Design](images/admin/proposed-help-tool-parse.png) |
| `query_scope` | Explain authorized source discovery and the scope actually returned; do not imply global access. | [Capture](images/admin/current-help-tool-query_scope.png) | [Design](images/admin/proposed-help-tool-query_scope.png) |
| `read_file` | Lead with path/citation and range; separate indexed text from live saved-byte verification. | [Capture](images/admin/current-help-tool-read_file.png) | [Design](images/admin/proposed-help-tool-read_file.png) |
| `search` | Put required query and chosen scope first; group mode/filter options separately from budgets. | [Capture](images/admin/current-help-tool-search.png) | [Design](images/admin/proposed-help-tool-search.png) |
| `code_facts` | Explain supported extracted facts and keep unknown or unsupported semantics explicit. | [Capture](images/admin/current-help-tool-code_facts.png) | [Design](images/admin/proposed-help-tool-code_facts.png) |
| `imports` | Distinguish written import relationships from resolved runtime dependencies. | [Capture](images/admin/current-help-tool-imports.png) | [Design](images/admin/proposed-help-tool-imports.png) |
| `outline` | Show the structural outline task and location fields before optional response controls. | [Capture](images/admin/current-help-tool-outline.png) | [Design](images/admin/proposed-help-tool-outline.png) |
| `references` | Explain textual/candidate uses and unresolved targets; do not imply complete references. | [Capture](images/admin/current-help-tool-references.png) | [Design](images/admin/proposed-help-tool-references.png) |
| `relationships` | Explain edge kinds and evidence provenance; distinguish candidate from resolved relationships. | [Capture](images/admin/current-help-tool-relationships.png) | [Design](images/admin/proposed-help-tool-relationships.png) |
| `symbols` | Lead with named symbol lookup and disambiguation, then limits and source locations. | [Capture](images/admin/current-help-tool-symbols.png) | [Design](images/admin/proposed-help-tool-symbols.png) |
| `document_facts` | Use page/section/cell examples and retain extraction/OCR caveats. | [Capture](images/admin/current-help-tool-document_facts.png) | [Design](images/admin/proposed-help-tool-document_facts.png) |
| `capabilities` | Separate binary-supported capabilities from actual selected-index coverage/readiness. | [Capture](images/admin/current-help-tool-capabilities.png) | [Design](images/admin/proposed-help-tool-capabilities.png) |
| `diagnostics` | Explain sampled runtime observations, age and identities; keep it distinct from Help. | [Capture](images/admin/current-help-tool-diagnostics.png) | [Design](images/admin/proposed-help-tool-diagnostics.png) |
| `help` | Lead with topic/tool/command selector examples and bounded continuation behavior. | [Capture](images/admin/current-help-tool-help.png) | [Design](images/admin/proposed-help-tool-help.png) |
| `status` | Separate lightweight readiness from detailed runtime diagnosis and source freshness. | [Capture](images/admin/current-help-tool-status.png) | [Design](images/admin/proposed-help-tool-status.png) |
| `projects` | Clarify the scope this catalog represents; use friendly names while retaining identifiers. | [Capture](images/admin/current-help-tool-projects.png) | [Design](images/admin/proposed-help-tool-projects.png) |
| `draft_search` | Identify the draft lifecycle and explain that drafting does not perform the search. | [Capture](images/admin/current-help-tool-draft_search.png) | [Design](images/admin/proposed-help-tool-draft_search.png) |
| `draft_strategy` | Lead with proposed investigation strategy and the required review/handoff boundary. | [Capture](images/admin/current-help-tool-draft_strategy.png) | [Design](images/admin/proposed-help-tool-draft_strategy.png) |
| `draft_plan` | Show plan intent and reviewability; keep proposed and executed steps distinct. | [Capture](images/admin/current-help-tool-draft_plan.png) | [Design](images/admin/proposed-help-tool-draft_plan.png) |
| `list_templates` | Make the template catalog browseable by task and show its applicability. | [Capture](images/admin/current-help-tool-list_templates.png) | [Design](images/admin/proposed-help-tool-list_templates.png) |
| `get_template` | Show selected template purpose, required inputs and version identity together. | [Capture](images/admin/current-help-tool-get_template.png) | [Design](images/admin/proposed-help-tool-get_template.png) |
| `validate_draft` | Explain what validation establishes and what still requires user review or execution. | [Capture](images/admin/current-help-tool-validate_draft.png) | [Design](images/admin/proposed-help-tool-validate_draft.png) |
| `proposal_status` | Show proposal identity and lifecycle state before raw status fields. | [Capture](images/admin/current-help-tool-proposal_status.png) | [Design](images/admin/proposed-help-tool-proposal_status.png) |
| `get_handoff` | Explain the handoff artifact, receiving context and provenance without promising execution. | [Capture](images/admin/current-help-tool-get_handoff.png) | [Design](images/admin/proposed-help-tool-get_handoff.png) |
| `suggest_view` | Explain a view suggestion as a suggestion, with explicit user navigation rather than forced switching. | [Capture](images/admin/current-help-tool-suggest_view.png) | [Design](images/admin/proposed-help-tool-suggest_view.png) |
| `list_files` | Lead with file discovery/filter scope; retain count, truncation and continuation interpretation. | [Capture](images/admin/current-help-tool-list_files.png) | [Design](images/admin/proposed-help-tool-list_files.png) |

## Review completion and remaining uncertainty

The requested outputs are the review and its image directory only. All 107 live captures have a matching proposed image. PNG structure/dimensions, manifest correspondence, relative image links and whitespace were checked. Independent final reviewers checked the combined report and selected images. The primary reviewer integrated their corrections and visually inspected representative main pages, Help home/article/tool/mobile views, error handling and the revised compact search layout. Final corrections removed unqualified progress percentages, made active search filters/freshness/verification visible, corrected the optional-enum design label and neutralized session-entry status. The compact-layout measurement reports no page-height overflow for any proposed desktop image at 1440 × 1000 or the mobile Help image at 390 × 844 with disclosures closed.

This is a UX/design review, not production acceptance. No native or frontend code tests, installed mutation journeys, browser accessibility qualification, cross-platform checks, participant studies or release qualification were performed. The known log refusal remains unresolved; no repair plan or existing agent connection detail was available to inspect. Session expiry, destructive confirmation execution and long-lived polling/continuation failures are source-reviewed future cases, not live reproductions. Current Help content discrepancies, the folder-dialog initial focus problem, the log refusal and the bounded search response are actual observations.

### Source anchors for implementation planning

- [Admin shell and routes](../../packages/admin/src/app.tsx), [shared rendering](../../packages/admin/src/render.tsx), [styles](../../packages/admin/src/app.css), [page forms](../../packages/admin/src/pages.tsx).
- [Index library](../../packages/admin/src/indexLibrary.tsx), [folder picker](../../packages/admin/src/folderPicker.tsx), [progress](../../packages/admin/src/progress.tsx), [Activity](../../packages/admin/src/activity.tsx).
- [Help presentation](../../packages/admin/src/help.tsx), [native Help topics](../../crates/airk-mcp/src/help/topics.rs), [Agents](../../packages/admin/src/agents.tsx), [Usage](../../packages/admin/src/usage.tsx).
- [Session handling](../../packages/admin/src/session.ts), [operation adapter](../../packages/admin/src/operations.ts), [Admin HTTP contract](../../crates/airk-https/src/admin_plane/CONTRACT.md), [shared Admin contract](../../crates/airk-endpoint/src/admin_contract/CONTRACT.md), [existing Admin guide](../admin-pages.md).

These links point into the working tree and may evolve independently of the retained installed screenshots. The capture manifest preserves image hashes and observation times so later implementation review can keep the original evidence distinct.
