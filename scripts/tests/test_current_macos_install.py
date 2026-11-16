# SPDX-License-Identifier: Apache-2.0
"""Mock/pure current Mac I4 driver guards; never launches a Mac product."""
import contextlib
import copy
import errno
import importlib.util
import io
import json
from pathlib import Path
import stat
import subprocess
import tempfile
import tomllib
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, call, patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("current_mac_install_test", ROOT / "scripts/check-current-macos-install.py")
DRIVER = importlib.util.module_from_spec(spec)
spec.loader.exec_module(DRIVER)


class CurrentMacInstallTests(unittest.TestCase):
    def options(self, root):
        return SimpleNamespace(target="aarch64-apple-darwin", runtime_dir=root / "source-pair",
            signed_dir=root / "signed", installer=root / "candidate.dmg", manifest=root / "installer.json",
            approved_installer_sha256="a" * 64, approved_manifest_sha256="b" * 64)

    def test_human_identity_refuses_wrong_host_root_and_other_login(self):
        DRIVER.verify_human("Darwin", 501, 501)
        for args in (("Windows", 501, 501), ("Darwin", 0, 0), ("Darwin", 501, 502)):
            with self.subTest(args=args), self.assertRaisesRegex(ValueError, "logged-in"):
                DRIVER.verify_human(*args)

    def test_native_pointer_and_bundle_paths_use_current_i4(self):
        home = Path("/Users/human")
        self.assertEqual(DRIVER.pointer_path(home), home / "Library/Application Support/AIRK/install-pointer/current.json")
        self.assertEqual(DRIVER.IMAGE, "AIRK.app/Contents/MacOS/airk")
        self.assertEqual(DRIVER.CLI, "AIRK.app/Contents/MacOS/airkctl")
        with self.assertRaises(ValueError):
            DRIVER.version_tuple("1.08.207")

    def test_occupied_authority_refuses_before_any_process_or_wrapper_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            authority = home / "Library/Application Support/AIRK/install-pointer"
            authority.mkdir(parents=True)
            original = authority / "active-setup.json"; original.write_bytes(b"retained unknown owner")
            commands = Mock()
            with self.assertRaisesRegex(ValueError, "occupied"):
                DRIVER.preflight(home, 501, commands)
            commands.run.assert_not_called()
            self.assertEqual(original.read_bytes(), b"retained unknown owner")

    def test_process_or_unverifiable_launch_job_preserves_predecessor(self):
        for first, second in (("9 501 /foreign/AIRK.app/Contents/MacOS/airk", ""),
                               ("", "12 0 io.iokaio.airk.service.preview")):
            with tempfile.TemporaryDirectory() as temporary:
                commands = Mock(); commands.report = {}
                commands.run.side_effect = [SimpleNamespace(stdout=first), SimpleNamespace(stdout=second)]
                with self.assertRaisesRegex(ValueError, "preserved"):
                    DRIVER.preflight(Path(temporary), 501, commands)
                self.assertLessEqual(commands.run.call_count, 2)
        with tempfile.TemporaryDirectory() as temporary:
            commands = Mock(); commands.report = {}
            commands.run.side_effect = [SimpleNamespace(stdout=""), SimpleNamespace(stdout=""), SimpleNamespace(stderr="domain unavailable")]
            with self.assertRaisesRegex(ValueError, "absence unverified"):
                DRIVER.preflight(Path(temporary), 501, commands)

    def test_literal_cached_tools_are_not_airk_images_even_below_airk_build_and_source_folders(self):
        text="""  100 501 /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13
  101 501 /Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059/tools/cmake-3.31.6-macos-universal/CMake.app/Contents/bin/cmake
  102 501 /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin/node
  103 501 /Users/Some Human/airk-builds/source with spaces/third party/python3.13
  104 501 /Users/Some Human/code/airk/source with spaces/airk-source-tool
  105 501 /Users/Some Human/AIRK.app tools/source/cmake
"""
        census=DRIVER.process_census(text,501)
        self.assertEqual(census['same_user_airk'],[])
        self.assertEqual(census['other_user_airk'],[])
        self.assertEqual(census['scanned_rows'],6)

    def test_actual_native_and_legacy_images_use_only_basename_and_preserve_internal_spaces(self):
        text="""201 501 /Users/Some Human/Library/Application Support/AIRK/program/AIRK.app/Contents/MacOS/airk
202 501 /Users/Some Human/airk-builds/source with spaces/AIRK.app/Contents/MacOS/airkctl
203 501 /Users/Some Human/old application/airk-embed
204 501 /Users/Some Human/old application/airk-launcher-image
205 501 /Users/Some Human/old application/airk-launcher
206 501 /Users/Some Human/old application/airk-service
207 501 /Users/Some Human/old application/airk-index
208 501 /Users/Some Human/old application/airk-source
"""
        census=DRIVER.process_census(text,501)
        self.assertEqual([row['pid'] for row in census['same_user_airk']],list(range(201,209)))
        self.assertEqual(census['same_user_airk'][0]['executable'],
            '/Users/Some Human/Library/Application Support/AIRK/program/AIRK.app/Contents/MacOS/airk')
        self.assertEqual(census['other_user_airk'],[])

    def test_other_uid_airk_rows_are_preserved_without_blocking_or_claiming_cleanup(self):
        text=""" 301 502 /Users/Other Human/Library/Application Support/AIRK/program/AIRK.app/Contents/MacOS/airk
302 0 /usr/libexec/some-system-process
303 501 /Users/tylerjensen/airk-builds/python/bin/python3.13
"""
        census=DRIVER.process_census(text,501)
        self.assertEqual(census['same_user_airk'],[])
        self.assertEqual(census['other_user_rows'],2)
        self.assertEqual(census['other_user_airk'],[{'pid':301,'uid':502,
            'executable':'/Users/Other Human/Library/Application Support/AIRK/program/AIRK.app/Contents/MacOS/airk'}])
        self.assertIn('no cleanup authority',census['scope'])

    def test_unknown_pid_uid_census_never_establishes_absence(self):
        for text in ('pid uid comm','301 other /Applications/AIRK.app/Contents/MacOS/airk',
                     '301 501','? 501 /usr/bin/python3'):
            with self.subTest(text=text),self.assertRaisesRegex(ValueError,'census row is unverified'):
                DRIVER.process_census(text,501)

    def test_preflight_accepts_cached_python_and_records_foreign_airk_without_product_actions(self):
        with tempfile.TemporaryDirectory() as temporary:
            commands=Mock();commands.report={}
            text='100 501 /Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13\n101 502 /Users/Other Human/AIRK.app/Contents/MacOS/airk\n'
            commands.run.side_effect=[SimpleNamespace(stdout=text),SimpleNamespace(stdout=''),
                SimpleNamespace(stderr='Could not find service "io.iokaio.airk.service.stable"'),
                SimpleNamespace(stderr='Could not find service "io.iokaio.airk.service.preview"')]
            DRIVER.preflight(Path(temporary),501,commands)
            self.assertEqual(commands.run.call_count,4)
            self.assertEqual(commands.report['preflight_processes']['same_user_airk'],[])
            self.assertEqual(commands.report['preflight_processes']['other_user_airk'][0]['uid'],502)
            self.assertEqual(commands.run.call_args_list[0].args[0],['/bin/ps','-axo','pid=,uid=,comm='])

    def test_artifact_digest_refuses_before_any_signature_or_app_probe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); args = self.options(root)
            args.runtime_dir.mkdir(); args.signed_dir.mkdir()
            args.installer.write_bytes(b"not a DMG; test must refuse this")
            args.manifest.write_bytes(b"{}")
            helpers = [Mock(), Mock(), Mock()]
            with patch.object(DRIVER, "module", side_effect=helpers), self.assertRaisesRegex(ValueError, "approved"):
                DRIVER.candidate(args, "", Mock(), current=True)
            helpers[2].read_verified_pair.assert_not_called()
            helpers[0].verify_image_signature.assert_not_called()

    def candidate_fixture(self, root):
        args = self.options(root)
        args.runtime_dir.mkdir(); args.signed_dir.mkdir()
        placed = args.signed_dir / "runtime"; placed.mkdir()
        (placed / "airk-install.json").write_bytes(b"mocked canonical validator input only")
        (args.signed_dir / "signing.json").write_bytes(b"mocked signing owner input only")
        args.installer.write_bytes(b"mock artifact; only mocked owner admits this test")
        args.approved_installer_sha256 = DRIVER.digest(args.installer)
        source = {"digest": "reviewed source"}
        signing = {"components": {"mock": {"notarization": "owner verified"}}}
        claim = {"team_id": "5PX7D99PXA", "identity": "f" * 40,
                 "notarization": {"status": "Accepted", "stapled": True, "scope": args.installer.name, "id": "native-notary-fixture"}}
        manifest = {"schema": "airk-standalone-installer/1", "version": "1.8.207", "target": args.target,
            "installer_kind": "macos_dmg", "artifact": args.installer.name, "artifact_sha256": args.approved_installer_sha256,
            "runtime_install_sha256": DRIVER.digest(placed / "airk-install.json"), "runtime_source": source,
            "commit": "c" * 40, "cargo_lock_sha256": DRIVER.digest(ROOT / "Cargo.lock"),
            "runtime_signing_sha256": DRIVER.digest(args.signed_dir / "signing.json"), "runtime_signing": signing,
            "payload_schema": "airk-service-payload/1", "signing": claim}
        files = {DRIVER.IMAGE: "a" * 64, DRIVER.CLI: "b" * 64}
        mac = SimpleNamespace(TEAM="5PX7D99PXA", verify_image_signature=Mock(return_value={"team_id": "5PX7D99PXA"}),
            module=Mock(return_value=SimpleNamespace(layout_names=Mock(return_value={name:name for name in files}))))
        package = SimpleNamespace(bounded_json=Mock(side_effect=lambda p,*_:manifest if p==args.manifest else {"schema":3,"source":source}),
            runtime_builder=Mock(return_value=SimpleNamespace(source_identity=Mock(return_value=source))),
            validate_runtime=Mock(return_value=(files,(DRIVER.IMAGE,DRIVER.CLI))),capture=Mock(return_value="c" * 40),
            tomllib=SimpleNamespace(loads=Mock(return_value={"workspace":{"package":{"version":"1.8.207"}}})))
        signer = SimpleNamespace(read_verified_pair=Mock(return_value=(placed,signing)),stapled_app_ticket=Mock(return_value=True))
        def seal():
            args.manifest.write_text(json.dumps(manifest),encoding="utf-8")
            args.approved_manifest_sha256 = DRIVER.digest(args.manifest)
        seal()
        return args,manifest,mac,package,signer,seal

    def test_candidate_delegates_full_source_pair_signatures_and_notary_to_owners(self):
        with tempfile.TemporaryDirectory() as temporary:
            args,manifest,mac,package,signer,_ = self.candidate_fixture(Path(temporary))
            commands=Mock()
            with patch.object(DRIVER,"module",side_effect=[mac,package,signer]):
                result=DRIVER.candidate(args,"",commands,current=True)
            self.assertEqual(package.validate_runtime.call_count,2)
            signer.read_verified_pair.assert_called_once_with(args.signed_dir,args.runtime_dir,args.target,"c"*40,manifest['cargo_lock_sha256'])
            signer.stapled_app_ticket.assert_called_once_with(manifest['runtime_signing']['components'])
            mac.verify_image_signature.assert_called_once()
            self.assertEqual(commands.run.call_count,3)
            self.assertEqual(result['version'],'1.8.207')
            self.assertIn(str(args.manifest),result['input_hashes'])

    def test_approved_manifest_cannot_substitute_source_signing_or_notarization(self):
        changes=[('runtime_source',{'foreign':'source'}),('runtime_signing',{'components':{}}),
                 ('runtime_install_sha256','d'*64),('payload_schema','historical'),('artifact','other.dmg')]
        for key,value in changes:
            with self.subTest(key=key),tempfile.TemporaryDirectory() as temporary:
                args,manifest,mac,package,signer,seal=self.candidate_fixture(Path(temporary))
                manifest[key]=value;seal()
                with patch.object(DRIVER,'module',side_effect=[mac,package,signer]),self.assertRaisesRegex(ValueError,'provenance'):
                    DRIVER.candidate(args,'',Mock(),current=True)
                mac.verify_image_signature.assert_not_called()
        with tempfile.TemporaryDirectory() as temporary:
            args,manifest,mac,package,signer,seal=self.candidate_fixture(Path(temporary))
            manifest['signing']['notarization']['stapled']=False;seal()
            with patch.object(DRIVER,'module',side_effect=[mac,package,signer]),self.assertRaisesRegex(ValueError,'notarized'):
                DRIVER.candidate(args,'',Mock(),current=True)

    def test_mount_roster_drift_is_retained_before_wrapper_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            out=Path(temporary);commands=Mock();commands.report={'mounts':[]}
            value={'version':'1.8.207','image':out/'input.dmg','manifest':{'payload':{'expected':'a'*64}},
                   'package':SimpleNamespace(inventory=Mock(return_value={'extra':'b'*64}))}
            with self.assertRaisesRegex(ValueError,'roster'):
                DRIVER.mount_candidate(value,out,commands)
            self.assertFalse(commands.report['mounts'][0]['detached'])
            self.assertEqual(commands.run.call_count,1)
            self.assertIn('-readonly',commands.run.call_args.args[0])

    def test_wrapper_recovery_uses_original_without_options_or_reset_acknowledgement(self):
        value={'mount':Path('/owned mounted image')}
        locations={name:Path('/owned')/name for name in ('program','data','state','cache','logs')}
        initial=DRIVER.wrapper_command(value,locations)
        self.assertIn('--silent',initial)
        self.assertNotIn('--acknowledge-reset',initial)
        self.assertNotIn('--no-start',initial)
        original=Path('/owned original/operation-request.json')
        resumed=DRIVER.wrapper_command(value,locations,original=original)
        self.assertEqual(resumed,['/bin/bash',str(value['mount']/'Install AIRK.command'),'--resume',str(original),'--silent'])

    def test_setup_success_requires_single_native_complete_and_no_retirement(self):
        good={'schema':'airk-setup-result/1','status':'complete','retired':0,'leftovers':[]}
        DRIVER.setup_result('ordinary disclosure\n'+json.dumps(good))
        for output in ('{}',json.dumps(dict(good,status='awaiting_placement')),json.dumps(dict(good,retired=1)),
                       json.dumps(dict(good,leftovers=[{'item':'foreign'}])),json.dumps(good)+'\n'+json.dumps(good)):
            with self.subTest(output=output),self.assertRaises(ValueError):DRIVER.setup_result(output)

    def test_upgrade_must_preserve_original_data_never_recreate_a_deleted_sentinel(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'original-data.txt'
            expected=DRIVER.data_sentinel(path)
            DRIVER.data_sentinel(path,expected)
            with self.assertRaises(FileExistsError):DRIVER.data_sentinel(path)
            path.write_bytes(b'changed during upgrade')
            with self.assertRaisesRegex(ValueError,'original data changed'):DRIVER.data_sentinel(path,expected)
            path.unlink()
            with self.assertRaises(FileNotFoundError):DRIVER.data_sentinel(path,expected)
            self.assertFalse(path.exists())

    def test_original_request_requires_real_nested_wrapper_custody_and_private_owner(self):
        home=Path(tempfile.gettempdir()).resolve()
        path=home/'.airk-setup-recovery.abcdef1234/operation-2/operation-request.json'
        with patch.object(DRIVER,'regular',return_value=SimpleNamespace(st_uid=501,st_mode=stat.S_IFREG|0o600)), \
             patch.object(DRIVER.os,'getuid',return_value=501,create=True):
            self.assertEqual(DRIVER.original_request('Original request: '+str(path)+'\n',home),path)
        for text in ('', 'Original request: /foreign/request.json\n',
                     'Original request: '+str(path)+'\nOriginal request: '+str(path)):
            with self.subTest(text=text),self.assertRaises(ValueError):DRIVER.original_request(text,home)
        with patch.object(DRIVER,'regular',return_value=SimpleNamespace(st_uid=502,st_mode=stat.S_IFREG|0o600)), \
             patch.object(DRIVER.os,'getuid',return_value=501,create=True),self.assertRaisesRegex(ValueError,'owner-private'):
            DRIVER.original_request('Original request: '+str(path),home)

    def test_installed_version_cannot_accept_unverified_or_mismatched_pair(self):
        value={'version':'1.8.207'}
        good={'version':'1.8.207','executable_sha256':'a'*64,'installation':{
            'state':'complete','pair':'exact','airk_version':'1.8.207','install_id':'b'*32}}
        self.assertEqual(DRIVER.require_installation(good,value,'a'*64),'b'*32)
        for key,value2 in (('state','unverified'),('pair','mismatch'),('airk_version','1.8.206'),('install_id','')):
            changed=copy.deepcopy(good);changed['installation'][key]=value2
            with self.subTest(key=key),self.assertRaises(ValueError):DRIVER.require_installation(changed,value,'a'*64)

    def test_health_binds_settled_native_user_version_image_and_actual_pid_path(self):
        value={'version':'1.8.207','files':{DRIVER.IMAGE:'a'*64}}
        image=Path('/selected/AIRK.app/Contents/MacOS/airk')
        health={'version':'1.8.207','user':'501','executable_sha256':'a'*64,'instance':'b'*32,'process_id':123,'build_error':None}
        status={'service':{'state':'running','pending_lifecycle':None,'explicitly_stopped':False,'health':health}}
        self.assertEqual(DRIVER.require_health(status,value,501,image,lambda pid:image),health)
        for key,bad in (('user','502'),('version','1.8.206'),('executable_sha256','c'*64),('process_id',0),('build_error','bad build')):
            changed=copy.deepcopy(status);changed['service']['health'][key]=bad
            with self.subTest(key=key),self.assertRaises(ValueError):DRIVER.require_health(changed,value,501,image,lambda pid:image)
        for changed in (dict(status,service=dict(status['service'],pending_lifecycle={'phase':'draining'})),
                        dict(status,service=dict(status['service'],explicitly_stopped=True))):
            with self.assertRaises(ValueError):DRIVER.require_health(changed,value,501,image,lambda pid:image)
        with self.assertRaisesRegex(ValueError,'actual native'):
            DRIVER.require_health(status,value,501,image,lambda pid:Path('/foreign/airk'))

    def test_native_login_envelope_refuses_unsupported_enabled_or_unknown_state(self):
        clear={'op':'login_startup','login':{'supported':True,'registered':False}}
        DRIVER.require_headless_login(clear)
        for response in ({'supported':True,'registered':False},
                         {'op':'other','login':clear['login']},
                         {'op':'login_startup','login':{'supported':False,'registered':False}},
                         {'op':'login_startup','login':{'supported':True,'registered':True}},
                         {'op':'login_startup','login':{'supported':True,'registered':False,'detail':'requires approval'}}):
            with self.subTest(response=response),self.assertRaisesRegex(ValueError,'enabled/unknown'):
                DRIVER.require_headless_login(response)

    def test_event_observation_keeps_one_deadline_and_never_replays_mutation(self):
        reads=Mock(side_effect=[None,{'closed':False},{'closed':True}]);observer=Mock()
        result=DRIVER.observe_until(reads,lambda row:row is not None and row['closed'],observer,50,clock=lambda:1)
        self.assertTrue(result['closed'])
        self.assertEqual(observer.wait.call_args_list,[call(50),call(50)])
        expired=Mock(side_effect=[1,11]);read=Mock(return_value=None)
        with self.assertRaisesRegex(ValueError,'deadline'):
            DRIVER.observe_until(read,bool,observer,10,clock=expired)
        read.assert_called_once()

    def test_kernel_wait_reports_original_remaining_time_and_unknown_event_errors(self):
        observer=object.__new__(DRIVER.Events);observer.clock=lambda:7;observer.queue=Mock()
        observer.queue.control.return_value=[SimpleNamespace(flags=0)]
        with patch.object(DRIVER.select,'KQ_EV_ERROR',0x4000,create=True):
            observer.wait(12)
            observer.queue.control.assert_called_once_with(None,32,5)
            observer.queue.control.return_value=[]
            with self.assertRaisesRegex(ValueError,'unavailable/deadline'):observer.wait(12)
            observer.queue.control.return_value=[SimpleNamespace(flags=0x4000)]
            with self.assertRaisesRegex(ValueError,'unavailable/deadline'):observer.wait(12)
        observer.clock=lambda:13
        with self.assertRaisesRegex(ValueError,'original observation deadline'):observer.wait(12)

    def test_ready_read_at_or_after_original_deadline_is_refused(self):
        for ended in (10,10.001):
            with self.subTest(ended=ended):
                read=Mock(return_value={'closed':True});observer=Mock()
                clock=Mock(side_effect=[9,ended])
                with self.assertRaisesRegex(ValueError,'deadline expired after read'):
                    DRIVER.observe_until(read,lambda row:row['closed'],observer,10,clock)
                read.assert_called_once();observer.wait.assert_not_called()
        observer=Mock()
        self.assertEqual(DRIVER.observe_until(lambda:'ready',bool,observer,10,
            clock=Mock(side_effect=[9,9.999])),'ready')
        observer.wait.assert_not_called()

    def test_kernel_result_at_or_after_original_deadline_is_refused(self):
        for ended in (10,10.001):
            with self.subTest(ended=ended):
                observer=object.__new__(DRIVER.Events)
                observer.clock=Mock(side_effect=[9,ended]);observer.queue=Mock()
                observer.queue.control.return_value=[SimpleNamespace(flags=0)]
                with patch.object(DRIVER.select,'KQ_EV_ERROR',0x4000,create=True), \
                     self.assertRaisesRegex(ValueError,'original kernel observation deadline expired'):
                    observer.wait(10)
                observer.queue.control.assert_called_once_with(None,32,1)

    def test_process_exit_during_kqueue_registration_requires_esrch_and_exact_pid_absence(self):
        image=Path('/owned/AIRK.app/Contents/MacOS/airk')
        factory=Mock(side_effect=ProcessLookupError(errno.ESRCH,'child exited during registration'))
        observe=Mock(side_effect=[image,None])
        self.assertEqual(DRIVER.observe_child_exit(124,image,50,factory,observe,clock=lambda:1),
                         'original_pid_exit_reobserved')
        factory.assert_called_once_with(pid=124)
        self.assertEqual(observe.call_args_list,[call(124),call(124)])
        for still_present in (image,Path('/foreign/reused-pid')):
            with self.subTest(still_present=still_present),self.assertRaisesRegex(ValueError,'child exit unverified'):
                DRIVER.observe_child_exit(124,image,50,factory,Mock(side_effect=[image,still_present]),clock=lambda:1)

    def test_process_registration_other_errors_and_late_absence_remain_unknown(self):
        image=Path('/owned/AIRK.app/Contents/MacOS/airk')
        for number in (errno.EPERM,errno.EACCES,errno.EINVAL):
            factory=Mock(side_effect=OSError(number,'unverifiable kernel registration'))
            observe=Mock(return_value=image)
            with self.subTest(number=number),self.assertRaises(OSError) as caught:
                DRIVER.observe_child_exit(124,image,50,factory,observe,clock=lambda:1)
            self.assertEqual(caught.exception.errno,number)
            observe.assert_called_once_with(124)
        for ended in (50,50.001):
            factory=Mock(side_effect=ProcessLookupError(errno.ESRCH,'gone'))
            observe=Mock(side_effect=[image,None])
            with self.subTest(ended=ended),self.assertRaisesRegex(ValueError,'deadline expired after final observation'):
                DRIVER.observe_child_exit(124,image,50,factory,observe,
                    clock=Mock(side_effect=[1,2,3,ended]))
            self.assertEqual(observe.call_args_list,[call(124),call(124)])

    def test_commands_retain_unknown_timeout_and_partial_streams_without_retry(self):
        with tempfile.TemporaryDirectory() as temporary:
            out=Path(temporary);report={'commands':[]};commands=DRIVER.Commands(out,report)
            failure=subprocess.TimeoutExpired(['native'],125,output=b'original stdout',stderr=b'original diagnostic')
            with patch.object(DRIVER.subprocess,'run',side_effect=failure) as run,self.assertRaises(subprocess.TimeoutExpired):
                commands.run(['native','mutation'])
            run.assert_called_once()
            retained=json.loads((out/'receipt.json').read_text())
            self.assertEqual(retained['commands'][0]['status'],'outcome_unknown')
            self.assertIsNone(retained['commands'][0]['exit_code'])
            self.assertEqual((out/'command-001.stderr.log').read_bytes(),b'original diagnostic')

    def test_native_refusal_retains_complete_error_and_does_not_retry(self):
        with tempfile.TemporaryDirectory() as temporary:
            out=Path(temporary);commands=DRIVER.Commands(out,{'commands':[]})
            result=subprocess.CompletedProcess(['native'],6,'',json.dumps({'class':'outcome_unknown','code':'uninstall_outcome_unknown'}))
            with patch.object(DRIVER.subprocess,'run',return_value=result) as run,self.assertRaisesRegex(ValueError,'refused'):
                commands.run(['native','service','uninstall','--yes'])
            run.assert_called_once()
            self.assertEqual((out/'command-001.stderr.log').read_text(),result.stderr)
            self.assertEqual(json.loads((out/'receipt.json').read_text())['commands'][0]['exit_code'],6)

    def journey(self,out):
        commands=Mock();commands.report={'commands':[],'installed':[],'login_observations':[],'recovery':[]}
        observer=Mock();factory=Mock(return_value=contextlib.nullcontext(observer))
        journey=DRIVER.Journey(out/'home',501,out,commands,events=factory,image_observer=Mock(return_value=None),clock=lambda:10)
        return journey,commands,observer,factory

    def test_lifecycle_requires_original_exit_event_and_fresh_incarnation(self):
        with tempfile.TemporaryDirectory() as temporary:
            journey,commands,observer,factory=self.journey(Path(temporary))
            old={'instance':'a'*32,'process_id':123,'executable_sha256':'c'*64};new={'instance':'b'*32,'process_id':124}
            journey.health=Mock(side_effect=[old,new])
            stop={'record':{'request':{'expected_instance':'a'*32,'action':'stop','runtime':'c'*64}},'service_instance':'a'*32,'process_id':123}
            journey.cli=Mock(side_effect=[stop,{'service':{'state':'not_running'}},{'observation':new}])
            self.assertEqual(journey.lifecycle({}),new)
            factory.assert_called_once_with(pid=123);observer.wait.assert_called_once_with(135)
            self.assertEqual(journey.cli.call_args_list,[call('service','stop',timeout=125),call('service','status'),call('service','start')])
            self.assertTrue(commands.report['lifecycle']['exact_process_exit_event'])

    def test_unobserved_stop_never_restarts_or_claims_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            journey,commands,observer,_=self.journey(Path(temporary))
            journey.health=Mock(return_value={'instance':'a'*32,'process_id':123,'executable_sha256':'c'*64})
            journey.cli=Mock(return_value={'record':{'request':{'expected_instance':'a'*32,'action':'stop','runtime':'c'*64}},'service_instance':'a'*32,'process_id':123})
            observer.wait.side_effect=ValueError('original deadline expired')
            with self.assertRaisesRegex(ValueError,'deadline'):journey.lifecycle({})
            journey.cli.assert_called_once_with('service','stop',timeout=125)
            self.assertNotIn('lifecycle',commands.report)

    def test_original_reconciliation_never_changes_operation_or_claims_interrupted_recovery(self):
        with tempfile.TemporaryDirectory() as temporary:
            out=Path(temporary);journey,commands,_,_=self.journey(out)
            value={'version':'1.8.207','mount':out/'mounted'}
            result={'schema':'airk-setup-result/1','status':'complete','retired':0,'leftovers':[],'operation_id':'a'*32}
            commands.run.side_effect=[SimpleNamespace(stdout=json.dumps(result),stderr='original request'),SimpleNamespace(stdout='reconciled',stderr='')]
            observation={'operation_id':'a'*32,'receipt':{'closed':True}}
            journey.installed=Mock(side_effect=[copy.deepcopy(observation),copy.deepcopy(observation)])
            journey.health=Mock(return_value={'instance':'b'*32,'process_id':123})
            journey.cli=Mock(return_value={'op':'login_startup','login':{'supported':True,'registered':False}})
            original=out/'original-request.json';original.write_bytes(b'retained original')
            with patch.object(DRIVER,'original_request',return_value=original):journey.install(value)
            self.assertEqual(commands.run.call_count,2)
            self.assertEqual(commands.run.call_args.args[0],DRIVER.wrapper_command(value,journey.locations,original=original))
            self.assertIn('interrupted setup recovery unexercised',commands.report['recovery'][0]['scope'])
            self.assertEqual(original.read_bytes(),b'retained original')

    def test_unknown_wrapper_admission_stops_before_status_recovery_or_uninstall(self):
        with tempfile.TemporaryDirectory() as temporary:
            out=Path(temporary);journey,commands,_,_=self.journey(out)
            commands.run.side_effect=ValueError('native admission outcome unknown')
            journey.installed=Mock();journey.cli=Mock()
            with self.assertRaisesRegex(ValueError,'unknown'):journey.install({'mount':out/'mounted'})
            commands.run.assert_called_once();journey.installed.assert_not_called();journey.cli.assert_not_called()

    def test_removal_needs_native_original_completion_not_just_pointer_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            out=Path(temporary);journey,commands,_,_=self.journey(out)
            operation='a'*32;folder=out/'temporary'/('airk-setup-'+operation);recovery=folder/'recovery-program'
            image=recovery/DRIVER.IMAGE;image.parent.mkdir(parents=True);image.write_bytes(b'mocked exact approved image')
            receipt_path=out/'state/setup'/(operation+'.json');receipt_path.parent.mkdir(parents=True)
            receipt={'closed':True,'operation_id':operation,'failure':None,'retired':[],'leftovers':[]}
            receipt_path.write_text(json.dumps(receipt))
            accepted={'started':True,'kind':'uninstall','delete_indexes':'keep','operation_id':operation,
                      'receipt':str(receipt_path),'operation':str(folder/'operation.json'),
                      'recovery_program':str(recovery),'process_id':124}
            journey.health=Mock(return_value={'instance':'b'*32,'process_id':123});journey.cli=Mock(return_value=accepted)
            package=SimpleNamespace(bounded_json=lambda p,*_:json.loads(p.read_text()))
            value={'package':package,'files':{DRIVER.IMAGE:DRIVER.digest(image)}}
            commands.run.return_value=SimpleNamespace(stdout='true')
            with self.assertRaisesRegex(ValueError,'completion remains unknown'):journey.remove(value)
            self.assertNotIn('removal',commands.report)
            commands.run.return_value=SimpleNamespace(stdout='false')
            journey.remove(value)
            self.assertTrue(commands.report['removal']['observed_removal_child_absent'])
            self.assertEqual(journey.cli.call_args_list,[call('service','uninstall','--yes',timeout=240)]*2)
            self.assertEqual(commands.run.call_args.args[0][-2:],['--recovery-field','completion-required'])

    def test_partial_previous_inputs_never_claim_an_upgrade(self):
        basic=['--out','fresh','--target','aarch64-apple-darwin','--allow-install',
               '--runtime-dir','pair','--signed-dir','signed','--installer','current.dmg','--manifest','installer.json',
               '--approved-installer-sha256','a'*64,'--approved-manifest-sha256','b'*64]
        self.assertIsNone(DRIVER.parse(basic).previous_installer)
        with contextlib.redirect_stderr(io.StringIO()),self.assertRaises(SystemExit):
            DRIVER.parse(basic+['--previous-installer','old.dmg'])


if __name__=='__main__':
    unittest.main()
