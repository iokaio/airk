# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
B=Path(__file__).resolve().parent;W=B/'p10-worktree'
rel='scripts/check-runtime-legacy-retirement.py'
original=(W/rel).read_bytes();s=(W/rel).read_text()
def replace(old,new):
 global s
 assert s.count(old)==1,old[:90]
 s=s.replace(old,new)
replace('''class Mcp:
''','''def search_arguments(payload, old_payload, new_payload):
    # The actual sealed 1.8.7 parser rejects verification. These common inputs
    # retain identical lexical read semantics; only the new parser accepts Off.
    assert payload in (old_payload, new_payload) and old_payload != new_payload
    args = {"query": "legacyhealthyneedle", "mode": "lexical", "limit": 4,
        "response_mode": "full"}
    if payload == new_payload:
        args["verification"] = "off"
    return args


def selected_payload(root, install, old_payload, new_payload):
    selection_path = contained(root, install / "selected.json")
    assert selection_path.stat().st_size <= MAX_FRAME
    selected = json.loads(selection_path.read_text(encoding="utf-8"))
    identity = selected["active"]
    expected = [item for item in (old_payload, new_payload)
        if item["manifest_sha256"] == identity]
    assert len(expected) == 1, "Selected runtime is outside the two actual sealed payloads"
    assert isinstance(identity, str) and len(identity) == 64 and all(c in "0123456789abcdef" for c in identity)
    directory = contained(root, install / "runtimes" / identity)
    assert seal(directory) == expected[0], "Selected installed payload changed"
    binary = contained(root, directory / "airk.exe")
    assert sha(binary) == expected[0]["files"]["airk.exe"]
    return binary, selected


def imported_query_scope(body, project, plan):
    matches = [item for item in plan["indexes"] if item["registration"] == project]
    assert len(matches) == 1, "Healthy read escaped the actual immutable native upgrade roster"
    observation = body["scope_observation"]
    assert observation["schema"] == 1 and observation["provenance"] == "imported_legacy_snapshot"
    assert len(observation["members"]) == 1
    member = observation["members"][0]
    assert member["index_id"] == matches[0]["index_id"]
    assert [item["project_id"] for item in member["memberships"]] == [matches[0]["project_id"]]
    return observation


def finish_owner(fixture, receipt):
    receipt["actual_host_readonly_parity"] = fixture.host_parity_receipts
    receipt["actual_query_observations"] = fixture.query_observations
    receipt["fixture_retained"] = True
    try:
        # Selection may still be OLD after a failed pre-upgrade assertion. Never
        # guess from the intended candidate or start/abort/activate during teardown.
        receipt["final_quiescence"] = fixture.drain()
    except Exception as error:
        receipt["stop_error"] = repr(error)
        receipt["status"] = "failed"


class Mcp:
''')
replace('''        self.host_parity_receipts = []
''','''        self.host_parity_receipts = []
        self.query_observations = []
''')
replace('''    def drain(self, binary):
''','''    def drain(self, binary=None):
''')
replace('''        health = self.invoke(binary, "service", "health", "--runtime-dir", self.control)
        selected = json.loads((self.install / "selected.json").read_text(encoding="utf-8"))
        assert health["status"] == "health" and health["observation"]["runtime_sha256"] == selected["active"]
        assert health["observation"]["executable_sha256"] == sha(binary)
        reply = self.invoke(binary, "service", "shutdown", "--runtime-dir", self.control)
        assert reply["status"] == "stopping", reply''','''        selected_binary, selected = selected_payload(self.root, self.install, self.old_seal, self.new_seal)
        if binary is not None:
            assert sha(binary) == sha(selected_binary), "Requested drain binary differs from actual selected owner"
        binary = selected_binary
        observed = self.command(binary, "service", "health", "--runtime-dir", self.control, check=False)
        health = None
        if observed.returncode == 0:
            health = json.loads(observed.stdout)
            assert health["status"] == "health" and health["observation"]["runtime_sha256"] == selected["active"]
            assert health["observation"]["executable_sha256"] == sha(binary)
            reply = self.invoke(binary, "service", "shutdown", "--runtime-dir", self.control)
            assert reply["status"] == "stopping", reply
        # Unavailable health does not imply absence. Existing owner files and the
        # final native all-role proof remain mandatory; never restart for a receipt.''')
replace('''        assert proof["launcher_repaired"] is True
        return proof''','''        assert proof == {"launcher_repaired": True, "runtime": selected["active"]}
        assert json.loads((self.install / "selected.json").read_text(encoding="utf-8")) == selected
        selected_after, _ = selected_payload(self.root, self.install, self.old_seal, self.new_seal)
        assert selected_after.samefile(binary)
        return {"selected_manifest_sha256": selected["active"], "binary": str(binary),
            "authenticated_health": health, "health_exit_code": observed.returncode,
            "native_all_role_proof": proof}''')
