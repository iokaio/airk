# SPDX-License-Identifier: Apache-2.0
"""Owned, offline CMake 3.31.6 source-tool fallback; never changes vendor trust."""
import json
import os
from pathlib import Path

LOCK_PATH = Path(__file__).with_name('static-cmake-source-lock.json')
LOCK = json.loads(LOCK_PATH.read_text(encoding='utf-8'))


def validate_lock():
    if (LOCK.get('schema') != 1 or LOCK.get('version') != '3.31.6'
            or LOCK.get('url') != 'https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6.tar.gz'
            or LOCK.get('bytes') != 11710589
            or LOCK.get('sha256') != '653427f0f5014750aafff22727fb2aa60c6c732ca91808cfb78ce22ddd9e55f0'
            or LOCK.get('archive_root') != 'cmake-3.31.6' or LOCK.get('program') != 'bootstrap'
            or LOCK.get('license') != {'path':'Copyright.txt','bytes':5477,
                'sha256':'0676e30827684487aa0ea8f72f01e9460a6320fa34a2888df0ad1dd3e8a06eb8'}):
        raise ValueError('Unapproved CMake source provenance')


def commands(root, compiler, cxx, sdk=None):
    source, build = root / 'cmake-source-source', root / 'cmake-source-build'
    bootstrap = ['/bin/sh', str(source / 'bootstrap'), '--parallel=2', '--no-system-libs', '--no-qt-gui',
        '--prefix=' + str(root / 'cmake-source-install'), '--', '-DCMAKE_BUILD_TYPE=Release',
        '-DBUILD_TESTING=OFF', '-DCMAKE_USE_OPENSSL=OFF', '-DFETCHCONTENT_FULLY_DISCONNECTED=ON',
        '-DCMAKE_OSX_ARCHITECTURES=arm64', '-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3',
        '-DCMAKE_C_COMPILER=' + str(compiler), '-DCMAKE_CXX_COMPILER=' + str(cxx)]
    if sdk is not None:
        bootstrap.append('-DCMAKE_OSX_SYSROOT=' + str(sdk))
    return bootstrap, ['/usr/bin/make', '-j2', 'cmake']


def source_environment(compiler, cxx, supplied=None):
    supplied = os.environ if supplied is None else supplied
    # Compiler, make and shell startup controls are not ambient authority.
    environment = {key:value for key,value in supplied.items()
                   if key in {'HOME','USER','LOGNAME','TMPDIR','TMP','TEMP','LANG','LC_ALL'}}
    environment.update(PATH='/usr/bin:/bin:/usr/sbin:/sbin', CC=str(compiler), CXX=str(cxx),
                       MAKE='/usr/bin/make', MACOSX_DEPLOYMENT_TARGET='13.3', PYTHONNOUSERSITE='1')
    return environment


def observe(argv, root, name, core, environment):
    log = core.probe_log(root, name)
    result = core.run_owned(argv, root, log, 15, environment)
    core.write_json(log.parent / 'result.json', result)
    if not core.successful(result) or log.stat().st_size > 4096:
        raise ValueError('CMake source prerequisite observation failed: ' + name)
    return result, log.read_text(errors='strict').strip()


def verify_source_cache(path, configure, core):
    # Authenticated bootstrap emits CMAKE_USE_SYSTEM_LIBRARIES=0, then
    # CMAKE_HANDLE_SYSTEM_LIBRARIES removes that umbrella cache input and
    # retains these effective per-library BOOL settings (CMakeLists.txt).
    libraries = ('BZIP2','CPPDAP','CURL','EXPAT','FORM','JSONCPP','KWIML',
                 'LIBARCHIVE','LIBLZMA','LIBRHASH','LIBUV','NGHTTP2','ZLIB','ZSTD')
    return core.verify_cmake_cache(path, configure +
        ['-DCMAKE_USE_SYSTEM_' + name + '=OFF' for name in libraries])


