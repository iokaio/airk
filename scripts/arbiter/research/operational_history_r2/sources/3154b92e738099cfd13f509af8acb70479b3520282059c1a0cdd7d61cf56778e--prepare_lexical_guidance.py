# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

out = Path(__file__).resolve().parent
changes = {
    'crates/airk-cli/src/main.rs': [(
        '    /// upgrade, with that separate limitation disclosed. --mode lexical uses AND terms.',
        '    /// upgrade, with that separate limitation disclosed. --mode lexical uses normalized\n'
        '    /// AND terms: punctuation and quotation marks separate terms; quotes do not enable\n'
        '    /// phrase matching. Lexical order is stable index identity, not relevance.')],
    'packages/vscode/src/explorerWebview/search.tsx': [(
        "            {form.mode === 'vector' && <p class=\"muted\">Describe a concept or behavior in natural language.",
        "            {form.mode === 'lexical' && <p class=\"muted\">All normalized query terms must occur in a candidate chunk or its path. Punctuation and quotation marks separate terms; quotes do not enable phrase matching. Results follow stable index order, not relevance.</p>}\n"
        "            {form.mode === 'vector' && <p class=\"muted\">Describe a concept or behavior in natural language.")],
    'packages/vscode/src/explorerHelp/search.md': [(
        '**Lexical (all words)** requires all query words to appear.',
        '**Lexical (all words)** requires all normalized query terms to occur in a candidate chunk or its path. Punctuation and quotation marks separate terms; quotation marks do not enable phrase matching.'), (
        '- Lexical results are ordered by index identity, not relevance.',
        '- Lexical results are ordered by stable index identity, not relevance. A low limit can omit a literal-containing result even when other candidates contain the separate AND terms. Narrow the path scope or raise the limit. Snippets prefer the contiguous query spelling when it is already present in a selected chunk; this does not change matching or ranking.')],
    'docs/mcp.md': [(
        '`search.mode` accepts `lexical` (explicit AND-term retrieval),',
        '`search.mode` accepts `lexical` (explicit normalized AND-term retrieval; punctuation and quotes separate terms, so quotes do not enable phrase matching; lexical order follows stable index identity rather than relevance),')],
    'docs/mcp-developer-guide.md': [(
        'Focused snippets are the default; `full_chunks` changes returned text, not the indexed source.',
        'Focused snippets are the default; `full_chunks` changes returned text, not the indexed source. Lexical matching requires normalized AND terms in a candidate chunk or path; punctuation and quotes separate terms, and quotes do not enable phrase matching. Lexical candidate order is stable index identity, not relevance. A contiguous original query spelling can guide excerpt focus inside an already selected chunk without changing matching, candidate order or limits.')],
}
patch = ''
for name, replacements in changes.items():
    p = Path(name)
    before = p.read_text(encoding='utf-8')
    after = before
    for old, new in replacements:
        assert after.count(old) == 1, name
        after = after.replace(old, new)
    patch += ''.join(difflib.unified_diff(before.splitlines(keepends=True), after.splitlines(keepends=True), fromfile='a/' + name, tofile='b/' + name))
    q = out / 'proposed' / name
    q.parent.mkdir(parents=True, exist_ok=True)
    q.write_text(after, encoding='utf-8', newline='\n')
(out / 'lexical-guidance.patch').write_text(patch, encoding='utf-8', newline='\n')
print('Prepared lexical-guidance.patch; production sources unchanged')
