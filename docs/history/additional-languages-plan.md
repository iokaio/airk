# AIRK Language Coverage: Gap Analysis and Recommendations

AIRK already has a strong foundation. Its 23 grammar entries cover most mainstream general-purpose programming languages, including the largest ecosystems used by coding agents.

However, I see three significant gaps:

1. Missing programming languages, especially Dart, Scala, Elixir, R, and Objective-C.
2. Missing infrastructure and build languages, especially Terraform/HCL, Dockerfiles, Protocol Buffers, and Gradle DSLs.
3. Incomplete parsing of languages AIRK already claims to support, particularly Vue, Svelte, SCSS, HTML with embedded JavaScript, and C/C++ headers.

For AIRK's purpose, accelerating AI coding agents through fast, local, fact-oriented retrieval, I would prioritize usefulness to coding agents over the sheer number of supported languages.

GitHub's 2025 Octoverse data reinforces this direction: TypeScript, Python, JavaScript, Java, and C# are among its leading languages, while HCL also appears in its top ten. AIRK covers the first five but not HCL.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.blog\&sz=32)

The GitHub Blog

+1



## 1. Highest-priority missing languages

These are the additions I would make first.

| Priority | Language or format | Extensions                       | Why AIRK needs it                                                                  |
| -------- | ------------------ | -------------------------------- | ---------------------------------------------------------------------------------- |
| P0       | Terraform / HCL    | `.tf`, `.tfvars`, `.hcl`         | Infrastructure resources, modules, variables, providers, outputs, and dependencies |
| P0       | Dockerfile         | `Dockerfile`, `.dockerfile`      | Container builds, stages, image dependencies, commands, and runtime configuration  |
| P0       | Protocol Buffers   | `.proto`                         | Service contracts, messages, RPCs, fields, and imports                             |
| P0       | GraphQL            | `.graphql`, `.gql`               | Schemas, operations, types, fields, fragments, and API relationships               |
| P0       | Dart               | `.dart`                          | Flutter applications and cross-platform development                                |
| P0       | Scala              | `.scala`, `.sc`                  | JVM systems, Apache Spark, and enterprise services                                 |
| P0       | Elixir             | `.ex`, `.exs`                    | Phoenix applications, distributed services, and OTP                                |
| P0       | Objective-C        | `.m`, `.mm`                      | Legacy and mixed-language Apple applications                                       |
| P1       | R                  | `.r`, `.R`, `.Rmd`               | Statistical analysis, data science, and enterprise analytics                       |
| P1       | Perl               | `.pl`, `.pm`, `.t`               | Legacy applications, automation, and enterprise maintenance                        |
| P1       | Groovy             | `.groovy`, `.gradle`             | Jenkins pipelines, Gradle builds, and JVM development                              |
| P1       | Nix                | `.nix`                           | Reproducible development environments and package builds                           |
| P1       | Zig                | `.zig`                           | Native tooling and systems programming                                             |
| P1       | Julia              | `.jl`                            | Scientific computing, numerical software, and research                             |
| P1       | Clojure            | `.clj`, `.cljs`, `.cljc`, `.edn` | JVM and JavaScript functional systems                                              |
| P1       | OCaml              | `.ml`, `.mli`                    | Compilers, analyzers, and formal-methods software                                  |
| P2       | Haskell            | `.hs`, `.lhs`                    | Functional and research-oriented codebases                                         |
| P2       | Erlang             | `.erl`, `.hrl`                   | Telecom, messaging, and fault-tolerant systems                                     |
| P2       | F#                 | `.fs`, `.fsi`, `.fsx`            | .NET functional applications                                                       |
| P2       | Fortran            | `.f`, `.f90`, `.f95`             | Scientific and high-performance computing                                          |
| P2       | COBOL              | `.cob`, `.cbl`, `.cpy`           | Financial, government, and legacy enterprise systems                               |

These priorities are my recommendations for a general-purpose coding-agent tool, not measurements of AIRK user demand.

Tree-sitter has a broad grammar ecosystem covering many of these languages, although available grammars differ in maintenance, licensing, Rust packaging, and compatibility with Tree-sitter 0.25. Each addition needs qualification before pinning.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub



## 2. Languages that are technically covered but need real support

This is arguably more important than adding another dozen grammars.

