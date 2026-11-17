# SPDX-License-Identifier: Apache-2.0
"""Fake-native controls through the exact frozen SDK and cold assertions; no native process."""
import base64
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import types
import unittest
from unittest.mock import Mock, patch
import capture

C = Path(__file__).resolve().parents[4] / "candidate-worktree-r2"
# Explicit known candidate, independent of the active Root's source WIP.
C = Path(r"D:\code\Github\airk\artifacts\v190\final-plan\candidate-worktree-r2")

def load(name):
    spec=importlib.util.spec_from_file_location("cold_test_"+name.replace("-","_"),C/"scripts"/(name+".py"))
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

CLIENTS, SDK = load("check-service-clients"), load("http_fixture")
VALUE={"index_state":"unavailable","files":None,"publication":None,
       "read_scope":{"source":"named","index_ids":["cold-index"]},
       "future_native_receipt":{"all":[1,"ÃƒÂ©Ã¤Â¸Â­",{"retained":True}]}}
GOOD={"isError":False,"content":[{"type":"text","text":json.dumps(VALUE,ensure_ascii=False)}],"structuredContent":VALUE}
ERROR={"isError":True,"content":[{"type":"text","text":json.dumps({"error":{"code":"fixture_native_refused","message":"ÃƒÂ©Ã¤Â¸Â­","future":[1,2]}})}],
       "structuredContent":{"error":{"code":"fixture_native_refused","message":"ÃƒÂ©Ã¤Â¸Â­","future":[1,2]}},"unknown_receipt":{"complete":True}}

class Socket:
    timeout=None
    def getpeercert(self,binary_form=False):return b"fixture-peer"
    def settimeout(self,value):self.timeout=value

class Response:
    status=200
    def __init__(self,body,session=None):self.body,self.session=body,session
    def read(self,*args,**kwargs):return self.body
    def getheaders(self):return [("Content-Type","application/json")]+([("Mcp-Session-Id",self.session)] if self.session else [])
    def getheader(self,name):return dict(self.getheaders()).get(name)

class Connection:
    timeout=125
    def __init__(self,status,client):self.status,self.client=status,client;self.sock=Socket();self.sent=[]
    def request(self,method,path,body=None,headers=None,*args,**kwargs):
        value=json.loads(body) if body is not None else None;self.sent.append((method,path,value));session=None
        if value and value.get("method")=="initialize":
            result={"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","serverInfo":{"name":"airk"}}};session="fixture-session-"+self.client
        elif value and value.get("method")=="tools/call":
            if isinstance(self.status,BaseException):raise self.status
            if isinstance(self.status,bytes):self.response=Response(self.status);return
            result={"jsonrpc":"2.0","id":2,"result":self.status(self.client) if callable(self.status) else self.status}
        else:result=None
        raw=json.dumps(result,ensure_ascii=False,separators=(",",":")).encode() if result is not None else b""
        self.response=Response(raw,session)
    def getresponse(self):return self.response
    def close(self):self.sock=None

class Fixture:
    single_image=catalog_authority=True
    scopes={"cold":{"index_id":"cold-index"}}
    owner=Path("fake-private-fixture")
    def __init__(self,statuses,recorder=None):self.statuses=statuses;self.recorder=recorder;self.networks={}
    def open_http(self,name,client=None):
        number=int(client.rsplit("-",1)[1]);network=object.__new__(SDK.McpClient)
        network.scope={"index_ids":["cold-index"]};network.connection=Connection(self.statuses[number],client)
        network._certificate_sha256=hashlib.sha256(b"fixture-peer").hexdigest();network.session=None
        network.initialized=False;network.protocol="2025-11-25";network.pending={};network.unsolicited=[]
        self.networks[client]=network
        if self.recorder:capture.tap_network(network,self.recorder,self._cold_admission,client,self.owner)
        return network
    def rpc(self,method,deadline=None):return {"status":"embedding_status","loads":0}
    def assert_alive(self):pass

