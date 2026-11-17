# SPDX-License-Identifier: Apache-2.0
"""Read-only source inventory: never imports, copies, deletes or rewrites candidates."""
import argparse
from collections import Counter, defaultdict
import datetime
import hashlib
import json
import os
import re
from pathlib import Path
import stat
import subprocess
import sys
import time

sys.dont_write_bytecode = True
REPOSITORY = Path(__file__).resolve().parents[4]
EXTENSIONS = frozenset('.py .pyi .rs .ps1 .psm1 .psd1 .sh .bash .zsh .fish .cmd .bat .js .mjs .cjs .ts .tsx .jsx .c .cc .cpp .cxx .h .hh .hpp .hxx .cs .go .java .kt .swift .r .jl .lua .sql .cmake .mk .def .ipynb .patch .diff .cu .cuh .html .htm .css .scss .sass .less .vue .svelte .yaml .yml .toml .proto'.split())
BASENAMES = frozenset(['cmakelists.txt', 'makefile', 'gnumakefile', 'dockerfile', 'justfile'])
EXCLUDED_DIRECTORIES = frozenset(['.git', '.hg', '.svn', 'target', 'targets', 'node_modules', '.venv', 'venv', 'env', 'envs', 'site-packages', '__pycache__', 'vendor', 'vendored', 'third_party', 'third-party', 'external', '.rustup', '.cargo', 'fixtures', 'testdata'])
MAX_SOURCE_BYTES = 16 << 20


def candidate_name(name):
    name = name.lower()
    if Path(name).suffix in {'.in', '.template', '.tmpl'}:
        name = str(Path(name).with_suffix(''))
    return Path(name).suffix in EXTENSIONS or name in BASENAMES


def exclusion(name):
    name = name.lower()
    if name in EXCLUDED_DIRECTORIES or name.startswith('native-fixtures'):
        return 'excluded dependency/environment/target/VCS/fixture directory'
    return None


def alias(info):
    return stat.S_ISLNK(info.st_mode) or bool(getattr(info, 'st_file_attributes', 0) & 0x400)


