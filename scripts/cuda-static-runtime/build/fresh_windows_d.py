# SPDX-License-Identifier: Apache-2.0
"""Prospective Windows CUDA build policy for the single D campaign build."""
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import types

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
OUTPUT = REPO / "artifacts/v190/final-plan/cu18"
CAMPAIGN = REPO / "artifacts/v190/final-plan/exec-20261116T052942Z"
ORIGINAL = CAMPAIGN / "fresh-cuda-configure-cu15-r1.json"
ORIGINAL_SHA = "d10eb3ced57d17901e3749f05864e1f230b0d8988558a3ddaa98d72f61c1a863"
GUARD_SHA = "cadb0dca69293b48007baa8cd4fa6c7cc447cbd36c428f61ad7252057d2a6729"
END = dt.datetime(2026, 11, 20, 17, 10, tzinfo=dt.timezone.utc)
SOURCE_SHA = {
    "fresh_configure.py": "0a45de8dc0aa23554e8fe17042b8b17853dea4356d138d52aa39470b8759fe51",
    "fresh_compile.py": "9baab4661425582367f636c46d2a02871d43f8567d1b6c0e68445197e99813da",
    "fresh_aggregate.py": "76266fa883786da4b1833f9316f5121eccaf7ac60440cc105544df8e351260a7",
    "fresh_verify.py": "4dd46151df7ff631ae93bb5c807a9c7c4de7c87579dd845ba64b22ea5e3c706f",
}
GIB = 1 << 30
VOLUME_BASE = 1_233_246_265_344


def require(condition, message):
    if not condition:
        raise ValueError(message)


def plain(path):
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            f"canonical existing input required: {path}")
    for part in (path, *path.parents):
        require(not part.is_symlink() and not part.is_junction(), f"linked input refused: {part}")
    return path


def read(path, sha):
    raw = plain(path).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == sha, f"reviewed input differs: {path}")
    return raw


def disk_admission(free, additional, other):
    require(all(type(v) is int and v >= 0 for v in (free, additional, other)), "invalid volume reservation")
    growth = max(0, VOLUME_BASE - free)
    require(growth + additional + other + (128 << 20) <= 120 * GIB, "Windows tooling cap")
    require(free >= 100 * GIB + max(0, 182 * GIB - growth), "campaign D free-space floor")
    return growth


def cutoff(now, start):
    require(now.utcoffset() == start.utcoffset() == dt.timedelta() and now >= start, "invalid UTC clocks")
    end = min(END, start + dt.timedelta(seconds=10800))
    require(now < end, "original CUDA campaign/build window expired")
    return end


def exact_output(value):
    root = Path(value)
    require(root == OUTPUT and root.is_absolute() and root.resolve(strict=False) == root, "unexpected CUDA output")
    for part in (root, *root.parents):
        require(not part.is_symlink() and not part.is_junction(), f"linked CUDA output: {part}")
    return root


def environment(source):
    env = dict(source["environment"])
    tmp = str(OUTPUT / "tmp")
    for key in ("TEMP", "TMP", "TMPDIR"):
        env[key] = tmp
    env.update(CUDA_CACHE_PATH=str(OUTPUT / "cuda-cache"), PYTHONDONTWRITEBYTECODE="1",
               PYTHONNOUSERSITE="1", PSModuleAnalysisCachePath=str(OUTPUT / "tmp/pwsh-analysis-cache"))
    tools = source["tools"]
    sdk = Path(env["WINDOWSSDKDIR"]) / "bin" / env["WINDOWSSDKVERSION"].strip("\\/") / "x64"
    dirs = [Path(tools[key]["path"]).parent for key in ("compiler", "cmake", "ninja", "python", "patch")]
    cuda = Path(source["native_inputs"][0]["path"]).parent / "bin"
    dirs.extend([sdk, cuda, Path(source["powershell"]["path"]).parent, Path(env["SYSTEMROOT"]) / "System32"])
    require(all(p.is_absolute() and p.is_dir() for p in dirs), "original tool directory unavailable")
    env["PATH"] = os.pathsep.join(map(str, dirs))
    return env


