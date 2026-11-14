# SPDX-License-Identifier: Apache-2.0
"""Independent authored-format and complete shipping-manifest site boundaries."""
import copy
import hashlib
from html.parser import HTMLParser
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import threading
import shutil
import subprocess
import re
from urllib.request import urlopen
from urllib.parse import urlsplit, unquote

SPEC=importlib.util.spec_from_file_location('help_site',Path(__file__).parents[1]/'help-site.py')
HELP=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(HELP)

BODY='''# Start

Paragraph with **bold**, *emphasis*, `literal <code>` and <script>unsafe()</script>.

## Procedure

1. Read the review.
   - Keep source locations.
   - Check nullable fields.

2. Run `airkctl help`.

| Field | Meaning |
| --- | --- |
| `mode` | Optional enum: `lexical` or `vector` |

```sh
# This is code, not an anchor
printf '<script>literal</script>'
```

## Procedure

[Related](engine.setup#service) [Editor only](command:airk.openAdmin) [unsafe](javascript:bad)
[This section](#procedure-2) [Official](https://airk.ioka.io/index.html)
'''

def manifest():
    native={'version':'1.8.35','cliCommands':{'help':'Usage: airkctl help [COMMAND]'},
            'helpRequestSchema':{'type':'object','properties':{'mode':{'anyOf':[{'enum':['lexical','vector']},{'type':'null'}]}},'required':['mode'],'additionalProperties':False},
            'tools':[{'name':'help','inputSchema':{'type':'object','properties':{'topic':{'type':'string'}}}}],
            'futureNative':{'unknown':[None,False,5]}}
    return {'schema':'airk.help.manifest/1','catalogVersion':1,'productVersion':'1.8.35','extensionVersion':'1.8.35','channel':None,
            'topics':[{'id':'install','title':'Install <AIRK>','area':'setup','applicability':'implemented','aliases':['first use'],
                       'relatedAirkHelp':['setup'],'body':BODY},
                      {'id':'engine.setup','title':'Service setup','area':'engine','applicability':'implemented','aliases':['setup'],
                       'body':'# Service\n\nKeep source files read-only.'}],
            'native':native,**{key:copy.deepcopy(native[key]) for key in ('cliCommands','helpRequestSchema','tools')}}

class Tags(HTMLParser):
    def __init__(self):super().__init__();self.tags=[];self.links=[];self.ids=[]
    def handle_starttag(self,tag,attrs):
        values=dict(attrs);self.tags.append(tag)
        if 'href' in values:self.links.append(values['href'])
        if 'id' in values:self.ids.append(values['id'])

