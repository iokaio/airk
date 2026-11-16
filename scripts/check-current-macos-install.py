# SPDX-License-Identifier: Apache-2.0
"""Opt-in current macOS I4 DMG journey; never repairs an occupied account.

Consumes approved genuine C7 source/signed pairs and notarized DMGs. Uses the
actual silent wrapper, current airkctl and native original-operation recovery.
Failures retain fixtures, mounts, requests and diagnostics. There is no automatic
cleanup, retry, reset acknowledgement, trust or login consent.
"""
import argparse
import contextlib
from datetime import datetime, timezone
import errno
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import select
import stat
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
IMAGE = "AIRK.app/Contents/MacOS/airk"
CLI = "AIRK.app/Contents/MacOS/airkctl"
SHA = re.compile(r"[0-9a-f]{64}")
INCARNATION = re.compile(r"[0-9a-f]{32}")


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def require(condition, detail):
    if not condition:
        raise ValueError(detail)


def regular(path, maximum=None):
    value = Path(path).lstat()
    require(stat.S_ISREG(value.st_mode) and value.st_nlink == 1,
            "regular unlinked input required: " + str(path))
    require(maximum is None or value.st_size <= maximum, "input exceeds bound: " + str(path))
    return value


def native_home():
    import pwd
    return Path(pwd.getpwuid(os.getuid()).pw_dir)


def verify_human(system, uid, console_uid):
    require(system == "Darwin" and uid != 0 and uid == console_uid,
            "requires the logged-in standard Mac human UID; no alternate identity")


def pointer_path(home):
    # airk-process/src/user_folders.rs::install_pointer, shared with I4 writer.
    return home / "Library/Application Support/AIRK/install-pointer/current.json"


def version_tuple(value):
    require(isinstance(value, str) and re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value),
            "invalid candidate version")
    return tuple(map(int, value.split(".")))


def snapshot(paths):
    result = {}
    for path in paths:
        if os.path.lexists(path):
            info = path.lstat()
            result[str(path)] = {"mode": info.st_mode, "uid": info.st_uid,
                                 "identity": [info.st_dev, info.st_ino],
                                 "sha256": digest(path) if stat.S_ISREG(info.st_mode) else None,
                                 "link": os.readlink(path) if path.is_symlink() else None}
        else:
            result[str(path)] = None
    return result


class Commands:
    def __init__(self, out, report, clock=time.monotonic):
        self.out, self.report, self.clock = out, report, clock

    def retain(self):
        path = self.out / "receipt.json"
        temporary = self.out / "receipt.json.writing"
        with temporary.open("w", encoding="utf-8", newline="\n") as stream:
            json.dump(self.report, stream, indent=2)
            stream.write("\n"); stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, path)

    def run(self, args, *, timeout=125, json_result=False, allowed=(0,)):
        require(timeout > 0, "original command deadline expired; no child launched")
        argv = list(map(str, args))
        row = {"argv": argv, "started_utc": datetime.now(timezone.utc).isoformat(),
               "deadline_seconds": timeout, "exit_code": None, "status": "started"}
        self.report["commands"].append(row); self.retain()
        number = len(self.report["commands"])
        tick = self.clock()
        try:
            done = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True,
                                  encoding="utf-8", errors="replace", timeout=timeout, cwd=ROOT)
        except BaseException as error:
            row.update(status="outcome_unknown", error={"type": type(error).__name__, "detail": str(error)},
                       elapsed_seconds=self.clock() - tick)
            for name in ("stdout", "stderr"):
                value = getattr(error, name, None)
                if value is not None:
                    raw = value if isinstance(value, bytes) else value.encode("utf-8")
                    (self.out / f"command-{number:03}.{name}.log").write_bytes(raw)
            self.retain()
            raise
        row.update(status="exited", exit_code=done.returncode, elapsed_seconds=self.clock() - tick)
        for name in ("stdout", "stderr"):
            text = getattr(done, name)
            path = self.out / f"command-{number:03}.{name}.log"
            path.write_text(text, encoding="utf-8", newline="\n")
            row[name] = {"file": path.name, "sha256": digest(path)}
        self.retain()
        require(done.returncode in allowed,
                f"command refused ({done.returncode}); preserve {self.out}/command-{number:03} logs")
        return json.loads(done.stdout) if json_result else done



