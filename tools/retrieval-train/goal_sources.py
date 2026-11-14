# SPDX-License-Identifier: Apache-2.0
"""Authenticate actual native excerpt bytes against the frozen licensed fixture."""
import hashlib
import json
from pathlib import Path,PurePosixPath


class FrozenSourceMetadata:
    """Explicit per-campaign metadata only; never cache authority or read results."""
    def __init__(self,fixture_root,expected_manifest_sha256='2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60'):
        self._fixture=Path(fixture_root).resolve()
        self._manifest_bytes=(self._fixture/'manifest.private.json').read_bytes()
        self._manifest_sha256=hashlib.sha256(self._manifest_bytes).hexdigest()
        if self._manifest_sha256!=expected_manifest_sha256:raise ValueError('unreviewed source fixture manifest')
        rows=json.loads(self._manifest_bytes)['files'];entries=[];seen=set()
        for ordinal,row in enumerate(rows):
            name=PurePosixPath(row['file'])
            if name.is_absolute() or '..' in name.parts or ':' in str(name):raise ValueError('foreign fixture metadata path')
            declared=self._fixture/row['file'];canonical=declared.resolve(strict=True)
            if canonical!=declared or not canonical.is_relative_to(self._fixture/'sources') or canonical in seen:
                raise ValueError('fixture metadata alias, duplicate or scope escape')
            seen.add(canonical);entries.append((ordinal,canonical))
        self._entries=tuple(entries);self._indices={}

    def entries(self,fixture,index_root,manifest_bytes):
        if not fixture.samefile(self._fixture) or manifest_bytes!=self._manifest_bytes:
            raise ValueError('metadata cache belongs to a different fixture or manifest')
        if index_root not in self._indices:
            selected=tuple((path.relative_to(index_root).as_posix(),ordinal,path)
                           for ordinal,path in self._entries if path.is_relative_to(index_root))
            if not selected:raise ValueError('index source root has no frozen source files')
            self._indices[index_root]=selected
        return self._indices[index_root]


class ResidentBinding:
    """Pin opaque native-emitted identities; never construct a Core authority."""
    def __init__(self, receipt, *, source_root=None, index_id=None):
        self.path=Path(receipt).resolve();self.raw=self.path.read_bytes();self.receipt=json.loads(self.raw)
        value=self.receipt
        if value.get('schema')!='airk.private-arbiter-index-bindings/1' or value.get('evaluation') is not True or value.get('release_qualified') is not False:
            raise ValueError('original native resident bindings required')
        rows=value.get('bindings',[])
        matches=[row for row in rows if (index_id is None or row.get('index_id')==index_id)
                 and (source_root is None or Path(row['source_root']).samefile(source_root))]
        if len(matches)!=1:raise ValueError('native immutable index binding missing or ambiguous')
        self.row=matches[0];self.index_id=self.row['index_id'];self.namespace=value['namespace']
        self.publication=self.row['publication'];definition=self.row['definition'];roots=self.row['source_roots']
        self.root_ids=[row['id'] for row in roots]
        if (not isinstance(self.publication,dict) or not isinstance(self.publication.get('token'),str)
            or not self.namespace or not value.get('instance') or definition['id']!=self.index_id
            or definition.get('tombstone') is not False or len(roots)!=1 or len(set(self.root_ids))!=1
            or not Path(definition['backing']['root']).samefile(self.row['source_root'])
            or not Path(roots[0]['canonical_path']).samefile(self.row['source_root'])
            or definition['backing']['collection']!=self.publication['collection']):
            raise ValueError('original native index/source/publication identity differs')

    def verify(self):
        if self.path.read_bytes()!=self.raw:raise ValueError('original native binding receipt drift')

    def scope(self, scope):
        if (not isinstance(scope,dict) or scope.get('index_id')!=self.index_id
            or scope.get('catalog')!=self.namespace or scope.get('publication')!=self.publication):
            raise ValueError('native Core scope index/catalog/publication differs from original binding')

    def validate(self,value):
        self.verify()
        members=value.get('scope_observation',{}).get('members',[])
        if (len(members)!=1 or members[0].get('index_id')!=self.index_id
            or members[0].get('source_root_ids')!=self.root_ids
            or members[0].get('source_revision')!=self.row['definition']['source_revision']):
            raise ValueError('native selected index/source observation differs from original binding')
        if isinstance(value.get('records'),list):
            applications=value.get('applications',[])
            if len(applications)!=1:raise ValueError('native application scope omitted or widened')
            self.scope(applications[0].get('scope'))
            for record in value['records']:
                self.scope(record.get('scope'));self.validate(record['canonical_result'])
        else:
            publication=value.get('publication',value.get('read',{}).get('publication'))
            if publication!=self.publication:raise ValueError('native first/current publication differs from original binding')


