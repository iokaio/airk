# SPDX-License-Identifier: Apache-2.0
"""Add whole-phase timing to an unchanged retained r1 launcher proposal."""
from pathlib import Path
import difflib
import hashlib
import json

HERE=Path(__file__).resolve().parent
PARENT=HERE.with_name("native-parent-bag-native-run-preparation-r1")

def once(text,old,new):
    if text.count(old)!=1:raise ValueError("nonunique whole-phase amendment: "+old[:100])
    return text.replace(old,new)

def main():
    original=(PARENT/"run_native.py").read_text()
    runner=once(original,"def main():",'''class PhaseDeadline:
    """One original whole-phase deadline; injected clock is only a cheap test seam."""
    def __init__(self, seconds=1800, clock=time.monotonic):
        if seconds != 1800: raise ValueError("original1800second whole-phase cap required")
        self.clock=clock;self.started=clock();self.deadline=self.started+seconds
    def remaining(self):
        remaining=self.deadline-self.clock()
        if not 0 < remaining <= 1800: raise TimeoutError("original native whole-phase deadline expired or clock regressed")
        return remaining
    def elapsed(self):return self.clock()-self.started
    def within(self):return 0 <= self.elapsed() < 1800

def main():
    phase=PhaseDeadline()''')
    runner=once(runner,'    if sha(a.packet)!=a.packet_sha256:', '    phase.remaining()\n    if sha(a.packet)!=a.packet_sha256:')
    runner=once(runner,'        if sha(name)!=pin:raise ValueError("native input/compiler/artifact drift: "+name)',
        '        phase.remaining()\n        if sha(name)!=pin:raise ValueError("native input/compiler/artifact drift: "+name)\n        phase.remaining()')
    runner=once(runner,'capture_output=True,text=True,timeout=10)',
        'capture_output=True,text=True,timeout=min(10,phase.remaining()))\n    phase.remaining()')
    runner=once(runner,'anchor_before=anchor_support.create(packet,pins,anchor)',
        'anchor_before=anchor_support.create(packet,pins,anchor,phase=phase)\n        phase.remaining()')
    runner=once(runner,'env.update(environment);started=time.monotonic()', 'env.update(environment)')
    runner=runner.replace('anchor_support.observe(packet,pins,anchor)',
        'anchor_support.observe(packet,pins,anchor,phase=phase)')
    runner=once(runner,'        members,active,total=native.job_snapshot(api,job)',
        '        phase.remaining()\n        members,active,total=native.job_snapshot(api,job)')
    runner=once(runner,'        return {"members":members,"active":active,"total":total,"image":native.image_proof(api,info.process,packet["binary"])}',
        '        proof={"members":members,"active":active,"total":total,"image":native.image_proof(api,info.process,packet["binary"])}\n        phase.remaining()\n        return proof')
    runner=once(runner,'native.run_windows_owned_training(argv,ROOT,log,1800,env,stdin,8589934592,before_resume=suspended)',
        'native.run_windows_owned_training(argv,ROOT,log,phase.remaining(),env,stdin,8589934592,before_resume=suspended)')
    runner=once(runner,'        anchor_support.require_same(anchor_before,anchor_after)',
        '        anchor_support.require_same(anchor_before,anchor_after)\n        phase.remaining()')
    runner=once(runner,'    result={"owned":owned,',
        '    complete=complete and phase.within()\n    result={"phase_seconds":1800,"phase_expired":not phase.within(),"owned":owned,')
    runner=once(runner,'{"complete":False,"native_started":False,',
        '{"complete":False,"native_started":False,"phase_seconds":1800,"phase_expired":not phase.within(),"elapsed_seconds":phase.elapsed(),')
    runner=once(runner,'{"owned":owned,"complete":False,"failure":failure,',
        '{"owned":owned,"complete":False,"phase_seconds":1800,"phase_expired":not phase.within(),"elapsed_seconds":phase.elapsed(),"failure":failure,')
    runner=runner.replace('"elapsed_seconds":time.monotonic()-started', '"elapsed_seconds":phase.elapsed()')
    runner=once(runner,'(work/"execution.json").write_text(json.dumps(result,indent=2)+"\\n");print(json.dumps(result))',
        '''(work/"execution.json").write_text(json.dumps(result,indent=2)+"\\n")
    if not phase.within():
        complete=False;result.update(complete=False,phase_expired=True,elapsed_seconds=phase.elapsed())
        (work/"execution.json").write_text(json.dumps(result,indent=2)+"\\n")
    print(json.dumps(result))''')
    (HERE/"run_native.py").write_text(runner)
    source=(PARENT/"anchor_runtime.py").read_text()
    amended=once(source,'def observe(packet, pins, path, create=False):','def observe(packet, pins, path, create=False, phase=None):')
    amended=once(amended,'timeout=30)', 'timeout=min(30,phase.remaining()) if phase is not None else 30)\n    if phase is not None:phase.remaining()')
    amended=once(amended,'def create(packet, pins, path):','def create(packet, pins, path, phase=None):')
    amended=once(amended,'produced = observe(packet, pins, path, True)','produced = observe(packet, pins, path, True, phase=phase)')
    amended=once(amended,'checked = observe(packet, pins, path)','checked = observe(packet, pins, path, phase=phase)')
    (HERE/"anchor_runtime.py").write_text(amended)
    patch="".join(difflib.unified_diff(original.splitlines(True),runner.splitlines(True),fromfile="retained-r1/run_native.py",tofile="whole-phase-r2/run_native.py"))
    patch+="".join(difflib.unified_diff(source.splitlines(True),amended.splitlines(True),fromfile="retained-r1/anchor_runtime.py",tofile="whole-phase-r2/anchor_runtime.py"))
    (HERE/"whole-phase-amendment.diff").write_text(patch)
    report={"parent_review_receipt_sha256":hashlib.sha256((PARENT/"preparation-review-r1.json").read_bytes()).hexdigest(),
            "parent_runner_sha256":hashlib.sha256((PARENT/"run_native.py").read_bytes()).hexdigest(),
            "new_runner_sha256":hashlib.sha256((HERE/"run_native.py").read_bytes()).hexdigest(),
            "whole_phase_seconds":1800,"public_mount":False,"native_executed":False}
    (HERE/"whole-phase-review.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report))

if __name__=="__main__":main()
