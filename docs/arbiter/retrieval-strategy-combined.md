# Gather Context · Combined retrieval strategy

**Windows review draft · 26 November 2026**

**The job of `gather_context` is to do the investigation an agent would otherwise have to do across several turns.** From the task alone, the agent picks something it can look up, finds the source responsible for a behavior, reads enough to understand it, and lets that reading determine its next move. It stops when another agent has enough cited context to do the requested work, or can say exactly what is missing. The name used in this design is **Gather Context**. The preserved accounts use the earlier name `answer` for the tool in the reviewed implementation. The existing `context` operation is the narrower read around a known location.

A simple example captures the behavior to teach. For a button that fails for one provider, start with the button text. Read the event handler to discover the outbound action and payload. Use the discovered action to find the receiver. Compare the failing branch with one that works. Once the evidence distinguishes a failure before the network from one after it, return the branch, the control and a test that would distinguish them. The reusable lesson is **how to find the receiver and choose the next read**. The provider, path and action spelling are results of that investigation and should never be built into its recipe.

| Read this part | What it gives the reviewer |
| --- | --- |
| [1. How the agents gathered context](#investigations) | Twenty prompts, each followed by the transferable move and both independent walkthroughs in full. |
| [2. What the available operations support](#capabilities) | A common vocabulary, scope of the actual probes, and a step-by-step crosswalk for both accounts. |
| [3. What to teach and how to check it](#teaching) | Decision rules with observable triggers, the gaps that need an engineering change, and experiments that separate finding evidence from using it. |
| [4. How one request should run](#request) | A bounded request flow, worked traces, output and stop conditions, and decisions for this review. |
| [Implementation and Testing Strategy](#implementation) | The build order for GPT‑6.2 Sol, source owners, commands, acceptance tests for all 20 tasks and Windows review criteria. |

The source accounts are `retrieval-strategy-openai.md` and `retrieval-strategy-anthropic.md` in this directory. OpenAI recorded 100 grouped source-inspection steps plus one external documentation lookup. Claude recorded 109 turns containing 312 calls. They used different harnesses and counted different units. Neither walkthrough was judged. The separate Windows experiment delivered all 113 required spans in Sol’s first reply and yielded 2 correct briefs out of 20. The 18 missed finishes make evidence delivery and use a separate question to test alongside retrieval. The completion plan remains at the Windows review. [Windows result](../simplify/one-process-ledger.md#26-november-2026--windows-cuda-complete-evidence-review-trigger) · [Completion plan](completion-plan.md)

<a id="investigations"></a>

## 1. How the agents gathered context

Each prompt appears once. The **move to teach** above each pair describes what Gather Context could do on a different repository. Expand either account for its original steps, queries, file ranges, expected and observed results, unsuccessful searches, handoff and stopping point as recorded. A **reading note** qualifies an observation where the two scopes or interpretations differ. The retained first-person passages belong to their named authors. The synthesis uses the two records as design evidence, without merging them into an imagined single run.

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

<details><summary>OpenAI harness, starting inputs and reproducible PowerShell tools</summary>

I treated each quoted instruction as a separate assignment. Bookbuilder is pinned at `238b22a722ceb78447d72e39585fd6a7d4d3dd0c` and Munarium at `9948345d1d148525f644bc4b8bc5e6da9cf73ca0`. The quotations match the 20 hash-checked development inputs. I used ordinary source inspection on those two exports for this section. The one external documentation lookup is identified under Bookbuilder C1. Neither the sealed tasks nor the private reference notes were inputs to these walks. [Development inventory](answer-evaluation/scenarios.md)

In each case **my reading** sets out the user’s decision and my initial uncertainty. Each numbered step gives the question I took to a tool, the reproducible command and the finding that led to the next step. The **handoff** says what I could now do with that evidence. These are executed source-gathering simulations with editorially grouped commands and paraphrased results. The work measured here ends at source inspection. Timing, product changes, builds and tests require separate observations.

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

</details>

<details><summary>Claude (Anthropic) harness, starting inputs and decision loop</summary>

## Method and limits

- **Tasks.** The 20 frozen development instructions: ten for Munarium at
  `9948345d` and ten for Bookbuilder at `238b22a7`. Both checkouts were at
  those commits when this ran. The instruction text is copied verbatim from
  the frozen scenario files. [Scenario inventory](answer-evaluation/scenarios.md)
- **The simulation was executed.** Every step in section 1 is a tool call I
  made on 26 November 2026 against the pinned checkouts with Claude Code's
  own tools: Grep (ripgrep), Glob, Read and read-only Git. "Expect" is what I
  predicted before the call. "Got" is what came back. Failed calls and
  wasted calls are kept in the record.
- **No AIRK tool was used in section 1.** I called AIRK `status` and
  `list_scopes` once to confirm the session binding, as the standing rules
  require. No AIRK search, read or Answer call informed a step.
- **The answer key stayed closed.** I read only the `instruction` field of
  each scenario file. I did not open the reference notes before or during
  the simulation, and I did not open the sealed scenarios at all. The
  traces show what an agent finds from the prompt. They are not graded
  against the notes here.
- **Each task was worked from its own prompt.** A step is justified only by
  the instruction, what the harness loads, and earlier results of the same
  task. Tool calls for different tasks sometimes went out in one message to
  save time. Where that happened, each call is listed under the task whose
  prompt motivated it.
- **No source is quoted.** Paths, line ranges, identifiers and counts from
  the two repositories are named. Their source text is described in my own
  words.
- **Limits.** One agent and one pass per task. Another model, or the same
  model on another day, would choose somewhat different steps. Turn and call
  counts describe this run. They are observations and not a benchmark.
  Section 2 reads AIRK's capabilities from its contracts, tool schemas and
  source, with a small number of live probes on the installed service
  (1.8.206, which has a Bookbuilder index and no Munarium index). It is not
  a measurement of the evaluation candidate.

### 1.1 What the harness gives me before my first call

Claude Code starts a session in the repository's folder and puts four things
in my context before I act:

| Given | Use |
| --- | --- |
| The repository's `CLAUDE.md` | Orientation I did not have to search for. Munarium's names the four top-level components, the coupled CI files, the validation commands per component and the rules for contracts and migrations. Bookbuilder's names the stack, the product invariants, the hard rules for bridge handlers and registries, a "where things live" reference table and the test commands. |
| Working folder, platform, branch, status and recent commits | Tells me which tree I am reading and whether it is clean. |
| My tool list | Grep, Glob, Read, a shell with Git, and subagents for broad sweeps. |
| The instruction | The only task-specific input. |

Both evaluation repositories carry such a file. In the judged evaluation the
agent "receives the instruction and nothing else about the repository", so
this free orientation is a real difference between my unaided sessions and
the evaluated ones. Section 2 records it as a mapped capability with a gap.

### 1.2 The loop I follow

The 20 traces below are instances of one loop.

1. **Take anchors from the prompt.** Literal tokens first: identifiers in
   code font, environment variables, routes, numbers with units, quoted
   error text and interface labels. Then domain nouns. Then the kind of
   deliverable, because it decides what "enough" means.
2. **Take a census before reading.** One count-mode or files-mode Grep per
   literal anchor over the whole tree. One call returns the file map, the
   blast radius and how ambiguous a word is in this repository.
3. **Read the owner.** Whole when it is small, by line range when it is
   large. Module comments usually state the invariant the change must keep.
4. **Follow the edges the owner names.** A constant, function, table,
   route, generator or test name found in the source becomes the next
   exact search. I do not guess vocabulary when the code has given me its
   own.
5. **Enumerate every site.** For a change, every place that enforces,
   declares, tests or documents the thing. This needs an exhaustive literal
   search with a known scope, because a missed site is a defect.
6. **Find the tests and their seam.** Which file, which fixture, how the
   transport or store is stubbed, how tests are selected and run.
7. **Find the governing documents and checks.** Reference docs, CI
   workflows, generators and drift checks that will fail if a coupled file
   is not updated.
8. **Stop when each clause of the instruction maps to a located site.** I
   stop when I can name the edit sites or facts, the invariant, the
   comparable implementation and the test home. What is left is then a
   choice to make, and no longer a fact to find.

Independent calls go out together in one turn. A turn below is one message
of mine with one to five parallel calls.

</details>

### Bookbuilder

Bookbuilder is WritingForge Pro: a .NET desktop application with a Vue
front end. Its code is under `pro/src/`, and paths below are relative to
that folder unless they start with `pro/`. The working tree holds large
untracked logs, build output and `node_modules`. Grep skips what the ignore
file lists. I scoped every search to `pro/` so that tracked fixtures and
bundled payloads elsewhere could not crowd the results.

<a id="bookbuilder-c1"></a>

#### Bookbuilder C1 · A provider that works from Settings through generation

> Add Mistral as another AI provider next to OpenAI, Anthropic, Gemini, DeepSeek, OpenRouter and Ollama. Users should be able to enter and test a Mistral API key in Settings, have it stored the same way as the other keys, pick Mistral models, and run generation through it.

**Move to teach.** Use a named sibling to discover all the places where a new member must be registered. Follow each user journey independently, then check whether edited catalogs are generated. A familiar vendor name in a model list can be a routing alias; follow it to the transport before deciding an integration already exists.

**Reading note.** The catalog generator owns generated native and UI model lists. The additional vendor-documentation read established the public wire contract. Account-specific model availability remained outside both source trees.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** The six sibling names. A new provider has no anchor of its own,
so I search for the rarest sibling: every place that lists the providers
must name DeepSeek.

1. **Turn 1, three calls.**
   - Grep `deepseek`, case-insensitive, in `pro/src`, count mode. *Expect:*
     an enum, a factory, a provider class, settings screens and tests.
     *Got:* 329 hits in 61 files across the Core, Services, Bridge and Host
     projects, the user interface and the tests.
   - Grep `provider` in the harness-named reference
     `pro/docs/ai/where-things-live.md`. *Got:* the provider interface, the
     library service interface and the registration file.
   - Grep `deepseek` in `pro/` outside `src`. *Got:* 130 hits in 19 files,
     among them a model catalog generator script with 19.
2. **Turn 2, four calls.** Read the provider enum
   (`WritingForgePro.Core/Enums/AIProviderType.cs`), the factory
   (`WritingForgePro.Services/AI/AIProviderFactory.cs`), the registration
   (`WritingForgePro.Host/ServiceRegistration/AiProviderExtensions.cs`) and
   the first 120 lines of the closest existing implementation
   (`WritingForgePro.Services/AI/DeepSeekProvider.cs`). *Got:* six numbered
   enum values, a factory that maps the enum to a string identifier and
   throws for an unknown one, one typed HTTP client per provider with its
   base address, and a comparable provider to copy the shape of.
3. **Turn 3, three calls.** Grep the same word with content output in three
   scopes: the Bridge, Core and Data projects, the Services project without
   the provider's own files, and the user interface's settings, utility,
   view, type and store files. *Got:* the full list of enumeration sites.
   Six bridge handlers map between the enum and the identifier. In Services:
   the embedding provider map and class, the model capability registry, the
   model families file, the error parser, the token ratio table, and three
   more identifier maps. In the interface: five hard-coded provider arrays
   in four views and two settings sections, the key-link table, and the
   built-in model module.
4. **Turn 4, four calls.** Read the save handler and the connection-test
   handler, Grep the key and request path inside the comparable provider,
   and read the head of the catalog generator. *Got:* the save handler
   parses the enum by name and its error text lists the six names. The test
   handler holds a per-provider default model. The provider resolves its
   key from the library row and decrypts it. The generator states that one
   specification produces the C# registry block, the interface data module,
   the help tables and a help-order test, so models are added there and not
   by hand.
5. **Turn 5, four calls.** Grep the library save method and the key
   encryption service, read the model-list handler
   (`GetModelsHandler.cs:28-139`), read the provider settings section
   (`SettingsProvidersSection.vue:1-70`), and Grep the interface types for
   a provider-name union. *Got:* the key is encrypted in
   `LibraryService.cs:938` through one service with three platform
   implementations. The provider type is a plain string in the interface
   types, so no union needs widening.
6. **Turn 6, one call.** Read `LibraryService.cs:895-946`. *Got:* storage is
   one row per provider type and has no provider-specific branch, so "stored
   the same way" needs no new code there.

**Context in hand.** The enum, factory and registration. A provider to
model the new one on. Every identifier map and hard-coded list on both
sides. The save, test and model-list handlers. The catalog generator and
the files it writes. The key path. The test files that enumerate providers.
The harness rules: direct HTTP client and no vendor SDK, keys decrypted only
for a call.

**Stop.** Each of the four user-visible capabilities maps to sites. **6
turns, 19 calls.**

</details>

<a id="bookbuilder-c2"></a>

#### Bookbuilder C2 · A valid Gemini key fails on first run

> During first-run onboarding, choosing Google Gemini, pasting a valid key and pressing Test connection fails with a message like "Specified argument was out of the range of valid values". The other providers test fine. Find the cause and fix it.

**Move to teach.** Start from the user-visible control and the reported exception by two independent routes. Follow the payload and evaluate the actual failing input in source order. Read the complete switch and a working sibling; once that identifies the first failing branch, look for a regression that would reach it.

**Reading note.** The temporary key forces default-model selection before HTTP. A saved row without a default model can reach that fallback too. An unmatched error string alone says nothing about whether a framework supplied it. The recorded diagnosis came from reading the throwing branch.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "onboarding", "Test connection", the quoted message, "Gemini".

1. **Turn 1, three calls.**
   - Grep the button text and its camel-case form in interface code.
     *Got:* the onboarding view sends one bridge action with the provider
     type and the pasted key (`views/OnboardingView.vue:102-127`).
   - The quoted message is the framework's default text for an
     out-of-range argument exception, so Grep that exception type in the
     Bridge, Services and Core projects. *Got:* 23 sites. One is in the
     handler for that action:
     `WritingForgePro.Bridge/Handlers/TestConnectionHandler.cs:128`.
   - Grep the error catalog, bug log and gotchas, which the harness file
     says to check first. *Got:* no catalog entry.
2. **Turn 2, five calls.** Read the handler (131 lines) and the onboarding
   view (`:60-139`), Grep the handler and its helper over the tree, Grep
   the Gemini model identifiers in the capability registry, and Grep the
   bug log with only-matching output. *Got:* the cause. The handler's
   default-model lookup has a case for five providers and none for Gemini.
   When a key arrives in the payload, which only onboarding does, the
   handler calls the lookup before its error-catching block, so the
   exception surfaces as the message the author saw. The Settings screen
   takes the saved default model first and reaches the lookup only when
   none is saved. No backend test covers the handler. Valid Gemini model
   identifiers are at `ModelCapability.cs:708-845`.
3. **Turn 3, one call.** Glob regression tests named for a handler. *Got:*
   none.
4. **Turn 4, one call.** Glob the bridge test folder. *Got:* 28 handler
   test files, including one for the AI handlers, which is where a
   symptom-named regression test's neighbors live.

**Context in hand.** The failing line, why only onboarding and only Gemini
reach it, why the message is raw, a list of valid model identifiers, the
missing test and the harness's rule that a behavioral fix needs a
symptom-named regression test and a bug-log entry.

**Stop.** Cause confirmed in source and fix site known. Not reproduced by
running the application. **4 turns, 10 calls.**

</details>

<a id="bookbuilder-c3"></a>

#### Bookbuilder C3 · A key that lives for exactly one generation run

> Every AI call re-reads the provider row from the library and decrypts the API key again, including each retry. Change this so a key is decrypted once per generation run and reused for that run's calls, and a key the user saves mid-run is picked up by the next run. Cover every place that reads a provider key, and keep the key-handling rules intact.

**Move to teach.** For “every call”, build a roster of readers, then classify by purpose. For “once per run”, locate creation, concurrency, retry, resume and disposal boundaries. Look for a project rule that the apparent optimization would touch before recommending a storage location.

**Reading note.** Claude’s literal census, OpenAI’s callback/fan-out trace and the distinction between diagnostics, embeddings, administration and run execution are complementary. The plaintext-key rule makes run-lifetime retention an explicit design question. A harvested identifier count is not a count of runtime decryptions.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "decrypts", "provider row", "each retry", "generation run",
"every place", "key-handling rules".

1. **Turn 1, two calls.** Grep the decrypt call and the encrypted-key
   column outside tests, and Grep the architecture and guide documents for
   the rules. *Got:* 41 lines. Five generation providers each decrypt in a
   private method. So do the embedding service, the research key reader and
   the settings masking helper. The copy handlers move encrypted bytes
   without decrypting. The rule is written four times, and
   `pro/docs/architecture.md:1918-1923` says a key is decrypted only at the
   moment of a call.
2. **Turn 2, five calls.** Read the send method and the key method of one
   provider (`DeepSeekProvider.cs:136-163` and `:432-457`), the library
   helpers (`LibraryService.cs:1028-1063`), Grep the ambient scopes in
   Services and read the rule text. *Got:* the key is resolved inside the
   method that the retry loop calls, so each attempt opens a scope, reads
   the row and decrypts. A key passed in the request bypasses the lookup.
   Twenty ambient scopes exist, several named for pipeline execution.
3. **Turn 3, four calls.** Read the pipeline execution scope, Grep the key
   resolver call sites, Grep where that scope begins, and count key-service
   uses in tests. *Got:* that scope is per stage and carries the run's
   identifier. One provider has a second call site for accounting with an
   extra "must be enabled" condition. Tests already use a key-service
   double in 36 files.
4. **Turn 4, three calls.** Glob the orchestrator's partial files, read the
   dispatch (`PipelineOrchestrator.Dispatch.cs:1196-1239`) and read the
   run ownership type. *Got:* ownership of a project is acquired once per
   run and is ambient for the run.
5. **Turn 5, three calls.** Grep the ownership acquisition sites, read the
   embedding key path (`EmbeddingProviders.cs:112-141`) and the second
   provider's resolver (`OpenRouterProvider.cs:564-585`). *Got:* the run
   entries are in `PipelineOrchestrator.cs:120` and `:1004` and the
   generation plan file. Fifteen other callers acquire ownership for
   shorter work.
6. **Turn 6, two calls.** Read the factory and the registration lines.
   *Got:* the factory is a singleton that builds its provider instances
   once, so a cache held on a provider instance would outlive every run. A
   run-scoped holder is required.

**Context in hand.** All eight reading sites with the three that must not
change. Where the retry loop sits relative to the key read. The candidate
run boundary and its entry points. The lifetime trap. The ephemeral-key
bypass. The written rule that the change touches and its four locations.
The test double.

**Stop.** Every reader is listed and the run boundary has a concrete
candidate. **6 turns, 19 calls.**

</details>

<a id="bookbuilder-w1"></a>

#### Bookbuilder W1 · An author deciding whether to prepare an imported book

> Our in-app help only mentions sequel preparation for imported Word manuscripts in passing. Write a dedicated help page for authors: what "Let the AI learn this book for sequels" does, where they can run it (during import or later), what it costs and produces, what they see when it is done, and how it leads to starting a sequel or prequel.

**Move to teach.** Follow the author’s actual journey in order: discover the option by its label, read the promise before the click, inspect what the worker saves, then follow the visible completion state into the next screen. Use those transitions as the outline of the help page.

**Reading note.** The service saves preparation artifacts and returns a handoff preview. A logical analysis operation can involve more than one billable request. The evidence supports variable cost and a sequel/prequel choice.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** The quoted option label, "in-app help", "during import or
later".

1. **Turn 1, three calls.** Grep the label, Grep sequel and prequel in the
   help folder, and Glob the help folder. *Got:* the option is in
   `views/ImportView.vue:528`, a second entry point is in
   `views/GenerationView.vue:2405`, and two regression tests pin the label
   text. Help mentions the feature in three places in passing. Help
   articles are exported strings in 33 files with one index.
2. **Turn 2, four calls.** Read the import dialog (`ImportView.vue:505-566`),
   the later entry point (`GenerationView.vue:2376-2425`), the bridge
   handler and the existing help section (`help/articles/getting-started.ts:142-177`).
   *Got:* what the dialog tells the author about cost and that it can be
   skipped and run later. The later entry point is the generate phase of an
   imported book, which shows a status, three counts and an excerpt once
   preparation is done. The handler streams progress and uses the model
   family's background assignment.
3. **Turn 3, four calls.** Outline the service by Grep, read the rest of the
   panel (`GenerationView.vue:2425-2474`), read the help index
   (`help/index.ts:56-105`) and Grep the interface for the preparation and
   sequel actions. *Got:* two buttons, of which the second opens the sequel
   wizard and is enabled only after preparation. How an article is
   registered. The store action and the mock.
4. **Turn 4, three calls.** Read the service
   (`WritingForgePro.Services/ImportedNovelSequelPrepService.cs:14-219`),
   the summary line (`GenerationView.vue:174-209`) and the troubleshooting
   entry. *Got:* what is produced, in order: chapter digests without a
   model call, one whole-book analysis, then book digests, a style charter,
   story entities, promises and anchored details, and a book audit. It
   runs only for Word imports and returns early when already prepared.
5. **Turn 5, one call.** Grep the help test. *Got:* slugs must be unique
   and each article's category must exist.

**Context in hand.** What the option does and produces, both places to run
it, what the author sees during and after, what the dialog says about cost,
how the second button leads into the sequel wizard, the three existing
passing mentions to link from, and how to register a page.

**Stop.** All five things the page must say have a source. The exact cost
is not stated anywhere I read, and the page should say what the dialog
says. **5 turns, 15 calls.**

</details>

<a id="bookbuilder-w2"></a>

#### Bookbuilder W2 · How a stage gets its final prompt

> Write a short explainer for a new contributor on how prompt templates work in WritingForgePro: where the built-in templates are defined, how the template used for a stage is chosen when there are project, genre and global versions, and how placeholders and prompt blocks are filled in before the request goes to the model. Point out anything in the existing architecture document that no longer matches the code.

**Move to teach.** To explain precedence, trace a winning input, a missing override and the fallback. Follow the chosen value into the transformation code and a real caller. Only then compare each documentation claim with the executable branch that decides it.

**Reading note.** Keep selection and hydration separate. The async and sync paths differ. Block expansion, conditions, data substitution, inherited settings and the recursion limit each need their own supporting range.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "built-in templates", "project, genre and global", "placeholders
and prompt blocks", "architecture document". The harness file names the
registry file and states a three-step resolution order.

1. **Turn 1, three calls.** Glob `WritingForgePro.Services/Prompts/*.cs`,
   Grep the architecture document's headings for prompt and template, and
   count `genre` in the prompts folder. *Got:* 36 files: a resolver, a
   project-aware resolver, a hydrator, a block file and a built-in store
   with 24 partial files. The document has a section 5 with eight
   subsections (`pro/docs/architecture.md:392-524`) and a short summary at
   `:1957`.
2. **Turn 2, two calls.** Read the document section and the resolver
   (`PromptResolver.cs`, 194 lines). *Got:* the documented order and the
   coded order, side by side.
3. **Turn 3, four calls.** Outline the built-in store by Grep, read the
   hydrator (`TemplateHydrator.cs:1-200`), the project-aware resolver (100
   lines) and the document's summary. *Got:* the built-in registry function
   and its genre keys, the four hydration passes in order, and a second
   project-level override table.
4. **Turn 4, four calls.** Read the rest of the hydrator (`:199-348`), the
   store's lookup (`BuiltInTemplateStore.cs:17-52`) and registration
   (`:95-169`), and count the resolve and hydrate call sites. *Got:* block
   lookup order, what happens to an unresolved token, parent-genre
   fallback in the built-in store, and 36 calling files with one pipeline
   preparation file.

**Context in hand.** Where built-ins are registered and keyed. The coded
resolution order. The hydration order. Six places where the document and
the code differ: the code adds a parent-genre fallback, a selected pipeline
default with three possible positions, and an error when an explicit
override is missing. A project override can be a text-only row that
inherits schema and defaults. Blocks are expanded before data tokens, where
the document says after. Conditional sections and retrieval tokens are not
in the document. Recursion stops at a fixed depth where the document says
cycle detection. Blocks fall back to a compiled set.

**Stop.** Each clause is cited and each mismatch has both line ranges. I did
not check the document's JSON example against the entity. **4 turns, 13
calls.**

</details>

<a id="bookbuilder-t1"></a>

#### Bookbuilder T1 · What a DOCX chapter heading actually contains

> The Word (DOCX) export has tests for margins, running headers, title page and scene breaks, but nothing checks what it writes as chapter headings. Add tests covering the chapter heading for every heading style our built-in export profiles use, including how chapter numbers above ten and chapter titles with small words are written.

**Move to teach.** When a task says “every style”, derive the test population from what the product ships. Then follow the actual writer to identify the observable output, including special branches that bypass the shared helper. Choose boundary inputs with independent literal expectations.

**Reading note.** Seven profiles select six values. Two reach the ordinary fallback, and one writes two body paragraphs. Inspecting the generated document’s body, separately from its table of contents, would exercise the user-visible behavior.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "DOCX export", "chapter headings", "heading style", "built-in
export profiles", "above ten", "small words".

1. **Turn 1, two calls.** Glob the test project for export tests and count
   the heading-style property in `pro/src`. *Got:* 25 export test files,
   one of them `Services/DocxExportFormatterTests.cs`. The property appears
   in the profile file, the formatter, the plain-text formatter and the
   model.
2. **Turn 2, two calls.** Grep the property with context in the Core and
   Services projects, and list the test methods of the formatter's test
   file. *Got:* the built-in profiles use six distinct style values across
   seven profiles (`Services/Export/BuiltInExportProfiles.cs:38-242`). The
   formatter has a heading function at `DocxExportFormatter.cs:1558` and a
   special path at `:725`. The test file has five tests and none reads a
   heading.
3. **Turn 3, four calls.** Read the special path (`:722-791`), the heading
   function with its two helpers (`:1552-1622`), the test file's first 120
   lines, and Grep the profile file for names. *Got:* two of the six values
   used by profiles have no case of their own and take the default form.
   One style spells numbers as words up to ten and writes digits above
   that. One style bypasses the heading function and writes two centered
   paragraphs, passing the title through a title-casing helper with a list
   of small words that keeps first and last words capitalized. The profile
   Grep found nothing because the profiles are positional records.
4. **Turn 4, four calls, one empty.** Read the head of the profile file,
   Grep for the test-visibility attribute, and read the test file's helper
   and assertions (`:140-216`). *Got:* the Services assembly exposes
   internals to the test project, so the heading function and the number
   helper can be tested directly. The existing tests export to a temporary
   path and read the document body with the Open XML library, which is the
   pattern for asserting what a reader sees.

**Context in hand.** The six style values to cover and where they are
declared. The function, the special path and the two helpers with their
rules. Which behaviors are reachable directly and which only through an
exported document. The test file, its helper and its assertion pattern.

**Stop.** I can write the tests and I know the expected text for each style
from the code. Whether two profile styles falling to the default form is
intended is a question for the author. **4 turns, 12 calls.**

</details>

<a id="bookbuilder-t2"></a>

#### Bookbuilder T2 · Which pipeline checks to run

> I'm about to change how the generation pipeline orchestrates stages. Which test suites cover the pipeline, how do I run them on my machine (quick loop and the full gate), which of them need real provider keys, and does anything run them automatically?

**Move to teach.** Distinguish three questions embedded in “what runs”: what a contributor is told to do, what the script selects, and what triggers it. Read the selectors and skip conditions. Follow the trigger back to tracked automation and carry the working directory and prerequisite alongside every command.

**Reading note.** The default script samples, the full ordinary run excludes tagged integration, and a tag alone cannot distinguish an offline replay from a paid call. The Git listing covers the tracked snapshot; external CI configuration remains unknown.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "pipeline", "quick loop", "full gate", "real provider keys",
"automatically". The harness file names the test script, its two modes and
the sample list.

1. **Turn 1, three calls.**
   - Grep the test script (`pro/scripts/test-all.ps1`) for its parameters
     and the commands it runs. *Got:* four switches and the exact commands:
     the full mode runs the backend suite with the integration category
     filtered out and the whole front-end suite. The default mode runs a
     sample plus tests changed in the working tree.
   - Glob the test project for files named for the pipeline. *Got:* 90
     files across the Services, Regression, Bridge and Integration folders.
   - `git ls-files` for workflow, hook, YAML and script paths. *Expect:* a
     CI workflow. *Got:* none. No workflow file, hook or YAML is tracked,
     only PowerShell scripts.
2. **Turn 2, four calls.** Read the script's header (`:1-46`), read the
   sample list (`pro/scripts/smoke-tests.txt`, 321 lines), Grep the release
   and launcher scripts and both package manifests for test invocations,
   and count the category trait in the test project. *Got:* the sample
   names the orchestrator and builder suites and about twenty pipeline
   regression classes. The release scripts do not run tests. A menu script
   offers the full run. 24 classes carry the integration category.
3. **Turn 3, three calls, one empty.** Read the integration fixture's head,
   Grep three pipeline integration classes for their traits, environment
   variables and class comments, and Grep the guides for integration
   instructions. *Got:* provider integration tests read keys from a secret
   settings file in the test output folder and skip without it. One
   pipeline smoke test is opt-in through environment variables and asserts
   a live key is present. Two others are offline replays of locally
   retained files and use no key and no network. The guides say nothing
   more.

**Context in hand.** The suites by folder. The two commands and what each
includes and excludes, with the harness's rule for orchestration changes.
Which classes need keys, how keys are supplied and how a test skips. That
nothing tracked runs tests automatically.

**Stop.** All four questions are answered from files. The last answer is an
absence, and it rests on the tracked-file listing. **3 turns, 10 calls.**

</details>

<a id="bookbuilder-a1"></a>

#### Bookbuilder A1 · One Generate request, out and back

> Trace what happens when an author starts a generation run from the Generate screen: from the Vue store call, across the Photino bridge, through the pipeline orchestrator and a stage, to the AI provider HTTP call, and back until the output is saved in the .bbpro project and progress reaches the screen. Name each component on the way.

**Move to teach.** For an end-to-end path, follow one concrete input. Join the sending and receiving sides of a boundary on a value found in the source. Follow the selected branch through to durable output, then follow progress and final completion back separately.

**Reading note.** The OpenAI walkthrough contains its HTTP example within this task. Claude explicitly identified a provider read borrowed from another task that would require another local read. The two providers are separate examples. They cannot be spliced into a single observed execution.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "Generate screen", "Vue store", "Photino bridge", "pipeline
orchestrator", "stage", "provider HTTP call", ".bbpro", "progress".

1. **Turn 1, three calls.** Grep the pipeline store for bridge calls, Grep
   the bridge project for pipeline action names, and Grep the "where things
   live" reference for the bridge and the orchestrator. *Got:* the store
   sends one execute action (`stores/pipelineStore.ts:146`). The handler is
   `WritingForgePro.Bridge/Handlers/ExecutePipelineHandler.cs`. The
   reference names the orchestrator's partial files.
2. **Turn 2, three calls.** Read the store's run function
   (`pipelineStore.ts:120-215`), Grep the bridge composable for the
   envelope and correlation logic, and read the handler. *Got:* the store
   checks saved decisions and an accepted plan, then sends the action with
   a progress callback and no timeout. The composable assigns a correlation
   identifier, posts through the window's message function and routes
   progress and final replies by that identifier. The handler registers a
   cancellable run, wraps progress into envelopes with the same identifier
   and calls the orchestrator in a new scope.
3. **Turn 3, two calls.** Grep the host and bridge for the window's message
   registration and the router, and Grep the orchestrator files for method
   signatures. *Got:* `WritingForgePro.Host/Program.cs:262-288` and
   `WritingForgePro.Bridge/MessageRouter.cs:45`. The signature search
   matched only the dispatch file because the main file wraps its
   signatures across lines.
4. **Turn 4, four calls.** Read the registration (`Program.cs:256-295`),
   the router (`MessageRouter.cs:40-159`), the orchestrator's entry
   (`PipelineOrchestrator.cs:96-205`) and the stage dispatch
   (`PipelineOrchestrator.Dispatch.cs:1019-1070`). *Got:* the router finds
   the handler by action, tracks cancellation by correlation identifier and
   sends the final or error envelope. The orchestrator takes ownership of
   the project for the run, opens the project database, refuses imported
   books, resolves the pipeline and its stages and checks the dependency
   graph. Dispatch picks a built-in function, an extension or a prompt
   stage.
5. **Turn 5, two calls.** Read the prompt-stage path (`Dispatch.cs:1183-1196`)
   and count provider calls and artifact writes in the pipeline folder.
   *Got:* 69 hits in 49 files. The shared stage base class holds four.
6. **Turn 6, two calls.** Grep the stage base class
   (`Services/Pipeline/PipelineStageBase.cs`) for its resolve, hydrate,
   provider, save and progress lines, and count the progress consumers in
   the interface. *Got:* the order inside a stage: open the project
   database through the factory (`:141`), resolve the prompt (`:446`),
   prepare and hydrate it (`:471`), build the request (`:534`), get the
   provider (`:1270`), call it under a timed policy (`:2218`), add the
   artifact (`:337`) and save (`:232`), reporting progress along the way.
   The store, the view and one panel consume progress.

**Context in hand.** Every hop with a file and line range, outward and
back: view, store, composable, native window, router, handler, orchestrator,
dispatch, stage base, prompt resolver and hydrator, provider factory with
its three decorators, provider and its HTTP request, the artifact write to
the project file, and the progress path in reverse.

**Stop.** The chain is closed in both directions. I read the provider's
request code in another task and would cite `DeepSeekProvider.cs:44-163`
after one more read here. **6 turns, 16 calls.**

</details>

<a id="bookbuilder-a2"></a>

#### Bookbuilder A2 · Assessing a macOS release

> We are considering shipping a macOS build again. Assess what that would take: what macOS runtime code still exists and is wired up, what in the build, release and installer path is Windows-only, which project rules and tests would have to change, and where the platform documentation is out of date.

**Move to teach.** For a feasibility question, first partition the evidence by authority: policy, wired runtime, executable packaging path and tests. Search those in parallel. Verify a real selection point and a real packaging entry before treating a platform name as support.

**Reading note.** Read the release tests that enforce the absence of old packaging as well as the surviving native implementations. Source can establish wiring and release policy. Installed Mac behavior still requires a matching native run.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "macOS", "wired up", "build, release and installer",
"Windows-only", "project rules and tests", "platform documentation". The
harness file states the rule: a Windows installer only, macOS runtime code
retained.

1. **Turn 1, three calls.**
   - Grep the platform words (the macOS check, `osx-`, Keychain) in
     non-test source, count mode. *Got:* 193 hits in 79 files. Most are
     two-line platform switches. The dense ones are the host start-up, the
     path service, the dialog service and the ownership lock.
   - Grep the build side: the release, deploy and launcher scripts and
     every project file, for runtime identifiers, installer words and
     platform conditions. *Got:* the release script publishes one Windows
     runtime and builds a WiX package. Two installer projects exist. The
     host project copies native webview files for six runtime identifiers,
     macOS included.
   - Count the platform words in the root and `pro/docs` documents. *Got:*
     109 hits in 24 files, including a `platform-notes.md`.
2. **Turn 2, three calls.** Grep the host, native UI, path, file-system,
   security and one handler file with content output, read
   `pro/docs/platform-notes.md`, and count platform and installer words in
   the test project. *Got:* what is wired: the registration picks macOS
   file-system, shell and Keychain implementations
   (`ServiceRegistration/InfrastructureExtensions.cs:25-28`,
   `LibraryServiceExtensions.cs:56-57`), the host has a non-Windows
   single-instance lock, the dialog service has a macOS branch and the
   path service has macOS locations. Tests exist for the key service, the
   file system and the path service.
3. **Turn 3, three calls.** Grep the release regression test for its
   assertions, read the host project file, and Grep six documents for the
   platform words with content output. *Got:* a test asserts that deleted
   release infrastructure stays deleted and names a Mac installer project,
   a Mac build folder and a Mac release script. The host project is a
   Windows executable with a Windows icon. The architecture document lists
   macOS runtime targets and says a macOS port will follow. The developer
   guide names a cross-platform verify script.
4. **Turn 4, one call.** `git ls-files` for Mac, Linux and shell scripts.
   *Got:* two legacy shell scripts and no packaging script. The Linux
   installer script and the verify script that the documents name are not
   tracked.

**Context in hand.** Runtime: the seven wired macOS surfaces. Build:
everything from publish to package is Windows-specific, in three scripts and
two installer projects. Rules and tests: the harness rule and its two README
statements, and the regression test that forbids the Mac release files.
Documents: the platform notes name an interface the code does not have,
attribute paths to the wrong class, and describe a Linux installer and an
update channel that are not in the tree. The architecture document and the
developer guide carry the stale statements above.

**Stop.** Each of the four parts has evidence. Whether the retained macOS
code still works was not run and cannot be judged from source alone. **4
turns, 10 calls.**

</details>

<a id="bookbuilder-a3"></a>

#### Bookbuilder A3 · What adding a grant-proposal product would change

> We want to ship a new bundled pipeline product for grant proposals. Where does a new product's source go, what files and contract must it have, how does it get compiled into the installer payload, which validation rules must it pass, and what tests or counts will break when we add it?

**Move to teach.** Check the premise before searching for an implementation example. If the requested item already exists, branch into “extend that identity” and “introduce another”. Read a neighboring product, the authoring contract, build entry, validator and independent count assertions for each interpretation.

**Reading note.** The pinned tree contains academic-grant-proposal. Claude’s count and validator survey is relevant to a second product. OpenAI’s exact identity lookup changes the decision for an update. Today’s tests pin 46 total and 30 nonfiction entries.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "bundled pipeline product", "source", "contract", "compiled
into the installer payload", "validation rules", "tests or counts". The
harness file names the seed build script.

1. **Turn 1, three calls.** `git ls-files` for the two product roots
   grouped to two levels, read the seed build script
   (`pro/scripts/subscripts/build-seed-pipelines.ps1:1-120`), and Grep the
   number 46 in tests, interface code, scripts and project files. *Expect:*
   a count somewhere. *Got:* the folder shape of a product. The script
   builds a builder project, runs it over both roots, stages one bundle per
   product plus a catalog into the host project, refuses duplicate slugs
   and identifiers, and requires the staged count to equal the number of
   product folders. 39 lines mention 46, of which about twenty are real
   count assertions in eight backend test files and two front-end tests, a
   generated help constant, four help sentences and one capture script.
2. **Turn 2, three calls, one wasted.** `git ls-files` for one sibling
   product, the root documents and the builder project. My pathspec
   matched recursively and returned 160 KB. Also read the sibling's
   `product.json` and the validation script
   (`pro/scripts/validate-pipelines.ps1:1-70`). *Got:* the closest sibling
   is an academic book proposal product with 51 files. Its metadata shows
   the setup defaults and recommendations a product declares.
3. **Turn 3, two calls.** Re-run the listing with exact globs and Grep the
   bundling regression test. *Got:* 40 products in one root and 6 in the
   other. The root has an authoring guide, an AI guide and an inventory.
   The builder is two source files. The test asserts the total, the
   nonfiction count and uniqueness of identifiers, slugs and file names.
4. **Turn 4, three calls.** Grep the headings of the three root documents,
   Grep the builder for its validator functions, and read the rest of the
   validation script (`:70-129`). *Got:* about fifty validators by name:
   product, project setup, pipeline, compatibility, stages, dependency
   graph, fan-out, projections, entities, prompts and their tokens, the
   generation contract, the final prose stage, the interview schema and
   the structured-output schema subset. The script runs the builder in
   validate mode and writes a report.
5. **Turn 5, one call.** Read the authoring guide
   (`pro/pipeline-products/README.md:1-125`). *Got:* the written contract:
   required and optional files, required metadata fields, stage and prompt
   rules, the build command and the validate command. It also states the
   product counts per category, which will change.

**Context in hand.** Where the source goes and what a sibling looks like.
The written contract and the validators that enforce it. The build path
into the host project and from there into the installer. The commands. The
complete list of places that hard-code the product count or the nonfiction
count.

**Stop.** All five questions have sources. **5 turns, 12 calls, 1 wasted.**

</details>

### Munarium

Munarium is a Rust server (`server/`) with client libraries in Rust, Python,
.NET and Java (`clients/`). Paths below are relative to the Munarium
checkout.

<a id="munarium-c1"></a>

#### Munarium C1 · A ninth output budget without losing tenant settings

> Vocabulary generation currently borrows the general completion default for its output-token ceiling. Give it its own per-call budget, `vocabulary_generation`, alongside the existing eight: a built-in value, a `MUNARIUM_MAX_TOKENS_*` variable, the `/v1/max-tokens` read and replace, and the client libraries, with tests. Tenants that have already replaced their budgets must keep their settings after the upgrade.

**Move to teach.** Start with the existing budget the prompt says is borrowed. Find its consumers and name variants across the configuration, API and client boundaries. Before choosing a default, follow a persisted old row through deserialization and the fallback branch.

**Reading note.** Old eight-field storage and a new whole-replacement request have different compatibility needs. The generic gRPC wrapper carries the server operation; the pinned Rust typed gRPC methods return unsupported. A generic route cannot establish availability in every SDK.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** `MUNARIUM_MAX_TOKENS_`, `/v1/max-tokens`, `vocabulary_generation`,
"existing eight", "client libraries", "keep their settings after the upgrade".

1. **Turn 1, three calls.**
   - Grep `MUNARIUM_MAX_TOKENS_`, whole tree, count mode. *Expect:* one
     config module, an environment table in the docs, a few clients.
     *Got:* 57 hits in 18 files. One file holds 16 of them:
     `server/src/munarium-server/src/max_tokens_api.rs`. Also a migration
     `0031_max_tokens_budgets.sql`, `server/docs/tokenbudgets.md` (9) and
     one hit in each of the four client model files.
   - Grep for the route and type names (`max-tokens`, `MaxTokens`), count
     mode. *Expect:* handler, store, proto, OpenAPI, client transports and
     tests. *Got:* 423 hits in 84 files, which lists every transport and
     test file per client, the proto, `openapi.json`, `clients/server-api.json`
     and a store module `munarium-store-pg/src/max_tokens.rs`.
   - Grep `vocabulary generat`, case-insensitive, content mode. *Expect:*
     the call site. *Got:* 30 lines, mostly docs. The code is
     `vocabulary_api.rs`, and `tokenbudgets.md:24` and `:74-75` state that
     vocabulary generation uses `complete_default`.
2. **Turn 2, three calls.** Read all of `max_tokens_api.rs` (579 lines),
   `munarium-store-pg/src/max_tokens.rs` (72) and the migration (14).
   *Expect:* the list of eight and how a replacement is stored. *Got:* one
   `FIELDS` table of eight rows (JSON field, variable, accessor) at `:50-91`,
   range checks at `:123-140`, and the registry at `:156-206`. A tenant's
   replacement is one JSON object per tenant. On load, a stored row that
   does not decode as the whole object falls back to the process defaults
   with a warning. That is the upgrade hazard the last sentence of the task
   is about: a ninth required field would make every existing row
   undecodable. The tests at `:312-578` pin the count, the built-in values
   and the "missing field is refused" rule.
3. **Turn 3, three calls.**
   - Grep `MaxTokensBudgets` in `munarium-api-types/src/lib.rs` with
     context. *Got:* the wire type at `:3224-3261`, response at `:3266-3278`.
   - Grep `complete_default` and the registry's `effective` call in
     `server/src`. *Expect:* the vocabulary call site and the other users.
     *Got:* `vocabulary_api.rs:552` is the one to change.
     `answers_api.rs:393` and `providers_api.rs:418-427` are the users that
     keep it.
   - Grep `MaxTokens` in `server/proto/mmp/v1/server_api.proto`. *Expect:*
     a message with eight fields. *Got:* only two generic RPCs at
     `:181-184`. The gRPC twin passes JSON through, so the proto needs no
     field.
4. **Turn 4, four calls.** Read the wire type (`lib.rs:3212-3283`), the call
   site (`vocabulary_api.rs:536-559`), the store test
   (`pg_integration.rs:1575-1634`) and Grep the budget terms in
   `tokenbudgets.md`. *Got:* every field is required on the wire and the
   type derives a plain deserializer, the store test proves whole-object
   replace, and the doc has a per-budget table, an environment table
   (`:88-95`), wire examples and a client table (`:184-196`).
5. **Turn 5, two calls.** Grep the last existing field name,
   `authoring_assist`, in `clients/` (content) and in `server/` (count).
   *Expect:* every place that lists the eight. *Got:* 34 client lines
   across the .NET, Java and Python models and tests and the Rust client
   tests, and 25 server hits in 8 files including `openapi.json`,
   `dev-guide.md`, `platform-features.md` and `authoring_api.rs`.
6. **Turn 6, four calls.** Read the .NET model
   (`Models.Platform.cs:340-411`), the Python model (`models.py:1100-1161`)
   and the Java model (`Providers.java:56-175`), and Grep the Rust client
   for the type. *Got:* .NET has required init properties and a response
   record that copies eight fields back. Java has a record with eight
   `with…` methods. Python subclasses the budgets model for the response.
   The Rust client reuses the server's wire crate, and its gRPC transport
   reports max-tokens as unsupported (`grpc.rs:1582-1586`).
7. **Turn 7, three calls.**
   - Grep `max.tokens` in seven server files (config, state, router,
     OpenAPI registration, generated gRPC, two test configs). *Got:* the
     parse site `config.rs:355`, the route `rest.rs:1758-1760`, the
     generated twin `grpc_api_generated.rs:237-242`, and two test configs
     that build the default object.
   - Glob the migrations folder. *Got:* the newest is `0088`. A backfill
     migration, if chosen, is `0089`. `CLAUDE.md` already told me applied
     migrations are never edited.
   - Grep `max.tokens` in `clients/server-api.json`, the compatibility
     replay cases, the retention inventory and `docs/route-index.py`.
     *Got:* a v1.4 replay case reads the route (`replay-v14.json:134`) and
     the retention inventory lists the table.
8. **Turn 8, one call.** Grep the compatibility tool for how it treats an
   added response field. *Got:* no matches. That stays an open question to
   settle by running the replay.

**Context in hand.** The registration table and its tests, the wire type,
the storage shape and its fail-closed load path, the single call site, the
other two users of the shared default, four client models with their tests,
the generated and hand-written documents that list the eight, the migration
numbering rule and the compatibility replay.

**Stop.** Every clause maps to a site. The design choice left is how an
existing eight-field row keeps decoding. That is a decision, and the facts
for it are in hand. **8 turns, 23 calls.**

</details>

<a id="munarium-c2"></a>

#### Munarium C2 · Why an expired session can still take a turn

> Operators set MUNARIUM_SESSION_IDLE_TTL_SECS=30, yet a session left idle for more than a minute sometimes still accepts a turn instead of answering 409 session-not-open. Find the cause and fix it so an idle session is refused as soon as its TTL has passed.

**Move to teach.** For an intermittent timing symptom, put the state writer, request reader and clock on one timeline. Ask what the request sees after the deadline but before the background writer runs. That interval identifies the decisive source read and the deterministic test fixture.

**Reading note.** The PostgreSQL sweep uses a minimum 60-second interval, initial jitter and stored activity time. Scheduled observations illustrate the race; they provide no guaranteed upper bound on delay. Cover first turn, refreshed activity, zero-disable and the exact cutoff.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** `MUNARIUM_SESSION_IDLE_TTL_SECS`, `session-not-open`, "sometimes",
"more than a minute".

1. **Turn 1, two calls.**
   - Grep the setting name in both spellings, content mode. *Expect:* a
     parse site, a sweeper and docs. *Got:* 38 lines. `config.rs:75` and
     `:352-354` parse it. `state.rs:580` uses it. About twenty test configs
     set it to zero. `server/README.md:262` says a janitor stamps idle
     sessions expired.
   - Grep the error slug in `server/src`. *Got:* the constructor
     `error.rs:338` and exactly one raise site, `sessions_api.rs:1332`.
2. **Turn 2, four calls.** Read the janitor (`state.rs:560-623`), the turn
   entry (`sessions_api.rs:1296-1345`), the setting's comment
   (`config.rs:64-77`) and the design note (`dev-guide.md:8128-8149`).
   *Expect:* a periodic sweep whose period explains "sometimes". *Got:* the
   cause. The sweep interval is the larger of one twenty-fourth of the TTL
   and 60 seconds, after a random start delay of up to 59 seconds. The turn
   entry refuses only when the stored state is not open. It never compares
   the session's age with the TTL. With a 30-second TTL a session can stay
   open for up to about 90 seconds. The sweep exists only with the
   PostgreSQL store.
3. **Turn 3, three calls.** Grep `last_turn_at`, Grep the state comparisons
   and the janitor's words, and Grep the session loader and turn function
   names. *Got:* the timestamp is written at `sessions_api.rs:1868` after a
   turn. The turn function is called from REST (`:2135`), the streaming
   route (`:2212`) and gRPC (`grpc_platform.rs:183`), so one check covers
   every transport. A second private loader lives in `mcp_tools.rs:1520`.
4. **Turn 4, four calls, one failed.** Read the loader
   (`sessions_api.rs:486-545`), the timestamp write (`:1852-1875`) and the
   close path (`:2258-2337`). A fourth Grep with a nested brace glob was
   rejected by ripgrep. *Got:* the loader selects five columns and no
   timestamp, so the fix must extend the query or do the comparison in SQL.
   The close path shows the house pattern for a guarded state change: one
   conditional update that only moves an open session.
5. **Turn 5, three calls.** Re-run the failed search as a files-mode Grep of
   the slug over the tree, Grep `mcp_tools.rs` for state checks, and Grep
   the session model tests for janitor coverage. *Got:* 25 files name the
   slug, including `docs/api/errors.md`, `docs/ops/troubleshooting.md` and
   each client's error decoder. The MCP file has no state check of its own.
   No test covers idle expiry.
6. **Turn 6, three calls.** Read the MCP loader (`mcp_tools.rs:1490-1551`),
   Grep the close route and the closed state across the server, and Grep
   the three reference documents for the slug. *Got:* MCP session tools
   dispatch through the native route, so they inherit the fix. The nearest
   lifecycle test is the close test in `mcp_tests.rs:1609-1618`. The
   documents to update are `errors.md:59`, `troubleshooting.md:26`,
   `README.md:262` and the design note.

**Context in hand.** The cause with its two line ranges, the single
enforcement site shared by all transports, the loader that lacks the
timestamp, the write site of the timestamp, the house pattern for a guarded
state change, the absence of a regression test and its nearest neighbor,
and four documents that describe the old behavior.

**Stop.** Cause located and confirmed in source. Fix site, pattern and test
home known. I have not reproduced the failure by running it. **6 turns, 19
calls, 1 failed.**

</details>

<a id="munarium-c3"></a>

#### Munarium C3 · A file-count change that preserves the byte ceiling

> Raise the per-request file cap for batch ingest and bulk-upload chunks from 500 to 1,000 files. Update every place that enforces, tests or documents the limit, on both transports and in the client libraries, and leave the 256 MiB body ceiling as it is.

**Move to teach.** For a numeric change, search the bare number, its unit, error wording and discovered constant names independently. Classify matches by operation and unit. Trace server transports, CLI and SDKs separately and close with a second enumeration.

**Reading note.** Include the CLI’s default of 400 and clamp at 500. Java describes bulk chunks as REST-only even though the native server has a gRPC route. The canonical message limit, separate framing allowance and aggregate-content guard have different purposes. Searches for a server test bound an absence only within their stated selection.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** `500`, "batch ingest", "bulk-upload chunks", "256 MiB", "every
place".

1. **Turn 1, three calls.**
   - Grep a prose pattern (500 near files, documents, per request), content
     mode. *Expect:* documentation and comments. *Got:* 35 lines: four
     client guides, client doc comments, `openapi.json`, three server
     guides, a proto comment and two error strings in `ingest_api.rs:198`
     and `:605`.
   - Grep constant-like names (a cap word next to batch, bulk or files).
     *Expect:* one server constant. *Got:* four client constants (Rust
     `planes.rs:229`, Python `_errors.py:329`, .NET `Planes.cs:116`, Java
     `Wire.java:19`) and no server constant.
   - Grep the body ceiling in three spellings, count mode. *Got:* 60 hits
     in 39 files. I now know what to leave alone.
2. **Turn 2, two calls.** The server enforces with a bare number, which a
   prose pattern cannot find. Grep `\b50[01]\b` in the server's ingest, gRPC
   and router files with context, and in client code files. *Got:* the two
   comparisons, `ingest_api.rs:196` and `:604`. In clients: the four
   constants, about twenty doc comments, and tests that build 501 files in
   every language (.NET `PlatformTests.cs:47-55` and `PlatformSmokes.cs:591-596`,
   Java `PlatformSmokesTest.java:444-450`, Python `test_c8_surface.py:77`
   and `conformance/test_platform.py:520-521`, Rust
   `platform_smoke.rs:975-987`). Several assert that the error text
   contains "500".
3. **Turn 3, four calls.**
   - Grep the two operation function names. *Got:* gRPC calls the same
     batch function (`grpc_data.rs:161`), so the batch check is shared by
     both transports. The chunk route is REST only.
   - Grep `501` and the error string over all server code. *Expect:* a
     server test of the cap. *Got:* none. I also got a hit I had not
     looked for: `munarium-cli/src/main.rs:598` clamps a value to 500.
   - Grep with only-matching output around `500` in the three API
     reference documents. *Got:* `rest.md:188`, `grpc.md:22` and
     `grpc-reference.md:716`.
   - Read the Python guard (`_errors.py:318-338`). *Got:* one shared check
     whose message is built from the constant.
4. **Turn 4, four calls.** Read the CLI (`main.rs:584-605`), count the four
   client constant names over the tree, Grep `500` in the root docs and
   scripts, and Grep the changelogs and compatibility text. *Got:* the CLI
   defaults to 400 files per chunk and clamps to 500. The constants have 19
   uses in 8 files. `clients/README.md:383` states the cap.
5. **Turn 5, one call.** Grep `MAX_SOURCE_BYTES`. *Got:* the body ceiling
   is one constant at `rest.rs:24`, used by the route limits and both gRPC
   message limits. Nothing about the file cap touches it.

**Context in hand.** Two server comparisons with their error strings, one
shared by both transports. One CLI clamp. Four client constants with their
guards. Client and conformance tests in four languages that assert 501 is
refused and that the message names 500. Generated and hand-written documents
on both sides. No server test exists.

**Stop.** The set of sites is closed under three independent searches
(prose, bare literal, constant names). **5 turns, 14 calls.**

</details>

<a id="munarium-w1"></a>

#### Munarium W1 · What authors of clients need to know about retiring a runbook

> Write the user-guide page on retiring a runbook version: the two-step request-and-confirm flow and its time limit, what callers and existing sessions see afterwards, what is and is not deleted, whether it can be undone, and how to call it from each client library.

**Move to teach.** When the user’s verb fails to find an owner, look in guides and captions for the product’s term, then search that exact term. Walk a single version through request, confirmation, new reads and an already-open session. Verify each client’s own method spelling.

**Reading note.** The matched reference and removal ID matter. Expiry or reapply can cancel a pending transition. A confirmed removal cannot be undone through that flow. Rust/Python, Java and .NET use different method spellings. A surviving session row and a successful next turn are different claims.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "retiring a runbook version", "two-step", "time limit", "each
client library".

1. **Turn 1, two calls.**
   - Grep `retir`, case-insensitive, count mode. *Expect:* a handler and a
     guide section. *Got:* 441 hits in 138 files. The word is used for
     index versions, artifacts, collections and a license identifier. It is
     too ambiguous to follow.
   - Glob `**/docs/guides/*.md` to see where a guide page belongs. *Got:*
     219 paths, most of them stale copies under an ignored
     `server/scratch/` folder. Glob does not honor ignore rules. The
     tracked guides are in `server/docs/guides/` and `clients/docs/guides/`.
     This call was mostly noise.
2. **Turn 2, three calls.** Grep `retir` in the runbook handler file, in
   client code and docs, and next to "runbook" or "version" in server docs.
   *Expect:* the feature's handler. *Got:* the handler file uses the word
   only for a different step that reclaims old index rows. The client hits
   are about collections. One line gave the real vocabulary: a figure
   caption in `dev-guide.md:13351` describes "double-pass soft removal".
3. **Turn 3, one call.** Grep the new vocabulary (double pass, soft
   removal, confirm removal). *Got:* 70 lines. The feature is called
   **removal** everywhere in code: `remove_request` and `remove_confirm` in
   every client, `ConfirmRemoval` on gRPC, and a conformance scenario named
   for the double pass.
4. **Turn 4, two calls.** Grep the server for the removal functions,
   identifier and constants, and Grep `remov` in the developer guide.
   *Got:* the implementation is `runbooks_api.rs:1286-1469` with a
   15-minute constant at `:1292`. The guide has a full narrative at
   `dev-guide.md:12285-12332`.
5. **Turn 5, three calls.** Read the implementation
   (`runbooks_api.rs:1286-1471`), the guide narrative
   (`dev-guide.md:12284-12349`) and the session section
   (`dev-guide.md:10440-10509`). *Got:* the complete rules. A request must
   name an exact version. It returns an identifier and an expiry. Confirm
   refuses when nothing is pending, when the identifier differs, or after
   15 minutes, and an expired request resets the runbook to active. Both
   calls need the read-write role. After confirmation the exact version
   answers 410 on session routes and is hidden from the default listing.
   Nothing is deleted.
6. **Turn 6, three calls.** Grep the client method signatures in four
   languages, Grep `remov` in client guides and READMEs, and Grep the two
   error slugs in the API reference. *Got:* 38 signature lines covering
   REST and gRPC transports in Rust, Python (sync and async), .NET and
   Java. The client guides say nothing about removal, so the page fills a
   real hole. `errors.md:87` and `:89` hold the two slugs.
7. **Turn 7, four calls.** Read the Python conformance scenario
   (`conformance/test_platform.py:312-355`), the Rust one
   (`platform_smoke.rs:516-575`), the apply function
   (`runbooks_api.rs:770-819`) and the two error rows. *Got:* the calling
   idiom per client and the typed errors a caller catches. Re-applying a
   removed version is refused, and re-applying while a request is pending
   cancels the request.
8. **Turn 8, two calls.** Grep the call sites of the removed-runbook error
   and read the session loader (`sessions_api.rs:424-453`). *Got:* a bare
   name resolves to the newest version that is not removed. An exact
   removed version answers 410 for new and existing sessions.

**Context in hand.** The two routes and their rules, the time limit and its
reset, the role, both error slugs with status and gRPC code, what a session
and a listing see, what is retained, the two "cannot undo" rules, and the
method names on every client and transport with a working example for two
of them. Also where the page goes and that the index must link it.

**Stop.** Every clause of the instruction has a cited source. **8 turns, 20
calls.** The first three turns were spent finding the repository's word for
the feature.

</details>

<a id="munarium-w2"></a>

#### Munarium W2 · Which claim a later reader sees

> Write an explainer for a new contributor on how a proposed claim is governed and written: what the command path does between the REST or gRPC handler and the database, what the gates decide, how a blocked claim, a head conflict and a concurrent writer are handled, and what a later read then returns.

**Move to teach.** To explain a write, follow both entry points to their shared owner and inspect the transaction, including a competing writer. To explain what persists, read through both an ID lookup and the default query. Put the stored status and its visibility on separate lines.

**Reading note.** A keyed block may persist a disputed claim. Direct lookup can return it while default facts omit it. Shape validation, head pinning, re-evaluation, as-of selection and supersession add conditions that a generic gate summary would lose.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "proposed claim", "gates", "head conflict", "concurrent writer",
"later read".

1. **Turn 1, three calls.** Grep the propose names in Rust and proto files
   (count), Grep the head-conflict names (count) and list the headings of
   `server/docs/architecture.md`. *Got:* 56 and 105 hits. `service.rs`,
   `rest.rs`, `grpc.rs`, the core `storage.rs`, `governance.rs` and both
   stores carry them. The architecture document has no section on the
   command path.
2. **Turn 2, two calls.** Grep `propose` in the three server files with
   line numbers, and the conflict names in three core files with context.
   *Got:* the REST handler (`rest.rs:310-360`), the gRPC handler
   (`grpc.rs:215-266`) and a module comment at `service.rs:6-8` that says
   the command path is the governance path. The storage trait documents the
   optimistic head check.
3. **Turn 3, two calls.** Read `service.rs:1-300` and `rest.rs:296-363`.
   *Got:* the spine of the explainer. Both transports call one function.
   It reads the head, checks a caller's expected head, loads a snapshot at
   that head, runs the gates, marks blocked claims as disputed and the rest
   as accepted, and appends the batch together with its findings pinned to
   the observed head. When the head moved and the caller sent no expected
   head, it gates again, up to three attempts.
4. **Turn 4, three calls.** Outline `munarium-core/src/gates.rs` by Grep,
   read the core error type (`error.rs:1-60`), and Grep the store function
   names and lock statements. *Got:* five always-on gates with their rule
   identifiers at `gates.rs:55-59`, a sixth chronology gate that is armed
   per version, and the PostgreSQL append functions.
5. **Turn 5, five calls.** Read the gate runner (`gates.rs:355-429`), the
   uncommitted append (`munarium-store-pg/src/lib.rs:176-245`), the lineage
   lock (`:448-487`), the two append entry points (`:720-799`) and the
   snapshot loader (`munarium-core/src/storage.rs:244-303`). *Got:* how a
   concurrent writer is serialized: one transaction holds a row lock on the
   lineage head, checks the expected head under that lock, and writes the
   claims and the findings together.
6. **Turn 6, four calls.** Grep the slice resolver with context
   (`ledger.rs:86-142`), Grep the error mapping in the server's `error.rs`,
   read the gRPC handler (`grpc.rs:212-267`) and Grep the idempotency
   wrapper (`rest.rs:193-257`). *Got:* a later read returns accepted claims
   by default, so a disputed claim is stored and hidden unless the reader
   asks for that status. A head conflict is HTTP 409 and gRPC aborted, with
   the expected and actual sequence numbers. Commands require an
   idempotency key, and a replay returns the stored response.
7. **Turn 7, one call.** Count "disputed" in the server docs. *Got:* 104
   hits in 17 files, 65 of them in the developer guide, which is where the
   explainer should link.

**Context in hand.** The handler-to-store path on both transports, the six
gates and what a block does, the three outcomes the task names with the
exact code that produces each, the read rule, and the error mapping.

**Stop.** Each clause is cited to code I read. **7 turns, 20 calls.**

</details>

<a id="munarium-t1"></a>

#### Munarium T1 · An offline test that would catch an accidental second send

> Make sure the Python client's REST retry rules are proven offline, end to end through the real transport: reads retry transient failures, a command is re-sent with the same idempotency key only after a connect-phase failure or a typed `overloaded`, never after a gateway 502 or 504, and plain writes are sent exactly once. Cover both the sync and the async transport, and add whatever tests are missing.

**Move to teach.** Translate the requirement into observables at the actual boundary: how many sends, which headers, which error phase. Inspect the test double and assertions before crediting coverage. Compare independent sync and async loops and use a scripted extra response to expose an accidental resend.

**Reading note.** Both accounts locate the same sync wire tests. The missing proofs include stable generated keys on connect replay and real one-shot plain writes in both modes. Content-addressed uploads follow a separate retry path.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "Python client", "retry", "idempotency key", `overloaded`, "502
or 504", "sync and the async", "whatever tests are missing".

1. **Turn 1, two calls.** Grep `retr` in the Python client (count) and Glob
   `clients/python/tests/*.py`. *Got:* the rules live in `rest.py` (39
   hits) with per-operation classes in `_specs.py` (43). Thirteen test
   files exist. `test_replay_safety.py` looks like the retry tests.
2. **Turn 2, three calls.** Read `_retry.py` (25 lines), Grep the retry
   terms in `rest.py` and list the test functions in
   `test_replay_safety.py`. *Got:* backoff only in `_retry.py`. The
   decision is in `rest.py:172-186` and the two run loops. The existing
   tests in that file call the decision function directly. They do not go
   through a transport.
3. **Turn 3, three calls.** Read `rest.py:1-60` and `:152-297`, and Grep
   the tests for a mock transport. *Got:* the module comment states the
   three rules. The transport constructor takes an injected transport as a
   documented test seam (`rest.py:217-228`). A second test file,
   `test_review_batch.py`, has a section that exercises the rule on the
   wire.
4. **Turn 4, four calls.** Read that section
   (`test_review_batch.py:134-245`), the async run loop
   (`rest.py:356-411`), Grep the retry classes in `_specs.py` and Grep the
   test configuration in `pyproject.toml`. *Got:* five wire tests exist,
   all on the sync transport: a command after typed overloaded keeps its
   key, a command after a connect error is re-sent, a command after 502 or
   504 is not, a read after 502 or 504 is, and the overloaded retry is
   bounded. The backoff patch only covers the sync sleep. Async tests run
   in auto mode.
5. **Turn 5, five calls, one failed.** Read the upload path
   (`rest.py:306-351`), the spec type (`_specs.py:16-51`), Grep the
   transient flags in `_errors.py` and Grep the Python job in
   `clients-ci.yml`. A multi-line Grep failed because the flag was missing.
   *Got:* content-addressed uploads retry on purpose and are outside the
   three rules. 502, 503 and 504 are transient for reads. CI runs
   `python -m pytest tests -q`, so new offline tests run automatically.
6. **Turn 6, one call.** Read `_specs.py:194-205` in place of the failed
   search. *Got:* a plain write spec to use in the new tests.

**Context in hand.** The rules as written, the two run loops, the seam, the
five existing wire tests and their helper, and the coverage gaps: every
async case, reads after transport failures and typed overloaded, plain
writes after each failure kind, the same generated key across a connect
retry, the other connect-phase error types, and a command after a failure
that may have been delivered.

**Stop.** I can write the missing tests against a known seam and helper. **6
turns, 18 calls, 1 failed.**

</details>

<a id="munarium-t2"></a>

#### Munarium T2 · Checks for a new REST route and SQL migration

> I've added a new REST route to the server, backed by a new SQL migration. Before I open the pull request, list the checks I must run, with the exact commands, and say what each one does and does not prove.

**Move to teach.** For “exact commands”, walk from contributor rules to actual CI steps and then into the gate implementation. For each command ask which inputs it exercises and what would remain untested after a pass. Request the actual patch for a change-specific selector.

**Reading note.** Run drift checks and follow the route’s actual contract mappings. Regeneration depends on what changed. Retention-inventory work depends on a new retained table or surface. A policy check on migrations and a populated predecessor upgrade establish different things.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** None are literal. The harness file supplies them:
`.github/workflows/server-ci.yml`, `server/docs/guides/validation.md`, the
gate catalog files, `gates.ps1`, `test.ps1` and the per-component table of
checks.

1. **Turn 1, three calls.** List the headings of the validation guide, read
   `CONTRIBUTING.md` and Grep the job and step names of the server
   workflow. *Got:* the gates table per component, the rule that a served
   route missing from the API reference fails a test, the additive
   migration rule, and the ordered CI steps including an OpenAPI drift
   check, a gRPC reference drift check and a migration check.
2. **Turn 2, two calls.** Read the validation guide (`:1-200`) and the
   workflow steps (`:140-267`). *Got:* the local commands, the exit-code
   meanings, the statement that database tests which return early without a
   database URL are not evidence, and the exact commands of each CI step.
3. **Turn 3, four calls.** Glob `server/tools/*`, Grep the step identifiers
   in the gate catalog, read the documentation coverage test's header
   (`docs_coverage.rs:1-70`) and read the pull request template. *Got:* the
   catalog's step names, what the route-coverage test matches and what it
   cannot, and the checks the template asks the author to tick.
4. **Turn 4, four calls.** Grep the retention and hygiene tools in the gate
   scripts, read the header of the generated gRPC file, read the retention
   checker's header and read `gates.ps1`. *Got:* the generated file names
   its generator, `scripts/generate_server_api.py`. The retention checker
   states that it checks declarations and not runtime erasure. The local
   gate profile lists every step in order.
5. **Turn 5, three calls.** Grep the workflows for the generator and the
   hygiene scripts, read the generator's header and Grep for a closed route
   table in the MCP tests. *Got:* the client workflow runs the generator in
   check mode, so a new route needs regenerated gRPC and client stubs. The
   root hygiene workflow runs the private-material scan, the link check and
   the license gate. MCP exposes a closed list of routes.
6. **Turn 6, one call.** Grep the retention checker's name over the tree.
   *Got:* its two commands in `docs/ops/retention-inventory.md:130-131` and
   a Rust test that runs it. A new table must be declared there.

**Context in hand.** For a route plus a migration: formatting through the
catalog entry point, both Clippy steps, the workspace tests with and without
a database, the route and link coverage test, OpenAPI regeneration and its
drift check, the generator check for gRPC and clients, the gRPC reference
drift check, the additive migration check and what it cannot prove, the
retention inventory check for the new table, the conformance tiers, the
root hygiene checks, and the template's requirements.

**Stop.** The list is complete against CI, the local gate profile and the
template. **6 turns, 17 calls.**

</details>

<a id="munarium-a1"></a>

#### Munarium A1 · How a collection-filtered search gets its results

> Trace a `POST /v1/search` request that filters on one collection, from the moment it reaches the REST listener to the ranked hits and provenance envelope it returns, naming each component it passes through and what each one decides. Assume the default PostgreSQL retrieval mode and a collection with no inference stages.

**Move to teach.** Hold the task’s configuration fixed while tracing the request. At each dispatch, read the predicate that selects the next component and identify the data passed forward. Trace ranking, the provenance envelope and optional diagnostic disclosure as separate results.

**Reading note.** Include deployment-dependent listener admission/capture when applicable. No optional inference stages still permits the PostgreSQL retrieval path to use built-in hash vectors. The provenance envelope and opt-in execution detail have different conditions.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** `POST /v1/search`, "REST listener", "provenance envelope",
"PostgreSQL retrieval mode", "no inference stages".

1. **Turn 1, two calls.** Grep the route string and search function names
   in server code, and Grep the middleware layers. *Got:* the route is
   registered at `rest.rs:1528` to a handler at `:1258-1342`. Two
   router-wide layers are applied at `rest.rs:1889-1896`.
2. **Turn 2, three calls.** Read the handler (`rest.rs:1236-1347`), the
   layers (`:1876-1905`) and the server start-up (`main.rs:396-445`).
   *Got:* the handler's decisions in order: the filter shape, exactly one
   collection, the caller's identity and query scope, the collection lookup
   by identifier and then by name, the clearance check that answers "not
   found" for a collection the caller may not see, the query preparation
   and the governed search.
3. **Turn 3, three calls.** Grep the function definitions the handler
   calls, Grep the REST listener in `main.rs`, and Grep the module comments
   of the two middleware files. *Got:* the listener at `main.rs:325-352`,
   the capture middleware that enforces the user identity header and
   records the interaction, the mutual-TLS admission layer, and the
   coordinator crate `munarium-retrieval`.
4. **Turn 4, three calls.** Read the coordinator
   (`munarium-retrieval/src/lib.rs:384-565`), the access helper
   (`rest.rs:104-149`) and the disclosure helper (`:1196-1237`). *Got:*
   the query is prepared once. The dispatch checks the scope is readable,
   then serves from PostgreSQL unless the mode routes the scope to the
   datastore.
5. **Turn 5, three calls.** Grep the governed search with context
   (`inference.rs:371-433`), read the PostgreSQL search
   (`munarium-retrieval-pg/src/collections.rs:1196-1405`) and the result
   types (`munarium-core/src/retrieval.rs:20-99`). *Got:* with no plan for
   the collection the governed search goes straight to the plain dispatch.
   The index version is the active one unless the caller pins one.
6. **Turn 6, two calls.** Read the two legs (`collections.rs:1404-1573`)
   and Grep the mode type and its default. *Got:* a lexical leg that ranks
   by matched terms with an OR query and optional runbook demotions, and a
   vector leg in its own transaction with its search settings. PostgreSQL
   is the default mode and the fallback for an unrecognized value.
7. **Turn 7, one call.** Read the fusion and envelope
   (`collections.rs:1574-1683`). *Got:* reciprocal-rank fusion, truncation
   to the requested count, an envelope that names the chunks, sources,
   content hashes, index version and watermark, and a final check that
   every cited source is still readable.

**Context in hand.** The whole path with a line range for each component
and decision: listener, admission, capture, handler, access, collection
lookup and clearance, vocabulary expansion, query preparation, governed
dispatch, version resolution, two legs, fusion, envelope, retention check
and disclosure.

**Stop.** The chain is unbroken from socket to response. **7 turns, 17
calls.**

</details>

<a id="munarium-a2"></a>

#### Munarium A2 · Changing the default PDF extractor

> We're thinking of making isolated PDF extraction the default instead of inline. What would that change for existing deployments (index identities, which documents end up indexed, builds, paid escalation, hosts that are not Linux), and what would have to change alongside it?

**Move to teach.** For a default change, compare unset, explicitly overridden and already-persisted states. Follow the configuration to identity, build outcomes and activation. Classify successful, deferred, failed and chargeable cases separately, then inspect platform-specific constraints.

**Reading note.** The identity change affects builds bound to the changed extractor set. A published index keeps serving until a verified successor is activated. Resource limits and escalation policy differ by platform and failure class; a measured fixture does not establish fleet-wide impact.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "isolated PDF extraction", "default", "inline", and the five
named impact areas.

1. **Turn 1, two calls.**
   - Grep `isolat`, case-insensitive, count mode. *Expect:* one extraction
     module. *Got:* 635 hits in 134 files. The word also covers transaction
     isolation, tenant isolation and test isolation. Too broad to follow.
   - Grep the setting family by shape (`MUNARIUM_` with EXTRACT, PDF or
     ISOLAT), only-matching. *Got:* `MUNARIUM_PDF_EXTRACTION` and eight
     companions. The parse site is `config.rs:589-690`. The operator guide
     is `docs/guides/document-intelligence.md:167-249`. There is a
     measurements document and an upgrade guide.
2. **Turn 2, three calls.** Read the parser (`config.rs:576-699`), the guide
   section (`document-intelligence.md:150-261`) and the upgrade guide
   (`server-1.5.md:60-99`). *Got:* inline is the default and equals the
   previous release. The guide answers four of the five areas in prose:
   the extractor identity changes, the extracted text is the same when
   extraction succeeds, a status table lists the new outcomes, and
   escalation and deferral each have a policy setting.
3. **Turn 3, three calls.** Read the module
   (`munarium-extract/src/isolated/mod.rs:1-200`), Grep the identity strings
   over code and fixtures, and Grep the measurements document for its
   headings and the words provisional, frozen and default. *Got:* the
   identity includes the operating-system mechanism and four limits
   (`mod.rs:123-138`). The mechanism is `rlimit` on 64-bit Linux and `none`
   everywhere else (`:187-193`). Tests pin the default identity string in
   three places.
4. **Turn 4, four calls.** Read the registry version function
   (`munarium-extract/src/lib.rs:386-431`), the deferred-document handling
   (`munarium-retrieval-pg/src/lib.rs:222-313`), the measurement findings
   (`pdf-isolation-measurements.md:95-146`) and Grep the deployment files,
   changelog and decision ledger. *Got:* the sorted extractor identities
   join the index identity, so every index rebuilt after the flip is a new
   index. A skipped deferred document is listed in the manifest so a
   rebuild does not reuse that version. The findings say CPU is the
   tightest real limit and that a document refused for it is indexed with
   no text under the default policy. Kubernetes limits and request latency
   under extraction load are recorded as not yet qualified. No deployment
   file sets the variable, so a default flip reaches every deployment that
   relies on the default.
5. **Turn 5, two calls.** Grep the configuration enum's uses and read the
   registry builder (`state.rs:318-357`). *Got:* three configuration tests
   assert the inline default (`config.rs:1109-1123`). The builder resolves
   the server's own executable at start-up and fails start-up if it cannot.

**Context in hand.** Index identity: how it is formed, that it differs
between Linux and other hosts and with each limit, and that existing indexes
keep serving until a rebuild is verified and activated. Indexed documents:
the outcome classes and the measured documents at risk. Builds: the skip or
fail policy, the manifest member and the error. Paid escalation: which
classes escalate under which setting. Other hosts: no memory or CPU limit
and a different identity. Alongside: the default and its tests, start-up
behavior, five documents that state the default, the frozen limits and their
open qualification items, and the two recorded maintainer decisions.

**Stop.** Each impact area has a code source and a document source. The
recommendation itself is the reader's decision. **5 turns, 14 calls.**

</details>

<a id="munarium-a3"></a>

#### Munarium A3 · A session-discovery tool for an agent that lost its ID

> We want MCP agents to be able to list their own sessions, so that an agent which has lost its context can find the session it had open. Where does that tool belong, how does it relate to the existing tools and native routes, and what rules does a new MCP tool have to satisfy?

**Move to teach.** For “list my own”, inspect selection, count and pagination before reusing any existing list. Derive authority from the caller. Test the reasoning with another user in the same tenant and with a narrower renewed credential for the same user. Check whether a narrower recovery operation already helps.

**Reading note.** The administrative session page filters by tenant. Rows, totals and cursors all need current caller authority. Recovery of a known open-operation key covers part of the lost-context use case. The pinned server/contract directory contains multiple contract subtrees as well as its README; a path-limited listing missed them.

<details><summary>OpenAI · executed walkthrough and handoff</summary>

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

</details>

<details><summary>Claude (Anthropic) · executed walkthrough and stopping point</summary>

**Anchors.** "MCP", "list their own sessions", "existing tools", "native
routes", "rules".

1. **Turn 1, three calls.** Outline `mcp_tools.rs` by Grep (tool names,
   route templates, constants, module comment), list the headings of the
   three MCP documents, and Grep the session routes and operations in five
   server files. *Got:* a closed catalog of eleven tools at
   `mcp_tools.rs:78-224` and a separate route allowlist at `:228-240`. The
   design document has sections for design rules, the tool contract and
   dispatch boundaries. There is no native route that lists a caller's
   sessions. The only listings are a management report and a console page
   function.
2. **Turn 2, three calls.** Read the catalog (`mcp_tools.rs:24-245`), the
   design rules (`mcp-analysis.md:1-30`) and the tool contract and dispatch
   boundaries (`:68-157`). *Got:* three handler kinds exist beside routed
   dispatch, all dedicated bounded reads. The seven design rules. Reports
   are excluded from MCP because they include other users' activity. Opening
   a session is a keyed tool whose recovery form already returns the
   session for the caller's operation key.
3. **Turn 3, four calls.** Read the implementation section
   (`mcp-analysis.md:156-225`) and the deferred and decision sections
   (`:336-381`), Grep the handler dispatch, the catalog count and the
   session helpers, and Glob `server/contract/`. *Got:* the durable
   admission rule applies to every tool before it reads data. Filtered
   listings must authorize before ordering, counting and paging, and bind
   the cursor to the caller. A test pins the catalog at eleven. The
   contract folder holds only a README in this checkout.
4. **Turn 4, four calls.** Read the session authority check
   (`mcp_tools.rs:1540-1609`), the console listing
   (`sessions_api.rs:1972-2033`), the closed-table test
   (`mcp_tests.rs:912-963`) and the operator's tool table
   (`docs/ops/mcp.md:130-173`). *Got:* the authority rule a listing must
   apply per row: same user, runbook still allowed, and the live token
   still covers the clearance the session was opened with. The console
   query is tenant-wide and cannot be reused unfiltered.

**Context in hand.** Where it belongs: a fourth dedicated filtered read in
the compiled catalog, with the query scope, not keyed and read-only. How it
relates: no native equivalent exists, the management report is deliberately
out of reach, the per-session authority check already exists, and
operation-key recovery on the open tool covers part of the lost-context
case. The rules: the seven design rules, durable admission, filtering before
paging, fixed metadata, the result size bound, the closed-table and parity
tests that pin the count, the operator and design tables that list the
tools, and the contract process for a new wire shape.

**Stop.** Placement, relation and rules are each cited. **4 turns, 14
calls.**

</details>

<details><summary>Claude’s cross-task observations, tool counts and read sizes</summary>

### 1.5 What the 20 traces have in common

| Task | Kind | Turns | Calls | The step that unlocked it |
| --- | --- | --- | --- | --- |
| Munarium C1 | coding | 8 | 23 | Count search for the variable prefix named one owning file |
| Munarium C2 | coding | 6 | 19 | Reading the sweep next to the single refusal site |
| Munarium C3 | coding | 5 | 14 | Bare-number search, after a prose search missed the server |
| Munarium W1 | writing | 8 | 20 | A figure caption supplied the code's own word for the feature |
| Munarium W2 | writing | 7 | 20 | One module comment that names the path as the governance path |
| Munarium T1 | test | 6 | 18 | Searching the tests for the transport seam found a second test file |
| Munarium T2 | test | 6 | 17 | The harness file's pointers to the workflow and the gate scripts |
| Munarium A1 | architecture | 7 | 17 | The route string, then following each called function |
| Munarium A2 | architecture | 5 | 14 | Searching by the shape of the setting's name |
| Munarium A3 | architecture | 4 | 14 | Outlining the tool catalog file by pattern |
| Bookbuilder C1 | coding | 6 | 19 | Searching for the rarest sibling to list every registration site |
| Bookbuilder C2 | coding | 4 | 10 | Translating the quoted message to its exception type |
| Bookbuilder C3 | coding | 6 | 19 | Searching for the decrypt call listed all eight readers |
| Bookbuilder W1 | writing | 5 | 15 | The quoted interface label |
| Bookbuilder W2 | writing | 4 | 13 | Reading the document and the resolver side by side |
| Bookbuilder T1 | test | 4 | 12 | The style property name led to profiles, formatter and tests |
| Bookbuilder T2 | test | 3 | 10 | The test script, and a tracked-file listing for the absence |
| Bookbuilder A1 | architecture | 6 | 16 | The action name that both sides of the bridge share |
| Bookbuilder A2 | architecture | 4 | 10 | Three parallel censuses: runtime, build and documents |
| Bookbuilder A3 | architecture | 5 | 12 | The build script and a search for the catalog count |
| **All 20** | | **109** | **312** | Mean 5.5 turns and 15.6 calls per task |

By tool: 154 Grep calls, 141 Read calls, 12 Glob calls and 5 read-only Git
listings. Four calls failed or were wasted through my own pattern or scope
errors, and several more returned nothing, which was itself the answer.

Eleven observations hold across the set. Section 2 turns each into a
capability to look for in AIRK.

1. **The first turn is a census, and it is exact.** In 16 of 20 tasks the
   first turn counted or listed every occurrence of a token or pattern
   over the whole tree or a whole component. Ranking played no part. I
   wanted the complete set and its distribution across files.
2. **Half the evidence is found by pattern, and the other half is read by
   address.** Search calls and read calls are nearly equal in number.
   Almost every read used a path and line number that a search had just
   returned.
3. **I read ranges of 20 to 200 lines, and whole files when they are under
   about 600 lines.** A function, a test or a document section is the unit.
   A passage of a few hundred bytes would not have been enough for most
   read steps.
4. **The decisive facts were often not near the prompt's words.** The cause
   in Munarium C2 is an interval expression. In Munarium C3 it is a bare
   number. In Bookbuilder C2 it is a missing case in a switch. In
   Bookbuilder T1 it is two style values with no case. None of these lines
   shares a content word with the instruction.
5. **Completeness claims need exhaustive, scoped search.** "Every place"
   appears in five instructions. I closed each set with two or three
   independent searches and reported the scope searched.
6. **An absence is a finding.** No server test of the cap, no test of idle
   expiry, no backend test of the handler, no CI workflow, no native list
   route, no removal page in the client guides. Each needed a search whose
   empty result I could trust, over a scope I could state.
7. **The repository's vocabulary differed from the user's in four tasks.**
   "Retire" is "removal" in code. "Isolated" is one setting among 635 uses
   of the word. A quoted message is a framework exception type. A limit of
   500 is a bare number. I pivoted through a document, a comment, the
   shape of a name or my own knowledge of the framework.
8. **Sibling search finds registration sites.** To add a ninth budget or a
   seventh provider, I searched for the eighth and the sixth. The result
   was the list of places a new member must be added.
9. **Tests, generators and checks are found through non-code files.** CI
   workflows, PowerShell scripts, a JSON gate catalog, project files, a
   sample list, SQL migrations and a pull request template carried the
   answers to the test and architecture tasks.
10. **Structure was recovered by pattern, not by a parser.** I outlined
    files by searching for headings, function signatures and test
    attributes. It failed twice on signatures that wrap across lines.
11. **The harness file gave the starting point in six tasks** by naming the
    workflow, the scripts, the registries and the rule a change must keep.

The 141 reads covered about 11,400 lines: a mean of roughly 80 lines per
read and 570 lines per task. These are counts of requested line ranges, not
measured bytes.

</details>

<a id="capabilities"></a>

## 2. What the available operations support

The two accounts agree most closely on what happens *after* an address is known. A candidate range becomes useful when an agent opens it and follows a particular branch, consumer or assertion. Their starting advantage was a repository-wide inventory and flexible literal search. Claude started by counting or listing matches across a tree or component in 16 of 20 tasks. OpenAI typically began by finding files that matched the prompt, then read the owner and a contrasting case. Neither result establishes that every request should pay for a full scan.

| Agent move | What AIRK can use today | Where the comparison breaks |
| --- | --- | --- |
| Find a known path, symbol or token | List Files pages the indexed path inventory by prefix. File Info identifies the retained artifact. Symbols finds written definitions. References pages exact-case whole-identifier matches, including textual mentions. | A path pattern, a punctuation-bearing action name, a word family and “all occurrences in the checkout” have different semantics. Indexed emptiness is bounded by selection, eligibility, retention and scan coverage. |
| Find relevant material when wording differs | Search can retrieve lexical, vector or hybrid candidates. Document Facts and Outline can surface a heading, configuration term or a named decision. Code Facts, Imports and Relationships can suggest an owner or edge where supported. | Explicit lexical search uses stable-ID ordering. Punctuation is tokenized. Structural edges can be syntax candidates, and embedded languages, runtime dispatch and SQL can lack a usable edge. Read both ends of a proposed relationship. |
| Open the complete deciding unit | Read File returns a bounded indexed range. Context expands around one anchor. Outline or symbol locations can guide a follow-up. Saved-source verification can check whether selected bytes still match. | Verification supplies an observation about the saved file. It does not fetch changed content. Include a full condition and its else/default or an assertion with its setup. Honor original document locations, byte clipping, revision and refusal. |
| Parse something with structure | Native extraction and document/code facts can expose supported structure. The public Parse operation composes bounded AIRK reads. | The ordinary harness also read JSON, project files, SQL and scripts directly. Parse is not a general-purpose parser of arbitrary live files. A Git tree, supplied diff, current vendor specification, generated DOCX or runtime receipt needs its own authority and identity. |
| Decide which claim survives | The kernel can offer evidence-backed next actions and retain citations. The present typed loop includes Search, Symbols, Definitions, References, Imports, Outline, Read File, Document Sections and Context. | The wider public catalog includes Code Facts, Document Facts and Relationships. Availability to a client and availability inside the loop are separate. A backend test name, matched route string or generated declaration is a lead until a range shows what it does. |

The OpenAI crosswalk classifies 101 steps qualitatively. Claude’s crosswalk classifies 109 turns: 47 fit with one or two existing reads, 51 fit partially, and 11 have no equivalent. Its 20 first turns comprise 13 partial and seven unavailable. These are judgments of fit for the recorded move. They are neither native replays nor measured wins. The complete task tables below retain both accounts’ individual assessments. [Native loop types](../../crates/airk-core/src/arbiter/answer_loop.rs) · [Read dispatch](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Structural evidence](../code-facts-and-relationships.md) · [Saved-source verification](../source-verification.md)

**What the installed probes established.** Claude reports 12 read-only calls to the installed 1.8.206 service. Its Bookbuilder index covered a live folder of unknown revision. It had no Munarium index. A page of five References lines from three files reported six as a lower bound; the separate case-insensitive Grep found 329 matches in 61 files. The scope, matching rule and page size differ. Numeric token lookup worked, a dotted action was refused, Symbols found a named method, Code Facts returned test declarations, and Document Facts found eight matching headings. A prefix listed 36 prompt-folder paths. An empty `.github` listing carried no absence guarantee. A read through line 131 of a 130-line file was refused; a 300-byte source allocation cut a read mid-line without a cursor, producing a roughly 2.9 KB reply. Search and Relationships refused the selected scope on that connection. Those last two calls give no evidence about result quality on the evaluation candidate.

**Scope for absence.** The pinned audits found 134 exported Bookbuilder paths and 15 Munarium paths absent from the respective indexed inventories. A completed retained-text scan may establish that a literal was absent from its eligible bytes. Establishing which files Git tracks requires an authoritative tree or manifest, and runtime behavior needs an observed run. [Bookbuilder corpus audit](windows-index-research-bookbuilder.md) · [Munarium corpus audit](windows-index-research-munarium.md)

### Crosswalk by task

The tables use the original OpenAI step numbers and Claude turn numbers independently. **Files** means indexed inventory and metadata; **Names** means the available symbol, reference, import, code-fact and relationship readers; **Docs** means document facts and outline; **Read** means retained range or context; **Raw** means separately authorized ordinary file tools. The gap names in the right column translate the two reports’ different numbering schemes into plain language. A fit is an assessment about today’s available reads, including where a caller must orchestrate several. A claim of absence in a preserved mapping inherits the scope limits above.

<details><summary>Bookbuilder C1 · 6 OpenAI steps and 6 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Find an existing hosted provider that crosses the named user journeys. | Search named providers, then Files for settings, provider and model paths. | Raw literal/filename results expose sibling implementations across languages. Check which hits actually ship (inventory and exact lookup). |
| OpenAI step 2 | Find a comparable provider wired into both the interface and service. | Names for the enum/factory plus Read of the Settings component and DI registration. | Vue registration needs a string or component read. A C# provider edge alone misses the UI (cross-boundary evidence). |
| OpenAI step 3 | Follow a save and a test separately to find out which owns the key. | Search the exact bridge actions and Read the relevant library and handler branches. | Match temporary and saved keys to their respective paths. Raw scoped literal lookup helps if the punctuation in the action is lost (inventory and exact lookup, cross-boundary evidence). |
| OpenAI step 4 | Find the source of model choices and the adapter that a selected model reaches. | Files and Read for model-list handler, capabilities and UI mapping. | Resolve built-in versus user-selected models by reading the join and its callers (cross-boundary evidence). |
| OpenAI step 5 | Check the actual network and test seams before deciding what can be reused. | Names and Read for the adapter, host registration and test doubles. | A read gives the intended wire shape. Mock dispatch and an observed vendor exchange are different proof levels (proof limits). |
| OpenAI step 6 | Verify the vendor contract outside the pinned application. | Corpus search can locate a checked-in vendor specification if the granted collection contains one. | This walk opened the official vendor documentation separately. Record its origin/date or request the specification; an account’s model entitlements remain unknown (structured or external source, proof limits). |
| Claude turn 1 | Case-insensitive sibling census with counts, in and outside the source tree | References in each spelling, paged, with lower-bound counts | No · complete counts, spelling variants |
| Claude turn 2 | Four whole-file reads | Symbols, then Read File | Yes |
| Claude turn 3 | List every enumeration site in three scopes | References per spelling and prefix | Part · complete counts, spelling variants |
| Claude turn 4 | Two reads, key-path lines in one provider, a generator's head | Read File and References | Yes |
| Claude turn 5 | Two names, two reads, a union type pattern | Symbols and Read File | Part · patterns |
| Claude turn 6 | One range read | Read File | Yes |

</details>

<details><summary>Bookbuilder C2 · 5 OpenAI steps and 4 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Use the button text and symptom before assuming the exception originated in the SDK. | Search the symptom or UI labels and list plausible onboarding/connection files. | Raw literal search is useful when the exception text occurs only in the framework (inventory and exact lookup). |
| OpenAI step 2 | Find the button handler and the precise payload sent from first run. | Read the view around testConnection and Search the exact action string. | Bridge routing is string-based. Verify the payload field and its value at the receiving end (cross-boundary evidence). |
| OpenAI step 3 | Trace this particular input to the earliest operation capable of throwing. | Read the enclosing branch and the entire default-model switch. | Read beyond the thrown default to compare the temporary and saved paths. The order selects the failure before HTTP (cross-boundary evidence, delivery and use). |
| OpenAI step 4 | Check whether a missing registration or lack of HTTP support is a competing explanation. | Names plus Read for the factory, DI registration and Gemini adapter. | Use those independent passages to rule out two alternatives; the existence of an HTTP adapter alone proves neither invocation nor failure (proof limits). |
| OpenAI step 5 | Find a repository-approved model candidate and a test that reaches the ephemeral branch. | Search test/model names, read setup and assertion and compare against the failing input. | The missing fixture is a reasoned test gap. Demonstrating the exception and its correction requires an executed bridge test (proof limits). |
| Claude turn 1 | A button label, an exception type, the bug documents | References for the type and the camel-case name. The label is a phrase | Part · literal strings, vocabulary pivot |
| Claude turn 2 | Two reads, the handler names, the dotted action name, model identifiers by prefix | Read File and References. The action name is refused | Part · literal strings, spelling variants |
| Claude turn 3 | Regression tests named for a handler | None | No · path patterns |
| Claude turn 4 | List the bridge test folder | List Files | Yes |

</details>

<details><summary>Bookbuilder C3 · 5 OpenAI steps and 6 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Identify where generation, secrets and retries meet, starting with the terms in the request. | Files, Search and References for decryption, provider, retry and run terms. | For “every place”, page eligible data and compare with the authorized source roster. Ranked hits omit readers (inventory and exact lookup). |
| OpenAI step 2 | Inventory every candidate reader before choosing a cache owner. | References and Code Facts list candidates; Read distinguishes senders from storage/UI. | A direct scoped scan can include wrappers, tests and admin calls. Each match needs classification and a stated search boundary (inventory and exact lookup, cross-boundary evidence). |
| OpenAI step 3 | Locate the lookup relative to an actual retry or secondary request. | Read the retry callback, lookup and secondary transport; inspect available call edges. | Trace the credential across callback boundaries and compare siblings. An edge cannot prove the number of decryptions (cross-boundary evidence, proof limits). |
| OpenAI step 4 | Find a scope that survives a stage boundary but ends when execution ownership returns. | Read execution/resume, per-stage construction and disposal together. | A lifetime/parallelism judgment needs multiple cited sources. The harness can later count calls under concurrent ownership (cross-boundary evidence, proof limits). |
| OpenAI step 5 | Check what would leak or change semantics if every reader used the same cache. | Read request DTO, embedding and administrative paths, plus masking and logging. | Checking all relevant read sites and preserving custody requires a classified inventory. Keep sensitive values outside the evidence (inventory and exact lookup, proof limits). |
| Claude turn 1 | Every decrypt call and key column outside tests, and the written rules | References with an exclusion prefix, and Document Facts | Part · complete counts |
| Claude turn 2 | Four reads and the ambient scopes | Read File and References | Yes |
| Claude turn 3 | One read, call sites of two names, a count in tests | Read File and References | Part · complete counts |
| Claude turn 4 | Files sharing a name prefix, two reads | List Files and Read File | Yes |
| Claude turn 5 | Acquisition sites and two reads | References and Read File | Yes |
| Claude turn 6 | Two reads | Read File | Yes |

</details>

<details><summary>Bookbuilder W1 · 5 OpenAI steps and 5 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Look up the author’s wording and near-synonyms in UI and help. | Search the author’s wording, retrieve UI/help candidates and list matching files. | If phrasing changes, search shorter literals or the action observed on a control (inventory and exact lookup). |
| OpenAI step 2 | Find both entry points and the current help destination. | Read the import and Generate views and help text at their source locations. | Verify both launch points. Search results favoring one screen cannot establish the other (cross-boundary evidence). |
| OpenAI step 3 | Read what an author agrees to at import, including cost and prerequisites. | Read the checkbox, prerequisite, message and follow-up handler ranges. | Keep cost wording and the deferral option together so the writer can qualify the advice (delivery and use). |
| OpenAI step 4 | Check what the paid work consumes and leaves behind. | Names and Read for the preparation service, eligibility and persisted outputs. | Compare what is created with what the author can see. Raw reads help if an exported artifact lacks retained text (structured or external source, proof limits). |
| OpenAI step 5 | Follow completion all the way to creating a linked book and locating the help article. | Read readiness controls, the wizard and help registration. | Join the displayed state to the next enabled action. The author needs a short journey with the cited UI states in order (cross-boundary evidence, delivery and use). |
| Claude turn 1 | A quoted interface label, two words in the help folder, the folder's files | Lexical Search on the label's words, References per word, List Files | Part · literal strings |
| Claude turn 2 | Four range reads | Read File | Yes |
| Claude turn 3 | A service outline, two reads, four action names | Outline, Read File and References | Yes |
| Claude turn 4 | Three range reads | Read File | Yes |
| Claude turn 5 | Constraints asserted by one test file | Read File | Part · patterns |

</details>

<details><summary>Bookbuilder W2 · 5 OpenAI steps and 4 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Locate the sources a new contributor would find under the prompt’s vocabulary. | Files and Search for template stores, prompt resolution, hydration and architecture documents. | Many docs share a name. Carry path/product and revision into every read (inventory and exact lookup). |
| OpenAI step 2 | Find template definitions, resolution and hydration as separate concerns. | Read registration and fallback code, aided by Symbols/Outline. | A default provider is one layer. Identify who calls it before making a precedence claim (cross-boundary evidence). |
| OpenAI step 3 | Walk a project override, a selected pipe and an ordinary library lookup. | Read both resolver bodies and the winning/absent override branches. | Record which input makes each precedence result true. A flat list of templates loses the decision (cross-boundary evidence, delivery and use). |
| OpenAI step 4 | Read the actual interpolation order and find a production caller. | Read hydrator and real stage caller; Names/References help connect them. | Follow async and sync separately, and show what happens after an unresolved token reaches the caller (cross-boundary evidence). |
| OpenAI step 5 | Compare the written explanation with the branches just inspected. | Docs/Outline and Read for the exact architecture claims. | Compare the same cases in the document and code. Deliver both ranges and identify the contradiction (proof limits, delivery and use). |
| Claude turn 1 | List a folder, find a document's prompt headings, count a word | List Files and Document Facts | Part · complete counts |
| Claude turn 2 | Reads of 136 and 194 lines | Read File with a raised limit | Yes |
| Claude turn 3 | Outline a store and three reads | Outline and Read File | Yes |
| Claude turn 4 | Three reads and a count of call sites | Read File and References | Part · complete counts |

</details>

<details><summary>Bookbuilder T1 · 5 OpenAI steps and 4 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Search by the output format and requested feature, without assuming a style enum. | Search the output terminology, then list plausible profiles, formatter and tests. | Find the runtime export and the export tests; unrelated sample DOCX files are weak leads (inventory and exact lookup). |
| OpenAI step 2 | Inventory shipped profiles, then locate the existing DOCX inspection harness. | Read built-in profiles, inspect the tests’ entry points. | Inventory all shipped profile values and preserve duplicates as distinct profile mappings. A top-k list is insufficient (inventory and exact lookup). |
| OpenAI step 3 | Trace profile selection into the body writer and distinguish it from the table of contents. | Read chapter loop, body writer and existing paragraph assertions. | The retained C# establishes intended output. Checking one generated document would require an authorized fixture and local package/XML parser (structured or external source, proof limits). |
| OpenAI step 4 | Work out boundary expectations independently as literal examples. | Read the formatter branches, numeric boundary and casing helper. | The agent must derive literal expected values independent of the helper, including whitespace/case and paragraph boundaries (proof limits). |
| OpenAI step 5 | Cross-check the two variants sharing a style and identify unrelated modes. | Read the full profile declarations and compare to the formatter’s cases. | Reconcile seven profiles to six values. Present unselected formatter branches and TOC text outside the body test denominator (proof limits, delivery and use). |
| Claude turn 1 | Test files named for export anywhere under the tests, and a count of one property | References for the property | Part · complete counts, path patterns |
| Claude turn 2 | The property with context in two projects, and a file's test methods | References, then Context per match, and Code Facts | Part · match context |
| Claude turn 3 | Three range reads | Read File | Yes |
| Claude turn 4 | Two reads and one attribute in the project files | Read File and References | Yes |

</details>

<details><summary>Bookbuilder T2 · 5 OpenAI steps and 3 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Inventory orchestration tests and conventional automation entry points. | Files can enumerate indexed scripts, test suites and contributor rules. | A raw tracked inventory including hidden paths is the comparator when asking about CI (inventory and exact lookup). |
| OpenAI step 2 | Identify the repository rule and the script responsible for enforcing it. | Docs and Read for governing rules, script switches and smoke roster. | Compare required gate to default script behavior. Both sources belong beside the command (proof limits, delivery and use). |
| OpenAI step 3 | Look past filenames to see which behaviors the suites assert. | Names and Read identify test bodies, callers and the frontend store suite. | Classify by asserted behavior, even if several test classes contain the same method under test (cross-boundary evidence). |
| OpenAI step 4 | Check whether integration means offline or requires a real key. | Read project filters, credential skip predicates and offline/live fixtures. | An Integration category does not establish a live call. Never fetch a real credential to determine a fixture’s prerequisites (proof limits). |
| OpenAI step 5 | Find the trigger as well as the command. Include hidden tracked workflow names in the search. | Read script call sites and inspect indexed workflow paths. | Finish the source inventory before stating absence in that snapshot. External CI settings and run results need separate evidence (inventory and exact lookup, proof limits). |
| Claude turn 1 | Test files named for the pipeline, and whether any workflow or hook is tracked | List Files returned an empty page with a disclaimer | No · path patterns, scoped absence, tracked inventory |
| Claude turn 2 | Two reads, test commands in six named files, a count of one trait | Read File and References | Part · complete counts, literal strings |
| Claude turn 3 | One read, traits and variables in three files, silence in the guides | Read File and References | Part · scoped absence |

</details>

<details><summary>Bookbuilder A1 · 5 OpenAI steps and 6 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Use the visible screen and action words to find where the trace starts. | Files and Search for the Generate screen, pipeline store and bridge. | Source reads distinguish the live app from a similarly named store or mock (inventory and exact lookup). |
| OpenAI step 2 | Find the triggering gesture, payload and literal bridge action. | Read event handler, review action, outbound message and progress callback. | Vue/TS source supplies the bridge key and payload for the next lookup (cross-boundary evidence). |
| OpenAI step 3 | Find sender, receiver, dispatch table and handler for that action. | Search that literal on each side; Read message envelope, router and registration. | Native/C# call candidates cannot bridge Vue’s string-based dispatch. Check both ends against the same action (cross-boundary evidence). |
| OpenAI step 4 | Follow the AI branch instead of assuming every stage makes a network request. | Read actual dispatch, selected stage and example provider HTTP operation. | A candidate factory edge needs case/registration evidence. Distinguish the example provider from other possible branches (cross-boundary evidence, proof limits). |
| OpenAI step 5 | Trace both persistence and the two kinds of return message. | Read persistence, callback delivery and final UI update. | Two paths complete the request: the durable write and correlated return. Deliver evidence for both (cross-boundary evidence, delivery and use). |
| Claude turn 1 | Dotted action names on both sides of the bridge, and the reference document | Symbols by substring for the handler. The action name is refused | Part · literal strings |
| Claude turn 2 | Two reads and several terms in one file | Read File and References | Yes |
| Claude turn 3 | Two registration names and a file's method signatures | References and Code Facts. Parsed definitions also find the wrapped signatures my pattern missed | Yes |
| Claude turn 4 | Four range reads | Read File | Yes |
| Claude turn 5 | One read and a count of two calls across a folder | Read File and References | Part · complete counts |
| Claude turn 6 | Five anchors in one large file and a count of consumers | References per anchor | Part · complete counts, match context |

</details>

<details><summary>Bookbuilder A2 · 5 OpenAI steps and 4 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Inventory platform evidence across different layers before assessing release readiness. | Files/Search find OS strings in project, policies, docs and runtime. | Use exact path filters to separate the shipping application from other products in the tree (inventory and exact lookup). |
| OpenAI step 2 | Read the distribution policy and look for actual runtime branches. | Docs and Read identify policy, branch guards and registrations. | A future distribution policy and wired runtime have different authorities (proof limits). |
| OpenAI step 3 | Verify representative implementations and their selection at startup. | Names/References and Read locate concrete Mac providers and registration. | Resolve platform branches before deciding whether a class is reachable. Source cannot qualify its native behavior (cross-boundary evidence, proof limits). |
| OpenAI step 4 | Follow the publish input through the installer and look for a corresponding Mac packaging path. | Files and Read for MSBuild RIDs, publish scripts, WiX inputs and packaging manifests. | Raw project XML, scripts and a scoped file inventory support the packaging assessment. Absence of a single file extension is inconclusive (inventory and exact lookup, structured or external source). |
| OpenAI step 5 | Compare test scope and prose with the code and release evidence. | Read platform docs and test assertions side by side. | Separate stale interface names from an accurate Windows-only distribution statement. Hardware qualification belongs to a separate run (proof limits, delivery and use). |
| Claude turn 1 | Three censuses with counts: runtime, build files and documents | None | No · complete counts, spelling variants, patterns |
| Claude turn 2 | Matches in a chosen set of files, one document, a count in tests | References per prefix and Read File | Part · complete counts |
| Claude turn 3 | A test's method names, one project file, words in six documents | Code Facts, Read File and References | Part · complete counts |
| Claude turn 4 | Which packaging scripts are tracked, and which are not | None | No · scoped absence, tracked inventory |

</details>

<details><summary>Bookbuilder A3 · 5 OpenAI steps and 5 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Test whether the requested product is genuinely new and find the authoring guide. | Files/Search for the proposed product and maintained authoring roots. | Raw path inventory supplies a decisive counterexample to the premise that the product is absent (inventory and exact lookup). |
| OpenAI step 2 | Find the authoring root and check for the requested subject before choosing an example. | Read the authoring guide and use File Info on discovered manifest paths. | A matching slug is a lead. Establish the exact product identity before inferring a count change (cross-boundary evidence). |
| OpenAI step 3 | Read the required source layout and parse the existing product's contract. | Read retained JSON and guide if present; Outline can locate prose rules. | A plain JSON parse identifies contract keys and nesting consistently. A file absent from retention requires authorized bytes (structured or external source). |
| OpenAI step 4 | Trace a source product all the way into installer inputs. | Read compiler, seed script, Host project and installer inputs. | Follow the same .bbpipe and catalog identity across script/XML boundaries. Files with similar names are insufficient (cross-boundary evidence, structured or external source). |
| OpenAI step 5 | Distinguish enforced invariants, calculated counts and fixed expectations. | Read validator and the actual hard-coded and derived assertions. | Count and compare the two product-identity scenarios. Keep the unresolved product choice visible (proof limits, delivery and use). |
| Claude turn 1 | Product folders grouped two levels deep, one script, the number 46 everywhere | List Files in pages of 100, Read File, References for the number | Part · complete counts, path patterns |
| Claude turn 2 | One product's files and two reads | List Files and Read File | Yes |
| Claude turn 3 | Count the product manifests one level down, and one test's assertions | Read File. No pattern over paths | Part · path patterns |
| Claude turn 4 | Three documents' headings and a builder's validators by name | Document Facts and Symbols by substring | Yes |
| Claude turn 5 | One range read | Read File | Yes |

</details>

<details><summary>Munarium C1 · 5 OpenAI steps and 8 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Search the exact current API name and the language used for the new feature. | Search the supplied route and budget name; Files for vocabulary, store and protocol. | Search in more than one vocabulary if splitting the environment identifier obscures exact spelling (inventory and exact lookup). |
| OpenAI step 2 | Find the current vocabulary call and compare it to another independently budgeted call. | Read the consumer and env/validation registry. | Associate the budget with this particular generation call and keep generic completion as a separate consumer (cross-boundary evidence). |
| OpenAI step 3 | Trace the public shape and default through both transports. | Names and Read for shared DTO, REST operation and generated RPC binding. | Check the generated mapping and actual default. A same-named client field does not establish wire compatibility (cross-boundary evidence). |
| OpenAI step 4 | Read how an old eight-field value is written and loaded before proposing an upgrade. | Read the SQL migration, store and decoder/fallback together. | Raw structured inspection of an authorized old fixture can illustrate the missing field. Preservation of a tenant through upgrade requires an observed compatibility test (structured or external source, proof limits). |
| OpenAI step 5 | Enumerate client representations and examine the current acceptance and refusal tests. | Files, References and Read for each SDK and existing refusal/replace tests. | Check all four clients and distinguish old storage from a new, invalid replacement request. Show that distinction explicitly (inventory and exact lookup, delivery and use). |
| Claude turn 1 | Count a variable prefix, a hyphenated route name and a phrase over the tree | References on one full variable name, paged | No · complete counts, literal strings, spelling variants |
| Claude turn 2 | Read three whole files of 579, 72 and 14 lines | File Info or Outline for the last line, then Read File with a raised limit | Part · unit reads |
| Claude turn 3 | Find the wire type, every use of one field, and the proto entries | Symbols and References | Yes |
| Claude turn 4 | Three range reads and terms inside one document | Read File and Document Facts | Yes |
| Claude turn 5 | Sibling census of the last field name in three spellings | References once per spelling | Part · complete counts, spelling variants |
| Claude turn 6 | Read four client models | Symbols, then Read File | Yes |
| Claude turn 7 | One term in seven named files, the migration list, five mixed files | References per prefix and List Files | Part · literal strings, path patterns |
| Claude turn 8 | Confirm a tool folder says nothing about added fields | None | No · scoped absence |

</details>

<details><summary>Munarium C2 · 5 OpenAI steps and 6 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Search the exact environment setting and error slug independently. | Search both the configuration literal and the error slug, then list matching source. | Literal matching can recover hyphen/underscore variants missed in one query (inventory and exact lookup). |
| OpenAI step 2 | Find where the TTL is read and where the reported error originates. | Read configuration and typed refusal definitions. | The error declaration proves its shape. It does not show when the active turn path uses it (proof limits). |
| OpenAI step 3 | Look at the expiry clock, starting timestamp, polling interval and disabled case. | Read the janitor’s zero, scheduling, clock and timestamp branches. | Carry all timing terms together. One timeout constant alone would misdescribe the race (delivery and use). |
| OpenAI step 4 | Inspect a request that arrives during that interval and locate the write that refreshes activity. | Read the request and activity writes, using References to locate callers. | Trace the order of ownership, admission and refresh. Concurrency semantics need transaction/clock evidence as well (cross-boundary evidence, proof limits). |
| OpenAI step 5 | Check transport entry points, error shape and which existing tests discriminate this defect. | Read transport delegates and test assertions for idle versus closed state. | A typed-error assertion for an already-closed session is a control. A deterministic expired-but-still-open case needs an executing test (proof limits). |
| Claude turn 1 | Find the setting in two spellings and the error slug | References for the names. The slug is hyphenated | Part · literal strings |
| Claude turn 2 | Four range reads | Read File | Yes |
| Claude turn 3 | A column name, the state comparisons, the loader and turn functions | References and Symbols. The comparison is a pattern | Part · patterns |
| Claude turn 4 | Three range reads | Read File | Yes |
| Claude turn 5 | Files naming the slug, state checks in one file, absence of a test | None for the slug or the absence | Part · literal strings, scoped absence |
| Claude turn 6 | One read, the close route, the slug in three documents | Read File and Symbols | Part · literal strings |

</details>

<details><summary>Munarium C3 · 5 OpenAI steps and 5 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Find count and byte claims together, then distinguish their units. | Search count, operation and byte terms, then classify paths with Files. | Raw unit-aware literal scan helps separate limits from port numbers, status codes and unrelated counts (inventory and exact lookup). |
| OpenAI step 2 | Locate both file-count checks and the byte-limit owner using their surrounding nouns. | Read both guards and independent body/manifest limits. | Keep precise units, comparator and owner in each citation (delivery and use). |
| OpenAI step 3 | Follow actual transport bindings and check for a wrapper that cannot issue one operation. | Read generated server dispatch and SDK declarations/implementations. | Verify each operation/transport independently. The public client may deliberately omit an operation present on the native server (cross-boundary evidence, proof limits). |
| OpenAI step 4 | Find each client’s guard and follow the constant through its users. | References and Read for four SDK constants and local validators. | Identifier spelling differs by language. A source inventory, multiple spellings and actual usages make the client matrix defensible (inventory and exact lookup, cross-boundary evidence). |
| OpenAI step 5 | Look for assertions, examples and documentation tied to the old boundary. | Search boundary fixtures, docs and OpenAPI, then Read the relevant ranges. | Large structured specs can need scoped local parsing. Tests for 1,001 small files and oversize bytes establish different claims (structured or external source, proof limits). |
| Claude turn 1 | A number near certain words, constant names by shape, a count of the body ceiling | None | No · complete counts, patterns |
| Claude turn 2 | The bare numbers 500 and 501 in chosen files | References for each number, paged | Part · complete counts |
| Claude turn 3 | Two function names, 501 and an error string over the server, three documents | Symbols and References. The string is a phrase | Part · literal strings, match context |
| Claude turn 4 | One read, a count of four constant names, the number in documents | Read File and References | Part · complete counts, literal strings |
| Claude turn 5 | Every use of the body-ceiling constant | References | Yes |

</details>

<details><summary>Munarium W1 · 5 OpenAI steps and 8 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Translate “retire” into action names used by route docs, storage and SDKs. | Search lifecycle synonyms and route literals; Docs for the guide. | Follow the vocabulary native code uses after discovery to avoid confusing other forms of retirement (inventory and exact lookup). |
| OpenAI step 2 | Discover the product vocabulary and read both state transitions. | Read request, confirm, mismatched ID and expiry branches. | Keep version, ID, expiry and consequence together so a caller can perform the flow (delivery and use). |
| OpenAI step 3 | Read selection, reactivation and listing to distinguish visibility from deletion. | Read selection, listing, changed columns and reapply guards. | A status transition alone cannot establish retention or reactivation. Check affected storage and the guard (cross-boundary evidence, proof limits). |
| OpenAI step 4 | Check the fate of a session that already pinned the version. | Read the stored session ref and the lookup on the next turn. | Join the already-open session to the same version resolver. Two individually relevant excerpts can otherwise suggest the opposite conclusion (cross-boundary evidence, delivery and use). |
| OpenAI step 5 | Verify each client spelling and how the request’s result becomes confirm input. | Names/References and Read for each SDK’s public methods. | Camel case, snake case and Async variants need independent lookups. A language-level example must forward the ID the previous call returned (inventory and exact lookup, cross-boundary evidence). |
| Claude turn 1 | Count a word stem, case-insensitive, and find guide files by pattern | None | No · complete counts, spelling variants, path patterns |
| Claude turn 2 | The stem in one file, in the clients and near two other words | None | No · spelling variants, patterns |
| Claude turn 3 | Pivot to the repository's vocabulary with a phrase alternation | Hybrid Search may rank the guide section | Part · literal strings, patterns, vocabulary pivot |
| Claude turn 4 | Functions and constants by name pattern, and the stem in the guide | Symbols by substring and Document Facts | Part · spelling variants |
| Claude turn 5 | Three range reads | Read File | Yes |
| Claude turn 6 | Method signatures in four languages, absence in client guides, two slugs | Symbols per name | Part · literal strings, scoped absence |
| Claude turn 7 | Four range reads | Read File | Yes |
| Claude turn 8 | Call sites of one constructor and one read | References and Read File | Yes |

</details>

<details><summary>Munarium W2 · 5 OpenAI steps and 7 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Find the command and error from the caller’s own vocabulary. | Search command and error names, then list service/store candidates. | Exclude client stubs when choosing the native entry point (inventory and exact lookup). |
| OpenAI step 2 | Find both entry points and the shared command they call. | Read REST and RPC handlers and their common service call. | Match arguments, optional head and authentication across both transports (cross-boundary evidence). |
| OpenAI step 3 | Read the gate inputs, classifications and the decision to append. | Read gate input, policy, decisions and projection status. | A gate finding and the stored claim status are distinct facts. Read the mapping between them (cross-boundary evidence, delivery and use). |
| OpenAI step 4 | Examine the transaction and the retry decision under a racing writer. | Read transaction, conflict handling, retry ceiling and transport mappings. | Determine who rechecks the head and whether the caller pinned it. Race behavior ultimately needs a competing-writers test (cross-boundary evidence, proof limits). |
| OpenAI step 5 | Read the direct lookup and the default projection using the same hypothetical disputed ID. | Read direct and facts-view projections with each filter condition. | Use the same disputed example for both readers. Pin as-of/supersession qualifiers to the pertinent evidence (delivery and use). |
| Claude turn 1 | Counts for two name families and a document's headings | References per name and Document Facts | Part · complete counts |
| Claude turn 2 | Names in three files, and matches with context | References, then Context per match | Part · match context |
| Claude turn 3 | Reads of 300 and 68 lines | Read File with a raised limit | Yes |
| Claude turn 4 | Outline a file, read the error type, find store functions and a lock statement | Outline and Symbols. The statement is a phrase | Part · literal strings |
| Claude turn 5 | Five range reads | Read File | Yes |
| Claude turn 6 | Four definitions with their bodies | Symbols and Read File | Yes |
| Claude turn 7 | Count a word in the documents | References, paged | Part · complete counts |

</details>

<details><summary>Munarium T1 · 5 OpenAI steps and 6 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Look for production REST transport and tests that actually send a request offline. | Search test names, mock transport and idempotency vocabulary. | Finding a helper does not establish an outbound request. Select the transport-level fixture (proof limits). |
| OpenAI step 2 | Find the actual transports and the closest wire-level tests. | Files/Names and Read locate both Python loops and relevant tests. | A list of similarly named tests does not distinguish sync, async and pure helper coverage (inventory and exact lookup, proof limits). |
| OpenAI step 3 | Inventory what the existing assertions observe. | Read the scripted responder, request recording and assertions. | Classify each observed assertion by send count, exact key and failure type (proof limits). |
| OpenAI step 4 | Read both request loops to make sure a sync test cannot mask an async defect. | Read both loops and the key-generation site before their retries. | Do not infer async behavior or testing from a sync implementation. Connect and post-connect errors are separate cases (cross-boundary evidence, proof limits). |
| OpenAI step 5 | Choose real operation specs, an async fixture and exact observables. | Read real operation specs and pytest configuration. | Construct an independent request matrix. Actual repeated headers, attempt counts and passes need a later execution receipt (proof limits, delivery and use). |
| Claude turn 1 | Count a stem in the Python client and list the test files | List Files for the tests. None for the stem | Part · complete counts, spelling variants |
| Claude turn 2 | One small read, several terms in one file, the test functions of a file | Read File, Code Facts for test declarations | Part · patterns |
| Claude turn 3 | Two reads and every test that uses a mock transport | Read File and References | Yes |
| Claude turn 4 | Two reads, the retry class per operation, test configuration keys | Read File and Document Facts. The class assignment is a pattern | Part · literal strings, patterns |
| Claude turn 5 | Two reads, transient flags, the CI job's commands | Read File and References | Part · patterns |
| Claude turn 6 | One range read | Read File | Yes |

</details>

<details><summary>Munarium T2 · 5 OpenAI steps and 6 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Find the published entry points for pre-PR checks and migrations. | Files and Docs for contributor rules, tests, validation and workflows. | The source inventory accounts for hidden workflow definitions and working directories (inventory and exact lookup). |
| OpenAI step 2 | Find contributor instructions and the working directory assumed by the validation ladder. | Read the guide’s command definitions and their prerequisites. | A command must travel with cwd, environment, tier and required local services (delivery and use). |
| OpenAI step 3 | Read what CI actually invokes for source, tests and the route contract. | Read the checked-in workflow, including OpenAPI generation and comparison. | Raw YAML inspection can recover job defaults, steps and shell context lost when a snippet is clipped (structured or external source). |
| OpenAI step 4 | Find the implementation of the migration check and the code that applies migrations. | Read gate implementations and SQLx migration registration. | Distinguish static policy from executing an actual populated upgrade (proof limits). |
| OpenAI step 5 | Recover the invocation and dependencies for the behavior checks. | Read local tiers and CI comparison logic; Changes may describe indexed changes at an available revision. | The prompt supplies no patch. A named Git base/head or authorized diff and a compatible database fixture are needed for route-specific proof; provide the generic commands meanwhile (structured or external source, proof limits). |
| Claude turn 1 | Guide headings, one whole document, the workflow's jobs and steps | Document Facts and Read File. Step names depend on the workflow adapter | Part · unit reads |
| Claude turn 2 | Reads of 200 and 128 lines | Read File with a raised limit | Yes |
| Claude turn 3 | List a tools folder, step identifiers in a JSON catalog, two reads | List Files, Document Facts for keys, Read File | Part · patterns |
| Claude turn 4 | Several tool names across five named scripts, three reads | References per name | Part · patterns |
| Claude turn 5 | Workflows that run a generator, one read, a closed-table phrase in tests | References and Read File | Part · literal strings |
| Claude turn 6 | Every mention of one checker | References | Yes |

</details>

<details><summary>Munarium A1 · 5 OpenAI steps and 7 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Find the exact route literal and its caller-facing documentation or handler. | Search exact route and Files for the serving component. | The occurrence can be in generated clients and documentation. Read the route registration before choosing an owner (inventory and exact lookup). |
| OpenAI step 2 | Find the listener, registration and handler for this exact operation. | Names and Read connect listener, router and handler. | Read the binding that relates a route string to a function (cross-boundary evidence). |
| OpenAI step 3 | Follow the single collection filter through access, resolution and request preparation. | Read collection resolution, access, tenant and query preparation. | Ground the single-collection branch and its refusal path. Keep ID/name fallback conditions (cross-boundary evidence). |
| OpenAI step 4 | Follow the enabled execution route and read how candidates become ranked hits. | Follow supported Names edges and Read the PostgreSQL implementation and ranking. | SQL and dynamic mode dispatch require source verification. Show which leg can still run without optional inference (cross-boundary evidence, structured or external source). |
| OpenAI step 5 | Trace the returned evidence and the optional execution disclosure independently. | Read provenance construction and REST disclosure separately. | Name the source of each returned field and the condition for optional detail. A rank snapshot would need a live query and data (proof limits, delivery and use). |
| Claude turn 1 | Find a route string, search functions and middleware layers | Symbols for the functions. The route is not an identifier | Part · literal strings |
| Claude turn 2 | Three range reads | Read File | Yes |
| Claude turn 3 | Nine function definitions and two module comments | Symbols and Outline | Yes |
| Claude turn 4 | Three range reads | Read File | Yes |
| Claude turn 5 | One definition with context and two reads | Symbols, Context and Read File | Yes |
| Claude turn 6 | One read and an enumeration with its default | Read File and Symbols | Yes |
| Claude turn 7 | One range read | Read File | Yes |

</details>

<details><summary>Munarium A2 · 5 OpenAI steps and 5 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Locate both possible modes and the operational description of their differences. | Search the two extractor modes; Docs, Files and Read for configuration/owner. | Historical README values and active code can both rank well. Keep revision and authority (inventory and exact lookup, proof limits). |
| OpenAI step 2 | Find the default, overrides and the executable profile actually used. | Read default, override and the active budget profile. | Show units, OS applicability and which settings affect a build separately (delivery and use). |
| OpenAI step 3 | Trace extractor identity into index publication, rebuild and activation. | Read identity construction/guide, build and activation. | Raw inspection of authorized manifests helps check an existing publication. Text describing activation does not activate or measure it (structured or external source, proof limits). |
| OpenAI step 4 | Trace deferral, build failure and paid fallback using their recorded classifications. | Read deferred/failure classification and paid eligibility branches. | Tie each outcome to the inputs and policy that actually allow spending. Do not infer paid fallback from any generic error (cross-boundary evidence, proof limits). |
| OpenAI step 5 | Read the platform split and compare operational documentation against source defaults. | Read worker start, platform guards, profile and operator docs. | A platform-specific test or benchmark needs that host. Preserve established limits and flag old wording beside current source (proof limits, delivery and use). |
| Claude turn 1 | Count a stem, and find a family of settings by the shape of the name | Hybrid Search may rank the guide | No · complete counts, spelling variants, patterns, vocabulary pivot |
| Claude turn 2 | Three range reads | Read File | Yes |
| Claude turn 3 | One read, identity strings with punctuation, a document's headings | Read File and Document Facts | Part · literal strings |
| Claude turn 4 | Three reads, the setting across deployment files, and its absence there | Read File and References | Part · scoped absence |
| Claude turn 5 | Uses of one type and one read | References and Read File | Yes |

</details>

<details><summary>Munarium A3 · 5 OpenAI steps and 4 Claude turns</summary>

| Record | Question or action | Closest read today | Fit or remaining limit |
| --- | --- | --- | --- |
| OpenAI step 1 | Find session-discovery candidates and neighboring protocol tools from the request alone. | Files and Search for target repo’s MCP/session APIs and governance. | The target repository’s protocol source is data for this investigation. Reading it is independent of calling AIRK's own MCP tools (inventory and exact lookup). |
| OpenAI step 2 | Find how tools are listed/called and what session operations the catalog already offers. | Read catalog, protocol dispatch and session page call sites. | Check whether a list-by-owner operation exists or there is only a by-ID read and an administrative list (cross-boundary evidence). |
| OpenAI step 3 | Read the existing list predicate, count and cursor before calling it reusable. | Read SQL rows, count, argument binds, cursor and native routes. | A local SQL read/parse can expose predicates explicitly. A tenant-wide query cannot establish uid isolation (cross-boundary evidence, structured or external source). |
| OpenAI step 4 | Read how an allowed list is authorized, advertised and paged. | Read adjacent bound projections, metadata, paging and scope filters. | Check that rows, totals and cursor use the same authority; the receiving agent must see this difference (cross-boundary evidence, delivery and use). |
| OpenAI step 5 | Check the normative rules and nearby conformance tests before proposing placement. | Docs and Read for normative rules plus test bodies. | Distinguish an existing admission rule from a proposed new read. An observed two-user refusal and paging test would qualify the implementation (proof limits). |
| Claude turn 1 | Outline a catalog by its string fields, three documents' headings, session routes | Outline and Document Facts. Tool names and routes are string literals | Part · literal strings |
| Claude turn 2 | Three range reads | Read File | Yes |
| Claude turn 3 | Two reads, dispatch and count sites, one folder listing | Read File, References and List Files | Part · complete counts |
| Claude turn 4 | Four range reads | Read File | Yes |

</details>

<a id="teaching"></a>

## 3. What to teach Gather Context

The 20 walkthroughs illustrate a policy that must work for any task an agent presents within the authorized scope. For each move, ask **what the agent knew, which question was open, which read could settle it, and what the read changed**. For example, following a registration to the handler that owns a decision and following a handbook reference to the policy that governs an exception use the same rule. Follow a reference found in the evidence when its destination can settle an open question. The current task and the sources determine the next read.

| Signal from the prompt or the latest read | Action worth taking | Evidence that changes the next decision |
| --- | --- | --- |
| A precise path, setting, route, quoted label or rare sibling appears | Go directly to a known address. Otherwise count or list matching paths using the most distinctive literal and reasonable spelling variants. Use a narrow justified scope first. | A few plausible files justify opening the owner. A broad set calls for another distinguishing term or path class. A sparse set calls for a vocabulary pivot. |
| The words describe behavior without naming its implementation | Search docs, comments, help text or tests for that behavior. Carry an observed product term, action or error type back into an exact lookup. | The new term must lead to a range that uses it in the relevant role. A framework-like message with no literal match is an unresolved hypothesis. |
| An artifact appears generated, a component uses a bridge, or dispatch is indirect | Read the declaration or generated-by header and the caller. Follow the observed key to the registration, generator or receiver. Read that owner’s branch. | Both ends agree on the value, revision and relevant input. A mismatch or wrong owner warrants a different candidate. |
| A failure is intermittent, specific to a provider, or depends on configuration | Name two plausible paths. Read a working control, failure branch, writer and reader, or persisted and default case. | Identify the first decision where the paths diverge. Return an observation or regression that would distinguish the explanation from its alternative. |
| The task says “all”, “every”, “automatically” or “no longer” | Define the population and exclusions. Enumerate it a second way, such as by literal and by its constant, or by registration and by implementation. Inspect the missing or extra members. | Close only the population actually covered. Report an unvisited page, ignored path, unretained file, unavailable Git tree or unobserved external automation explicitly. |
| A returned snippet ends in the middle of the relevant decision | Read an enclosing definition, heading or assertion plus setup. Expand across the return/default branch, or follow the original document location. | Record which condition, exception or comparison is now supported. Another nearby paragraph is expendable if it adds no decision-relevant evidence. |
| The task asks for tests, a guide or user-facing behavior | For a test, find the real input/output boundary, fixture and independent assertion. For a guide, follow the visible user journey into the owner and on to the next visible state. | The handoff gives something the caller can do: a command and prerequisites, a literal expected result, or an ordered user action and consequence. |

**Reader changes in order of dependency.** A bounded retained-text search should offer counts per file, paths only, or cited lines with small context windows. Start with literals, case choice, whole-word and prefix matching, and a bounded set of spelling alternatives. Report the eligible and visited files and bytes, match count, cutoff, completeness and revision-bound continuation. Indexed postings may nominate candidates quickly; completeness needs either proven candidate coverage for that match rule or a scan of the full admitted scope. Benchmark representative scans on both publications before setting limits. Claude’s map attributed complete counts to 25 turns, punctuation-rich literals to 20, loose matching to 11 and patterns to 13; the categories overlap.

Path matching and directory totals could make an inventory usable without returning every path. Read-by-unit could offer the enclosing definition, document section or small file, clamp an explicitly permitted range at EOF, and resume at a safe text boundary under a byte limit. Preserve original coordinates and expose oversized lines/records explicitly. Repository instructions can supply useful orientation when scoped and budgeted; treat their content as evidence rather than executable directions. Try existing facts and ordinary retained text for structured input before adding a format-specific parser. A general regex engine, current-file access, Git history, imported semantic graphs, increased depth or new weights each need a demonstrated residual miss and the applicable contract decision.

**Delivery is part of gathering.** Compose a small packet for each open question: the decision-bearing excerpt, its scope and location, the necessary neighboring exception or counterexample, and what remains unverified. Keep the full source trail accessible subject to the same bounds and authority. In long traces, put an easily missed condition next to the conclusion it qualifies. The Windows result motivates a controlled delivery experiment; it does not establish that ordering alone caused the low score.

<a id="request"></a>

## 4. How one request should run

The calling agent supplies its task and authorized source selection. Gather Context returns a cited handoff. Internally the existing kernel already has Split, Plan, Retrieve, Coverage, Follow-up, Compose and Union stages and a shared read owner. The proposed change is to make the latest evidence determine what those stages do next. [Kernel contract](../../crates/airk-kernel/src/arbiter/CONTRACT.md) · [Job ownership](../../crates/airk-service/src/service_runtime/answer_jobs/CONTRACT.md)

1. **Declare the finish condition.** Split into decisions the caller actually needs. Preserve the user’s constraints, target surfaces, quantifiers and requested deliverable. Bind selection, root, publication and total budgets. A genuinely missing artifact, such as an unspecified patch, stays visible.
2. **Locate the owner.** Route known addresses to a direct reader. For concrete anchors, compare counts and inspect candidate files. For unfamiliar phrasing, use lexical/hybrid or document search to discover vocabulary and repeat with an observed token. Select by role, applicability and evidence as well as frequency. The governing guide may be the right owner for a policy question.
3. **Follow a supported lead.** Read enough of the unit to see what it decides. Track each new edge with its source range, target selector and purpose. Prioritize the edge that separates alternatives or closes an unsatisfied clause. Batch independent reads and share a range across clauses. Deduplicate addresses by scope, publication, digest and coordinates.
4. **Check coverage and alternatives.** Reconcile rosters for exhaustive tasks, check an active configuration against an alternate, and distinguish code, documentation, test declarations and observations. Return to a clause only when there is a new admissible lead. Defaults, exceptions and failure paths count as evidence. A keyword occurrence by itself cannot establish a claim.
5. **Compose for the next agent.** Put the action the evidence enables first. Attach ranges for each decisive claim, together with its qualifiers and any conflict. Keep the source, counterexample, proof limit and suggested next read together. Stop on sufficient evidence, a missing input, no further productive authorized action, budget/deadline or cancellation. Partial results identify what remains and why. Continuations stay tied to the original selection and may expire.

**Two different paths through the same code.** In a provider-specific failure, an agent may reach the responsible switch from a UI label or from the exception type. Once it has the payload, switch and contrasting working case, those paths can converge and stop. In a changed file-count limit, finding the first guard opens new work: enumerate the second operation, CLI and SDK checks, distinguish count from bytes and reconcile independent searches. Source density might favor the first guard in both situations. The task’s finish condition determines whether one decisive branch is enough or a population still needs accounting. [Provider example](#bookbuilder-c2) · [Limit example](#munarium-c3)

**Model roles and costs.** Fixed rules should always be able to route a known address, reject a stale or unauthorized read, and return a supported partial. Focus can rank admitted file/section candidates. Arbiter can score grounded pivots or competing next reads. Neither should invent a selector, waive authority or promote a candidate occurrence to proof. The kernel owns execution, deduplication and accounting. Record files/bytes scanned, source bytes retained, model input, serialized bytes delivered, external calls, internal rounds and wall time separately. The current broad part-search allowance reaches 24,576 text bytes; 768 is a multiplier in its budget calculation. Some targeted reads use 3,072 bytes. Retention defaults to 256 KiB and the reply ceiling is 128 KiB subject to the request budget. A human harness’s line count cannot determine which native limit will bind. [Read budgets](completion-plan.md#read-and-delivery-budgets) · [Retrieval recipe](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs)

<a id="implementation"></a>

## Implementation and Testing Strategy

**Assignment for GPT‑6.2 Sol.** Implement a general *search–read–decide* policy for `gather_context`. It takes the calling agent’s instruction and authorized indexes, identifies what the caller needs to decide, and returns cited evidence or a precise account of what remains unresolved. Use both recorded paths on each development task to check the quality and cost of the handoff. For every routing change, describe the observable clue that triggers it and why the next read would help on a different task. Derive search terms, paths and stopping decisions from the current instruction, scope, retrieved evidence and remaining budget. Keep benchmark-specific branches, ranking boosts and prompt fragments out of the production code and model instructions. An unfamiliar deliverable can enter the same loop with an open question drawn directly from the prompt. This section is the implementation plan for review. Product changes, experimental replays and qualification start when the user resumes that work.

### Starting state and choices Sol can make

- Work on `feat/arbiter-final` from the current reviewed head. Use the verified Windows human execution path, the existing warm target, one Cargo owner and the campaign disk and cleanup rules. Reuse the settled Windows publications for Bookbuilder `238b22a7` and Munarium `9948345d`. The branch and source revision are independent identities; record both full source commits and both actual publication tokens. [Current assignment](../agent-current-work.md) · [Windows preparation and custody](completion-plan.md#1-reconcile-the-starting-point)
- Keep the recorded control, index, model assets, enrolled scopes, three-round ceiling, total call/byte limits and deadline fixed for matched retrieval comparisons. Charge orientation, scans, speculative candidates and follow-ups to the same budget. Default to a census **inside round one**. A known path can skip the census. A further round, a new public response shape, relaxing a limit, changing persisted state or granting live-source authority requires its own contract decision. Implement the internal behavior using current contracts where possible; record and defer a blocked optional extension and continue with the rest. [Decision 6 and open proposals](completion-plan.md#decisions-for-the-user-at-this-review)
- Use OpenAI models for implementation, experimental actors and judges. Claude’s prior record is a supplied design reference. Do not start new Anthropic sessions. Treat the section 1 traces, both source documents, labels, graders and run receipts as evaluation-side material. Exclude their contents from indexed inputs, product prompts, examples shipped to actors, and model training. Keep the sealed cases closed. Preserve authenticated uploads/checkpoints and the unchanged control on failures.
- Use **Gather Context** in docs, UI and discussion and **`gather_context`** for the public tool. Locate the public `answer` entry in `crates/airk-mcp/src/combined.rs` and its native `airk_answer` handler in `crates/airk-mcp/src/lib.rs` and route the chosen public name through the same canonical job and kernel handler. Inventory catalog/help, CLI/session, extension/Workbench, schema-generation bindings and frozen harness offers first. Old-name alias/removal and any wire compatibility change need the recorded compatibility decision. Retain the approved `task` input, service defaults, overrides and the compatible full request. Verify each spelling against the real offer and the canonical effective input. Historical fixtures keep their original name and digest. Run the first retrieval experiments against the existing owner if the public naming decision is still being integrated.

### Milestones, code owners and exit evidence

**M0 · Build the reference comparator before changing retrieval.** Extend the maintained evaluation harness and its README. Add a runner (proposed file `scripts/arbiter-answer-eval/gather_context_replay.py`) for the four commands specified below. Reuse the existing scope validator, fixture and session adapters, exact range coverage, deadlines and grading. New evaluation-only fixtures and observed outputs live in the private campaign root. No new crate, service, external provider or persistent index format is needed. [Harness](../../scripts/arbiter-answer-eval/README.md) · [Coverage](../../scripts/arbiter-answer-eval/coverage.py) · [Evidence](../../scripts/arbiter-answer-eval/evidence.py) · [Historical replay owner](../../scripts/arbiter-answer-eval/followup_search_replay.py)

- **Input.** Verify both development source/export identities, all 20 prompt hashes, the two author documents and the two publication tokens. Per case and author extract each observation as a prerequisite, action, result, next useful lead, and any stop/cutoff. Record the precise range where one was reported. Mark reconstructed search intent, unknown byte cost, cross-task leads, and cases for which only a filename was recorded. Preserve the source wording/digest alongside the normalized representation. A trace with a missing range cannot earn verified range coverage from an inferred location.
- **Reference.** For each clause create an evaluator-only list of independent evidence obligations and acceptable alternatives from the frozen reference notes and source-checked moves. Track each obligation as shared, unique to one investigator, contradictory, or still unverified. Keep both branches of a real ambiguity. Freeze this file before candidate runs. A fact mentioned in either brief is a *candidate* until its scope and source are verified. A single well-supported alternative can satisfy an obligation. The held-out reference is never consulted.
- **Control.** Replay today’s canonical gatherer on the same publications with Focus and Arbiter off and label that output as the native control. Separately compare recorded first-delivery bytes with the prior ceiling disposition. Build a maintained ordinary-tools adapter for a fresh, matched Sol reference and a separate adapter that replays recorded evidence in a different presentation. The existing `replay.py` can replay canonical or recorded native calls. It does not supply either of those adapters. Start each agent session independently with the prompt, the same authorized source and equivalent orientation. Record the dispatch and return from its actual entry point. Reserve the reference notes, both strategy documents and all prior answers for the evaluator. Verify disabled Focus and Arbiter from runtime activation receipts. The harness label `noModel` alone cannot prove either state. Do not substitute an installed live index for either pinned publication. If native admission is paused, finish the static comparator, synthetic regressions and unprivileged offline analysis.
- **Experiment order.** First compare fresh ordinary-tools Sol with matched Sol receiving the archived complete evidence. Next vary presentation alone while holding passages, order-independent membership, actor, effort, budgets and judging fixed. Freeze the chosen presentation before testing task-derived retrieval. If a reviewer selects minimal passages to diagnose a miss, label those passages evaluator-supplied and keep that experiment outside product grading.
- **Exit.** The comparator enumerates exactly 20 cases, both author paths and their evidence limits. It rejects a swapped repository, changed prompt, missing case, stale publication and an evidence obligation sourced only from an actor’s guess. Golden expected sets in tests are authored independently of the candidate code.

**M1 · Represent what the agent needs to decide.** Extend the existing split/plan and retrieval policy in the owning kernel modules. Reuse existing Core loop types where their meaning fits. Internally keep, per need, its parent clause, known anchor and origin (prompt or prior citation), operation requested, evidence already present, competing explanation, and what would close it. Select from a small fixed set of actions: open a known range, list a bounded inventory, find an identifier or literal, search for a vocabulary pivot, read a deciding unit, follow a verified join, reconcile a population, or stop with a reason. This is transient request state. [Split](../../crates/airk-kernel/src/arbiter/answer/split.rs) · [Plan](../../crates/airk-kernel/src/arbiter/answer/plan.rs) · [Current typed reads](../../crates/airk-core/src/arbiter/answer_loop.rs)

Start deterministically. For a known path and span, open it. For a route, quoted text, number with unit or sibling name, find exact matches within scope. For broad behavior with no anchor, start with semantic/lexical search and docs. Where both paths are plausible, choose the cheapest operation that distinguishes them. Use a stable tie-break on approved scope/member, path and start location. Before dispatch, validate that the target is permitted and came from the prompt, admitted inventory or an earlier result. When all such actions are exhausted, return the supported partial. Test the decision table with renamed symbols, swapped directories, a misleading high-ranked document and a task without any identifier. None of the 20 real repo names, slots, paths or answers may occur in production policy.

**M2 · Implement exact finding and useful reads.** At the canonical kernel read boundary, add the Decision 6 scoped literal scan and wire approved Code Facts, Document Facts and Relationships to their existing owners. Use the common dispatch/dedup and service grant/provenance path; do not implement lookalike handling in MCP or the harness. For public or shared changes, update the owning contract, Rust schema fixtures and consumer generation before client adapters. Keep the minimal internal literal comparison functional independently of optional public scan extensions. [Retrieve and react](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Shared read owner](../../crates/airk-kernel/src/arbiter/answer/gather.rs) · [References](../../crates/airk-kernel/src/references.rs) · [Conditional reads](../../crates/airk-kernel/src/current/conditional.rs)

- Bind every scan to the selected root/member, publication and eligible retained text. Start with a known file, justified prefix or admitted component. For exact literals treat dots, slashes and hyphens as data. Distinguish matched occurrences from matching lines, and files examined from files returned. Deduplicate overlapping context ranges while preserving match locations. Permit a verified postings fast path; if it lacks complete recall for this match rule, scan the remaining eligible bytes or mark coverage partial.
- Internally retain enough data to produce paths only, per-file counts, or hits with small context and a coverage receipt. Prefer streaming and existing paged inventory. Stop on cancellation, deadline, byte/file limit, expired token or revoked scope. Freeze cursor identity to request/filter/revision. If a new public scan mode or cursor would change a contract, use the internal result for this experiment and record the pending decision.
- Implement enclosing-unit selection first as composition over current Outline/Symbols and retained range readers. Expand until the condition plus its consequence, or setup plus assertion, is present. For extraction-backed documents preserve original locations. For optional public EOF clamping, multi-literal/pattern flags and line-safe continuation, follow the contract decision; existing range-refusal and legacy fixtures keep their original meaning.
- **Exit.** On synthetic retained corpora, independent literal search agrees on occurrence locations and per-file counts for exact, dotted, hyphenated and numeric examples within the admitted scope. Exercise case sensitivity, repeated matches on one line, UTF-8 boundaries, identical paths in different indexes, old unretained paths, denied files, stale tokens, truncated scans and cancellation. A complete no-match receipt and an incomplete scan must be distinguishable through the real reader.

**M3 · Make each follow-up depend on the previous result.** Change the policy at `initial` and `react`, and use the existing Coverage and shared gathering owners. Use a request-local key for each candidate action (source identity, operation, arguments). Keep a per-clause queue of grounded actions. Round one finds/reads owners. Later rounds follow the best observed edge, test a counterexample, or close an enumeration. Deduplicate before scheduling, share a read that answers multiple clauses, and emit deterministic evidence ordering even when independent reads finish out of order. Rank novel actions by the unmet obligation they can settle, then cost and stable source order. Skip repeated identical requests unless a real continuation advances. [Retrieval](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Coverage](../../crates/airk-kernel/src/arbiter/answer/coverage.rs) · [Admission](../../crates/airk-kernel/src/arbiter/answer/admission.rs)

Maintain distinct outcomes for nomination, read, support, contradiction and unresolved evidence. Following the text of a call into a test mock is a candidate path; require the production registration for a production claim. For a shortage of matches, try spelling variants or a term found in the governing prose. For “all”, close with a second inventory/lookup and reconcile omissions. Resolve words such as “new” and “missing” against evidence even when that changes the premise. Use the Windows CUDA comparison below to localize the failure first. If it implicates the census or enclosing-unit reads, test each change with Arbiter on, against the previous code on the same inputs and at equal total bytes. Cover its mechanical behavior with a focused unit regression. Do not replay real tasks with both optional models disabled. Record the first stage to deliver each essential. Compare old readers versus the expanded roster under the same original policy. Compare one versus two follow-ups per open need only after the required contract decision, with the reader roster fixed. Compare fixed versus need-based excerpt sizes at equal total bytes when range selection or evidence delivery is the observed failure. Do not silently combine an extra round, new model and new delivery format into the same comparison.

**M4 · Deliver evidence in a form the caller can use.** In Compose/Union assemble per-clause evidence with source identity, exact range, what it establishes and its limit. Place the relevant exception or counterexample next to the deciding passage. Reserve room for another unsatisfied clause before spending more on a duplicate hit. Make it possible to tell the difference between a ready handoff, a pending read, a refusal, a partial answer and an empty match under a completed scan. Preserve root isolation, original location and authorization when sharing or continuing a request. [Compose](../../crates/airk-kernel/src/arbiter/answer/compose.rs) · [Union](../../crates/airk-kernel/src/arbiter/answer/union.rs) · [Read-surface parity](../read-interface-parity.md)

Test presentation separately first: take the same retained first-delivery evidence and compare the original view to task-grouped ranges in fresh blinded sessions under identical output and tool budgets. The group labels may use the task or source terms; never feed the grader’s obligation text to an actor. During the approved fixed-protocol comparisons, keep that protocol stable. Freeze any selected presentation change in a separate comparison after any required contract decision. Run the Windows CUDA model comparison below as soon as the deterministic control has a valid settled capture. The control qualifies as a comparison point when its scope, inputs, budgets, and outcomes can be verified, including when it misses evidence or produces zero correct finishes. The settled 26 November capture of signed source `4df7c8c8` qualifies. Do not rerun it.

**M5 · Verify the real entry point and prepare the Windows review.** Exercise the canonical operation through the actual MCP catalog/transport and the corresponding service, CLI/session and Workbench reads for the affected surfaces. Compare equivalent inputs and full replies, normalizing only documented transport IDs and timing. Include multiple indexes with the same relative path, a partial result, a refused or expired continuation, model disabled/unavailable, budget exhaustion and cancellation. Use the real public tool offer to confirm both what the client sees and which handler ran. Run the per-task matrix with the model states and evidence requirements below. Transport parity establishes that callers receive the same reply. It cannot close the retrieval-quality milestone. Linux, macOS, WebGPU, deployment and publication are outside this review.

### Windows CUDA model comparison and iteration

**Starting evidence.** The first Windows replay exercised the deterministic loop. Both native arms delivered the same two of 113 authenticated required ranges. The candidate made 261 native calls and the control made 236. The hosted ordinary-tools runs produced zero correct finishes at either 12 or 24 external calls. Grouping the same archived passages produced two correct finishes in 20 tasks, up from zero for the original presentation. The missing comparison is a model's judgment inside the native loop. The completed Windows transport receipts and deterministic replay remain usable at their recorded scope. [Windows review record](../simplify/one-process-ledger.md#26-november-2026--gather-context-windows-review-disposition)

**Inference is required (user decision, 26 November 2026).** Fixed rules and retrieval readers cannot prepare context the way an agent does. An agent decides what to look up from the task, chooses its next read from what the last read means and judges when the evidence is enough. Each of those steps needs inference. Every tested run therefore has a model taking part, and the both-models-off loop is not replayed. The user first limited testing to Context with Focus off and Focus with Context, and then replaced both models with the single-stage design below. Gather Context must run on a 16 GB GPU, and a user without one may not be able to use it. [Decision record](../simplify/one-process-ledger.md#26-november-2026--gather-context-model-configurations-weights-and-16-gb-target)

**Single-stage Arbiter (user decision, 26 November 2026).** One model serves one inference stage named Arbiter, which does the work that Focus and Context did. The model is Qwen3-Reranker-4B, stored with 4-bit weights and computed in FP16. In the publisher's results it ranks code better than today's Focus model (81.20 against 73.42 on MTEB-Code) and follows a task instruction better than the 0.6B or the 8B. Today's Focus and Context models are superseded for Gather Context and are not run. Where earlier sections of this document describe Focus and Arbiter as two models or two stages, this section governs. [Model card](https://huggingface.co/Qwen/Qwen3-Reranker-4B) · [Decision record](../simplify/one-process-ledger.md#26-november-2026--single-stage-arbiter-on-qwen3-reranker-4b)

The published checkpoint scores a pair with a yes or no readout under a task instruction. Arbiter keeps two scoring forms on the one resident model, and a test chooses between them for each use. A **pair judgment** scores one candidate against the task or need and works with the published checkpoint. A **roster choice** scores all offered candidates in one pass by reading the option letters, as today's Context compiler does, and this checkpoint has to be trained for it. A pair judgment repeats the shared state for every candidate, so compare the two forms on accuracy and time before choosing one for next reads, lookups and coverage. Passage ranking uses pair judgments. [Roster compiler](../../crates/airk-retrieval/src/arbiter/compiler/qwen.rs)

| Use | What Arbiter judges |
| --- | --- |
| Rank passages | The task or open need with a passage or parsed unit: does it hold evidence the task needs? |
| Rank next reads | The task and what is established with each grounded read: would making it help? |
| Rank lookups from the task | The task with each candidate anchor or lookup: is it worth making? |
| Check coverage | An open need with the evidence gathered for it: does it establish what the need asks? |
| Build the reply | Each open need with the passages gathered for it: which lead the reply and which are trimmed? |

The kernel lists the candidates, Arbiter scores them, and the kernel expands the best under the existing scope, hash, admission and budget rules. Do not call Arbiter where there is no choice: a known address, a single candidate, citation formatting or exact deduplication. The split of a request stays with the fixed rule unless a trace shows a miss. Scores for different uses are not on one scale. Fit a threshold for each use from traces on the deployed 4-bit build, because quantizing shifts scores. Give each use a private evaluation switch, so a run can show what that use adds.

**Request state.** The model remembers nothing between calls, and a scoring model cannot write notes. The kernel builds the state it shows from structure: the units already read with their paths and signatures, each need and its standing, and the scores already given. Keep that state within a fixed token allowance and retain the exact state shown with every judgment.

**Input budgets.** The model accepts up to 32K tokens in one input. That is the documented maximum and not a default. Start at today's limits so results stay comparable: 1,024 tokens for a pair and 8,192 for a roster. Raise a passage pair to about 2,048 tokens so a whole function fits. Allow 16K and then 32K only after memory and time are measured at that length and the largest batch. Passages scored as separate pairs do not share one window, but their batch shares GPU memory. State how the one resident serves each admitted input size on one copy of the weights.

**One resident and one queue.** Hold one resident for the model, owned by its checkpoint, export and device identity. Each request names the Arbiter use it needs. Residency is keyed by stage today, so two stages that name one checkpoint would load it twice, and renaming the stages would not change that. Give the resident one load, warm-up, memory reservation and unload, one queue with bounded batching, and one unavailable state that falls back to the fixed order. [Session owner](../../crates/airk-inference/src/inference_engine/pool/arbiter.rs) · [Stage configuration](../../crates/airk-core/src/arbiter/stages.rs)

**The caller states the needs (user decision, 26 November 2026).** Gather Context stays one tool. `task` remains the only required argument. The call also takes optional `needs`, `anchors`, `deliverable` and `known`, and the tool description asks the calling agent to fill them before it calls. `needs` are three to eight short statements of what the agent must find out, one fact each. `anchors` are exact strings copied from the request. `deliverable` says what the agent will produce. `known` lists paths or ranges it already holds. There is no separate tool for establishing needs: an agent skips a preparatory call more readily than it leaves a parameter empty. Record the public call shape in the owning contract before changing it. [Probe results](../simplify/one-process-ledger.md#26-november-2026--needs-written-by-a-calling-agent-from-the-prompt-alone)

The published checkpoint was probed outside AIRK on the 20 tasks. With the whole task as the query it put 40 of the 113 required passages in the first five places. With needs a fresh agent wrote from the prompt alone it put 56 there, and with each need stated by someone who had read the code, 92. The judge is capable when it is told what is needed, so the statement of each need is the lever.

- **Run each need as its own job.** Look up the anchors exactly, gather and parse candidates for each need, and have Arbiter judge each need's candidates against that need's own statement. Needs do not depend on one another, so run them side by side under the shared budgets and batch their judgments through the one queue.
- **Union by need.** Give each need its best passages. Carry a passage once and list every need it serves. Report for each need what was found, what was not, and the best lead.
- **Return the repository's vocabulary.** For a need that came up short, return the names, paths and counts the sources use near its anchors, so the agent can restate the need in the repository's words.
- **A second call uses the same tool.** The agent sends refined needs and `known`. This is one of the two follow-up calls the sufficiency standard allows.
- **Work without needs.** With `task` alone the kernel derives needs by fixed rules, and the reply says the needs were derived. Say so too when a supplied need is too vague to judge against.

**Weights and arithmetic.** Store the weights as 4-bit blocks with per-block scales, and keep the embeddings, normalization parameters and the two readout rows in FP16. Compute every large operation in FP16, as the publisher recommends for this checkpoint. Use FP32 only in a short fixed list of small reductions on activations, never on a weight matrix. Start on 16 GB cards by expanding the 4-bit weights to FP16 when the model loads. That needs only ordinary FP16 kernels and holds about 8 GB. Keeping the weights packed in GPU memory, at about 3 GB, is the route to smaller cards once the pinned runtime's 4-bit multiply on CUDA is verified. Both sizes are estimates until measured. Return only the readout scores from the graph. Before the packed form is admitted, show one weight allocation for the resident, bounded peak memory at the longest admitted input and largest batch, the same ordering as the expanded form, and CUDA verified separately from any later WebGPU route.

**FP16 first (user decision, 26 November 2026).** Until the design works, every model is FP16: Arbiter's weights are stored and computed in FP16, about 8 GB, which fits a 16 GB card, and the bundled MiniLM is the FP16 profile. Quantization is deferred. The 4-bit and 8-bit storage above, the test that a quantized build keeps the ranking, and the acceptance rule for a quantized export all wait until the FP16 build reaches the sufficiency aim. The probe's measurements of 4-bit and 8-bit weights stay on record for that later step.

**8-bit weights, FP16 compute (user decision, later on 26 November 2026).** "Keep it simple": the working model is the published Qwen3-Reranker-4B with its weights stored in 8 bits and computed in FP16. The 0.6B model is not a candidate; the user reports it was tried and its results were not good. Other models, other quantization and speed work wait until the gathering problem is solved. The probe measured 8-bit block weights on this checkpoint: the same best candidate as FP16 in 19 of 20 cases, score differences under 0.05, about 4.9 GB packed. Until an export exists, development runs stand in for it by rounding the weights to 8 bits in blocks and expanding them to FP16 at load.

**What the first tests must show.** First, the 4-bit build keeps the ranking: compare its order with the FP16 build of the same checkpoint on captured candidate lists. Second, instructions work for actions: the checkpoint was trained to judge passages, and judging whether a read would help is untested. Third, coverage tells complete evidence from partial evidence: a passage can be highly relevant and still omit the exception that decides the answer. Until this test passes, a coverage score may lower a need's priority but must not end work on it while budget and unexplored candidates remain. Fourth, the scoring form is chosen: compare pair judgments with the roster choice for next reads, lookups and coverage. Fifth, time fits: by recorded timings each pair costs roughly four to five times a pair on today's Focus model, so measure seconds per request and pairs scored. Reusing the shared part of a prompt across candidates can cut that cost later and needs runtime work.

**Decisions this design needs from the user.** Collapsing two stages into one changes the saved stage configuration, the receipts and the two client switches. Do it in two steps: one shared resident behind the existing stages first, then one Arbiter switch and renamed receipts under a recorded contract decision. Qualifying the 4-bit build by agreement in ranking and unchanged sufficiency results would replace the 0.007 raw-score limit for this model. Ordinary Search would get a slower reranker, so it needs a small pair limit or no reranking by default. Record each decision before changing a wire shape, a digest input or a persisted format.

**What the settled control already shows.** All 60 settled native replies made no Focus call and no Context call. Claude's unreceipted read of the 20 final-candidate replies, using the rubric's line-overlap rule, places the 113 required passages as follows. Four were opened at the required lines. Twenty were listed in a hit at those lines and never opened, 15 of them in Munarium. Thirty-five lie in a file the loop surfaced at other lines, 25 of them in Munarium. Fifty-four lie in files the loop never surfaced, 45 of them in Bookbuilder. This is design input until the maintained evaluator reproduces it. Derive the control's trace from those replies on disk. They carry every dispatched read with its arguments and returned ranges.

**Start from the settled control, then bring up Arbiter.** On resumption, reuse the verified Windows CUDA fixture and frozen Bookbuilder and Munarium publications. Judge the 20 settled control replies under the sufficiency standard first. That sets the starting count and needs no run. Then build Arbiter and add private trace capture as needed. Start with Munarium C1 and C3 and Bookbuilder C2 and T2, covering implementation ownership, inventory, a failure branch, and the exact test suites to run. Freeze the instruction, publication, scope, model and tokenizer identities, weight identity and precision, tool offer, presentation, native rounds and call/byte budgets, and deadline before comparing. The settled both-models-off capture is the control wherever those inputs match. Do not rerun it. Private trace capture that only records observations leaves those inputs unchanged. Keep the comparison at the approved three native rounds and 132-call budget. Success is judged directly from the reply under the sufficiency standard below, so this comparison needs no hosted caller or judge sessions. Retain the 12-call and 24-call hosted diagnostics as separate evidence.

| Configuration | Evidence to retain |
| --- | --- |
| Arbiter on (required) | Verify the enrolled Arbiter profile, the weight identity and precision, and actual CUDA execution. Retain every judgment with its use, instruction, candidate and score, and the read or stop that followed. |
| Arbiter off | Do not rerun. The settled both-models-off capture is the control, and a low score is a usable control. |
| Today's Focus and Context models | Do not run. The single-stage design supersedes them for Gather Context. |
| Model unavailable | Exercise the native fallback under a controlled unavailability on the same inputs. Retain the attempted stage, actual fallback, disclosed degradation and final reply. Label this separately from a disabled configuration. |

Bind each model receipt to the exact service image, publication, profile, device, request and selected action. A CUDA-capable build, activation setting, embedding smoke test, or hosted caller and judge does not establish that a retrieval model ran. Require actual native dispatch observations for each enabled arm across its cases. If an individual task legitimately needs no dispatch, record the reason and keep it in the denominator. A timeout, refusal, fallback or missing receipt cannot count as an enabled success. Count the decisions whose recorded source is Arbiter, by use and case. A decision logged as deterministic, abstained or not dispatched is not an Arbiter decision, and an enabled run with none of them is not an enabled success. Count a step that offered no choice separately from a fallback. Record peak device memory and elapsed time for every run. A run that does not fit a 16 GB GPU fails the fit. [CUDA fixture owner](../../scripts/arbiter-answer-eval/gpu_fixture.py) · [Native capture](../../scripts/arbiter-answer-eval/gather_context_capture.py)

**Trace each deciding passage.** In the private evaluator, join the required witness to nomination, the dispatched reader and actual returned range, retention, and the verbatim delivered reply. Record exact location and digest, scope, parent action, plausible alternatives, cost, stage decision and reason for omission or clipping. Keep evaluator obligations out of runtime prompts and product state. Use the first observed divergence to decide what to change.

- **Discovery and ranking.** In the kernel retrieval and coverage owners, nominate a bounded mix of exact hits, structural owners, callers and governing prose for each open clause. Add a scoped lookup from a prompt anchor or a lead observed in source where the correct owner is missing. Widen the candidate set beyond the first 40, because a judge can only lift what it is given. Give Arbiter the actual candidate passages and the clause to rank for. Preserve the candidate set and the ranked order so a discovery miss can be distinguished from a ranking miss. [Retrieval](../../crates/airk-kernel/src/arbiter/answer/retrieve.rs) · [Coverage](../../crates/airk-kernel/src/arbiter/answer/coverage.rs)
- **Parse on demand where the index falls short.** The grammars that index code are functions of a path and its text in the ingest crate, so the kernel can run them at request time on retained source. Read the enclosing definition around a hit in place of a fixed line window. List the arms of a switch or match and the members of a registration table. Classify each literal match by its role as a declaration, a call argument, a string or a comment, and report counts by file and role. Take call-argument literals from the parsed tree in place of the hand-written scanner. Offer these results to Arbiter as grounded next reads. Embedded Vue and Svelte script and SQL have no call profile today, so record a miss that depends on them. A file inside the authorized root that the index lacks needs the user's source-authority decision before a direct read and parse. Record the missed case and continue. [Syntax profiles](../../crates/airk-ingest/src/syntax.rs) · [Structural evidence](../code-facts-and-relationships.md) · [Current scanner](../../crates/airk-kernel/src/arbiter/answer/follow.rs)
- **Arbiter and the next read.** In the follow-up and model decision owners, offer bounded actions grounded in confirmed source, such as opening the deciding branch, following a registration, reading a working control or reconciling an inventory. Describe each action in a few lines with its grounding, so that judging it is a short pair. Show Arbiter the unresolved clause and the request state the kernel built: what was read, what stands established and which reads already ran. Keep scope, publication, hash checks, deduplication, admission and budget enforcement deterministic. Retain the offered actions, actual selection and resulting evidence. Route every choice that depends on what the evidence means to Arbiter, including which lookups to make from the task. A fixed rule such as the first extracted term may order the candidates. It decides only when Arbiter abstains or cannot be dispatched, and the trace records that. [Follow-up](../../crates/airk-kernel/src/arbiter/answer/follow.rs) · [Model decisions](../../crates/airk-kernel/src/arbiter/answer/model.rs)
- **Delivery and use.** When the proof was opened but omitted, fix Compose/Union allocation before adding search work. Keep the condition with its consequence, the relevant exception next to the rule, and the inventory count with the population examined. Order each need's passages by Arbiter's score, lead with the highest and trim those below the threshold, so the reply shrinks without losing the deciding passage. If the deciding evidence reached the caller and the answer is still wrong, hold the delivered passages fixed and test their organization or the caller's use of them, judged directly under the sufficiency standard. [Compose](../../crates/airk-kernel/src/arbiter/answer/compose.rs) · [Union](../../crates/airk-kernel/src/arbiter/answer/union.rs)

**Iterate until the real tasks succeed.** For each failed case, preserve the trace, identify the earliest supported failure, make one targeted repair, run a focused regression and rerun the affected real case under the same inputs. If the trace is inconclusive, add the smallest observation that can distinguish the plausible causes before choosing a fix. If a repair fails, revise the diagnosis and try the next supported change. Use the four tasks for rapid iteration, protect successful cases with regressions, and use the full replay to identify the next failures to work on. A poor score, a failed hypothesis or an unavailable optional model configuration is a finding to work through. Continue independent cases while resolving blockers. If progress genuinely depends on a user decision or unavailable dependency, record the exact blocker, affected cases, attempts and next executable action for review.

**Change model weights where the trace requires it (user decision, 26 November 2026).** A repair may change kernel code, a roster or prompt, or model weights. Start from the published checkpoint with a task instruction for each use and no training. Change Arbiter's weights when it repeatedly scores a wrong candidate above the right one for a stated use, with the right one among its candidates. Produce one final checkpoint that keeps its passage ranking and adds the other uses. Low-rank tuning may be the training method, merged into the weights before quantizing. Mix the training across passage relevance with plausible wrong matches, necessary against redundant needs, productive against repeated reads, complete against partial evidence, and justified against premature stopping. Check the tuned checkpoint's ranking against the published one. When the right choice was never offered, fix the candidates. When a call was shed for tokens or time, fix the allocation. Build training pairs by running the loop on mined tasks from both repositories and labelling each offered choice by its source-checked outcome. The 20 judged tasks, their notes and traces, and the sealed set never train a model. Hold out a share of each repository's pairs and include general replay so existing abilities survive. Keep the original weights selectable. Tune at full precision, then quantize to the delivered 4-bit form, then verify the export under the acceptance rule in force. Before judging, compare old and new weights on identical code and inputs, and confirm the 16 GB fit. Record the weight identity of every run. Keep training code in maintained repository folders, and keep pairs and weight files in the private campaign folder. No cloud teacher is authorized. [Training data and guards](final-plan-fp16.md#54-training-data-for-model-weights) · [Context tools](../../scripts/arbiter/training/context_answer_eval_r1/README.md) · [Focus tools](../../scripts/arbiter/training/focus_answer_eval_r1/README.md)

**Check that a judge is enough (proposed, for the user's review).** Arbiter judges candidates and never writes one. The probe has answered part of this: fixed clause cutting is not enough, and the calling agent now writes the needs. For every passage missed with Arbiter on, record whether any candidate offered to it could have led to the passage. A miss that no offered candidate could reach is a limit of candidate generation, and a weight change cannot fix it. If such misses remain the larger share after one round of candidate repairs on the four tasks, bring the user a proposal for a model that writes lookups. State the role it would play, the candidate models, their measured results on the same four tasks and their peak memory on a 16 GB GPU. Adding a decode runtime or changing a model's wire contract is the user's decision. [How the models take part](final-plan-fp16.md#122-how-the-models-take-part)

**Measure progress against the caller's result.** Success is a judgment of sufficiency (user decision, 26 November 2026). A task succeeds when the Gather Context reply gives an AI agent most or all of what it needs, and the agent can complete its preparation with at most two follow-up tool calls. The reply does not have to match the reference notes item by item. The notes show what matters. A different passage that establishes the same fact counts, and a detail the agent would meet while doing the work does not have to be delivered. A task fails when more than two follow-up calls are needed, when the preparation still lacks or contradicts something the work depends on, or when the reply would lead the agent to act on misleading evidence. The agent carrying out this plan makes this judgment directly. Do not launch separate headless sessions to judge. Read the reply as delivered, list what the task still needs, make at most two follow-up calls through the same tool surface, then check the outcome against the reference notes. Record the verdict with its reasons and the passages relied on, and label it as the working agent's own judgment. [Sufficiency standard](answer-evaluation/rubrics.md#sufficiency-standard-from-26-november-2026)

First get a sufficient result on a real task with a trace showing exactly how it recovered and delivered deciding evidence the matched control missed. That earns a full replay and begins the next iteration. Freeze each reviewed candidate before running all 20 on the real tool surface with the same budgets. Judge every result directly under the sufficiency standard, and count every timeout, refusal and unfinished reply in the denominator. If attributing a gain to a code change, compare the previous and revised code with the same selected model configuration. Retest transfer on an unfamiliar task and source layout. Aim for **20/20 sufficient results**. Use **at least 16/20 overall and at least 8/10 per repository** as the proposed minimum for the user to review as “most.” Keep working on the remaining failures toward 20/20. Report the share of essential notes established beside each verdict as a diagnostic. Any decision to accept fewer than 20 belongs in the user's recorded disposition. The historical complete-evidence gate and the formal product comparison remain governed by the completion plan.

**Make completion verifiable.** Extend the maintained replay/report validator to require the frozen case roster and the required Arbiter configuration, bound CUDA dispatch receipts and Arbiter-sourced decision counts for enabled runs, the weight identity, precision, largest admitted input and peak device memory of every run, the unavailable-path observation, the source-to-delivery trace, and a recorded sufficiency verdict with its reasons for every case. Add negative tests for a missing case, a disabled or CPU-fallback run mislabeled as Arbiter-enabled, a run that exceeds the 16 GB fit, and a missing trace or verdict. Keep incomplete runs reportable with their exact missing evidence. Require the validator's successful complete report and the recorded user disposition before marking the implementation and testing complete. After the user reviews the plan and resumes work, run authorized Windows CUDA development diagnostics under their own frozen scope. Restarting the formal product baseline still requires the recorded review decision.

### Strategy from 27 November 2026

This section governs Gather Context work from this date. Where it differs from the sections around it, this one applies. The earlier sections stay as the record of what was planned and why. The user directed this strategy on 27 November 2026 after reviewing the prototype and first kernel results. Claude carries out the work (user direction, 26 November 2026). [Results record](../simplify/one-process-ledger.md) · [The loop as prototyped](gather-context-loop.md)

**What the work so far established.**

- *The agents we imitate did poorly on our own standard.* Where sessions were judged, almost none met the bar. The loop baseline finished no scenario correctly on all five paths. With all 113 required passages placed in the agent's first reply, 2 of 20 briefs were correct. The two reference investigations, Claude's and OpenAI's, were never judged.
- *Delivering evidence and an agent using it are separate problems.* Required passages delivered does not predict a correct result. It stays a diagnostic.
- *The reference agents worked in a recognisable way.* A mean of 5.5 turns and 15.6 calls a task; about 570 lines read a task; an exact census of a token as the first move in 16 of 20 tasks; a search address turned into a read of a whole function or section; six findings that were absences; five tasks that demanded every place.
- *Fetching needs no model and choosing does.* On four tasks the kernel without the new fetching delivered 1 of 21 required passages. The same fetching rules outside the runtime put 14 of 21 in the candidate pool with no model, and word overlap delivered 4 of them. With Qwen3-Reranker-4B choosing, about 600 judgments a task delivered 12 and several thousand delivered 14. Weights rounded to 8 bits and computed in FP16 delivered exactly what FP16 did.
- *A check on the judge's own confidence does not find its misses.* Where it ranked a wrong passage above a right one, the right one scored high once a different need saw it or a need asked for that kind of evidence.
- *The kernel now runs the fetching rules.* The first baseline delivered 6 of 21 with no model in 8 to 28 seconds a request, with three known faults.

**The goal (user, 27 November 2026).** AIRK succeeds when using Gather Context reduces what an agent spends on its model to reach the goal of a task. Cost reduction is the goal. It comes from fewer round trips to the model, and the aim is to reduce them, not to eliminate them.

- *Why round trips and reply size both matter.* An agent's token use is roughly the size of its context multiplied by its round trips, because each round trip sends the whole conversation again. Saving a round trip saves the whole context once. Everything Gather Context returns is sent again on every later round trip, so a large reply can cut round trips and still raise the cost.
- *Two ways a call lowers cost (user, 27 November 2026).* It can save round trips. And it can shrink what the agent carries on the round trips that remain: an agent working alone reads whole files and long search results to find the few lines that matter and carries all of it afterwards, so a tool that gathers the evidence and reduces it to what the agent can use makes every later round trip cheaper. Round trips down by 10 percent with cost down by 30 percent is a large win. Where the two differ, cost is what counts. A reply therefore has to replace the agent's own reading, not add to it, and each attempt records the lines and bytes the agent read through its own tools in each arm.
- *More reduction is better, up to a point.* Past that point the chance of a wrong result, and later the time the tool takes, make it a drag on value.
- *Time is not a constraint in the first solution (user, 27 November 2026).* Elapsed time is recorded and reported and does not decide whether a change is kept. It is taken up after the strategies that reduce cost most are found.
- *Success in this effort is cost alone (user, 27 November 2026).* Cost is the agent's own reported usage: how many round trips it makes to its model and how much each round trip carries. The time a call takes in the evaluation tooling is not a measure of success, least of all while the model's choice is applied outside the runtime, where its timing says nothing about the product. The model is incorporated into the runtime after the strategy proves out. Correctness stays the guard: a cheaper session that reaches a worse result is not a saving.
- *The agent is expected to call the tool several times in one task.* A task that goes from 10 round trips to 4, or from 20 to 15, is a win. The tool and its description in the MCP catalog are designed for repeated use.
- *A single deterministic pass that does everything is not the goal.* The reference agents' deciding steps often used knowledge the tool does not have, such as a framework's exception type or the repository's own word for a feature. The agent keeps that work; the tool shortens the looking.
- *The yardstick.* An average reduction of 30 percent, with results no less correct, would be an outstanding outcome.
- *What a call must do to save a round trip.* An agent already runs independent lookups together: the Claude record shows 109 turns holding 312 calls, about three a turn. What costs it a round trip is a chain, where the next step depends on the last result: search, then read what the search pointed at, then follow what that passage calls or who calls it. A Gather Context call is worth making when it completes two or three such dependent steps at once. A call that only does what the agent could have run together saves nothing. Each rule is judged by how many dependent steps it collapses.

**The shape of the product.**

1. *Deterministic fetching does the wide work.* Exact census of anchors and identifiers, search by need in separate lanes for code, documents and configuration, the project's rule files, the files the evidence names, and code structure followed from located passages through the index's own facts.
2. *The reply is a map first and little text.* For each need: where the thing lives, a few whole functions, the census by file, what was searched and not found, and what is still open. The size target is what a reference agent read for itself, about 25 KB, against the 50 to 110 KB replies of today.
3. *Absence and completeness are stated with their scope.* "Searched these indexed files for X and found none", and which files are outside the index. Without that a reply cannot support "no test exists" or "these are all the places".
4. *A need closes on a cited source or a declared absence.* Rounds and budgets are limits, not the stop rule. Open needs are listed as open.
5. *The model chooses among what was fetched, and no more.* The working model is the published Qwen3-Reranker-4B with 8-bit weights and FP16 compute. It is not yet in the runtime. Whether it is needed at all once the census narrows the candidates is an open test. Other models, other quantization and speed work wait until the gathering problem is solved.
6. *The request may carry a folder scope.* An agent working in one project of a repository says so. Both scoped and unscoped requests are tested.
7. *Every call is worth making again.* An agent will call the tool at the start of a task and again as it learns more, so each reply must cost less in tokens than the round trips it saves. A reply is small, sends nothing the agent already holds (the request's `known` ranges, which today are checked and unused), and answers a narrow follow-up as readily as a whole task. How long a call takes is recorded and is not a limit yet.
8. *Each reply sets up the next call.* It returns the repository's own vocabulary that it found, the names and files worth asking about next, and the needs still open, so the agent's next request is better aimed than its first.
9. *The description tells an agent how to use it.* When to call it first, when to call it again with what was learned, what to put in `needs`, `anchors` and `known`, and when a direct lookup is the better call.

**Development tasks.** Two sets replace the 20 broad tasks as the working set.

*Broad set, 6 tasks, 3 per repository.* Chosen by rule from the existing 20: per repository one coding, one architecture and one test or writing task, taking the first of each kind that Claude has not tuned on. Claude tuned on Munarium C1 and C3 and Bookbuilder C2 and T2 during the prototype.

| Repository | Coding | Architecture | Test or writing |
| --- | --- | --- | --- |
| Bookbuilder | C3, which also asks for every place | A1 | T1 |
| Munarium | C2 | A1 | T1 |

Munarium's every-place task, C3, is one of the tuned four, so one of the new Munarium tasks below asks for every place. Of the 14 tasks no longer in the working set, the 10 that were never tuned on are a holdout, run only at checkpoints.

*Narrow set, 14 tasks, 7 per repository:* 4 coding, 2 architecture and 1 writing. These are the requests an agent meets in everyday development: fix this error, add one more of an existing kind, change a limit or a default, find where something is decided, write a test for this behaviour, say what calls this.

- Bookbuilder tasks stay inside the `pro` folder. The repository holds several divergent projects, and tasks that span them would be less typical.
- Coding tasks start from real commits in each repository's history and are then adjusted by Claude's judgment toward what a typical user of an agent would ask. The user's own tasks and commits may be atypical, so a mined commit is a source, not the task as written.
- Every task is posed against the indexed snapshot, Bookbuilder `238b22a722ce` and Munarium `9948345d1d14`. A mined commit qualifies only when the code it changed is present unchanged in that snapshot.
- Each task's truth is written and frozen with a hash before any tool or agent runs on it. For a coding task it is the lines that have to change, taken from the source commit where one exists. For an architecture or writing task it is a short list of facts with their locations.
- Claude writes both the tasks and their truth, which is a known weakness. Mined commits and the freeze before any run are the guards against it.

**How results are measured.**

- *The result is the reduction in cost.* An acting agent attempts each task with Gather Context available and again with ordinary reads only. The measure is the percentage lower model cost with the tool, averaged over tasks. Cost is the agent's own reported usage (fresh input, cached input read, cache written and output tokens) priced at the model's list rates, so the four kinds combine fairly. Sonnet and Opus are the acting models (user decision, 27 November 2026).
- *Reported beside it:* round trips to the model, the four token counts, and elapsed time split into model time, Gather Context time and other tool time.
- *One guard goes with it:* the result is no less correct against the frozen truth. A reduction bought with wrong answers is not a win. Also recorded for each attempt: how many times the agent called Gather Context, the seconds and the bytes returned for each call.
- *A change is kept when* cost per task falls and the result is no less correct.
- *The effort curve.* The tool arm is run at several effort settings, using the kernel's deadline, call budget and rule switches, for example about 2, 10 and 30 seconds a call. Cost, round trips, elapsed time and correctness at each setting show where more gathering keeps reducing cost, where it flattens and where it turns. The setting at the knee becomes the default, and it may differ by kind of task.
- *The exchange rate.* Cost saved for each second of tool time added is reported for every rule. It decides nothing while time is unconstrained, and it is the record the later time work starts from.
- *Repeats.* The same agent on the same task can differ by several round trips between runs, so each arm is run three times and medians are compared.
- *What is run when (user accepted the cost, 27 November 2026).* For steering: Sonnet, three repeats, all 20 tasks at the default setting, and the effort curve on a subset of six tasks. At milestones: Sonnet and Opus on all 20 tasks at the chosen setting, and the holdout.
- *The agent decides when to call the tool.* The tool arm offers Gather Context beside the ordinary reads with its own description and prescribes no first call, so the measure includes whether an agent chooses it and returns to it.
- *Correctness is judged by Claude directly* against the frozen truth, as the sufficiency standard of 26 November already requires. No separate judging agents.
- *The unaided baseline is measured first.* It gives each task's cost and round trips without the tool, which is what the reduction is measured against. A task an agent finishes in two round trips cannot show a saving and is marked as such.
- *The narrow set counts four times the broad set* in the combined score. The two are always also reported apart, so a gain on one cannot hide a loss on the other.
- *Diagnostics stay diagnostics:* required passages delivered, reply bytes, seconds, and the kernel's own report of reads, passages and time by stage.
- *Twenty tasks steer and do not prove.* One task changes the score by five to seven points. Claims wait for the holdout.

**Order of work.**

1. Correct the three faults the first kernel baseline showed, one at a time: the default retention budget ends a request before the guidance, cited-file and structure stages; those stages did nothing on Munarium; guidance finds no file when the root listing runs past one page.
2. Build the task sets: choose and freeze the 6 broad tasks, write and freeze the 14 narrow tasks and their truth.
3. Measure the unaided baseline for all 20 tasks with both acting models.
4. Run experiments, one change at a time, each behind its switch, each measured on the same tasks: the exact census as the first move; the map-first reply and its size target; repeated use (honouring `known`, narrow follow-up calls, what each reply offers for the next call, and the tool's description); absence and scope; the stop rule; the folder scope; then the least model use that still pays for itself. Each is kept or dropped on cost saved with results no less correct.
5. At each checkpoint, run the acting comparison and the holdout.
6. Admitting the 4B model to the runtime follows only if step 4 shows the model earns its place.

**Topical index and real-time reads (user, 27 November 2026).** After the first real-data results the user asked whether indexing every file in full is the wrong foundation, and authorized a prototype of a small topical index of files with search and parsing done on the live files. Its plan, measures and gates are in [topical-index-research.md](topical-index-research.md). Until a gate there says otherwise, the order of work above is paused after step 1, and the model-dose experiment below is applied to whichever gathering wins.

**How much local inference (user, 27 November 2026).** The amount of inference by the 8-bit Qwen model needed to reach the cost reduction is itself measured. The same tasks are run with the model at several doses: none, then about 50, 150 and 600 judgments a call. Recorded at each dose: agent cost and round trips against the unaided baseline, correctness, judgments per call and per task, GPU seconds and peak GPU memory, and the cost saved for each hundred judgments. The dose where the curve flattens is the budget the runtime version is built to, and a flat curve from zero means no model is needed. Until the model is in the runtime, the dose is applied in the evaluation tooling: the one-tool server takes the kernel's full reply and has the model, with 8-bit weights expanded to FP16, choose which passages go into the small reply. Everything else is held the same, so the difference between doses is the model's choice alone. This runs after the small reply and the census, because both change how many candidates there are to choose among.

**What is not changing.** The sealed scenarios stay closed. Earlier results keep their identities and their place in the record. The gate's own task kinds and rubric are the user's to change and are not changed here. Passage delivery on the four tuned tasks remains available for comparison with the prototype runs.

### A decision Sol can turn into an executable test

Use a synthetic provider fault to exercise the control loop end to end. The prompt supplies a quoted button label and a provider name. Construct a UI file that emits an action string, a handler registered under that string, two provider branches and a high-ranked guide that mentions the error. Give the reference validator the expected branch behavior and the two source witnesses. Give the product only the task and scope. Assert that the first useful read identifies the action or its receiver. A later read must use a lead found in that evidence. A complete handoff must include the failing branch, the working control and their effect before any network call. Accept a route starting at the exception if it obtains the same proof.

Change the action name in both files, move the handler, and give a second root a misleading copy under the original path. Rerun the identical task. The first trace must point to the old name, the second to the new one in the selected root. Remove the deciding branch in a third fixture. The result must disclose the missing proof and its search scope. This covers learning a next move from evidence, taking a shorter valid path, and stopping when the evidence runs out. Apply the same pattern to the distinct obligations in the table.

### Acceptance matrix for the 20 tasks

Each row is an evaluator instruction for Sol. Use the case link to inspect *both* reference paths. Freeze the expected behavior and counterexample before replay. The production service sees just the original task and authorized scope. The last column is a condition that must prevent giving credit for a convincing but incomplete path.

| Independent walkthroughs | Evidence or decision Gather Context must enable | Counterexample the evaluator must distinguish |
| --- | --- | --- |
| [Bookbuilder C1](#bookbuilder-c1) | Find a comparable provider, follow save/test/model selection/generation, and identify the catalog generator. | Recognize a routed vendor model as a different entity. External vendor protocol must have independently supplied provenance. |
| [Bookbuilder C2](#bookbuilder-c2) | Reach the pre-HTTP default-model branch from either the UI action or exception clue; return the temporary-key path, saved fallback and control. | A provider registration or the exception string alone cannot locate the failing branch. |
| [Bookbuilder C3](#bookbuilder-c3) | Classify key readers by run, retry, parallel stage, diagnostic and administrative usage; locate execution ownership and the key-lifetime policy. | A singleton or stage cache, or a roster missing another call path, cannot establish run-scoped reuse and next-run rotation. |
| [Bookbuilder W1](#bookbuilder-w1) | Connect both controls to preparation, cost disclosure, persisted artifacts, the returned preview and the sequel/prequel handoff. | Do not infer a fixed charge or equate a saved artifact with a returned preview. |
| [Bookbuilder W2](#bookbuilder-w2) | Resolve a winning template and fallbacks, follow block and token processing into a caller, and pair stale claims with deciding ranges. | An architecture paragraph or a sync-only read cannot substantiate the production async order. |
| [Bookbuilder T1](#bookbuilder-t1) | Enumerate shipped styles, find the separate two-paragraph branch, and give independent DOCX-body assertions at and above ten with small words. | A helper-only test, an assertion satisfied by the TOC, or one example per guessed mode is insufficient. |
| [Bookbuilder T2](#bookbuilder-t2) | Recover quick/full commands, selectors, key/skip prerequisites and actual automation triggers; separate offline replays from paid cases. | A test filename, integration tag or empty indexed workflow list cannot establish execution or global CI absence. |
| [Bookbuilder A1](#bookbuilder-a1) | Trace one coherent request across the bridge and provider through durable save, progress and final callback. Bind the same action/provider on the relevant hops. | Reject a route assembled from incompatible providers or a trace ending at HTTP. |
| [Bookbuilder A2](#bookbuilder-a2) | Classify wired runtime, allowed distribution, packaging, release regressions and stale docs; distinguish source evidence from a platform run. | An OS branch or an asset RID cannot qualify an installer. |
| [Bookbuilder A3](#bookbuilder-a3) | Check product identity before copying a sibling, then follow authoring, validation, bundle and installer. Compare impact for update versus new identity. | Reject an unconditional product/count increment when the requested identity already exists. |
| [Munarium C1](#munarium-c1) | Trace the borrowed consumer through defaults, env, API, persisted-row fallback and all four client surfaces. | A fresh install is not a legacy-row test. Preserve the eight tenant values and distinguish the generic bridge from a refusing typed client. |
| [Munarium C2](#munarium-c2) | Find state writer, stored-state reader, time source and pre-side-effect admission; specify a clock-driven test between expiry and sweep. | A typed error on an already-closed row does not prove timely idle refusal. Account for zero-disable and refreshed activity. |
| [Munarium C3](#munarium-c3) | Reconcile count sites across both operations, CLI, server routes and SDKs; separate byte and framing guards. Identify tests and docs at the boundary. | Reject a bare replacement of 500; distinguish unsupported client transport, 1,001 tiny files and one oversize body. |
| [Munarium W1](#munarium-w1) | Pivot from user vocabulary to native request/confirm, then trace time, identity, retention, already-open sessions and actual SDK spellings. | A retained row does not imply a successful later turn. Separate cancellation of pending removal from confirmed removal. |
| [Munarium W2](#munarium-w2) | Join both handlers to governance and atomic write; distinguish stored disputed claims, head conflicts, re-evaluation and later direct/facts reads. | A single read projection or an unqualified statement that blocked claims disappear fails. |
| [Munarium T1](#munarium-t1) | Locate a real scripted HTTP seam. Classify present and missing sync/async sends, key equality and failure phases using authentic operation specs. | Helper-only checks or sync-only proofs cannot cover async. Ordinary writes and content-addressed uploads differ. |
| [Munarium T2](#munarium-t2) | Recover commands, cwd, applicable gates and proof limits for the route and migration; identify the actual diff and predecessor needed for specific checks. | OpenAPI generation alone is not drift comparison. Fresh schema creation does not establish a populated upgrade. |
| [Munarium A1](#munarium-a1) | Trace the selected single-collection route through authority, configuration, query preparation, ranked PostgreSQL legs and the provenance envelope. | No optional inference stages does not imply no built-in vector retrieval; distinguish provenance from execution disclosure. |
| [Munarium A2](#munarium-a2) | Compare unset/explicit policy, extractor identity, old/new publications, coverage, failure class, escalation and OS resource controls. | A published index need not change immediately. Separate observed fixtures from population-wide predictions. |
| [Munarium A3](#munarium-a3) | Follow existing recovery and by-ID reads, then inspect tenant, UID and current authority in rows, totals and pagination before proposing reuse. | A tenant filter or display-side row filter is insufficient. Include same-UID reduced authority and a second tenant. |

### Replay commands and artifacts Sol must deliver

Implement the following **proposed harness commands** in M0. They are the interface to build. Run them from the repository root in the checked Windows shell. The control and candidate runs take only the frozen prompt and the authorized scope as task-specific inputs. Keep reference annotations in the evaluator process.

~~~powershell
$campaign = (Resolve-Path 'artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1').Path
$trial = Join-Path $campaign 'passes/gather-context-development-r1'
$runner = 'scripts/arbiter-answer-eval/gather_context_replay.py'
$spec = Join-Path $trial 'spec.json'
$reference = Join-Path $trial 'reference.json'
$control = Join-Path $trial 'control'
$candidate = Join-Path $trial 'candidate'
$comparison = Join-Path $trial 'comparison.json'
python -B $runner freeze-reference --campaign-root $campaign --spec $spec --output $reference
python -B $runner capture --spec $spec --arm control --output $control
python -B $runner capture --spec $spec --arm candidate --output $candidate
python -B $runner compare --reference $reference --control $control --candidate $candidate --output $comparison
python -B $runner report --comparison $comparison --judged (Join-Path $trial 'judged.json') --output (Join-Path $trial 'report.md')
~~~

For each command, check the exit status before the next command. The harness must refuse a reused output identity or a partial capture presented as complete. Resume from an authenticated completed case using the same input identities. Keep cancelled, timed-out and excluded cases in the denominator with their actual disposition. A fresh comparison uses a fresh run identity.

1. **Prepare the private spec.** Preflight the trial directory. Derive the spec from the existing development scenario freeze, the two settled fixture publications and manifests, and the frozen Windows pass under `passes/07-windows-complete-evidence-r1`. Record the full commits, path to the existing handle for each participating runtime, expected publication/root/member identities, the 20 scenario IDs and instruction digests, control and candidate binary digests, model/tokenizer identities or disabled state, limits, deadlines, order/seed and exact tool offer. Include the relative input-document names and digests on the evaluator side. Verify grants at runtime from the handle. Keep credentials in the established private store. Use the current native budget values in the spec. Assign a new spec identity for every changed setting.
2. **Freeze the reference.** Produce one `reference.json` with two path records per case, the verified obligations and acceptable alternatives from M0, the exact proof boundaries, and evidence of the source check. An obligation identifies the clause it supports, source revision, enough location information to verify the claim, applicable scope and acceptable stop reason. Encode unresolved or inaccessible evidence explicitly. Verify all 20 original instructions against the scenario freeze before assigning an obligation. The annotation reviewer should be able to open each source citation without reading product output.
3. **Capture each arm.** Use the existing native job and record the actual dispatched readers. For each attempt write `actions.jsonl` (case, round, need, parent action/citation, grounded selector, requested read, reason, outcome, cost and stop), `passages.jsonl` (case, publication/member/path, original location, range or cursor, content digest and eligibility), and `handoff.json` (verbatim delivered content, cited passage IDs, partial/refusal status, byte count and elapsed time). Record a scan’s eligible, examined, excluded and matched populations and its cutoffs. Keep retained passages separate from those the caller actually received. Do not infer an internal read from the final reply. When instrumentation cannot observe a value, record it as unavailable.
4. **Compare and report.** For every obligation in both reference paths, join the actually delivered passages to the correct revision and range; inspect equivalent alternative evidence when ranges differ. Require the content to support the conclusion. Record the first action where each arm either resolved or lost the lead. Mark per-path reproduction, additional support, unresolved questions, unsupported claims and cost deltas by case. Keep the measured native and agent call units distinct. Before scoring a new or relocated witness, reopen the cited range against its recorded publication, confirm its content digest and record which proposition the bytes support. A disagreement goes to the blinded reviewer with both ranges. The report takes blinded judgements from the existing evaluation pipeline via a lossless adapter into `judged.json` (case, masked arm, grader disposition, correct finish, missing/misleading facts, citations and external turn count). Reject a missing expected judgement in qualification mode. For an unjudged offline report, use `report --comparison $comparison --unjudged --output (Join-Path $trial 'diagnostic.md')`. Make `--judged` and `--unjudged` mutually exclusive. An unjudged report may describe routing and byte coverage only.

### Test order and commands

**Test transfer to unfamiliar tasks.** Freeze the candidate, then have an evaluator write a fresh task for a different fixture repository without consulting the 20 prompts or their traces. Vary the decision the caller needs as well as the vocabulary and layout. For example, ask which notice period applies on a given date in a collection containing a handbook, a dated amendment and a cross-reference. Keep the expected conclusion and supporting ranges with the evaluator. Give the unchanged policy only the task, authorized collection and usual budget. Check that its citations justify the conclusion. In a variant with the decisive amendment withheld, check that the handoff identifies the unresolved condition and the evidence needed to settle it. Record transfer results separately from the 20 regressions. If a failure leads to a code change, retain it as a regression and assess the revised policy on another fresh task. Use independent fixtures for this check and keep the campaign’s sealed cases closed.

On resumption, start with the Windows CUDA model comparison and four-case trace above. Reuse the existing comparator, kernel and transport receipts at their recorded scope. For changed behavior, run a focused regression before replaying the affected real cases. Freeze each reviewed candidate before running all 20 with Arbiter on against the settled control. A development replay remains a development result. Keep case IDs and expected witnesses in the evaluator and out of production routing. Exercise transfer with renamed literals, moved files, a tempting wrong-project match, a changed scope, and a relevant owner appearing beyond the first page. Assert the useful *next decision and evidence* after each changed input, allowing more than one valid route.

Use the existing Windows target after verifying its identity, free space and ownership. Select affected crates and features against the current validation matrix. The following commands cover the first no-model kernel and harness slice after the proposed tests exist. They are unit regressions and never stand in for the Arbiter-enabled real-task runs. Keep the same features for listing and running tests. Check counts per selector and use the actual fully qualified test path from the list if the owner changes. Run the applicable service, CLI/MCP, generation, CUDA or extension matrix rows when those layers change. Record the target, features, timing and selected counts for every native run. [Validation matrix](../../CONTRIBUTING.md#agent-validation-matrix) · [Command reference](../../CONTRIBUTING.md#agent-command-reference)

~~~powershell
$target = Join-Path $campaign 'answer-loop/target-cuda'
$env:CARGO_INCREMENTAL = '1'
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_gather_context*.py'
python -B -m unittest discover -s scripts/arbiter-answer-eval -p 'test_replay.py'
cargo fmt --all -- --check
python -I -B scripts/dev-check.py --offline -p airk-kernel --target-dir $target --skip-fmt
$tests = 'test(arbiter::answer::retrieve::tests::) | test(arbiter::answer::gather::tests::) | test(arbiter::answer::follow::tests::) | test(arbiter::answer::coverage::tests::focus_absent_uses_canonical_order_and_never_lexically_overclaims)'
cargo nextest list --locked --offline -p airk-kernel --lib --no-default-features --target-dir $target -E $tests
cargo nextest run --profile ci --locked --offline -p airk-kernel --lib --no-default-features --target-dir $target --timings -E $tests
pwsh -NoProfile -File scripts/check-repo.ps1
git diff --check
~~~

When Core types change, run the same scoped Clippy and nextest checks for `airk-core` using its `arbiter::answer_loop` selector, plus the affected schema and client checks. Add selectors for any new test modules outside that initial filter. For M5, use the current matched Windows service/client pair. Build the `airk-cli` integration target `combined_reads` on that pair and pass its prebuilt validator to `scripts/check-service-clients.py --answer-validator`. Its `retained_current_answer_pair_validates_full_transport_results` check consumes real transport receipts. Set `$pair` to the verified runtime directory of that matched pair and `$validator` to its prebuilt `combined_reads` executable. For the model-enabled full preflight run `python -B scripts/check-service-clients.py --runtime-dir $pair --files 16 --model --answer-validator $validator --report (Join-Path $trial 'transport.json')`. Use the same documented build identity for both inputs and record the receipt. A zero-test selector, an unverified build, a mocked renderer alone or a truncated response cannot qualify that path. Stop at the first failing command, capture its input identities and reproduce on the smallest matching fixture. After a repair, repeat the failing check and affected consumers. Reuse unchanged earlier receipts.

### How Sol should judge the result

**Sufficiency standard (user decision, 26 November 2026).** The agent carrying out this plan judges each result directly. Do not launch separate headless sessions to judge. A result succeeds when the reply gives an AI agent most or all of what it needs and the agent can complete its preparation with at most two follow-up tool calls. Identity with the reference notes is not required. In this section, read a correct finish as a sufficient result. The last two rows of the table and the released-to-run paragraph describe the completion plan's formal gate, which still names blinded judges and the earlier rule. Whether that gate adopts this standard is the user's decision at the Windows review. [Sufficiency standard](answer-evaluation/rubrics.md#sufficiency-standard-from-26-november-2026)

| Decision | Required evidence |
| --- | --- |
| Did we reproduce each path’s useful work? | For every task, score the two paths separately against their source-checked obligations. Show delivered supporting citations, contradictions, justified scoped absences, and any open clauses. Give credit to any independently verified equivalent route. A correct file name or a high-ranked hit alone earns no support credit. Missing evidence that either investigator needed remains visible. |
| Did the caller benefit? | On matched inputs compare first-delivery and full-session coverage, correct finish, misleading claims, external turns to a correct finish and partials. Record the real differences in orientation and scope. The two historical walkthroughs supply behaviors to reproduce. Claims about fewer calls, bytes or lower latency require matched measurements. |
| Is the candidate better? | Mark an improvement per task when it adds verified deciding evidence within the same budget or preserves the supported handoff and correct finish at lower measured cost. Report unchanged, mixed and regressed cases individually. Report internal rounds, files/bytes scanned, retained/delivered bytes, model tokens, external calls, elapsed time and timeouts separately. Preserve the losing example and its first divergence. |
| Is a product evaluation admissible? | The complete-evidence review trigger has already fired at 2/20. Its threshold is 12/20 correct and at least five correct in each repository. Any newly authorized diagnostic records its own scope. Keep the failed disposition and get the recorded review decision before restarting the paused baseline or change sets. |
| Did a product series pass? | Run the frozen comparison with the approved OpenAI actors and blinded judging through the existing pass driver. Compare to the stronger of Parse and Search. A substantial win reaches a correct finish at least two turns earlier, or reaches a correct finish when both comparators fail. Require at least 12/20 wins, at least five per repository, at most two worse, no task category worse on average, coverage of at least the essentials Search covers, and a timely reply or disclosed partial. Report counts and ties by repository and task kind. The diagnostic reference score and this gate answer different questions. |

**Select a candidate.** Reject revisions that grant a new scope, claim unsupported facts, discard a required obligation without disclosing the gap, or exceed the agreed resource limits. Compare eligible candidates case by case to both reference paths and the native control. Prefer the one with more correct supported finishes, then earlier correct finishes and lower measured cost at equal quality. Keep ties and quality–cost trade-offs visible to the reviewer. A single superior example cannot establish that all 20 paths improved.

For a released-to-run series, use the existing driver with a reviewed pass spec, e.g. `python -B scripts/arbiter-answer-eval/run_pass.py --r-root $campaign --spec $passSpec --stage all`. Use the existing Codex spec with `provider` set to `codex`, both scenario and prompt freezes, strict fresh receipts and judges `j1` and `j2`. Pin the executable, catalog, isolated home and freeze digests through `codex_config`, and pin the actual tool offer per arm. For the main comparison select `gpt-6.2-sol` at medium effort and `gpt-6.2-astra` at medium effort to judge. Set `$passSpec` to that reviewed series spec. The historical defaults select a different provider. A stopped offline capture can be resumed for diagnostics; a judged Codex series requires fresh sessions under its own rules. Keep the reconstruction spec above evaluator-side. Use the existing rubric and second-judgement rules. The final Windows review needs the exact source/candidate and publication identities, per-case comparisons, actual resource use, verified spans that reached the caller, blind finishes and the first observed cause of every regression. [Gate and comparators](completion-plan.md#3-stage-l7-with-a-windows-first-review) · [Evaluation rubric](answer-evaluation/rubrics.md)

### When a case fails

| First observed divergence | Repair and rerun |
| --- | --- |
| Wrong root, stale identity or mismatched model/offer | Invalidate that comparison. Reconcile handle, scope, source, binary and offer. Repeat the affected arm on the frozen pair. |
| No owner candidate, overly broad hits or a vocabulary miss | Inspect the first selector against the prompt. Add a generic literal, inventory or evidence-derived pivot regression and compare with the original discovery policy. |
| Correct owner nominated but branch, consumer or assertion missing | Inspect range boundaries and the open need. Reproduce with a minimal enclosing-unit or registration fixture and replay the affected path. |
| Candidate declares an exhaustive result after truncation | Verify eligible population, paging, filters and receipt. Require an exact completed scan or a clearly scoped partial. Test both outcomes with the same corpus. |
| Evidence retained internally but lost in the reply | Compare retained versus delivered passage IDs and byte allocation. Fix selection, deduplication or bounded composition and replay identical inputs. |
| All deciding evidence delivered, caller still wrong | Freeze those bytes. Reorganize the reply and judge it directly under the sufficiency standard. Inspect omissions or misinterpretations before expanding retrieval depth. |
| Deadline, cancellation or retry leaves a job unsettled | Stop new admissions for the affected run. Inspect job/worker state and retained diagnostics. Reproduce under an injected clock/cancellation signal, settle owned writers and verify the terminal state before retrying. |

**Handoff for Windows review.** Deliver a signed code milestone with the scoped test receipts, the private reproducible comparison and a reviewable 20-row report with both reference columns, their evidence limits and the candidate disposition. Include the case with the largest observed improvement and the first remaining failure with their trace and delivery differences. If no gain survives the comparison, record that result and the smallest experiment that could distinguish the remaining explanations. The next executable action upon resumption is to judge the 20 settled control replies under the sufficiency standard, then build single-stage Arbiter on Qwen3-Reranker-4B, install the private trace and report validation needed to observe it, and run the four real tasks with Arbiter on. Judge each reply directly under the sufficiency standard. The Windows review remains incomplete until the Arbiter runs, the full 20-task replay, and the recorded disposition are present.
