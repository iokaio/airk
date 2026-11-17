# SPDX-License-Identifier: Apache-2.0

from pathlib import Path
import datetime,hashlib,importlib.util,json,os,subprocess,sys,time
root=Path(r"D:\code\Github\airk")
base=root/"artifacts/v190/final-plan/exec-20261116T052942Z"
packet=base/"gpu-runtime-strategy-proposal-r1/gpu-provision-owner-proposal-r8"
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
for name,expected in {
 "preparation.json":"cb8588eaa6bd699a6a477bbb1129f68d9b7b33ea61cbc035dd52167f25c85ce0",
 "provision_cuda_runtime.py":"c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a",
 "cuda-runtime-producer-lock.json":"549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d"
}.items():assert sha(packet/name)==expected,"reviewed GPU input changed: "+name
sid=subprocess.check_output(["C:/Windows/System32/whoami.exe","/user","/fo","csv","/nh"],text=True)
assert "S-1-5-21-3678370870-3858642417-826968105-1002" in sid
destination=Path(r"C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\cu8")
assert not destination.exists(),"fresh output already exists"
out=base/"gpu-combined-runtime-build-r3"
assert not out.exists(),"original execution directory exists"
out.mkdir()
lock=json.loads((packet/"cuda-runtime-producer-lock.json").read_bytes())
owner_path=root/"scripts/provision-static-onnx-runtime.py"
assert sha(owner_path)==lock["cpu_owner_sha256"]
spec=importlib.util.spec_from_file_location("owned_cpu_runtime",owner_path)
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
argv=[sys.executable,"-S","-B",str(packet/"provision_cuda_runtime.py"),"--destination",str(destination),"--approved-script-sha256","c2f9c3e305106469d6a58699eaa36ec2afed7a6cd4be63275498fd78af2d3f4a","--approved-lock-sha256","549be258fd21be030f994f6da7f74146a9e9edd3bb611bee83305d9a4deade6d","--execute-build","--timeout-seconds","10800"]
env=dict(os.environ);env.update(PYTHONDONTWRITEBYTECODE="1",PYTHONIOENCODING="utf-8",PYTHONNOUSERSITE="1")
started=time.monotonic()
r={"schema":"airk.private.combined-cuda-runtime-execution/1","started_utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"argv":argv,"actual_destination":str(destination),"destination_decision":"cu8 matches locked descriptive fresh_child; R8 source/lock actual identities; failed cu3/cu6 preserved; all51 capturedcacheflags+configuredtargets reviewed","producer_preparation_sha256":sha(packet/"preparation.json"),"first_model_gate":"FP32-unfused-TF32off; no model execution in producer","combined_kernels_available_not_FP16_qualification":True,"outer_timeout_seconds":10800,"passed":False}
(out/"start.json").write_text(json.dumps(r,indent=2)+"\n",encoding="utf-8")
try:
 result=owner.run_owned(argv,root,out/"owned-producer.log",10800,env)
 r["owned"]=result
 native=destination/"gpu-build-receipt.json"
 if native.exists():
  body=json.loads(native.read_bytes());r["native_receipt"]={"path":str(native),"sha256":sha(native),"status":body.get("status"),"error":body.get("error")}
 else:body={}
 r["passed"]=(owner.successful(result) and body.get("status")=="PASS_PRODUCER_ONLY_UNQUALIFIED")
 if not r["passed"]:r["failure"]="Actual producer or ownership failed; preserve source/logs/first failure; no automatic retry"
except BaseException as error:r["failure"]={"type":type(error).__name__,"message":str(error)}
finally:
 r["elapsed_seconds"]=time.monotonic()-started;r["ended_utc"]=datetime.datetime.now(datetime.timezone.utc).isoformat()
 (out/"execution.json").write_text(json.dumps(r,indent=2)+"\n",encoding="utf-8")
print(json.dumps({"output":str(out),"passed":r["passed"],"native_receipt":r.get("native_receipt"),"failure":r.get("failure")}))
sys.exit(0 if r["passed"] else 1)
