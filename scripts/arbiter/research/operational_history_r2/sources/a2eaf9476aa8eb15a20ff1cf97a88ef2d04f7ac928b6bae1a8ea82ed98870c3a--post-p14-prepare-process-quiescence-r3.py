# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
B=Path(__file__).resolve().parent;W=B/'p10-worktree'
OUT=B/'post-p14-process-harness-proposed-r3';OUT.mkdir(exist_ok=False)
rel='scripts/check-runtime-legacy-retirement.py';original=(W/rel).read_bytes();s=(W/rel).read_text()
def replace(old,new):
 global s
 assert s.count(old)==1,old[:100]
 s=s.replace(old,new)
replace('''def finish_owner(fixture, receipt):''','''def old_status_ready(value, project, indexed_job):
    # This is a cache-sampling barrier, not a query-result comparison or a live
    # source verification claim. The exact completed Index receipt supplies both
    # the source job and publication; no fabricated watermark is introduced.
    assert indexed_job["state"] == "succeeded" and indexed_job["kind"] == "index"
    assert indexed_job["outcome"]["state"] == "succeeded"
    assert value["collection"] == project["collection"]
    freshness = value["freshness"]
    required = {"state", "source_job", "requested_watermark", "completed_watermark",
        "running_watermark", "last_success_unix_ms", "error"}
    assert isinstance(freshness, dict) and required <= freshness.keys(), "Actual old full status lacks required freshness metadata"
    assert freshness["state"] in ("pending", "running", "idle"), freshness
    assert freshness["error"] is None, freshness
    requested, completed = freshness["requested_watermark"], freshness["completed_watermark"]
    assert type(requested) is int and type(completed) is int and 0 <= completed <= requested
    if (freshness["state"] != "idle" or completed < requested
            or freshness["running_watermark"] is not None
            or freshness["source_job"] != indexed_job["id"]):
        return False
    assert type(freshness["last_success_unix_ms"]) is int and freshness["last_success_unix_ms"] > 0
    assert value["publication"]["token"] == indexed_job["outcome"]["revision"], "Status publication differs from exact completed Index"
    return True


def finish_owner(fixture, receipt):''')
replace('''    receipt["actual_host_readonly_parity"] = fixture.host_parity_receipts''','''    receipt["actual_host_readonly_parity"] = fixture.host_parity_receipts
    receipt["actual_old_read_quiescence"] = getattr(fixture, "old_read_quiescence", [])''')
replace('''        self.host_parity_receipts = []''','''        self.host_parity_receipts = []
        self.source_jobs = {}
        self.old_read_quiescence = []''')
replace('''    def query(self, binary, project):''','''    def wait_old_read_quiescence(self, binary, project, deadline):
        assert sha(binary) == self.old_seal["files"]["airk.exe"]
        indexed_job = self.source_jobs[project["binding"]]
        # airk_status is the existing content-free native read. Its full response
        # is retained by command(); no search is submitted by this barrier.
        def read_status(until):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_status",
                "--args", json.dumps({"response_mode": "full"}), check=False,
                timeout=self.clients.remaining(until, 60))
            envelope = json.loads(result.stdout)
            assert result.returncode in (0, 1)
            assert result.returncode == 0 or self.clients.read_retry_delay(envelope) is not None, envelope
            return envelope
        for poll in range(601):
            self.clients.remaining(deadline, 60)
            envelope, _ = self.clients.read_envelope(read_status, deadline)
            value = self.clients.tool_value(envelope)
            if old_status_ready(value, project, indexed_job):
                self.old_read_quiescence.append({"project": project["binding"],
                    "source_job": indexed_job["id"], "publication": indexed_job["outcome"]["revision"],
                    "freshness": value["freshness"], "metadata_polls": poll + 1,
                    "final_status_command_ordinal": len(self.commands),
                    "qualification": "Actual old worker source-metadata sample idle; not live verification"})
                return
            time.sleep(min(.1, self.clients.remaining(deadline, 60)))
        raise AssertionError("Old metadata observation budget exhausted; no evidence read was retried to match")

    def query(self, binary, project):''')