def stamp(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def source_hashes(path, object_format, max_bytes=MAX_SOURCE_BYTES):
    before = path.lstat()
    if alias(before) or not stat.S_ISREG(before.st_mode):
        raise ValueError('candidate is not a regular non-reparse file')
    if before.st_size > max_bytes:
        raise ValueError('candidate exceeds explicit source byte cap')
    if object_format not in ('sha1', 'sha256'):
        raise ValueError('unsupported Git object format')
    source, blob = hashlib.sha256(), hashlib.new(object_format)
    blob.update(b'blob ' + str(before.st_size).encode('ascii') + b'\0')
    count, chunks = 0, []
    with path.open('rb') as stream:
        if stamp(os.fstat(stream.fileno())) != stamp(before):
            raise ValueError('candidate changed before open')
        while chunk := stream.read(1 << 20):
            count += len(chunk)
            if count > max_bytes or count > before.st_size:
                raise ValueError('candidate grew during hashing')
            source.update(chunk)
            blob.update(chunk)
            chunks.append(chunk)
        if stamp(os.fstat(stream.fileno())) != stamp(before):
            raise ValueError('candidate changed during hashing')
    if count != before.st_size or stamp(path.lstat()) != stamp(before):
        raise ValueError('candidate changed after hashing')
    result = {'bytes': count, 'source_sha256': source.hexdigest(), 'git_blob_oid': blob.hexdigest()}
    normalized = normalized_blob(b''.join(chunks), object_format)
    if normalized is not None:
        result['normalized_text_git_blob_oid'] = normalized
        result['normalization'] = 'Strict UTF-8 text; CRLF replaced by LF only; not raw-byte identity.'
    return result


def evidence_hashes(path, object_format, max_bytes=4 << 30):
    """Stream pinned archives/proofs; never decode or accumulate their payload."""
    before=path.lstat()
    if alias(before) or not stat.S_ISREG(before.st_mode) or before.st_size>max_bytes:
        raise ValueError('evidence must be regular non-reparse and within explicit4GiB cap')
    digest=hashlib.sha256();count=0
    with path.open('rb') as stream:
        if stamp(os.fstat(stream.fileno()))!=stamp(before):raise ValueError('evidence changed before open')
        while chunk:=stream.read(1 << 20):
            count+=len(chunk)
            if count>before.st_size or count>max_bytes:raise ValueError('evidence grew during hashing')
            digest.update(chunk)
        if stamp(os.fstat(stream.fileno()))!=stamp(before):raise ValueError('evidence changed during hashing')
    if count!=before.st_size or stamp(path.lstat())!=stamp(before):raise ValueError('evidence changed after hashing')
    return {'source_sha256':digest.hexdigest(),'bytes':count}


def normalized_blob(data, object_format):
    if b'\0' in data or b'\r\n' not in data:
        return None
    try:
        data.decode('utf-8', errors='strict')
    except UnicodeDecodeError:
        return None
    normalized = data.replace(b'\r\n', b'\n')
    return hashlib.new(object_format, b'blob ' + str(len(normalized)).encode('ascii') + b'\0' + normalized).hexdigest()


def classify(hashes, committed, tracked, migrations=None):
    matches = sorted(tracked.get(hashes['source_sha256'], []))
    if hashes['git_blob_oid'] in committed:
        category, reason = 'committed_git_blob_match', 'Exact raw bytes match a blob reachable from recorded HEAD; no text normalization.'
    elif matches:
        category, reason = 'tracked_worktree_match', 'Exact raw bytes match observed tracked worktree code, not a committed blob reachable from recorded HEAD.'
    elif (migrations or {}).get(hashes['source_sha256']):
        category, reason = 'authenticated_migration_map_match', 'Exact original SHA matches a declared migration map; actual maintained target bytes verified. No equivalence or qualification inferred.'
    elif hashes.get('normalized_text_git_blob_oid') in committed:
        category, reason = 'normalized_text_committed_blob_match', 'CRLF-to-LF normalized UTF-8 text matches a reachable Git blob; this is NOT raw-byte identity.'
    else:
        category, reason = 'remaining_implementation_candidate', 'Filename-selected code has no exact committed-blob or observed tracked-code match; human review required.'
    return {'classification': category, 'reason': reason, 'tracked_worktree_matches': matches,
            'raw_byte_identity_match': category in ('committed_git_blob_match', 'tracked_worktree_match'),
            'migration_matches': (migrations or {}).get(hashes['source_sha256'], [])}


def git(repo, *args, input_bytes=None):
    result = subprocess.run(['git', '-C', str(repo), *args], input=input_bytes, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, env=dict(os.environ, GIT_OPTIONAL_LOCKS='0'), check=False)
    if result.returncode:
        raise RuntimeError('git ' + ' '.join(args) + ': ' + result.stderr.decode('utf-8', errors='replace'))
    return result.stdout


def reachable_blobs(repo, head):
    listing = git(repo, 'rev-list', '--objects', head)
    ids = sorted({line.split(b' ', 1)[0] for line in listing.splitlines() if line})
    lines = git(repo, 'cat-file', '--batch-check=%(objectname) %(objecttype)', input_bytes=b'\n'.join(ids) + b'\n').splitlines()
    if len(lines) != len(ids):
        raise ValueError('partial Git object-type observation')
    blobs = set()
    for expected, line in zip(ids, lines):
        fields = line.split()
        if len(fields) != 2 or fields[0] != expected or fields[1] not in (b'blob', b'tree', b'commit', b'tag'):
            raise ValueError('invalid Git object-type observation')
        if fields[1] == b'blob':
            blobs.add(fields[0].decode('ascii'))
    return blobs


def record_error(errors, path, stage, error):
    errors.append({'path': str(path), 'stage': stage, 'type': type(error).__name__, 'message': str(error)})


def checked_relative(repo, value, prefix):
    if not isinstance(value, str) or ':' in value or '\\' in value or value.startswith('/') or any(p in ('', '.', '..') for p in value.split('/')) or not value.startswith(prefix):
        raise ValueError('noncanonical declared source-map path')
    path = repo / value
    for ancestor in [path, *path.parents]:
        if alias(ancestor.lstat()):
            raise ValueError('declared source-map path has reparse ancestry')
        if ancestor == repo:
            break
    return path


def verified_json(path, object_format):
    identity = source_hashes(path, object_format)
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != identity['source_sha256']:
        raise ValueError('JSON evidence changed during read')
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('duplicate JSON evidence key')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs), identity