class HelpSiteTests(unittest.TestCase):
    def test_complete_manifest_bytes_alias_pages_schema_and_search(self):
        encoded=json.dumps(manifest(),ensure_ascii=False,indent=2).encode()
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)/'help';receipt=HELP.build(encoded,root,'1.8.35','stable')
            self.assertEqual((root/'manifest.json').read_bytes(),encoded)
            self.assertEqual((root/'index.html').read_bytes(),(root/'html.index/index.html').read_bytes())
            self.assertEqual(set(receipt['files']),{'index.html','html.index/index.html','reference.html','manifest.json','help.css','search.js','topics/install.html','topics/engine.setup.html'})
            self.assertEqual(receipt['manifest_sha256'],hashlib.sha256(encoded).hexdigest())
            html=(root/'index.html').read_text()
            self.assertIn('complete article content',html);self.assertIn('nullable fields',html)
            self.assertIn('aria-describedby="search-count"',html);self.assertIn('role="status"',html)
            reference=(root/'reference.html').read_text()
            for term in ('required','anyOf','null','lexical','vector','additionalProperties','Usage: airkctl help'):
                self.assertIn(term,reference)
            self.assertNotIn('any</',reference)
    def test_authored_markdown_nested_lists_tables_duplicate_headings_and_safe_links(self):
        data=manifest();renderer=HELP.Markdown(data['topics']);html=renderer.render(BODY,'install');tags=Tags();tags.feed(html)
        self.assertIn('<ol><li>',html);self.assertIn('<ul><li>',html)
        self.assertEqual(html.count('<ol>'),1)
        procedure='3. Continue after the review.\n\n4. Verify the result.\n## Next section\nRead the outcome.'
        custom=manifest();custom['topics'][0]['body']=procedure
        continuation=HELP.Markdown(custom['topics']).render(procedure,'install')
        self.assertIn('<ol start="3">',continuation)
        self.assertEqual(continuation.count('<ol'),1)
        self.assertIn('<h3 id="next-section">',continuation)
        self.assertIn('<table>',html);self.assertIn('<th scope="col">',html)
        self.assertIn('procedure-2',tags.ids);self.assertNotIn('this-is-code-not-an-anchor',tags.ids)
        self.assertIn('/help/topics/engine.setup.html#service',tags.links)
        self.assertIn('#procedure-2',tags.links)
        self.assertFalse(any(link.startswith(('command:','javascript:','file:','data:')) for link in tags.links))
        self.assertNotIn('script',tags.tags);self.assertIn('&lt;script&gt;unsafe()',html)
        self.assertIn('&lt;script&gt;literal&lt;/script&gt;',html)
    def test_requested_directory_route_and_alias_serve_identical_html_locally(self):
        class Handler(SimpleHTTPRequestHandler):
            def log_message(self, *_args):
                pass
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);HELP.build(json.dumps(manifest()).encode(),root/'help','1.8.35','stable')
            server=ThreadingHTTPServer(('127.0.0.1',0),partial(Handler,directory=root))
            thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            try:
                base='http://127.0.0.1:'+str(server.server_address[1])
                with urlopen(base+'/help/html.index',timeout=5) as requested:
                    self.assertEqual(requested.headers.get_content_type(),'text/html')
                    self.assertTrue(requested.url.endswith('/help/html.index/'))
                    content=requested.read()
                with urlopen(base+'/help/index.html',timeout=5) as alias:
                    self.assertEqual(alias.headers.get_content_type(),'text/html')
                    self.assertEqual(alias.read(),content)
            finally:
                server.shutdown();server.server_close();thread.join(timeout=5)
                self.assertFalse(thread.is_alive())

    @unittest.skipUnless((Path(__file__).resolve().parents[2]/'packages/ui/generated/help-manifest.json').is_file(), 'shared manifest not emitted')
    def test_actual_authored_corpus_has_complete_pages_and_valid_local_navigation(self):
        source=Path(__file__).resolve().parents[2]/'packages/ui/generated/help-manifest.json'
        encoded=source.read_bytes();data=json.loads(encoded)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)/'help';HELP.build(encoded,root,data['productVersion'],'stable')
            self.assertGreater(len(data['topics']),20)
            private = re.compile(r"(?:artifacts[\\/](?:advanced-split|editor-qualification|one-process[\\/]native-fixtures)|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|airk1\.[0-9a-f]{32}\.[0-9a-f]{64})",re.IGNORECASE)
            private_paths = {str(Path(__file__).resolve().parents[2]).replace('\\','/').lower(), str(Path.home()).replace('\\','/').lower()}
            # Report only a field location if this refuses; never print a secret.
            def public_values(value,path='manifest'):
                if isinstance(value,str):
                    normalized = value.replace('\\','/').lower()
                    self.assertFalse(private.search(value) or any(home in normalized for home in private_paths if len(home)>3), 'Private bytes in '+path)
                elif isinstance(value,dict):
                    for key,item in value.items():public_values(item,path+'.'+key)
                elif isinstance(value,list):
                    for index,item in enumerate(value):public_values(item,path+'['+str(index)+']')
            public_values(data)

            self.assertEqual((root/'manifest.json').read_bytes(),encoded)
            self.assertEqual({path.stem for path in (root/'topics').glob('*.html')},{topic['id'] for topic in data['topics']})
            pages={}
            for path in root.rglob('*.html'):
                parsed=Tags();parsed.feed(path.read_text(encoding='utf-8'));pages[path.relative_to(root).as_posix()]=parsed
                self.assertEqual(len(parsed.ids),len(set(parsed.ids)),str(path))
            for name,parsed in pages.items():
                for value in parsed.links:
                    link=urlsplit(value)
                    if link.scheme:
                        self.assertEqual(link.scheme,'https',value);continue
                    if value=='/index.html':continue
                    target=link.path.removeprefix('/help/') if link.path else name
                    self.assertTrue((root/target).is_file(),(name,value))
                    if link.fragment and target in pages:self.assertIn(unquote(link.fragment),pages[target].ids,(name,value))

    def test_generated_search_matches_full_content_and_moves_article_focus(self):
        root=Path(__file__).resolve().parents[2]
        pinned=root/'artifacts/tools/node-v24.15.0-win-x64/node.exe'
        node=str(pinned) if pinned.is_file() else shutil.which('node')
        if node is None:self.skipTest('Node unavailable')
        with tempfile.TemporaryDirectory() as temporary:
            source=Path(temporary)/'search.js';source.write_text(HELP.SEARCH,encoding='utf-8')
            result=subprocess.run([node,str(Path(__file__).with_name('help-site-browser.cjs')),str(source)],capture_output=True,text=True,timeout=15)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertIn('full-content search and article focus passed',result.stdout)

    def test_bad_manifest_identity_unknown_topic_or_duplicate_native_projection_refused_before_output(self):
        changes=[lambda m:m.update(productVersion='1.8.34'),lambda m:m.update(extensionVersion='1.8.34'),
                 lambda m:m.update(schema='future'),lambda m:m.update(channel='stable'),
                 lambda m:m['topics'].append(copy.deepcopy(m['topics'][0])),
                 lambda m:m['topics'][0].update(id='../private'),
                 lambda m:m['native'].update(version='1.8.34'),lambda m:m['tools'].append({'name':'invented'})]
        for change in changes:
            data=manifest();change(data)
            with tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary)/'help'
                with self.subTest(data=data),self.assertRaises(ValueError):HELP.build(json.dumps(data).encode(),root,'1.8.35','stable')
                self.assertFalse(root.exists())
    def test_unknown_internal_links_are_text_and_unclosed_fence_is_a_build_failure(self):
        renderer=HELP.Markdown(manifest()['topics'])
        html=renderer.inline('[missing](not-a-topic) [file](file:///private) [heading](#unknown)','install')
        self.assertNotIn('<a',html)
        with self.assertRaisesRegex(ValueError,'Unclosed'):renderer.render('```sh\nprivate','install')

