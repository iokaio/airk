# SPDX-License-Identifier: Apache-2.0
"""Render the complete published Help manifest as an offline static HTML site.

This consumer never reads Rust or authors an alternate manual. All topic bodies,
CLI documents and request/tool schemas are retained from the shipping manifest.
"""
from html import escape
import hashlib
import json
from pathlib import Path
import re
from urllib.parse import urlsplit

SCHEMA = 'airk.help.manifest/1'
MANIFEST_ENTRY = 'extension/out/help-manifest.json'
MAX_MANIFEST = 4 << 20
ID = re.compile(r'[a-z][a-z0-9.-]{0,127}')
VERSION = re.compile(r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)')


def manifest_bytes(encoded, version):
    if len(encoded) > MAX_MANIFEST:
        raise ValueError('Help manifest exceeds the shipping bound')
    value = json.loads(encoded)
    if (not isinstance(value, dict) or value.get('schema') != SCHEMA
            or not VERSION.fullmatch(version) or value.get('productVersion') != version
            or value.get('extensionVersion') != version or value.get('channel') is not None
            or not isinstance(value.get('topics'), list) or not value['topics']
            or not isinstance(value.get('cliCommands'), dict)
            or not isinstance(value.get('helpRequestSchema'), dict)
            or not isinstance(value.get('tools'), list) or not isinstance(value.get('native'), dict)
            or value['native'].get('version') != version
            or any(value.get(key) != value['native'].get(key) for key in ('cliCommands','helpRequestSchema','tools'))):
        raise ValueError('Help manifest differs from the published product identity')
    seen = set()
    for topic in value['topics']:
        if (not isinstance(topic, dict) or not ID.fullmatch(topic.get('id', ''))
                or topic['id'] in seen or any(not isinstance(topic.get(key), str) or not topic[key]
                                             for key in ('title', 'body', 'area', 'applicability'))
                or not isinstance(topic.get('aliases'), list)
                or not all(isinstance(alias, str) for alias in topic['aliases'])):
            raise ValueError('Help manifest has an invalid or duplicate topic')
        seen.add(topic['id'])
    return value


def topic_url(identifier):
    return '/help/topics/' + identifier + '.html'


def heading_id(text):
    return re.sub(r'[^a-z0-9-]+', '-', text.lower()).strip('-') or 'section'