# Current pair plus retained native launcher/helper/role image names. Names only
# select conservative blockers; they never grant termination or cleanup authority.
AIRK_IMAGE_BASENAMES = frozenset(("airk", "airkctl", "airk-embed", "airk-launcher",
    "airk-launcher-image", "airk-service", "airk-index", "airk-source"))


def process_census(text, uid):
    result = {"same_user_airk": [], "other_user_airk": [], "scanned_rows": 0,
              "other_user_rows": 0,
              "scope": "effective UID only, matching native PROC_UID_ONLY; other users preserved, no cleanup authority"}
    for line in text.splitlines():
        if not line.strip():
            continue
        fields = line.split(None, 2)
        require(len(fields) == 3 and fields[0].isascii() and fields[0].isdecimal()
                and fields[1].isascii() and fields[1].isdecimal(),
                "native PID/UID/executable census row is unverified")
        pid, owner = int(fields[0]), int(fields[1])
        executable = fields[2]
        result["scanned_rows"] += 1
        result["other_user_rows"] += owner != uid
        if PurePosixPath(executable).name.lower() not in AIRK_IMAGE_BASENAMES:
            continue
        row = {"pid": pid, "uid": owner, "executable": executable}
        result["same_user_airk" if owner == uid else "other_user_airk"].append(row)
    return result


def preflight(home, uid, commands):
    support = home / "Library/Application Support/AIRK"
    candidates = [support, home / "Library/Caches/AIRK/service", home / "Library/Logs/AIRK/service",
                  home / "Applications/AIRK.app", Path("/Applications/AIRK.app"),
                  home / ".local/bin/airk", home / ".local/bin/airkctl"]
    require(not any(os.path.lexists(path) for path in candidates),
            "occupied or retained AIRK location preserved; fresh-account journey refused")
    processes = commands.run(["/bin/ps", "-axo", "pid=,uid=,comm="]).stdout
    census = process_census(processes, uid)
    commands.report["preflight_processes"] = census
    commands.retain()
    require(not census["same_user_airk"], "existing same-user AIRK process preserved")
    jobs = commands.run(["/bin/launchctl", "list"]).stdout
    require(not any("airk" in line.lower() for line in jobs.splitlines()),
            "existing or unknown AIRK login job preserved")
    for channel in ("stable", "preview"):
        label = f"io.iokaio.airk.service.{channel}"
        result = commands.run(["/bin/launchctl", "print", f"gui/{uid}/{label}"], allowed=(113,))
        require(f'Could not find service "{label}"' in result.stderr,
                "fixed login registration absence unverified")
    for folder in (home / "Library/LaunchAgents", Path("/Library/LaunchAgents"), Path("/Library/LaunchDaemons")):
        require(not folder.exists() or not any("airk" in child.name.lower() for child in folder.iterdir()),
                "predecessor login definition preserved")
    return snapshot(candidates + [home / name for name in (".profile", ".zprofile", ".zshrc", ".bash_profile", ".bashrc")])