class PublishedHelpTests(unittest.TestCase):
    def setUp(self):
        spec=importlib.util.spec_from_file_location('published_help',Path(__file__).parents[1]/'build-downloads.py')
        self.site=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.site)
    def archive(self, owner, entries):
        path=owner/'product.vsix'
        with zipfile.ZipFile(path,'w') as package:
            for entry,encoded in entries:package.writestr(entry,encoded)
        data=path.read_bytes()
        return path,{'size':len(data),'digest':'sha256:'+hashlib.sha256(data).hexdigest()}
    def test_manifest_is_exact_fixed_entry_in_digest_verified_published_vsix(self):
        encoded=json.dumps(manifest()).encode()
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            path,asset=self.archive(root,[('extension/out/help-manifest.json',encoded),('extension/extension.js',b'not executed')])
            self.assertEqual(self.site.help_from_vsix(path,asset,'1.8.35'),encoded)
            for field,value in [('digest','sha256:'+'0'*64),('size',asset['size']+1)]:
                with self.subTest(field=field),self.assertRaisesRegex(ValueError,'immutable'):
                    self.site.help_from_vsix(path,{**asset,field:value},'1.8.35')
            with self.assertRaisesRegex(ValueError,'identity'):
                self.site.help_from_vsix(path,asset,'1.8.36')
    def test_historical_missing_manifest_preserves_downloads_new_product_refuses_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            path,asset=self.archive(Path(temporary),[('extension/old.js',b'old')])
            self.assertIsNone(self.site.help_from_vsix(path,asset,'1.8.24'))
            with self.assertRaisesRegex(ValueError,'required'):
                self.site.help_from_vsix(path,asset,'1.8.35')
    def test_archive_does_not_follow_linked_manifest_or_duplicate_entry(self):
        encoded=json.dumps(manifest()).encode()
        with tempfile.TemporaryDirectory() as temporary:
            owner=Path(temporary);info=zipfile.ZipInfo('extension/out/help-manifest.json');info.external_attr=0o120777<<16
            path,asset=self.archive(owner,[(info,encoded)])
            with self.assertRaisesRegex(ValueError,'invalid'):
                self.site.help_from_vsix(path,asset,'1.8.35')
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter('ignore',UserWarning)
                path,asset=self.archive(owner,[(HELP.MANIFEST_ENTRY,encoded),(HELP.MANIFEST_ENTRY,encoded)])
            with self.assertRaisesRegex(ValueError,'ambiguous'):
                self.site.help_from_vsix(path,asset,'1.8.35')
    def test_build_uses_verified_published_stable_not_newer_draft_checkout(self):
        encoded=json.dumps(manifest()).encode();seen=[]
        records=[{'tag':'v1.8.35','version':'1.8.35','verified':True,'channel':'stable'},
                 {'tag':'v1.8.36-build.1','version':'1.8.36','verified':True,'channel':'preview'}]
        catalog=[{'tag_name':'v1.8.35','draft':False},{'tag_name':'v1.8.36-build.1','draft':False},
                 {'tag_name':'v1.8.37','draft':True}]
        def fetch(release,metadata):seen.append((release['tag_name'],metadata['version']));return encoded
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.site.build_help(records,catalog,root,fetch)
            self.assertEqual(seen,[('v1.8.35','1.8.35')])
            self.assertEqual((root/'help/manifest.json').read_bytes(),encoded)
            self.assertNotIn('1.8.36',(root/'help/index.html').read_text())
    def test_historical_stable_can_offer_explicit_published_preview_help(self):
        encoded=json.dumps(manifest()).encode();seen=[]
        records=[{'tag':'v1.8.24','version':'1.8.24','verified':True,'channel':'stable'},
                 {'tag':'v1.8.35-build.1','version':'1.8.35','verified':True,'channel':'preview'}]
        catalog=[{'tag_name':record['tag'],'draft':False} for record in records]
        def fetch(release,metadata):
            seen.append(metadata['version']);return encoded if metadata['version']=='1.8.35' else None
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.site.build_help(records,catalog,root,fetch)
            self.assertEqual(seen,['1.8.24','1.8.35'])
            self.assertIn('preview',(root/'help/index.html').read_text(encoding='utf-8'))
        with tempfile.TemporaryDirectory() as temporary,self.assertRaisesRegex(ValueError,'required Help'):
            self.site.build_help(records,catalog,Path(temporary),lambda *_:None)

if __name__=='__main__':unittest.main()