class Markdown:
    """Safe authored subset: headings, paragraphs, code, tables and nested lists.

    Raw HTML is text. Internal product commands remain text on the website.
    Unknown relative links are text; known topic/heading links remain navigable.
    """
    def __init__(self, topics):
        self.topics = {topic['id']: topic for topic in topics}
        self.aliases = {topic['id']: topic['id'] for topic in topics}
        for topic in topics:
            for alias in topic['aliases']:
                # An ambiguous alias is never used as navigation authority.
                if alias in self.aliases and self.aliases[alias] != topic['id']:
                    self.aliases[alias] = None
                else:
                    self.aliases[alias] = topic['id']
        self.anchors = {}
        for topic in topics:
            used = {}; anchors = []; fence = None
            for line in topic['body'].splitlines():
                marker = re.match(r'^\s*(`{3,}|~{3,})', line)
                if marker:
                    if fence is None:
                        fence = marker[1]
                    elif marker[1][0] == fence[0] and len(marker[1]) >= len(fence):
                        fence = None
                    continue
                if fence is not None:
                    continue
                match = re.match(r'^#{1,6}\s+(.+?)\s*#*$', line)
                if match:
                    base = heading_id(match[1]); used[base] = used.get(base, 0) + 1
                    anchors.append(base + ('-' + str(used[base]) if used[base] > 1 else ''))
            self.anchors[topic['id']] = anchors

    def target(self, target, current):
        if target.startswith(('command:', 'vscode:', 'file:', 'javascript:', 'data:', '//')):
            return None
        parsed = urlsplit(target)
        if parsed.scheme == 'https' and parsed.netloc and not parsed.username and not parsed.password:
            return target
        if target.startswith('#'):
            fragment = target[1:]
            return '#' + fragment if fragment in self.anchors[current] else None
        candidate, _, fragment = target.partition('#')
        candidate = re.sub(r'^(?:help:|airk-help:)', '', candidate)
        candidate = candidate.removesuffix('.md')
        identifier = self.aliases.get(candidate)
        if identifier and (not fragment or fragment in self.anchors[identifier]):
            return topic_url(identifier) + ('#' + fragment if fragment else '')
        return None

    def inline(self, text, current):
        # Tokens are consumed from raw text once, so authored markup cannot
        # reinterpret escaped HTML, generated anchors, or literal code contents.
        pattern = re.compile(r'(`+)(.+?)\1|\[([^\]\n]+)\]\(([^\s)]+)\)|\*\*(.+?)\*\*|(?<!\*)\*([^*\n]+)\*')
        out = []; offset = 0
        for match in pattern.finditer(text):
            out.append(escape(text[offset:match.start()]))
            if match[1]:
                out.append('<code>' + escape(match[2]) + '</code>')
            elif match[3]:
                label = self.inline(match[3], current); target = self.target(match[4], current)
                out.append('<a href="' + escape(target, quote=True) + '">' + label + '</a>' if target else label)
            elif match[5]:
                out.append('<strong>' + self.inline(match[5], current) + '</strong>')
            else:
                out.append('<em>' + escape(match[6]) + '</em>')
            offset = match.end()
        out.append(escape(text[offset:]))
        return ''.join(out)

    def render(self, body, current):
        lines = body.replace('\r\n', '\n').splitlines(); out = []; index = 0; anchor = iter(self.anchors[current])
        while index < len(lines):
            line = lines[index]
            if not line.strip():
                index += 1; continue
            fence = re.match(r'^\s*(`{3,}|~{3,})([^`]*)$', line)
            if fence:
                content = []; index += 1
                while index < len(lines) and not re.fullmatch(r'\s*' + re.escape(fence[1][0]) + '{' + str(len(fence[1])) + r',}\s*', lines[index]):
                    content.append(lines[index]); index += 1
                if index == len(lines):
                    raise ValueError('Unclosed authored Help code fence: ' + current)
                out.append('<pre tabindex="0"><code>' + escape('\n'.join(content)) + '</code></pre>'); index += 1; continue
            heading = re.match(r'^(#{1,6})\s+(.+?)\s*#*$', line)
            if heading:
                level = min(6, len(heading[1]) + 1)
                out.append(f'<h{level} id="{next(anchor)}">' + self.inline(heading[2], current) + f'</h{level}>'); index += 1; continue
            if index + 1 < len(lines) and '|' in line and re.fullmatch(r'\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*', lines[index + 1]):
                def cells(row):
                    return re.split(r'(?<!\\)\|', row.strip().strip('|'))
                headers = cells(line); rows = []; index += 2
                while index < len(lines) and '|' in lines[index] and lines[index].strip():
                    row = cells(lines[index])
                    if len(row) != len(headers):
                        raise ValueError('Authored Help table column mismatch: ' + current)
                    rows.append(row); index += 1
                out.append('<div class="table-scroll" tabindex="0"><table><thead><tr>' + ''.join('<th scope="col">' + self.inline(cell.strip(), current) + '</th>' for cell in headers) + '</tr></thead><tbody>' + ''.join('<tr>' + ''.join('<td>' + self.inline(cell.strip(), current) + '</td>' for cell in row) + '</tr>' for row in rows) + '</tbody></table></div>'); continue
            if re.match(r'^\s*(?:[-+*]|\d+[.)])\s+', line):
                block = []
                while index < len(lines):
                    candidate = lines[index]
                    if not candidate.strip():
                        following = index + 1
                        while following < len(lines) and not lines[following].strip():
                            following += 1
                        if following < len(lines) and re.match(r'^(?:\s*(?:[-+*]|\d+[.)])\s+|\s{2,}\S)', lines[following]):
                            block.append(''); index = following; continue
                        break
                    if block and re.match(r'^(?:#{1,6}\s|`{3,}|~{3,}|>)', candidate):
                        break
                    block.append(candidate); index += 1
                out.append(self.list_block(block, current)); continue
            if line.startswith('>'):
                block = []
                while index < len(lines) and lines[index].startswith('>'):
                    block.append(re.sub(r'^> ?', '', lines[index])); index += 1
                out.append('<blockquote>' + self.inline(' '.join(block), current) + '</blockquote>'); continue
            paragraph = [line]; index += 1
            while index < len(lines) and lines[index].strip() and not re.match(r'^\s*(?:#{1,6}\s|`{3,}|~{3,}|[-+*]\s|\d+[.)]\s|>)', lines[index]):
                paragraph.append(lines[index]); index += 1
            out.append('<p>' + self.inline(' '.join(paragraph), current) + '</p>')
        return '\n'.join(out)

    def list_block(self, lines, current):
        items = []
        for line in lines:
            match = re.match(r'^(\s*)([-+*]|\d+[.)])\s+(.*)$', line)
            if match:
                ordered = match[2][0].isdigit()
                items.append((len(match[1].expandtabs(4)), ordered, match[3], int(match[2][:-1]) if ordered else None))
            elif items:
                indent, ordered, text, start = items[-1]; items[-1] = (indent, ordered, text + ' ' + line.strip(), start)
        def group(offset, indent):
            ordered = items[offset][1]; tag = 'ol' if ordered else 'ul'
            start = ' start="' + str(items[offset][3]) + '"' if ordered and items[offset][3] != 1 else ''
            result = ['<' + tag + start + '>']
            while offset < len(items) and items[offset][0] == indent:
                if items[offset][1] != ordered:
                    break
                result.append('<li>' + self.inline(items[offset][2], current)); offset += 1
                while offset < len(items) and items[offset][0] > indent:
                    nested, offset = group(offset, items[offset][0]); result.append(nested)
                result.append('</li>')
            result.append('</' + tag + '>'); return ''.join(result), offset
        result = []; offset = 0
        while offset < len(items):
            chunk, offset = group(offset, items[offset][0]); result.append(chunk)
        return ''.join(result)