def verify_migration_row(row, repo, object_format, observe=source_hashes):
    for key in ['original_sha256', 'maintained_sha256']:
        if not isinstance(row.get(key), str) or re.fullmatch('[0-9a-f]{64}', row[key]) is None:
            raise ValueError('invalid declared migration digest')
    source_prefix = 'tools/retrieval-train/' if isinstance(row.get('source'), str) and row['source'].startswith('tools/retrieval-train/') else 'artifacts/'
    original = checked_relative(repo, row['source'], source_prefix)
    target_prefix = next((p for p in ['scripts/','crates/','packages/','tools/'] if isinstance(row.get('destination'),str) and row['destination'].startswith(p)), None)
    if target_prefix is None:
        raise ValueError('migration target outside maintained source roots')
    target = checked_relative(repo, row['destination'], target_prefix)
    source_pin, target_pin = observe(original, object_format), observe(target, object_format)
    if source_pin['source_sha256'] != row['original_sha256'] or target_pin['source_sha256'] != row['maintained_sha256']:
        raise ValueError('migration original or maintained bytes differ from declaration')
    for field, observed in [('original_bytes', source_pin['bytes']), ('maintained_bytes', target_pin['bytes'])]:
        if field in row and (type(row[field]) is not int or row[field] != observed):
            raise ValueError('migration declared byte count differs')
    changes=row.get('transformations',row.get('changes',[]))
    requested_range=row.get('disposition')=='reviewed-diagnostic-byte-range' or (isinstance(changes,list) and any(isinstance(c,dict) and (str(c.get('kind','')).startswith('byte-range') or 'extracted_sha256' in c or 'offset' in c) for c in changes))
    if requested_range:
        if not isinstance(changes,list) or len(changes)!=2 or not all(isinstance(c,dict) for c in changes):
            raise ValueError('range migration requires exactly extraction then SPDX prefix')
        extraction,prefix=changes
        if set(extraction)!={'kind','offset','bytes','extracted_sha256'} or extraction['kind']!='byte-range-extraction' or set(prefix)!={'kind','prefix'} or prefix['kind']!='spdx-prefix' or prefix['prefix']!='# SPDX-License-Identifier: Apache-2.0\n':
            raise ValueError('unsupported range migration transformation')
        offset=extraction['offset'];length=extraction['bytes'];digest=extraction['extracted_sha256']
        if type(offset) is not int or type(length) is not int or offset<0 or length<=0 or offset+length>source_pin['bytes'] or not isinstance(digest,str) or re.fullmatch('[0-9a-f]{64}',digest) is None:
            raise ValueError('range migration bounds/digest invalid')
        if 'bytes' in row and (type(row['bytes']) is not int or row['bytes']!=source_pin['bytes']):raise ValueError('range container declared size differs')
        raw=original.read_bytes()
        if len(raw)!=source_pin['bytes'] or hashlib.sha256(raw).hexdigest()!=source_pin['source_sha256']:raise ValueError('range container changed during read')
        extracted=raw[offset:offset+length];expected=prefix['prefix'].encode('utf-8')+extracted
        if hashlib.sha256(extracted).hexdigest()!=digest or hashlib.sha256(expected).hexdigest()!=target_pin['source_sha256'] or len(expected)!=target_pin['bytes']:
            raise ValueError('range slice or maintained prefix/body inverse differs')
        return {'declared_source':row['source'],'original_sha256':digest,'range_source':True,
                'container_sha256':source_pin['source_sha256'],'container_bytes':source_pin['bytes'],'offset':offset,'extracted_bytes':length,
                'maintained':{'path':row['destination'],'sha256':target_pin['source_sha256'],'bytes':target_pin['bytes']},'declared_changes':changes}
    return {'declared_source': row['source'], 'original_sha256': row['original_sha256'],
            'maintained': {'path': row['destination'], 'sha256': target_pin['source_sha256'], 'bytes': target_pin['bytes']},
            'declared_changes': row.get('transformations', row.get('changes', []))}