class Controls(unittest.TestCase):
    def run_case(self,statuses,observed):
        with tempfile.TemporaryDirectory() as temporary:
            recorder=capture.Capture(Path(temporary)/"events.jsonl");fixture=Fixture(statuses,recorder if observed else None)
            error=None;result=None
            try:
                if observed:
                    with capture.observe_clients(CLIENTS,None,recorder,"fixture-SID"):
                        result=CLIENTS.cold_first_status(fixture,"concurrent",case_deadline=time.monotonic()+240)
                else:result=CLIENTS.cold_first_status(fixture,"concurrent",case_deadline=time.monotonic()+240)
            except BaseException as actual:error=actual
            recorder.close();return result,error,fixture,recorder

    def requests(self,fixture):return {key:network.connection.sent for key,network in fixture.networks.items()}

    def test_complete_keeps_every_native_field_and_exact_request_matrix(self):
        baseline=self.run_case([GOOD]*4,False);observed=self.run_case([GOOD]*4,True)
        self.assertIsNone(observed[1]);self.assertEqual(baseline[0],observed[0]);self.assertEqual(self.requests(baseline[2]),self.requests(observed[2]))
        replies=[r for r in observed[3].rows if r["kind"]=="native_reply_before_caller_validation" and r["identifier"]==2]
        self.assertEqual(len(replies),4);self.assertTrue(all(r["envelope"]["result"]==GOOD for r in replies));self.assertFalse(observed[3].errors)
        clocks=[r["original_deadline_monotonic"] for r in observed[3].rows if r["kind"]=="original_read_clock"]
        self.assertEqual(len(clocks),4);self.assertEqual(len(set(clocks)),1)

    def test_all_error_bodies_captured_before_unchanged_rejection_without_replay(self):
        baseline=self.run_case([ERROR]*4,False);observed=self.run_case([ERROR]*4,True)
        self.assertEqual((type(baseline[1]),str(baseline[1])),(type(observed[1]),str(observed[1])))
        self.assertEqual(len(observed[2].networks),4)
        for network in observed[2].networks.values():
            self.assertEqual([row[2].get("method") if row[2] else row[0] for row in network.connection.sent],
                             ["initialize","notifications/initialized","tools/call","DELETE"])
        replies=[r for r in observed[3].rows if r["kind"]=="native_reply_before_caller_validation" and r["identifier"]==2]
        self.assertEqual(len(replies),4);self.assertTrue(all(r["envelope"]["result"]==ERROR and r["attempt"]==1 for r in replies))
        siblings=[r for r in observed[3].rows if r["kind"]=="sibling_outcome" and r["function"]=="observe"]
        self.assertEqual(len(siblings),4);self.assertTrue(all(r["outcome"]=="error" for r in siblings))

    def test_malformed_json_and_result_shapes_remain_exact_and_fail_originally(self):
        for bad in [b"{broken-native-json",[],None]:
            with self.subTest(bad=bad):
                baseline=self.run_case([bad]*4,False);observed=self.run_case([bad]*4,True)
                self.assertEqual((type(baseline[1]),str(baseline[1])),(type(observed[1]),str(observed[1])))
                self.assertEqual(len(observed[2].networks),4)
                self.assertTrue(all(sum(1 for row in network.connection.sent if row[2] and row[2].get("method")=="tools/call")==1 for network in observed[2].networks.values()))
                raw=[r for r in observed[3].rows if r["kind"]=="wire_response_before_sdk_validation" and r["attempt"]==1 and r["evidence"]["request"]["jsonrpc_id"]==2]
                self.assertEqual(len(raw),4)
                if isinstance(bad,bytes):self.assertTrue(all(base64.b64decode(r["evidence"]["raw_body_base64"])==bad for r in raw))

    def test_mixed_siblings_all_finish_and_native_io_exception_identity_survives(self):
        marker=OSError("native transport failure marker")
        result,error,fixture,recorder=self.run_case([marker,GOOD,b"{bad",GOOD],True)
        self.assertIs(error,marker)
        siblings=[r for r in recorder.rows if r["kind"]=="sibling_outcome" and r["function"]=="observe"]
        self.assertEqual(len(siblings),4);self.assertEqual([r["outcome"] for r in sorted(siblings,key=lambda r:r["client"])],["error","complete","error","complete"])
        self.assertTrue(all(network.connection.sent[-1][:2]==("DELETE","/mcp") for network in fixture.networks.values()))

    def test_capture_io_failure_does_not_replace_native_error_or_drop_sibling_memory(self):
        class FailedSink:
            def write(self,value):raise OSError("capture disk failure")
            def flush(self):pass
            def close(self):pass
        with tempfile.TemporaryDirectory() as temporary:
            recorder=capture.Capture(Path(temporary)/"events");recorder.stream.close();recorder.stream=FailedSink()
            marker=OSError("original native IO");fixture=Fixture([marker,GOOD,GOOD,GOOD],recorder)
            with capture.observe_clients(CLIENTS,None,recorder,"fixture-SID"):
                with self.assertRaises(OSError) as actual:CLIENTS.cold_first_status(fixture,"concurrent",case_deadline=time.monotonic()+240)
            self.assertIs(actual.exception,marker);self.assertTrue(recorder.errors)
            self.assertEqual(len([r for r in recorder.rows if r["kind"]=="sibling_outcome" and r["function"]=="observe"]),4);recorder.close()

    def test_session_redaction_uses_sdk_owner_and_preserves_native_body_bytes(self):
        _,_,_,recorder=self.run_case([GOOD]*4,True)
        self.assertNotIn("fixture-session-",json.dumps(recorder.rows))
        raw=[r for r in recorder.rows if r["kind"]=="wire_response_before_sdk_validation" and r["evidence"]["request"]["jsonrpc_id"]==2]
        for row in raw:
            self.assertFalse(row["evidence"]["body_redacted"]);self.assertFalse(row["evidence"]["body_truncated"])
            self.assertEqual(json.loads(base64.b64decode(row["evidence"]["raw_body_base64"]))["result"],GOOD)

    def test_session_echo_is_redacted_in_both_copies_without_mutating_native_reply(self):
        def status(client):
            value=copy.deepcopy(VALUE);value["echo"]="fixture-session-"+client
            return {"isError":False,"content":[{"type":"text","text":json.dumps(value)}],"structuredContent":value}
        _,error,fixture,recorder=self.run_case([status]*4,True)
        self.assertIsNone(error);self.assertNotIn("fixture-session-",json.dumps(recorder.rows))
        replies=[r for r in recorder.rows if r["kind"]=="native_reply_before_caller_validation" and r["identifier"]==2]
        self.assertTrue(all(r["body_redacted"] for r in replies));self.assertTrue(all(r["envelope"]["result"]["structuredContent"]["echo"]=="[REDACTED_FIXTURE_SESSION]" for r in replies))

    def test_existing_process_handle_is_closed_after_exact_original_cleanup_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            owner=Path(temporary);(owner/"source").mkdir();binary=owner/"airk.exe";binary.write_bytes(b"fake")
            recorder=capture.Capture(owner/"events");marker=RuntimeError("original Stop failure")
            held=Mock();held.gone.side_effect=[False,True];held.observation.return_value={"path":str(binary),"user":"fixture-SID"}
            backend=types.SimpleNamespace(user="fixture-SID",open=Mock(return_value=held));processes=types.SimpleNamespace(_backend=lambda:backend)
            def start(f):f.owner=owner;f.binary=binary;f.managed_health={"process_id":123,"user":"fixture-SID"};f.managed_stop=None
            with patch.object(CLIENTS.Fixture,"start",start),patch.object(CLIENTS.Fixture,"close",side_effect=marker):
                with capture.observe_clients(CLIENTS,processes,recorder,"fixture-SID"):
                    fixture=object.__new__(CLIENTS.Fixture);fixture.start()
                    with self.assertRaises(RuntimeError) as actual:fixture.close()
            self.assertIs(actual.exception,marker);held.close.assert_called_once();self.assertTrue(any(r["kind"]=="native_process_handle_after_stop" and r["handle_signaled"] for r in recorder.rows));recorder.close()

if __name__=="__main__":unittest.main(verbosity=2)