replace('''        args = {"query": "legacyhealthyneedle", "mode": "lexical", "limit": 4,
            "verification": "off", "response_mode": "full"}''','''        digest = sha(binary)
        payloads = [item for item in (self.old_seal, self.new_seal) if item["files"]["airk.exe"] == digest]
        assert len(payloads) == 1, "Query binary differs from actual old/new payload seals"
        payload = payloads[0]["manifest_sha256"]
        args = search_arguments(payload, self.old_seal["manifest_sha256"], self.new_seal["manifest_sha256"])''')
replace('''        assert stable(body) == stable(other), "Complete CLI/MCP legacy-body mismatch"
        observation = uuid.uuid4().hex
        for suffix, value in (("cli", cli), ("mcp", wire)):
            with (self.root / f"search-{observation}-{suffix}.json").open("x", encoding="utf-8") as stream:
                json.dump(value, stream, indent=2)
                stream.write("\\n")''','''        observation = uuid.uuid4().hex
        outputs = []
        for suffix, value in (("cli", cli), ("mcp", wire)):
            path = self.root / f"search-{observation}-{suffix}.json"
            with path.open("x", encoding="utf-8") as stream:
                json.dump(value, stream, indent=2)
                stream.write("\\n")
            outputs.append({"path": str(path), "sha256": sha(path)})
        assert stable(body) == stable(other), "Complete CLI/MCP legacy-body mismatch"
        scope = None
        if payload == self.new_seal["manifest_sha256"]:
            scope = imported_query_scope(body, project, immutable_upgrade(self.control)["plan"])
        self.query_observations.append({"payload_manifest_sha256": payload,
            "arguments": args, "complete_cli_mcp_replies": outputs,
            "native_imported_scope_observation": scope,
            "cross_version_projection": "All literal publication/collection/generation/hit/citation/text/location/truncation data plus unchanged native registration root. Newly added scope provenance is separately checked and retained, never compared as an invented old field."})''')
replace('''            receipt["actual_host_readonly_parity"] = fixture.host_parity_receipts
            # Retain even successful synthetic fixtures: ordinary physical cleanup
            # is a separate owner proof, not implied by logical Applied.
            receipt["fixture_retained"] = True
            try:
                receipt["final_quiescence"] = fixture.drain(fixture.new / "airk.exe")
            except Exception as error:
                receipt["stop_error"] = repr(error)
                receipt["status"] = "failed"''','''            # Retain even successful fixtures; quiescence is separate from physical
            # cleanup and from the scenario's original primary result.
            finish_owner(fixture, receipt)''')
out=B/'post-p14-process-harness-proposed-r1';out.mkdir(exist_ok=False)
dest=out/'source'/rel;dest.parent.mkdir(parents=True);new=s.replace('\n','\r\n').encode() if b'\r\n' in original else s.encode();dest.write_bytes(new)
patch=''.join(difflib.unified_diff(original.decode().splitlines(True),new.decode().splitlines(True),fromfile='a/'+rel,tofile='b/'+rel));(out/'fix.patch').write_bytes(patch.encode())
m={'status':'frozen inert processharness correction; no live edits or native execution','files':[{'path':rel,'base_sha256':hashlib.sha256(original).hexdigest(),'proposed_sha256':hashlib.sha256(new).hexdigest()}],'patch_sha256':hashlib.sha256((out/'fix.patch').read_bytes()).hexdigest(),'causes':['Actualpublic1.8.7 sealed parser rejects verification; pinned old uses commonlexical/full arguments, exactnewpayload explicitlyOff.','Finally previouslyguessednewbinary whileactiveold; nowreadactualselected.active, requirematchingcompleteinstalledold/newseal, exactselectedhealth/shutdown/repair andexistingallroleleases; no startup/activation/abort/retrymutation.'],'parity':'Complete within-version CLI/MCP body comparisons unchanged. Cross-version retainedprojection stillpreservesallliteralhits/citations/location/text/truncation/publication/collection/generation/root; newimported Index/Project provenance checkedagainstactualimmutableupgradeplan and retainedseparately.','pending':['Independent complete review','Root apply/puretests and exactfreshowned4process rerun using matcheddebugpair; all failed firstbatch/cleanup remainsretained']}
(out/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'patch_sha256':m['patch_sha256'],'manifest_sha256':hashlib.sha256((out/'manifest.json').read_bytes()).hexdigest()}))