def candidate(options, prefix, commands, *, current):
    """Delegate C7/I4 artifact authority to the existing owning validators."""
    mac = module("current_mac_package", "package-standalone-macos.py")
    package = module("current_mac_native", "package-native.py")
    signer = module("current_mac_signer", "sign-native.py")
    get = lambda name: getattr(options, prefix + name)
    runtime, signed, image, manifest = [get(name).resolve(strict=True)
                                      for name in ("runtime_dir", "signed_dir", "installer", "manifest")]
    for path, approved in ((image, get("approved_installer_sha256")), (manifest, get("approved_manifest_sha256"))):
        regular(path)
        require(SHA.fullmatch(approved or "") and digest(path) == approved,
                "independently approved installer/manifest bytes differ")
    receipt = package.bounded_json(manifest, 4 << 20)
    version, target = receipt.get("version"), receipt.get("target")
    require(version_tuple(version) >= (1, 8, 35) and target == options.target,
            "current I4 matched-pair installer target/version required")
    build = package.bounded_json(runtime / "airk-build.json", 65536)
    require(package.bounded_json(runtime / "airk-runtime.json", 4096).get("schema") == 3,
            "schema3 matched pair required; no predecessor metadata substitution")
    if current:
        require(version == package.tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"],
                "current candidate version differs from source")
    expected_source = package.runtime_builder().source_identity() if current else build["source"]
    source_files, _ = package.validate_runtime(runtime, version, target, expected_source)
    commit = package.capture("git", "rev-parse", "HEAD") if current else receipt.get("commit")
    lock = digest(ROOT / "Cargo.lock") if current else receipt.get("cargo_lock_sha256")
    placed, signing = signer.read_verified_pair(signed, runtime, target, commit, lock)
    files, _ = package.validate_runtime(placed, version, target, expected_source)
    require(receipt.get("schema") == "airk-standalone-installer/1"
            and receipt.get("installer_kind") == "macos_dmg"
            and receipt.get("artifact") == image.name and receipt.get("artifact_sha256") == digest(image)
            and receipt.get("runtime_install_sha256") == digest(placed / "airk-install.json")
            and receipt.get("runtime_source") == expected_source and receipt.get("commit") == commit
            and receipt.get("cargo_lock_sha256") == lock
            and receipt.get("runtime_signing_sha256") == digest(signed / "signing.json")
            and receipt.get("runtime_signing") == signing
            and receipt.get("payload_schema") == "airk-service-payload/1",
            "DMG/source/signed-pair provenance differs")
    require(signer.stapled_app_ticket(signing["components"]), "exact app notarization receipt required")
    claim = receipt.get("signing", {})
    notarization = claim.get("notarization", {})
    require(claim.get("team_id") == mac.TEAM and notarization.get("status") == "Accepted"
            and notarization.get("stapled") is True and notarization.get("scope") == image.name
            and notarization.get("id"), "genuine notarized DMG receipt required")
    observed = mac.verify_image_signature(image, claim.get("identity", ""), mac.TEAM, None)
    require(all(claim.get(key) == value for key, value in observed.items()), "DMG signature identity differs")
    commands.run(["/usr/bin/xcrun", "stapler", "validate", image])
    commands.run(["/usr/sbin/spctl", "--assess", "--type", "open", "--context", "context:primary-signature", image])
    commands.run(["/usr/sbin/spctl", "--assess", "--type", "execute", placed / "AIRK.app"])
    mapping = mac.module("current_mac_layout", "package-standalone-linux.py").layout_names(files, "macos")
    return {"version": version, "target": target, "source": expected_source, "manifest": receipt,
            "image": image, "runtime": placed, "source_runtime": runtime, "source_files": source_files, "runtime_files": files, "files": {name: files[source] for name, source in mapping.items()},
            "mac": mac, "package": package, "input_hashes": {str(path): digest(path) for path in (image, manifest, signed / "signing.json")}}


