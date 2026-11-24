# Answer evaluation scenarios

## Current Windows CUDA development inventory

Windows L7 uses ten development scenarios each from munarium at
`9948345d1d148525f644bc4b8bc5e6da9cf73ca0` and bookbuilder-v2 at
`238b22a722ceb78447d72e39585fd6a7d4d3dd0c`. The maintained harness reads
their individual hashes from the private development freeze. The current
freeze also lists the bookbuilder sealed set. The munarium sealed set is
due before the first L7 change set is fitted. See the
[Answer loop plan](../final-plan-fp16.md#128-work-order) for the work order.

## Original one-turn inventory, 21 November 2026

These are the 20 development scenarios of the Answer evaluation: ten for the
airk repository and ten for the bookbuilder repository (WritingForge Pro, a
C#/.NET application with a Vue front end). Each is an instruction written the
way a developer would give it to an agent. The instructions below are copied
verbatim from the frozen scenario files.

Each repository has the same mix of ten kinds of task, in the same slots:
three coding (C1 to C3), two writing (W1, W2), two test or QA (T1, T2) and
three architecture (A1 to A3).

A second, sealed set of ten scenarios per repository was written and frozen
at the same time. It is not listed here and has not been run. It runs once,
on the user's word after an inspection, on the configuration named before it
is opened.

## Freeze

| Item | Value |
| --- | --- |
| Frozen at | 21 November 2026, 15:56:26 UTC |
| Freeze file SHA-256 | `0a7d7b0a478b6e9d7cdeca9bc363452a40e6afc8ae4db5f2af557856d53e7806` |
| Files in the freeze | 40: the 20 development scenarios and the 20 sealed ones, each with its own SHA-256 |
| airk commit | `1fe7b6003574e4e385896120c62b99acbca00a3a` |
| bookbuilder commit | `238b22a722ceb78447d72e39585fd6a7d4d3dd0c` |

Every scenario file names the commit its reference notes were written
against, and each matches the commit above. Every pass checks each scenario
file against its SHA-256 in the freeze and refuses to run if one differs.
The freeze file, the scenario files and their reference notes are private:
they live in the git-ignored campaign folder
`artifacts/v190/final-plan/exec-20261116T052942Z/answer-eval-r1/`.

## Reference notes

Each scenario carries between four and ten reference notes, written by
reading the repository directly (file reads, search and git), never through
the AIRK tools under test. A note names a file, a line range and, where
there is one, a symbol, and is marked essential or helpful. Every essential
note states, in one sentence, the fact an agent must take from that place.
Some notes list alternative places that establish the same fact equally
well. The notes' facts are not published here, so that the scenarios stay
usable for later passes.

| Repository | Notes | Essential | Helpful |
| --- | --- | --- | --- |
| airk | 90 | 53 | 37 |
| bookbuilder | 90 | 61 | 29 |
| Both | 180 | 114 | 66 |

The table columns below are: slot, kind, the instruction, the number of
notes, how many are essential and how many helpful, and the first 16
hexadecimal digits of the scenario file's SHA-256 (the full value is in the
freeze file).

## airk

| Slot | Kind | Instruction | Notes | Essential | Helpful | File SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | coding | Add a file-extension filter to search and references, for example `path_suffix: ".rs"` to return only Rust files, next to the existing language, evidence_kind and exclude_path_prefix filters. It has to work the same from MCP, the airkctl CLI and Workbench, including multi-index reads, with validation and tests. | 9 | 5 | 4 | `da6eb1d932e5bf92` |
| C2 | coding | The docs say Management → Service → Concurrent requests… lets you pick 4, 8, 16, 32 or 64, but the current Service Admin page gives no way to change it: it only shows the value, and only the GPU switch works. Find out why the control is missing and restore it without breaking the other resource settings. | 9 | 6 | 3 | `8bec921da5667ea6` |
| C3 | coding | Search hits are too short for our agents. Raise the default focused snippet size for search results from 768 bytes to 1,536 and update everything that depends on it, so that every surface keeps returning the same results. | 8 | 5 | 3 | `fdef969f8de06c5b` |
| W1 | writing | Write the user-guide section that explains AIRK's search modes (lexical, vector and hybrid): what leaving the mode out does, what happens while vectors are still being built or the embedding model is unavailable, and how a user can tell from a reply which mode actually ran. | 8 | 5 | 3 | `41984571ab44b89f` |
| W2 | writing | Write an explainer for a new contributor on how a source file becomes searchable in AIRK: discovery and ignore rules, extraction, chunking, embedding and publication, and what a reader sees while that work is still in progress. | 9 | 6 | 3 | `12e86f61077dd288` |
| T1 | test | Add tests proving that common credential files (.npmrc, SSH private keys, kubeconfig, Terraform state and the like) are never indexed unless an .airkignore negation re-includes them, and that .env files and .pem/.key files stay excluded even with a negation. Cover both the full scan and the direct-path checks used for selected files. | 8 | 5 | 3 | `25c37262e0a38993` |
| T2 | test | I changed the source watcher so renamed directories are picked up faster. Before I send it for review, list the checks I must run, with the exact commands, and say what each one does and does not prove. | 9 | 4 | 5 | `d62673d43cd4e901` |
| A1 | architecture | Trace an airk_search call from the moment an agent's MCP request reaches the service to the ranked hits it gets back, naming each component it passes through and the contract that governs it. Leave the optional Arbiter re-ranking out of it. | 10 | 5 | 5 | `4e1b99b05fe2be6d` |
| A2 | architecture | We're thinking of changing the chunk size limits used when files are indexed. What would that affect (stored formats, vectors, identities, reuse of existing indexes and any recorded decisions), and what would we have to change alongside it? | 10 | 6 | 4 | `707f3c7458e4e42b` |
| A3 | architecture | We want a call-hierarchy read tool (callers and callees of a function, a few levels deep). Where should it live, how does it relate to what already exists, and which registries, parity rules and tests does a new read tool have to satisfy? | 10 | 6 | 4 | `024539c293af9f62` |

## bookbuilder

| Slot | Kind | Instruction | Notes | Essential | Helpful | File SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | coding | Add Mistral as another AI provider next to OpenAI, Anthropic, Gemini, DeepSeek, OpenRouter and Ollama. Users should be able to enter and test a Mistral API key in Settings, have it stored the same way as the other keys, pick Mistral models, and run generation through it. | 10 | 7 | 3 | `edff1a0e78438956` |
| C2 | coding | During first-run onboarding, choosing Google Gemini, pasting a valid key and pressing Test connection fails with a message like "Specified argument was out of the range of valid values". The other providers test fine. Find the cause and fix it. | 8 | 5 | 3 | `7cee3678a81d68ee` |
| C3 | coding | Every AI call re-reads the provider row from the library and decrypts the API key again, including each retry. Change this so a key is decrypted once per generation run and reused for that run's calls, and a key the user saves mid-run is picked up by the next run. Cover every place that reads a provider key, and keep the key-handling rules intact. | 10 | 6 | 4 | `33fdae2a840d9537` |
| W1 | writing | Our in-app help only mentions sequel preparation for imported Word manuscripts in passing. Write a dedicated help page for authors: what "Let the AI learn this book for sequels" does, where they can run it (during import or later), what it costs and produces, what they see when it is done, and how it leads to starting a sequel or prequel. | 9 | 5 | 4 | `5bc76f8e7f39b5be` |
| W2 | writing | Write a short explainer for a new contributor on how prompt templates work in WritingForgePro: where the built-in templates are defined, how the template used for a stage is chosen when there are project, genre and global versions, and how placeholders and prompt blocks are filled in before the request goes to the model. Point out anything in the existing architecture document that no longer matches the code. | 9 | 6 | 3 | `f038336ea9a7e8d0` |
| T1 | test | The Word (DOCX) export has tests for margins, running headers, title page and scene breaks, but nothing checks what it writes as chapter headings. Add tests covering the chapter heading for every heading style our built-in export profiles use, including how chapter numbers above ten and chapter titles with small words are written. | 8 | 6 | 2 | `4e5967256104f419` |
| T2 | test | I'm about to change how the generation pipeline orchestrates stages. Which test suites cover the pipeline, how do I run them on my machine (quick loop and the full gate), which of them need real provider keys, and does anything run them automatically? | 8 | 6 | 2 | `945d68b546991a25` |
| A1 | architecture | Trace what happens when an author starts a generation run from the Generate screen: from the Vue store call, across the Photino bridge, through the pipeline orchestrator and a stage, to the AI provider HTTP call, and back until the output is saved in the .bbpro project and progress reaches the screen. Name each component on the way. | 10 | 8 | 2 | `a0b619a8eb62e983` |
| A2 | architecture | We are considering shipping a macOS build again. Assess what that would take: what macOS runtime code still exists and is wired up, what in the build, release and installer path is Windows-only, which project rules and tests would have to change, and where the platform documentation is out of date. | 9 | 6 | 3 | `c7b8192d3e70e70a` |
| A3 | architecture | We want to ship a new bundled pipeline product for grant proposals. Where does a new product's source go, what files and contract must it have, how does it get compiled into the installer payload, which validation rules must it pass, and what tests or counts will break when we add it? | 9 | 6 | 3 | `d4b4897486c9a5c4` |

## How the instructions are used

- The agent receives the instruction and nothing else about the repository.
  Its first call passes the instruction unchanged: as the question for
  Answer, as the query for Search and as the text for Parse.
- A distinctive phrase of each instruction is the leak probe for the airk
  index (see [00-setup.md](00-setup.md)): it is searched for after the index
  is built, and a hit from one of the benchmark's own files would refuse the
  index.
- The user directed that the first round (the baseline and change set 1)
  run on the airk scenarios, because the bookbuilder index was still being
  built. Both repositories are used from change set 2 on.
- A correction to a scenario after the freeze is allowed only for a factual
  error in its notes, and is recorded with its reason in the report of the
  series that makes it. None has been made.
