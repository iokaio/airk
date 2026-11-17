# SPDX-License-Identifier: Apache-2.0
"""Prepare reviewed native patches without changing frozen production inputs."""
from pathlib import Path
import difflib

ROOT = Path(__file__).resolve().parents[2]
changes = {}


def change(path, edit):
    source = (ROOT / path).read_text(encoding="utf-8")
    updated = edit(source)
    if updated == source:
        raise ValueError(f"No change prepared for {path}")
    changes[path] = (source, updated)


def schemas(source):
    # Evidence reads share Core's existing 1..100 / 1..65536 budget validation.
    source = source.replace('    pub limit: usize,', '    #[schemars(range(min = 1, max = 100))]\n    pub limit: usize,')
    source = source.replace('    pub max_bytes: usize,', '    #[schemars(range(min = 1, max = 65536))]\n    pub max_bytes: usize,')
    for field in ['before', 'after']:
        source = source.replace(f'    pub {field}: usize,', f'    #[schemars(schema_with = "context_radius_schema")]\n    pub {field}: usize,')
    source = source.replace('    pub direction: String,', '    #[schemars(schema_with = "relationship_direction_schema")]\n    pub direction: String,')
    source = source.replace('    pub hops: usize,', '    #[schemars(range(min = 1, max = 3))]\n    pub hops: usize,')
    source = source.replace('fn default_radius() -> usize {', '''fn context_radius_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type":"integer","minimum":0,"maximum":airk_core::native_reads::CONTEXT_RADIUS_MAX
    })).expect("static native context radius schema")
}
fn relationship_direction_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({"type":"string","enum":["incoming","outgoing"]}))
        .expect("static relationship direction schema")
}
fn default_radius() -> usize {''')
    source = source.replace('fn default_radius() -> usize {\n    5\n}', 'fn default_radius() -> usize {\n    airk_core::native_reads::CONTEXT_RADIUS_DEFAULT\n}')
    source = source.replace('/// Default hybrid retrieval; explicit lexical uses AND terms.',
        '/// Default hybrid retrieval; lexical normalizes punctuation/quotes into AND terms, not phrase matching.')
    source = source.replace('explicit lexical uses AND terms. Eligible Default search',
        'explicit lexical normalizes punctuation and quotes into AND terms; quotes do not enable phrase matching. Lexical candidates use stable-id order, not relevance ranking. Eligible Default search')
    source = source.replace('Reports available capabilities, not whether optional providers or this collection are ready. read_guidance',
        'Reports available capabilities, not whether optional providers or this collection are ready. Terse replies summarize Workbench operation schemas; response_mode=full includes them. read_guidance')
    for structure in ['FactArgs', 'CodeFactArgs', 'DocumentFactArgs']:
        start = source.index(f'pub struct {structure} {{')
        end = source.index('\n}', start)
        block = source[start:end]
        block = block.replace('    pub language: Option<String>,',
            '    /// Exact indexed native or retained producer language/profile label; 1..128 UTF-8 bytes. JSON maxLength is a character bound; native validation enforces the byte bound.\n'
            '    #[schemars(length(min = 1, max = 128))]\n    pub language: Option<String>,')
        block = block.replace('/// Exact category:', '/// Native category examples:')
        block = block.replace('    pub kind: Option<String>,',
            '    /// Exact native or retained producer kind label; 1..128 UTF-8 bytes. Labels are open, not an enum; native validation enforces the byte bound.\n'
            '    #[schemars(length(min = 1, max = 128))]\n    pub kind: Option<String>,')
        source = source[:start] + block + source[end:]
    return source


change('crates/airk-mcp/src/lib.rs', schemas)
change('crates/airk-core/src/native_reads.rs', lambda source: source.replace(
    '//! Canonical native read requests shared by native transports and MCP adapters.\n',
    '//! Canonical native read requests shared by native transports and MCP adapters.\n'
    '/// Maximum neighboring indexed lines accepted by every native context read.\n'
    'pub const CONTEXT_RADIUS_MAX: usize = 100;\n'
    '/// Neighboring indexed lines used when a context radius is omitted.\n'
    'pub const CONTEXT_RADIUS_DEFAULT: usize = 5;\n'))
for path in ['crates/airk-kernel/src/lib.rs', 'crates/airk-kernel/src/conditional.rs', 'crates/airk-kernel/src/current/conditional.rs']:
    change(path, lambda source: source.replace(
        'before <= 100 && after <= 100',
        'before <= airk_core::native_reads::CONTEXT_RADIUS_MAX\n'
        '                && after <= airk_core::native_reads::CONTEXT_RADIUS_MAX'))
def workbench(source):
    source = source.replace('"before" | "after" => 40,', '"before" | "after" => airk_core::native_reads::CONTEXT_RADIUS_MAX as u64,')
    source = source.replace('number(args, "before", 3).min(40)', 'number(args, "before", airk_core::native_reads::CONTEXT_RADIUS_DEFAULT)')
    source = source.replace('number(args, "after", 3).min(40)', 'number(args, "after", airk_core::native_reads::CONTEXT_RADIUS_DEFAULT)')
    source = source.replace('            let schema = argument_schema(key);', '''            let schema = if matches!(*tool,"symbols"|"imports"|"code_facts"|"document_facts") && matches!(key,"kind"|"language") {
                json!({"type":"string","minLength":1,"maxLength":128,"description":"Exact native or retained producer label; 1..128 UTF-8 bytes. JSON maxLength is a character bound; native validation enforces the byte bound. Labels are open, not an enum."})
            } else { argument_schema(key) };''')
    source = source.replace('''        "limit" | "max_bytes" | "start_line" | "end_line" | "line" | "before" | "after"
        | "hops" => {
            json!({"type":"integer","minimum":if matches!(key,"before"|"after"){0}else{1},"maximum":numeric_max(key)})
        }''', '''        "before" | "after" => json!({
            "type":"integer","minimum":0,"maximum":airk_core::native_reads::CONTEXT_RADIUS_MAX,
            "default":airk_core::native_reads::CONTEXT_RADIUS_DEFAULT
        }),
        "limit" | "max_bytes" | "start_line" | "end_line" | "line" | "hops" => {
            json!({"type":"integer","minimum":1,"maximum":numeric_max(key)})
        }''')
    return source


change('crates/airk-kernel/src/workbench.rs', workbench)
def cli_defaults(source):
    for field in ['before', 'after']:
        source = source.replace(f'#[arg(long, default_value_t = 5)]\n        {field}: usize,',
            f'#[arg(long, default_value_t = airk_core::native_reads::CONTEXT_RADIUS_DEFAULT)]\n        {field}: usize,')
    return source


change('crates/airk-cli/src/main.rs', cli_defaults)
change('crates/airk-mcp/src/help/topics.rs', lambda source: source.replace(
    'Lexical search uses AND terms and indexed text.',
    'Lexical search normalizes punctuation and quotes into AND terms over indexed text; quotes do not enable phrase matching. Lexical candidates use stable-id order, not relevance ranking.'))
change('crates/airk-mcp/src/optimization_tests.rs', lambda source: source + '\n' + Path(__file__).with_name('mcp_schema_tests.rs').read_text(encoding='utf-8'))

patch = ''.join('diff --git a/{0} b/{0}\n'.format(path) + ''.join(difflib.unified_diff(
    before.splitlines(keepends=True), after.splitlines(keepends=True), fromfile=f'a/{path}', tofile=f'b/{path}'))
    for path, (before, after) in changes.items())
destination = Path(__file__).with_name('mcp-schemas-context-search.patch')
destination.write_text(patch, encoding='utf-8')
print(destination)