def mount_candidate(value, out, commands):
    mount = out / ("mount-" + value["version"])
    mount.mkdir(mode=0o700)
    commands.report["mounts"].append({"path": str(mount), "image": str(value["image"]), "detached": False})
    commands.retain()
    commands.run(["/usr/bin/hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount, value["image"]])
    require(value["package"].inventory(mount) == value["manifest"]["payload"], "exact mounted DMG roster differs")
    payload = value["package"].inventory(mount / "payload")
    expected = dict(value["files"], **{"payload-inventory.json": value["manifest"]["payload_inventory_sha256"]})
    require(payload == expected, "mounted app/legal/inventory differs from signed matched pair")
    wrappers = value["mac"].split_installer_files(value["version"], value["target"],
        expected["payload-inventory.json"], digest(mount / "payload.sha256"), expected[IMAGE], len(expected))
    for name, raw in wrappers.items():
        require((mount / name).read_bytes() == raw, "mounted canonical wrapper differs")
    value["mount"] = mount
    return value


def wrapper_command(value, locations, *, original=None):
    argv = ["/bin/bash", str(value["mount"] / "Install AIRK.command")]
    if original is not None:
        return argv + ["--resume", str(original), "--silent"]
    argv.append("--silent")
    for name in ("program", "data", "state", "cache", "logs"):
        argv += ["--" + name + "-dir", str(locations[name])]
    return argv


def setup_result(output):
    lines = []
    for text in output.splitlines():
        try:
            value = json.loads(text)
        except ValueError:
            continue
        if isinstance(value, dict) and value.get("schema") == "airk-setup-result/1":
            lines.append(value)
    require(len(lines) == 1 and lines[0].get("status") == "complete", "native setup completion unverified")
    require(lines[0].get("retired") == 0 and lines[0].get("leftovers") == [],
            "unexpected predecessor retirement or unresolved leftovers preserved")
    return lines[0]



def data_sentinel(path, expected=None):
    if expected is None:
        with path.open("xb") as stream:
            stream.write(b"Keep-data uninstall sentinel.\n")
            stream.flush(); os.fsync(stream.fileno())
        return digest(path)
    regular(path)
    require(digest(path) == expected, "owned original data changed during upgrade/removal")
    return expected


def original_request(stderr, home):
    matches = re.findall(r"^Original request: (.+)$", stderr, re.MULTILINE)
    require(len(matches) == 1, "original wrapper recovery request unavailable")
    path = Path(matches[0])
    require(path.is_absolute() and path.parent.parent.parent == home
            and re.fullmatch(r"\.airk-setup-recovery\.[A-Za-z0-9]+", path.parent.parent.name)
            and re.fullmatch(r"operation-[1-9][0-9]*", path.parent.name)
            and path.name == "operation-request.json", "unrecognized original wrapper custody path")
    info = regular(path, 16384)
    require(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) & 0o077 == 0,
            "original wrapper request is not human-owner-private")
    return path


def require_installation(document, value, own_cli_digest):
    installation = document.get("installation", {})
    require(document.get("version") == value["version"] and document.get("executable_sha256") == own_cli_digest
            and installation.get("state") == "complete" and installation.get("pair") == "exact"
            and installation.get("airk_version") == value["version"]
            and INCARNATION.fullmatch(installation.get("install_id", "")), "native I4 verification/pair differs")
    return installation["install_id"]


def require_health(status, value, uid, image_path, observe_image):
    service = status.get("service", {})
    health = service.get("health", {})
    require(service.get("state") == "running" and service.get("pending_lifecycle") is None
            and service.get("explicitly_stopped") is False, "native service is not settled and running")
    require(health.get("version") == value["version"] and health.get("user") == str(uid)
            and health.get("executable_sha256") == value["files"][IMAGE]
            and health.get("build_error") is None and INCARNATION.fullmatch(health.get("instance", ""))
            and isinstance(health.get("process_id"), int) and health["process_id"] > 0,
            "native health identity differs")
    require(observe_image(health["process_id"]) == image_path, "actual native service image differs")
    return health



def require_headless_login(response):
    login = response.get("login", {})
    require(response.get("op") == "login_startup" and login.get("supported") is True
            and login.get("registered") is False and login.get("detail") is None,
            "headless native login status is enabled/unknown; retained")
    return login


def process_path(pid):
    # Reuse the existing OS observer only; its historical journey is not invoked.
    return module("current_mac_process_path", "check-macos-direct-runtime.py").process_path(pid)


class Events:
    """Kernel events with one original deadline; no polling sleeps or retries."""
    def __init__(self, paths=(), pid=None, clock=time.monotonic):
        self.clock, self.queue, self.files = clock, select.kqueue(), []
        try:
            changes = []
            for path in dict.fromkeys(paths):
                descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
                self.files.append(descriptor)
                changes.append(select.kevent(descriptor, filter=select.KQ_FILTER_VNODE,
                    flags=select.KQ_EV_ADD | select.KQ_EV_CLEAR,
                    fflags=select.KQ_NOTE_WRITE | select.KQ_NOTE_RENAME | select.KQ_NOTE_DELETE))
            if pid is not None:
                changes.append(select.kevent(pid, filter=select.KQ_FILTER_PROC,
                    flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT, fflags=select.KQ_NOTE_EXIT))
            self.queue.control(changes, 0, 0)
        except BaseException:
            self.close(); raise

    def wait(self, deadline):
        left = deadline - self.clock()
        require(left > 0, "original observation deadline expired; outcome retained")
        events = self.queue.control(None, 32, left)
        require(self.clock() < deadline, "original kernel observation deadline expired; outcome retained")
        require(events and not any(event.flags & select.KQ_EV_ERROR for event in events),
                "native kernel observation unavailable/deadline expired")
        return events

    def close(self):
        self.queue.close()
        for descriptor in self.files:
            os.close(descriptor)
        self.files.clear()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def observe_until(read, ready, observer, deadline, clock=time.monotonic):
    while True:
        require(clock() < deadline, "original observation deadline expired; outcome retained")
        value = read()
        require(clock() < deadline, "original observation deadline expired after read; outcome retained")
        if ready(value):
            return value
        observer.wait(deadline)


def observe_child_exit(pid, image, deadline, events=Events, image_observer=process_path, clock=time.monotonic):
    require(clock() < deadline, "original child exit deadline expired; outcome retained")
    observed = image_observer(pid)
    require(clock() < deadline, "original child exit deadline expired after path observation")
    if observed is None:
        return "original_pid_observed_absent"
    require(observed == image, "removal child PID image changed; settlement unknown")
    try:
        with events(pid=pid) as child:
            child.wait(deadline)
    except OSError as error:
        if error.errno != errno.ESRCH:
            raise
        # The previously observed exact child can exit before kqueue attaches.
        # ESRCH alone is no proof: reobserve the same original PID, once, below.
    require(clock() < deadline, "original child exit deadline expired before final observation")
    require(image_observer(pid) is None, "native removal child exit unverified")
    require(clock() < deadline, "original child exit deadline expired after final observation")
    return "original_pid_exit_reobserved"


class Journey:
    def __init__(self, home, uid, out, commands, events=Events, image_observer=process_path, clock=time.monotonic):
        self.home, self.uid, self.out, self.commands = home, uid, out, commands
        self.events, self.image_observer, self.clock = events, image_observer, clock
        self.locations = {name: out / name for name in ("program", "data", "state", "cache", "logs")}
        self.pointer = pointer_path(home)
        self.active = None

    def cli(self, *args, timeout=125):
        return self.commands.run([self.locations["program"] / CLI, "--output", "json", *args], json_result=True, timeout=timeout)

    def installed(self, value):
        ident = require_installation(self.cli("version"), value, value["files"][CLI])
        observation = self.cli("service", "setup-receipt")
        require(observation.get("install_id") == ident and observation.get("successful") is True
                and observation.get("closed") is True, "native canonical setup receipt unverified")
        receipt = observation["receipt"]
        require(receipt.get("retired") == [] and receipt.get("leftovers") == [], "native setup retained unexpected predecessors/leftovers")
        record = value["package"].bounded_json(self.locations["program"] / "install-record.json", 65536)
        pointer = value["package"].bounded_json(self.pointer, 4096)
        require(record.get("install_id") == pointer.get("install_id") == ident
                and pointer.get("record_sha256") == digest(self.locations["program"] / "install-record.json")
                and record.get("locations") == {key: str(path) for key, path in self.locations.items()},
                "observed I4 pointer/locations differ from native verified installation")
        for name, expected in value["files"].items():
            require(digest(self.locations["program"] / name) == expected, "placed approved payload differs: " + name)
        self.commands.report["installed"].append({"version": value["version"], "pointer": pointer,
            "record": record, "setup": observation})
        self.commands.retain()
        return observation

    def health(self, value):
        return require_health(self.cli("service", "status"), value, self.uid,
                              self.locations["program"] / IMAGE, self.image_observer)

    def install(self, value, previous_health=None):
        deadline = self.clock() + 600
        context = self.events(pid=previous_health["process_id"]) if previous_health else contextlib.nullcontext()
        with context as predecessor:
            result = self.commands.run(wrapper_command(value, self.locations), timeout=600)
            if predecessor is not None:
                predecessor.wait(deadline)
        setup = setup_result(result.stdout)
        self.active = value
        observation = self.installed(value)
        require(observation["operation_id"] == setup["operation_id"], "wrapper/native operation crossed")
        health = self.health(value)
        if previous_health is not None:
            require(health["instance"] != previous_health["instance"], "upgrade reused prior service incarnation")
        login = self.cli("service", "login", "status")
        require_headless_login(login)
        self.commands.report["login_observations"].append(login)
        original = original_request(result.stderr, self.home)
        frozen = digest(original)
        resumed = self.commands.run(wrapper_command(value, self.locations, original=original), timeout=600)
        require(digest(original) == frozen, "original recovery request changed")
        again = self.installed(value)
        require(again["operation_id"] == observation["operation_id"] and again["receipt"] == observation["receipt"],
                "successful original-operation reconciliation replayed/changed native setup")
        self.commands.report["recovery"].append({"original_request": str(original), "sha256": frozen,
            "operation_id": observation["operation_id"], "stdout_sha256": hashlib.sha256(resumed.stdout.encode()).hexdigest(),
            "scope": "successful original-operation wrapper reconciliation; interrupted setup recovery unexercised"})
        self.commands.retain()
        return health

    def lifecycle(self, value):
        health = self.health(value)
        deadline = self.clock() + 125
        with self.events(pid=health["process_id"]) as observer:
            result = self.cli("service", "stop", timeout=deadline - self.clock())
            record = result.get("record", {})
            require(record.get("request", {}).get("expected_instance") == health["instance"]
                    and record.get("request", {}).get("action") == "stop"
                    and record.get("request", {}).get("runtime") == health["executable_sha256"]
                    and result.get("service_instance") == health["instance"]
                    and result.get("process_id") == health["process_id"], "native Stop receipt identity differs")
            observer.wait(deadline)
        require(self.cli("service", "status").get("service", {}).get("state") == "not_running",
                "saved Stop did not leave service stopped")
        self.cli("service", "start")
        restarted = self.health(value)
        require(restarted["instance"] != health["instance"], "explicit Start reused prior incarnation")
        self.commands.report["lifecycle"] = {"stop": result, "exact_process_exit_event": True,
            "before": health, "after": restarted}
        self.commands.retain()
        return restarted

    def remove(self, value):
        health = self.health(value)
        deadline = self.clock() + 240
        paths = [self.locations["state"] / "setup", self.pointer.parent, self.locations["program"].parent]
        with self.events(paths=paths, pid=health["process_id"]) as observer:
            accepted = self.cli("service", "uninstall", "--yes", timeout=deadline - self.clock())
            require(accepted.get("started") is True and accepted.get("kind") == "uninstall"
                    and accepted.get("delete_indexes") == "keep" and INCARNATION.fullmatch(accepted.get("operation_id", "")),
                    "native original removal admission unverified")
            receipt_path = self.locations["state"] / "setup" / (accepted["operation_id"] + ".json")
            require(accepted.get("receipt") == str(receipt_path), "removal receipt path crossed")
            def read():
                if not receipt_path.exists():
                    return None
                return value["package"].bounded_json(receipt_path, 256 << 10)
            receipt = observe_until(read, lambda row: row is not None and row.get("closed") is True,
                                    observer, deadline, self.clock)
            require(receipt.get("operation_id") == accepted["operation_id"] and receipt.get("failure") is None
                    and receipt.get("retired") == [] and receipt.get("leftovers") == [], "native removal failed or retained leftovers")
            # Native Core validates the full canonical receipt and original request.
            original = Path(accepted["operation"]).parent / "operation-request.json"
            recovery = Path(accepted["recovery_program"])
            require(recovery.is_relative_to(self.out / "temporary") and original.parent == recovery.parent,
                    "removal recovery program/request escaped owned temporary root")
            image = recovery / IMAGE
            require(digest(image) == value["files"][IMAGE], "retained native removal image differs")
            native = self.commands.run([image, "__airk_role", "setup", "--operation", original,
                                       "--recovery-field", "completion-required"], timeout=min(125, deadline - self.clock()))
            require(native.stdout.strip() == "false", "native original removal completion remains unknown")
            observe_until(lambda: not os.path.lexists(self.pointer) and not os.path.lexists(self.locations["program"] / "AIRK.app"),
                          bool, observer, deadline, self.clock)
        require(self.image_observer(health["process_id"]) is None, "removed service exact PID exit unverified")
        child_exit = observe_child_exit(accepted["process_id"], image, deadline,
            self.events, self.image_observer, self.clock)
        self.commands.report["removal"] = {"accepted": accepted, "receipt": receipt,
            "receipt_sha256": digest(receipt_path), "native_original_completion_required": False,
            "pointer_absent": True, "app_bundle_absent": True, "observed_service_pid_absent": True,
            "observed_removal_child_absent": True, "removal_child_exit_observation": child_exit,
            "retained_data": {name: path.exists() for name, path in self.locations.items() if name != "program"}}
        self.commands.retain()


def parse(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="fresh human-owned evidence directory outside target")
    parser.add_argument("--target", choices=("aarch64-apple-darwin", "x86_64-apple-darwin"), required=True)
    parser.add_argument("--allow-install", action="store_true", required=True, help="explicit actual account I4/DMG journey opt-in")
    for prefix in ("", "previous-"):
        required = not prefix
        for name in ("runtime-dir", "signed-dir", "installer", "manifest"):
            parser.add_argument("--" + prefix + name, type=Path, required=required)
        for name in ("approved-installer-sha256", "approved-manifest-sha256"):
            parser.add_argument("--" + prefix + name, required=required)
    args = parser.parse_args(argv)
    previous = [getattr(args, "previous_" + key) for key in ("runtime_dir", "signed_dir", "installer", "manifest", "approved_installer_sha256", "approved_manifest_sha256")]
    if any(previous) and not all(previous):
        parser.error("previous upgrade inputs require the complete independently approved source/signed pair and DMG/manifest")
    return args


def main(argv=None):
    args = parse(argv)
    uid = os.getuid() if hasattr(os, "getuid") else -1
    verify_human(platform.system(), uid, Path("/dev/console").stat().st_uid if sys.platform == "darwin" else -2)
    require(platform.machine() == ("arm64" if args.target.startswith("aarch64") else "x86_64"), "native host target differs")
    home = native_home()
    out = args.out.absolute()
    parent = out.parent.resolve(strict=True)
    require(parent.is_relative_to(home) and parent.stat().st_uid == uid
            and not parent.stat().st_mode & 0o022 and not os.path.lexists(out)
            and not out.is_relative_to(ROOT / "target"), "fresh same-human evidence parent outside target required")
    require(out.parent == parent, "redirected evidence parent refused")
    out.mkdir(mode=0o700)
    temporary = out / "temporary"; temporary.mkdir(mode=0o700)
    os.environ.update(PYTHONDONTWRITEBYTECODE="1", TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    import tempfile
    tempfile.tempdir = str(temporary)
    report = {"schema": "airk-current-macos-install-journey/1", "status": "started", "passed": False,
              "started_utc": datetime.now(timezone.utc).isoformat(), "command": [sys.executable, *sys.argv],
              "harness_sha256": digest(Path(__file__)), "consumed_helper_sha256": {str(path.relative_to(ROOT)): digest(path)
                  for path in [ROOT / "scripts" / name for name in ("package-native.py", "sign-native.py", "package-standalone-macos.py", "package-standalone-linux.py", "build-service-runtime.py", "split_receipts.py", "signing-identity.json", "check-macos-direct-runtime.py")]
                  + [ROOT / "installers/macos/Install AIRK.command.in", ROOT / "installers/macos/README.txt.in"]}, "identity": {"uid": uid, "console_uid": uid, "home": str(home)},
              "commands": [], "mounts": [], "installed": [], "login_observations": [], "recovery": [],
              "upgrade_exercised": False, "release_qualified": False,
              "scope": "actual headless I4 DMG install, successful original-operation reconciliation, native lifecycle and keep-data removal; no consent, model/read transport, interrupted recovery or other platform claim"}
    commands = Commands(out, report)
    started = time.monotonic(); commands.retain()
    try:
        preserved = preflight(home, uid, commands)
        report["preservation_before"] = preserved
        current = candidate(args, "", commands, current=True)
        previous = candidate(args, "previous_", commands, current=False) if args.previous_installer else None
        if previous:
            require(version_tuple(previous["version"]) < version_tuple(current["version"]), "real upgrade requires strictly older genuine candidate")
        candidates = ([previous] if previous else []) + [current]
        report["inputs"] = [{"version": value["version"], "source": value["source"], "hashes": value["input_hashes"]} for value in candidates]
        for value in candidates:
            if value is candidates[0]:
                discovery = commands.run([value["runtime"] / CLI, "--output", "json", "version"], json_result=True)
                require(discovery.get("installation", {}).get("state") == "not_found", "native occupied/unverified installation preserved")
            mount_candidate(value, out, commands)
        source = out / "original-source.txt"; source.write_bytes(b"Read-only original source; preserve through current Mac I4 journey.\n")
        original_source_hash = digest(source)
        report["source_sentinel"] = {"path": str(source), "sha256": original_source_hash}
        commands.retain()
        journey = Journey(home, uid, out, commands)
        prior = None
        data_hash = None
        for value in candidates:
            prior = journey.install(value, previous_health=prior)
            sentinel = journey.locations["data"] / "original-data.txt"
            data_hash = data_sentinel(sentinel, data_hash)
            report["data_sentinel"] = {"path": str(sentinel), "sha256": data_hash}
            commands.retain()
        report["upgrade_exercised"] = previous is not None
        journey.lifecycle(current)
        journey.remove(current)
        require(digest(source) == original_source_hash and digest(sentinel) == data_hash, "source/kept data preservation failed")
        after = snapshot([Path(path) for path in preserved])
        for path, before in preserved.items():
            if path == str(home / "Library/Application Support/AIRK"):
                continue  # authentic native authority/retained receipts may remain
            require(after[path] == before, "unrelated/predecessor/profile changed: " + path)
        require(not os.path.lexists(pointer_path(home)), "current pointer remained after native removal")
        require(not any("airk" in line.lower() for line in commands.run(["/bin/launchctl", "list"]).stdout.splitlines()),
                "AIRK launch job remains after native removal")
        report.update(source_preserved=True, kept_data_preserved=True, preservation_after=after)
        require(digest(Path(__file__)) == report["harness_sha256"]
                and all(digest(ROOT / path) == expected for path, expected in report["consumed_helper_sha256"].items()),
                "consumed driver/helper sources changed during journey")
        for value in candidates:
            require(value["package"].inventory(value["runtime"]) == value["runtime_files"]
                    and value["package"].inventory(value["source_runtime"]) == value["source_files"],
                    "approved source/signed runtime roster drifted")
            require(all(digest(Path(path)) == expected for path, expected in value["input_hashes"].items()), "approved artifact drifted")
            commands.run(["/usr/bin/hdiutil", "detach", value["mount"]])
            next(row for row in report["mounts"] if row["path"] == str(value["mount"]))["detached"] = True
        report.update(status="passed", passed=True)
    except BaseException as error:
        report.update(status="failed_retained", passed=False,
                      failure={"type": type(error).__name__, "detail": str(error)},
                      cleanup="No automatic stop/uninstall/replay/detach/deletion attempted after failure; observe original native request and process state before recovery.")
        raise
    finally:
        report.update(ended_utc=datetime.now(timezone.utc).isoformat(), elapsed_seconds=time.monotonic() - started)
        commands.retain()
    print(json.dumps({"receipt": str(out / "receipt.json"), "passed": report["passed"], "upgrade_exercised": report["upgrade_exercised"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
