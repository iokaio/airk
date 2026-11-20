# SPDX-License-Identifier: Apache-2.0
"""CPU-only kernel controls for the bounded, same-user WSL2 scorer."""
import errno
import json
import mmap
import os
from pathlib import Path
import resource
import subprocess
import sys
import threading
import unittest

from fp16_process import bind_process, process_memory

MIB = 1 << 20


def control(parent):
    bind_process(parent, 256 * MIB)
    before = process_memory(os.getpid())
    threads = []
    thread = threading.Thread(target=lambda: threads.append(os.getpid()))
    thread.start()
    thread.join()
    if threads != [os.getpid()]:
        raise ValueError("same-address-space worker thread unavailable")
    try:
        child = os.fork()
    except OSError as exc:
        if exc.errno != errno.EPERM:
            raise
    else:
        if child == 0:
            os._exit(1)
        os.waitpid(child, 0)
        raise RuntimeError("kernel allowed a child address space")
    try:
        extra = mmap.mmap(-1, 257 * MIB, flags=mmap.MAP_PRIVATE, prot=mmap.PROT_READ)
    except OSError as exc:
        if exc.errno != errno.ENOMEM:
            raise
    else:
        extra.close()
        raise RuntimeError("kernel failed to enforce RLIMIT_AS on read-only VA")
    try:
        subprocess.run([sys.executable, "-V"], capture_output=True, check=True)
    except OSError as exc:
        if exc.errno != errno.EPERM:
            raise
    else:
        raise RuntimeError("kernel allowed a subprocess address space")
    # DATA covers private writable data mappings, but a read-only VA range
    # remains possible below the separately measured AS limit.
    resource.setrlimit(resource.RLIMIT_DATA, (32 * MIB, 32 * MIB))
    try:
        extra = mmap.mmap(-1, 40 * MIB, flags=mmap.MAP_PRIVATE,
                          prot=mmap.PROT_READ | mmap.PROT_WRITE)
    except OSError as exc:
        if exc.errno != errno.ENOMEM:
            raise
    else:
        extra.close()
        raise RuntimeError("RLIMIT_DATA did not deny oversized private writable mapping")
    with mmap.mmap(-1, 40 * MIB, flags=mmap.MAP_PRIVATE, prot=mmap.PROT_READ) as readonly:
        if readonly[0] != 0:
            raise ValueError("anonymous read-only mapping differs")
    fd = os.memfd_create("airk-fp16-control", os.MFD_CLOEXEC)
    try:
        for _ in range(8):
            os.write(fd, bytes(MIB))
        blocks = os.fstat(fd).st_blocks * 512
        after = process_memory(os.getpid())
    finally:
        os.close(fd)
    if blocks != 8 * MIB:
        raise ValueError("unmapped memfd backing size differs")
    return {"host_limit": resource.getrlimit(resource.RLIMIT_AS),
            "thread_allowed": True, "fork_eperm": True, "large_readonly_mmap_enomem": True,
            "unmapped_memfd_backing_bytes": blocks, "before": before, "after": after,
            "subprocess_eperm": True, "data_limit": 32 * MIB,
            "private_writable_40mib_enomem": True, "readonly_40mib_succeeded": True}


class LinuxProcess(unittest.TestCase):
    def test_guarded_exec_retains_pid_limit_and_thread_creation(self):
        program = ("import os,resource,threading,json;"
                   "thread=threading.Thread(target=lambda:None);thread.start();thread.join();"
                   "print(json.dumps([os.getpid(),resource.getrlimit(resource.RLIMIT_AS),"
                   "len(os.listdir('/proc/self/task'))]))")
        guard = Path(__file__).with_name("fp16_process.py")
        with open(sys.executable, "rb") as image:
            cmd = [sys.executable, "-IB", str(guard), str(os.getpid()), str(256 * MIB),
                   str(image.fileno()), sys.executable, "-IB", "-c", program]
            with subprocess.Popen(cmd, pass_fds=(image.fileno(),), stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True) as proc:
                out, err = proc.communicate(timeout=10)
                self.assertEqual(proc.returncode, 0, err)
                self.assertEqual(json.loads(out), [proc.pid, [256 * MIB, 256 * MIB], 1])

    def test_single_address_space_and_mapped_memory_limit(self):
        cmd = [sys.executable, "-B", str(Path(__file__).resolve()), "--control", str(os.getpid())]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10, check=True)
        facts = json.loads(result.stdout)
        self.assertEqual(facts["host_limit"], [256 * MIB, 256 * MIB])
        self.assertTrue(facts["thread_allowed"] and facts["fork_eperm"])
        self.assertTrue(facts["large_readonly_mmap_enomem"])
        self.assertEqual(facts["unmapped_memfd_backing_bytes"], 8 * MIB)
        print(json.dumps(facts, sort_keys=True))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--control":
        print(json.dumps(control(int(sys.argv[2]))))
    else:
        unittest.main()
