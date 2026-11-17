# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
B=Path(__file__).resolve().parent;W=B/'p10-worktree'
OUT=B/'post-p14-process-harness-proposed-r2';OUT.mkdir(exist_ok=False)
rel='scripts/check-runtime-legacy-retirement.py';original=(W/rel).read_bytes();s=(W/rel).read_text()
def replace(old,new):
 global s
 assert s.count(old)==1,old[:85]
 s=s.replace(old,new)
replace('''class Mcp:
''','''def bounded_differences(left, right, limit=32):
    """Diagnostic only: never changes the actual full parity comparison."""
    entries = []
    truncated = False
    def shown(value):
        text = json.dumps(value, ensure_ascii=False, sort_keys=True)
        return {"type": type(value).__name__, "json_preview": text[:256],
            "truncated": len(text) > 256,
            "json_sha256": hashlib.sha256(text.encode("utf-8")).hexdigest()}
    def visit(a, b, pointer):
        nonlocal truncated
        if len(entries) >= limit:
            truncated = True
            return
        if type(a) is not type(b):
            entries.append({"pointer": pointer, "cli": shown(a), "mcp": shown(b)})
        elif isinstance(a, dict):
            for key in sorted(a.keys() | b.keys()):
                path = pointer + "/" + key.replace("~", "~0").replace("/", "~1")
                if key not in a or key not in b:
                    if len(entries) >= limit:
                        truncated = True
                        return
                    entries.append({"pointer": path, "present_in": "cli" if key in a else "mcp",
                        "value": shown(a[key] if key in a else b[key])})
                else:
                    visit(a[key], b[key], path)
        elif isinstance(a, list):
            if len(a) != len(b):
                entries.append({"pointer": pointer, "cli_length": len(a), "mcp_length": len(b)})
            for index, (x, y) in enumerate(zip(a, b)):
                visit(x, y, pointer + "/" + str(index))
        elif a != b:
            entries.append({"pointer": pointer, "cli": shown(a), "mcp": shown(b)})
    assert isinstance(limit, int) and 1 <= limit <= 64
    visit(left, right, "")
    return {"difference_limit": limit, "differences_shown": len(entries),
        "truncated": truncated, "entries": entries}


class Mcp:
''')
replace('''        self.log = (fixture.root / f"mcp-{uuid.uuid4().hex}.stderr").open("wb")
        self.child = subprocess.Popen([str(binary), *fixture.project_args(project), "mcp"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
            env=fixture.environment, creationflags=fixture.flags)''','''        identity = uuid.uuid4().hex
        self.log = (fixture.root / f"mcp-{identity}.stderr").open("xb")
        self.trace = (fixture.root / f"mcp-{identity}-wire.jsonl").open("x", encoding="utf-8")
        try:
            self.child = subprocess.Popen([str(binary), *fixture.project_args(project), "mcp"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
                env=fixture.environment, creationflags=fixture.flags)
        except BaseException:
            self.trace.close()
            self.log.close()
            raise''')
replace('''        self.write({"jsonrpc": "2.0", "id": self.counter, "method": method, "params": params})''','''        request = {"jsonrpc": "2.0", "id": self.counter, "method": method, "params": params}
        self.trace.write(json.dumps({"request": request}, ensure_ascii=False) + "\\n")
        self.trace.flush()
        self.write(request)''')
replace('''        reply = json.loads(line)
        assert reply.get("id") == self.counter and "error" not in reply, reply''','''        # Exact raw bytes survive malformed/error replies and every subsequent
        # assertion. The unchanged reader frame bound still applies.
        self.trace.write(json.dumps({"response_bytes_hex": line.hex()}) + "\\n")
        self.trace.flush()
        reply = json.loads(line)
        assert reply.get("id") == self.counter and "error" not in reply, reply''')
replace('''            self.child.stdout.close()
            self.log.close()''','''            self.child.stdout.close()
            self.log.close()
            self.trace.close()''')