class SourceLedger:
    """Frozen-byte projection; resident A6 additionally requires native_bindings.

    The default supports retained direct-Kernel/calibration evidence only and
    supplies no service-instance or catalog-authority qualification.
    """
    def __init__(self,fixture_root,index_source_root,expected_manifest_sha256='2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60', *, native_bindings=None,metadata_cache=None):
        self.fixture=Path(fixture_root).resolve();self.index_root=Path(index_source_root).resolve()
        if not self.index_root.is_relative_to(self.fixture/'sources'):raise ValueError('index source root is outside licensed frozen sources')
        self.manifest_bytes=(self.fixture/'manifest.private.json').read_bytes();self.manifest=json.loads(self.manifest_bytes)
        if hashlib.sha256(self.manifest_bytes).hexdigest()!=expected_manifest_sha256:raise ValueError('unreviewed source fixture manifest')
        self.files={};self._canonical_files={}
        if metadata_cache is None:
            for row in self.manifest['files']:
                path=(self.fixture/row['file']).resolve()
                if path.is_relative_to(self.index_root):
                    name=path.relative_to(self.index_root).as_posix()
                    self.files[name]=row;self._canonical_files[name]=path
        else:
            if not isinstance(metadata_cache,FrozenSourceMetadata):raise ValueError('explicit frozen source metadata required')
            for name,ordinal,path in metadata_cache.entries(self.fixture,self.index_root,self.manifest_bytes):
                self.files[name]=self.manifest['files'][ordinal];self._canonical_files[name]=path
        if not self.files:raise ValueError('index source root has no frozen source files')
        self.resident=ResidentBinding(native_bindings,source_root=self.index_root) if native_bindings is not None else None
        self.records={};self.keys={};self.order=[];self.publication=self.resident.publication['token'] if self.resident else None

    def _hits(self,value):
        if isinstance(value.get('records'),list):
            for record in value['records']:
                yield from self._hits(record['canonical_result'])
            return
        publication=value.get('publication',value.get('read',{}).get('publication'))
        if not isinstance(publication,dict) or not isinstance(publication.get('token'),str):raise ValueError('native source result omitted publication')
        if self.publication is None:self.publication=publication['token']
        if self.publication!=publication['token']:raise ValueError('native source publication changed')
        for hit in value.get('hits',[]):yield hit,publication

    def observe(self,canonical_value):
        if (self.fixture/'manifest.private.json').read_bytes()!=self.manifest_bytes:raise ValueError('licensed fixture manifest drift')
        if self.resident:self.resident.validate(canonical_value)
        added=[]
        for hit,publication in self._hits(canonical_value):
            citation=hit['citation'];name=citation['path'].replace('\\','/');relative=PurePosixPath(name)
            if relative.is_absolute() or '..' in relative.parts or ':' in name or name not in self.files:raise ValueError('foreign native citation source')
            expected=self.files[name];path=self.fixture/expected['file']
            if path.is_symlink():raise ValueError('source symlink is not frozen fixture')
            canonical=path.resolve(strict=True)
            if canonical!=self._canonical_files[name] or not canonical.is_relative_to(self.index_root):
                raise ValueError('native source canonical path changed or escaped index scope')
            raw=path.read_bytes();actual=hashlib.sha256(raw).hexdigest()
            if len(raw)!=expected['bytes'] or actual!=expected['sha256'] or citation['content_hash']!=actual:raise ValueError('original source/citation identity differs')
            pieces=raw.split(b'\n');lines=[part+b'\n' for part in pieces[:-1]]+([pieces[-1]] if pieces[-1] else [])
            start,end=citation['start_line'],citation['end_line']
            if type(start) is not int or type(end) is not int or not 1<=start<=end<=len(lines):raise ValueError('native citation line range differs')
            first=sum(len(line) for line in lines[:start-1]);window=b''.join(lines[start-1:end]);excerpt=hit['text'].encode()
            if not excerpt:continue
            at=window.find(excerpt)
            if at<0 or window.find(excerpt,at+1)>=0:raise ValueError('native excerpt is not an unambiguous original byte range')
            begin=first+at;finish=begin+len(excerpt);key=(expected['file'],begin,finish,excerpt)
            if key not in self.keys:
                identifier='r'+str(len(self.records)+1);self.keys[key]=identifier
                self.records[identifier]={'id':identifier,'text':hit['text'],'file':expected['file'],'byte_start':begin,'byte_end':finish,
                    'native_path':citation['path'],'native_start_line':start,'native_end_line':end,'source_sha256':actual,'original_url':expected['paragraphs'][0]['original_url'],
                    'native_observations':[]}
            identifier=self.keys[key];self.records[identifier]['native_observations'].append({'full_hit':hit,'full_publication':publication})
            added.append(identifier)
        self.order=list(dict.fromkeys(added+self.order))
        return added

    def visible_candidates(self):
        """Newest external response first; preserve native record/hit order within it."""
        return [self.records[identifier] for identifier in self.order]

    def read_followup(self,identifier):
        if identifier not in self.records:raise ValueError('agent requested an unreturned record')
        record=self.records[identifier]
        return 'context',{'path':record['native_path'],'line':record['native_start_line'],'before':0,'after':32,'max_bytes':4096,'revision':self.publication,'response_mode':'full','include_status':True}
