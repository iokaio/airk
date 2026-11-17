# SPDX-License-Identifier: Apache-2.0
"""Read-only finite Cargo output roles; never invokes Cargo or upstream generators."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib
from project_archives import REPOSITORY, consumed, checked, strict_json

REVISION='2a71e35d94b2a02f7dd4a0c4cfd59c339370e747'
ROOTS=('artifacts/v190/p10-target/','artifacts/one-process/workers-target/')

def sha(raw):return hashlib.sha256(raw).hexdigest()
def pin(path,raw):
    try:name=path.relative_to(REPOSITORY).as_posix()
    except ValueError:name=path.as_posix()
    return dict(path=name,sha256=sha(raw),bytes=len(raw))
def external(path):
    for part in [path,*path.parents]:
        st=part.lstat()
        if part.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('aliased input refused')
    return consumed(path)[0]

def private_template(producer,version):
    text=producer.decode('utf-8')
    matches=re.findall(r'const PRIVATE: &str = "\\\r?\n(.*?)";',text,re.S)
    if len(matches)!=1 or 'PRIVATE.replace("$$", &patch_version)' not in text or 'out_dir.join("private.rs")' not in text:raise ValueError('unrecognized private-module producer')
    body=matches[0]
    if '\\' in body:raise ValueError('unexpected template escape')
    return body.replace('$$',version.rsplit('.',1)[1]).encode()

def flag_template(producer):
    text=producer.decode();start=text.index('fn ensure_check_file(');section=text[start:text.index('fn is_flag_supported_inner',start)]
    if 'out_dir.join("flag_check.c")' not in section:raise ValueError('missing exact flag-probe destination')
    values=re.findall(r'write!\(f, "([^"\\]+)"\)',section)
    if len(values)!=1:raise ValueError('ambiguous flag probe literal')
    return values[0].replace('{{','{').replace('}}','}').encode()

def oid_role(raw,database,producer):
    if b'fn generate_file' not in producer or b'File::create(&dest_path)' not in producer:raise ValueError('missing OID generation owner')
    entries=[line.split('\t',4) for line in database.decode().splitlines() if line and not line.startswith('#')]
    if any(len(row)!=5 for row in entries):raise ValueError('unexpected OID source table')
    expected={(r[1],r[2]) for r in entries if r[1]!='""'}
    actual=re.findall(r"pub const ([A-Za-z_][A-Za-z0-9_]*[ ]*): Oid<'static> = oid!\(([0-9.]+)\);",raw.decode())
    if len(actual)!=len(expected) or set(actual)!=expected:raise ValueError('OID constants differ from source table')
    features={r[0].replace('-','_') for r in entries}
    if set(re.findall(r'pub fn with_(\w+)\(mut self\)',raw.decode()))!=features:raise ValueError('OID registry features differ')
    if raw.count(b'self.insert(')!=len(entries):raise ValueError('OID registry row count differs')
    return {'named_constants':len(actual),'input_rows':len(entries),'features':len(features)}

def project(input_path,input_sha,cargo_home):
    raw=external(input_path)
    if sha(raw)!=input_sha:raise ValueError('input roster changed')
    report=strict_json(raw);lock_path=REPOSITORY/'Cargo.lock';lock_raw=external(lock_path);lock=tomllib.loads(lock_raw.decode())
    common=[pin(input_path,raw),pin(lock_path,lock_raw),pin(Path(__file__).resolve(),external(Path(__file__).resolve()))]
    packages={};archives=[]
    def package(name,version,member):
        key=(name,version)
        if key not in packages:
            matches=[p for p in lock['package'] if p['name']==name and p['version']==version]
            if len(matches)!=1 or matches[0].get('source')!='registry+https://github.com/rust-lang/crates.io-index':raise ValueError('unique registry lock binding required')
            paths=list((cargo_home/'registry/cache').glob(f'*/{name}-{version}.crate'))
            if len(paths)!=1:raise ValueError('unique retained crate archive required')
            path=paths[0];body=external(path)
            if sha(body)!=matches[0]['checksum']:raise ValueError('archive differs from Cargo.lock checksum')
            archive=tarfile.open(fileobj=io.BytesIO(body),mode='r:*');packages[key]=(archive,pin(path,body));archives.append(archive)
        archive,archive_pin=packages[key];full=f'{name}-{version}/{member}';found=[m for m in archive.getmembers() if m.name==full]
        if len(found)!=1 or not found[0].isfile() or found[0].size>16<<20:raise ValueError('unique bounded producer member required')
        body=archive.extractfile(found[0]).read()
        return body,{'archive':archive_pin,'member':full,'sha256':sha(body),'bytes':len(body),'package':name,'version':version}
    selected=[x for x in report['unresolved'] if any(x['path'].startswith(root) for root in ROOTS)]
    result={'schema':'airk.private.generated-build-roles/1','source':common,'files':[],'unresolved':[],'role_only':True,'exact_historical_identity_complete':False,'qualification':False}
    try:
        for row in selected:
            try:
                path=checked(REPOSITORY,row['path']);body=external(path)
                if sha(body)!=row['source_sha256'] or len(body)!=row['bytes']:raise ValueError('candidate differs from frozen roster')
                out=path.parent;root_path=out.parent/'root-output';root_raw=external(root_path)
                if root_raw.decode().strip().replace('\\','/').casefold()!=out.as_posix().casefold():raise ValueError('retained Cargo OUT_DIR does not bind candidate')
                output_path=out.parent/'output';output=external(output_path);evidence=[*common,pin(root_path,root_raw),pin(output_path,output)];producer=[];details={};exact=False
                if path.name=='flag_check.c':
                    source,p=package('cc','1.2.67','src/lib.rs');producer.append(p)
                    if flag_template(source)!=body or b'CC_FORCE_DISABLE' not in output:raise ValueError('flag-probe bytes/cc build-output anchors differ')
                    method='Whole source equals authenticated cc flag-check literal; retained Cargo output binds compiler-probe role.';exact=True
                elif path.name=='private.rs':
                    package_name=out.parent.name.rsplit('-',1)[0];versions={'ref-cast':'1.0.26','serde':'1.0.229','serde_core':'1.0.229','thiserror':'2.0.21'}
                    version=versions[package_name];source,p=package(package_name,version,'build.rs');producer.append(p)
                    rerun=b'cargo:rerun-if-changed=build/probe.rs' if package_name=='thiserror' else b'cargo:rerun-if-changed=build.rs'
                    if private_template(source,version)!=body or rerun not in source or rerun not in output:raise ValueError('private-module template or build-output anchor differs')
                    method='Whole source equals authenticated PRIVATE template with locked package patch version.';exact=True
                elif path.name=='version.rs':
                    source,p=package('protobuf','3.7.2','build.rs');producer.append(p)
                    if b'fn write_version()' not in source or b'"version.rs"' not in source or b'pub const VERSION:' not in body or b'"3.7.2"' not in body or b'VERSION_3_7_2' not in body:raise ValueError('protobuf version output/producer anchors differ')
                    method='Locked protobuf version declarations and authenticated build.rs write_version destination agree; role only.'
                elif path.name=='oid_db.rs':
                    source,p=package('oid-registry','0.8.1','build.rs');producer.append(p)
                    loader,p=package('oid-registry','0.8.1','src/load.rs');producer.append(p)
                    database,p=package('oid-registry','0.8.1','assets/oid_db.txt');producer.append(p)
                    if b'join("oid_db.rs")' not in source or b'generate_file(&m, dest_path)' not in source or output.strip()!=b'cargo:rerun-if-changed=assets/oid_db.txt':raise ValueError('OID build/input binding differs')
                    details=oid_role(body,database,loader);method='All named OID constants, feature methods and row count agree with authenticated generator inputs; role only, not full-byte regeneration.'
                elif path.name=='bindgen.rs':
                    matches=[p for p in lock['package'] if p['name']=='libsqlite3-sys' and p.get('source','').endswith('#'+REVISION)]
                    if len(matches)!=1:raise ValueError('exact rusqlite Git lock required')
                    roots=list((cargo_home/'git/checkouts').glob('rusqlite-*/'+REVISION[:7]));
                    if len(roots)!=1:raise ValueError('unique retained rusqlite checkout required')
                    checkout=roots[0]
                    def git(*args):return subprocess.check_output(['git','-C',str(checkout),*args],stderr=subprocess.PIPE,timeout=30)
                    if git('rev-parse','HEAD').decode().strip()!=REVISION:raise ValueError('rusqlite checkout revision differs')
                    for member in ('libsqlite3-sys/build.rs','libsqlite3-sys/sqlite3/bindgen_bundled_version.rs'):
                        data=git('show',REVISION+':'+member)
                        if external(checkout/member)!=data:raise ValueError('rusqlite producer/input differs from exact Git blob')
                        producer.append({'repository':'https://github.com/rusqlite/rusqlite','revision':REVISION,'path':member,'sha256':sha(data),'bytes':len(data),'git_blob_oid':git('rev-parse',REVISION+':'+member).decode().strip()})
                        if member.endswith('build.rs'):
                            if b'std::fs::copy(from, out_path)' not in data or b'join("bindgen.rs")' not in data:raise ValueError('rusqlite copy producer differs')
                        elif data!=body:raise ValueError('bindings differ from exact committed upstream source')
                    if (checkout/'libsqlite3-sys/sqlite3').as_posix().casefold() not in output.decode().replace('\\','/').casefold():raise ValueError('retained include path does not bind upstream source')
                    method='Whole source equals locked upstream Git bundled bindings; authenticated copy producer and retained include/OUT_DIR metadata bind role.';exact=True
                else:raise ValueError('unreviewed Cargo output family')
                result['files'].append({'source':row['path'],'source_sha256':sha(body),'source_bytes':len(body),'role':'generated_build_output','producer_role':'third_party_cargo_build_input','method':method,'producer':producer,'evidence':evidence,'details':details,'exact_observed_literal_or_copy_reproduced':exact,'exact_historical_bytes_reproduced':False,'whole_directory_exclusion':False,'historical_execution_identity_proven':False})
            except (ValueError,OSError,KeyError,subprocess.SubprocessError) as error:result['unresolved'].append({'source':row['path'],'reason':str(error)})
        if external(lock_path)!=lock_raw:raise ValueError('Cargo.lock changed during role inspection')
    finally:
        for archive in archives:archive.close()
    result['counts']={'selected':len(selected),'proved_roles':len(result['files']),'unresolved':len(result['unresolved'])};return result

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--input',type=Path,required=True);parser.add_argument('--input-sha256',required=True);parser.add_argument('--cargo-home',type=Path,required=True);args=parser.parse_args()
    result=project(args.input.resolve(),args.input_sha256,args.cargo_home.resolve());print(json.dumps(result,indent=2))
    if result['unresolved']:raise SystemExit(1)