def page(title, body, version, channel):
    return f'''<!doctype html>
<!-- SPDX-License-Identifier: Apache-2.0 -->
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'self'; object-src 'none'; base-uri 'none'">
<title>{escape(title)} · AIRK Help</title><link rel="stylesheet" href="/help/help.css"><script src="/help/search.js" defer></script></head>
<body><a class="skip" href="#main">Skip to content</a><header><a href="/index.html">AIRK downloads</a> · <a href="/help/index.html">Help</a>
<p>Product {escape(version)} · {escape(channel)} · published product content</p></header><p id="help-announcement" class="sr-only" role="status"></p><main id="main">{body}</main></body></html>'''


STYLE = '''/* SPDX-License-Identifier: Apache-2.0 */
:root{color-scheme:light dark;font:17px/1.6 system-ui,sans-serif;background:#f8fafb;color:#172f3c}body{margin:auto;max-width:75rem;padding:1.2rem}header,main{max-width:70ch;margin:auto}a{color:#08648a}a:focus-visible,input:focus-visible,summary:focus-visible,pre:focus-visible,.table-scroll:focus-visible{outline:3px solid #d17e00;outline-offset:3px}header{border-bottom:1px solid #75939e;padding-bottom:1rem}h1,h2,h3{line-height:1.25;scroll-margin-top:1rem}input{box-sizing:border-box;width:100%;padding:.7rem;font:inherit}pre,.table-scroll{overflow:auto;max-width:100%}pre{padding:1rem;background:#e8eef2}code{font-size:.9em;overflow-wrap:anywhere}table{border-collapse:collapse;width:100%}th,td{border:1px solid #75939e;padding:.5rem;text-align:left;vertical-align:top}li{margin:.3rem 0}.topics{padding-left:1.2rem}.topics small{display:block}.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap}.skip:not(:focus){position:absolute;left:-10000px}.metadata{font-size:.9rem}details{border:1px solid #75939e;padding:.6rem;margin:1rem 0}summary{cursor:pointer}@media(prefers-color-scheme:dark){:root{background:#111e27;color:#e5edf2}a{color:#7ad8f1}pre{background:#1e303b}}@media(max-width:40rem){body{padding:.8rem}h1{font-size:1.8rem}}
'''
SEARCH = '''// SPDX-License-Identifier: Apache-2.0
'use strict';
const article=document.querySelector('article h1');
if(article){article.tabIndex=-1;article.focus({preventScroll:true});document.querySelector('#help-announcement').textContent=`Opened ${article.textContent}`;}
const input=document.querySelector('#help-search');
if(input){const rows=[...document.querySelectorAll('[data-search]')];const count=document.querySelector('#search-count');input.addEventListener('input',()=>{const terms=input.value.toLocaleLowerCase().trim().split(/\\s+/).filter(Boolean);let found=0;for(const row of rows){row.hidden=!terms.every(term=>row.dataset.search.includes(term));if(!row.hidden)found++;}count.textContent=`${found} of ${rows.length} topics`;});}
'''


