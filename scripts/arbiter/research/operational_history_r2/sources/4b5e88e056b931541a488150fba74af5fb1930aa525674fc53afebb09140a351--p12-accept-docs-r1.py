# SPDX-License-Identifier: Apache-2.0
"""Apply only the reviewed nine-document P12 acceptance update after all proofs pass.

No build, test, SSH, Git mutation, publication or commit is performed. The explicit
flag records the primary reviewer's decision; passing automated evidence alone
does not invent that decision. Existing historical failure entries are preserved.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path

BASE = Path(__file__).resolve().parent
WORK = BASE / 'p10-worktree'
SOURCE = {'sha256': '4121692a2cc0272358023f5362ae886eeed4fd06203cea7af23bbfc5f4be86b9', 'files': 960}
MAP_SHA = '2b020b25af63bfdfd4febbeff455c5a8aefc26ff4d7e0f36cf333fb9a1c603ba'
NAMES = ['reference-refresh-release-parity', 'retention-release-parity', 'clients',
         'publication-faults', 'off', 'report', 'latency-gates', 'repo-check',
         'license-notices', 'dependency-audit', 'python-tests', 'whitespace']


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def receipt(path):
    path = Path(path)
    value = read(path)
    assert value['exit_code'] == value['actual_exit_code'] == 0, path
    assert value['source_before'] == value['source_after'] == SOURCE, path
    assert value['source_unchanged'] is True, path
    assert value.get('validation_error') is None, path
    log = path.with_name(path.name.replace('.receipt.json', '.log'))
    assert sha(log) == value['log_sha256'], log
    return value


def validate_proofs():
    map_path = BASE / 'p12-final-qualification-command-map-r1.json'
    final_path = BASE / 'p12-final-qualification-r1.json'
    neo_path = BASE / 'p12-neo-combined-verification-r1.json'
    assert sha(map_path) == MAP_SHA, 'Reviewed map changed'
    plan, final, neo = read(map_path), read(final_path), read(neo_path)
    assert final['status'] == 'passed', 'Final queue must finish successfully first'
    assert final['source'] == plan['source_expected'] == SOURCE
    assert final['map_sha256'] == MAP_SHA
    assert [row['name'] for row in final['steps']] == NAMES
    assert [row['name'] for row in plan['commands']] == NAMES
    for row, command in zip(final['steps'], plan['commands'], strict=True):
        assert row['exit_code'] == 0 and row['seal_or_source_error'] is None
        value = receipt(row['receipt'])
        assert value['command'] == command['command'][3:], row['name']
        if command.get('expected_passing_tests'):
            assert value['passing_tests'] == command['expected_passing_tests']
        if row['name'] == 'dependency-audit':
            audit = read(command['expected_report']['path'])
            assert audit['status'] == 'passed' and audit['all_requested_checks_passed'] is True
            assert audit['fresh_full_advisory_check'] is True
            assert audit['network_mode'] == 'live advisories, locked vet imports'
            assert set(audit['checks']) == {'source_review', 'cargo_audit', 'npm_audit',
                                           'cargo_deny', 'cargo_vet', 'advisory_dispositions', 'input_integrity'}
            assert all(check['status'] == 'passed' for check in audit['checks'].values())
    gates = read(BASE / 'p12-final-gates-r1.json')
    assert gates['passed'] is True and gates['limits_changed'] is False
    assert len(gates['gates']) == 84
    focus = read(BASE / 'p12-focused-r4/qualification.json')
    assert focus['status'] == 'passed' and focus['source_expected'] == SOURCE
    assert next(row for row in focus['steps'] if row['name'] == 'tests')['cases'] == 39
    assert final['adopted_focus']['passed'] == 39
    assert final['adopted_focus']['source'] == SOURCE
    receipt(final['adopted_current_fmt_clippy'])
    assert final['separate_ui']['tests'] == 795
    assert final['separate_ui']['passed'] == 776
    assert final['separate_ui']['skipped'] == 19
    for path, digest in plan['pinned_inputs'].items():
        assert sha(path) == digest, ('Qualification input drift', path)
    release_path = BASE / 'p12-release-runtime-r1.receipt.json'
    release = receipt(release_path)
    assert release['cargo_incremental'] == '0'
    assert round(release['seconds'], 3) == 815.503
    assert final['adopted_release']['receipt_sha256'] == sha(release_path)
    seal_path = BASE / 'p12-runtime-final-r1-seal.json'
    assert final['adopted_release']['seal_sha256'] == sha(seal_path)
    sealer = load('p12_accept_sealer', BASE / 'p9-integrated-checks.py')
    assert sealer.seal_runtime(Path(plan['runtime']), SOURCE) == read(seal_path)
    assert neo['status'] == 'passed'
    assert neo['native_source'] == neo['current_windows_source'] == SOURCE
    assert neo['passing_tests'] == 19 and neo['successful_commands'] == 10
    assert len(neo['batches']) == 1
    cleanup = neo['batches'][0]['cleanup']
    assert cleanup['test_exit_code'] == cleanup['cargo_clean_exit_code'] == 0
    assert cleanup['target_removed'] is True and cleanup['source_preserved'] is True
    assert cleanup['clean_log_error'] is None and cleanup['timing_copy_error'] is None
    for name, digest in neo['evidence'].items():
        assert Path(name).name == name and sha(BASE / name) == digest
    os.environ['GIT_CONFIG_GLOBAL'] = str(BASE / 'p10-owned-ignore-controls/global.gitconfig')
    builder = load('p12_accept_builder', WORK / 'scripts/build-service-runtime.py')
    assert builder.source_identity() == SOURCE, 'Native source changed'
    return {'final': sha(final_path), 'map': sha(map_path), 'neo': sha(neo_path),
            'release': sha(release_path), 'runtime_seal': sha(seal_path)}


def render(name, text):
    def replace(old, new):
        nonlocal text
        assert text.count(old) == 1, (name, 'Expected exact unique reviewed text', old)
        text = text.replace(old, new, 1)

    text = text.replace('#explicit-reference-refresh-p12-candidate', '#explicit-reference-refresh')
    text = text.replace('#p12--focused-windows-qualification-release-and-neo-pending-6-november-2026',
                        '#p12--local-phase-checkpoint-6-november-2026')
    if name in ['README.md', 'docs/README.md', 'packages/vscode/README.md', 'docs/releases/release-1.8.8.md']:
        replace('P12 adds an explicit **reviewed reference refresh and separate requery** candidate.\n',
                'P12 adds accepted local **reviewed reference refresh and separate requery**.\n')
        replace('Release-runtime and focused Neo qualification, final primary review and phase\nacceptance are still in progress. The first wave refreshes one existing index\n',
                'The 12-stage Windows release qualification and 19 scoped Neo tests passed;\nNeo testing ended with `cargo clean` and its target removed. Primary review\naccepted this unpublished local phase. The first wave refreshes one existing index\n')
    if name == 'docs/releases/release-1.8.8.md':
        replace('P9–P11b checkpoints and the pending P12 qualification on source `4121692a…` /\n960 native inputs. No P12 release-runtime or Mac result is claimed yet.',
                'P9–P12 local checkpoints on source `4121692a…` / 960 native inputs. The\nmatched Windows release pair built in **815.503 seconds**; all **12** final\nqualification stages and **19** scoped Neo tests passed. Neo `cargo clean`\nexited zero and removed the target. These results do not qualify a published\nrelease, installed editor, default ONNX provider or the remaining P14 matrix.')
        replace('accepted for unpublished local development; P12 remains under qualification.',
                'accepted for unpublished local development, including P12 reviewed refresh.')
    elif name == 'docs/releases/release-1.9.0.md':
        replace('candidate passed 39 focused Windows native tests; release-runtime, focused Mac\nqualification and primary acceptance are still in progress. P13 remains',
                'passed 39 focused Windows native tests, all 12 final release qualification\nstages and 19 scoped Neo tests with successful cleanup. Primary review accepted\nP12 for unpublished local development; installed-editor and final plan acceptance\nremain separate. P13 remains')
    elif name == 'docs/architecture.md':
        replace("P12's unpublished **1.8.8 candidate** adds explicitly reviewed reference refresh",
                "P12's accepted local **1.8.8 working build** adds explicitly reviewed reference refresh")
        replace('and [ledger](research/implementation-v190.md) distinguish the 39 passing focused\nWindows tests from release/Mac/primary acceptance still in progress.',
                'and [ledger](research/implementation-v190.md) record 39 passing focused Windows\ntests, all 12 final release qualification stages, 19 scoped Neo tests with\nsuccessful cleanup and primary local phase acceptance. Installed-editor and\nP14 platform qualification remain separate.')
    elif name == 'docs/source-verification.md':
        replace('the P12 candidate below adds a separate', 'P12 below adds a separate')
        replace('## Explicit reference refresh (P12 candidate)', '## Explicit reference refresh')
        replace('The unpublished 1.8.8 candidate separates **refresh**, **receipt lookup** and',
                'The accepted local, unpublished 1.8.8 build separates **refresh**, **receipt lookup** and')
        replace("exporting it does not start indexing. The candidate's focused Windows native\nselection passed 39 tests and the extension suite passed 776 tests with 19 skips;\nrelease-runtime, focused Neo and primary phase acceptance are still in progress.",
                'exporting it does not start indexing. Focused Windows native selection passed\n39 tests, the extension suite passed 776 tests with 19 skips, all 12 final\nrelease qualification stages passed, and 19 scoped Neo tests passed. Neo\n`cargo clean` exited zero and removed its target. Primary review accepted the\nlocal phase.')
        replace('for exact source identities, historical failures and pending platform/release\nchecks.', 'for exact source identities, historical failures, accepted local qualification\nand remaining installed/platform checks.')
    elif name == 'docs/vscode-extension.md':
        replace('**1.8.8 P12 development candidate:**', '**1.8.8 P12 accepted local development:**')
        replace('Focused Windows native and UI tests passed; release-runtime, focused Mac and\nprimary phase acceptance are in progress. The working build is unpublished.',
                'Focused Windows native and UI tests, all 12 final release qualification stages\nand 19 scoped Neo tests passed; Neo cleanup completed successfully. Primary\nreview accepted the local phase. The build is unpublished and installed-editor\nqualification remains separate.')
    elif name == 'docs/research/implementation-v190.md':
        historical = text[text.index('**Earlier attempts remain failed evidence.**'):text.index('\nThis is bounded development evidence,', text.index('**Earlier attempts remain failed evidence.**'))]
        earlier = text[text.index('### P0a'):text.index('### P12 —')] if '### P0a' in text else None
        replace('**Current checkpoint, 6 November 2026:** P9, P10, P11a and P11b are accepted for',
                '**Current checkpoint, 6 November 2026:** P9, P10, P11a, P11b and P12 are accepted for')
        replace('and P11b as `aa28425`. P12 is implemented but **not phase accepted**: its focused',
                'and P11b as `aa28425`. P12 passed primary local phase review: its focused')
        replace('passed **776 tests with 19 skips**. Release-runtime and focused Neo qualification\nand final primary review are in progress. P13 ranking/diversity remains',
                'passed **776 tests with 19 skips**. All **12** final Windows release qualification\nstages and **19** scoped Neo tests passed; Neo `cargo clean` exited zero and\nremoved its target. The matched release pair built in **815.503 seconds**.\nThe signed checkpoint is recorded by the subsequent human-authorized commit;\nthis entry does not invent its hash. P13 ranking/diversity remains')
        replace('| P12 | Implemented; qualification in progress, not accepted | 39 focused Windows native tests and strict scoped Clippy passed; UI 776 passed/19 skipped; release/Neo and primary review pending |',
                '| P12 | Accepted locally | 39 focused Windows native tests; strict scoped Clippy; UI 776 passed/19 skipped; all 12 final Windows release stages and 19 scoped Neo tests passed; clean exit zero and target removed |')
        replace('### P12 — focused Windows qualification; release and Neo pending, 6 November 2026',
                '### P12 — local phase checkpoint, 6 November 2026')
        replace("P12's current **1.8.8** implementation is **not yet phase accepted**. The frozen",
                "P12's current **1.8.8** implementation passed primary review and is **accepted\nfor unpublished local development**. The frozen")
        replace('(**960 inputs**). The optimized release pair and focused Neo batch are in\nprogress; no result or cleanup completion is claimed for those runs here.',
                '(**960 inputs**). The matched Windows release pair built in **815.503 seconds**\nwith release incremental compilation disabled. All **12** final qualification\nstages and **19** scoped ARM64 Neo tests passed; Neo cleanup exited zero and\nremoved the target. This is a local phase checkpoint, not publication or final\ninstalled-editor acceptance.')
        release_evidence = '''**Final source-bound qualification:**

- `p12-final-qualification-r1` passed all **12** stages: actual refresh release
  parity, retained-profile release parity, service clients, publication faults,
  Off and Report samples, unchanged latency gates, repository checks, license
  notices, fresh dependency audit, Python tests and whitespace checks. These
  adopt the existing successful focused and UI results; they do not rerun or
  relabel those earlier results. All **84** original latency gates passed with
  their limits unchanged. No P12 resource-cost improvement is claimed.
- The release producer and all native stages retain the exact 960-input source
  above. Main SHA-256:
  `1e41d1bf497eb8b092491629160e0d988cdd15c675417603a1fb30844e4543e1`;
  helper:
  `bc873940a807c772deaf0b678ca878a769d8a99023a54eb5b78a1e9f102750a1`.
  Runtime/build/install manifests and notices remain sealed together; no
  replacement producer or source identity is inferred from current timestamps.
- `p12-neo-combined-verification-r1` confirms the same source, **19** passing
  tests across **10** successful commands on ARM64 Neo. Actual native/helper and
  source/OS custody cases passed. Test exit and `cargo clean` exit were **0**;
  the target was removed, while source, reports and timing evidence survived.
  This is scoped Mac evidence, not a full workspace, installed release, Intel,
  Linux, signing, default ONNX or editor/platform qualification.

'''
        replace('**Earlier attempts remain failed evidence.**', release_evidence + '**Earlier attempts remain failed evidence.**')
        replace('This is bounded development evidence, not release/installer/Genesis, ONNX/default',
                'This is bounded development evidence, not a published release/installer/Genesis, ONNX/default')
        replace('Release-runtime, source-bound Neo results and mandatory `cargo clean`, final\nprimary review and a signed phase checkpoint are still pending. P13 remains',
                'Release-runtime, source-bound scoped Neo qualification, mandatory `cargo clean`\nand primary review passed. The authorized signed commit remains a separate\nGit action; no human sign-off is invented here. P13 remains')
        assert historical in text, 'Historical P12 failures changed'
        if earlier is not None:
            assert earlier in text, 'Prior dated phase history changed'
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply-after-primary-review', action='store_true', required=True)
    args = parser.parse_args()
    assert args.apply_after_primary_review
    output = BASE / 'p12-docs-accepted-r1'
    assert not output.exists(), 'Fresh output label required; do not replay updates'
    proofs = validate_proofs()
    manifest = read(BASE / 'p12-docs-change-r1/manifest.json')
    assert len(manifest) == 9
    prepared = {}
    for name, expected in manifest.items():
        path = WORK / name
        assert sha(path) == expected['updated_sha256'], ('Reviewed document changed', name)
        raw = path.read_bytes()
        text = raw.decode('utf-8').replace('\r\n', '\n')
        updated = render(name, text)
        assert updated != text and '#explicit-reference-refresh-p12-candidate' not in updated
        prepared[name] = (raw, updated.replace('\n', '\r\n').encode('utf-8') if b'\r\n' in raw else updated.encode('utf-8'))
    # Every proof and all nine baselines are checked before any document write.
    output.mkdir()
    result = {'source': SOURCE, 'proofs': proofs, 'primary_review_asserted_by_operator': True,
              'commit_or_publication_performed': False, 'files': {}}
    for name, (before, after) in prepared.items():
        assert (WORK / name).read_bytes() == before, ('Concurrent change', name)
        backup = output / 'before' / name
        backup.parent.mkdir(parents=True, exist_ok=True)
        backup.write_bytes(before)
        (WORK / name).write_bytes(after)
        result['files'][name] = {'before': hashlib.sha256(before).hexdigest(),
                                'after': hashlib.sha256(after).hexdigest()}
    builder = load('p12_accept_final_builder', WORK / 'scripts/build-service-runtime.py')
    assert builder.source_identity() == SOURCE
    (output / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'updated': 9, 'native_source_unchanged': SOURCE, 'receipt': str(output / 'receipt.json')}))


if __name__ == '__main__':
    main()
