# SPDX-License-Identifier: Apache-2.0
"""Exercise the WSL boot admission with recorded outputs. No wsl or pwsh process is started."""
import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import wsl_boot_admission as boot

SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
WSL = "C:\\Windows\\System32\\wsl.exe"
PREFIX = [WSL, "--distribution", "Ubuntu", "--user", "tyler"]
SCRIPT = ("id; uname -r; df -B1 / /mnt/c /mnt/d; cat /proc/swaps; stat -c '%u:%g %a %n' /home/tyler "
          "/home/tyler/airk-builds/arbiter-final-linux-r1 /home/tyler/airk-builds/arbiter-final-linux-r1/target; "
          "nproc; lscpu -e=CPU,CORE,SOCKET; free -b; pgrep -a 'ollama|cargo|rustc|nextest' || true")
# The first eleven lines are the guest lines of the retained linux-signed-r7-cpu-resume-boot-r1.json.
GUEST = "\n".join([
    "uid=1000(tyler) gid=1000(tyler) groups=1000(tyler),4(adm),20(dialout),24(cdrom),25(floppy),27(sudo),29(audio),"
    "30(dip),44(video),46(plugdev),100(users),107(netdev),989(ollama)",
    "5.15.167.4-microsoft-standard-WSL2",
    "Filesystem         1B-blocks          Used     Available Use% Mounted on",
    "/dev/sdc       1081101176832   72669507584  953439313920   8% /",
    "C:\\            3978248581120 1335004364800 2643244216320  34% /mnt/c",
    "D:\\            4000768323584 2872390430720 1128377892864  72% /mnt/d",
    "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority",
    "/dev/sdb                                partition\t16777216\t0\t\t-2",
    "1000:1000 750 /home/tyler",
    "1000:1000 700 /home/tyler/airk-builds/arbiter-final-linux-r1",
    "1000:1000 700 /home/tyler/airk-builds/arbiter-final-linux-r1/target",
    "16",
    "CPU CORE SOCKET",
    *("%3d %4d %6d" % (cpu, cpu // 2, 0) for cpu in range(16)),
    "               total        used        free      shared  buff/cache   available",
    "Mem:     101190062080   812345344 99876543488     3125248   501173248 99412344832",
    "Swap:     17179869184           0 17179869184",
    "218 /usr/local/bin/ollama serve",
]) + "\n"
ROW = (b'{"utc":"2026-11-20T13:58:30.6559742+00:00","windows_free":1128990367744,"backing_free":2643336200192,'
       b'"vhd_bytes":79086747648,"windows_start":1233246265344,"backing_start":2669960904704,'
       b'"vhd_start":60649635840,"windows_budget":195421011968,"backing_budget":120000000000,'
       b'"free_floor":107374182400,"within_budget":true,"has_headroom":true,"backing_writes_held":false,'
       b'"original_alert_log_sha256":"9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda",'
       b'"previous_backing_budget":25769803776}')
SWAP = {"path": "C:\\Users\\tdotn\\AppData\\Local\\Temp\\4547E929-518D-4ECA-8C21-9113A32EEE19\\swap.vhdx",
        "length": 37748736}
HOST = {
    "schema": "airk.private.wsl-boot-host/1", "utc": "2026-11-20T14:05:20.0000000+00:00",
    "identity": {"name": "TYAI\\tdotn", "sid": SID, "owner_sid": SID, "session": 1,
                 "interactive_owner_sids": [SID], "elevated": False},
    "c_free": 2643244216320, "d_free": 1128377892864,
    "distributions": [
        {"name": "docker-desktop", "default": False, "default_uid": 0, "version": 2,
         "vhd_path": "C:\\Users\\tdotn\\AppData\\Local\\Docker\\wsl\\main\\ext4.vhdx", "vhd_length": 109051904},
        {"name": "Ubuntu", "default": False, "default_uid": 1000, "version": 2,
         "vhd_path": "C:\\Users\\tdotn\\AppData\\Local\\Packages\\CanonicalGroupLimited.Ubuntu_79rhkp1fndgsc"
                     "\\LocalState\\ext4.vhdx", "vhd_length": 79086747648},
        {"name": "Ubuntu-24.04", "default": True, "default_uid": 0, "version": 2,
         "vhd_path": "E:\\WSL\\Ubuntu-24.04\\ext4.vhdx", "vhd_length": 657896505344}],
    "swap_files": [], "wslconfig": {"path": "C:\\Users\\tdotn\\.wslconfig", "present": True, "memory": "96GB",
                                    "processors": "16", "swap": "16GB"},
    "keep_alive_task": {"name": "WSL LLM Server (keep Ubuntu running)", "state": "Disabled"},
    "processes": [{"name": "ollama", "pid": 59696}],
    "memory": {"physical_available_bytes": 93056598016, "commit_limit_bytes": 145875468288,
               "commit_bytes": 47110377472, "commit_available_bytes": 98765090816},
    "volume_log": {"exists": True, "owner_checked": True}, "receipt": {"exists": False, "owner_checked": False}}


def changed(document, **paths):
    """Copy a nested document and replace the values at dotted paths."""
    document = copy.deepcopy(document)
    for path, value in paths.items():
        *parents, leaf = path.split("__")
        cursor = document
        for name in parents:
            cursor = cursor[int(name) if name.isdigit() else name]
        cursor[int(leaf) if leaf.isdigit() else leaf] = value
    return document


class Commands(unittest.TestCase):
    def test_boot_command_is_the_runbook_command_for_ubuntu_as_tyler(self):
        command = boot.guest_command(["/bin/sh", "-c", boot.GUEST_SCRIPT])
        self.assertEqual(command, [*PREFIX, "--exec", "/bin/sh", "-c", SCRIPT])
        # The Windows command line carries the script as one quoted argument with no escaped quote.
        line = subprocess.list2cmdline(command)
        self.assertEqual(line, WSL + ' --distribution Ubuntu --user tyler --exec /bin/sh -c "' + SCRIPT + '"')
        self.assertEqual(boot.RUNNING, [WSL, "--list", "--running", "--quiet"])

    def test_gate_command_keeps_the_same_identity_prefix(self):
        gate = ["/usr/bin/python3", "-B", "-S", "scripts/arbiter/evaluation/linux_cuda/maintained_gate.py",
                "--gate", "inspect"]
        self.assertEqual(boot.guest_command(gate, cd="/mnt/d/code/Github/airk"),
                         [*PREFIX, "--cd", "/mnt/d/code/Github/airk", "--exec", *gate])

    def test_bare_wsl_other_distribution_user_and_management_verbs_refuse(self):
        true = ["--exec", "/bin/true"]
        for command in (
                [], ["wsl", *PREFIX[1:], *true], ["wsl.exe", *PREFIX[1:], *true],
                ["C:\\Windows\\Sysnative\\wsl.exe", *PREFIX[1:], *true],
                [WSL], [WSL, *true], [WSL, "--distribution", "Ubuntu", *true], [WSL, "--user", "tyler", *true],
                [WSL, "--distribution", "Ubuntu-24.04", "--user", "tyler", *true],
                [WSL, "--distribution", "docker-desktop", "--user", "tyler", *true],
                [WSL, "--distribution", "ubuntu", "--user", "tyler", *true],
                [WSL, "--distribution", "Ubuntu", "--user", "root", *true],
                [WSL, "--distribution", "Ubuntu", "--user", "0", *true],
                [WSL, "-d", "Ubuntu", "-u", "tyler", *true],
                [WSL, "--user", "tyler", "--distribution", "Ubuntu", *true],
                [WSL, "--shutdown"], [WSL, "--terminate", "Ubuntu"], [*PREFIX, "--shutdown"],
                [*PREFIX, "--system", *true], [*PREFIX, "--", "ls"], [*PREFIX, "ls"], [*PREFIX, "--exec"],
                [*PREFIX, "--exec", "python3"], [*PREFIX, "--cd", "scripts", *true],
                [*PREFIX, "--cd", "/mnt/d/../c", *true], [*PREFIX, "--cd"]):
            with self.subTest(command=command), self.assertRaises(ValueError):
                boot.check_command(command)
        for options in ({"distribution": "Ubuntu-24.04"}, {"user": "root"}, {"cd": "relative"}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                boot.guest_command(["/bin/true"], **options)

    def test_privilege_and_package_commands_refuse(self):
        for argv in (["/usr/bin/sudo", "-n", "true"], ["/bin/su", "-"], ["/bin/sh", "-c", "sudo apt-get update"],
                     ["/bin/sh", "-c", "id;su root"], ["/bin/sh", "-c", "id && /usr/bin/sudo -n true"],
                     ["/bin/sh", "-c", "echo $(sudo id)"], ["/usr/bin/nice", "sudo", "true"],
                     ["/usr/bin/apt", "install", "gcc"], ["/bin/sh", "-c", "'su' -c id"],
                     ["/usr/bin/env", "doas", "id"], ["/usr/bin/pkexec", "id"]):
            with self.subTest(argv=argv), self.assertRaisesRegex(ValueError, "privilege and package"):
                boot.guest_command(argv)
        # Words that merely contain those letters are ordinary arguments.
        boot.guest_command(["/bin/sh", "-c", "ls /home/tyler/results; echo sum super"])

    def test_only_three_command_shapes_start(self):
        observer = ["C:\\pwsh.exe", "-NoProfile", "-NonInteractive", "-File",
                    str(Path(boot.__file__).resolve().parent / "wsl_boot_host.ps1"), "-ExpectedSid", SID]
        boot.permitted(observer, "C:\\pwsh.exe")
        boot.permitted(boot.RUNNING, "C:\\pwsh.exe")
        boot.permitted([*PREFIX, "--exec", "/bin/true"], "C:\\pwsh.exe")
        for command in ([WSL, "--list"], [WSL, "--shutdown"], ["wsl", "--list", "--running", "--quiet"],
                        ["C:\\pwsh.exe", "-NoProfile", "-Command", "wsl"],
                        ["C:\\other.exe", *observer[1:]], ["cmd.exe", "/c", "wsl"]):
            with self.subTest(command=command), self.assertRaises(ValueError):
                boot.permitted(command, "C:\\pwsh.exe")


class Parsing(unittest.TestCase):
    REGISTERED = ["docker-desktop", "Ubuntu", "Ubuntu-24.04"]

    def test_running_distributions_in_either_encoding(self):
        self.assertEqual(boot.parse_running("Ubuntu\r\n".encode("utf-16-le"), 0, self.REGISTERED), ["Ubuntu"])
        self.assertEqual(boot.parse_running(b"\xef\xbb\xbfUbuntu\n", 0, self.REGISTERED), ["Ubuntu"])
        self.assertEqual(boot.parse_running(b"Ubuntu\r\nUbuntu-24.04\r\n", 0, self.REGISTERED),
                         ["Ubuntu", "Ubuntu-24.04"])
        self.assertEqual(boot.parse_running(b"", 0, self.REGISTERED), [])
        quiet = "There are no running distributions.\r\n".encode("utf-16-le")
        self.assertEqual(boot.parse_running(quiet, 4294967295, self.REGISTERED), [])
        self.assertEqual(boot.parse_running(b"There are no running distributions.\n", 1, self.REGISTERED), [])

    def test_unrecognized_listings_refuse(self):
        for raw, code in ((b"", 1), (b"Debian\n", 0), (b"Ubuntu\n", 1), (b"Ubuntu\nUbuntu\n", 0),
                          (b"Wsl/Service/E_FAIL\n", 4294967295), (b"Ubuntu\nUsage: wsl.exe [Argument]\n", 0)):
            with self.subTest(raw=raw, code=code), self.assertRaisesRegex(ValueError, "unrecognized listing"):
                boot.parse_running(raw, code, self.REGISTERED)

    def test_guest_output_is_read_into_the_retained_fields(self):
        guest = boot.parse_guest(GUEST)
        self.assertEqual(guest["identity"], {
            "uid": 1000, "user": "tyler", "gid": 1000, "group": "tyler",
            "groups": ["tyler", "adm", "dialout", "cdrom", "floppy", "sudo", "audio", "dip", "video", "plugdev",
                       "users", "netdev", "ollama"]})
        self.assertEqual(guest["kernel"], "5.15.167.4-microsoft-standard-WSL2")
        self.assertEqual(guest["filesystems"], {
            "/": {"filesystem": "/dev/sdc", "total": 1081101176832, "used": 72669507584, "available": 953439313920},
            "/mnt/c": {"filesystem": "C:\\", "total": 3978248581120, "used": 1335004364800,
                       "available": 2643244216320},
            "/mnt/d": {"filesystem": "D:\\", "total": 4000768323584, "used": 2872390430720,
                       "available": 1128377892864}})
        self.assertEqual(guest["swaps"], [{"name": "/dev/sdb", "type": "partition", "size_bytes": 17179869184,
                                           "used_bytes": 0}])
        self.assertEqual(guest["paths"], {
            "/home/tyler": {"uid": 1000, "gid": 1000, "mode": "750"},
            "/home/tyler/airk-builds/arbiter-final-linux-r1": {"uid": 1000, "gid": 1000, "mode": "700"},
            "/home/tyler/airk-builds/arbiter-final-linux-r1/target": {"uid": 1000, "gid": 1000, "mode": "700"}})
        self.assertEqual((guest["nproc"], len(guest["cpus"]), guest["distinct_cores"]), (16, 16, 8))
        self.assertEqual(guest["cpus"][15], {"cpu": 15, "core": 7, "socket": 0})
        self.assertEqual(guest["memory"], {"total": 101190062080, "available": 99412344832,
                                           "swap_total": 17179869184, "swap_used": 0})
        self.assertEqual(guest["processes"], [{"pid": 218, "command": "/usr/local/bin/ollama serve"}])
        self.assertEqual(boot.guest_refusals(guest, 8), [])
        self.assertEqual(boot.cargo_owners(guest), [])

    def test_missing_or_extra_guest_sections_refuse(self):
        lines = GUEST.splitlines()
        for broken in ("\n".join(lines[1:]), "\n".join(lines[:5] + lines[6:]), "\n".join(lines[:11] + lines[12:]),
                       "\n".join(lines[:9] + lines[10:]), GUEST + "unexpected trailer", "",
                       GUEST.replace("CPU CORE SOCKET", "CPU CORE"), GUEST.replace("Swap:", "Swp:")):
            with self.subTest(broken=broken[:40]), self.assertRaisesRegex(ValueError, "unexpected guest boot output"):
                boot.parse_guest(broken)

    def test_guest_identity_topology_swap_and_cargo_refusals(self):
        root = GUEST.replace("uid=1000(tyler) gid=1000(tyler) groups=1000(tyler),", "uid=0(root) gid=0(root) groups=0(root),")
        self.assertEqual(boot.guest_refusals(boot.parse_guest(root), 8),
                         ["guest identity is not tyler with UID and GID 1000"])
        other = GUEST.replace("uid=1000(tyler)", "uid=1001(tyler)")
        self.assertEqual(boot.guest_refusals(boot.parse_guest(other), 8),
                         ["guest identity is not tyler with UID and GID 1000"])
        renamed = GUEST.replace("uid=1000(tyler)", "uid=1000(build)")
        self.assertEqual(boot.guest_refusals(boot.parse_guest(renamed), 8),
                         ["guest identity is not tyler with UID and GID 1000"])
        self.assertEqual(boot.guest_refusals(boot.parse_guest(GUEST.replace("1000:1000 700 /home/tyler/airk-builds/"
                         "arbiter-final-linux-r1/target", "0:0 700 /home/tyler/airk-builds/arbiter-final-linux-r1/target")), 8),
                         ["guest path owner or write policy differs: /home/tyler/airk-builds/arbiter-final-linux-r1/target"])
        self.assertEqual(boot.guest_refusals(boot.parse_guest(GUEST.replace("1000:1000 750 /home/tyler",
                         "1000:1000 777 /home/tyler")), 8), ["guest path owner or write policy differs: /home/tyler"])
        self.assertEqual(boot.guest_refusals(boot.parse_guest(GUEST), 9), ["guest has fewer distinct cores than required"])
        self.assertEqual(boot.guest_refusals(boot.parse_guest(GUEST.replace("\n16\n", "\n12\n")), 8),
                         ["guest processor topology is inconsistent"])
        self.assertEqual(boot.guest_refusals(boot.parse_guest(GUEST.replace("\t16777216\t", "\t16777217\t")), 8),
                         ["guest swap exceeds the reserve kept for it on C:"])
        self.assertEqual(boot.guest_refusals(boot.parse_guest(GUEST.replace("-standard-WSL2", "-generic")), 8),
                         ["guest kernel is not the WSL2 kernel"])
        busy = boot.parse_guest(GUEST + "301 /home/tyler/airk-builds/arbiter-final-linux-r1/rustup/toolchains/x/bin/cargo "
                                "clippy\n")
        self.assertEqual(boot.guest_refusals(busy, 8), ["another Cargo owner is active in the guest"])
        self.assertEqual([row["pid"] for row in boot.cargo_owners(busy)], [301])


class Host(unittest.TestCase):
    def test_recorded_host_state_is_accepted(self):
        self.assertEqual(boot.host_refusals(HOST, SID, 12884901888), [])
        self.assertEqual(boot.host_refusals(changed(HOST, swap_files=[SWAP]), SID, 0), [])
        self.assertEqual(boot.HOST_RESERVE, 12884901888)
        facts = boot.volumes(changed(HOST, swap_files=[SWAP]))
        self.assertEqual((facts["c_free"], facts["d_free"], facts["vhd"], facts["swap"]),
                         (2643244216320, 1128377892864, 79086747648, 37748736))
        self.assertEqual(facts["swap_files"], [SWAP])
        self.assertEqual(facts["keep_alive_task"]["state"], "Disabled")

    def test_each_precondition_refuses(self):
        other = "S-1-5-21-3678370870-3858642417-826968105-1003"
        for expected, host in (
                ("Windows identity is not the expected human", changed(HOST, identity__sid=other)),
                ("Windows identity is not the expected human", changed(HOST, identity__owner_sid="S-1-5-32-544")),
                ("Windows identity is not the expected human", changed(HOST, identity__interactive_owner_sids=[other])),
                ("Windows shell is elevated", changed(HOST, identity__elevated=True)),
                ("registered Ubuntu distribution, default UID 1000 or C: virtual disk differs",
                 changed(HOST, distributions__1__default_uid=0)),
                ("registered Ubuntu distribution, default UID 1000 or C: virtual disk differs",
                 changed(HOST, distributions__1__vhd_path="E:\\WSL\\Ubuntu\\ext4.vhdx")),
                ("registered Ubuntu distribution, default UID 1000 or C: virtual disk differs",
                 changed(HOST, distributions__1__name="Ubuntu-22.04")),
                ("registered Ubuntu distribution, default UID 1000 or C: virtual disk differs",
                 changed(HOST, distributions__1__version=1)),
                ("keep-alive task is not Disabled", changed(HOST, keep_alive_task__state="Ready")),
                ("keep-alive task is not Disabled", changed(HOST, keep_alive_task__state="Absent")),
                ("Docker Desktop backend is running",
                 changed(HOST, processes=[{"name": "com.docker.backend", "pid": 4}])),
                ("swap file count, volume or length differs from the C: reserve",
                 changed(HOST, swap_files=[SWAP, dict(SWAP, path="C:\\Users\\tdotn\\AppData\\Local\\Temp\\swap.vhdx")])),
                ("swap file count, volume or length differs from the C: reserve",
                 changed(HOST, swap_files=[dict(SWAP, path="D:\\swap.vhdx")])),
                ("swap file count, volume or length differs from the C: reserve",
                 changed(HOST, swap_files=[dict(SWAP, length=17179869185)])),
                ("Windows physical or commit memory is below the required reserve",
                 changed(HOST, memory__physical_available_bytes=12884901887)),
                ("Windows physical or commit memory is below the required reserve",
                 changed(HOST, memory__commit_available_bytes=12884901887))):
            with self.subTest(expected=expected):
                self.assertEqual(boot.host_refusals(host, SID, 12884901888), [expected])


class Admission(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.campaign = Path(directory.name).resolve()
        patch = mock.patch.object(boot, "CAMPAIGN", self.campaign)
        patch.start()
        self.addCleanup(patch.stop)
        self.log = self.campaign / "host-volume-watch-r8.jsonl"
        self.log.write_bytes(ROW + b"\r\n" + ROW.replace(b"13:58:30.6559742", b"14:05:00.0000000") + b"\r\n")
        self.receipt = self.campaign / "linux-fp16-boot-r1.json"
        self.now = datetime(2026, 11, 20, 14, 5, 30, tzinfo=timezone.utc).timestamp()
        self.before, self.after = copy.deepcopy(HOST), changed(HOST, swap_files=[SWAP], c_free=2643204493312)
        self.listing = [("There are no running distributions.\r\n".encode("utf-16-le"), 4294967295),
                        ("Ubuntu\r\n".encode("utf-16-le"), 0)]
        self.guest = (0, GUEST.encode(), b"")
        self.host_exit = 0
        self.started = []

    def options(self, **changes):
        values = {"expected_sid": SID, "receipt": self.receipt, "reserve_bytes": 536870912, "other_host_bytes": 0,
                  "min_distinct_cores": 8, "plan": False, "volume_log": self.log,
                  "volume_log_first_row_sha256": hashlib.sha256(ROW).hexdigest(), "not_after": "2026-11-20T18:00:00Z"}
        return argparse.Namespace(**(values | changes))

    def launch(self, command, timeout):
        self.started.append(command)
        if command[0] == "C:\\pwsh.exe":
            observations = sum(1 for earlier in self.started if earlier[0] == "C:\\pwsh.exe")
            answer = self.before if observations == 1 else self.after
            return self.host_exit, json.dumps(answer).encode(), b"Effective user does not match the expected human SID"
        if command == boot.RUNNING:
            raw, code = self.listing.pop(0)
            return code, raw, b""
        return self.guest

    def admit(self, **changes):
        options = self.options(**changes)
        return boot.run(options, launch=self.launch, clock=lambda: self.now, pwsh="C:\\pwsh.exe")

    def kinds(self):
        return ["guest" if command[:5] == PREFIX else "list" if command == boot.RUNNING else
                "audit" if command[-1] == "-Created" else "host" for command in self.started]

    def test_admitted_boot_reproduces_the_retained_receipt_fields(self):
        report = self.admit()
        self.assertEqual(self.kinds(), ["host", "list", "guest", "list", "host", "audit"])
        self.assertEqual(self.started[2], [*PREFIX, "--exec", "/bin/sh", "-c", SCRIPT])
        self.assertEqual(self.started[0], [
            "C:\\pwsh.exe", "-NoProfile", "-NonInteractive", "-File",
            str(Path(boot.__file__).resolve().parent / "wsl_boot_host.ps1"), "-ExpectedSid", SID,
            "-VolumeLog", str(self.log), "-ReceiptPath", str(self.receipt)])
        self.assertEqual(self.started[5], self.started[0] + ["-Created"])
        self.assertIs(report["admitted"], True)
        self.assertEqual(report["refusals"], [])
        self.assertEqual(json.loads(self.receipt.read_text(encoding="utf-8")), report)
        self.assertNotIn(b"\r", self.receipt.read_bytes())
        expected = {
            "schema": "airk.private.wsl-boot-admission/1", "sid": SID, "distribution": "Ubuntu", "user": "tyler",
            "registered_uid": 1000, "uid": 1000, "exit_code": 0, "guest_started": True,
            "guest_workload_started": False, "swap_reservation": 17179869184, "guest_vhd_reservation": 536870912,
            "windows_host_reservation": 12884901888, "post_boot_physical": 93056598016,
            "post_boot_virtual": 98765090816, "swap_path": SWAP["path"],
            "vhd_path": HOST["distributions"][1]["vhd_path"], "guest": GUEST.splitlines(), "guest_stderr": [],
            "guest_ollama": [{"pid": 218, "command": "/usr/local/bin/ollama serve"}],
            "headroom": {"guest_growth": 13939441664, "guest_remaining": 11830362112,
                         "vhd_growth": 18437111808, "vhd_remaining": 7332691968,
                         "c_growth": 26756411392, "c_remaining": 76063719424,
                         "d_growth": 104868372480, "d_remaining": 90552639488}}
        self.assertEqual({name: report[name] for name in expected}, expected)
        self.assertEqual((report["before"]["c_free"], report["before"]["d_free"], report["before"]["vhd"],
                          report["before"]["swap"], report["before"]["running_distributions"]),
                         (2643244216320, 1128377892864, 79086747648, 0, []))
        self.assertEqual((report["after"]["c_free"], report["after"]["swap"], report["after"]["swap_files"],
                          report["after"]["running_distributions"]), (2643204493312, 37748736, [SWAP], ["Ubuntu"]))
        self.assertEqual(report["monitor"]["utc"], "2026-11-20T14:05:00.0000000+00:00")
        self.assertEqual(report["volume_policy"]["not_after"], "2026-11-20T18:00:00+00:00")
        self.assertEqual(report["before"]["keep_alive_task"]["state"], "Disabled")

    def refused(self, expected, kinds, **changes):
        report = self.admit(**changes)
        self.assertEqual(report["refusals"], expected)
        self.assertIs(report["admitted"], False)
        self.assertEqual(self.kinds(), kinds)
        self.assertEqual(json.loads(self.receipt.read_text(encoding="utf-8")), report)
        return report

    def test_stale_watcher_starts_no_wsl_command(self):
        self.now += 16
        report = self.refused(["ValueError: volume monitor observation is stale"], ["host", "audit"])
        self.assertIs(report["guest_started"], False)
        self.assertNotIn("guest", report)

    def test_host_preconditions_start_no_wsl_command(self):
        self.before = changed(HOST, keep_alive_task__state="Ready", identity__elevated=True)
        self.refused(["Windows shell is elevated", "keep-alive task is not Disabled"], ["host", "audit"])

    def test_reservation_beyond_the_vhd_cap_starts_no_wsl_command(self):
        self.refused(["VHD growth cap"], ["host", "audit"], reserve_bytes=7332691969)

    def test_another_running_distribution_stops_before_the_guest_command(self):
        self.listing[0] = ("Ubuntu-24.04\r\n".encode("utf-16-le"), 0)
        self.refused(["another distribution is running"], ["host", "list", "audit"])

    def test_unreadable_listing_stops_before_the_guest_command(self):
        self.listing[0] = (b"Wsl/Service/E_FAIL\n", 4294967295)
        self.refused(["ValueError: unrecognized listing of running distributions"], ["host", "list", "audit"])

    def test_wrong_guest_identity_is_recorded_and_refused(self):
        root = GUEST.replace("uid=1000(tyler) gid=1000(tyler) groups=1000(tyler),", "uid=0(root) gid=0(root) groups=0(root),")
        self.guest = (0, root.encode(), b"")
        report = self.refused(["guest identity is not tyler with UID and GID 1000"],
                              ["host", "list", "guest", "list", "host", "audit"])
        self.assertEqual((report["uid"], report["guest"][0][:11]), (0, "uid=0(root)"))

    def test_failed_or_unreadable_guest_command_is_recorded_and_refused(self):
        self.guest = (1, b"", b"The system cannot find the file specified.\n")
        report = self.refused(["guest command exited 1"], ["host", "list", "guest", "list", "host", "audit"])
        self.assertEqual(report["guest_stderr"], ["The system cannot find the file specified."])

    def test_unexpected_guest_output_is_recorded_and_refused(self):
        self.guest = (0, b"uid=1000(tyler) gid=1000(tyler) groups=1000(tyler)\nsurprise\n", b"")
        report = self.refused(["ValueError: unexpected guest boot output: expected the df header at line 3"],
                              ["host", "list", "guest", "list", "host", "audit"])
        self.assertEqual(report["guest"][1], "surprise")

    def test_second_distribution_after_boot_refuses(self):
        self.listing[1] = (b"Ubuntu\nUbuntu-24.04\n", 0)
        self.refused(["only Ubuntu may be running after the boot"], ["host", "list", "guest", "list", "host", "audit"])

    def test_guest_free_space_beyond_its_cap_refuses_after_boot(self):
        self.guest = (0, GUEST.replace("953439313920", "941608951807").encode(), b"")
        self.refused(["guest free-space growth cap"], ["host", "list", "guest", "list", "host", "audit"])

    def test_wrong_windows_identity_writes_nothing(self):
        self.host_exit = 1
        with self.assertRaisesRegex(RuntimeError, "Effective user does not match"):
            self.admit()
        self.assertEqual(self.kinds(), ["host"])
        self.assertEqual(sorted(path.name for path in self.campaign.iterdir()), ["host-volume-watch-r8.jsonl"])

    def test_receipt_name_place_and_freshness(self):
        for path in (self.campaign / "boot.json", self.campaign / "linux-fp16-boot-r0.json",
                     self.campaign / "nested" / "linux-fp16-boot-r1.json", Path("linux-fp16-boot-r1.json"),
                     self.campaign / "linux-FP16-boot-r1.json"):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "directly inside the campaign root"):
                self.admit(receipt=path)
        self.receipt.write_text("{}")
        with self.assertRaisesRegex(ValueError, "fresh boot receipt"):
            self.admit()
        self.assertEqual(self.started, [])

    def test_plan_starts_nothing(self):
        plan = self.admit(plan=True)["plan"]
        self.assertEqual(self.started, [])
        self.assertFalse(self.receipt.exists())
        self.assertEqual([command[:5] == PREFIX for command in plan], [False, False, True, False, False, False])
        # The second listing follows the guest command at once, before the slower host observation.
        self.assertEqual((plan[1], plan[3]), (boot.RUNNING, boot.RUNNING))

    def test_binding_and_reservations_are_required(self):
        for changes in ({"volume_log": None, "volume_log_first_row_sha256": None, "not_after": None},
                        {"reserve_bytes": -1}, {"other_host_bytes": -1}, {"min_distinct_cores": 0}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.admit(**changes)
        self.assertEqual(self.started, [])


if __name__ == "__main__":
    unittest.main()