def verify_embedded_row(row, repo, object_format, encoded=False):
    container=checked_relative(repo,row['container'],'artifacts/')
    container_format=row.get('container_format','json')
    pointer=row['json_pointer']
    if container_format=='utf8-text':
        if not encoded or pointer!='':raise ValueError('text encoded container requires empty pointer')
        identity=source_hashes(container,object_format)
        raw=container.read_bytes()
        if hashlib.sha256(raw).hexdigest()!=identity['source_sha256'] or len(raw)!=identity['bytes']:
            raise ValueError('text container changed during read')
        value=raw.decode('utf-8',errors='strict')
    elif container_format=='json':
        value,identity=verified_json(container,object_format)
        if not isinstance(pointer,str) or not pointer.startswith('/'):
            raise ValueError('embedded JSON pointer must identify a field')
        for token in pointer[1:].split('/'):
            if re.search(r'~(?![01])',token):raise ValueError('invalid JSON pointer escape')
            key=token.replace('~1','/').replace('~0','~')
            if isinstance(value,list):
                if re.fullmatch('0|[1-9][0-9]*',key) is None:raise ValueError('invalid JSON pointer index')
                value=value[int(key)]
            elif isinstance(value,dict):value=value[key]
            else:raise ValueError('JSON pointer traverses noncontainer')
    else:raise ValueError('unsupported encoded container format')
    if identity['source_sha256']!=row['container_sha256'] or identity['bytes']!=row['container_bytes']:
        raise ValueError('embedded container pin differs')
    if not isinstance(value,str):raise ValueError('embedded code field is not text')
    decoded=value.encode('utf-8')
    if encoded:
        import base64
        if row.get('decode_method')!='single-python-b64decode-literal/1' or hashlib.sha256(decoded).hexdigest()!=row.get('encoded_field_sha256') or len(decoded)!=row.get('encoded_field_bytes'):
            raise ValueError('encoded source field or method differs')
        matches=re.findall(r"b64decode\('([A-Za-z0-9+/=]+)'\)",value)
        if len(matches)!=1 or value.count('b64decode(')!=1:raise ValueError('encoded source requires exactly one supported literal')
        decoded=base64.b64decode(matches[0],validate=True)
        decoded.decode('utf-8',errors='strict')
    if hashlib.sha256(decoded).hexdigest()!=row['decoded_sha256'] or len(decoded)!=row['decoded_bytes']:
        raise ValueError('decoded embedded source differs')
    destination=row['destination']
    prefix_root=next((p for p in ['scripts/','crates/','packages/','tools/'] if isinstance(destination,str) and destination.startswith(p)),None)
    if prefix_root is None:raise ValueError('embedded target outside maintained source roots')
    target=checked_relative(repo,destination,prefix_root)
    observed=source_hashes(target,object_format)
    prefix=row.get('prefix','')
    if not isinstance(prefix,str):raise ValueError('embedded prefix must be explicit text')
    expected=prefix.encode('utf-8')+decoded
    if observed['source_sha256']!=row['maintained_sha256'] or observed['bytes']!=row['maintained_bytes'] or hashlib.sha256(expected).hexdigest()!=observed['source_sha256'] or len(expected)!=observed['bytes']:
        raise ValueError('embedded maintained target is not declared prefix plus exact decoded body')
    return {'declared_source':row['container'],'json_pointer':pointer,'original_sha256':row['decoded_sha256'],
            'container_sha256':row['container_sha256'],'maintained':{'path':row['destination'],'sha256':observed['source_sha256'],'bytes':observed['bytes']},
            'declared_changes':['explicit prefix only'],'embedded_source':True}


