# SPDX-License-Identifier: Apache-2.0
"""Read-only hosted Help verification against a downloaded published VSIX.

The release directory must first pass verify-published-release.py and the live
GitHub producer check. This check does not execute downloaded HTML or binaries.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from urllib.parse import urlsplit
from urllib.request import build_opener, HTTPRedirectHandler

ROOT=Path(__file__).resolve().parents[1]
SPEC=importlib.util.spec_from_file_location('published_help_downloads',ROOT/'scripts/build-downloads.py')
site=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(site)

class SameSiteRedirect(HTTPRedirectHandler):
    def __init__(self,base):
        self.origin=urlsplit(base)[:2];self.redirects=[]
    def redirect_request(self,request,fp,code,msg,headers,newurl):
        target=urlsplit(newurl)
        if target[:2]!=self.origin or target.path!=urlsplit(request.full_url).path.rstrip('/')+'/':
            raise ValueError('Hosted Help redirected outside its exact directory route')
        self.redirects.append({"status":code,"from":request.full_url,"to":newurl})
        return super().redirect_request(request,fp,code,msg,headers,newurl)


def fetcher(base):
    def fetch(path):
        handler=SameSiteRedirect(base);opener=build_opener(handler)
        with opener.open(base.rstrip('/')+path,timeout=30) as response:
            encoded=response.read((4<<20)+1)
            if len(encoded)>4<<20:raise ValueError('Hosted Help response exceeds bound: '+path)
            return {'body':encoded,'mime':response.headers.get_content_type(),'status':response.status,'url':response.url,'redirects':handler.redirects}
    return fetch


def verify(release_dir,fetch=None,base='https://airk.ioka.io'):
    record=json.loads((release_dir/'release.json').read_text(encoding='utf-8'))
    if site.optional.current(record["package_version"]):
        site.optional.validate_release(record, site.optional.projection(record["tag"]))
    else:
        site.publication.validate_release(record)
    version,channel=record['package_version'],record['channel']
    name=f'airk-{version}.vsix';archive=release_dir/name
    encoded=site.help_from_vsix(archive,{'size':archive.stat().st_size,'digest':'sha256:'+record['assets'][name]},version)
    if encoded is None:raise ValueError('Published product does not contain shared Help')
    fetch=fetch or fetcher(base)
    observations=[]
    def checked(path,expected,mime):
        response=fetch(path)
        target=urlsplit(response['url']);origin=urlsplit(base)
        if (target[:2]!=origin[:2] or target.path not in (path,path.rstrip('/')+'/')
                or response['status']!=200 or response['mime'] not in ((mime,) if isinstance(mime,str) else mime) or response['body']!=expected):
            raise ValueError('Hosted Help bytes, route or MIME differs: '+path)
        return {'path':path,'final_url':response['url'],'content_type':response['mime'],
                'sha256':hashlib.sha256(response['body']).hexdigest(),'bytes':len(response['body']),
                'redirects':response.get('redirects',[])}
    with tempfile.TemporaryDirectory(prefix='airk-help-check-') as temporary:
        output=Path(temporary)/'help';expected=site.help_site.build(encoded,output,version,channel)
        observations.append(checked('/help/html.index',(output/'index.html').read_bytes(),'text/html'))
        tasks=[]
        for relative in expected['files']:
            mime=('text/html' if relative.endswith('.html') else 'application/json' if relative.endswith('.json') else
                  'text/css' if relative.endswith('.css') else ('text/javascript','application/javascript'))
            tasks.append(('/help/'+relative,(output/relative).read_bytes(),mime))
        # Requests are independent public reads. No failed request is retried.
        pool=ThreadPoolExecutor(max_workers=4)
        futures={pool.submit(checked,*task):index for index,task in enumerate(tasks)}
        completed={}
        try:
            for future in as_completed(futures):
                completed[futures[future]]=future.result()
        except BaseException:
            for future in futures:future.cancel()
            pool.shutdown(wait=True,cancel_futures=True)
            raise
        else:
            pool.shutdown(wait=True)
        observations.extend(completed[index] for index in range(len(tasks)))
        receipt=fetch('/help/site-receipt.json')
        if (receipt['status']!=200 or receipt['mime']!='application/json'
                or urlsplit(receipt['url'])[:2]!=urlsplit(base)[:2]
                or urlsplit(receipt['url']).path!='/help/site-receipt.json'):
            raise ValueError('Hosted Help receipt unavailable')
        actual=json.loads(receipt['body'])
        if any(actual.get(key)!=value for key,value in expected.items()) or actual.get('release_tag')!=record['tag']:
            raise ValueError('Hosted Help receipt does not bind the downloaded product and generated files')
        asset=actual.get('published_vsix',{})
        if (type(asset.get('id')) is not int or asset['id']<=0 or asset.get('name')!=name
                or asset.get('digest')!='sha256:'+record['assets'][name] or asset.get('size')!=archive.stat().st_size):
            raise ValueError('Hosted Help receipt names another published VSIX')
    return {'schema':'airk.published-help-check/1','release_tag':record['tag'],'version':version,'channel':channel,
            'published_vsix_sha256':record['assets'][name],'manifest_sha256':hashlib.sha256(encoded).hexdigest(),
            'observations':observations,'complete_generated_file_count':len(expected['files']),
            'not_verified':['Browser rendering and interaction','Live GitHub producer origin; verify the release provenance separately']}


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release-dir',type=Path,required=True)
    parser.add_argument('--report',type=Path,required=True)
    args=parser.parse_args(argv)
    try:
        report=verify(args.release_dir.resolve(strict=True))
    except Exception as error:
        report={'schema':'airk.published-help-check/1','failed':[str(error)]}
        args.report.parent.mkdir(parents=True,exist_ok=True)
        args.report.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        raise
    args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(f"Verified hosted Help: {report['complete_generated_file_count']} files; {report['release_tag']}")

if __name__=='__main__':main()