replace('''        def read(_):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_search",''','''        observation = uuid.uuid4().hex
        retained = {}
        def retain(suffix, value):
            path = self.root / f"search-{observation}-{suffix}.json"
            with path.open("x", encoding="utf-8") as stream:
                json.dump(value, stream, indent=2)
                stream.write("\\n")
            retained[suffix] = {"path": str(path), "sha256": sha(path)}
        attempts = {"cli": 0, "mcp": 0}
        def read(_):
            result = self.command(binary, *self.project_args(project), "tools", "call", "airk_search",''')
replace('''            assert result.returncode in (0, 1)
            envelope = json.loads(result.stdout)
            assert result.returncode == 0 or self.clients.read_retry_delay(envelope) is not None, envelope''','''            envelope = json.loads(result.stdout)
            attempts["cli"] += 1
            retain(f"cli-attempt-{attempts['cli']}", envelope)
            assert result.returncode in (0, 1)
            assert result.returncode == 0 or self.clients.read_retry_delay(envelope) is not None, envelope''')
replace('''        cli, _ = self.clients.read_envelope(read, time.monotonic() + 60)
        success_flag(cli)''','''        cli, _ = self.clients.read_envelope(read, time.monotonic() + 60)
        retain("cli", cli)
        success_flag(cli)''')
replace('''            wire, _ = self.clients.read_envelope(lambda _: mcp.call("tools/call",
                {"name": "search", "arguments": args}), time.monotonic() + 60)''','''            def read_mcp(_):
                envelope = mcp.call("tools/call", {"name": "search", "arguments": args})
                attempts["mcp"] += 1
                retain(f"mcp-attempt-{attempts['mcp']}", envelope)
                return envelope
            wire, _ = self.clients.read_envelope(read_mcp, time.monotonic() + 60)
            retain("mcp", wire)''')
replace('''        assert stable(body) == stable(other), "Complete CLI/MCP legacy-body mismatch"
        observation = uuid.uuid4().hex
        for suffix, value in (("cli", cli), ("mcp", wire)):
            with (self.root / f"search-{observation}-{suffix}.json").open("x", encoding="utf-8") as stream:
                json.dump(value, stream, indent=2)
                stream.write("\\n")''','''        diagnostic = {"schema": "airk.legacy-read-parity-diagnostic/1",
            "complete_retained_replies": retained,
            "raw_body_differences": bounded_differences(body, other),
            "existing_parity_projection_differences": bounded_differences(stable(body), stable(other)),
            "comparison_unchanged": True,
            "no_cause_inferred": True}
        assert len(json.dumps(diagnostic).encode("utf-8")) <= MAX_FRAME
        retain("comparison", diagnostic)
        assert stable(body) == stable(other), (
            "Complete CLI/MCP legacy-body mismatch; complete replies and bounded structural diff: "
            + retained["comparison"]["path"])''')