| Existing AIRK coverage | Current problem                       | Recommended correction                                                     |
| ---------------------- | ------------------------------------- | -------------------------------------------------------------------------- |
| Vue                    | `.vue` parsed as HTML                 | Dedicated Vue parsing with embedded TypeScript/JavaScript and CSS          |
| Svelte                 | `.svelte` parsed as HTML              | Dedicated Svelte grammar with script and style extraction                  |
| HTML                   | Embedded scripts ignored              | Parse JavaScript, TypeScript where applicable, and CSS in embedded regions |
| SCSS                   | Parsed using CSS grammar              | Use an SCSS-specific grammar                                               |
| C / C++                | `.h` always treated as C              | Resolve header dialect using project configuration and context             |
| PHP                    | Incomplete call extraction            | Improve function, method, namespace, trait, and class extraction           |
| Ruby                   | Incomplete call extraction            | Improve method calls, mixins, modules, and dynamic dispatch candidates     |
| SQL                    | Only four CREATE statement categories | Support queries, CTEs, joins, views, procedures, triggers, and references  |
| YAML                   | Generic key extraction                | Add schema-aware extraction for GitHub Actions, Kubernetes, Compose, etc.  |
| JSON                   | Generic key extraction                | Interpret meaningful manifests and configuration schemas                   |

Dedicated Vue and Svelte Tree-sitter grammars already exist, as do Astro grammars that would be useful for modern web projects.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub

+2



The key point: AIRK should distinguish between grammar availability, syntax fact extraction, and semantically resolved facts. A language appearing on its supported list should not imply equivalent extraction quality.

## 3. Missing framework and project formats

For coding agents, these formats are especially important because they explain how a repository is built, configured, tested, and deployed.

| Format                                  | Extensions or filenames                                                | Facts worth extracting                                           |
| --------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Razor / Blazor                          | `.razor`, `.cshtml`                                                    | Components, directives, C# members, routes, dependency injection |
| Astro                                   | `.astro`                                                               | Components, frontmatter, imports, embedded code                  |
| MDX                                     | `.mdx`                                                                 | Imports, exports, components, embedded JSX                       |
| Jupyter Notebook                        | `.ipynb`                                                               | Code cells, imports, definitions, execution order metadata       |
| CMake                                   | `CMakeLists.txt`, `.cmake`                                             | Targets, dependencies, libraries, includes                       |
| Make                                    | `Makefile`, `.mk`                                                      | Targets, prerequisites, recipes                                  |
| Bazel                                   | `BUILD`, `BUILD.bazel`, `.bzl`                                         | Targets, dependencies, macros                                    |
| Gradle Kotlin DSL                       | `.gradle.kts`                                                          | Plugins, tasks, modules, dependencies                            |
| MSBuild / .NET                          | `.csproj`, `.fsproj`, `.vbproj`, `.props`, `.targets`, `.sln`, `.slnx` | Project references, packages, target frameworks                  |
| GitHub Actions                          | `.github/workflows/*.yml`                                              | Jobs, steps, triggers, dependencies                              |
| Kubernetes                              | YAML manifests                                                         | Workloads, services, resources, selectors                        |
| Docker Compose                          | `compose.yaml`, `docker-compose.yml`                                   | Services, networks, volumes, dependencies                        |
| OpenAPI                                 | `.yaml`, `.json`                                                       | API operations, schemas, response types, references              |
| GitLab CI                               | `.gitlab-ci.yml`                                                       | Jobs, stages, dependencies, artifacts                            |
| GitHub workflows and agent instructions | `AGENTS.md`, `CLAUDE.md`, `.github/copilot-instructions.md`            | Scoped instructions, paths, constraints                          |

Some of these are not programming languages and do not need dedicated grammars. Instead, AIRK can reuse existing JSON, YAML, XML, Kotlin, or Markdown parsers and introduce format-specific fact extractors.

For example, a `.csproj` file is XML, but understanding the XML structure alone does not tell a coding agent which projects depend on which others.

Razor deserves attention for .NET repositories. Dedicated grammars exist, including a Rust Blazor grammar, although the latter currently documents a Tree-sitter 0.20 dependency, so compatibility would need testing against AIRK's 0.25.10 version.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub

+1



## 4. Additional enterprise languages

If AIRK is intended to become a broadly useful, language-independent coding intelligence tool, I would also put these on its longer-term roadmap.