def build(encoded, output, version, channel):
    if channel not in ('stable', 'preview'):
        raise ValueError('Invalid published Help channel')
    manifest = manifest_bytes(encoded, version); renderer = Markdown(manifest['topics'])
    output.mkdir(parents=True, exist_ok=False); (output/'topics').mkdir(); (output/'html.index').mkdir()
    entries = []
    for topic in manifest['topics']:
        identifier = topic['id']; searchable = ' '.join([topic['title'],topic['area'],topic['applicability'],*topic['aliases'],topic['body']]).lower()
        excerpt = re.sub(r'\s+', ' ', topic['body']).strip()[:240]
        entries.append('<li data-search="' + escape(searchable, quote=True) + '"><a href="' + topic_url(identifier) + '">' + escape(topic['title']) + '</a><small>' + escape(topic['area'] + ' · ' + topic['applicability']) + '</small><p class="excerpt">' + escape(excerpt) + '</p></li>')
        article = '<article><h1>' + escape(topic['title']) + '</h1><p class="metadata">' + escape(topic['area'] + ' · ' + topic['applicability']) + '</p>' + renderer.render(topic['body'], identifier) + '</article>'
        related = []
        for name in topic.get('relatedAirkHelp', []):
            destination = renderer.aliases.get(name)
            if destination and destination != identifier:
                related.append('<a href="' + topic_url(destination) + '">' + escape(renderer.topics[destination]['title']) + '</a>')
        if related:
            article += '<nav aria-label="Related Help"><h2>Related Help</h2><ul>' + ''.join('<li>' + item + '</li>' for item in related) + '</ul></nav>'
        (output/'topics'/f'{identifier}.html').write_text(page(topic['title'],article,version,channel),encoding='utf-8')
    landing = '<h1>AIRK Help</h1><p>Guides for the service, CLI, agents and VS Code. Search matches complete article content.</p><label for="help-search">Search Help</label><input type="search" id="help-search" aria-describedby="search-count"><p id="search-count" role="status">' + str(len(entries)) + ' topics</p><ul class="topics">' + ''.join(entries) + '</ul><p><a href="/help/reference.html">Complete CLI and tool/schema reference</a> · <a href="/help/manifest.json">Shipping Help manifest</a></p>'
    encoded_html = page('Help',landing,version,channel).encode('utf-8')
    (output/'index.html').write_bytes(encoded_html); (output/'html.index'/'index.html').write_bytes(encoded_html)
    reference = '<h1>CLI and tool reference</h1><p>Complete schemas retain required fields, enums, nullable variants and limits. A missing optional field does not imply arbitrary values are accepted.</p>'
    reference += '<h2>CLI commands</h2><p>Exact help from the shipped parser; commands do not require VS Code.</p>'
    for command, value in sorted(manifest['cliCommands'].items()):
        text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, indent=2)
        reference += '<details><summary><code>' + escape(command or 'airkctl') + '</code></summary><pre tabindex="0"><code>' + escape(text) + '</code></pre></details>'
    reference += '<h2>Help request schema</h2><pre tabindex="0"><code>' + escape(json.dumps(manifest['helpRequestSchema'], ensure_ascii=False, indent=2)) + '</code></pre>'
    reference += '<h2>Complete tool catalog</h2><p>Each entry includes every native catalog field, input and output schema.</p>'
    for index, tool in enumerate(manifest['tools']):
        title = tool.get('name', 'Tool ' + str(index + 1)) if isinstance(tool, dict) else 'Tool ' + str(index + 1)
        reference += '<details id="tool-' + str(index + 1) + '"><summary>' + escape(str(title)) + '</summary><pre tabindex="0"><code>' + escape(json.dumps(tool, ensure_ascii=False, indent=2)) + '</code></pre></details>'
    (output/'reference.html').write_text(page('Reference',reference,version,channel),encoding='utf-8')
    (output/'manifest.json').write_bytes(encoded)
    (output/'help.css').write_text(STYLE,encoding='utf-8'); (output/'search.js').write_text(SEARCH,encoding='utf-8')
    return {'schema':'airk.help-site/1','product_version':version,'channel':channel,
            'manifest_sha256':hashlib.sha256(encoded).hexdigest(),
            'files':{path.relative_to(output).as_posix():hashlib.sha256(path.read_bytes()).hexdigest()
                     for path in sorted(output.rglob('*')) if path.is_file()}}