dest=OUT/'source'/rel;dest.parent.mkdir(parents=True);dest.write_bytes(s.replace('\n','\r\n').encode()if b'\r\n'in original else s.encode())
test='scripts/tests/test_runtime_legacy_retirement.py'
test_original=(W/test).read_bytes();t=(W/test).read_text();insertion='''    def test_structural_diagnostic_retains_real_freshness_fields_and_type_differences(self):
        diff=h.bounded_differences({'freshness':{'state':'pending'},'flag':True},
            {'freshness':{'state':'idle'},'flag':1})
        self.assertEqual([entry['pointer'] for entry in diff['entries']],['/flag','/freshness/state'])
        self.assertFalse(diff['truncated'])

    def test_structural_diagnostic_bounds_output_without_changing_values(self):
        left={str(i):'a'*1000 for i in range(100)};right={str(i):'b'*1000 for i in range(100)}
        diff=h.bounded_differences(left,right,limit=3)
        self.assertEqual(diff['differences_shown'],3);self.assertTrue(diff['truncated'])
        self.assertTrue(all(entry['cli']['truncated'] for entry in diff['entries']))
        self.assertEqual(left['0'],'a'*1000);self.assertEqual(right['0'],'b'*1000)

    def test_complete_replies_survive_failed_parity_before_any_failure_summary(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();source=root/'source';source.mkdir()
            (source/'README.md').write_bytes(b'legacyhealthyneedle original fictional bytes\\n')
            binary=root/'old.exe';binary.write_bytes(b'inert unit bytes, never executed')
            project={'root':str(source),'data_root':str(root/'data'),'collection':'legacy-b','binding':'a'*64}
            body={'collection':'legacy-b','generation':1,'truncated':False,
                'hits':[{'text':'legacyhealthyneedle','truncated':False,'citation':{
                    'path':'README.md','content_hash':h.sha(source/'README.md')}}],
                'publication':{'token':'literal-fixture-publication'},'freshness':{'state':'pending'}}
            other=json.loads(json.dumps(body));other['freshness']['state']='idle'
            envelope=lambda value:{'content':[{'type':'text','text':json.dumps(value)}],'isError':False}
            class Clients:
                @staticmethod
                def read_envelope(read,_deadline):return read(None),[]
                @staticmethod
                def tool_value(value):return json.loads(value['content'][0]['text'])
                @staticmethod
                def read_retry_delay(_value):return None
            class Mcp:
                def __init__(self,*_args):pass
                def call(self,*_args):return envelope(other)
                def close(self):pass
            fixture=object.__new__(h.Fixture);fixture.root=root;fixture.control=root/'control'
            fixture.old_seal={'files':{'airk.exe':h.sha(binary)},'manifest_sha256':'1'*64}
            fixture.new_seal={'files':{'airk.exe':'2'*64},'manifest_sha256':'3'*64}
            fixture.clients=Clients()
            fixture.command=lambda *_args,**_kwargs:SimpleNamespace(returncode=0,stdout=json.dumps(envelope(body)))
            with patch.object(h,'Mcp',Mcp):
                with self.assertRaisesRegex(AssertionError,'Complete CLI/MCP legacy-body mismatch'):
                    fixture.query(binary,project)
            cli=list(root.glob('search-*-cli.json'));mcp=list(root.glob('search-*-mcp.json'))
            self.assertEqual(len(cli),1);self.assertEqual(len(mcp),1)
            self.assertEqual(Clients.tool_value(json.loads(cli[0].read_text())),body)
            self.assertEqual(Clients.tool_value(json.loads(mcp[0].read_text())),other)
            report=json.loads(next(root.glob('search-*-comparison.json')).read_text())
            self.assertEqual(report['existing_parity_projection_differences']['entries'][0]['pointer'],'/freshness/state')

'''
assert t.count("if __name__=='__main__':")==1
t=t.replace("\n\nif __name__=='__main__':",'\n\n'+insertion+"if __name__=='__main__':")
dest=OUT/'source'/test;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(t.replace('\n','\r\n').encode()if b'\r\n'in test_original else t.encode())
rows=[];patch=''
for name in [rel,test]:
 old=(W/name).read_bytes();new=(OUT/'source'/name).read_bytes();rows.append({'path':name,'base_sha256':hashlib.sha256(old).hexdigest(),'proposed_sha256':hashlib.sha256(new).hexdigest()});patch+=''.join(difflib.unified_diff(old.decode().splitlines(True),new.decode().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
(OUT/'fix.patch').write_bytes(patch.encode())
m={'status':'Frozen diagnostic-only proposal; no live edits or native execution','files':rows,'patch_sha256':hashlib.sha256((OUT/'fix.patch').read_bytes()).hexdigest(),'observed':'ActualR2 first3oldbaselinecomparisonsfailedbutMCPbodywasnotretained. Actualforeign-return oldpair fullcomparesuccess, bothfreshnessidle; failedoldCLIwaspending. This supportsa possibleobservationracebutdoesnotestablishfailedMCPcause.','scope':'PersistcompleteCLI/MCPresults beforebody/flag/parityassertions andexactRPCrequest/responsebytes beforeRPCassertion. Boundedstructuraldiffretainsactualfreshness/state/type/key differences. Existingtimingnormalization/fullbodycomparison/aliases/query/scope/nativeactions/timeouts/framebounds unchanged.','pure_tests':'Existing5plus3diagnosticregressions; no native execution.','next':'Rootreview+pure8 then ONE source-only diagnostic run, not another4. No blanketfreshnessnormalization/retryuntilmatch/newPausepolicy or speculativebodyfix.'}
(OUT/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'patch_sha256':m['patch_sha256'],'manifest_sha256':hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest()}))