replace('''        args = search_arguments(payload, self.old_seal["manifest_sha256"], self.new_seal["manifest_sha256"])
        observation = uuid.uuid4().hex''','''        args = search_arguments(payload, self.old_seal["manifest_sha256"], self.new_seal["manifest_sha256"])
        deadline = time.monotonic() + 60
        if payload == self.old_seal["manifest_sha256"]:
            self.wait_old_read_quiescence(binary, project, deadline)
        observation = uuid.uuid4().hex''')
replace('''        def read(_):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_search",
                "--args", json.dumps(args), check=False)''','''        def read(until):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_search",
                "--args", json.dumps(args), check=False, timeout=self.clients.remaining(until, 60))''')
replace('''        cli, _ = self.clients.read_envelope(read, time.monotonic() + 60)''','''        cli, _ = self.clients.read_envelope(read, deadline)''')
replace('''            f.wait(f.old / "airk.exe", f.invoke(f.old / "airk.exe", *f.project_args(p), "index"))''','''            f.source_jobs[p["binding"]] = f.wait(f.old / "airk.exe",
                f.invoke(f.old / "airk.exe", *f.project_args(p), "index"))''')
dest=OUT/'source'/rel;dest.parent.mkdir(parents=True);dest.write_bytes(s.replace('\n','\r\n').encode()if b'\r\n'in original else s.encode())
test='scripts/tests/test_runtime_legacy_retirement.py';old_test=(W/test).read_bytes();t=(W/test).read_text()
t=t.replace('''            fixture.clients=Clients()
''','''            fixture.clients=Clients()
            fixture.wait_old_read_quiescence=lambda *_args:None  # This unit isolates failure-evidence retention.
''')
insertion='''    def test_old_status_barrier_waits_for_same_source_job_idle_watermark_and_publication(self):
        project={'collection':'legacy-b'}
        job={'id':'actual-index','state':'succeeded','kind':'index',
            'outcome':{'state':'succeeded','revision':'exact-publication'}}
        value={'collection':'legacy-b','publication':{'token':'exact-publication'},
            'freshness':{'state':'pending','source_job':'actual-index',
                'requested_watermark':0,'completed_watermark':0,'running_watermark':None,
                'last_success_unix_ms':None,'error':None}}
        self.assertFalse(h.old_status_ready(value,project,job))
        value['freshness'].update(state='idle',last_success_unix_ms=123)
        self.assertTrue(h.old_status_ready(value,project,job))
        value['freshness']['source_job']='older-source-job'
        self.assertFalse(h.old_status_ready(value,project,job))
        value['freshness']['source_job']='actual-index';value['publication']['token']='another-publication'
        with self.assertRaises(AssertionError):h.old_status_ready(value,project,job)

    def test_old_status_barrier_refuses_failed_missing_or_malformed_metadata(self):
        project={'collection':'legacy-b'}
        job={'id':'index','state':'succeeded','kind':'index','outcome':{'state':'succeeded','revision':'publication'}}
        with self.assertRaises(AssertionError):h.old_status_ready({'collection':'legacy-b','freshness':{'state':'idle'}},project,job)
        value={'collection':'legacy-b','publication':{'token':'publication'},'freshness':{
            'state':'failed','source_job':'index','requested_watermark':0,'completed_watermark':0,
            'running_watermark':None,'last_success_unix_ms':12,'error':'failed'}}
        with self.assertRaises(AssertionError):h.old_status_ready(value,project,job)
        value['freshness'].update(state='idle',error=None,completed_watermark=True)
        with self.assertRaises(AssertionError):h.old_status_ready(value,project,job)

    def test_old_status_wait_performs_only_metadata_reads_with_original_deadline(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        project={'binding':'b','root':'source','data_root':'data','collection':'legacy-b'}
        job={'id':'index','state':'succeeded','kind':'index','outcome':{'state':'succeeded','revision':'publication'}}
        pending={'collection':'legacy-b','publication':{'token':'publication'},'freshness':{
            'state':'pending','source_job':'index','requested_watermark':0,'completed_watermark':0,
            'running_watermark':None,'last_success_unix_ms':None,'error':None}}
        idle=json.loads(json.dumps(pending));idle['freshness'].update(state='idle',last_success_unix_ms=12)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();binary=root/'old.exe';binary.write_bytes(b'inert unit bytes')
            fixture=object.__new__(h.Fixture);fixture.old_seal={'files':{'airk.exe':h.sha(binary)}}
            fixture.control=root/'control';fixture.source_jobs={'b':job};fixture.old_read_quiescence=[];fixture.commands=[]
            values=iter([pending,idle]);calls=[];deadlines=[]
            def command(_binary,*arguments,**options):
                calls.append(arguments);self.assertEqual(arguments[-4:-2],('call','airk_status'))
                fixture.commands.append(arguments)
                return SimpleNamespace(returncode=0,stdout=json.dumps({'structuredContent':next(values)}))
            fixture.command=command
            class Clients:
                @staticmethod
                def remaining(until,ceiling):deadlines.append(until);return ceiling
                @staticmethod
                def read_envelope(read,until):return read(until),[]
                @staticmethod
                def tool_value(envelope):return envelope['structuredContent']
                @staticmethod
                def read_retry_delay(_value):return None
            fixture.clients=Clients()
            with patch.object(h.time,'sleep') as sleep:
                fixture.wait_old_read_quiescence(binary,project,999.0)
            self.assertEqual(len(calls),2);self.assertEqual(set(deadlines),{999.0})
            self.assertEqual(fixture.old_read_quiescence[0]['metadata_polls'],2)
            sleep.assert_called_once_with(.1)

'''
assert t.count("if __name__=='__main__':")==1
t=t.replace("\nif __name__=='__main__':",'\n'+insertion+"if __name__=='__main__':")
dest=OUT/'source'/test;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(t.replace('\n','\r\n').encode()if b'\r\n'in old_test else t.encode())
rows=[];patch=''
for name in [rel,test]:
 old=(W/name).read_bytes();new=(OUT/'source'/name).read_bytes();rows.append({'path':name,'base_sha256':hashlib.sha256(old).hexdigest(),'proposed_sha256':hashlib.sha256(new).hexdigest()});patch+=''.join(difflib.unified_diff(old.decode().splitlines(True),new.decode().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
(OUT/'fix.patch').write_bytes(patch.encode())
m={'status':'Frozen minimal old-only read-quiescence fixture proposal; no live edits/native execution','files':rows,'patch_sha256':hashlib.sha256((OUT/'fix.patch').read_bytes()).hexdigest(),'actual_cause':'SingleR3 exactfullbodydiff ONLYfreshness.last_success null->timestamp andstatepending->idle. ActualROoldschema shows sameexplicitBIndexsource_job succeeded andwatchlast_reconciled matchesMCPtimestamp; documentedworkerstatuscachelag, not changedhits/publication/aliasdata.','fix':'Oldpayloadonly actualairk_status fullmetadata polls untilsamecompletedIndexsource_job/publication, idle/no running watermark/completed>=requested/positive lastsuccess/no error; thenoriginalCLI/MCPevidencereads once. StatuswaitandCLIsearchshareORIGINAL60sdeadline. All pollsretainedoriginalcommandstreams; noPause/mutation/sleep-only policy/searchretryuntilmatch/normalization/alias/framechange. Newimportedlegacyfreshnessintentionallyunavailable: no barrierthere.','schema_evidence':'ActualoldROregistry columns/sourcejob+fullreturnedSearchFreshness fields verified. Nativeairk_status fullresponse is existing read-onlystatus contract; firstactualstatus response must satisfyexactrequiredfields/types, otherwise failclosed. No silent fallback/inventedready.','pure_tests':'Existing8+3 readiness/failclosed/metadata-onlyoriginaldeadline regressions; actualnativequalification pending'}
(OUT/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'patch_sha256':m['patch_sha256'],'manifest_sha256':hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest()}))