def migration_index(repo, object_format, tracked_names, blobs, errors):
    result, documents = defaultdict(list), []
    root = repo / 'scripts/arbiter'
    def traversal_error(error):
        record_error(errors, error.filename or str(root), 'migration_map_traversal', error)
    for directory, dirs, files in os.walk(root, followlinks=False, onerror=traversal_error):
        keep = []
        for name in sorted(dirs):
            try:
                if not exclusion(name) and not alias((Path(directory)/name).lstat()):
                    keep.append(name)
            except OSError as error:
                traversal_error(error)
        dirs[:] = keep
        for name in sorted(set(files) & {'source-map.json', 'source-provenance.json', 'encoded-source-provenance.json'}):
            path = Path(directory)/name
            relative = path.relative_to(repo).as_posix()
            try:
                value, identity = verified_json(path, object_format)
                if value.get('schema') not in ['airk.research-source-migration/1', 'airk.source-relocation/1', 'airk.embedded-source-migration/1', 'airk.encoded-source-migration/1'] or not isinstance(value.get('files'), list):
                    raise ValueError('unsupported declared migration map')
                document = {'path': relative, 'sha256': identity['source_sha256'], 'bytes': identity['bytes'],
                            'git_tracked': relative in tracked_names, 'committed_blob': identity['git_blob_oid'] in blobs,
                            'declared_rows': len(value['files']), 'verified_rows': 0}
                documents.append(document)
                for row in value['files']:
                    try:
                        verified = verify_embedded_row(row,repo,object_format,encoded=value['schema']=='airk.encoded-source-migration/1') if value['schema'] in ['airk.embedded-source-migration/1','airk.encoded-source-migration/1'] else verify_migration_row(row, repo, object_format)
                        verified['map'] = {'path': relative, 'sha256': identity['source_sha256']}
                        result[verified['original_sha256']].append(verified)
                        document['verified_rows'] += 1
                    except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
                        record_error(errors, relative, 'migration_row', error)
            except (OSError, ValueError, KeyError, TypeError) as error:
                record_error(errors, relative, 'migration_map', error)
    return result, documents


def corpus_evidence(repo, object_format, errors):
    """Only named retained dataset receipts can exclude their declared payload."""
    base = repo/'artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1'
    roots, files, evidence = {}, {}, []
    receipts = [base/f'contextbench-source-join-r{n}/receipt.json' for n in (1,2)]
    receipts += [base/'cosqa-source-join-r1/native-visibility-preparation-r2/native-visibility-input-r1.json']
    for path in receipts:
        if not path.exists():
            continue
        try:
            value, identity = verified_json(path, object_format)
            proof = {'path': path.relative_to(repo).as_posix(), 'sha256': identity['source_sha256']}
            evidence.append(proof)
            if value.get('schema') == 'airk.private.contextbench-original-train-source-join/1':
                for task in value['results']:
                    for source in task.get('source_files', []):
                        file = Path(source['path'])
                        if not file.is_relative_to(path.parent):
                            raise ValueError('corpus source outside named join')
                        suffix = file.relative_to(path.parent).parts
                        if len(suffix) < 3 or suffix[1] != 'source' or re.fullmatch('.+--[0-9a-f]{40}', suffix[0]) is None:
                            raise ValueError('corpus source lacks explicit repository/commit locator')
                        roots[str(path.parent/suffix[0]/'source')] = {'reason':'documented ContextBench pinned upstream source input; not AIRK implementation', 'evidence':proof}
            elif value.get('schema') == 'airk.private.public-human-native-visibility/1':
                source_root = Path(value['source_root'])
                if source_root != path.parent/'source-tree':
                    raise ValueError('corpus source_root differs from retained visibility input')
                roots[str(source_root)] = {'reason':'documented CosQA/CodeSearchNet source-tree input; not AIRK implementation','evidence':proof}
                # The earlier retained r1 staging tree contains the same input
                # bodies; only exact declared payload hashes receive that role.
                old_root = path.parent.parent/'native-visibility-preparation-r1/source-tree'
                for source in value['source_files']:
                    files[str(old_root/source['file'])] = {'sha256':source['sha256'],'bytes':source['bytes'],
                        'reason':'exact corpus payload declared by retained native visibility input','evidence':proof}
            else:
                raise ValueError('unexpected corpus evidence schema')
        except (OSError, ValueError, KeyError, TypeError) as error:
            record_error(errors, path.relative_to(repo), 'corpus_evidence', error)
    return roots, files, evidence


