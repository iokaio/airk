# SPDX-License-Identifier: Apache-2.0
"""Synthetic zero-NN phase/order/deadline/refusal controls, not qualification."""
import json,tempfile,time,unittest
from pathlib import Path
from unittest.mock import patch
from chunk_support import partitions,pin,global_deadline,PhaseBoundary,merge_seals
from run_controller import walk_phases
class Controls(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup);self.root=Path(self.temp.name)
        self.start=self.root/'start.json';self.start.write_text(json.dumps({'started_unix':1000}))
        self.bound=self.root/'bound.json';self.bound.write_text(json.dumps({'original_start_unix_ms':1000000,'absolute_deadline_unix_ms':11800000,'original_start_evidence':pin(self.start),'original_start_field':'started_unix'}))
        self.packet={'fixture_bound':pin(self.bound),'owner_seconds':1800};self.mono=0.;self.wall=1000.
    def boundary(self):return PhaseBoundary(self.packet,0,lambda:self.mono,lambda:self.wall)
    def test_exact_independent100_order_and_13_ranges(self):
        ids=['q'+str(i) for i in range(100)];parts=partitions(ids)
        self.assertEqual([len(x['questions']) for x in parts],[8]*12+[4]);self.assertEqual([x for p in parts for x in p['questions']],ids);self.assertEqual(parts[-1]['question_start'],96);self.assertEqual(parts[-1]['question_end'],100)
    def test_actual_frozen_protocol_partitions_match_original_ids(self):
        protocol=json.loads((Path(__file__).resolve().parent.parent/'proof/protocol.proposed.json').read_bytes())
        scopes=json.loads(Path(protocol['frozen_question_scopes']['path']).read_bytes())
        self.assertEqual([part['questions'] for part in protocol['partitions']],[8]*12+[4])
        self.assertEqual(protocol['partitions'],[dict(part,questions=len(part['questions'])) for part in partitions([q['id'] for q in scopes['questions']])])
    def test_pure_packet_producer_matches_actual_protocol_without_native_execution(self):
        from prepare_packets import prepare,HERE
        from run_controller import validate_controller
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            root=Path(folder);work=root/'fixture';work.mkdir();(work/'ready.json').write_text('{}')
            protocol=pin(HERE.parent/'proof/protocol.proposed.json')
            release={'schema':'airk.private-selected-Context-FocusON-CAL-capture-release/1','approved':True,'Context_task_forward':False,'maximum_questions':100,'maximum_probes':500,'chunk_protocol_sha256':protocol['sha256'],'original_fixture_bound':pin(self.bound),'paths':{'fixture_ready':str(work/'ready.json')}}
            release_path=root/'release.json';release_path.write_text(json.dumps(release))
            metadata=root/'packets';execution=root/'execution';image=Path(__file__)
            reference=prepare(pin(release_path),protocol,pin(self.bound),image,image,image,metadata,execution)
            packet=json.loads(Path(reference['path']).read_bytes())
            validate_controller(packet,{'schema':'airk.private-selected-cal-chunk-controller-decision/1','approved':True})
            self.assertEqual(len(packet['phase_packets']),13)
            first=json.loads(Path(packet['phase_packets'][0]['path']).read_bytes());last=json.loads(Path(packet['phase_packets'][-1]['path']).read_bytes())
            self.assertEqual(len(first['questions']),8);self.assertEqual(len(last['questions']),4);self.assertFalse(execution.exists())
    def test_generated_phase_packets_integrate_independent_proof_consumer(self):
        import sys
        from prepare_packets import prepare,HERE
        proof=HERE.parent/'proof';sys.path.insert(0,str(proof))
        from test_proof import ProofFixture
        from chunk_contract import validate_members
        with tempfile.TemporaryDirectory(dir=HERE) as folder:
            root=Path(folder);f=ProofFixture(root);release=json.loads(Path(f.release['path']).read_bytes())
            owner=HERE.parents[5]/'scripts/provision-static-onnx-runtime.py'
            controller=prepare(f.release,f.protocol,f.bound,Path(__file__),Path(__file__),owner,root/'plans',root/'execution')
            control=json.loads(Path(controller['path']).read_bytes());decision=f.put('new-decision.json',{'schema':'airk.private-selected-cal-chunk-controller-decision/1','approved':True,'controller_packet_sha256':controller['sha256']})
            members=[]
            for index,oldref in enumerate(f.members):
                member=json.loads(Path(oldref['path']).read_bytes());capture=json.loads(Path(member['capture']['path']).read_bytes());capture['release_sha256']=f.release['sha256'];capture_ref=f.put('capture'+str(index)+'.json',capture)
                phase_ref=control['phase_packets'][index];phase=json.loads(Path(phase_ref['path']).read_bytes())
                member.update(controller_packet=controller,decision=decision,packet=phase_ref,capture=capture_ref,input_seals=merge_seals(release['input_pins'],phase['input_pins'],{phase[k]['path']:phase[k]['sha256'] for k in ('runtime_python','capture_driver','owner','node')}))
                members.append(f.put('execution'+str(index)+'.json',member))
            checked=validate_members(f.protocol,controller,decision,members)
            self.assertEqual(len(checked['reports']),13);self.assertEqual(len(checked['ids']),100)
    def test_repeated_identical_seals_preserved_and_conflicts_refuse(self):
        self.assertEqual(merge_seals({'same':'actual'},{'same':'actual','other':'retained'}),{'same':'actual','other':'retained'})
        with self.assertRaises(ValueError):merge_seals({'same':'original'},{'same':'foreign'})
    def test_duplicate_or_partial_census_refuses(self):
        for ids in ([str(i) for i in range(99)],['duplicate']*100):
            with self.assertRaises(ValueError):partitions(ids)
    def test_first_failure_stops_without_next_phase_or_retry(self):
        seen=[]
        def run(index):seen.append(index);return {'complete':index==0,'stopped':False}
        self.assertEqual(len(list(walk_phases(range(13),run))),2);self.assertEqual(seen,[0,1])
    def test_cooperative_stop_is_not_a_complete_batch_or_resume(self):
        seen=[]
        def run(index):seen.append(index);return {'complete':False,'stopped':True}
        self.assertEqual(len(list(walk_phases(range(13),run))),1);self.assertEqual(seen,[0])
    def test_phase_deadline_is_original_1800(self):
        boundary=self.boundary();self.mono=1800
        with self.assertRaises(ValueError):boundary.verify_global()
    def test_original_global_expiry_is_not_reset_by_new_phase(self):
        self.wall=11800
        with self.assertRaises(ValueError):self.boundary().verify_global()
    def test_backward_wallclock_does_not_extend_original_monotonic_bound(self):
        self.wall=11790;boundary=self.boundary();self.wall=1000;self.mono=11
        with self.assertRaises(ValueError):boundary.verify_global()
    def test_original_start_cannot_be_relabelled(self):
        value=json.loads(self.bound.read_bytes());value['original_start_unix_ms']+=1;self.bound.write_text(json.dumps(value))
        with self.assertRaises(ValueError):global_deadline(pin(self.bound))
    def test_stop_requires_exact_genuine_fixture_metadata_root_and_plain_file(self):
        work=self.root/'work';work.mkdir();ready=work/'ready.json';ready.write_text('{}');release=self.root/'release';release.write_text(json.dumps({'paths':{'fixture_ready':str(ready)}}))
        self.packet.update(release=pin(release),stop_file=str(work/'stop'));boundary=self.boundary();self.assertFalse(boundary.stop_requested());(work/'stop').write_bytes(b'');self.assertTrue(boundary.stop_requested())
        self.packet['stop_file']=str(self.root/'foreign-stop')
        with self.assertRaises(ValueError):self.boundary().stop_requested()
    def test_owner_phase_limit_cannot_be_widened(self):
        self.packet['owner_seconds']=1801
        with self.assertRaises(ValueError):self.boundary()
    def test_admission_refuses_without_original_call_and_closure_headroom(self):
        boundary=self.boundary();self.mono=1750
        with self.assertRaises(ValueError):boundary.require_budget(60)
if __name__=='__main__':unittest.main()
