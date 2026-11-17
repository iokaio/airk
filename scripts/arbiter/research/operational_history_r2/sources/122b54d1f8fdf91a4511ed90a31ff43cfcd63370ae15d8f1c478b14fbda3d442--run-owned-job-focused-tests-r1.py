# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,io,json,time,unittest
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/receipts/owned-job-event-drain-after-r1';out.mkdir(exist_ok=False)
paths=[root/'scripts/provision-static-onnx-runtime.py',root/'scripts/tests/test_static_onnx_provisioning.py'];before={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
spec=importlib.util.spec_from_file_location('test_static_owner_review',paths[1]);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
names=['StaticOrtPolicyTests.test_failed_or_unsettled_stage_cannot_qualify','OwnedObserverPolicyTests','OwnedStaticBuildProcessTests'];suite=unittest.TestSuite(unittest.defaultTestLoader.loadTestsFromName(n,m) for n in names);count=suite.countTestCases();assert count==8,count
stream=io.StringIO();start=time.monotonic();r=unittest.TextTestRunner(stream=stream,verbosity=2).run(suite);elapsed=time.monotonic()-start;(out/'tests.log').write_text(stream.getvalue(),encoding='utf-8')
after={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths};assert before==after
receipt={'schema':'airk.private-script-test-receipt/1','selections':names,'selected':count,'passed':r.testsRun-len(r.failures)-len(r.errors)-len(r.skipped),'failed':len(r.failures),'errors':len(r.errors),'skipped':len(r.skipped),'elapsed_seconds':elapsed,'source_sha256':before,'inputs_unchanged':True,'log_sha256':hashlib.sha256((out/'tests.log').read_bytes()).hexdigest(),'scope':'Actual Windows ownedJob normalhiddenconsole+waitedobserver/no-forcing, blocked-orphanneverqualifies, timeouttree settles; observerduration/policyrefusals. No corpus/alignment/model/native/Cargo/network execution.'}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2));print(stream.getvalue());raise SystemExit(0 if r.wasSuccessful() else 1)