def disposition(row, corpus_files, repo):
    declared = corpus_files.get(str(repo/row['path']))
    if declared and row['source_sha256'] == declared['sha256'] and row['bytes'] == declared['bytes']:
        return 'documented_corpus_input', declared['reason']
    if row['classification'] != 'remaining_implementation_candidate':
        return 'source_identity_or_migration_evidence', 'Source identity/declared maintained owner is recorded separately; no automatic inactivity or purge claim.'
    parts = row['path'].split('/')
    for index, part in enumerate(parts[:-1]):
        if part in ['source','sources','source-tree','native-proposal'] and parts[index+1] in ['crates','packages','scripts','tools']:
            return 'repository_snapshot_or_proposal_review', 'Repository-shaped retained source snapshot/proposal; supersession and callers need review.'
    if any('proposal' in part for part in parts) and Path(row['path']).suffix.lower() in ['.rs','.c','.cc','.cpp','.h','.hpp','.ts','.tsx','.js']:
        return 'native_or_client_proposal_review', 'Retained proposal source, not automatically an active workflow or obsolete code.'
    if Path(row['path']).suffix.lower() in ['.py','.ps1','.sh','.cmd','.bat','.ipynb']:
        return 'authored_operational_or_research_candidate', 'Unmapped script/notebook outside identified corpus/repository snapshots; caller and migration review required.'
    return 'other_unresolved_source_candidate', 'Unmatched source needs an explicit owner/disposition; no automatic exclusion.'


def review_summary(rows, limit):
    groups = {}
    gaps = defaultdict(list)
    for row in rows:
        if row['classification'] != 'remaining_implementation_candidate':
            continue
        parts = row['path'].split('/')
        if 'models' in parts:
            start=parts.index('models')+1; folder='/'.join(parts[start:start+2])
        else:
            start=next((i+1 for i,p in enumerate(parts) if p.startswith('exec-')),len(parts)-1); folder=parts[start]
        key=(folder,Path(row['path']).suffix.lower(),row['disposition'])
        group=groups.setdefault(key, {'top_folder':folder,'extension':key[1],'disposition':key[2],'paths':0,'hashes':set()})
        group['paths']+=1;group['hashes'].add(row['source_sha256'])
        if row['disposition'] in ['authored_operational_or_research_candidate','other_unresolved_source_candidate']:
            gaps[row['source_sha256']].append(row['path'])
    grouped=[]
    for group in groups.values():
        group['unique_sha256']=len(group.pop('hashes'));grouped.append(group)
    review=[]
    for sha, paths in sorted(gaps.items(),key=lambda pair:min(pair[1])):
        review.append({'source_sha256':sha,'representative':min(paths),'path_count':len(paths),'paths':sorted(paths)[:8],
                       'omitted_same_hash_paths':max(0,len(paths)-8),'requires_owner_review':True})
    return {'remaining_groups':sorted(grouped,key=lambda g:(-g['paths'],g['top_folder'],g['extension'])),
            'migration_gap_review':review[:limit], 'migration_gap_distinct_hashes':len(review),
            'migration_gap_list_omitted':max(0,len(review)-limit), 'review_limit':limit}


