# SPDX-License-Identifier: Apache-2.0
"""Hosted Help check uses bounded public observations; never executes product bytes."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from urllib.request import Request
import zipfile
from scripts.tests.test_help_site import manifest

ROOT=Path(__file__).resolve().parents[2]
SPEC=importlib.util.spec_from_file_location('published_help_check',ROOT/'scripts/check-published-help.py')
CHECK=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(CHECK)
CASES=json.loads((ROOT/'fixtures/split/receipts/publication_2.json').read_text(encoding='utf-8'))

class PublishedHelpCheck(unittest.TestCase):
    def fixture(self,root):
        release=root/'release';release.mkdir();record=copy.deepcopy(CASES['cases'][1]['input'])
        name='airk-1.8.35.vsix';archive=release/name;encoded=json.dumps(manifest()).encode()
        with zipfile.ZipFile(archive,'w') as product:product.writestr('extension/out/help-manifest.json',encoded)
        digest=CHECK.hashlib.sha256(archive.read_bytes()).hexdigest();record['assets'][name]=digest
        (release/'release.json').write_text(json.dumps(record),encoding='utf-8')
        output=root/'site';receipt=CHECK.site.help_site.build(encoded,output,'1.8.35','stable')
        receipt.update(release_tag='v1.8.35',published_vsix={'id':999,'name':name,'digest':'sha256:'+digest,'size':archive.stat().st_size})
        (output/'site-receipt.json').write_text(json.dumps(receipt),encoding='utf-8')
        def fetch(path):
            relative=path.removeprefix('/help/');relative='index.html' if relative=='html.index' else relative
            content=(output/relative).read_bytes()
            mime='text/html' if relative.endswith('.html') else 'application/json' if relative.endswith('.json') else 'text/css' if relative.endswith('.css') else 'text/javascript'
            return {'body':content,'mime':mime,'status':200,'url':'https://airk.ioka.io'+path+('/' if path=='/help/html.index' else '')}
        return release,fetch
    def test_checks_requested_url_alias_and_every_exact_generated_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            release,fetch=self.fixture(Path(temporary));report=CHECK.verify(release,fetch)
            self.assertEqual(report['complete_generated_file_count'],8)
            self.assertEqual(len(report['observations']),9)
            self.assertEqual(report['observations'][0]['path'],'/help/html.index')
            self.assertEqual(report['observations'][0]['content_type'],'text/html')
    def test_wrong_mime_stale_topic_alias_manifest_and_receipt_refuse(self):
        faults=[('/help/html.index','mime','application/octet-stream'),('/help/index.html','body',b'other'),
                ('/help/topics/install.html','body',b'old topic'),('/help/manifest.json','body',b'{}'),
                ('/help/site-receipt.json','body',b'{}'),('/help/site-receipt.json','url','https://elsewhere.invalid/receipt')]
        for path,field,value in faults:
            with self.subTest(path=path,field=field),tempfile.TemporaryDirectory() as temporary:
                release,fetch=self.fixture(Path(temporary))
                def changed(requested):
                    response=fetch(requested)
                    if requested==path:response[field]=value
                    return response
                with self.assertRaises(ValueError):CHECK.verify(release,changed)
    def test_redirect_handler_refuses_cross_origin_and_unrelated_destination(self):
        handler=CHECK.SameSiteRedirect('https://airk.ioka.io');request=Request('https://airk.ioka.io/help/html.index')
        for target in ('https://outside.invalid/help/html.index/','http://airk.ioka.io/help/html.index/','https://airk.ioka.io/unrelated/'):
            with self.subTest(target=target),self.assertRaises(ValueError):handler.redirect_request(request,None,301,'redirect',{},target)

if __name__=='__main__':unittest.main()