def current_recipe(head, reviewed_utc):
    reviewed = dt.datetime.fromisoformat(reviewed_utc)
    require(reviewed.utcoffset() == dt.timedelta() and reviewed.date() == END.date() and
            reviewed <= min(END, dt.datetime.now(dt.timezone.utc)), "invalid D build review time")
    import re
    require(re.fullmatch("[0-9a-f]{40}", head) is not None, "reviewed Git commit required")
    source = json.loads(read(ORIGINAL, ORIGINAL_SHA))
    source["fresh_output"] = str(OUTPUT)
    source["head"] = head
    source["reviewed_utc"] = reviewed_utc
    source["environment"] = environment(source)
    guard = source["owners"]["guards"]
    data = read(Path(guard["path"]), GUARD_SHA)
    guard.update(bytes=len(data), sha256=GUARD_SHA)
    return source


def check_recipe(path, sha):
    actual = json.loads(read(path, sha))
    exact_output(actual["fresh_output"])
    require(actual == current_recipe(actual["head"], actual["reviewed_utc"]), "prospective recipe differs from exact reviewed inputs")
    return actual


def implementation(name):
    path = HERE / name
    module = types.ModuleType("airk_cu18_" + path.stem)
    module.__file__ = str(path)
    exec(compile(read(path, SOURCE_SHA[name]), str(path), "exec"), vars(module))
    return module


def owners():
    config, compiler, archive, verifier = (implementation(name) for name in SOURCE_SHA)
    for owner in (config, compiler, verifier):
        owner.ANCHOR = OUTPUT.parent
    config.CAMPAIGN_END = compiler.CAMPAIGN_END = END
    # Each loader originally imports a fresh module. Reuse only these genuine,
    # pinned module objects so the narrowly scoped campaign clock survives the join.
    bound = {Path(module.__file__): module for module in (config, compiler, archive, verifier)}
    def scoped(loader):
        def load(path, name):
            module = bound.get(Path(path))
            return module if module is not None else loader(path, name)
        return load
    for owner in bound.values():
        owner.load = scoped(owner.load)
    original_chain = compiler.load_chain
    def checked_chain(pins, configured_pin, deadline):
        require(Path(configured_pin["path"]) == OUTPUT / "fresh-configure.json", "foreign configure receipt")
        chain = original_chain(pins, configured_pin, deadline)
        cfg, recipe = chain[:2]
        exact_output(recipe["fresh_output"])
        check_recipe(Path(cfg["recipe"]["path"]), cfg["recipe"]["sha256"])
        return chain
    compiler.load_chain = checked_chain
    return config, compiler, archive, verifier


def verify_receipt(aggregate_pin, deadline):
    require(Path(aggregate_pin["path"]) == OUTPUT / "fresh-aggregate.json", "foreign CUDA archive")
    exact_output(OUTPUT)
    facts = owners()[3].verify_receipt(aggregate_pin, deadline)
    import runpy
    supervisor = runpy.run_path(str(HERE / "fresh_windows_d_job.py"))
    supervisor["verify_outer"](facts, deadline)
    return facts


def execute(stage, path, sha):
    now = dt.datetime.now(dt.timezone.utc)
    cutoff(now, now)
    exact_output(OUTPUT)
    config, compiler, archive, _ = owners()
    if stage == "configure":
        check_recipe(path, sha)
        return config.execute(path, sha)
    if stage == "compile":
        require(path == OUTPUT / "fresh-configure.json", "foreign configure input")
        return compiler.execute(path, sha, SOURCE_SHA["fresh_compile.py"])
    require(stage == "aggregate", "invalid execution stage")
    recipe = json.loads(read(path, sha))
    require(Path(recipe["compile_receipt"]["path"]) == OUTPUT / "fresh-compile.json", "foreign compile receipt")
    return archive.execute(path, sha)