def walk_candidates(root, repo, errors, excluded, counts, scandir=os.scandir, corpus_roots=None):
    pending = [root]
    while pending:
        directory = pending.pop()
        try:
            if str(directory) in (corpus_roots or {}):
                excluded.append({'path':str(directory.relative_to(repo)), **corpus_roots[str(directory)]})
                continue
            info = directory.lstat()
            if alias(info) or not stat.S_ISDIR(info.st_mode):
                raise ValueError('traversal root is not a plain directory')
            with scandir(directory) as iterator:
                entries = sorted(iterator, key=lambda entry: entry.name)
        except (OSError, ValueError) as error:
            record_error(errors, directory.relative_to(repo), 'traversal', error)
            continue
        counts['directories_visited'] += 1
        for entry in entries:
            path = directory / entry.name
            try:
                info = entry.stat(follow_symlinks=False)
                reason = None
                if alias(info):
                    reason = 'symlink/reparse entry never followed'
                elif stat.S_ISDIR(info.st_mode):
                    reason = exclusion(entry.name)
                    if not reason:
                        pending.append(path)
                elif stat.S_ISREG(info.st_mode):
                    counts['regular_files_seen'] += 1
                    if candidate_name(entry.name):
                        yield path
                    else:
                        counts['non_code_files_excluded'] += 1
                else:
                    reason = 'nonregular entry never consumed'
                if reason:
                    excluded.append({'path': str(path.relative_to(repo)), 'reason': reason})
            except OSError as error:
                record_error(errors, path.relative_to(repo), 'entry_metadata', error)


def validate_roots(repo, roots):
    selected = []
    for root in roots:
        path = Path(os.path.abspath(root))
        if not path.is_relative_to(repo / 'artifacts'):
            raise ValueError('roots must stay under repository artifacts')
        for ancestor in [path, *path.parents]:
            if alias(ancestor.lstat()):
                raise ValueError('root ancestry contains a symlink/reparse point')
            if ancestor == repo:
                break
        if not path.is_dir() or any(path.is_relative_to(p) or p.is_relative_to(path) for p in selected):
            raise ValueError('root missing, overlapping or duplicated')
        selected.append(path)
    if not selected:
        raise ValueError('at least one explicit scoped artifact root required')
    return selected


