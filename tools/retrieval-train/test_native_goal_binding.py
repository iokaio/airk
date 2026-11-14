# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

from native_goal_client import NativeGoalClient


class NativeFixtureBindingTests(unittest.TestCase):
    def test_live_health_detects_same_process_namespace_or_instance_change(self):
        client=NativeGoalClient.__new__(NativeGoalClient)
        client.images={'airkctl':Path('test-client')};client.control=Path('test-control')
        health={'instance':'original','process_id':42,'executable_sha256':'a'*64,
                'catalog_clock':{'namespace':'original-catalog'}}
        client.service_identity={'actual_native_health':health}
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            good=SimpleNamespace(returncode=0,stdout=json.dumps({'status':'health','observation':health}).encode(),stderr=b'')
            with patch('native_goal_client.subprocess.run',return_value=good):
                self.assertEqual(client.observe_health(root,'before',1)['full_control_response']['observation'],health)
            for field in ('instance','namespace','process_id'):
                changed=json.loads(json.dumps(health))
                if field=='instance':changed['instance']='replacement'
                elif field=='namespace':changed['catalog_clock']['namespace']='replacement-catalog'
                else:changed['process_id']=43
                value=SimpleNamespace(returncode=0,stdout=json.dumps({'status':'health','observation':changed}).encode(),stderr=b'')
                with patch('native_goal_client.subprocess.run',return_value=value),self.assertRaisesRegex(ValueError,'instance/namespace'):
                    client.observe_health(root,'changed-'+field,1)

    def test_actual_fixture_binary_is_distinct_and_pid_reuse_refused(self):
        repository = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory(dir=repository/'artifacts') as directory:
            root=Path(directory);pair=root/'pair';pair.mkdir();control=root/'control';control.mkdir()
            pins={}
            for name in ('airk','airkctl'):
                data=name.encode();(pair/(name+'.exe')).write_bytes(data)
                pins[name+'_sha256']=hashlib.sha256(data).hexdigest()
            (pair/'airk-runtime.json').write_text(json.dumps(pins))
            profile=b'opaque native profile bytes'
            profiles=[]
            for index in range(4):
                name=f'profile-{index+1}.json';(root/name).write_bytes(profile)
                profiles.append({'snapshot':name,'profile_sha256':hashlib.sha256(profile).hexdigest()})
            inputs=b'opaque native input bytes';(root/'inputs.json').write_bytes(inputs)
            ready={'schema':'airk.private-arbiter-fixture/1','evaluation':True,'release_qualified':False,
                   'control_root':str(control),'inputs_sha256':hashlib.sha256(inputs).hexdigest(),'profiles':profiles,
                   'health':{'process_id':42,'executable_sha256':'f'*64,'instance':'observed-instance','catalog_clock':{'namespace':'observed-namespace'}}}
            ready_path=root/'ready.json';ready_path.write_text(json.dumps(ready))
            observed={'process_id':42,'executable':'actual-native-test.exe','executable_sha256':'f'*64,'creation_filetime_100ns':1}
            source=root/'source';source.mkdir()
            publication={'token':'original-publication','collection':'original-collection'}
            bindings={'schema':'airk.private-arbiter-index-bindings/1','evaluation':True,'release_qualified':False,
                      'control_root':str(control),'instance':'observed-instance','namespace':'observed-namespace',
                      'bindings':[{'index_id':'opaque-index','source_root':str(source),'publication':publication,
                                   'definition':{'id':'opaque-index','tombstone':False,'source_revision':7,'backing':{'root':str(source),'collection':'original-collection'}},
                                   'source_roots':[{'id':'original-source-root','canonical_path':str(source)}]}]}
            binding_path=root/'bindings.json';binding_path.write_text(json.dumps(bindings))
            with self.assertRaisesRegex(ValueError,'outside frozen source'):
                NativeGoalClient(pair,control,'opaque-index',source/'forbidden-metadata',ready_path,lambda pid:dict(observed),index_bindings=binding_path)
            self.assertFalse((source/'forbidden-metadata').exists())
            client=NativeGoalClient(pair,control,'opaque-index',root/'calls',ready_path,lambda pid:dict(observed),index_bindings=binding_path)
            client.verify()
            payload={'publication':publication,'hits':[],'future_native':{'retained':True},
                     'scope_observation':{'members':[{'index_id':'opaque-index','source_root_ids':['original-source-root'],'source_revision':7}]}}
            def invoke(identifier,tool,args_file,timeout):
                envelope={'content':[{'type':'text','text':json.dumps(payload)}],'structuredContent':payload,'isError':False}
                return {'kind':'invoked','id':identifier,'arguments_sha256':hashlib.sha256(args_file.read_bytes()).hexdigest(),
                        'argv':['tools','call',tool,'--args-file',str(args_file)],
                        'result':{'exitCode':0,'class':'success','stdout':json.dumps(envelope),'stderr':'full diagnostic'}}
            client.session=SimpleNamespace(invoke=invoke,close=lambda:None)
            actual=SimpleNamespace(returncode=0,stdout=json.dumps({'status':'health','observation':ready['health']}).encode(),stderr=b'')
            with patch('native_goal_client.subprocess.run',return_value=actual):
                good=client.call('search',{'query':'original'},1)
            self.assertTrue(good['validated']);self.assertEqual(good['full_canonical_value'],payload)
            self.assertEqual(good['returncode'],0);self.assertNotIn('--index',good['command'])
            payload['publication']={**publication,'token':'foreign-first-publication'}
            with patch('native_goal_client.subprocess.run',return_value=actual):
                bad=client.call('search',{'query':'original'},1)
            self.assertFalse(bad['validated']);self.assertIn('publication',bad['identity_error'])
            self.assertEqual(bad['returncode'],0);self.assertEqual(bad['full_canonical_value'],payload)
            payload['publication']=publication
            changed=json.loads(json.dumps(ready['health']));changed['catalog_clock']['namespace']='replacement-catalog'
            after=SimpleNamespace(returncode=0,stdout=json.dumps({'status':'health','observation':changed}).encode(),stderr=b'')
            with patch('native_goal_client.subprocess.run',side_effect=[actual,after]):
                replaced=client.call('search',{'query':'original'},1)
            self.assertFalse(replaced['validated']);self.assertIn('instance/namespace',replaced['identity_error'])
            self.assertEqual(replaced['returncode'],0);self.assertEqual(replaced['full_canonical_value'],payload)
            self.assertFalse(client.service_identity['running_service_is_matched_pair_airk'])
            self.assertNotEqual(client.service_identity['actual_process_observation']['executable_sha256'],pins['airk_sha256'])
            observed['creation_filetime_100ns']=2
            with self.assertRaisesRegex(ValueError,'PID reused'):client.verify()
            observed['creation_filetime_100ns']=1;(root/'profile-1.json').write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError,'profile drift'):client.verify()


if __name__=='__main__':unittest.main()
