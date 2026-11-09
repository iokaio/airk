# SPDX-License-Identifier: Apache-2.0
"""Mocked native editor fixture producer, never starts a runtime or editor."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import os
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("prepare_editor_legacy", Path(__file__).parents[1]/"prepare-native-editor-legacy.py")
producer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(producer)


class NativeEditorFixtureTests(unittest.TestCase):
    def journey(self, fail_read=False, crossed_stop=False, healthy_after=False):
        temporary=tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root=Path(temporary.name).resolve()
        (root/"artifacts").mkdir()
        owner=root/"artifacts"/"fixture"
        def payload(name,schema):
            directory=root/name;directory.mkdir()
            (directory/"airk.exe").write_bytes(name.encode())
            metadata={"schema":schema,"version":"1.8.14" if schema==2 else "1.8.8","os":"windows","arch":"x86_64","airk_sha256":producer.sha(directory/"airk.exe")}
            if schema==1:
                (directory/"airk-embed.exe").write_bytes(b"old helper")
                metadata["embed_sha256"]=producer.sha(directory/"airk-embed.exe")
            (directory/"airk-runtime.json").write_text(json.dumps(metadata)+"\n")
            manifest={"files":{path.name:producer.sha(path) for path in directory.iterdir()}}
            (directory/"airk-install.json").write_text(json.dumps(manifest)+"\n")
            return directory,producer.sha(directory/"airk-install.json")
        old,old_id=payload("old",1);new,new_id=payload("new",2)
        state={"active":None,"stops":[],"manual_reads":0}
        class Fixture:
            def __init__(self,runtime,folder):
                self.binary=runtime/"airk.exe";self.projects={};self.log=io.StringIO();self.environment={}
                self.compatibility={"topology":"paired","authority":"legacy","original_scope":"service_project","migration_path":"audited_imported_legacy"}
            def register(self,name,source,data):
                self.projects[name]={"binding":"9"*64,"root":str(source),"data_root":str(data),"collection":name}
            def job(self,name,kind):return {"state":"succeeded","kind":kind}
            def arguments(self,name):
                registration=self.projects[name]
                return ["--service-runtime-dir",str(self.control),"--service-project",registration["binding"],"--root",registration["root"],"--data-dir",registration["data_root"],"--collection",name]
            def rpc(self,method):
                if method=="health":
                    runtime=new if state["active"]==new_id else old
                    return {"status":"health","observation":{"runtime_sha256":state["active"],"executable_sha256":producer.sha(runtime/"airk.exe"),"instance":"1"*32}}
                if method=="request_epoch":return {"namespace":"2"*32,"epoch":1}
                return {}
        def command(folder,records,binary,arguments,**kwargs):
            action=arguments[1]
            if arguments[:2]==("service","health"):
                self.assertEqual(len(state["stops"]),1)
                return SimpleNamespace(returncode=0 if healthy_after else 1,stdout="",stderr="owned listener absent")
            install=owner/"install"
            if action=="stage":
                runtime=old if binary==old/"airk.exe" else new
                identity=old_id if runtime==old else new_id
                shutil.copytree(runtime,install/"runtimes"/identity)
                (owner/"state"/"control").mkdir(parents=True,exist_ok=True)
                value={}
            elif action=="activate":
                state["active"]=arguments[arguments.index("--runtime-sha256")+1]
                if state["active"]==old_id:
                    launcher=install/"airk-launcher.exe";launcher.write_bytes(b"owned launcher")
                    (install/"launcher.json").write_text(json.dumps({"schema":1,"sha256":producer.sha(launcher)}))
                else:
                    (install/"activation-result.json").write_text(json.dumps({"candidate":new_id,"preceding":old_id}))
                    (owner/"state/control/index-catalog-upgrade.json").write_text(json.dumps({"authority_v1":{"phase":"settled"}}))
                value={"active":state["active"]}
            elif action=="inspect":
                value={"selection":{"active":state["active"]},"approval":{"manifest_sha256":state["active"]},"payload_verified":True,
                    "launcher":str(install/"airk-launcher.exe") if state["active"]==old_id else None,
                    "executable":str(install/"runtimes"/state["active"]/"airk.exe")}
            elif action=="lifecycle":
                request=json.loads(kwargs["input"]);state["stops"].append(request)
                self.assertEqual(request["runtime"],state["active"])
                expected={key:value for key,value in request.items() if key!="force"};expected["manager"]=False
                if crossed_stop:expected["runtime"]="crossed"
                value={"status":"lifecycle","record":{"phase":"complete","request":expected}}
            else:raise AssertionError(arguments)
            records.append({"arguments":arguments})
            return SimpleNamespace(returncode=0,stdout=json.dumps(value),stderr="")
        def retire(invoke,binary,install,candidate,previous):
            self.assertEqual((binary,candidate,previous),(new,new_id,old_id))
            for name in ("airk-launcher.exe","launcher.json"):(install/name).unlink()
            shutil.rmtree(install/"runtimes"/old_id)
            return {"launchers":{"active":candidate},"runtimes":{"active":candidate}}
        class Session:
            def __init__(self,configuration,timeout):
                self.configuration=configuration
                assert Path(configuration["command"]).is_file()
            def __enter__(self):return self
            def __exit__(self,*_):return False
            def tool(self,task):
                state["manual_reads"]+=1
                if fail_read:raise RuntimeError("original read failed")
                body={"items":[{"citation":{"content_hash":producer.sha(owner/"source/fixture.ts")}}]}
                return {"content":[{"text":json.dumps(body)}]}, {"tool":task[0]}
        modules={"check-service-clients.py":SimpleNamespace(Fixture=Fixture,runtime_compatibility=lambda _: {"topology":"paired","authority":"legacy","original_scope":"service_project","migration_path":"audited_imported_legacy"},tool_value=lambda envelope:json.loads(envelope["content"][0]["text"])),
            "check-runtime-upgrade.py":SimpleNamespace(record_native_command=command,retire_obsolete_runtime=retire),
            "benchmark-mcp-investigation.py":SimpleNamespace(Session=Session)}
        argv=["producer","--runtime",str(new),"--previous-runtime",str(old),"--expected-runtime-sha256",new_id,
            "--expected-previous-sha256",old_id,"--owner",str(owner)]
        with patch.object(producer,"ROOT",root),patch.object(producer,"load",side_effect=lambda _name,file:modules[file]), \
             patch.object(producer,"sys",SimpleNamespace(platform="win32")),patch("sys.argv",argv),contextlib.redirect_stdout(io.StringIO()):
            if fail_read:
                with self.assertRaisesRegex(RuntimeError,"original read failed"):producer.main()
            elif crossed_stop or healthy_after:
                with self.assertRaises(AssertionError):producer.main()
            else:producer.main()
        return owner,state,old_id,new_id

    def test_positive_producer_retains_json_provenance_sources_and_only_candidate_executables(self):
        owner,state,old_id,new_id=self.journey()
        report=json.loads((owner/"fixture.json").read_text())
        self.assertEqual(report["legacy_config"]["binding_kind"],"retired_installation_launcher")
        self.assertEqual(report["candidate"]["install_manifest_sha256"],new_id)
        self.assertNotIn("helper_sha256",report["candidate"])
        self.assertEqual([path.name for path in (owner/"install/runtimes").iterdir()],[new_id])
        self.assertFalse((owner/"install/airk-launcher.exe").exists())
        self.assertEqual(len(state["stops"]),1)
        self.assertFalse(state["stops"][0]["force"])
        self.assertEqual(state["manual_reads"],1)
        snapshot=Path(report["predecessor"]["snapshot"]["path"])
        self.assertEqual(producer.sha(snapshot),report["predecessor"]["snapshot"]["sha256"])
        self.assertEqual(report["source_hashes_before"],report["source_hashes_after"])

    def test_crossed_stop_retains_failed_cleanup_without_publishing_fixture(self):
        owner,state,_old_id,_new_id=self.journey(crossed_stop=True)
        self.assertFalse((owner/"fixture.json").exists())
        self.assertTrue((owner/"producer-native-stop.json").is_file())
        self.assertTrue((owner/"producer-cleanup-failure.json").is_file())
        self.assertEqual(len(state["stops"]),1)

    def test_healthy_after_stop_retains_incomplete_cleanup_without_replaying_stop(self):
        owner,state,_old_id,_new_id=self.journey(healthy_after=True)
        self.assertFalse((owner/"fixture.json").exists())
        self.assertTrue((owner/"producer-post-stop-health.json").is_file())
        self.assertTrue((owner/"producer-cleanup-failure.json").is_file())
        self.assertEqual(len(state["stops"]),1)

    def test_original_read_failure_stops_only_selected_predecessor_and_preserves_failed_fixture(self):
        owner,state,old_id,_new_id=self.journey(fail_read=True)
        self.assertFalse((owner/"fixture.json").exists())
        self.assertTrue((owner/"producer-failure.json").is_file())
        self.assertTrue((owner/"source/fixture.ts").is_file())
        self.assertEqual([request["runtime"] for request in state["stops"]],[old_id])




class PayloadAndStopContracts(unittest.TestCase):
    def test_payload_extra_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            (root/"airk.exe").write_bytes(b"main")
            (root/"airk-install.json").write_text(json.dumps({"files":{"airk.exe":producer.sha(root/"airk.exe")}}))
            expected=producer.sha(root/"airk-install.json")
            producer.verify_payload(root,expected)
            (root/"foreign.exe").write_bytes(b"extra")
            with self.assertRaises(AssertionError):producer.verify_payload(root,expected)

    def test_payload_reparse_ancestor_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)/"real";root.mkdir()
            (root/"airk.exe").write_bytes(b"main")
            (root/"airk-install.json").write_text(json.dumps({"files":{"airk.exe":producer.sha(root/"airk.exe")}}))
            link=Path(temporary)/"redirect"
            # A junction can be created without Windows symlink privileges.
            if os.name=="nt":
                import subprocess
                result=subprocess.run(['cmd','/c','mklink','/J',str(link),str(root)],capture_output=True)
                self.assertEqual(result.returncode,0)
            else:link.symlink_to(root,target_is_directory=True)
            try:
                with self.assertRaises(AssertionError):producer.verify_payload(link,producer.sha(root/"airk-install.json"))
            finally:
                if os.name=="nt":link.rmdir()
                else:link.unlink()

    def test_stale_or_crossed_complete_stop_record_is_rejected(self):
        request={"caller":"a","key":{"nonce":"b"},"action":"stop","expected_instance":"c","runtime":"d","force":False}
        exact={key:value for key,value in request.items() if key!="force"};exact["manager"]=False
        response={"status":"lifecycle","record":{"phase":"complete","request":exact}}
        producer.verify_stop_response(request,response)
        for field in ("caller","key","expected_instance","runtime","manager"):
            changed=json.loads(json.dumps(response));changed["record"]["request"][field]="crossed"
            with self.assertRaises(AssertionError):producer.verify_stop_response(request,changed)

if __name__=="__main__":unittest.main()