def inventory(repo, roots, prior_notes=(), review_limit=200):
    started, clock = datetime.datetime.now(datetime.timezone.utc).isoformat(), time.monotonic()
    repo = Path(os.path.abspath(repo))
    roots = validate_roots(repo, roots)
    head = git(repo, 'rev-parse', 'HEAD').decode('ascii').strip()
    object_format = git(repo, 'rev-parse', '--show-object-format').decode('ascii').strip()
    blobs = reachable_blobs(repo, head)
    errors, excluded, rows, counts, tracked = [], [], [], Counter(), defaultdict(list)
    tracked_raw = git(repo, 'ls-files', '-z').split(b'\0')
    tracked_names = {os.fsdecode(raw).replace('\\','/') for raw in tracked_raw if raw}
    migrations, maps = migration_index(repo, object_format, tracked_names, blobs, errors)
    corpus_roots, corpus_files, corpus_proofs = corpus_evidence(repo, object_format, errors)
    for raw in tracked_raw:
        if not raw:
            continue
        relative = os.fsdecode(raw)
        path = repo / relative
        if not candidate_name(path.name):
            continue
        try:
            value = source_hashes(path, object_format)
            tracked[value['source_sha256']].append(relative)
            counts['tracked_code_files_hashed'] += 1
        except (OSError, ValueError) as error:
            record_error(errors, relative, 'tracked_worktree_hash', error)
    for root in roots:
        for path in walk_candidates(root, repo, errors, excluded, counts, corpus_roots=corpus_roots):
            relative = path.relative_to(repo).as_posix()
            try:
                value = source_hashes(path, object_format)
                row = {'path': relative, **value, **classify(value, blobs, tracked, migrations)}
                row['disposition'], row['disposition_reason'] = disposition(row, corpus_files, repo)
                rows.append(row)
            except (OSError, ValueError) as error:
                record_error(errors, relative, 'candidate_hash', error)
    rows.sort(key=lambda row: row['path'])
    groups = Counter(row['source_sha256'] for row in rows if row['classification'] == 'remaining_implementation_candidate')
    for row in rows:
        if row['classification'] == 'remaining_implementation_candidate':
            row['same_candidate_bytes_count'] = groups[row['source_sha256']]
    end_head = git(repo, 'rev-parse', 'HEAD').decode('ascii').strip()
    if end_head != head:
        record_error(errors, 'HEAD', 'git_identity', ValueError(head + ' -> ' + end_head))
    summary = review_summary(rows, review_limit)
    counts.update(row['classification'] for row in rows)
    counts.update(candidate_paths_hashed=len(rows), remaining_distinct_source_hashes=len(groups),
                  committed_blobs_reachable=len(blobs), errors=len(errors), excluded_entries=len(excluded))
    return {'schema': 'airk.private.source-inventory/1', 'repository': str(repo), 'head': head,
            'object_format': object_format, 'started_utc': started, 'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'elapsed_seconds': time.monotonic() - clock, 'roots': [p.relative_to(repo).as_posix() for p in roots],
            'complete_within_declared_scope': not errors, 'atomic_filesystem_snapshot': False,
            'deletion_or_migration_authorized': False, 'reported_prior_scan_notes': list(prior_notes),
            'selection': {'extensions': sorted(EXTENSIONS), 'template_suffixes': ['.in', '.template', '.tmpl'], 'basenames': sorted(BASENAMES),
                          'excluded_directory_names': sorted(EXCLUDED_DIRECTORIES), 'max_source_bytes': MAX_SOURCE_BYTES,
                          'model_data_policy': 'Non-code suffixes are not read; models/weights directories may also contain implementation.',
                          'limitations': ['Only declared roots and filename-selected candidates covered.',
                                         'Receipt JSON embedded code is not extracted or executed.',
                                         'Exact matches do not prove current callers, maintained status, or permission to purge.',
                                         'Worktree matches are per-file observations, not an atomic snapshot.']},
            'migration_maps': maps, 'corpus_evidence': corpus_proofs, 'disposition_counts':dict(Counter(row['disposition'] for row in rows)),
            **summary, 'counts': dict(counts), 'errors': errors, 'excluded': sorted(excluded, key=lambda row: row['path']), 'candidates': rows}


def main():
    global EXTENSIONS, BASENAMES
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=REPOSITORY)
    parser.add_argument('--root', type=Path, action='append', required=True)
    parser.add_argument('--prior-scan-note', action='append', default=[])
    parser.add_argument('--review-limit', type=int, default=200)
    parser.add_argument('--extension-only', action='append', default=[], help='Explicit supplemental suffix selection; repeat per suffix, e.g. .cu. Basename selection disabled.')
    args = parser.parse_args()
    try:
        if args.extension_only:
            if any(re.fullmatch(r'\.[a-z0-9]+', item) is None for item in args.extension_only):
                raise ValueError('extension-only requires lowercase literal dot suffixes')
            EXTENSIONS=frozenset(args.extension_only);BASENAMES=frozenset()
        if not 1 <= args.review_limit <= 2000:
            raise ValueError('review limit must be1..2000')
        value = inventory(args.repo, args.root, args.prior_scan_note, args.review_limit)
    except (OSError, ValueError, RuntimeError) as error:
        value = {'schema': 'airk.private.source-inventory/1', 'complete_within_declared_scope': False,
                 'deletion_or_migration_authorized': False, 'errors': [{'stage': 'initialization', 'type': type(error).__name__, 'message': str(error)}],
                 'reported_prior_scan_notes': args.prior_scan_note}
    json.dump(value, sys.stdout, indent=2, ensure_ascii=True, allow_nan=False)
    sys.stdout.write('\n')
    return 0 if value['complete_within_declared_scope'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