def build(root, target, download, core, archive_owner):
    validate_lock()
    if target != 'aarch64-apple-darwin' or core.host_target() != target:
        raise ValueError('The reviewed CMake source fallback is Mac arm64 only')
    root = core.plain(root)
    receipt_path = root / 'cmake-source-receipt.json'
    if receipt_path.exists():
        return verify(root, target, core, archive_owner)
    build_dir = root / 'cmake-source-build'
    if build_dir.exists() or (root / 'cmake-source-attempt.json').exists():
        raise ValueError('Incomplete CMake source build retained; no automatic retry')
    archive = root / 'downloads/cmake-3.31.6.tar.gz'
    core.fetch(archive, LOCK, download)
    entries = archive_owner.extract(archive, root / 'cmake-source-source', LOCK, core)
    if any(row['kind'] != 'file' and row['kind'] != 'directory' for row in entries):
        raise ValueError('CMake source archive contains unsupported links')
    core.verify_file(root / 'cmake-source-source/Copyright.txt', LOCK['license'])
    compiler, cxx = Path('/usr/bin/cc'), Path('/usr/bin/c++')
    environment = source_environment(compiler, cxx)
    compiler_probe, compiler_version = observe([compiler, '--version'], root, 'source-cc', core, environment)
    cxx_probe, cxx_version = observe([cxx, '--version'], root, 'source-cxx', core, environment)
    sdk_probe, sdk_path = observe(['/usr/bin/xcrun', '--sdk', 'macosx', '--show-sdk-path'], root, 'source-sdk', core, environment)
    sdk = Path(sdk_path).resolve(strict=True)
    sdk_version_probe, sdk_version = observe(['/usr/bin/xcrun', '--sdk', 'macosx', '--show-sdk-version'], root, 'source-sdk-version', core, environment)
    selected_compilers = {}
    for kind, name in [('cc','clang'), ('cxx','clang++')]:
        backend_probe, backend = observe(['/usr/bin/xcrun', '--find', name], root, 'source-' + kind + '-backend', core, environment)
        selected_compilers[kind] = {**core.tool_record(Path(backend).resolve(strict=True)), 'probe':backend_probe}
    configure, make = commands(root, compiler, cxx, sdk)
    receipt = {'schema':1, 'result':'running', 'target':target, 'lock_sha256':core.digest(LOCK_PATH),
        'source_entries':entries, 'compiler':core.tool_record(compiler), 'cxx':core.tool_record(cxx),
        'make':core.tool_record(Path('/usr/bin/make')), 'selected_compilers':selected_compilers, 'configure_argv':configure, 'build_argv':make,
        'commands':[], 'license':LOCK['license'],
        'macos_sdk':{'path':str(sdk),'version':sdk_version,'probe':sdk_probe,'version_probe':sdk_version_probe},
        'compiler_observation':{'version_output':compiler_version,'probe':compiler_probe},
        'cxx_observation':{'version_output':cxx_version,'probe':cxx_probe}}
    build_dir.mkdir()
    core.write_json(root / 'cmake-source-attempt.json', receipt)
    try:
        for stage, argv in [('cmake-source-bootstrap', configure), ('cmake-source-build', make)]:
            command = core.run_owned(argv, build_dir, root / ('logs/' + stage + '.log'), 3600, environment)
            command['stage'] = stage
            receipt['commands'].append(command)
            core.write_json(root / 'cmake-source-attempt.json', receipt, replace=True)
            if not core.successful(command):
                raise ValueError('Owned CMake source stage failed; original evidence retained')
        receipt['program'] = core.tool_record(build_dir / 'bin/cmake')
        receipt['probe'] = core.probe_version(build_dir / 'bin/cmake', '3.31.6', root, 'source-cmake')
        archive_owner.verify_tree(root / 'cmake-source-source', entries, core)
        verify_source_cache(build_dir / 'CMakeCache.txt', configure, core)
        receipt['result'] = 'pinned_cmake_source_tool_built'
        core.write_json(root / 'cmake-source-attempt.json', receipt, replace=True)
        core.write_json(receipt_path, receipt)
    except Exception as error:
        receipt.update(result='failed', error=str(error))
        core.write_json(root / 'cmake-source-attempt.json', receipt, replace=True)
        raise
    return verify(root, target, core, archive_owner)


