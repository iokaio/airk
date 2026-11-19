# SPDX-License-Identifier: Apache-2.0
"""Emit exhaustive rescue routing, not migration proof; stdout only."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import subprocess
from pathlib import Path
from audit_memo import cached


def lane(row):
    path = row['path']
    if row['disposition'] == 'documented_corpus_input':
        return 'corpus_evidence'
    if row['classification'] != 'remaining_implementation_candidate':
        return 'existing_identity_evidence'
    if any(s in path for s in ['/backend-research/', '/reference-code/', '/public-human-label-admission-r1/', '/selected-model-rights/', '/research-attribution']):
        return 'artifact_code_audit'
    if row['disposition'] == 'repository_snapshot_or_proposal_review':
        return 'component_review'
    if Path(path).suffix.lower() in {'.rs','.c','.cc','.cpp','.h','.hpp','.ts','.tsx','.js','.mjs','.cjs','.cs','.def'}:
        return 'fp16_readiness'
    if '/models/' in path and any(s in path.lower() for s in ['gqa', 'attention', 'training', 'lora', 'minilm', 'qwen', 'encoder', 'parent-bag', 'calibration', 'export', 'mixed-r']):
        return 'focus_review'
    if '/v190/final-plan/' in path:
        return 'component_review'
    return 'graph_emitter'


def disposition(inventory, source_sha256):
    lanes = defaultdict(list)
    for row in inventory['candidates']:
        assigned = lane(row)
        lanes[assigned].append({'path':row['path'], 'source_sha256':row['source_sha256'],
                               'bytes':row['bytes'], 'classification':row['classification'],
                               'disposition':row['disposition'], 'reason':row['disposition_reason'],
                               'status':'evidence_present_requires_commit_review' if assigned in ['existing_identity_evidence','corpus_evidence'] else 'assigned_pending_verified_disposition'})
    return {'schema':'airk.private.source-rescue-routing/1','input_sha256':source_sha256,
            'scope':inventory['roots'], 'input_complete':inventory['complete_within_declared_scope'],
            'migration_complete':False, 'routing_is_not_migration_proof':True,
            'counts':{name:{'paths':len(rows),'unique_sha256':len({r['source_sha256'] for r in rows})} for name,rows in sorted(lanes.items())},
            'lanes':dict(sorted(lanes.items()))}


def verify_javascript_fixture_literal(rows,fixture,source):
    wanted=fixture['literal'];matches=[]
    if wanted.get('kind') not in ['service_static_template','literal_output_property']:
        raise ValueError('unsupported fixture AST kind')
    for key in ['start_utf16','end_utf16']:
        if type(wanted.get(key)) is not int or wanted[key]<0:raise ValueError('invalid fixture AST span')
    if wanted['end_utf16']<=wanted['start_utf16']:raise ValueError('empty fixture AST span')
    for row in rows:
        if row.get('kind')==wanted['kind'] and row.get('start')==wanted['start_utf16'] and row.get('end')==wanted['end_utf16'] and row.get('output')==wanted.get('output'):
            matches.append(row)
    if len(matches)!=1 or not isinstance(matches[0].get('value'),str):raise ValueError('exact fixture AST literal not uniquely observed')
    text=matches[0]['value'];tag=fixture.get('tag')
    if wanted['kind']=='service_static_template':
        if fixture['source']['path']!='scripts/check-extension-update-editor.cjs' or tag not in ['initial','updated','rebuilt','closed']:
            raise ValueError('unsupported fixture template owner or TAG')
        text='const TAG='+json.dumps(tag,separators=(',',':'))+';'+text
    elif fixture['source']['path']!='packages/vscode/src/test/extensionUpdateLoader.test.ts' or tag is not None or not isinstance(wanted.get('output'),str) or not wanted['output'].startswith('out/'):
        raise ValueError('unsupported literal output owner/path')
    body=text.encode('utf-8')
    if hashlib.sha256(body).hexdigest()!=source['sha256'] or len(body)!=source['bytes']:
        raise ValueError('complete fixture output differs from actual source')
    return {'exact_output_reproduced':True,'generator_executed':False}


def verify_recovery_payload(archive_raw,record_raw,base,source,select,recover):
    archive=base['archive']
    if hashlib.sha256(archive_raw).hexdigest()!=archive['sha256'] or len(archive_raw)!=archive['bytes']:
        raise ValueError('recovery archive changed during read')
    if len(record_raw)>16<<20:raise ValueError('recovery record exceeds16MiB')
    member=base['member']
    if not isinstance(member,str) or ':' in member or '\\' in member or any(p in ('','.','..') for p in member.split('/')):
        raise ValueError('unsafe recovery archive member')
    import zipfile,tarfile
    gzip=archive_raw.startswith(b'\x1f\x8b')
    if not gzip and not archive_raw.startswith(b'PK\x03\x04'):raise ValueError('unsupported recovery archive encoding')
    try:body=select(archive_raw,{member},tar=gzip)[member]
    except (zipfile.BadZipFile,tarfile.TarError,KeyError) as error:raise ValueError('recovery archive/member could not be read') from error
    if type(base.get('bytes')) is not int or len(body)>16<<20 or len(body)!=base['bytes'] or hashlib.sha256(body).hexdigest()!=base['sha256']:
        raise ValueError('recovery base member identity differs')
    output=recover(body,json.loads(record_raw))
    if len(output)!=source['bytes'] or hashlib.sha256(output).hexdigest()!=source['sha256']:
        raise ValueError('recovered output differs from actual observed source')
    return {'exact_output_reproduced':True,'historical_application_verified':False,'airk_authorship':False}


def verify_disposition_row(row, repo, observe, checked_path, observe_evidence=None, git_cache=None):
    """Verify metadata pins only; never infer model or execution qualification."""
    verified_observations={}
    def pin(path, digest, size=None, prefix='', evidence=False):
        if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('invalid disposition digest')
        actual=(observe_evidence if evidence and observe_evidence is not None else observe)(checked_path(repo,path,prefix),'sha1')
        if actual['source_sha256']!=digest:
            raise ValueError('disposition source/evidence bytes changed')
        if size is not None and (type(size) is not int or size!=actual['bytes']):
            raise ValueError('disposition byte count differs')
        return {'path':path,'sha256':digest,'bytes':actual['bytes']}
    def shared_source_pin(item,prefix):
        if type(item.get('bytes')) is not int or not 0<=item['bytes']<=16<<20:
            raise ValueError('shared source proof exceeds16MiB or lacks exact size')
        return pin(item['path'],item['sha256'],item['bytes'],prefix,evidence=True)
    if type(row.get('source_bytes')) is not int or row['source_bytes']<0:
        raise ValueError('explicit source byte count required')
    source=pin(row['source'],row['source_sha256'],row['source_bytes'],'artifacts/')
    if not isinstance(row.get('reason'),str) or not row['reason'].strip():
        raise ValueError('disposition reason required')
    evidence=row.get('evidence')
    if not isinstance(evidence,list) or not evidence:
        raise ValueError('explicit disposition evidence required')
    proofs=[pin(e['path'],e['sha256'],e.get('bytes'),evidence=True) for e in evidence]
    kind=row['disposition']
    if kind in ['upstream_research_input','upstream_build_input']:
        upstream=row.get('upstream',{})
        if any(not isinstance(upstream.get(k),str) or not upstream[k].strip() for k in ['repository','revision','path']):
            raise ValueError('unknown upstream provenance remains unresolved')
        if kind=='upstream_build_input':
            representation=row.get('representation')
            if representation=='raw':
                if upstream.get('source_sha256')!=source['sha256']:
                    raise ValueError('upstream raw source digest differs')
            elif representation=='CRLF-to-LF':
                raw=checked_path(repo,row['source'],'artifacts/').read_bytes()
                if hashlib.sha256(raw).hexdigest()!=source['sha256'] or b'\0' in raw:
                    raise ValueError('normalized upstream source changed or is binary')
                raw.decode('utf-8',errors='strict')
                normalized=raw.replace(b'\r\n',b'\n')
                oid=hashlib.sha1(b'blob '+str(len(normalized)).encode('ascii')+b'\0'+normalized).hexdigest()
                if upstream.get('git_blob_oid')!=oid:
                    raise ValueError('upstream normalized Git blob differs')
            else:raise ValueError('explicit upstream representation required')

    elif kind=='private_fixture_source':
        fixture=row.get('fixture',{})
        producer=fixture['producer'];input_pin=fixture['input'];summary_pin=fixture['summary']
        pin(producer['path'],producer['sha256'],producer['bytes'],'scripts/',evidence=True)
        for item in [input_pin,summary_pin]:pin(item['path'],item['sha256'],item['bytes'],'artifacts/',evidence=True)
        document_cache=git_cache if git_cache is not None else {}
        def document(item):
            key=('fixture-document',item['path'],item['sha256'])
            def load_document():
                raw=checked_path(repo,item['path'],'artifacts/').read_bytes()
                if hashlib.sha256(raw).hexdigest()!=item['sha256']:raise ValueError('fixture metadata changed during read')
                return json.loads(raw)
            return cached(document_cache,key,load_document)
        inputs=document(input_pin);summary=document(summary_pin);label=fixture.get('label')
        choices=[r for r in inputs.get('workspaces',[]) if r.get('label')==label]
        if not isinstance(label,str) or not label or len(choices)!=1:raise ValueError('fixture input label is not unique')
        root=Path(choices[0]['root']);relative=fixture['copy_relative_path']
        if not root.is_absolute() or root==repo:raise ValueError('private fixture requires a concrete indexed input root')
        ultimate_root=root
        if root.is_relative_to(repo):
            chain=fixture.get('upstream_fixture',{})
            if not root.is_relative_to(repo/'artifacts') or choices[0].get('copied_tree') is not True:
                raise ValueError('inside-repository input lacks explicit copied-tree chain')
            prior_input=chain['input'];prior_summary=chain['summary']
            for item in [prior_input,prior_summary]:pin(item['path'],item['sha256'],item['bytes'],'artifacts/',evidence=True)
            if checked_path(repo,chain['copy_root'],'artifacts/')!=root:raise ValueError('fixture chain copy root differs')
            prior_choices=[r for r in document(prior_input).get('workspaces',[]) if r.get('label')==label]
            if len(prior_choices)!=1:raise ValueError('fixture origin label is not unique')
            ultimate_root=Path(prior_choices[0]['root'])
            prior=document(prior_summary)
            if not ultimate_root.is_absolute() or ultimate_root.is_relative_to(repo) or prior.get('original_workspaces_unchanged') is not True or not any(r.get('workspace')==label and r.get('isolation_and_citations_passed') is True for r in prior.get('runs',[])):
                raise ValueError('fixture chain lacks concrete external origin/summary')

        if not isinstance(relative,str) or ':' in relative or '\\' in relative or any(p in ('','.','..') for p in relative.split('/')):
            raise ValueError('invalid fixture relative path')
        copy_root=checked_path(repo,fixture['copy_root'],'artifacts/')
        if copy_root/relative != repo/row['source']:raise ValueError('fixture copy path does not match source')
        if summary.get('original_workspaces_unchanged') is not True or not any(r.get('workspace')==label and r.get('isolation_and_citations_passed') is True for r in summary.get('runs',[])):
            raise ValueError('fixture summary lacks recorded unchanged isolated input evidence')
        original=fixture['original'];original_path=Path(original['path'])
        if original_path != root/relative:raise ValueError('original comparison path differs from pinned input root')
        status=original.get('status')
        try:
            actual=observe(original_path,'sha1')
        except FileNotFoundError:
            if status!='missing':raise ValueError('fixture original missing without explicit status')
        except OSError:
            if status!='unavailable' or not original.get('error'):raise ValueError('fixture original inaccessible without explicit gap')
        else:
            if status not in ['matches','differs'] or actual['source_sha256']!=original.get('sha256') or actual['bytes']!=original.get('bytes'):
                raise ValueError('fixture current original comparison differs')
            equal=actual['source_sha256']==source['sha256'] and actual['bytes']==source['bytes']
            if equal != (status=='matches'):raise ValueError('fixture original match status is false')
        origin_path=ultimate_root/relative
        origin_matches=False
        try:
            origin=observe(origin_path,'sha1')
            origin_matches=origin['source_sha256']==source['sha256'] and origin['bytes']==source['bytes']
            verified_observations['ultimate_original']={'path':str(origin_path),'status':'matches' if origin_matches else 'differs',**origin}
        except OSError as error:
            verified_observations['ultimate_original']={'path':str(origin_path),'status':'unavailable','error':str(error)}
        if not origin_matches:
            historical=fixture.get('historical_index',fixture.get('upstream_fixture',{}).get('historical_index'))
            if historical is None:
                gap='No exact historical hash witness; source role established by pinned copy chain'
                if fixture.get('identity_gap')!=gap:raise ValueError('fixture role-only proof requires explicit historical identity gap')
                return {**row,'verified_source':source,'verified_evidence':proofs,**verified_observations,'historical_identity_complete':False,'qualification':False}
            if not isinstance(historical,dict):raise ValueError('invalid historical fixture proof')
            inv=historical['inventory'];manifest=historical['manifest']
            for item in [inv,manifest]:pin(item['path'],item['sha256'],item['bytes'],'artifacts/',evidence=True)
            index=document(inv);binding=document(manifest)
            if binding.get('inventory_digest')!=inv['sha256'] or historical.get('relative_path')!=relative:
                raise ValueError('historical fixture inventory binding differs')
            entry=index['files'][relative]
            if entry['state']['source_hash']!=source['sha256'] or entry['source_bytes']!=source['bytes'] or historical.get('source_sha256')!=source['sha256'] or historical.get('source_bytes')!=source['bytes']:
                raise ValueError('historical indexed source bytes differ')
            indexed_root=str(index['root'])
            if indexed_root.startswith('\\\\?\\'):indexed_root=indexed_root[4:]
            if Path(indexed_root)!=ultimate_root:raise ValueError('historical input root differs from concrete origin')
            verified_observations['historical_input_verified']=True

    elif kind=='upstream_reference_document':
        from urllib.parse import urlsplit
        reference=row.get('reference',{});url=urlsplit(reference.get('url',''))
        if url.scheme not in ['http','https'] or not url.hostname or url.username or url.password:
            raise ValueError('explicit public reference URL required')
        if reference.get('content_sha256')!=source['sha256']:
            raise ValueError('reference content digest differs')
        if 'provenance_gap' not in reference or (reference['provenance_gap'] is not None and (not isinstance(reference['provenance_gap'],str) or not reference['provenance_gap'].strip())):
            raise ValueError('explicit reference provenance gap state required')
    elif kind=='observed_tool_input':
        import re
        tool=row['tool_input'];installed=tool['installed'];producer=tool['producer'];relative=tool['relative_path']
        if not isinstance(relative,str) or not relative.startswith('skills/.system/') or ':' in relative or '\\' in relative or any(p in ('','.','..') for p in relative.split('/')):
            raise ValueError('invalid installed tool input relative path')
        home=checked_path(repo,tool['private_home'],'artifacts/')
        if home/relative!=repo/row['source']:raise ValueError('private tool home does not bind source path')
        installed_path=Path(installed['absolute_path']);expected=Path.home()/'.codex'/relative
        if installed_path!=expected:raise ValueError('tool input is not the verified caller current profile source')
        actual=observe(installed_path,'sha1')
        if actual['source_sha256']!=installed['sha256'] or actual['bytes']!=installed['bytes'] or installed['sha256']!=source['sha256'] or installed['bytes']!=source['bytes']:
            raise ValueError('installed tool source bytes differ')
        pin(producer['path'],producer['sha256'],producer['bytes'],'scripts/',evidence=True)
        if 'CODEX_HOME' not in checked_path(repo,producer['path'],'scripts/').read_text(encoding='utf-8'):
            raise ValueError('fixture producer lacks private tool-home binding')
        bindings=tool.get('bindings',[])
        if not bindings:raise ValueError('tool fixture logs required')
        normalized_home=str(home).replace('\\','/').casefold();bound=False
        for item in bindings:
            pin(item['path'],item['sha256'],item['bytes'],'artifacts/',evidence=True)
            raw=checked_path(repo,item['path'],'artifacts/').read_bytes()
            if hashlib.sha256(raw).hexdigest()!=item['sha256']:raise ValueError('tool binding changed during read')
            text=re.sub('/+','/',raw.decode('utf-8').replace('\\','/')).casefold()
            if re.search(re.escape(normalized_home)+r'(?=/|["\s]|$)',text):bound=True
        if not bound:raise ValueError('logs do not bind the declared private tool home')
        verified_observations['upstream_release_verified']=False
    elif kind=='generated_literal':
        import re
        generated=row['generated'];owner=generated['git_generator'];commit=owner['commit'];path=owner['path']
        if re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',commit) is None or path!='packages/vscode/src/test/native.test.ts':
            raise ValueError('unsupported historical fixture literal generator')
        reachable_ref=owner.get('reachable_ref');reachable_head=owner.get('reachable_head','HEAD')
        if reachable_ref is not None:
            if re.fullmatch(r'refs/(heads|remotes)/[A-Za-z0-9._/-]+',reachable_ref) is None or re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',reachable_head) is None:
                raise ValueError('invalid retained generator ref identity')
            current=subprocess.check_output(['git','-C',str(repo),'rev-parse','--verify',reachable_ref]).decode().strip()
            if current!=reachable_head:raise ValueError('retained generator ref changed')
        elif reachable_head!='HEAD':raise ValueError('explicit historical head requires its actual retained ref')
        cache=git_cache if git_cache is not None else {};key=('literal',commit,path,reachable_ref,reachable_head)
        if key not in cache:
            subprocess.run(['git','-C',str(repo),'merge-base','--is-ancestor',commit,reachable_head],check=True,capture_output=True)
            raw=subprocess.check_output(['git','-C',str(repo),'show',commit+':'+path])
            oid=subprocess.check_output(['git','-C',str(repo),'rev-parse',commit+':'+path]).decode().strip();cache[key]=(raw,oid)
        raw,oid=cache[key]
        if oid!=owner['blob_oid'] or hashlib.sha256(raw).hexdigest()!=owner['sha256'] or len(raw)!=owner['bytes']:raise ValueError('fixture literal generator bytes differ')
        literal=generated['literal_lexeme'];anchor=generated['source_anchor'];text=raw.decode('utf-8')
        filename=Path(row['source']).name
        if filename not in ['a.py','b.py'] or not isinstance(literal,str) or len(literal)>65536 or re.fullmatch(r"'(?:[^'\\\r\n]|\\[nrt\\'])*'",literal) is None:
            raise ValueError('unsupported fixture JavaScript string literal')
        expected_anchor=f"await fs.writeFile(path.join({filename[0]}.root,'{filename}'),{literal});"
        if anchor!=expected_anchor or text.count(anchor)!=1:
            raise ValueError('fixture literal is not the exact supported write content argument')
        escapes={'n':'\n','r':'\r','t':'\t',"'":"'",'\\':'\\'}
        body=iter(literal[1:-1]);decoded=''
        for character in body:
            decoded+=escapes[next(body)] if character=='\\' else character
        actual=checked_path(repo,row['source'],'artifacts/').read_bytes()
        if decoded.encode('utf-8')!=actual or hashlib.sha256(actual).hexdigest()!=source['sha256'] or generated['reproduced_sha256']!=source['sha256']:
            raise ValueError('whole fixture literal output differs')
    elif kind=='generated_template':
        import re
        from pathlib import PureWindowsPath
        generated=row['generated'];owner=generated['git_generator'];commit=owner['commit'];path=owner['path']
        if re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',commit) is None or path!='packages/vscode/src/cliInstall.ts':
            raise ValueError('unsupported historical launcher generator')
        cache=git_cache if git_cache is not None else {};key=('template',commit,path)
        if key not in cache:
            subprocess.run(['git','-C',str(repo),'merge-base','--is-ancestor',commit,'HEAD'],check=True,capture_output=True)
            raw=subprocess.check_output(['git','-C',str(repo),'show',commit+':'+path])
            oid=subprocess.check_output(['git','-C',str(repo),'rev-parse',commit+':'+path]).decode().strip()
            cache[key]=(raw,oid)
        raw,oid=cache[key]
        if oid!=owner['blob_oid'] or hashlib.sha256(raw).hexdigest()!=owner['sha256'] or len(raw)!=owner['bytes']:
            raise ValueError('historical template generator bytes differ')
        text=raw.decode('utf-8');literal=generated['template_literal']
        anchor="const content=platform==='win32'?`"+literal+'`'
        if text.count(anchor)!=1 or '/["%!]/.test(launcher)' not in text:
            raise ValueError('actual historical template/parameter guard not uniquely bound')
        if hashlib.sha256(literal.encode('utf-8')).hexdigest()!=generated['template_sha256'] or literal.count('${launcher}')!=1:
            raise ValueError('template literal identity differs')
        launcher=generated['parameters']['launcher']
        if not isinstance(launcher,str) or not PureWindowsPath(launcher).is_absolute() or any(c in launcher for c in '"%!\r\n'):
            raise ValueError('historical template parameter is unsafe')
        reproduced=literal.replace('\\r','\r').replace('\\n','\n').replace('${launcher}',launcher).encode('utf-8')
        actual=checked_path(repo,row['source'],'artifacts/').read_bytes()
        if reproduced!=actual or hashlib.sha256(actual).hexdigest()!=source['sha256'] or generated['reproduced_sha256']!=source['sha256']:
            raise ValueError('whole historical template output differs')
    elif kind=='generated_fixture':
        generated=row.get('generated',{});generator=generated['generator']
        prefix=next((p for p in ['scripts/','crates/','packages/','tools/'] if isinstance(generator.get('path'),str) and generator['path'].startswith(p)),None)
        if prefix is None:raise ValueError('fixture generator outside maintained source roots')
        pin(generator['path'],generator['sha256'],generator['bytes'],prefix)
        if generated.get('reproduced_sha256')!=source['sha256'] or not isinstance(generated.get('method'),str) or not generated['method']:
            raise ValueError('fixture exact reproduction evidence required')
    elif kind=='generated_from_maintained_typescript':
        generated=row.get('generated',{})
        compiler=generated['compiler'];version=generated['source_version']
        shared_source_pin(compiler,'packages/')
        version_path=version.get('path')
        if version_path:
            prefix=next((p for p in ['artifacts/','scripts/','crates/','packages/','tools/'] if isinstance(version_path,str) and version_path.startswith(p)),None)
            if prefix is None:raise ValueError('TypeScript source outside explicit source roots')
            version_pin=shared_source_pin(version,prefix)
        elif version.get('git_blob_oid') and version.get('reachable_head'):
            import re
            cache=git_cache if git_cache is not None else {}
            head=version['reachable_head'];oid=version['git_blob_oid']
            if re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',head) is None or re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',oid) is None:
                raise ValueError('invalid reachable source Git identity')
            def load_reachable():
                listing=subprocess.check_output(['git','-C',str(repo),'rev-list','--objects',head])
                return {line.split(b' ',1)[0].decode('ascii') for line in listing.splitlines()}
            if oid not in cached(cache,('reachable',head),load_reachable):raise ValueError('source blob not reachable from recorded head')
            def load_blob():
                raw=subprocess.check_output(['git','-C',str(repo),'cat-file','blob',oid])
                if len(raw)>16 << 20:raise ValueError('TypeScript source blob exceeds16MiB')
                return {'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)}
            actual=cached(cache,('blob',oid),load_blob)
            if actual['sha256']!=version.get('sha256') or actual['bytes']!=version.get('bytes'):
                raise ValueError('reachable TypeScript blob content differs')
            version_pin={'sha256':actual['sha256'],'bytes':actual['bytes']}
        else:raise ValueError('TypeScript source requires actual file or reachable Git blob')
        if not isinstance(generated.get('options'),dict) or not isinstance(generated.get('method'),str) or not generated['method']:
            raise ValueError('compiler options and method required')
        if generated.get('reproduced_js_sha256')!=source['sha256']:
            raise ValueError('reproduced JavaScript digest differs')
        if version.get('maintained_path'):
            current=shared_source_pin({'path':version['maintained_path'],'sha256':version['sha256'],'bytes':version['bytes']},'packages/')
        elif version.get('git_commit') and version.get('git_path'):
            import re
            commit=version['git_commit'];git_path=version['git_path']
            if re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',commit) is None or not isinstance(git_path,str) or ':' in git_path or '\\' in git_path or any(p in ('','..','.') for p in git_path.split('/')):
                raise ValueError('invalid Git source locator')
            cache=git_cache if git_cache is not None else {};key=('commit_path',str(repo),commit,git_path)
            def load_commit_path():
                oid=subprocess.check_output(['git','-C',str(repo),'rev-parse','--verify',commit+':'+git_path]).decode('ascii').strip()
                if re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',oid) is None:raise ValueError('invalid resolved source blob identity')
                size_raw=subprocess.check_output(['git','-C',str(repo),'cat-file','-s',oid]).decode('ascii').strip()
                if re.fullmatch('[0-9]+',size_raw) is None or int(size_raw)>16<<20:raise ValueError('historical TypeScript source exceeds16MiB before read')
                raw=subprocess.check_output(['git','-C',str(repo),'cat-file','blob',oid])
                if len(raw)!=int(size_raw):raise ValueError('historical TypeScript source size differs')
                blob=(hashlib.sha1 if len(oid)==40 else hashlib.sha256)(b'blob '+str(len(raw)).encode('ascii')+b'\0'+raw).hexdigest()
                if blob!=oid:raise ValueError('resolved Git blob bytes differ')
                return raw,oid
            raw,oid=cached(cache,key,load_commit_path)
            if version.get('git_blob_oid') is not None and version['git_blob_oid']!=oid:raise ValueError('declared source Git blob identity differs')
            retained=checked_path(repo,version['path'],'artifacts/').read_bytes()
            if hashlib.sha256(raw).hexdigest()!=version_pin['sha256']:
                if version.get('normalization') != 'CRLF to LF only' or retained.replace(b'\r\n',b'\n')!=raw:
                    raise ValueError('Git source differs without exact declared CRLF normalization')
        elif version.get('git_blob_oid') and version.get('reachable_head'):
            pass
        elif version_path and any(version_path.startswith(p) for p in ['scripts/','crates/','packages/','tools/']):
            pass
        else:
            raise ValueError('generated TypeScript lacks verified maintained/Git owner')
    elif kind=='generated_javascript_fixture':
        fixture=row['fixture'];parser=fixture['parser'];compiler=fixture['compiler'];generator=fixture['source'];node=fixture['node']
        if parser['path']!='scripts/arbiter/analysis/javascript_provenance/fixture_proof.cjs' or compiler['path']!='packages/vscode/node_modules/typescript/lib/typescript.js':
            raise ValueError('unsupported fixture parser/compiler')
        shared_source_pin(parser,'scripts/');shared_source_pin(compiler,'packages/')
        prefix=next((p for p in ['scripts/','packages/'] if generator['path'].startswith(p)),None)
        if prefix is None:raise ValueError('fixture generator outside maintained roots')
        shared_source_pin(generator,prefix)
        node_path=Path(node['path'])
        if node_path!=Path('C:/Program Files/nodejs/node.exe'):raise ValueError('unsupported fixture parser Node identity')
        actual=(observe_evidence or observe)(node_path,'sha1')
        if actual['source_sha256']!=node['sha256'] or actual['bytes']!=node['bytes']:raise ValueError('fixture parser Node bytes differ')
        cache=git_cache if git_cache is not None else {}
        key=('fixture_ast',parser['sha256'],compiler['sha256'],generator['path'],generator['sha256'],node['sha256'])
        if key not in cache:
            request={'parser':str(checked_path(repo,parser['path'],'scripts/')),'compiler':str(checked_path(repo,compiler['path'],'packages/')),'source':str(checked_path(repo,generator['path'],prefix)),
                     'parser_sha256':parser['sha256'],'compiler_sha256':compiler['sha256'],'source_sha256':generator['sha256']}
            program="const fs=require('node:fs'),crypto=require('node:crypto');const r=JSON.parse(fs.readFileSync(0,'utf8'));for(const k of ['parser','compiler','source']){if(crypto.createHash('sha256').update(fs.readFileSync(r[k])).digest('hex')!==r[k+'_sha256'])throw Error('Parser input drift');}const rows=require(r.parser).extract(require(r.compiler),fs.readFileSync(r.source,'utf8'),r.source);process.stdout.write(JSON.stringify(rows));"
            result=subprocess.run([str(node_path),'-e',program],input=json.dumps(request).encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30,check=True,cwd=repo)
            if len(result.stdout)>16<<20:raise ValueError('fixture AST response exceeds16MiB')
            parsed=json.loads(result.stdout)
            if not isinstance(parsed,list) or len(parsed)>100000:raise ValueError('fixture AST roster exceeds bound')
            cache[key]=parsed
        verified_observations.update(verify_javascript_fixture_literal(cache[key],fixture,source))
    elif kind=='observed_source_recovery':
        import importlib.util,sys
        cache=git_cache if git_cache is not None else {}
        names=['project','patch_replay','recovery','project_patches'];helper_pins={e['path']:e for e in evidence}
        paths=['scripts/arbiter/analysis/upstream_ort/'+n+'.py' for n in names]
        if any(p not in helper_pins for p in paths):raise ValueError('recovery requires all executable helper pins')
        key=('recovery_helpers',tuple(helper_pins[p]['sha256'] for p in paths))
        if key not in cache:
            saved={n:sys.modules.get(n) for n in names};loaded={}
            try:
                for name,path in zip(names,paths):
                    spec=importlib.util.spec_from_file_location(name,checked_path(repo,path,'scripts/'))
                    module=importlib.util.module_from_spec(spec);sys.modules[name]=module
                    code=checked_path(repo,path,'scripts/').read_bytes()
                    if hashlib.sha256(code).hexdigest()!=helper_pins[path]['sha256']:raise ValueError('recovery helper changed before execution')
                    exec(compile(code,str(spec.origin),'exec'),module.__dict__);loaded[name]=module
                cache[key]=(loaded['project_patches'].selected_members,loaded['recovery'].recover)
            finally:
                for name,old in saved.items():
                    if old is None:sys.modules.pop(name,None)
                    else:sys.modules[name]=old
        base=row['base'];archive=base['archive'];record=row['recovery']
        if type(archive.get('bytes')) is not int or not 0<=archive['bytes']<=4<<30:raise ValueError('recovery archive exceeds bound')
        pin(archive['path'],archive['sha256'],archive['bytes'],'artifacts/',evidence=True)
        shared_source_pin(record,'scripts/arbiter/research/ort_dependency_recovery_r1/')
        payload_key=('recovery_payload',key,archive['sha256'],record['sha256'],base['member'],base['sha256'],base['bytes'],source['sha256'],source['bytes'])
        if payload_key not in cache:
            archive_raw=checked_path(repo,archive['path'],'artifacts/').read_bytes()
            record_raw=checked_path(repo,record['path'],'scripts/').read_bytes()
            if hashlib.sha256(record_raw).hexdigest()!=record['sha256'] or len(record_raw)!=record['bytes']:raise ValueError('recovery record changed during read')
            select,recover=cache[key]
            cache[payload_key]=verify_recovery_payload(archive_raw,record_raw,base,source,select,recover)
        verified_observations.update(cache[payload_key])
        verified_observations['maintained_recovery_record']=record
    elif kind=='generated_source_snapshot':
        owner=row.get('maintained',{})
        prefix=next((p for p in ['scripts/','crates/','packages/','tools/'] if isinstance(owner.get('path'),str) and owner['path'].startswith(p)),None)
        if prefix is None:
            raise ValueError('generated owner outside maintained source roots')
        target=pin(owner['path'],owner['sha256'],owner['bytes'],prefix)
        if target['sha256']!=source['sha256']:
            raise ValueError('generated snapshot must match maintained bytes exactly')
    else:
        raise ValueError('unknown disposition subtype')
    return {**row,'verified_source':source,'verified_evidence':proofs,**verified_observations,'qualification':False}


def parse_map_pin(value):
    path,separator,digest=value.rpartition('=')
    if not separator or not path or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):
        raise ValueError('external disposition requires PATH=lowercase-SHA256')
    return Path(path).absolute(),digest


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('inventory',type=Path)
    p.add_argument('--disposition-map',action='append',default=[],help='Pinned external map PATH=SHA256; repeat as needed')
    args=p.parse_args()
    raw=args.inventory.read_bytes()
    value=json.loads(raw)
    if value.get('schema')!='airk.private.source-inventory/1':
        raise ValueError('unexpected inventory schema')
    report=disposition(value,hashlib.sha256(raw).hexdigest())
    if args.disposition_map:
        import inventory as scanner
        repo=Path(value['repository'])
        documents=[];errors=[]
        for supplied in args.disposition_map:
            path,expected=parse_map_pin(supplied)
            try:
                document,identity=scanner.verified_json(path,'sha1')
                if identity['source_sha256']!=expected:raise ValueError('external disposition document pin differs')
                if document.get('schema')!='airk.source-disposition/1' or not isinstance(document.get('files'),list):
                    raise ValueError('unsupported disposition document')
                verified=[verify_disposition_row(row,repo,scanner.source_hashes,scanner.checked_relative) for row in document['files']]
                documents.append({'path':str(path),'sha256':identity['source_sha256'],'verified_rows':verified})
            except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError) as error:
                errors.append({'path':str(path),'type':type(error).__name__,'message':str(error)})
        report['verified_disposition_maps']=documents
        report['disposition_errors']=errors
    print(json.dumps(report,indent=2,ensure_ascii=True))
    if report.get('disposition_errors'):
        raise SystemExit(1)


if __name__=='__main__':
    main()