| Language                | Extensions                 | Target ecosystem               |
| ----------------------- | -------------------------- | ------------------------------ |
| Visual Basic .NET       | `.vb`                      | Enterprise .NET                |
| Delphi / Object Pascal  | `.pas`, `.dpr`, `.dpk`     | Legacy business software       |
| PL/SQL                  | `.pks`, `.pkb`, `.pls`     | Oracle databases               |
| T-SQL                   | `.sql` (dialect detection) | SQL Server                     |
| ABAP                    | `.abap`                    | SAP                            |
| Apex                    | `.cls`, `.trigger`         | Salesforce                     |
| Solidity                | `.sol`                     | Smart contracts                |
| VHDL                    | `.vhd`, `.vhdl`            | Hardware engineering           |
| Verilog / SystemVerilog | `.v`, `.sv`, `.svh`        | Hardware engineering           |
| CUDA                    | `.cu`, `.cuh`              | GPU programming                |
| WGSL                    | `.wgsl`                    | GPU shaders                    |
| GLSL                    | `.vert`, `.frag`, `.glsl`  | Graphics programming           |
| Assembly                | `.asm`, `.s`, `.S`         | Low-level and embedded systems |

For AIRK, SQL dialect awareness deserves particular attention. PostgreSQL, SQL Server, Oracle, and MySQL all have differences relevant to fact extraction.

## 5. My suggested implementation order

RECOMMENDED DEVELOPMENT ROADMAP

1. Phase 1: Repository intelligence

   Terraform/HCL, Dockerfile, Protocol Buffers, GraphQL, CMake, MSBuild, Make, GitHub Actions, Docker Compose.

   Highest immediate value for agents navigating unfamiliar repositories.
2. Phase 2: Modern web and application coverage

   Vue, Svelte, Astro, Razor/Blazor, SCSS, MDX, embedded JavaScript and TypeScript.

   Repair missing facts in existing and increasingly common application formats.
3. Phase 3: General-purpose language expansion

   Dart, Scala, Elixir, Objective-C, R, Groovy, Perl, Clojure, Zig, Julia, F#, and Nix.

   Expand the number of codebases AIRK can meaningfully understand.
4. Phase 4: Enterprise and specialist ecosystems

   COBOL, ABAP, Apex, Fortran, Delphi, PL/SQL, T-SQL, CUDA, and hardware description languages.

   Support legacy modernization, specialized development, and enterprise adoption.

## 6. One architectural recommendation

I would avoid making `syntax.rs` increasingly large with one-off language extraction logic.

Instead, use a language adapter interface with separate capabilities for parsing, extraction, and optional resolution:

```
trait LanguageAdapter {
    fn language_id(&self) -> &str;

    fn matches_file(&self, path: &Path) -> bool;

    fn parse(&self, source: &[u8]) -> ParseResult;

    fn extract_definitions(&self, tree: &Tree) -> Vec<Fact>;

    fn extract_references(&self, tree: &Tree) -> Vec<Fact>;

    fn extract_dependencies(&self, tree: &Tree) -> Vec<Fact>;
}
```

This is an illustrative starting point rather than a complete trait design. Real adapters also need error handling, source context, nested-language handling, and provenance.

Crucially, I would make language capabilities explicit:

- Lexical: Can AIRK search its contents?
- Syntax: Can it extract structurally meaningful facts?
- Cross-file: Can it infer or obtain dependency relationships?
- Resolved: Can it attach producer-backed symbol identities and references?

An adapter should never promote a syntactic guess to a resolved relationship merely because it implements the same interface.

For grammar selection, promising starting points include [tree-sitter-hcl](https://github.com/tree-sitter-grammars/tree-sitter-hcl) and [tree-sitter-containerfile](https://github.com/wharflab/tree-sitter-containerfile). Both have published grammar implementations suitable for qualification.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub

+1



## Bottom line

I would prioritize the nine Phase 1 formats plus the seven Phase 2 parser improvements before pursuing the long tail of general-purpose languages.

AIRK's competitive advantage is not saying that it parses 50 or 100 languages. It is being able to answer coding-agent questions efficiently and accurately:

- Where is this symbol defined?
- What calls this function?
- Which packages, projects, or services depend on this component?
- What configuration controls this behavior?
- Which files must change together?
- Which assertions are merely syntactic, and which have external semantic evidence?

Adding a language without exposing those facts is mostly a larger extension list.

The more valuable metric is how much repository understanding AIRK provides per millisecond and per token saved.
