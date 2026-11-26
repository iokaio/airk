# Retrieval strategy for Gather Context

**Windows results review · 26 November 2026 · Design for discussion**

An agent needs enough source evidence to make the next engineering decision. Finding a passage about a provider, for example, gets me to a file. To decide where a provider change belongs, I need to see who selects it, who persists the setting, what actually sends the request, and which test would distinguish a working change. This document uses **Gather Context** for that task. The reviewed public tool is still called `answer`. AIRK also has a narrower operation called `context` that reads around a known location. [Completion plan and proposed public names](completion-plan.md#agent-facing-tool-surface)

On Windows, Sol received all 113 required source spans in the first reply, appended to the ordinary delivery. It finished 0 of 10 Bookbuilder tasks and 2 of 10 Munarium tasks correctly. Of the 18 remaining briefs, 12 missed multiple essential facts, three missed one, and three followed misleading evidence. The experiment establishes that supplying the spans in that format was insufficient. It leaves open where an agent lost a fact, accepted the wrong source, or failed to carry a relationship into its brief. Ordinary discovery is a separate problem. With the entire instruction as the query, Search delivered passages covering 3 of 61 Bookbuilder essentials and 12 of 52 Munarium essentials in the recorded diagnostic. Both observations motivate the design. [Windows result](../simplify/one-process-ledger.md#26-november-2026--windows-cuda-complete-evidence-review-trigger) · [Bookbuilder Search](windows-index-research-bookbuilder.md) · [Munarium Search](windows-index-research-munarium.md)

1. [How I would investigate the 20 tasks with ordinary repository tools](#1-how-i-would-investigate-the-20-tasks)
2. [Which parts AIRK and ordinary file tools can support today](#2-what-can-support-each-step-today)
3. [How to close the gaps and tell whether it helped](#3-how-to-close-the-gaps)
4. [How one Gather Context request could work](#4-how-one-gather-context-request-could-work)

## 1. How I would investigate the 20 tasks

I treated each quoted instruction as a separate assignment. Bookbuilder is pinned at `238b22a722ceb78447d72e39585fd6a7d4d3dd0c` and Munarium at `9948345d1d148525f644bc4b8bc5e6da9cf73ca0`. The quotations match the 20 hash-checked development inputs. I used ordinary source inspection on those two exports for this section. The one external documentation lookup is identified under Bookbuilder C1. Neither the sealed tasks nor the private reference notes were inputs to these walks. [Development inventory](answer-evaluation/scenarios.md)

In each case **my reading** sets out the user’s decision and my initial uncertainty. Each numbered step gives the question I took to a tool, the reproducible command and the finding that led to the next step. The **handoff** says what I could now do with that evidence. These are executed source-gathering simulations with editorially grouped commands and paraphrased results. The work measured here ends at source inspection. Timing, product changes, builds and tests require separate observations.

**Jump to a walkthrough.**

| Task | Bookbuilder | Munarium |
| --- | --- | --- |
| C1 | [A provider that works from Settings through generation](#bookbuilder-c1) | [A ninth output budget without losing tenant settings](#munarium-c1) |
| C2 | [A valid Gemini key fails on first run](#bookbuilder-c2) | [Why an expired session can still take a turn](#munarium-c2) |
| C3 | [A key that lives for exactly one generation run](#bookbuilder-c3) | [A file-count change that preserves the byte ceiling](#munarium-c3) |
| W1 | [An author deciding whether to prepare an imported book](#bookbuilder-w1) | [What authors of clients need to know about retiring a runbook](#munarium-w1) |
| W2 | [How a stage gets its final prompt](#bookbuilder-w2) | [Which claim a later reader sees](#munarium-w2) |
| T1 | [What a DOCX chapter heading actually contains](#bookbuilder-t1) | [An offline test that would catch an accidental second send](#munarium-t1) |
| T2 | [Which pipeline checks to run](#bookbuilder-t2) | [Checks for a new REST route and SQL migration](#munarium-t2) |
| A1 | [One Generate request, out and back](#bookbuilder-a1) | [How a collection-filtered search gets its results](#munarium-a1) |
| A2 | [Assessing a macOS release](#bookbuilder-a2) | [Changing the default PDF extractor](#munarium-a2) |
| A3 | [What adding a grant-proposal product would change](#bookbuilder-a3) | [A session-discovery tool for an agent that lost its ID](#munarium-a3) |

**Replaying the reads.** Start in the root of the indicated unmodified export in PowerShell 7 with ripgrep installed. For this review both exports sit under `artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/fixture-win-minilm-cuda-l7-cs1-bundle-cuda-r2/sources/`, in `bookbuilder-v2-238b22a722ce` and `munarium-9948345d1d14`. The small functions below are the actual path search, content search and inclusive range reader used to replay the documented commands. Run the setup anew in each repository. `Select-Object -First` in a discovery step denotes a sample. I used named-file reads or a separate inventory when the claim required more.

~~~powershell
$paths = @(rg --files --hidden -g '!.git' | ForEach-Object { $_.Replace('\','/') })
function Find-Paths($pattern) { $paths | Where-Object { $_ -match $pattern } }
function Find-Lines($pattern) {
    $globs = foreach ($glob in $args) { '-g'; $glob }
    rg --hidden -g '!.git' -n -i @globs -- $pattern .
}
function Read-Lines($name) {
    $matchesByPath = @($paths | Where-Object { $_ -eq $name -or $_.EndsWith("/$name") })
    if ($matchesByPath.Count -ne 1) { throw "Supply a unique path for $name" }
    $lines = Get-Content -LiteralPath $matchesByPath[0]
    for ($pair = 0; $pair -lt $args.Count; $pair += 2) {
        for ($i = $args[$pair]; $i -le [Math]::Min($args[$pair+1],$lines.Count); $i++) {
            '{0}:{1} {2}' -f $matchesByPath[0], $i, $lines[$i-1]
        }
    }
}
function Find-Files($pattern) {
    $globs = foreach ($glob in $args) { '-g'; $glob }
    rg --hidden -g '!.git' -l -i @globs -- $pattern .
}
~~~

### Bookbuilder

<a id="bookbuilder-c1"></a>

#### Bookbuilder C1 · A provider that works from Settings through generation

> Add Mistral as another AI provider next to OpenAI, Anthropic, Gemini, DeepSeek, OpenRouter and Ollama. Users should be able to enter and test a Mistral API key in Settings, have it stored the same way as the other keys, pick Mistral models, and run generation through it.

**My reading.** I read “add a provider” as four journeys a user must complete: save a key, test it, select a model and generate with it. I would use one existing hosted provider as a control. I would also check whether the local provider follows a different path before copying its behavior.

**1.** Find an existing hosted provider that crosses the named user journeys.

~~~powershell
Find-Files 'DeepSeek|OpenRouter' '*.vue' '*.ts' '*.cs' | Where-Object { $_ -match 'Settings|Provider|Model' }
~~~

**Found.** The matches include Settings components, client stores, provider implementations and the C# factory. I followed an existing hosted sibling to see whether its UI and transport share a single registry.

**2.** Find a comparable provider wired into both the interface and service.

~~~powershell
Find-Lines 'OpenRouter|DeepSeek' '*SettingsProvidersSection.vue' '*AIProviderType.cs' '*AIProviderFactory.cs' '*AiProviderExtensions.cs'
~~~

**Found.** The enum, Vue provider roster, factory and host registration all name siblings. A new enum value alone would leave the user unable to choose or dispatch the provider.

**3.** Follow a save and a test separately to find out which owns the key.

~~~powershell
Find-Lines 'ai\.saveProvider|ai\.testConnection|GetDefaultModel|Encrypt\(|MaskedApiKey' 'aiProvidersStore.ts' 'SettingsProvidersSection.vue' 'TestConnectionHandler.cs' 'LibraryService.cs'
~~~

**Found.** The store sends separate bridge actions. LibraryService encrypts the saved value and returns a masked value. TestConnectionHandler chooses a model and accepts an ephemeral override. The Settings path can use the saved configuration.

**4.** Find the source of model choices and the adapter that a selected model reaches.

~~~powershell
Read-Lines 'GetModelsHandler.cs' 117 164
Read-Lines 'AIProviderFactory.cs' 29 81
Find-Lines 'DeepSeek|OpenRouter' 'SettingsModelsSection.vue' '*ModelCapabilit*.cs'
Find-Lines 'mistralai/|Mistral' 'ModelCapability.cs' 'AIProviderType.cs' | Select-Object -First 18
~~~

**Found.** Model choices combine a built-in capability inventory and user settings. The UI also carries a provider mapping. The factory normalizes provider names and wraps the chosen transport. These are separate points to cover. The existing catalog also contains Mistral-branded model entries under another provider. Those entries are a false positive for a direct Mistral provider.

**5.** Check the actual network and test seams before deciding what can be reused.

~~~powershell
Read-Lines 'AiProviderExtensions.cs' 1 108
Find-Lines 'SendAsync|Authorization|BuildRequestBody|GenerateAsync' 'DeepSeekProvider.cs'
Find-Paths 'AIProviderFactoryTests|IntegrationTests|AIHandlerTests' | Select-Object -First 25
~~~

**Found.** Host registration supplies typed HTTP dependencies. The sibling provider owns request shaping and authentication. Factory and bridge tests can check routing, message construction and redaction with synthetic credentials. A sibling's wire format is only a lead for a new vendor.

**6. Verify the vendor contract outside the pinned application.** I used the web reader to open Mistral’s official Chat and Models references, then searched them for `Authorization: Bearer`, `/v1/chat/completions` and `/v1/models`. I expected to find the method, auth scheme, request fields and the scope of the model list. The references specify a bearer-authenticated chat POST with model and messages, and a GET returning models available to the caller. These are external, current documentation observations. No account or provider call was needed. [Chat API](https://docs.mistral.ai/api/endpoint/chat) · [Models API](https://docs.mistral.ai/api/endpoint/models)

**Handoff.** I have the UI lists, encrypted-store boundary, diagnostic default, model inventory, factory, transport and mockable vendor contract. A test can exercise save, masked readback, model selection and generation with a dummy credential. Which models an actual customer may use is account-specific.

<a id="bookbuilder-c2"></a>

#### Bookbuilder C2 · A valid Gemini key fails on first run

> During first-run onboarding, choosing Google Gemini, pasting a valid key and pressing Test connection fails with a message like "Specified argument was out of the range of valid values". The other providers test fine. Find the cause and fix it.

**My reading.** The symptom singles out onboarding, one provider and a local-looking exception. I would test two explanations against the source: an unsupported choice before any request, or a malformed request reaching Gemini. A valid key alone cannot distinguish them.

**1.** Use the button text and symptom before assuming the exception originated in the SDK.

~~~powershell
Find-Files 'test connection|testConnection|specified argument was out of the range' '*.vue' '*.ts' '*.cs' | Where-Object { $_ -match 'Onboard|Connect|Settings' }
~~~

**Found.** This found the first-run view and the shared test-connection handler. I traced the payload from the reported screen before looking at Gemini’s network code.

**2.** Find the button handler and the precise payload sent from first run.

~~~powershell
Find-Lines 'Gemini|ai\.testConnection|function testConnection' 'OnboardingView.vue'
Read-Lines 'OnboardingView.vue' 102 124
~~~

**Found.** The button sends the shared ai.testConnection action with a trimmed temporary key. That leads to the bridge diagnostic, even though the screen is called onboarding.

**3.** Trace this particular input to the earliest operation capable of throwing.

~~~powershell
Read-Lines 'TestConnectionHandler.cs' 35 104 120 131
~~~

**Found.** The nonempty temporary key forces GetDefaultModel before the protected generation call. Its provider switch omits Gemini and its default throws ArgumentOutOfRangeException. A saved Settings row can instead supply DefaultModelId. This explains the first-run distinction.

**4.** Check whether a missing registration or lack of HTTP support is a competing explanation.

~~~powershell
Read-Lines 'AIProviderFactory.cs' 30 49
Find-Lines 'Gemini|HttpClient' 'AiProviderExtensions.cs'
Read-Lines 'GeminiProvider.cs' 156 182
~~~

**Found.** The factory recognizes Gemini, the host registers its transport and the adapter already constructs an authenticated request. The failing branch precedes all three.

**5.** Find a repository-approved model candidate and a test that reaches the ephemeral branch.

~~~powershell
Find-Lines 'gemini-.*flash|TestConnection|Ephemeral' 'GeminiIntegrationTests.cs' '*ModelCapabilit*.cs' 'AIHandlerTests.cs' 'settingsStore.test.ts' | Select-Object -First 32
~~~

**Found.** The capability and integration sources contain Flash model examples. Existing Settings-side controls do not establish that Gemini reaches a fake provider with no saved row. That exact input is the discriminating regression.

**Handoff.** I can point to the omitted case, explain why good credentials never reach HTTP, and specify a test using a dummy key and captured model. A saved-key control and a working-provider control would catch regressions. This is a source diagnosis.

<a id="bookbuilder-c3"></a>

#### Bookbuilder C3 · A key that lives for exactly one generation run

> Every AI call re-reads the provider row from the library and decrypts the API key again, including each retry. Change this so a key is decrypted once per generation run and reused for that run's calls, and a key the user saves mid-run is picked up by the next run. Cover every place that reads a provider key, and keep the key-handling rules intact.

**My reading.** “Once” needs a lifetime and an identity. I would count decryptions per provider per run, including concurrent branches and retries. I would ask where a run begins and ends, which callers can run outside it, and what must happen to a key saved between two runs.

**1.** Identify where generation, secrets and retries meet, starting with the terms in the request.

~~~powershell
Find-Files 'decrypt|encrypted.?api.?key' '*.cs'
Find-Paths 'Retry|GenerationRequest|PipelineContext|Orchestrat' | Select-Object -First 60
~~~

**Found.** The first list includes generation adapters, embeddings, library operations and encryption services. The second identifies candidate owners for a lifetime that spans multiple calls. I needed to classify these before choosing where to cache.

**2.** Inventory every candidate reader before choosing a cache owner.

~~~powershell
Find-Lines 'Decrypt\(|EncryptedApiKey' '*.cs' | Select-String 'Provider|LibraryService|SettingsImport|ApiKeyCopy|EncryptionService' | Select-Object -First 120
~~~

**Found.** The inventory includes hosted text adapters, embeddings, settings/library operations and key-management code. I separated consumers that send generation traffic from deliberate admin reads. One provider class cannot own all of them.

**3.** Locate the lookup relative to an actual retry or secondary request.

~~~powershell
Read-Lines 'OpenAIProvider.cs' 173 190 567 584
Find-Lines 'ResolveApiKeyAsync|NativeToolTransport.SendAsync|requireEnabled' 'GeminiProvider.cs' 'AnthropicProvider.cs' 'OpenRouterProvider.cs' 'NativeToolTransport.cs'
~~~

**Found.** The outbound attempt can resolve a temporary override or open a fresh library scope and decrypt. Companion adapters and native-tool/accounting work have related paths. Retrying the attempt can repeat the lookup.

**4.** Find a scope that survives a stage boundary but ends when execution ownership returns.

~~~powershell
Find-Lines 'AcquireAsync|ExecuteOwnedAsync|StartRunAsync|ResumeCoreAsync|new PipelineContext|finally' 'PipelineOrchestrator*.cs'
Find-Lines 'Enter|Dispose|AsyncLocal' 'GenerationBudgetScope.cs'
~~~

**Found.** Execution and resume converge at an owning orchestrator path. Individual stages and fan-out create additional contexts. A disposable ambient budget scope is a local precedent for lifetime, while stage state is too short-lived.

**5.** Check what would leak or change semantics if every reader used the same cache.

~~~powershell
Read-Lines 'GenerationRequest.cs' 89 107
Find-Lines 'Decrypt\(|MaskDecryptedKey|Encrypt\(|ResolveApiKeyAsync|EphemeralApiKey' 'EmbeddingProviders.cs' 'LibraryService.cs' 'LibraryApiKeyCopyHandlers.cs' 'LibrarySettingsImportService.cs' 'AIRequestLoggerService.cs'
~~~

**Found.** The request carries diagnostic overrides. Embedding and administration have distinct usage and the library masks or encrypts keys. A cached plaintext value belongs only to an executing run. Existing protection for persistence, UI and logs still applies.

**Handoff.** I would propose one in-memory first lookup shared by concurrent calls for a provider in the owning execution. A fake key service can distinguish repeated reads, parallel reads, retries, in-run rotation, next-run rotation and independent simultaneous runs. Resume needs an explicit lifetime rule. I would preserve standalone and ephemeral lookups.

<a id="bookbuilder-w1"></a>

#### Bookbuilder W1 · An author deciding whether to prepare an imported book

> Our in-app help only mentions sequel preparation for imported Word manuscripts in passing. Write a dedicated help page for authors: what "Let the AI learn this book for sequels" does, where they can run it (during import or later), what it costs and produces, what they see when it is done, and how it leads to starting a sequel or prequel.

**My reading.** I would write from an author’s decisions: whether to enable a paid operation, whether it can be deferred, how to recognize completion and how to start the next book. I would start from the wording on the screen, since a developer’s service name might describe none of this.

**1.** Look up the author’s wording and near-synonyms in UI and help.

~~~powershell
Find-Files 'learn this book|prepare.{0,35}sequel|sequel.{0,25}prep' '*.vue' '*.ts' '*.cs' | Where-Object { $_ -match 'View|help|Import|Sequel' }
~~~

**Found.** The search turned up import and generation views, help articles, a preparation handler and service. I started at the two author-facing controls and followed each to its implementation.

**2.** Find both entry points and the current help destination.

~~~powershell
Find-Lines 'learn this book for sequels|prepareImportedNovelForSequel|imported.novel preparation' 'ImportView.vue' 'GenerationView.vue' 'import.ts' 'getting-started.ts'
~~~

**Found.** The import and Generate views expose the operation. Existing author help mentions it briefly. The two UI entry points lead to the same preparation operation.

**3.** Read what an author agrees to at import, including cost and prerequisites.

~~~powershell
Read-Lines 'ImportView.vue' 287 317 523 544
~~~

**Found.** The option is voluntary, uses the configured AI provider/model and has a variable cost related to book length. It can be deferred. The handler reports success or error after import. No fixed price appears here.

**4.** Check what the paid work consumes and leaves behind.

~~~powershell
Read-Lines 'ImportedNovelSequelPrepService.cs' 126 159 165 213
~~~

**Found.** The service checks eligibility and an already-prepared case, analyzes chapter material, and persists a book digest, style/canon information, promises, chapter planning material, and an audit, then builds and returns a handoff preview. These are concrete outputs an author can use.

**5.** Follow completion all the way to creating a linked book and locating the help article.

~~~powershell
Read-Lines 'GenerationView.vue' 2390 2452
Read-Lines 'SequelWizard.vue' 460 478 633 658
Find-Lines 'articles/import' 'index.ts' | Select-String 'help'
Read-Lines 'pro/src/WritingForgePro-ui/src/help/index.ts' 463 468
~~~

**Found.** Generate shows readiness, ending facts and a preview, and enables starting a linked book when prepared. The wizard distinguishes sequel and prequel and confirms the source and relationship. The help index registers the existing import article.

**Handoff.** I have source support for both starting points, cost disclosure, the resulting artifacts, visible success and next-book navigation. The page can describe what an author sees without promising a fixed charge or implying that generation is enabled on the imported manuscript.

<a id="bookbuilder-w2"></a>

#### Bookbuilder W2 · How a stage gets its final prompt

> Write a short explainer for a new contributor on how prompt templates work in WritingForgePro: where the built-in templates are defined, how the template used for a stage is chosen when there are project, genre and global versions, and how placeholders and prompt blocks are filled in before the request goes to the model. Point out anything in the existing architecture document that no longer matches the code.

**My reading.** The reader needs to predict which template wins and what text reaches the model. I would trace one stage with several matching templates, then repeat the reasoning with the override missing and with an unresolved placeholder. I would check the architecture document last as a claim about those behaviors.

**1.** Locate the sources a new contributor would find under the prompt’s vocabulary.

~~~powershell
Find-Paths 'template|prompt|architecture\.md$' | Where-Object { $_ -match 'BuiltIn|Hydrat|Resolv|/docs/architecture\.md' }
~~~

**Found.** The file inventory separates built-in stores, selection/resolution and hydration. The architecture document is a candidate for comparison after establishing the implementation order.

**2.** Find template definitions, resolution and hydration as separate concerns.

~~~powershell
Find-Paths 'BuiltInTemplateStore|ProjectAwarePromptResolver|PromptResolver\.cs|TemplateHydrator|BuiltInPromptBlocks'
Read-Lines 'BuiltInTemplateStore.cs' 1 58 97 112
~~~

**Found.** Compiled template groups register stage IDs and genre variants. Lookup can fall back through a parent genre to the stage default. That identifies the lowest fallback, but the caller can still override it.

**3.** Walk a project override, a selected pipe and an ordinary library lookup.

~~~powershell
Read-Lines 'ProjectAwarePromptResolver.cs' 27 101
Read-Lines 'PromptResolver.cs' 42 178
~~~

**Found.** The project-aware layer can replace project stage text and inherit defaults. Underlying resolution includes frozen-pipe selection, explicit IDs, customized pipe content, library genre/parent/global candidates, pipe defaults and compiled fallback. A missing explicit ID has its own error outcome.

**4.** Read the actual interpolation order and find a production caller.

~~~powershell
Read-Lines 'TemplateHydrator.cs' 54 129 150 266 280 301
Read-Lines 'AuthoringWorkshop.cs' 120 143
~~~

**Found.** The async path expands blocks from project, library and built-ins, including nesting under a depth guard, before conditional and data-token processing. The sync path does less. The caller checks unresolved values and places resolved system and user text on the generation request.

**5.** Compare the written explanation with the branches just inspected.

~~~powershell
Read-Lines 'pro/docs/architecture.md' 432 448 484 505
~~~

**Found.** The document’s simplified precedence misses parent/pipe cases. Its block/data order and its description of the recursion guard disagree with the implementation. I can cite each disputed claim alongside the resolver or hydrator.

**Handoff.** The explainer can give worked project, genre and global examples, identify the source of a block, and explain what an unresolved token does at the call site. Code/document disagreements can be corrected individually.

<a id="bookbuilder-t1"></a>

#### Bookbuilder T1 · What a DOCX chapter heading actually contains

> The Word (DOCX) export has tests for margins, running headers, title page and scene breaks, but nothing checks what it writes as chapter headings. Add tests covering the chapter heading for every heading style our built-in export profiles use, including how chapter numbers above ten and chapter titles with small words are written.

**My reading.** I would derive a fixture from the user-visible export profiles, turn off or bypass the table of contents, and inspect the chapter’s body paragraphs. Checking the internal formatter alone could pass while writing the wrong paragraphs to the archive.

**1.** Search by the output format and requested feature, without assuming a style enum.

~~~powershell
Find-Files 'chapter heading|chapterheading|scene break' '*.cs' | Where-Object { $_ -match 'Docx|Export|Profile' }
~~~

**Found.** This located the DOCX formatter, shipped profiles and relevant export tests. I used the profiles to define the test denominator before looking at the writer’s switch.

**2.** Inventory shipped profiles, then locate the existing DOCX inspection harness.

~~~powershell
Find-Lines 'ChapterHeadingStyle\s*=' 'BuiltInExportProfiles.cs'
Find-Paths 'Docx.*Test|ExportFormatter.*Test'
~~~

**Found.** Seven built-ins select six different heading values. Two use numbered. The others use centered-caps, fictionforge, centered, title-only and equals. Existing DOCX tests provide a way to open the exported document.

**3.** Trace profile selection into the body writer and distinguish it from the table of contents.

~~~powershell
Read-Lines 'DocxExportFormatter.cs' 133 169 716 742
Find-Lines 'WordprocessingDocument|MainDocumentPart|Paragraph|SceneBreak' '*Docx*Tests.cs' | Select-Object -First 22
~~~

**Found.** The chapter loop reaches AddChapterHeading. Fictionforge writes separate number and title paragraphs. Other styles use the formatted heading. Existing assertions traverse document parts/paragraphs, so text from a TOC need not satisfy a body assertion.

**4.** Work out boundary expectations independently as literal examples.

~~~powershell
Read-Lines 'DocxExportFormatter.cs' 1558 1620
~~~

**Found.** Centered-caps spells ten and uses digits for eleven. Centered uses the number, title-only preserves the raw title, and numbered and equals reach the ordinary number-and-title form. Fictionforge applies title casing, preserving internal capitals and lowercasing interior small words.

**5.** Cross-check the two variants sharing a style and identify unrelated modes.

~~~powershell
Read-Lines 'BuiltInExportProfiles.cs' 24 40 55 71 89 108 125 144 161 176 193 208 228 244
~~~

**Found.** The two numbered profiles are Query Package and KDP EPUB. Shunn, KDP Print, IngramSpark, Smashwords and Plain Manuscript cover the other five. The exporter has extra branches that no built-in selects.

**Handoff.** I can specify literal expected body paragraphs for every shipped style. I would use chapters 10 and 11, a title such as “the road to iPhone and the sea”, and assert the separate title paragraph where applicable. The test should read the emitted DOCX and use expectations independent of formatter helpers.

<a id="bookbuilder-t2"></a>

#### Bookbuilder T2 · Which pipeline checks to run

> I'm about to change how the generation pipeline orchestrates stages. Which test suites cover the pipeline, how do I run them on my machine (quick loop and the full gate), which of them need real provider keys, and does anything run them automatically?

**My reading.** I would distinguish a fast edit loop, the required pre-review gate, and paid or opt-in suites. “Automatically” could mean local script selection, a release prerequisite or remote CI. Each needs its own source.

**1.** Inventory orchestration tests and conventional automation entry points.

~~~powershell
Find-Paths 'Pipeline.*Tests|pipelineStore.*test|test-all|smoke-tests|^AGENTS\.md$|(^|/)workflows/'
~~~

**Found.** The paths include contributor instructions, a PowerShell gate, a smoke roster, backend suites and a Vue store test. I next determined what the repository actually requires for orchestration changes.

**2.** Identify the repository rule and the script responsible for enforcing it.

~~~powershell
Read-Lines 'AGENTS.md' 55 75
Read-Lines 'pro/scripts/test-all.ps1' 1 48
Find-Lines 'PipelineOrchestrator|MoRPipelineBuilder|GenerationDecision' 'smoke-tests.txt'
~~~

**Found.** The rules call for the full gate for orchestration changes. The default script samples suites and includes changed tests. Full and optional E2E are separate switches.

**3.** Look past filenames to see which behaviors the suites assert.

~~~powershell
Find-Lines 'public (async Task|void)' 'PipelineOrchestrator*Tests.cs' 'PipelineExecutionTests.cs' 'PipelineStageDispatchTests.cs' 'PipelineHandlerTests.cs' | Select-Object -First 60
Find-Paths 'pipelineStore\.test\.ts|MoRPipelineBuilderTests'
~~~

**Found.** There are distinct suites for graph selection/validation, dispatch, budget, checkpoint/invalidation, handlers, pipeline construction and the frontend store. A single class filter misses important consumers.

**4.** Check whether integration means offline or requires a real key.

~~~powershell
Find-Lines 'IncludeIntegration|testsettings.secret|ApiKeys:|OPENAI_API_KEY|WFP_WIZARD_SMOKE_LIVE' 'WritingForgePro.Tests.csproj' '*IntegrationTests.cs' '*Wizard*Tests.cs' | Select-Object -First 70
~~~

**Found.** Ordinary runs exclude tagged integration tests. The live provider suites load their own credentials and can skip for a missing key. Offline replay and separately authorized paid wizard tests exist. A collected test count alone cannot prove it made a live call.

**5.** Find the trigger as well as the command. Include hidden tracked workflow names in the search.

~~~powershell
Find-Lines 'dotnet test|vitest|RunE2E|test-all.ps1' 'test-all.ps1' 'release.ps1'
$ci = @(Find-Paths '(^|/)(\.github/workflows/|\.githooks/|azure-pipelines|\.gitlab-ci|Jenkinsfile)')
"CI or hook path matches: $($ci.Count)"
~~~

**Found.** The local script drives the back and front ends. No matching checked-in conventional CI/hook path appeared in this export, and the release scripts examined do not invoke the gate. Server-side automation cannot be inferred from this snapshot.

**Handoff.** From the repository root, a focused command is `dotnet test pro/src/WritingForgePro.Tests/WritingForgePro.Tests.csproj --filter 'FullyQualifiedName~PipelineOrchestratorTests|FullyQualifiedName~PipelineOrchestratorBudgetTests|FullyQualifiedName~PipelineStageDispatchTests|FullyQualifiedName~PipelineExecutionTests'`. From `pro/src/WritingForgePro-ui`, `npx vitest run src/stores/__tests__/pipelineStore.test.ts` exercises the screen-side store. `pwsh pro/scripts/test-all.ps1` is the sampled loop and `pwsh pro/scripts/test-all.ps1 -Full` the full ordinary gate. `-Restore` allows restore and `-RunE2E` includes browser tests. A selective live run adds `-p:IncludeIntegration=true --filter 'Category=Integration&FullyQualifiedName~OpenAIIntegrationTests'` to the backend project command. These are discovered commands, not recorded passes.

<a id="bookbuilder-a1"></a>

#### Bookbuilder A1 · One Generate request, out and back

> Trace what happens when an author starts a generation run from the Generate screen: from the Vue store call, across the Photino bridge, through the pipeline orchestrator and a stage, to the AI provider HTTP call, and back until the output is saved in the .bbpro project and progress reaches the screen. Name each component on the way.

**My reading.** I would follow one concrete AI stage. A list of classes would leave three unanswered questions: where a JavaScript action becomes a .NET call, where output becomes durable in this project, and how the UI distinguishes progress from completion.

**1.** Use the visible screen and action words to find where the trace starts.

~~~powershell
Find-Paths 'Generat.*View\.vue|pipeline.*Store\.ts|Photino|MessageRouter'
~~~

**Found.** The tree supplies a generation view, pipeline store and native router. I started with the screen and recovered the action from its sender before using the router as evidence.

**2.** Find the triggering gesture, payload and literal bridge action.

~~~powershell
Find-Lines 'pipelineStore.execute|currentProgress|applyRunOutcome' 'GenerationView.vue'
Find-Lines 'review\(|pipeline.execute|applyProgress' 'pipelineStore.ts'
~~~

**Found.** GenerationView submits a selected run. The store reviews the plan, sends pipeline.execute and installs a progress callback. The action string gives me an exact cross-language join.

**3.** Find sender, receiver, dispatch table and handler for that action.

~~~powershell
Find-Lines 'receiveMessage|sendMessage|isProgress' 'useBackend.ts'
Find-Lines 'RegisterWebMessageReceivedHandler|RouteAsync|SendWebMessage|ExecutePipelineHandler' 'Program.cs' 'MessageRouter.cs' 'PipelineBridgeHandlers.cs' 'ExecutePipelineHandler.cs'
~~~

**Found.** The Vue composable sends a correlated envelope through Photino. The host callback forwards it to MessageRouter and supplies the callback for replies. ExecutePipelineHandler adapts stage progress and invokes the orchestrator.

**4.** Follow the AI branch instead of assuming every stage makes a network request.

~~~powershell
Find-Lines 'ProjectDbContextFactory|DynamicAIStageFactory|DispatchStage' 'PipelineOrchestrator*.cs'
Find-Lines 'GetProvider|GenerateWithTimeoutPolicyAsync|provider.GenerateAsync' 'PipelineStageBase.cs'
Find-Lines 'chat/completions|SendAsync|progress.Report' 'OpenAIProvider.cs'
~~~

**Found.** The orchestrator resolves the project and stage. For a dynamic AI stage, stage preparation and provider selection reach the registered adapter. The OpenAI example sends an HTTP chat request, processes the response and reports progress. Other stage types take different branches.

**5.** Trace both persistence and the two kinds of return message.

~~~powershell
Find-Lines 'GenerationArtifacts.Add|ApplyCompletedGenerationOutput|ParseAndStoreOutputAsync|CommitAsync|SaveChangesAsync' 'PipelineStageBase.cs' 'DynamicAIStage.OutputParsing.cs'
Find-Lines 'isProgress|onProgress|applyProgress|applyRunOutcome|SendWebMessage' 'useBackend.ts' 'pipelineStore.ts' 'GenerationView.vue' 'Program.cs'
~~~

**Found.** Stage output is recorded as a generation artifact and parsed into project entities through the project DB context. Progress reaches the matching pending callback, while the final reply resolves the promise and refreshes view state. These paths establish both ends of the trace.

**Handoff.** I can draw the request and return arrows through Vue, Photino, router, handler, orchestrator, stage, provider and the .bbpro project database. I would label OpenAI as the concrete HTTP example and keep progress delivery separate from the transaction that saves output.

<a id="bookbuilder-a2"></a>

#### Bookbuilder A2 · Assessing a macOS release

> We are considering shipping a macOS build again. Assess what that would take: what macOS runtime code still exists and is wired up, what in the build, release and installer path is Windows-only, which project rules and tests would have to change, and where the platform documentation is out of date.

**My reading.** I would separate four questions: what code still runs on a Mac, what the release scripts can produce, what the repository permits shipping today, and what claims need actual hardware. A platform branch or RID in a project file is an initial clue.

**1.** Inventory platform evidence across different layers before assessing release readiness.

~~~powershell
Find-Files 'macOS|osx-arm64|osx-x64' '*.md' '*.cs' '*.csproj' '*.ps1' | Where-Object { $_ -match 'AGENTS|platform|Host|NativeUI|release|Keychain' }
~~~

**Found.** Policy and platform notes sit beside Host and NativeUI code and project files. They address different questions, so I followed an actual service registration and an actual release entry point separately.

**2.** Read the distribution policy and look for actual runtime branches.

~~~powershell
Find-Lines 'macOS|Windows installer|packaging' 'AGENTS.md' | Select-Object -First 35
Find-Lines 'IsMacOS|MacOs|Keychain|ConfigureForWindow' 'WindowStateService.cs' 'NativeDialogService.cs' '*Extensions.cs' 'Program.cs' | Select-Object -First 45
~~~

**Found.** Project rules retain Mac runtime behavior while specifying Windows distribution. Runtime paths exist for file handling, native dialogs and key storage. Registration/entry-point reads distinguish wired services from orphan classes.

**3.** Verify representative implementations and their selection at startup.

~~~powershell
Find-Lines 'MacOsAppFileSystem|MacOsShellFileService|KeychainKeyEncryptionService|IKeyEncryptionService|/usr/bin/security|ConfigureMacOS' '*.cs' | Select-Object -First 65
~~~

**Found.** The host selects Mac filesystem/shell services and the Keychain implementation of IKeyEncryptionService. The latter invokes the system security utility. Native-dialog dispatch selects a Mac-specific configuration.

**4.** Follow the publish input through the installer and look for a corresponding Mac packaging path.

~~~powershell
Find-Lines 'osx-|win-x64|dotnet publish|WiX|Burn' 'WritingForgePro.Host.csproj' 'release.ps1'
Find-Paths 'Package\.wxs|\.entitlements$|notari|\.pkgproj$|Info\.plist$'
~~~

**Found.** The Host project selects Photino native assets for Intel and Arm Macs. The release path publishes win-x64 and builds WiX media. No corresponding macOS distribution path was established by those files.

**5.** Compare test scope and prose with the code and release evidence.

~~~powershell
Find-Lines 'MacOs|macOS|WindowsCurrentBuildSearch' 'AppFileSystemTests.cs' 'InstallerPackageUiRegressionTests.cs' 'platform.test.ts'
Find-Lines 'IKeyProtector|Keychain|osx-arm64|osx-x64|Windows-only' 'platform-notes.md' 'architecture.md' 'devguide.md' | Select-Object -First 38
~~~

**Found.** Source tests cover portions of the abstractions and the Windows installer. Platform notes name an obsolete protection interface/implementation. Older architecture RID listings and present support policy need to be read alongside the actual release script.

**Handoff.** A release decision needs the approved Mac architectures and distribution policy. Implementation would need app bundling, signing/notarization as appropriate, packaging and a native install/upgrade/launch check including Keychain, dialogs and project open/save. The source identifies that work. Only a matching native run could qualify it.

<a id="bookbuilder-a3"></a>

#### Bookbuilder A3 · What adding a grant-proposal product would change

> We want to ship a new bundled pipeline product for grant proposals. Where does a new product's source go, what files and contract must it have, how does it get compiled into the installer payload, which validation rules must it pass, and what tests or counts will break when we add it?

**My reading.** I would first test the premise that the catalog lacks this product. If an identity already exists, updating it and creating a second independently selectable product have different compatibility and count consequences.

**1.** Test whether the requested product is genuinely new and find the authoring guide.

~~~powershell
Find-Paths 'grant.?proposal|pipeline.products/README|pipeline.*build|build.*pipeline' | Select-Object -First 120
~~~

**Found.** The path inventory exposes an existing grant-proposal family and a product authoring root. That changes the question from simply adding a folder to distinguishing an update from a second identity.

**2.** Find the authoring root and check for the requested subject before choosing an example.

~~~powershell
Find-Paths 'pipeline-products/README|build-seed-pipelines|validate-pipelines'
Find-Paths 'academic-grant-proposal/(product|pipeline|compatibility)\.json'
~~~

**Found.** The maintained product root has an authoring guide and already contains academic-grant-proposal. I would inspect that existing identity before assigning a new slug or changing a count.

**3.** Read the required source layout and parse the existing product's contract.

~~~powershell
Read-Lines 'pro/pipeline-products/README.md' 1 96
Get-ChildItem -Name pro/pipeline-products/academic-grant-proposal
(Get-Content -Raw pro/pipeline-products/academic-grant-proposal/pipeline.json | ConvertFrom-Json).generationContract.PSObject.Properties.Name
~~~

**Found.** Source includes product and pipeline manifests, stage definitions and prompts. This product also has entities, interview material and compatibility maps. The generation contract identifies intake, required artifact stages, final prose, supported forms and output capabilities.

**4.** Trace a source product all the way into installer inputs.

~~~powershell
Find-Lines 'PipelineBuilder|catalog.json|\.bbpipe|expectedCount' 'build-seed-pipelines.ps1'
Find-Lines 'build-seed-pipelines|seedpipelines|Files Include' 'pro/scripts/release.ps1' 'WritingForgePro.Host.csproj' 'Package.wxs'
~~~

**Found.** PipelineBuilder compiles both maintained product roots. The seed script validates the builder catalog and stages .bbpipe files plus catalog.json. The Host project copies those artifacts into publish output, which WiX consumes.

**5.** Distinguish enforced invariants, calculated counts and fixed expectations.

~~~powershell
Find-Lines 'compatibility|generationContract|requiredArtifactStageIds|targetEntityType' 'validate-pipelines.ps1' | Select-Object -First 35
Find-Lines 'Assert.Equal\(46|Assert.Equal\(30|academic-grant-proposal|ProductSpecificFields.Keys|build-seed-pipelines' 'AcademicPipelineProductCompletenessTests.cs' 'AllPipelinesBundledTests.cs' 'InstallerReleaseBundleRegressionTests.cs'
~~~

**Found.** The validator checks compatibility and stage/artifact contracts. Academic completeness tests enumerate the grant product. Bundling tests compare source and catalog and also fix totals of 46 products and 30 nonfiction entries. Stable IDs, unique slugs and matching package metadata remain independent obligations.

**Handoff.** I can explain how to edit the existing product while preserving identity, or what a genuinely new product would add to authoring, validation, catalog and tests. Which identity the user intends is the one product decision the source cannot make.

### Munarium

<a id="munarium-c1"></a>

#### Munarium C1 · A ninth output budget without losing tenant settings

> Vocabulary generation currently borrows the general completion default for its output-token ceiling. Give it its own per-call budget, `vocabulary_generation`, alongside the existing eight: a built-in value, a `MUNARIUM_MAX_TOKENS_*` variable, the `/v1/max-tokens` read and replace, and the client libraries, with tests. Tenants that have already replaced their budgets must keep their settings after the upgrade.

**My reading.** I would look for three owners that may differ: the call that spends tokens, the contract that exposes limits, and the storage that survives upgrade. A new default working on a fresh install would say little about an existing tenant’s replacement settings.

**1.** Search the exact current API name and the language used for the new feature.

~~~powershell
Find-Files '/v1/max-tokens|vocabulary_generation|complete_default' '*.rs' '*.sql' '*.proto' | Where-Object { $_ -match 'vocab|max.tokens|api.types|proto' }
~~~

**Found.** The vocabulary service, budget API/store and protocol sources are separate leads. I started at the consumer to establish which existing budget it borrows.

**2.** Find the current vocabulary call and compare it to another independently budgeted call.

~~~powershell
Find-Lines 'complete_default|max_tokens:' 'vocabulary_api.rs'
Find-Lines 'complete_default|MUNARIUM_MAX_TOKENS|effective\(|replace\(' 'max_tokens_api.rs'
~~~

**Found.** The vocabulary call uses the tenant’s effective complete_default. The max-tokens service has separate environment bindings, validation, effective reads and whole replacement. The consumer and the owner both need attention.

**3.** Trace the public shape and default through both transports.

~~~powershell
Find-Lines 'struct MaxTokens|complete_default|/v1/max-tokens' 'server/src/munarium-api-types/src/lib.rs' 'server_api.proto' 'grpc_api_generated.rs' 'max_tokens_api.rs' | Select-Object -First 42
~~~

**Found.** The shared DTO holds the fields/defaults, and the server exposes GET and replace. The generated gRPC surface bridges those operations. An additive server field has client DTO and validation implications.

**4.** Read how an old eight-field value is written and loaded before proposing an upgrade.

~~~powershell
Read-Lines 'server/src/munarium-store-pg/src/max_tokens.rs' 16 74
Read-Lines 'max_tokens_api.rs' 156 212
Read-Lines '0031_max_tokens_budgets.sql' 1 50
~~~

**Found.** PostgreSQL stores a tenant’s replacement as one JSONB document. Failure to decode that document can fall back to process defaults. Requiring a ninth key while reading stored rows would silently discard effective tenant overrides.

**5.** Enumerate client representations and examine the current acceptance and refusal tests.

~~~powershell
Find-Lines 'complete_default|CompleteDefault|completeDefault' 'clients/**' | Select-Object -First 60
Find-Lines 'fn .*default|fn .*replac|fn .*missing|fn .*range' 'max_tokens_api.rs'
Find-Paths 'test_max_tokens|MaxTokensTests'
~~~

**Found.** Python, Java and .NET carry model fields and Rust exposes the shared budget type. There are focused route/client tests for replacement and validation. An existing persisted row with distinctive values is a necessary upgrade fixture.

**Handoff.** I would add a ninth effective value and environment binding, wire vocabulary to it, cover read/replace and SDKs, and supply a backward-compatible stored-row read or migration. Missing public replacement fields and old persisted fields have different meanings. The prompt leaves the exact numeric default open. Reusing the previous effective default is one way to preserve fresh-install behavior pending a product choice.

<a id="munarium-c2"></a>

#### Munarium C2 · Why an expired session can still take a turn

> Operators set MUNARIUM_SESSION_IDLE_TTL_SECS=30, yet a session left idle for more than a minute sometimes still accepts a turn instead of answering 409 session-not-open. Find the cause and fix it so an idle session is refused as soon as its TTL has passed.

**My reading.** The timing and the word “sometimes” suggest either a stale periodic update or the wrong clock/last-activity field. I would put timeout configuration, background expiry and request admission on one timeline before choosing a fix.

**1.** Search the exact environment setting and error slug independently.

~~~powershell
Find-Files 'MUNARIUM_SESSION_IDLE_TTL_SECS|session[-_]not[-_]open' '*.rs' '*.md' | Where-Object { $_ -match 'config|state|session|error|test' }
~~~

**Found.** I located the setting, session operations, error mapping and tests. The question became whether admission uses the same time evidence as the expiry writer.

**2.** Find where the TTL is read and where the reported error originates.

~~~powershell
Find-Lines 'MUNARIUM_SESSION_IDLE_TTL_SECS|session_not_open' 'config.rs' 'state.rs' 'sessions_api.rs' 'error.rs'
~~~

**Found.** The TTL reaches server configuration. The typed refusal exists in turn admission. That makes it possible for configuration to be correct while a turn still succeeds.

**3.** Look at the expiry clock, starting timestamp, polling interval and disabled case.

~~~powershell
Read-Lines 'server/src/munarium-server/src/state.rs' 570 605
~~~

**Found.** The PostgreSQL janitor is disabled at zero. Otherwise it starts with jitter and polls at intervals of at least 60 seconds. It bases expiry on last turn or creation and the database clock. A 30-second TTL can therefore pass before the next sweep.

**4.** Inspect a request that arrives during that interval and locate the write that refreshes activity.

~~~powershell
Read-Lines 'sessions_api.rs' 1300 1341 1855 1880
Find-Lines 'last_turn_at|created_at' '0013*.sql' | Select-Object -First 12
~~~

**Found.** Turn admission loads the session, checks caller ownership and checks its stored open state before work. That sequence does not compare idle age. A later write updates last activity. A still-open row explains the symptom without invoking a client cache.

**5.** Check transport entry points, error shape and which existing tests discriminate this defect.

~~~powershell
Find-Lines 'op_turn_covered|/v1/sessions/\{id\}/turns|session-not-open' 'sessions_api.rs' 'grpc_api_generated.rs' 'error.rs' 'clearview_r11_tests.rs' 'sessions_model_tests.rs' | Select-Object -First 70
~~~

**Found.** Unary/stream paths converge on the turn owner and existing tests cover parts of the problem vocabulary. Seeing a typed error for a closed session would not catch an idle row that is still marked open.

**Handoff.** I can design the fix at admission and a deterministic two-clock regression: leave the janitor unrun, admit before the cutoff, refuse expired first/subsequent turns before side effects, and exercise zero-disable and concurrent refresh. Define exact-cutoff equality consistently with the stated TTL. None of this requires waiting a real minute.

<a id="munarium-c3"></a>

#### Munarium C3 · A file-count change that preserves the byte ceiling

> Raise the per-request file cap for batch ingest and bulk-upload chunks from 500 to 1,000 files. Update every place that enforces, tests or documents the limit, on both transports and in the client libraries, and leave the 256 MiB body ceiling as it is.

**My reading.** I would split the request into two operations, two server entry points and four clients. I would keep an independent column for bytes. The number 500 also appears in status codes, port numbers, backoff and unrelated limits, so a global replacement would be unsafe.

**1.** Find count and byte claims together, then distinguish their units.

~~~powershell
Find-Files '500 files|500-file|256 MiB|files\.len\(\) > 500' '*.rs' '*.md' '*.py' '*.cs' '*.java' | Where-Object { $_ -match 'ingest|bulk|rest|client|openapi' } | Select-Object -First 85
~~~

**Found.** Both server and client source and documentation mention limits. Searching by files/bytes brought up ingestion and transport owners without treating every integer 500 as an edit site.

**2.** Locate both file-count checks and the byte-limit owner using their surrounding nouns.

~~~powershell
Find-Lines 'files.len\(\) > 500|at most 500 files|MAX_SOURCE_BYTES.*256' 'ingest_api.rs' 'server/src/munarium-server/src/rest.rs'
~~~

**Found.** The batch and chunk handlers each enforce a 500-file maximum. MAX_SOURCE_BYTES is an independent 256 MiB limit. The manifest also has its own count limit.

**3.** Follow actual transport bindings and check for a wrapper that cannot issue one operation.

~~~powershell
Find-Lines '/v1/ingest/batch|/v1/ingest/bulk/\{bulk_id\}/chunk' 'grpc_api_generated.rs' 'server/src/munarium-server/src/rest.rs'
Find-Lines 'REST.only|bulk_chunk|bulkChunk' 'Planes.java' 'clients/rust/munarium-client/src/grpc.rs' | Select-Object -First 25
~~~

**Found.** The generated server gRPC routes forward both operations to their corresponding HTTP-shaped handlers. A client can still expose a narrower surface. Java’s client contract describes bulk chunks as REST-only, so server reachability alone does not prove every SDK supports that call on gRPC.

**4.** Find each client’s guard and follow the constant through its users.

~~~powershell
Find-Lines 'MAX_FILES_PER_CHUNK|BulkMaxFilesPerChunk|check_bulk_chunk_size' '_errors.py' 'Validation.cs' 'Planes.cs' 'Wire.java' 'planes.rs' 'grpc.rs' 'rest.rs' | Select-Object -First 72
~~~

**Found.** The Python constant in _errors.py, the .NET IIngestPlane constant and Validation.cs, Java Wire, and Rust planes.rs all impose a lower/upper file count. Client-side refusal can occur before the server sees a request.

**5.** Look for assertions, examples and documentation tied to the old boundary.

~~~powershell
Find-Lines '501|500 files|500-file|500 entries' 'test_c8_surface.py' 'PlatformTests.cs' 'PlatformSmokes.cs' 'bulk_upload.rs' 'ingest-v2.md' 'openapi.json' | Select-Object -First 40
Find-Lines '256 MiB|MAX_SOURCE_BYTES|body_limit_for_route' 'server/src/munarium-server/src/rest.rs' 'ingest_api.rs' | Select-Object -First 22
Find-Lines 'max_decoding_message_size|MAX_SOURCE_BYTES' 'server/src/munarium-server/src/main.rs' 'grpc_data.rs'
~~~

**Found.** Old examples and negative fixtures encode the previous ceiling. The route/body-policy read gives an independent byte-control case. I would classify each match by operation and unit before including it. The canonical gRPC decoder uses the same byte constant. A separate data service has framing allowance plus an aggregate-content guard; its allowance is not a new upload file-count limit.

**Handoff.** I can specify 1,000 tiny files accepted and 1,001 rejected for each supported surface, with empty-input semantics preserved, and a small-count oversized-body case that still fails for bytes. The server and client transport matrices must be reported separately.

<a id="munarium-w1"></a>

#### Munarium W1 · What authors of clients need to know about retiring a runbook

> Write the user-guide page on retiring a runbook version: the two-step request-and-confirm flow and its time limit, what callers and existing sessions see afterwards, what is and is not deleted, whether it can be undone, and how to call it from each client library.

**My reading.** I would translate the user’s word “retire” into the product’s route names, then follow a single version through request, confirmation, reads and an already-open session. I would verify what is stored, what is visible and whether the ID can ever be reused.

**1.** Translate “retire” into action names used by route docs, storage and SDKs.

~~~powershell
Find-Files 'retir.{0,25}runbook|runbook.{0,25}retir|remove-confirm|remove-request' '*.md' '*.rs' | Where-Object { $_ -match 'runbook|api|client' }
~~~

**Found.** The public vocabulary includes remove-request and remove-confirm. I followed the paired operations to find which identity they take and what survives confirmation.

**2.** Discover the product vocabulary and read both state transitions.

~~~powershell
Find-Lines 'remove-request|remove-confirm|request_removal|confirm_removal' 'runbooks_api.rs'
Read-Lines 'runbooks_api.rs' 1284 1354 1364 1428
~~~

**Found.** The operation is soft removal of an exact name@version. The request yields an ID and expiry. Confirmation must match that pending ID within 15 minutes. Missing/mismatched confirmation fails, and expiry handling resets the pending state.

**3.** Read selection, reactivation and listing to distinguish visibility from deletion.

~~~powershell
Read-Lines 'runbooks_api.rs' 32 72 775 807 1558 1576
~~~

**Found.** A bare name selects the latest eligible version. A removed version is normally hidden, with an include-removed option for listing. Reapplying the removed identity is refused. The status update leaves the version material and historical records in place.

**4.** Check the fate of a session that already pinned the version.

~~~powershell
Read-Lines 'sessions_api.rs' 433 465 482 488 1328 1346
~~~

**Found.** A later turn re-resolves the session’s exact stored reference. A removed version returns the typed removal error (HTTP 410), even for an existing open session. The session record surviving does not imply another turn succeeds.

**5.** Verify each client spelling and how the request’s result becomes confirm input.

~~~powershell
Find-Lines 'remove_request|remove_confirm|removeRequest|removeConfirm' 'planes.rs' 'rest_planes.py' 'grpc_transport.py' 'RestTransport.java' 'GrpcTransport.java' 'RestTransport.Platform.cs' 'GrpcTransport.Platform.cs' | Select-Object -First 100
~~~

**Found.** Rust and Python expose remove_request/remove_confirm, Java removeRequest/removeConfirm and .NET RemoveRequestAsync/RemoveConfirmAsync. All need the exact version and the returned removal ID. Python has sync and async surfaces.

**Handoff.** I can write a two-call example in each language, describe the deadline, distinguish a missing/expired confirmation from a removed version, and explain that a new version is the route forward. The guide should name the surviving YAML, history, collection/index data and session records. Expiry can reset an unconfirmed request. Confirmed removal cannot be undone through this public flow.

<a id="munarium-w2"></a>

#### Munarium W2 · Which claim a later reader sees

> Write an explainer for a new contributor on how a proposed claim is governed and written: what the command path does between the REST or gRPC handler and the database, what the gates decide, how a blocked claim, a head conflict and a concurrent writer are handled, and what a later read then returns.

**My reading.** I would follow a proposed claim twice: once with blocking evidence and once with a head that changes between evaluation and commit. Then I would read it through two different queries. “Blocked” might mean refused, or accepted into history with a disputed status.

**1.** Find the command and error from the caller’s own vocabulary.

~~~powershell
Find-Files 'propose_claim|head.conflict|blocked claim' '*.rs' | Where-Object { $_ -match 'rest|grpc|service|gate|ledger|store' } | Select-Object -First 85
~~~

**Found.** Two transport handlers, a shared service, gate code and store code match. I used the handlers’ actual call as the join into governance.

**2.** Find both entry points and the shared command they call.

~~~powershell
Find-Lines 'propose_claim' 'server/src/munarium-server/src/rest.rs' 'server/src/munarium-server/src/grpc.rs'
Read-Lines 'server/src/munarium-server/src/rest.rs' 314 355
Read-Lines 'server/src/munarium-server/src/grpc.rs' 215 248
~~~

**Found.** REST and gRPC authenticate the request and handle idempotency, shape and chronology inputs before calling append_events. I would follow that common owner once and return to transport errors afterward.

**3.** Read the gate inputs, classifications and the decision to append.

~~~powershell
Read-Lines 'server/src/munarium-server/src/service.rs' 82 220
Find-Lines 'fn |Blocking' 'server/src/munarium-core/src/gates.rs' | Select-Object -First 40
~~~

**Found.** The service checks the head, loads a pinned snapshot and governance policy and evaluates applicable gates, including anchor/ledger conflicts, corrections, text checks where applicable, chronology and shape. A keyed block yields a disputed claim and findings. Warnings alone can accompany an accepted claim.

**4.** Examine the transaction and the retry decision under a racing writer.

~~~powershell
Read-Lines 'server/src/munarium-store-pg/src/lib.rs' 165 260 744 777
Read-Lines 'server/src/munarium-server/src/service.rs' 194 220
Find-Lines 'HeadConflict|ABORTED' 'server/src/munarium-server/src/error.rs' | Select-Object -First 16
~~~

**Found.** The store rechecks the head under the lineage lock and commits projection, event, head and findings together. A caller-pinned mismatch surfaces. An unpinned race can take a fresh snapshot and re-evaluate, up to three total attempts. Final conflict maps to HTTP 409 or gRPC ABORTED with head information.

**5.** Read the direct lookup and the default projection using the same hypothetical disputed ID.

~~~powershell
Read-Lines 'server/src/munarium-server/src/service.rs' 319 363
Read-Lines 'server/src/munarium-core/src/ledger.rs' 85 140
~~~

**Found.** Direct lookup can return a disputed record and supersession details. The facts view defaults to accepted claims, can request disputed status, and applies the as-of boundary before resolving supersession. A transaction that ultimately conflicts added no claim.

**Handoff.** I have the components and three distinct outcomes for a contributor diagram: accepted or disputed append, explicit head conflict, and implicit concurrent re-evaluation. I can give a later-read example for both history and the default facts view.

<a id="munarium-t1"></a>

#### Munarium T1 · An offline test that would catch an accidental second send

> Make sure the Python client's REST retry rules are proven offline, end to end through the real transport: reads retry transient failures, a command is re-sent with the same idempotency key only after a connect-phase failure or a typed `overloaded`, never after a gateway 502 or 504, and plain writes are sent exactly once. Cover both the sync and the async transport, and add whatever tests are missing.

**My reading.** I would count requests at the HTTP transport boundary, not calls to the retry predicate. I need a cross-product of operation kind, failure phase and sync/async client. A scripted success following a forbidden retry makes a wrong second send observable.

**1.** Look for production REST transport and tests that actually send a request offline.

~~~powershell
Find-Files 'MockTransport|ConnectError|idempotency.key' 'clients/python/**' | Where-Object { $_ -match 'rest|retry|replay|review|transport' }
~~~

**Found.** A REST implementation and several test files match. I checked which test installs a scripted transport and inspects requests, since a helper with “retry” in its name might never send one.

**2.** Find the actual transports and the closest wire-level tests.

~~~powershell
Find-Paths 'clients/python/.*(rest\.py|_specs\.py|test_replay_safety|test_review_batch|pyproject)'
Find-Lines 'MockTransport|ConnectError|502|504|overloaded' 'test_review_batch.py' 'test_replay_safety.py' | Select-Object -First 42
~~~

**Found.** test_review_batch.py uses httpx.MockTransport and records outbound requests. The similarly named replay-safety tests also contain predicate-level checks. I would inspect the wire-level assertions before counting coverage.

**3.** Inventory what the existing assertions observe.

~~~powershell
Read-Lines 'clients/python/tests/test_review_batch.py' 140 236
Read-Lines 'clients/python/tests/test_replay_safety.py' 43 69
~~~

**Found.** The sync matrix tests read recovery from 502/504, stable supplied keys after typed overload, connect failure by send count, and refusal to replay commands on gateway 502/504. The plain-write predicate test never sends a request. The connect test leaves key equality unasserted.

**4.** Read both request loops to make sure a sync test cannot mask an async defect.

~~~powershell
Read-Lines 'clients/python/src/munarium_client/rest.py' 152 190 238 286 350 408
~~~

**Found.** Each transport computes the command key before its attempt loop. The loops distinguish connect-phase errors and typed overload from ambiguous failure. Async has its own request and backoff code. Both take an injectable HTTP transport.

**5.** Choose real operation specs, an async fixture and exact observables.

~~~powershell
Find-Lines 'def head|def create_version|def upsert_digest' 'clients/python/src/munarium_client/_specs.py'
Find-Lines 'asyncio|pytest' 'clients/python/pyproject.toml'
~~~

**Found.** head, create_version and upsert_digest supply read, keyed command and plain-write specimens. The repo supports pytest-asyncio. Run each against a scripted transport, recording method, target, attempt count and every idempotency header while replacing only the delay.

**Handoff.** I can lay out the missing tests: transient read then success, command connect failure or typed overload then success with the same generated or supplied key, command 502/504 with one send, and plain writes with one send under connect failure and overload. Include an ambiguous post-connect error. Apply the same table to sync and async. The table describes proposed assertions, not a test run.

<a id="munarium-t2"></a>

#### Munarium T2 · Checks for a new REST route and SQL migration

> I've added a new REST route to the server, backed by a new SQL migration. Before I open the pull request, list the checks I must run, with the exact commands, and say what each one does and does not prove.

**My reading.** I would treat the API and database change as two contracts. A green build could miss both route registration and an upgrade failure. Since the prompt supplies no diff, I would first find the repository’s standard commands and then identify what requires the actual route, SQL and base revision.

**1.** Find the published entry points for pre-PR checks and migrations.

~~~powershell
Find-Paths '(^|/)(AGENTS\.md|CONTRIBUTING\.md|validation\.md|server-ci\.yml|gate.catalog.*|test\.ps1|gates\.ps1)$'
~~~

**Found.** The inventory identifies contributor rules, server validation docs, scripts, gate catalog and a checked-in CI workflow. These let me recover exact commands despite the prompt omitting the patch.

**2.** Find contributor instructions and the working directory assumed by the validation ladder.

~~~powershell
Find-Lines 'migration|format|test.ps1|gates.ps1|gate_catalog' 'AGENTS.md' 'CONTRIBUTING.md' | Select-Object -First 38
Read-Lines 'server/docs/guides/validation.md' 1 25 194 213
Read-Lines 'CONTRIBUTING.md' 45 63
~~~

**Found.** The native scripts run from server/ under PowerShell 7. The guide distinguishes local offline checks, optional PostgreSQL and black-box coverage, and the broader gate; the DB tiers require a suitable disposable service or container.

**3.** Read what CI actually invokes for source, tests and the route contract.

~~~powershell
Find-Lines 'gate_catalog.py|cargo run.*openapi|openapi.json|working-directory' '.github/workflows/server-ci.yml'
~~~

**Found.** The server workflow invokes catalog-backed format, Clippy, test and migration gates and regenerates OpenAPI for a structural comparison with the committed document. Those commands and their directory are explicit.

**4.** Find the implementation of the migration check and the code that applies migrations.

~~~powershell
Find-Lines 'migrations.additive|clippy.default|tests.workspace' 'gate-catalog.json'
Find-Lines 'destructive DDL|sqlx::migrate!' 'gate_catalog.py' 'server/src/munarium-store-pg/src/lib.rs'
~~~

**Found.** Static migration policy checks history/additivity. The SQLx store applies the migrations. Neither source scan nor a successful fresh schema proves that an already populated predecessor database upgrades correctly.

**5.** Recover the invocation and dependencies for the behavior checks.

~~~powershell
Find-Lines 'test.ps1|gates.ps1|Postgres|BlackBox|OpenAPI|Docker' 'server/docs/guides/validation.md' | Select-Object -First 65
Read-Lines '.github/workflows/server-ci.yml' 217 247
~~~

**Found.** The guide supplies explicit local tiers; CI shows the OpenAPI generation/comparison. For this prompt a focused HTTP request with an independent expected response and a predecessor-data upgrade are still case-specific. Their exact selectors need the patch.

**Handoff.** From `server/`, `python tools/gate_catalog.py runner.regression catalog.regression catalog.equivalence format` checks gate definitions and formatting. `python tools/gate_catalog.py clippy.default clippy.all-features tests.workspace` covers lint and workspace tests; `python tools/gate_catalog.py migrations.additive` checks migration policy. `cargo run -q -p munarium-server -- openapi` emits the schema; the `.github/workflows/server-ci.yml` block at lines 227–235 parses the generated JSON and the BOM-aware committed JSON and asserts structural equality. `./test.ps1` is the local loop, `./test.ps1 -Postgres -BlackBox -Platform -Cluster` exercises the documented provisioned tiers, and `./gates.ps1` runs the broader ladder. The contributor ladder also lists an Enterprise tier. From the repository root, `py check_license.py` and `py clients/check_compatibility.py` are required cross-cutting checks. Only targeted execution on the actual inputs can qualify this unspecified patch. For that, inspect its base/head and exercise both the endpoint and an upgrade with representative old data.

<a id="munarium-a1"></a>

#### Munarium A1 · How a collection-filtered search gets its results

> Trace a `POST /v1/search` request that filters on one collection, from the moment it reaches the REST listener to the ranked hits and provenance envelope it returns, naming each component it passes through and what each one decides. Assume the default PostgreSQL retrieval mode and a collection with no inference stages.

**My reading.** I would fix the example to one authorized collection, default PostgreSQL retrieval and no inference stages. I would follow the actual route in both directions and check whether provenance is always present or depends on an option. “No inference stages” alone tells me nothing about the retrieval engine’s built-in vector leg.

**1.** Find the exact route literal and its caller-facing documentation or handler.

~~~powershell
Find-Files '/v1/search' '*.rs' '*.md' | Where-Object { $_ -match 'rest|server|retrieval' } | Select-Object -First 85
~~~

**Found.** The route occurs in clients, server and documents. The server’s routing table is the entry point for this trace. I followed its handler rather than selecting a result on the strength of its prose description.

**2.** Find the listener, registration and handler for this exact operation.

~~~powershell
Find-Lines 'TcpListener|axum::serve|rest::router' 'server/src/munarium-server/src/main.rs'
Find-Lines 'route\("/v1/search"|fn hybrid_search|fn data_plane_access' 'server/src/munarium-server/src/rest.rs'
~~~

**Found.** The server listener serves the Axum router. POST /v1/search is bound to hybrid_search. The route gives me a specific handler to inspect instead of every API carrying the word search.

**3.** Follow the single collection filter through access, resolution and request preparation.

~~~powershell
Read-Lines 'server/src/munarium-server/src/rest.rs' 121 144 1261 1342
Find-Lines 'fn retrieval_for' 'server/src/munarium-server/src/state.rs'
~~~

**Found.** The handler resolves caller and query scope, uses tenant retrieval, resolves the collection ID or name and checks visibility and active state. It supplies vocabulary/defaults and a hit limit. These decisions determine which index and query go downstream.

**4.** Follow the enabled execution route and read how candidates become ranked hits.

~~~powershell
Find-Lines 'search_governed|search_collection_dispatch|search_collection_prepared' 'server/src/munarium-retrieval/src/*.rs'
Find-Lines 'fn resolve_collection_index|fn prepare_query|rrf|lexical|vector' 'server/src/munarium-retrieval-pg/src/collections.rs' | Select-Object -First 80
Read-Lines 'server/src/munarium-retrieval/src/inference.rs' 383 418
Read-Lines 'server/src/munarium-retrieval/src/lib.rs' 446 512
Read-Lines 'server/src/munarium-retrieval-pg/src/collections.rs' 1560 1610
~~~

**Found.** AppState supplies retrieval bound to the caller. After preparation, search_collection_disclosed calls search_governed. Its no-stage branch reaches search_collection_dispatch and PgRetrieval::search_collection_prepared. That owner resolves the index, combines eligible lexical and hash-vector candidates by reciprocal-rank fusion, limits results and builds the envelope. Visibility checks still apply.

**5.** Trace the returned evidence and the optional execution disclosure independently.

~~~powershell
Find-Lines 'fn envelope_for|watermark|provider_fingerprint' 'server/src/munarium-retrieval-pg/src/lib.rs'
Read-Lines 'server/src/munarium-server/src/rest.rs' 1205 1256 1332 1364
~~~

**Found.** The provenance builder collects source identities/paths and hashes, index version, watermark and provider identity. REST converts the hits and envelope to response DTOs. Execution-detail disclosure depends here on req.disclose_execution. Absence of that diagnostic is distinct from missing provenance.

**Handoff.** I can draw the path from listener to filtered store, candidate generation, rank fusion and response, naming the components with a source anchor for each decision. This is a source trace of the specified configuration. It does not supply measured ranks or timing for a particular live request.

<a id="munarium-a2"></a>

#### Munarium A2 · Changing the default PDF extractor

> We're thinking of making isolated PDF extraction the default instead of inline. What would that change for existing deployments (index identities, which documents end up indexed, builds, paid escalation, hosts that are not Linux), and what would have to change alongside it?

**My reading.** I would model two deployments: an existing index built inline and a fresh build after the change. I would classify small successful, resource-limited and failed PDFs separately. I would then ask which decisions cost money and which protections depend on the operating system.

**1.** Locate both possible modes and the operational description of their differences.

~~~powershell
Find-Files 'isolated.{0,32}pdf|pdf.{0,32}isolated|pdf.extraction' '*.rs' '*.md' '*.toml' | Where-Object { $_ -match 'config|extract|guide|README|retrieval' } | Select-Object -First 90
~~~

**Found.** I found operator configuration, extractor code and a document-intelligence guide. I used the active profile as authority for defaults, then followed coverage and publication behavior.

**2.** Find the default, overrides and the executable profile actually used.

~~~powershell
Find-Lines 'MUNARIUM_PDF_EXTRACTION|EscalateOverBudget|PdfExtraction' 'server/src/munarium-server/src/config.rs' | Select-Object -First 45
Read-Lines 'server/src/munarium-extract/src/isolated/mod.rs' 89 108
~~~

**Found.** Inline is the current default. Isolated mode has a bounded profile including 512 MiB memory, 30 CPU seconds, 60 wall seconds, 64 MiB output and bounded concurrency/queue. An explicit inline setting and an unset setting would react differently to a default change.

**3.** Trace extractor identity into index publication, rebuild and activation.

~~~powershell
Read-Lines 'server/docs/guides/document-intelligence.md' 208 263
Find-Lines 'extractor_version|deferred_sources|ExtractionDeferred' 'server/src/munarium-retrieval-pg/src/lib.rs' | Select-Object -First 34
~~~

**Found.** The extractor identity includes mode/profile and applicable OS controls. Already published versions remain usable. A new build can differ in indexed coverage, even where successful extracted text agrees, and must be verified and activated before replacing the serving version.

**4.** Trace deferral, build failure and paid fallback using their recorded classifications.

~~~powershell
Read-Lines 'server/src/munarium-retrieval-pg/src/lib.rs' 253 290 470 491
Find-Lines 'escalat|defer|skip|fail' 'server/docs/guides/document-intelligence.md' | Select-Object -Last 23
~~~

**Found.** Deferred sources can be declared in build coverage under skip policy. Fail policy can reject the build. Parser failure and an over-budget extraction have different escalation eligibility. Paid escalation needs applicable policy, a configured provider and budgets. Pure deferral does not itself trigger a paid call.

**5.** Read the platform split and compare operational documentation against source defaults.

~~~powershell
Find-Lines 'run_child|rlimit|env_clear|target_os|target_pointer_width' 'server/src/munarium-extract/src/isolated/mod.rs' 'server/src/munarium-server/src/main.rs' | Select-Object -First 46
Read-Lines 'server/README.md' 194 206
~~~

**Found.** The worker uses the same executable. The OS resource-limit path applies on 64-bit Linux. Other hosts still have process, deadline, output and admission boundaries with a different OS profile. The server README’s older provisional timings need reconciliation with the code.

**Handoff.** I can specify the affected operators, rebuild/activate/rollback sequence, coverage and spend changes, host limits and docs/tests. I would preserve an explicit inline opt-out and compare deferred, failed and successful documents before recommending a default. Deployment frequency, spend and performance require measurements outside this source walk.

<a id="munarium-a3"></a>

#### Munarium A3 · A session-discovery tool for an agent that lost its ID

> We want MCP agents to be able to list their own sessions, so that an agent which has lost its context can find the session it had open. Where does that tool belong, how does it relate to the existing tools and native routes, and what rules does a new MCP tool have to satisfy?

**My reading.** The critical words are “their own” and “lost its context.” A tool that already requires a session ID cannot solve discovery. A tenant-wide list could reveal another user’s sessions. I would trace the existing read and the closest list, checking the predicates before accepting either as an owner.

**1.** Find session-discovery candidates and neighboring protocol tools from the request alone.

~~~powershell
Find-Paths 'mcp.*\.rs$|session.*\.rs$|mcp.*\.md$' | Where-Object { $_ -match 'server/' } | Select-Object -First 70
~~~

**Found.** The inventory yielded protocol handling, tool definitions, session API, tests and design rules. The next question was whether any list query already enforces caller ownership.

**2.** Find how tools are listed/called and what session operations the catalog already offers.

~~~powershell
Find-Lines 'tools/list|tools/call' 'server/src/munarium-server/src/mcp.rs' | Select-Object -First 12
Find-Lines 'name: "(open_session|ask_session|get_session|close_session|list_runbooks|list_sessions)"' 'mcp_tools.rs'
Find-Lines 'op_sessions_page' '*.rs'
~~~

**Found.** The MCP protocol owner delegates to mcp_tools. The catalog offers open, ask, get and close, plus other list operations. A separate session page is used by the dashboard. That is a candidate implementation to inspect.

**3.** Read the existing list predicate, count and cursor before calling it reusable.

~~~powershell
Read-Lines 'sessions_api.rs' 1987 2043
Find-Lines '/v1/sessions' 'server/src/munarium-server/src/rest.rs' | Select-Object -First 12
~~~

**Found.** The page accepts tenant, state, cursor and limit. Its SQL rows and count are tenant-filtered; uid is selected as output, not constrained to the caller. The native get-by-ID route addresses a different need. Filtering returned rows after a tenant-wide count would still expose information.

**4.** Read how an allowed list is authorized, advertised and paged.

~~~powershell
Read-Lines 'mcp_tools.rs' 36 79 397 421
Find-Lines 'SessionProjection|caller.access.uid|encode_cursor|next_cursor|Handler::ListRunbooks' 'mcp_tools.rs' | Select-Object -Last 23
~~~

**Found.** The catalog binds a scope, handler and annotations. List metadata is fixed and filtered by caller scope. Dedicated readers can return bounded projections/cursors. Another user’s rows, totals and cursor state all need to be excluded at the owning query.

**5.** Check the normative rules and nearby conformance tests before proposing placement.

~~~powershell
Read-Lines 'server/docs/design/mcp-analysis.md' 18 31 77 91 163 168
Find-Lines 'admission|scope|catalog|tools/list|tenant|uid|projection' 'mcp_tests.rs' | Select-Object -First 42
~~~

**Found.** The rules require native compatibility, verified caller authority, an explicit allowlist, durable attributed admission before dispatch, preserved refusal/uncertainty, operator opt-in and fixed tenant-independent metadata. Existing tests give patterns for refusal, catalog and authority coverage.

**Handoff.** I would propose a read-only bounded list in the existing MCP catalog backed by an owner-filtered session query. Bind tenant and caller identity before counting and paging. Return enough stable information to select a session and use get_session. Check two UIDs in one tenant, two tenants, pagination and refusals without revealing totals or entries belonging to others. Apply current clearance, compartments and runbook authority to every returned row, count and continuation. Include a renewed token with the same UID but narrower authority.

## 2. What can support each step today

Read each row alongside the numbered step in its section 1 walkthrough. This is a capability crosswalk. An entry tells us whether a human-led investigation has an equivalent operation, needs ordinary source access, or depends on judgment or observation. It makes no scheduling decisions for the product.

| Operation in the table | Evidence it can establish | Boundary that matters here |
| --- | --- | --- |
| **Files** (`list_files`, `file_info`) | Indexed paths and available identity, type, retention or coverage metadata. Page lists to completion on the same selection. | The tracked export can contain paths outside this inventory. Missing paths require a source inventory or an explicit scope limit. |
| **Search** | Bounded lexical, vector or hybrid candidates under the actual served mode. Explicit lexical search uses stable-ID ordering. Exact product names and natural-language behaviors are useful starting queries. | Lexical terms, punctuation and tokenization differ from a byte-literal search. Top results and empty results have no exhaustive meaning. |
| **Names** (`symbols`, `references`, `imports`, `code_facts`, `relationships`) | Written declarations, case-sensitive whole-identifier occurrences, imports, syntax candidates and available producer assertions. A caller candidate gives a place to read. | A string match can be a comment. Vue embedded code, SQL, message keys, configuration and DI registrations can join two real owners without a resolved graph edge. Check each end in source. |
| **Docs** (`document_facts`, `outline`) | Extracted headings, rules and document anchors. A section read can establish the text of a rule and its exception. | A match can be historical, superseded, an example, or outside the governing product. Authority requires checking scope, version and executable consumers. |
| **Read** (`read_file`, `context`) | Bounded retained source at an inclusive line range or around an anchor. Preserve original document locations for supported formats. | Read through the closing branch, assertion or section. Check digest, publication, truncation and continuation. Saved-source verification compares selected bytes with the indexed source hash. It does not return newly changed content. |
| **Raw** (ordinary harness tools) | A scoped path inventory, literal scan, direct file read, or local format-aware parse on an independently authorized export or checkout. Git can provide an identified diff when supplied or authorized. | Outside the current AIRK gathering contract. Declare which snapshot and files were available. Credentials, unrelated checkouts and mutable user state are outside these walks. |
| **Reason / observe** (agent or separate test harness) | Classify a caller, reconcile two claims, derive an independent expectation, or verify behavior from a matching observed run. | These are different kinds of evidence. Reading the source of a retry test cannot establish how often a real request ran. |

The present Answer loop starts with searches and can follow anchors with a subset of the native readers. Its typed operations cover Search, Symbols, Definitions, References, Imports, Outline, Read File, Document Sections and Context. The wider public catalog also offers Code Facts, Document Facts and Relationships. Public `parse` prepares or executes a bounded composition of AIRK reads. It can bind, for example, a discovered hit to a range read. It is neither a general file-format parser nor permission to open a live path. A complete discovery cursor covers that indexed query at its publication. It does not account for excluded or unretained source. [Loop read types](../../crates/airk-core/src/arbiter/answer_loop.rs) · [Retrieval dispatch](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Public tool contracts](../../crates/airk-mcp/src/lib.rs) · [Discovery cursors](../discovery.md) · [Code facts and relationships](../code-facts-and-relationships.md) · [Saved-source verification](../source-verification.md)

The gap references point to section 3. **G1** concerns inventory and exact lookup, **G2** evidence across boundaries, **G3** structured or unavailable source, **G4** what can be claimed from the evidence, and **G5** what the receiving agent can use.

### Bookbuilder capability map

| Walkthrough step | Available indexed or retained operation | Remaining work or limit |
| --- | --- | --- |
| [C1](#bookbuilder-c1) · 1 | Search named providers, then Files for settings, provider and model paths. | Raw literal/filename results expose sibling implementations across languages. Check which hits actually ship (G1). |
| [C1](#bookbuilder-c1) · 2 | Names for the enum/factory plus Read of the Settings component and DI registration. | Vue registration needs a string or component read. A C# provider edge alone misses the UI (G2). |
| [C1](#bookbuilder-c1) · 3 | Search the exact bridge actions and Read the relevant library and handler branches. | Match temporary and saved keys to their respective paths. Raw scoped literal lookup helps if the punctuation in the action is lost (G1, G2). |
| [C1](#bookbuilder-c1) · 4 | Files and Read for model-list handler, capabilities and UI mapping. | Resolve built-in versus user-selected models by reading the join and its callers (G2). |
| [C1](#bookbuilder-c1) · 5 | Names and Read for the adapter, host registration and test doubles. | A read gives the intended wire shape. Mock dispatch and an observed vendor exchange are different proof levels (G4). |
| [C1](#bookbuilder-c1) · 6 | Corpus search can locate a checked-in vendor specification if the granted collection contains one. | This walk opened the official vendor documentation separately. Record its origin/date or request the specification; an account’s model entitlements remain unknown (G3, G4). |
| [C2](#bookbuilder-c2) · 1 | Search the symptom or UI labels and list plausible onboarding/connection files. | Raw literal search is useful when the exception text occurs only in the framework (G1). |
| [C2](#bookbuilder-c2) · 2 | Read the view around testConnection and Search the exact action string. | Bridge routing is string-based. Verify the payload field and its value at the receiving end (G2). |
| [C2](#bookbuilder-c2) · 3 | Read the enclosing branch and the entire default-model switch. | Read beyond the thrown default to compare the temporary and saved paths. The order selects the failure before HTTP (G2, G5). |
| [C2](#bookbuilder-c2) · 4 | Names plus Read for the factory, DI registration and Gemini adapter. | Use those independent passages to rule out two alternatives; the existence of an HTTP adapter alone proves neither invocation nor failure (G4). |
| [C2](#bookbuilder-c2) · 5 | Search test/model names, read setup and assertion and compare against the failing input. | The missing fixture is a reasoned test gap. Demonstrating the exception and its correction requires an executed bridge test (G4). |
| [C3](#bookbuilder-c3) · 1 | Files, Search and References for decryption, provider, retry and run terms. | For “every place”, page eligible data and compare with the authorized source roster. Ranked hits omit readers (G1). |
| [C3](#bookbuilder-c3) · 2 | References and Code Facts list candidates; Read distinguishes senders from storage/UI. | A direct scoped scan can include wrappers, tests and admin calls. Each match needs classification and a stated search boundary (G1, G2). |
| [C3](#bookbuilder-c3) · 3 | Read the retry callback, lookup and secondary transport; inspect available call edges. | Trace the credential across callback boundaries and compare siblings. An edge cannot prove the number of decryptions (G2, G4). |
| [C3](#bookbuilder-c3) · 4 | Read execution/resume, per-stage construction and disposal together. | A lifetime/parallelism judgment needs multiple cited sources. The harness can later count calls under concurrent ownership (G2, G4). |
| [C3](#bookbuilder-c3) · 5 | Read request DTO, embedding and administrative paths, plus masking and logging. | Checking all relevant read sites and preserving custody requires a classified inventory. Keep sensitive values outside the evidence (G1, G4). |
| [W1](#bookbuilder-w1) · 1 | Search the author’s wording, retrieve UI/help candidates and list matching files. | If phrasing changes, search shorter literals or the action observed on a control (G1). |
| [W1](#bookbuilder-w1) · 2 | Read the import and Generate views and help text at their source locations. | Verify both launch points. Search results favoring one screen cannot establish the other (G2). |
| [W1](#bookbuilder-w1) · 3 | Read the checkbox, prerequisite, message and follow-up handler ranges. | Keep cost wording and the deferral option together so the writer can qualify the advice (G5). |
| [W1](#bookbuilder-w1) · 4 | Names and Read for the preparation service, eligibility and persisted outputs. | Compare what is created with what the author can see. Raw reads help if an exported artifact lacks retained text (G3, G4). |
| [W1](#bookbuilder-w1) · 5 | Read readiness controls, the wizard and help registration. | Join the displayed state to the next enabled action. The author needs a short journey with the cited UI states in order (G2, G5). |
| [W2](#bookbuilder-w2) · 1 | Files and Search for template stores, prompt resolution, hydration and architecture documents. | Many docs share a name. Carry path/product and revision into every read (G1). |
| [W2](#bookbuilder-w2) · 2 | Read registration and fallback code, aided by Symbols/Outline. | A default provider is one layer. Identify who calls it before making a precedence claim (G2). |
| [W2](#bookbuilder-w2) · 3 | Read both resolver bodies and the winning/absent override branches. | Record which input makes each precedence result true. A flat list of templates loses the decision (G2, G5). |
| [W2](#bookbuilder-w2) · 4 | Read hydrator and real stage caller; Names/References help connect them. | Follow async and sync separately, and show what happens after an unresolved token reaches the caller (G2). |
| [W2](#bookbuilder-w2) · 5 | Docs/Outline and Read for the exact architecture claims. | Compare the same cases in the document and code. Deliver both ranges and identify the contradiction (G4, G5). |
| [T1](#bookbuilder-t1) · 1 | Search the output terminology, then list plausible profiles, formatter and tests. | Find the runtime export and the export tests; unrelated sample DOCX files are weak leads (G1). |
| [T1](#bookbuilder-t1) · 2 | Read built-in profiles, inspect the tests’ entry points. | Inventory all shipped profile values and preserve duplicates as distinct profile mappings. A top-k list is insufficient (G1). |
| [T1](#bookbuilder-t1) · 3 | Read chapter loop, body writer and existing paragraph assertions. | The retained C# establishes intended output. Checking one generated document would require an authorized fixture and local package/XML parser (G3, G4). |
| [T1](#bookbuilder-t1) · 4 | Read the formatter branches, numeric boundary and casing helper. | The agent must derive literal expected values independent of the helper, including whitespace/case and paragraph boundaries (G4). |
| [T1](#bookbuilder-t1) · 5 | Read the full profile declarations and compare to the formatter’s cases. | Reconcile seven profiles to six values. Present unselected formatter branches and TOC text outside the body test denominator (G4, G5). |
| [T2](#bookbuilder-t2) · 1 | Files can enumerate indexed scripts, test suites and contributor rules. | A raw tracked inventory including hidden paths is the comparator when asking about CI (G1). |
| [T2](#bookbuilder-t2) · 2 | Docs and Read for governing rules, script switches and smoke roster. | Compare required gate to default script behavior. Both sources belong beside the command (G4, G5). |
| [T2](#bookbuilder-t2) · 3 | Names and Read identify test bodies, callers and the frontend store suite. | Classify by asserted behavior, even if several test classes contain the same method under test (G2). |
| [T2](#bookbuilder-t2) · 4 | Read project filters, credential skip predicates and offline/live fixtures. | An Integration category does not establish a live call. Never fetch a real credential to determine a fixture’s prerequisites (G4). |
| [T2](#bookbuilder-t2) · 5 | Read script call sites and inspect indexed workflow paths. | Finish the source inventory before stating absence in that snapshot. External CI settings and run results need separate evidence (G1, G4). |
| [A1](#bookbuilder-a1) · 1 | Files and Search for the Generate screen, pipeline store and bridge. | Source reads distinguish the live app from a similarly named store or mock (G1). |
| [A1](#bookbuilder-a1) · 2 | Read event handler, review action, outbound message and progress callback. | Vue/TS source supplies the bridge key and payload for the next lookup (G2). |
| [A1](#bookbuilder-a1) · 3 | Search that literal on each side; Read message envelope, router and registration. | Native/C# call candidates cannot bridge Vue’s string-based dispatch. Check both ends against the same action (G2). |
| [A1](#bookbuilder-a1) · 4 | Read actual dispatch, selected stage and example provider HTTP operation. | A candidate factory edge needs case/registration evidence. Distinguish the example provider from other possible branches (G2, G4). |
| [A1](#bookbuilder-a1) · 5 | Read persistence, callback delivery and final UI update. | Two paths complete the request: the durable write and correlated return. Deliver evidence for both (G2, G5). |
| [A2](#bookbuilder-a2) · 1 | Files/Search find OS strings in project, policies, docs and runtime. | Use exact path filters to separate the shipping application from other products in the tree (G1). |
| [A2](#bookbuilder-a2) · 2 | Docs and Read identify policy, branch guards and registrations. | A future distribution policy and wired runtime have different authorities (G4). |
| [A2](#bookbuilder-a2) · 3 | Names/References and Read locate concrete Mac providers and registration. | Resolve platform branches before deciding whether a class is reachable. Source cannot qualify its native behavior (G2, G4). |
| [A2](#bookbuilder-a2) · 4 | Files and Read for MSBuild RIDs, publish scripts, WiX inputs and packaging manifests. | Raw project XML, scripts and a scoped file inventory support the packaging assessment. Absence of a single file extension is inconclusive (G1, G3). |
| [A2](#bookbuilder-a2) · 5 | Read platform docs and test assertions side by side. | Separate stale interface names from an accurate Windows-only distribution statement. Hardware qualification belongs to a separate run (G4, G5). |
| [A3](#bookbuilder-a3) · 1 | Files/Search for the proposed product and maintained authoring roots. | Raw path inventory supplies a decisive counterexample to the premise that the product is absent (G1). |
| [A3](#bookbuilder-a3) · 2 | Read the authoring guide and use File Info on discovered manifest paths. | A matching slug is a lead. Establish the exact product identity before inferring a count change (G2). |
| [A3](#bookbuilder-a3) · 3 | Read retained JSON and guide if present; Outline can locate prose rules. | A plain JSON parse identifies contract keys and nesting consistently. A file absent from retention requires authorized bytes (G3). |
| [A3](#bookbuilder-a3) · 4 | Read compiler, seed script, Host project and installer inputs. | Follow the same .bbpipe and catalog identity across script/XML boundaries. Files with similar names are insufficient (G2, G3). |
| [A3](#bookbuilder-a3) · 5 | Read validator and the actual hard-coded and derived assertions. | Count and compare the two product-identity scenarios. Keep the unresolved product choice visible (G4, G5). |

### Munarium capability map

| Walkthrough step | Available indexed or retained operation | Remaining work or limit |
| --- | --- | --- |
| [C1](#munarium-c1) · 1 | Search the supplied route and budget name; Files for vocabulary, store and protocol. | Search in more than one vocabulary if splitting the environment identifier obscures exact spelling (G1). |
| [C1](#munarium-c1) · 2 | Read the consumer and env/validation registry. | Associate the budget with this particular generation call and keep generic completion as a separate consumer (G2). |
| [C1](#munarium-c1) · 3 | Names and Read for shared DTO, REST operation and generated RPC binding. | Check the generated mapping and actual default. A same-named client field does not establish wire compatibility (G2). |
| [C1](#munarium-c1) · 4 | Read the SQL migration, store and decoder/fallback together. | Raw structured inspection of an authorized old fixture can illustrate the missing field. Preservation of a tenant through upgrade requires an observed compatibility test (G3, G4). |
| [C1](#munarium-c1) · 5 | Files, References and Read for each SDK and existing refusal/replace tests. | Check all four clients and distinguish old storage from a new, invalid replacement request. Show that distinction explicitly (G1, G5). |
| [C2](#munarium-c2) · 1 | Search both the configuration literal and the error slug, then list matching source. | Literal matching can recover hyphen/underscore variants missed in one query (G1). |
| [C2](#munarium-c2) · 2 | Read configuration and typed refusal definitions. | The error declaration proves its shape. It does not show when the active turn path uses it (G4). |
| [C2](#munarium-c2) · 3 | Read the janitor’s zero, scheduling, clock and timestamp branches. | Carry all timing terms together. One timeout constant alone would misdescribe the race (G5). |
| [C2](#munarium-c2) · 4 | Read the request and activity writes, using References to locate callers. | Trace the order of ownership, admission and refresh. Concurrency semantics need transaction/clock evidence as well (G2, G4). |
| [C2](#munarium-c2) · 5 | Read transport delegates and test assertions for idle versus closed state. | A typed-error assertion for an already-closed session is a control. A deterministic expired-but-still-open case needs an executing test (G4). |
| [C3](#munarium-c3) · 1 | Search count, operation and byte terms, then classify paths with Files. | Raw unit-aware literal scan helps separate limits from port numbers, status codes and unrelated counts (G1). |
| [C3](#munarium-c3) · 2 | Read both guards and independent body/manifest limits. | Keep precise units, comparator and owner in each citation (G5). |
| [C3](#munarium-c3) · 3 | Read generated server dispatch and SDK declarations/implementations. | Verify each operation/transport independently. The public client may deliberately omit an operation present on the native server (G2, G4). |
| [C3](#munarium-c3) · 4 | References and Read for four SDK constants and local validators. | Identifier spelling differs by language. A source inventory, multiple spellings and actual usages make the client matrix defensible (G1, G2). |
| [C3](#munarium-c3) · 5 | Search boundary fixtures, docs and OpenAPI, then Read the relevant ranges. | Large structured specs can need scoped local parsing. Tests for 1,001 small files and oversize bytes establish different claims (G3, G4). |
| [W1](#munarium-w1) · 1 | Search lifecycle synonyms and route literals; Docs for the guide. | Follow the vocabulary native code uses after discovery to avoid confusing other forms of retirement (G1). |
| [W1](#munarium-w1) · 2 | Read request, confirm, mismatched ID and expiry branches. | Keep version, ID, expiry and consequence together so a caller can perform the flow (G5). |
| [W1](#munarium-w1) · 3 | Read selection, listing, changed columns and reapply guards. | A status transition alone cannot establish retention or reactivation. Check affected storage and the guard (G2, G4). |
| [W1](#munarium-w1) · 4 | Read the stored session ref and the lookup on the next turn. | Join the already-open session to the same version resolver. Two individually relevant excerpts can otherwise suggest the opposite conclusion (G2, G5). |
| [W1](#munarium-w1) · 5 | Names/References and Read for each SDK’s public methods. | Camel case, snake case and Async variants need independent lookups. A language-level example must forward the ID the previous call returned (G1, G2). |
| [W2](#munarium-w2) · 1 | Search command and error names, then list service/store candidates. | Exclude client stubs when choosing the native entry point (G1). |
| [W2](#munarium-w2) · 2 | Read REST and RPC handlers and their common service call. | Match arguments, optional head and authentication across both transports (G2). |
| [W2](#munarium-w2) · 3 | Read gate input, policy, decisions and projection status. | A gate finding and the stored claim status are distinct facts. Read the mapping between them (G2, G5). |
| [W2](#munarium-w2) · 4 | Read transaction, conflict handling, retry ceiling and transport mappings. | Determine who rechecks the head and whether the caller pinned it. Race behavior ultimately needs a competing-writers test (G2, G4). |
| [W2](#munarium-w2) · 5 | Read direct and facts-view projections with each filter condition. | Use the same disputed example for both readers. Pin as-of/supersession qualifiers to the pertinent evidence (G5). |
| [T1](#munarium-t1) · 1 | Search test names, mock transport and idempotency vocabulary. | Finding a helper does not establish an outbound request. Select the transport-level fixture (G4). |
| [T1](#munarium-t1) · 2 | Files/Names and Read locate both Python loops and relevant tests. | A list of similarly named tests does not distinguish sync, async and pure helper coverage (G1, G4). |
| [T1](#munarium-t1) · 3 | Read the scripted responder, request recording and assertions. | Classify each observed assertion by send count, exact key and failure type (G4). |
| [T1](#munarium-t1) · 4 | Read both loops and the key-generation site before their retries. | Do not infer async behavior or testing from a sync implementation. Connect and post-connect errors are separate cases (G2, G4). |
| [T1](#munarium-t1) · 5 | Read real operation specs and pytest configuration. | Construct an independent request matrix. Actual repeated headers, attempt counts and passes need a later execution receipt (G4, G5). |
| [T2](#munarium-t2) · 1 | Files and Docs for contributor rules, tests, validation and workflows. | The source inventory accounts for hidden workflow definitions and working directories (G1). |
| [T2](#munarium-t2) · 2 | Read the guide’s command definitions and their prerequisites. | A command must travel with cwd, environment, tier and required local services (G5). |
| [T2](#munarium-t2) · 3 | Read the checked-in workflow, including OpenAPI generation and comparison. | Raw YAML inspection can recover job defaults, steps and shell context lost when a snippet is clipped (G3). |
| [T2](#munarium-t2) · 4 | Read gate implementations and SQLx migration registration. | Distinguish static policy from executing an actual populated upgrade (G4). |
| [T2](#munarium-t2) · 5 | Read local tiers and CI comparison logic; Changes may describe indexed changes at an available revision. | The prompt supplies no patch. A named Git base/head or authorized diff and a compatible database fixture are needed for route-specific proof; provide the generic commands meanwhile (G3, G4). |
| [A1](#munarium-a1) · 1 | Search exact route and Files for the serving component. | The occurrence can be in generated clients and documentation. Read the route registration before choosing an owner (G1). |
| [A1](#munarium-a1) · 2 | Names and Read connect listener, router and handler. | Read the binding that relates a route string to a function (G2). |
| [A1](#munarium-a1) · 3 | Read collection resolution, access, tenant and query preparation. | Ground the single-collection branch and its refusal path. Keep ID/name fallback conditions (G2). |
| [A1](#munarium-a1) · 4 | Follow supported Names edges and Read the PostgreSQL implementation and ranking. | SQL and dynamic mode dispatch require source verification. Show which leg can still run without optional inference (G2, G3). |
| [A1](#munarium-a1) · 5 | Read provenance construction and REST disclosure separately. | Name the source of each returned field and the condition for optional detail. A rank snapshot would need a live query and data (G4, G5). |
| [A2](#munarium-a2) · 1 | Search the two extractor modes; Docs, Files and Read for configuration/owner. | Historical README values and active code can both rank well. Keep revision and authority (G1, G4). |
| [A2](#munarium-a2) · 2 | Read default, override and the active budget profile. | Show units, OS applicability and which settings affect a build separately (G5). |
| [A2](#munarium-a2) · 3 | Read identity construction/guide, build and activation. | Raw inspection of authorized manifests helps check an existing publication. Text describing activation does not activate or measure it (G3, G4). |
| [A2](#munarium-a2) · 4 | Read deferred/failure classification and paid eligibility branches. | Tie each outcome to the inputs and policy that actually allow spending. Do not infer paid fallback from any generic error (G2, G4). |
| [A2](#munarium-a2) · 5 | Read worker start, platform guards, profile and operator docs. | A platform-specific test or benchmark needs that host. Preserve established limits and flag old wording beside current source (G4, G5). |
| [A3](#munarium-a3) · 1 | Files and Search for target repo’s MCP/session APIs and governance. | The target repository’s protocol source is data for this investigation. Reading it is independent of calling AIRK's own MCP tools (G1). |
| [A3](#munarium-a3) · 2 | Read catalog, protocol dispatch and session page call sites. | Check whether a list-by-owner operation exists or there is only a by-ID read and an administrative list (G2). |
| [A3](#munarium-a3) · 3 | Read SQL rows, count, argument binds, cursor and native routes. | A local SQL read/parse can expose predicates explicitly. A tenant-wide query cannot establish uid isolation (G2, G3). |
| [A3](#munarium-a3) · 4 | Read adjacent bound projections, metadata, paging and scope filters. | Check that rows, totals and cursor use the same authority; the receiving agent must see this difference (G2, G5). |
| [A3](#munarium-a3) · 5 | Docs and Read for normative rules plus test bodies. | Distinguish an existing admission rule from a proposed new read. An observed two-user refusal and paging test would qualify the implementation (G4). |

## 3. How to close the gaps

**Start with a comparison that can change the decision.** The Windows treatment put every labeled passage in front of Sol and produced two correct briefs. Rebuilding the index first would leave the reason for that result unresolved. I would first use the ordinary-tool walks as a reference for *what an investigator does*: form a question, seek a discriminator, read a complete branch and carry its consequence forward. The walks are development examples. Their discovered names, paths, judgments and expected answers must stay out of production recipes, prompts and the evaluated source collections.

| Gap | Smallest change worth testing | What would justify keeping it |
| --- | --- | --- |
| **G5. The agent can receive a fact and still lose it.** A long reply can separate the condition, the decisive branch and the result. | Start with a presentation-only replay of retained development evidence. Give the agent a short index of the user’s actual questions. Under each, put the decisive excerpt with its path and range, the qualifying exception or counterevidence, and any unresolved point. Preserve access to the original evidence. Keep unrelated matches and accounting as expandable detail. | Hold the task, actor, evidence available, output budget and grading fixed. Check which needed facts appeared in the delivered view, which the agent used correctly, which it contradicted, and whether the brief finished correctly. A larger packet or a nicer-looking reply earns no quality credit on its own. |
| **G2. A lead often crosses a boundary that search or syntax cannot resolve.** Examples include a Vue action string and a C# registration, a session ref and a SQL predicate, or a stage and its writer. | Offer the existing native readers to the loop where their evidence applies. From the current excerpt, carry one observed action, option, field, route or symbol into a lookup at the other end. Require a read of the actual receiver and branch. Keep alternative explanations until a source distinguishes them. | On examples with two plausible receivers, show both ends, the matching value, and what rules the other out. Check direction, common scope/revision and caller/consumer agreement. A written occurrence remains a candidate until verified. |
| **G1. Ranked retrieval cannot enumerate a source tree or guarantee a literal match.** This affects “every provider”, “all profiles” and “anything automatic”. | Start with Files, targeted References and paginated discovery over the authorized publication. Add a bounded literal/identifier scan over eligible **retained** text only where those operations leave a demonstrated gap. Record roots, exclusions, scanned versus eligible files/bytes, match boundaries, cutoff and continuation. Give punctuation and path literals their own fixtures. | Compare with ordinary literal search on the same admitted text for exact matches, including false friends, aliases and no-match cases. Separately compare publication inventory with the authorized source inventory. Surface unretained or unreadable paths explicitly and decline an exhaustive claim if the denominator is unknown. |
| **G3. Retained text, source syntax and live state answer different questions.** Structured JSON/XML/YAML, generated DOCX parts, an unprovided diff or an external spec may be decisive. | First try the canonical retained file/section read. Where structure matters, add a bounded local parser for the *specific* admitted format with original location and source digest. Prefer existing ingestion/reader owners. For source outside retention, accept an explicitly supplied artifact or propose a separate read-only source-authority contract binding the granted root, file identity, revision and exclusions. A client may supply a pinned diff or vendor spec as a separately attributed input. | Compare fields, sections or paragraphs to the same original bytes, including malformed, stale, excluded and too-large cases. A DOCX body test must distinguish a body heading from identical text in a TOC. A changed file needs a new dated source identity. External documents and current saved files never silently replace the frozen corpus. |
| **G4. A source read cannot establish what actually ran.** Repository policy, declared tests, inferred behavior, observed results and unknowns need different wording. | Preserve the type of evidence for every claim. Pair source assertions with their preconditions, direction, counterexample and scope. A command should include cwd, flags and prerequisites. Accept an existing matching run receipt when authorized. Otherwise return the exact discriminating command, fixture or missing input for the calling agent. | Ask a reviewer to distinguish a skipped test from a pass, an indexed no-match from an absent file, an offline mock from paid live behavior, and a state-only check from a reproduced race. Any stronger claim must point to evidence that supports it. |

**A diagnostic sequence for review.** These are proposed comparisons. The paused completion plan has no new execution order.

1. **Check the agent/harness first.** In a fresh ordinary-tools reference arm, give the actor the prompt, the same pinned source and ordinary path/search/range/parser tools. Hide this document, the reference notes and previous answers. Capture the queries, files opened, decisive branches, external calls, final brief and unreached requirements. Compare its brief with one produced by the same actor from the recorded complete-evidence delivery. A weak ordinary-tools result points to the actor, brief instructions, task ambiguity or grading alignment as possible contributors. It would not by itself identify which one.
2. **Isolate the delivery effect.** Compare the archived delivery with a condition-grouped presentation of the same available passages, using a fresh actor per case/arm. Hold task, model, effort, citation access, turn/byte budgets and grading constant. Do not supply reference-note prose as headings or summaries. If a separate minimal-passage diagnostic uses evaluator-selected spans, label it as an oracle condition and keep its selector outside product evaluation. Report changes in brief correctness and whether particular omissions moved.
3. **Measure retrieval where the actor can use the result.** Once a presentation is selected, compare task-derived retrieval using today’s primitives with the nominated changes from G1–G3. Attribute each miss to the earliest observed failure: no eligible source, never nominated, failed to follow the lead, read cut short, source present but omitted from delivery, or delivered but absent/incorrect in the brief. Record unresolved causes as unknown. For ambiguous cases retain both explanations and the read that would separate them.
4. **Keep the decision tied to the user’s task.** Measure correct finishes per repository and task kind, then turns to a correct finish against the stronger of Search and Parse. Report bytes, external reads, elapsed time, citations a reviewer can reopen, unsupported claims and regressions alongside them. Keep the historical ceiling threshold (12/20 and at least five per repository) distinct from the plan’s separate substantial-win rule. Inspect discordant cases before proposing a new change. Preserve the existing sealed-set custody and blind judges. [Evaluation rubric](answer-evaluation/rubrics.md) · [Windows comparison and acceptance](completion-plan.md#3-stage-l7-with-a-windows-first-review)

**Order of investment.** G5 and the ordinary-tools reference arm can discriminate between presentation and agent-use problems with existing evidence and readers. Next, try G2 on observed broken joins and G1 on observed literal/inventory misses. Fund a G3 adapter or new source authority only when the pinned comparison identifies a need it cannot satisfy. Keep G4 in every comparison. Weight changes or a broader index would need evidence that the failure still occurs before the deciding passage reaches the reader. [Native gathering owner](../../crates/airk-kernel/src/arbiter/CONTRACT.md) · [Existing read and delivery budgets](completion-plan.md#read-and-delivery-budgets)

## 4. How one Gather Context request could work

The calling agent supplies the task and an authorized source selection. The service returns evidence the agent can use to act. The proposed flow builds on the existing service-owned Answer job, kernel reads, split/plan/retrieve/follow-up stages and shared budgets. The design below would change which question drives a read and how its result reaches the caller. Public naming, new read operations and response-shape changes would need their normal contract decisions. [Answer job ownership](../../crates/airk-service/src/service_runtime/answer_jobs/CONTRACT.md) · [Kernel contract](../../crates/airk-kernel/src/arbiter/CONTRACT.md) · [Current retrieval recipes](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs)

| When | Service decision and action | What the caller can rely on |
| --- | --- | --- |
| **Receive the task** | Pin index, collection/root, publication and request budgets. Identify the user’s deliverable and write a few concrete questions from the prompt. For a diagnosis, include at least one alternative explanation if the symptom admits one. For “all”, identify the population before searching. | Each question has a finish condition. The caller can see the selected scope and whether a referenced patch, artifact or external input is available. |
| **Find the first owner** | Use an exact path/known symbol directly. Otherwise search a short distinctive phrase or route, and separately search the user’s behavioral language. List relevant paths when names matter. Compare candidate type, owning project and configuration before opening a small number of ranges. | A lead carries its source and why it might answer the question. A hit in old documentation or a generated client stays classified as such. |
| **Read and follow the evidence** | Expand a location to the whole branch, section, assertion or data object that makes the decision. Extract an observed join value and look up its consumer or producer. Use Names/Docs where supported, a scoped retained literal scan when available, and a format-aware reader only for an authorized source. Re-evaluate the question after each read. | A chain contains the cited source on both sides and any competing branch. A missing permission, clipped excerpt, ambiguous consumer or unsupported format is visible. |
| **Spend the next read** | Deduplicate by scope, publication, source digest and range. Prioritize a read that separates alternatives or completes a required output. Share a useful read across questions. Focus may rank candidates and Arbiter may score admissible next actions; fixed rules also work when either is disabled. Keep source authority, admission and budget decisions in the service. | Models cannot introduce a destination, scope or fact absent from the evidence. The caller can tell which question was left open by a byte, call, round or time limit. |
| **Decide whether to stop** | For each question record supported, contradicted or unresolved, with the needed evidence and any exception. Stop when the requested handoff is supported, a remaining question needs input/runtime observation, no admissible novel read remains, or a budget/deadline/cancellation ends the job. | A partial result includes what was learned and the smallest useful continuation, such as the missing diff or unvisited source range. An empty search never becomes a fabricated negative conclusion. |
| **Deliver for use** | Lead with the user’s decision or next action, then give a small set of task-organized claims paired with exact excerpts, paths and original coordinates. Include disagreements, scope, material omissions and an expandable read trail. Reserve space for each open question before adding nearby hits. | The agent can open an edit site, write an author-facing instruction, choose a regression, or draw a verified edge. Preserve provenance for delivered evidence and disclose truncation. A continuation remains bound to the same publication and authority, and can expire or be refused. |

**What this would look like on Bookbuilder C2.** From the prompt alone the open question is whether the validation fails locally or a request reaches Gemini. “Test connection” leads to the onboarding view. Reading the handler yields the action `ai.testConnection` and a temporary key. The corresponding receiver shows that a temporary key selects a default model before calling the provider. Expanding that exact switch exposes the missing Gemini case. A factory read checks the competing registration explanation. The response could then lead with three reopenable facts from Bookbuilder `238b22a7`:

| What the agent needs | Source-backed handoff |
| --- | --- |
| Which user action reaches the defect? | `pro/src/WritingForgePro-ui/src/views/OnboardingView.vue:102–124` shows the Test connection handler supplying the temporary credential. |
| Why can a valid key fail, and where should a change be considered? | `pro/src/WritingForgePro.Bridge/Handlers/TestConnectionHandler.cs:35–104,120–131` shows the ephemeral/saved paths, the point of default-model selection, and the switch fallback. The default model is selected before provider dispatch. |
| What would discriminate the fix? | `pro/src/WritingForgePro.Services/AI/AIProviderFactory.cs:30–49` recognizes Gemini. Exercise the bridge with Gemini, a dummy temporary key and no saved configuration; capture whether the fake provider receives the selected model. Use a working provider and the saved-key path as controls. The source walk did not execute this test. |

A different prompt produces a different stopping condition. For Munarium A3, finding the page query leaves the essential question open until both its row predicate and its count have been read. For Bookbuilder T2, a verified command and its prerequisites answer part of the request even if CI outside the repository is unknowable. For Munarium T2, an absent diff should produce a useful generic checklist plus the exact missing input for a route-specific one. A tool that exposes those distinctions gives the agent a place to act or an honest reason to ask for more.
