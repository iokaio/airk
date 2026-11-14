# Native Help and shared content

Owner: `airk-mcp::help`. Help is read-only documentation. It neither opens a
collection nor executes examples. The live native owner validates selectors,
line continuations and the complete serialized response budget. A continuation
retains the selector and version; it does not silently concatenate a different
topic. Tool descriptions and input schemas come from the actual native catalog;
command descriptions come from the CLI parser's generated catalog.

## Shared content contract

The approved generation path is the native owner followed by the existing
validated Explorer Help catalog builder. `content.json` has schema
`airk.help.native-content/1` and these required fields:

- `version`: the native package version.
- `topics`: native source order, complete `{id,title,text}` objects.
- `cliCommands`: the complete generated CLI command-path/help map.
- `helpRequestSchema`: the complete native `HelpArgs` JSON Schema.
- `tools`: the complete actual unified tool catalog, including input schemas,
  descriptions, annotations and supported extra fields.

The Rust emitter compares generated values with retained JSON. It emits no
filesystem paths, credentials, observed live state, clock or inferred release
qualification. Source-relative provenance paths in the downstream manifest are
public authored-content identities. Updating Help content is a content change;
changing this generated test/build contract requires an explicit reviewed
decision before consumers.

`packages/vscode/scripts/build-explorer.cjs` consumes those native values and
the authored `packages/vscode/src/explorerHelp/*.md` front matter and bodies.
It retains existing topic IDs, aliases, route, area, applicability, related Help
IDs, source and complete body. Its single output is
`packages/ui/generated/help-manifest.json`, schema `airk.help.manifest/1`.
Native and extension versions derive from existing version owners and must
agree. Content is channel-neutral (`channel: null`); shipped artifact identity
supplies its actual channel. UI bundles and the static website consume the
same manifest; they do not maintain another manual or parse Rust literals.

The manifest, native input and authored/generator inputs participate in the
applicable runtime, extension and container source identities. Static rendering
must retain full topic content and use safe HTML handling; a successful content
generation is not a hosted-site, listener or installed-product qualification.

## Preservation and validation

Preserve stable topic IDs, complete content, source isolation, generated command
catalog bytes, full tool schemas and the live Help line/budget/error behavior.
Date historical descriptions and keep them separate from current workflows.
Document actual service/CLI-only usage so the corpus works without VS Code.
No stale setup, launcher, process or agent-scope instructions may be presented as
current behavior. Optional Arbiter guidance must distinguish implemented controls
from available models and qualified capabilities.

The native generation test, existing CLI catalog drift test, full topic and
task-findability checks, continuation/error/schema cases and affected UI/static
generation checks qualify their own boundaries. The Usage/Help transport
[semantic cases](../../../../fixtures/split/ui_pilot/README.md) retain complete
native and formatted outcomes separately. Do not weaken or replace those
assertions when updating prose.

Generate native content with `AIRK_WRITE_HELP_CONTENT=1` and
`cargo test -p airk-mcp --lib help_content_manifest_matches_native_owners --locked --timings`.
Run that test without the variable for drift, and include
`current_help_tasks_have_complete_native_guides_and_continuations` plus the
existing Help selector/continuation/schema regressions. Regenerate the parser
catalog first when public command paths change. After Help text changes,
regenerate its native pilot payload and the I2/I3 projections before frontend
qualification; unchanged Usage input keeps its original identity.
