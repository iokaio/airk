# Vendor-language evidence: L5.05–08

These adapters extract bounded local written evidence. They do not connect to Oracle, SQL Server, SAP or Salesforce, execute source, fetch vendor metadata, resolve references or establish compiler acceptance.

## Selection and implemented facts

| Plan row / profile | Selection | Implemented evidence and limits |
| --- | --- | --- |
| L5.05 `plsql` | `.pks`, `.pkb`, `.pls`; explicit policy for `.sql` | Package spec/body headers (body has a distinct syntax kind), procedure/function/trigger/cursor headers, selected SQL queries and written reference candidates. Headers end at a recognized delimiter; this does not establish complete procedural body understanding. |
| L5.06 `tsql` | Explicit policy for `.sql`; unconfigured `.sql` keeps the generic SQL profile | Isolated `GO` batch separators, CREATE/CREATE OR ALTER procedure/function/trigger/table/view headers, selected query/CTE/join/alias facts and written references. Repeated `GO` counts, SQLCMD directives, dynamic SQL and deployment semantics are not interpreted. |
| L5.07 `abap` | `.abap` | Period-delimited report/program, class/interface, method, form, module, include and selected interface/PERFORM reference statements. Selected enclosing statement scopes are preserved. Chained declarations and string templates are unsupported and make extraction partial; no compiler or runtime binding. |
| L5.08 `apex` | `.trigger`; `.cls` needs a visible strong marker or explicit policy | Native class/interface/enum/method/constructor/trigger syntax, receiver call spellings, inheritance/interface references and selected SOQL/SOSL query/reference candidates. No Salesforce object/schema resolution, security analysis or query execution. |

The `.cls` discriminator scans at most 4 KiB, skips comments and quoted strings, and requires a class plus a sharing-qualified class marker or `@isTest`. Ordinary ambiguous class text remains unselected. Original Visual Basic `.cls` negative fixtures ensure comments and strings do not trigger Apex selection.

## Dependencies and provenance

PL/SQL and T-SQL reuse the existing exact `tree-sitter-sequel = 0.3.11` grammar and original AIRK finite structural rules. The shared lexer retains the previous PostgreSQL/MySQL options. Vendor modes add T-SQL bracket escapes and temporary-table `#` names, plus Oracle alternative `q`/`nq` literals. No new Oracle or SQL Server grammar dependency was admitted. ABAP uses an original bounded statement recognizer and adds no parser dependency.

Apex pins `tree-sitter-sfapex = 2.4.0`, MIT, source commit `9247e2cd12490f1aac71cb212f726ef1a2b5f7f9`. The exact published archive SHA-256 is `7b97637acb3bcfc2518162ecf9b02a18b28a502cd7631bec5c2a26f4e5ea024b`. Its generated Apex parser uses ABI 14, within the local Tree-sitter 0.25.10 accepted range 13–15. Rust bindings use edition 2021 and `tree-sitter-language 0.1`; `cc 1.1` builds four bundled C parsers (Apex, SOQL, SOSL and log), with no external scanner or runtime downloader. These properties were inspected in the exact archive, not inferred from the repository's newer main branch. [Published API](https://docs.rs/tree-sitter-sfapex/2.4.0/tree_sitter_sfapex/), [exact crate archive](https://static.crates.io/crates/tree-sitter-sfapex/tree-sitter-sfapex-2.4.0.crate).

The archive omits its license file. The notice manifest therefore binds the MIT text from that exact source commit, copyright 2012 Anthony Heber, with SHA-256 `c3d045abbc0a8dec1b00737c0668f6dbb028d248521b74e79778c726671925ed`. [Exact license source](https://raw.githubusercontent.com/aheber/tree-sitter-sfapex/9247e2cd12490f1aac71cb212f726ef1a2b5f7f9/LICENSE).

## Evidence and work limits

Original UTF-8 byte spans and line locations are preserved, including CRLF and non-ASCII text. SQL grammar nodes have syntax provenance; vendor headers and ABAP statements have structural-rule provenance. ABAP and vendor SQL do not emit native declaration fingerprints or claim native continuity. Apex declarations use the existing bounded native fingerprint machinery; imported metadata and semantic identity remain unknown.

Structured source parsing is capped at 4 MiB; larger UTF-8 sources use [partial text-only extraction](ingestion-limits.md). Default shared work allows 500,000 node/token operations and 10,000 facts; source bytes consume the 16 MiB shared text budget. SQL token vectors and pending traversal queues are independently bounded. SQL headers stop after 4,096 tokens, parenthesis nesting after 64, and nested comments after 32. ABAP statements allow at most 4,096 word tokens and owner stacks at most 128; Apex owner stacks use the same depth limit. Display names are at most 4 KiB, aggregate displayed details at most 4 MiB, and serialized facts at most 16 MiB. Exhausted shared budgets return errors; unsupported lexical forms and malformed source report partial extraction rather than fabricated complete evidence.

## Qualification receipt

Native Windows passed all eight initial vendor unit tests and all twelve syntax-repair tests, including the new vendor lexer tests and previous PostgreSQL/MySQL behavior. The receipt is local `target/p11-native.log`. Original fixtures cover package spec/body/cursor/trigger, T-SQL batches and bracket names, ABAP statement scope and literals, Apex class/method/query/trigger syntax, strong-marker negative selection, malformed headers, CRLF/Unicode spans and shared budget rejection.

The subsequent required-row audit added a ninth vendor test directly asserting T-SQL table/view/query facts, Apex interface declarations and PL/SQL package-body syntax distinction. Linux integration then exposed two contract gaps: ABAP header-only owner spans did not contain child statements, and Apex declaration fact kinds did not enter the native continuity projection. The fixes extend matched ABAP owners through exact closing statements and regenerate syntax identities, and use the canonical `definition` fact kind for Apex declarations while retaining their specific syntax kinds. A tenth test exercises payload conversion/binding and Apex owner-linked continuity descriptors. All ten vendor tests subsequently passed in the native ingest run.

Five kernel integration cases cover retained original bytes, exact profile selection, bounded outlines, warm reuse, moves, Apex native-key lookup and profile invalidation; All five Linux cases passed after those corrections; final P12 platform/package qualification remains pending. The Windows unit receipt alone does not establish cross-platform or full vendor-language support.