def complete_retained_build(root, target, core, archive_owner):
    """Explicit verification-only completion after the old umbrella-check bug.

    Never re-executes bootstrap/build or edits the original failed attempt.
    """
    root = core.plain(root)
    attempt_path = root / 'cmake-source-attempt.json'
    original = core.read_json(attempt_path)
    if (original.get('result') != 'failed' or original.get('error') !=
            'Actual static ORT CMake cache differs: CMAKE_USE_SYSTEM_LIBRARIES'):
        raise ValueError('Retained CMake source attempt is not the known post-build check failure')
    receipt = {**original, 'result':'pinned_cmake_source_tool_built',
               'retained_attempt_sha256':core.digest(attempt_path)}
    receipt.pop('error')
    verify_receipt(root, target, receipt, core, archive_owner)
    core.write_json(root / 'cmake-source-receipt.json', receipt)
    return verify(root, target, core, archive_owner)


def verify(root, target, core, archive_owner):
    root = core.plain(root)
    receipt = core.read_json(root / 'cmake-source-receipt.json')
    verify_receipt(root, target, receipt, core, archive_owner)
    return {'program':str(root / 'cmake-source-build/bin/cmake'),
            'receipt_sha256':core.digest(root / 'cmake-source-receipt.json')}


def verify_receipt(root, target, receipt, core, archive_owner):
    validate_lock()
    if (receipt.get('schema') != 1 or receipt.get('result') != 'pinned_cmake_source_tool_built'
            or receipt.get('target') != target or target != 'aarch64-apple-darwin'
            or receipt.get('lock_sha256') != core.digest(LOCK_PATH) or receipt.get('license') != LOCK['license']):
        raise ValueError('CMake source tool receipt differs')
    core.verify_file(root / 'downloads/cmake-3.31.6.tar.gz', LOCK)
    entries = archive_owner.archive_entries(root / 'downloads/cmake-3.31.6.tar.gz', LOCK, core)
    if receipt['source_entries'] != entries:
        raise ValueError('CMake source inventory changed')
    archive_owner.verify_tree(root / 'cmake-source-source', entries, core)
    core.verify_file(root / 'cmake-source-source/Copyright.txt', LOCK['license'])
    for key in ('compiler','cxx','make'):
        core.verify_file(receipt[key]['path'], receipt[key], system_tool=True)
    for backend in receipt['selected_compilers'].values():
        core.verify_file(backend['path'], backend, system_tool=True)
    configure, make = commands(root, Path(receipt['compiler']['path']), Path(receipt['cxx']['path']), Path(receipt['macos_sdk']['path']))
    if (receipt['configure_argv'] != configure or receipt['build_argv'] != make
            or [row['stage'] for row in receipt['commands']] != ['cmake-source-bootstrap','cmake-source-build']
            or not all(core.successful(row) for row in receipt['commands'])):
        raise ValueError('CMake source actual commands did not pass/settle')
    verify_source_cache(root / 'cmake-source-build/CMakeCache.txt', configure, core)
    expected = root / 'cmake-source-build/bin/cmake'
    if Path(receipt['program']['path']) != expected:
        raise ValueError('CMake source output path differs')
    core.verify_file(expected, receipt['program'])
    if receipt['probe']['version'] != '3.31.6' or not core.successful(receipt['probe']['probe']):
        raise ValueError('CMake source version probe did not pass')
    if ('retained_attempt_sha256' in receipt and receipt['retained_attempt_sha256'] !=
            core.digest(root / 'cmake-source-attempt.json')):
        raise ValueError('Retained CMake source attempt changed')
