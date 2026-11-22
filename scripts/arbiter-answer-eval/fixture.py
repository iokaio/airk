# SPDX-License-Identifier: Apache-2.0
"""Own one private Answer-evaluation fixture lifetime on Windows (CPU).

``start`` launches the private evaluation service (the airk-service test image
run on its software-v2 selector) inside an owned Windows Job, makes sure both
repository indexes exist with MiniLM vectors, writes ``handle.json`` for
clients and stays up until the lifetime's stop file appears or its deadline
passes. It then stops the service through ``service shutdown`` and checks the
settlement receipts. A later ``start --restart-from`` reopens the same control
root, catalog, model installation and indexes, so the indexes are built once.

``stage`` enables or disables Focus or Context with a named profile. ``stop``
writes the stop file. ``leak-probe`` checks that the airk index serves nothing
from the benchmark paths. ``smoke`` times one call of each read tool.
See FIXTURE.md beside this file.
"""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import runpy
import secrets
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
EVALUATION = CAMPAIGN / "answer-eval-r1"
FIXTURES = EVALUATION / "fixture"
NATIVE = ROOT / "scripts/arbiter/evaluation/run_compact_answer.py"
OWNER = ROOT / "scripts/provision-static-onnx-runtime.py"
SELECTOR = "service_runtime::evaluation_fixture_tests::actual_private_arbiter_fixture_software_v2"
HANDLE_SCHEMA = "airk.private.answer-eval-fixture-handle/1"
OWNER_VARIABLE = "AIRK_ANSWER_EVAL_FIXTURE_OWNER"
# The owned Job observer allows at most 21,600 s, including a 600 s settlement margin.
MAX_SECONDS = 21000
SHUTDOWN_MARGIN = 300
COMMAND_SECONDS = 120
# The software-v2 selector requires exactly one Focus, one Compact and one Large
# profile. This roster is the one the TechQA stage arms ran on.
DEFAULT_INPUTS = CAMPAIGN / "cal100-focus1024-launch-r1/inputs.json"
DEFAULT_INPUTS_SHA256 = "8fbde10f3fc8a42d1d05ec0f362a1e0527840ad49fa6686224f1feee4b08373d"
MINILM = ROOT / "artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3"
REPOSITORIES = {"airk": Path(r"D:\code\Github\airk"), "bookbuilder": Path(r"D:\code\Github\bookbuilder"),
                # A fresh bookbuilder index (22 Nov 2026): its denies are source policy before the first index
                # pass, because trimming the live bookbuilder index degraded its vector publication (token 267).
                "bookbuilder-v2": Path(r"D:\code\Github\bookbuilder")}
# Repositories indexed from a git-archive export of one commit instead of in place (see ensure_snapshot).
SNAPSHOTS = {"airk": "1fe7b6003574e4e385896120c62b99acbca00a3a",
             # Exported like airk (git archive of the commit the bookbuilder scenarios were written at), so the
             # new index has its own read-only root beside the in-place bookbuilder index.
             "bookbuilder-v2": "238b22a722ceb78447d72e39585fd6a7d4d3dd0c"}
# Benchmark paths denied through the airk index's own source policy (formats.md).
EXCLUSIONS = {
    "airk": [
        ("exact", "docs/arbiter/final-plan-fp16.md"),
        ("subtree", "docs/arbiter/answer-evaluation"),
        ("exact", "docs/simplify/one-process-ledger.md"),
        ("exact", "docs/agent-current-work.md"),
        ("subtree", "scripts/arbiter-answer-eval"),
    ],
    # Credential-like scripts found by credential_probe.py (22 Nov 2026), denied on the primary agent's
    # decision through the index's own source policy, so every rebuild or restart denies them too.
    "bookbuilder": [
        ("exact", "ironspire/scripts/set-rss-key.ps1"),
        ("exact", "ironspire/scripts/set-sendgrid-key.ps1"),
    ],
}
# The fresh bookbuilder index carries every bookbuilder deny from its setup, before its first index pass.
EXCLUSIONS["bookbuilder-v2"] = list(EXCLUSIONS["bookbuilder"])
# Named profiles. Only CPU rows run today; a GPU row names what it still needs.
PROFILES = {
    "focus-static1024-cpu": {"stage": "focus", "device": "cpu",
                             "manifest": "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64"},
    "context-compact-cpu": {"stage": "context", "device": "cpu",
                            "manifest": "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"},
    "context-large-cpu": {"stage": "context", "device": "cpu",
                          "manifest": "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"},
    # Change set 1 (catalog-only profiles installed by the change-set-1 selector; manifests from
    # R/change-set-1/profiles/change-set-1-profiles.json and the selector's change-set.json). Added by the
    # harness agent on 22 Nov 2026: without these rows `stage --profile` could not name them.
    "focus-static1024-cpu-cs1-software-w16": {"stage": "focus", "device": "cpu",
                                              "manifest": "f2d194b0070458d5af772802dcbb9ae5e27959746794f9bdfaa00eac9202830a"},
    "focus-static1024-cpu-cs1-publisher-w16": {"stage": "focus", "device": "cpu",
                                               "manifest": "41f0024d5da48f5ef9927dc1b8ddc95931a5784e33004ccdd827db1f2aaaf6f0"},
    "context-compact-cpu-cs1-provisional-read": {"stage": "context", "device": "cpu",
                                                 "manifest": "1e6e3d7240f6ab01c0a33be454352958c258fa969e9622606650ff3a7a43a038"},
    "context-large-cpu-cs1-provisional-read": {"stage": "context", "device": "cpu",
                                               "manifest": "8c864551f95668c769b2cd790ff4b40f75add19958f93d9ac67d7bbd192cc4af"},
    # Change set 1's trained Focus candidate (catalog-only, candidate: true; manifest from the cs1c smoke's
    # change-set.json, inputs change-set-inputs-r2.json).
    "focus-static1024-cpu-cand1-software-w16": {"stage": "focus", "device": "cpu",
                                                "manifest": "c3cc396a0e85521f53ff186f753330b2f6f1a632f79909f3202dc640079e98c4"},
    # Change set 2 extras (inputs change-set-2-inputs-r4.json; manifests from change-set-2-profiles.json and the
    # primary agent's release note): the second trained Focus candidate and the two option-B Context profiles.
    "focus-static1024-cpu-cand2-software-w16": {"stage": "focus", "device": "cpu",
                                                "manifest": "994e9cd75837169fb2994ef4244759d81a97753aa4a38f09c1110a3f96397c33"},
    "context-compact-cpu-cs2-context-b": {"stage": "context", "device": "cpu",
                                          "manifest": "c393cac2c6d7785143b92784f16e4d4e55600efed14eec9c9f19f2549a012504"},
    "context-large-cpu-cs2-context-b": {"stage": "context", "device": "cpu",
                                        "manifest": "4fd52d861dcbd11e9b5ab2a4f9d072c34e13ef0fc97f09e224fe017a1f0dbef1"},
    "focus-static1024-cuda": {"stage": "focus", "device": "cuda", "manifest": None,
                              "missing": "no CUDA Focus selector or launcher exists (plan section 7)"},
    "context-compact-cuda": {"stage": "context", "device": "cuda", "manifest": None,
                             "missing": "cuda_answer.rs selector: Compact only, fresh lifetime, gpu-cuda image"},
    "context-large-cuda": {"stage": "context", "device": "cuda", "manifest": None,
                           "missing": "cuda_answer.rs selector must be extended to Large (plan section 7)"},
}
ENVIRONMENT = {
    "AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT": "throughput64gib4threads",
    "AIRK_SELECTED_CONTEXT_CAL_OBSERVER": "false",
    "AIRK_NATIVE_WARM_OBSERVER": "true",
    "AIRK_MCP_RESPONSE_MODE": "full",
}
PROBE_TOOLS = ("airk_search",)


SELECTORS = {
    "software-v2": SELECTOR,
    "change-set-1": "service_runtime::evaluation_fixture_tests::restart::change_set::"
                    "actual_private_arbiter_fixture_change_set_1",
}
CHANGE_SET_1_INPUTS = EVALUATION / "change-set-1/profiles/change-set-1-inputs.json"
CHANGE_SET_1_INPUTS_SHA256 = "51275bd62a4cb326e39e4477560cf95624a1c83ee29651044a7a5ec8511c59ee"


def selector_extras(selector, restart, inputs=None, inputs_sha256=None, layers=None, seed=None, check_file=True):
    """Environment the named selector needs beyond the common fixture environment.

    change-set-1 (``evaluation_fixture_tests/restart/change_set.rs``) takes its pinned extra profile
    inputs, an optional ``AIRK_ARBITER_EVALUATION_ANSWER_LAYERS`` (``all``, ``none``, a list, or removals
    such as ``all,-focus``; the Rust parser is the authority) and, on a fresh run only,
    ``AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU`` = all | focus | none."""
    require(selector in SELECTORS, "unknown selector " + str(selector))
    if selector == "software-v2":
        require(inputs is None and inputs_sha256 is None and layers is None and seed is None,
                "change-set arguments need --selector change-set-1")
        return {}
    require(inputs is not None and inputs_sha256 is not None, "change-set-1 needs its inputs and their SHA-256")
    # The Rust selector refuses a relative source ("private fixture source must be absolute"; L15-cs1,
    # 22 Nov 2026), so the inputs path is made absolute here.
    inputs = Path(inputs).resolve()
    require(len(inputs_sha256) == 64 and all(c in "0123456789abcdef" for c in inputs_sha256),
            "change-set inputs SHA-256 must be 64 lowercase hex characters")
    if check_file:
        pin({"path": str(inputs), "sha256": inputs_sha256})
    extras = {"AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS": str(inputs),
              "AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS_SHA256": inputs_sha256}
    if layers is not None:
        tokens = layers.split(",")
        # Tuning tokens (cs1e, R/design/cs1-tuning-tokens.md) carry values: pool=8, subjects=strict, ...
        require(layers and all(token and all(c.isalnum() or c in "_-=" for c in token)
                               and not token.startswith("--") and token.count("=") <= 1 for token in tokens),
                "answer layers are comma-separated names, each optionally prefixed with '-' or given as name=value")
        extras["AIRK_ARBITER_EVALUATION_ANSWER_LAYERS"] = layers
    if restart:
        require(seed is None, "a change-set restart never seeds the CPU roster")
    else:
        require(seed in ("all", "focus", "none"), "a fresh change-set lifetime needs --seed-cpu all|focus|none")
        extras["AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU"] = seed
    return extras


SEARCH_SWITCHES = ("none", "search_v2")


def search_extras(search):
    """The service environment for the search switch: unset when not asked, else the named value."""
    if search is None:
        return {}
    require(search in SEARCH_SWITCHES, "unknown search switch " + str(search))
    return {"AIRK_ARBITER_EVALUATION_SEARCH": search}


def add_selector_arguments(parser):
    parser.add_argument("--selector", choices=sorted(SELECTORS), default="software-v2")
    parser.add_argument("--change-set-inputs", type=Path)
    parser.add_argument("--change-set-inputs-sha256")
    parser.add_argument("--answer-layers")
    parser.add_argument("--seed-cpu", choices=("all", "focus", "none"))
    # Change set 2 retrieval switch (R/design/search-v2.md): passed to the service as
    # AIRK_ARBITER_EVALUATION_SEARCH, which service_environment would otherwise strip with every AIRK_*.
    parser.add_argument("--search", choices=SEARCH_SWITCHES)


def selector_arguments(args):
    """The selector options as worker argv (the outer launcher validates them before launch)."""
    selector_extras(args.selector, bool(args.restart_from), args.change_set_inputs, args.change_set_inputs_sha256,
                    args.answer_layers, args.seed_cpu)
    argv = []
    for flag, value in (("--change-set-inputs", args.change_set_inputs),
                        ("--change-set-inputs-sha256", args.change_set_inputs_sha256),
                        ("--answer-layers", args.answer_layers), ("--seed-cpu", args.seed_cpu),
                        ("--search", getattr(args, "search", None))):
        if value is not None:
            argv += [flag, str(value)]
    return argv


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read_json(path):
    return json.loads(Path(path).read_bytes())


def write_json(path, value, *, replace=False):
    """Write UTF-8 LF JSON; fresh files only unless the caller owns a replaceable record."""
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    path = Path(path)
    if replace:
        temporary = path.with_name(path.name + ".tmp")
        temporary.write_bytes(data)
        os.replace(temporary, path)
    else:
        with path.open("xb") as stream:
            stream.write(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def checked_name(value):
    require(value and len(value) <= 64 and all(c.isalnum() or c in "-_." for c in value)
            and not value.startswith("."), "lifetime names use letters, digits, '-', '_' and '.'")
    return value


def lifetime_dir(name, fixtures=FIXTURES):
    return fixtures / checked_name(name)


def native():
    return runpy.run_path(str(NATIVE))


def pin(value):
    path = Path(value["path"])
    require(path.is_file() and sha256(path) == value["sha256"], "pinned file changed: " + str(path))
    return path


def image_sha256(entry):
    """Both images.json shapes: {"image": {"sha256": …}} (baseline) and {"sha256": …} (change sets)."""
    return entry["image"]["sha256"] if "image" in entry else entry["sha256"]


def image_pins(image):
    record = read_json(image / "images.json")
    service = {"path": str(image / "airk-service-fixture.exe"), "sha256": image_sha256(record["service"])}
    cli = {"path": str(image / "airkctl.exe"), "sha256": image_sha256(record["cli"])}
    pin(service)
    pin(cli)
    return {"label": record["label"], "images_json": {"path": str(image / "images.json"),
                                                      "sha256": sha256(image / "images.json")},
            "service": service, "cli": cli, "git_head": record["git_head"]}


def service_environment(work, seconds, inputs, temp, extra=()):
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("AIRK_")}
    overrides = dict(ENVIRONMENT)
    overrides.update(AIRK_ARBITER_EVALUATION_INPUTS=str(inputs["path"]),
                     AIRK_ARBITER_EVALUATION_INPUTS_SHA256=inputs["sha256"],
                     AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_SECONDS=str(seconds),
                     TEMP=str(temp), TMP=str(temp))
    overrides.update(extra)
    env.update(overrides)
    return env, overrides


def client_environment(handle):
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("AIRK_")}
    env.update(handle["environment"])
    return env


class Commands:
    """Logged CLI calls; each call keeps its argv, exit code, elapsed time and output files."""

    def __init__(self, handle_or_paths, folder, deadline):
        self.native = native()
        self.cli = handle_or_paths["cli"]
        self.control = handle_or_paths["control"]
        self.env = handle_or_paths["env"]
        self.folder = Path(folder)
        self.folder.mkdir(parents=True, exist_ok=True)
        self.deadline = deadline
        self.prefix = datetime.datetime.now(datetime.timezone.utc).strftime("%H%M%S") + "-" + secrets.token_hex(2)
        self.stop_file = None  # the worker sets its lifetime stop file; long waits end early on it
        self.count = 0

    def run(self, label, *arguments, command=None, maximum=COMMAND_SECONDS, env=None):
        self.count += 1
        name = f"{self.prefix}-{self.count:04}-{label}"
        argv = command or [self.cli, "--output", "json", "--service-runtime-dir", self.control, *arguments]
        path, elapsed = self.native["run_logged"](argv, self.folder, name, env or self.env, self.deadline,
                                                  maximum=maximum)
        return path, elapsed

    def json(self, label, *arguments, **options):
        path, _ = self.run(label, *arguments, **options)
        return read_json(path)

    def lines(self, label, *arguments):
        path, _ = self.run(label, *arguments)
        return [json.loads(line) for line in path.read_bytes().splitlines() if line.strip()]

    def tool(self, label, tool, index_id, arguments=None, args_file=None, attempts=240):
        """One tool call. A typed retry (``status: retry``, ``retryable: true``, e.g. publication_busy)
        is honored by repeating the same request after its ``retry_after_ms``; every attempt is logged
        and the count is returned with the last attempt's elapsed time."""
        given = ["--args-file", str(args_file)] if args_file is not None else ["--args", json.dumps(arguments or {})]
        for attempt in range(1, attempts + 1):
            name = f"{self.prefix}-{self.count + 1:04}-{label}"
            try:
                path, elapsed = self.run(label, "tools", "call", tool, "--index", index_id, *given)
                return read_json(path), path, elapsed
            except ValueError:
                stdout = self.folder / f"{name}.stdout"
                retry = typed_retry(stdout)
                if retry is None or attempt == attempts:
                    raise
                time.sleep(min(5.0, max(0.025, retry / 1000)))
        raise AssertionError("unreachable")

    def rpc(self, label, method):
        request = self.folder / f"{self.prefix}-{self.count + 1:04}-{label}.request.json"
        request.write_bytes(json.dumps(method).encode())
        return self.json(label, "service", "rpc", "--request-file", request)


def typed_retry(stdout):
    """retry_after_ms of a retryable typed tool answer in a CLI stdout file, else None."""
    try:
        envelope = read_json(stdout)
        body = (envelope["structuredContent"] if "structuredContent" in envelope
                else json.loads(envelope["content"][0]["text"]))
    except (OSError, ValueError, KeyError, IndexError, TypeError):
        return None
    if isinstance(body, dict) and body.get("status") == "retry" and body.get("retryable") is True:
        return int(body.get("retry_after_ms") or 25)
    error = body.get("error") if isinstance(body, dict) else None
    # A typed retryable error whose query was never submitted (for example scoped_status_unavailable
    # with fresh_brokerage_required): a new CLI call is a fresh brokerage of the same request.
    if (isinstance(error, dict) and error.get("retryable") is True and error.get("query_submitted") is None
            and error.get("query_replay_safe") in (None, False, True)):
        return int(error.get("retry_after_ms") or 500)
    return None


def tool_body(envelope):
    require(envelope.get("isError") is not True, "tool returned an error: " + json.dumps(envelope)[:2000])
    if "structuredContent" in envelope:
        return envelope["structuredContent"]
    content = envelope["content"]
    require(len(content) == 1 and content[0]["type"] == "text", "unexpected tool projection")
    return json.loads(content[0]["text"])


def wait_job(commands, job_id, kind, seconds):
    end = min(commands.deadline, time.monotonic() + seconds)
    sample = 0
    while True:
        job = commands.json(f"{kind}-job", "jobs", "get", job_id)["job"]
        require(job["id"] == job_id and job["kind"] == kind, "job identity changed")
        if job["state"] == "succeeded":
            require(job["outcome"]["state"] == "succeeded", "job has no successful outcome")
            return job
        require(job["state"] in ("pending", "running"), f"{kind} job {job['state']}: {json.dumps(job)[:2000]}")
        sample += 1
        require(time.monotonic() < end, f"{kind} job did not finish within {seconds} s")
        require(commands.stop_file is None or not commands.stop_file.is_file(), "stop file written while waiting")
        # Bounded sampling of the actual job record, as the earlier fixture owners do.
        time.sleep(1.0 if sample > 20 else 0.5)


def resume_semantic(commands, states, repositories, samples=3):
    """Queue one index pass for each named index that is not semantically ready and has no
    unfinished job over ``samples`` consecutive observations. Returns what was queued."""
    queued = []
    for repo in repositories:
        state_path = states / f"{repo}.state.json"
        if not state_path.is_file():
            continue
        state = read_json(state_path)
        if not state.get("indexed"):
            continue
        status = tool_body(commands.tool("resume-status", "airk_status", state["index_id"], {"response_mode": "full"})[0])
        if status["semantic_readiness"]["state"] == "ready":
            continue
        idle = 0
        for _ in range(samples):
            jobs = unfinished_jobs(commands.json("resume-progress", "--service-project", state["binding"], "progress"))
            idle = 0 if jobs else idle + 1
            if jobs:
                break
            time.sleep(2.0)
        if idle >= samples:
            job = commands.json("semantic-resume-index", "--service-project", state["binding"], "index")["job"]
            queued.append({"repository": repo, "job": job["id"], "at": utc()})
    return queued


def wait_semantic(commands, index_id, seconds, binding=None, requeued=None):
    """Wait for the MiniLM publication. Embedding runs as a series of short ``vectors`` jobs; when
    an earlier lifetime canceled that series, nothing is queued, so one index pass is queued to
    resume it (recorded in ``requeued``)."""
    end = min(commands.deadline, time.monotonic() + seconds)
    idle = 0
    while True:
        status = tool_body(commands.tool("semantic-status", "airk_status", index_id, {"response_mode": "full"})[0])
        state = status["semantic_readiness"]["state"]
        if state == "ready":
            require(status["publication"]["token"], "semantic index lacks an actual publication")
            return status
        require(state in ("pending", "partial"), f"semantic status became {state}")
        require(time.monotonic() < end, "semantic publication did not become ready")
        require(commands.stop_file is None or not commands.stop_file.is_file(), "stop file written while waiting")
        if binding is not None and requeued is not None and not requeued:
            jobs = unfinished_jobs(commands.json("semantic-progress", "--service-project", binding, "progress"))
            idle = 0 if jobs else idle + 1
            if idle >= 3:
                job = commands.json("semantic-resume-index", "--service-project", binding, "index")["job"]
                requeued.append({"job": job["id"], "at": utc(), "indexed_vectors": status.get("indexed_vectors")})
        time.sleep(5.0)


def git(root, *arguments, text=True):
    """Read-only git query; optional locks off so a checkout under test is never written."""
    env = os.environ | {"GIT_OPTIONAL_LOCKS": "0"}
    return subprocess.run(["git", "-C", str(root), *arguments], capture_output=True, text=text, timeout=600,
                          check=True, stdin=subprocess.DEVNULL, env=env).stdout


def git_commit(root):
    return git(root, "rev-parse", "HEAD").strip()


def snapshot_root(repo, fixtures):
    commit = SNAPSHOTS[repo]
    return fixtures / "sources" / f"{repo}-{commit[:12]}"


def ensure_snapshot(repo, fixtures):
    """Export the tracked tree of one commit (git archive) as the index's read-only source root.

    The service refuses a source root that contains its control root or index storage, and the
    fixture keeps both under ``artifacts`` inside the airk checkout, so airk is indexed from this
    export. The export also pins the indexed bytes to one commit while the checkout keeps changing.
    """
    import tarfile
    commit = SNAPSHOTS[repo]
    destination = snapshot_root(repo, fixtures)
    receipt = destination.with_name(destination.name + ".json")
    if receipt.is_file():
        record = read_json(receipt)
        require(record["commit"] == commit and destination.is_dir(), "source snapshot differs from its receipt")
        return destination, record
    require(not destination.exists(), "an unfinished source snapshot exists: " + str(destination))
    require(git_commit(REPOSITORIES[repo]) == commit or git(REPOSITORIES[repo], "cat-file", "-t", commit).strip()
            == "commit", "snapshot commit is not in the checkout")
    destination.parent.mkdir(parents=True, exist_ok=True)
    archive = destination.with_name(destination.name + ".tar")
    started = time.monotonic()
    with archive.open("xb") as stream:
        stream.write(git(REPOSITORIES[repo], "archive", "--format=tar", commit, text=False))
    destination.mkdir()
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        tar.extractall(destination, filter="data")
    files = sum(1 for member in members if member.isfile())
    record = {"repository": repo, "checkout": str(REPOSITORIES[repo]), "commit": commit, "root": str(destination),
              "archive": {"path": str(archive), "sha256": sha256(archive), "bytes": archive.stat().st_size},
              "files": files, "seconds": time.monotonic() - started, "at": utc(),
              "method": "git archive --format=tar <commit>, extracted with tarfile filter='data'"}
    archive.unlink()
    write_json(receipt, record)
    return destination, record


def tree_bytes(path):
    total, files = 0, 0
    for base, _, names in os.walk(path):
        for name in names:
            try:
                total += (Path(base) / name).stat().st_size
                files += 1
            except FileNotFoundError:
                pass
    return {"bytes": total, "files": files}


# --- index setup and source policy -------------------------------------------------------------


def catalog_revision(commands):
    page = commands.json("catalog-revision", "library", "list", "--kind", "indexes")
    return page_revision(page)


def page_revision(page):
    for key in ("catalog_revision", "revision", "expected_catalog_revision"):
        if isinstance(page.get(key), int):
            return page[key]
    if isinstance(page.get("page"), dict):
        return page_revision(page["page"])
    raise ValueError("catalog page has no revision: " + json.dumps(page)[:1000])


def index_sources(commands, index_id):
    page = commands.json("sources", "library", "list", "--kind", "sources", "--index", index_id)
    items = page.get("items") or page.get("page", {}).get("items") or []
    roots = [item.get("value") or item.get("source") or item.get("root") or item for item in items]
    return page, roots


def index_policy(commands, index_id):
    return commands.json("policy-status", "service", "rpc", "--request-file",
                         write_request(commands, "policy-status", {"catalog_policy_status": {"index_id": index_id}}))


def write_request(commands, label, method):
    commands.count += 1
    path = commands.folder / f"{commands.prefix}-{commands.count:04}-{label}.request.json"
    path.write_bytes(json.dumps(method).encode())
    return path


def unwrap(response, variant):
    """service rpc prints the ControlResponse; accept the bare or wrapped projection."""
    if isinstance(response, dict):
        if response.get("status") == variant:
            return response
        if variant in response:
            return response[variant]
        for key in ("response", "result", "answer"):
            if isinstance(response.get(key), dict) and variant in response[key]:
                return response[key][variant]
    raise ValueError(f"service refused {variant}: " + json.dumps(response)[:3000])


def trim_path(commands, index_id, source_root_id, kind, path, nonce):
    epoch = unwrap(commands.json("request-epoch", "service", "rpc", "--request-file",
                                 write_request(commands, "request-epoch", "request_epoch")), "request_epoch")
    selection = {"key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": nonce},
                 "expected_catalog_revision": catalog_revision(commands), "index_id": index_id,
                 "change": {"operation": "trim_path", "source_root_id": source_root_id, "kind": kind, "path": path}}
    draft = unwrap(commands.json("trim-draft", "service", "rpc", "--request-file",
                                 write_request(commands, "trim-draft",
                                               {"catalog_source_mutation_draft": {"selection": selection}})),
                   "catalog_source_mutation_draft")["draft"]
    applied = commands.json("trim-apply", "service", "rpc", "--request-file",
                            write_request(commands, "trim-apply", {"catalog_apply": {
                                "request": draft["request"], "preview_digest": draft["preview_digest"]}}))
    require(isinstance(applied, dict) and applied.get("status") in ("catalog_applied", "catalog_receipt"),
            "trim apply was not accepted: " + json.dumps(applied)[:3000])
    return {"selection": selection, "draft_preview_digest": draft["preview_digest"], "applied": applied}


def trim_with_busy_retry(commands, index_id, source_root_id, kind, path, attempts=12, sleep=time.sleep):
    """trim_path, repeated while the service refuses with registry_busy (L16-cs1, 22 Nov 2026: a restart's
    first trim met the resumed indexer). Each attempt is a new draft with a new nonce; before each retry the
    policy is read again, so a trim that did land is never applied twice."""
    for attempt in range(1, attempts + 1):
        try:
            return trim_path(commands, index_id, source_root_id, kind, path, secrets.token_hex(16))
        except ValueError as error:
            if "registry_busy" not in str(error) or attempt == attempts:
                raise
        sleep(min(5.0 * attempt, 30.0))
        have, _ = denied_atoms(index_policy(commands, index_id))
        if (kind, path) in have:
            return {"selection": None, "draft_preview_digest": None, "applied": "already denied after a busy retry"}
    raise ValueError("unreachable")


def denied_atoms(policy_status):
    """The (kind, path) pairs of every deny in a CatalogPolicyStatus answer."""
    text = json.dumps(policy_status)
    found = set()

    def visit(value):
        if isinstance(value, dict):
            if "rule_id" in value and "kind" in value and "path" in value and "reason" in value:
                found.add((value["kind"], value["path"]))
            for item in value.values():
                visit(item)
        elif isinstance(value, list):
            for item in value:
                visit(item)
    visit(policy_status)
    return found, len(text)


def ensure_index(commands, repo, root, data_root, state_path, first_vectors, record, commit, phase="all",
                 installed=None):
    """Set up, trim, install MiniLM once, index and wait for semantic publication; reuse what exists.

    ``phase`` "lexical" stops after the index pass and the deny check, so the caller can start the next
    repository while this one's vectors build; "semantic" does only the semantic wait and the probe."""
    state = read_json(state_path) if state_path.is_file() else None
    if state is None:
        data = data_root / repo
        data.mkdir(parents=True, exist_ok=False)
        started = time.monotonic()
        lines = commands.lines(f"setup-{repo}", "--root", str(root), "--data-dir", str(data), "library", "setup",
                               "--name", f"answer-eval-{repo}", "--watch", "off", "--apply")
        require(len(lines) == 2 and "draft" in lines[0] and "receipt" in lines[1], "setup draft or receipt missing")
        receipt = lines[1]["receipt"]
        state = {"repository": repo, "root": str(root), "data_dir": str(data), "index_id": receipt["index_id"],
                 "binding": receipt["request"]["registration"]["binding"], "setup_seconds": time.monotonic() - started,
                 "trims": [], "indexed": None, "vectors_installed": False}
        write_json(state_path, state, replace=True)
        # Setup finishes asynchronously: its model_setup and inspect jobs run, then it queues its own
        # lexical index job. Until that job appears the project is still "setup pending", which a
        # same-install restart refuses. Wait for it, then cancel it (letting the others finish) so
        # the trims and the MiniLM install precede the one index pass this launcher queues.
        wait_setup_index(commands, state["binding"])
        state["setup_jobs_canceled"] = cancel_binding(commands, state["binding"], kinds=("index",))
        write_json(state_path, state, replace=True)
    index_id = state["index_id"]
    record["index_id"] = index_id
    if phase == "semantic":
        return finish_semantic(commands, repo, state, state_path, record)
    deferred = apply_trims(commands, repo, state, state_path, before_index=state["indexed"] is None)
    if first_vectors is not None:
        job = commands.json(f"install-minilm-{repo}", "--service-project", state["binding"], "configure",
                            "--file", str(first_vectors / "minilm-install.json"))["job"]
        wait_job(commands, job["id"], "configure", 4 * 3600)
        state["vectors_installed"] = True
        write_json(state_path, state, replace=True)
        if installed is not None:
            write_json(installed, {"installed_by": repo, "at": utc(), "minilm": str(MINILM), "job": job["id"]},
                       replace=True)
    if state["indexed"] is None:
        started = time.monotonic()
        job = commands.json(f"index-{repo}", "--service-project", state["binding"], "index")["job"]
        wait_job(commands, job["id"], "index", 4 * 3600)
        state["indexed"] = {"job": job["id"], "seconds": time.monotonic() - started, "at": utc(),
                            "source_commit": commit}
        write_json(state_path, state, replace=True)
    if deferred:
        apply_trims(commands, repo, state, state_path, before_index=False)
    if EXCLUSIONS[repo]:
        have, _ = denied_atoms(index_policy(commands, index_id))
        require(all(atom in have for atom in EXCLUSIONS[repo]), "source policy lacks a required deny")
        record["denies"] = sorted([list(atom) for atom in have])
    record.update(binding=state["binding"], root=state["root"], data_dir=state["data_dir"],
                  source_commit=state["indexed"]["source_commit"], indexing_seconds=state["indexed"]["seconds"])
    if phase == "lexical":
        return record
    return finish_semantic(commands, repo, state, state_path, record)


REPLAYABLE_PROBE_REFUSALS = ("Focus policy or source changed after native work",)


def probe_search(commands, repo, index_id, attempts=4, sleep=time.sleep):
    """The read-only one-hit probe search. A refusal caused by a concurrent stage change (L23, 22 Nov 2026:
    "Focus policy or source changed after native work") ended the lifetime; the probe is a fresh read with no
    side effect, so it is issued again, a bounded number of times."""
    for attempt in range(1, attempts + 1):
        try:
            return tool_body(commands.tool(f"probe-{repo}", "airk_search", index_id,
                                           {"query": "source", "limit": 1, "max_bytes": 4096, "response_mode": "full",
                                            "include_status": True})[0])
        except ValueError:
            stdout = sorted(commands.folder.glob(f"{commands.prefix}-*-probe-{repo}.stdout"))
            text = stdout[-1].read_text(encoding="utf-8", errors="replace") if stdout else ""
            if attempt == attempts or not any(refusal in text for refusal in REPLAYABLE_PROBE_REFUSALS):
                raise
        sleep(2.0 * attempt)
    raise ValueError("unreachable")


def finish_semantic(commands, repo, state, state_path, record):
    index_id = state["index_id"]
    requeued = []
    status = wait_semantic(commands, index_id, MAX_SECONDS, state["binding"], requeued)
    if requeued:
        state.setdefault("semantic_requeued", []).extend(requeued)
        write_json(state_path, state, replace=True)
    search = probe_search(commands, repo, index_id)
    require(search["publication"]["token"] == status["publication"]["token"]
            and search["semantic_readiness"]["state"] == "ready"
            and search["execution"]["served_mode"] in ("hybrid", "vector"), "MiniLM publication differs")
    record.update(binding=state["binding"], root=state["root"], data_dir=state["data_dir"],
                  publication_token=status["publication"]["token"], files=status.get("files"),
                  served_mode=search["execution"]["served_mode"],
                  pipeline_digest=search["semantic_readiness"].get("pipeline_digest"),
                  source_commit=state["indexed"]["source_commit"], indexing_seconds=state["indexed"]["seconds"],
                  data_bytes=tree_bytes(state["data_dir"])["bytes"])
    return record


def apply_trims(commands, repo, state, state_path, before_index):
    """Deny each benchmark path through the index's source policy. Returns True when the service
    refused a trim before the first index pass, so the caller applies it after that pass."""
    if not EXCLUSIONS[repo]:
        return False
    index_id = state["index_id"]
    _, roots = index_sources(commands, index_id)
    source_ids = sorted({entry["id"] for entry in roots if isinstance(entry, dict)
                         and str(entry.get("id", "")).startswith("src_")})
    require(len(source_ids) == 1, "expected exactly one source root for " + repo)
    for kind, path in EXCLUSIONS[repo]:
        have, _ = denied_atoms(index_policy(commands, index_id))
        if (kind, path) in have:
            continue
        try:
            outcome = trim_with_busy_retry(commands, index_id, source_ids[0], kind, path)
        except ValueError as error:
            if before_index:
                state.setdefault("trim_refusals_before_index", []).append({"kind": kind, "path": path,
                                                                            "error": str(error)[:2000]})
                write_json(state_path, state, replace=True)
                return True
            raise
        state["trims"].append({"kind": kind, "path": path, "at": utc(), "before_index": before_index,
                               "preview_digest": outcome["draft_preview_digest"]})
        write_json(state_path, state, replace=True)
        wait_policy(commands, index_id, kind, path)
    return False


def wait_policy(commands, index_id, kind, path, seconds=3600):
    end = min(commands.deadline, time.monotonic() + seconds)
    while True:
        status = unwrap(index_policy(commands, index_id), "catalog_policy_status")
        snapshot, job = status.get("snapshot"), status.get("job")
        have, _ = denied_atoms(snapshot)
        require(snapshot is None or snapshot.get("last_error") is None,
                "source policy application failed: " + str((snapshot or {}).get("last_error")))
        require(job is None or job.get("state") in ("pending", "running", "succeeded"),
                "source policy job failed: " + json.dumps(job)[:2000])
        if ((kind, path) in have and snapshot.get("application") is not None
                and (job is None or job.get("state") == "succeeded")):
            return status
        require(time.monotonic() < end, "source policy change did not settle")
        time.sleep(1.0)


def unfinished_jobs(progress):
    observation = progress.get("observation") or {}
    jobs = [observation.get("job"), progress.get("pending_job"), *(progress.get("queued_jobs") or [])]
    return {job["id"]: job for job in jobs if isinstance(job, dict) and job.get("state") in ("pending", "running")}


def wait_setup_index(commands, binding, seconds=600):
    end = min(commands.deadline, time.monotonic() + seconds)
    while True:
        jobs = unfinished_jobs(commands.json("setup-progress", "--service-project", binding, "progress"))
        if any(job["kind"] == "index" for job in jobs.values()):
            return
        require(time.monotonic() < end, "setup did not queue its initial index job")
        time.sleep(0.5)


def restart_blockers(control):
    """After the service has exited: the conditions the same-install restart guard refuses
    (evaluation.rs verify_restart_settled), read from the service registry without writing."""
    import sqlite3
    database = Path(control) / "service-1.4.sqlite3"
    connection = sqlite3.connect(f"file:{database.as_posix()}?mode=ro", uri=True)
    try:
        jobs = connection.execute("SELECT id, project, kind, state FROM service_jobs "
                                  "WHERE state IN ('pending','running')").fetchall()
        setup = connection.execute("SELECT project FROM service_setup_pending").fetchall()
    finally:
        connection.close()
    return {"unfinished_jobs": [list(row) for row in jobs], "setup_pending": [row[0] for row in setup]}


def cancel_binding(commands, binding, kinds=None, seconds=600):
    """Cancel unfinished jobs (of the given kinds) and wait until every unfinished job has settled."""
    canceled = []
    jobs = unfinished_jobs(commands.json("progress", "--service-project", binding, "progress"))
    for job_id, job in jobs.items():
        if kinds is None or job["kind"] in kinds:
            commands.json("cancel-job", "jobs", "cancel", job_id)
            canceled.append({"job": job_id, "kind": job["kind"]})
    end = time.monotonic() + seconds
    while jobs:
        require(time.monotonic() < end, "unfinished jobs did not settle")
        time.sleep(1.0)
        jobs = unfinished_jobs(commands.json("progress", "--service-project", binding, "progress"))
    return canceled


CANCELABLE = ("index", "vectors", "verify", "rebuild", "inspect")
QUIET_SAMPLES = 5


def settle_projects(commands, states, cancel, seconds=900):
    """Before shutdown: no project may hold pending or running work, because a same-install restart
    refuses it. The service queues follow-up work (for example ``verify`` after a canceled vectors
    job), so idleness must hold for several consecutive samples. With ``cancel`` false the work is
    allowed to finish; with it true, cancelable kinds are canceled as they appear. Returns
    (canceled jobs, waited seconds)."""
    bindings = {}
    for state_path in sorted(states.glob("*.state.json")) if states.is_dir() else []:
        state = read_json(state_path)
        if state.get("binding") is None:
            continue
        if "setup_jobs_canceled" not in state:
            # Setup was interrupted; its follow-up must finish before shutdown (see ensure_index).
            wait_setup_index(commands, state["binding"])
        bindings[state_path.name.split(".")[0]] = state["binding"]
    started, canceled, quiet, requested = time.monotonic(), [], 0, set()
    while quiet < QUIET_SAMPLES:
        require(time.monotonic() - started < seconds, "project work did not settle before shutdown")
        busy = False
        for repo, binding in bindings.items():
            jobs = unfinished_jobs(commands.json("settle-progress", "--service-project", binding, "progress"))
            busy = busy or bool(jobs)
            for job_id, job in jobs.items():
                if cancel and job["kind"] in CANCELABLE and job_id not in requested:
                    commands.json("cancel-job", "jobs", "cancel", job_id)
                    requested.add(job_id)
                    canceled.append({"repository": repo, "job": job_id, "kind": job["kind"]})
        quiet = 0 if busy else quiet + 1
        time.sleep(1.0)
    return canceled, time.monotonic() - started


# --- the lifetime worker -------------------------------------------------------------------------


def prior_settled(prior):
    """A lifetime can seed a restart when its service closed naturally with idle stages,
    even if the lifetime itself ended on an error after that (recorded in its handle)."""
    if prior.get("restartable") is False:
        return False
    if prior.get("state") == "closed":
        return True
    closed_path = Path(prior.get("work", "")) / "closed.json"
    if not (prior.get("final_status") and prior.get("final_stage_idle") is True
            and prior.get("service_exit_code") == 0 and closed_path.is_file()):
        return False
    closed = read_json(closed_path)
    return (closed.get("instance") == prior.get("instance") and closed.get("shutdown_completed") is True
            and closed.get("service_lease_reacquired") is True)


# One-time, recorded exceptions to prior_settled (primary agent's decision, 22 November 2026; the user
# authorized the primary agent to solve evaluation problems as it recommends). Each names one lifetime
# and the sha256 of its closed.json; no other lifetime or digest is accepted, and prior_settled itself is
# unchanged. L38-cs2a2: the harness agent's no-session Search replay had one airk_search call in flight
# (baseline/airk-A2 call 2) when the plan stopped the lifetime, so the final status showed Focus
# queued 0, running 1 and the worker refused to mark the handle closed; the service exited 0 and the
# lease was released.
UNIDLE_CLOSE_EXCEPTIONS = {
    "L38-cs2a2": "f8bacf1cd2c7a24248b4f9052b66ef4cdf7c1cc31d7f992b83e79e4311083830",
}
FOCUS_TASKS = ["airk.focus_rank.v1"]


def unidle_close_exception(prior, previous, digest, allowed=None):
    """Check, from the previous lifetime's receipts, the one accepted non-idle close; returns the record."""
    allowed = UNIDLE_CLOSE_EXCEPTIONS if allowed is None else allowed
    require(previous in allowed, f"no recorded close exception for {previous}")
    require(digest == allowed[previous], f"the close exception for {previous} names another closed.json digest")
    require(prior.get("lifetime") == previous, "the handle is not the named lifetime's")
    closed_path = Path(prior.get("work", "")) / "closed.json"
    require(closed_path.is_file() and sha256(closed_path) == digest, "closed.json differs from the recorded digest")
    closed = read_json(closed_path)
    require(closed.get("instance") == prior.get("instance") and closed.get("shutdown_completed") is True
            and closed.get("service_lease_reacquired") is True, "the private lease was not naturally released")
    require(prior.get("state") != "closed" and prior.get("final_stage_idle") is False,
            "the exception covers only a lifetime refused for a non-idle final stage")
    require(prior.get("service_exit_code") == 0, "the service did not exit 0")
    require(prior.get("restartable") is True
            and prior.get("restart_blockers") == {"setup_pending": [], "unfinished_jobs": []},
            "the installation has restart blockers")
    require(prior.get("canceled_jobs") == [], "the settlement cancelled jobs")
    final = prior.get("final_status") or {}
    require(bool(final.get("path")) and Path(final["path"]).is_file() and sha256(final["path"]) == final.get("sha256"),
            "the final status receipt is missing or changed")
    status = read_json(final["path"])
    focus, context = status["focus"], status["context"]
    require(focus.get("queued") == 0 and focus.get("running") == 1, "Focus was not exactly one running item")
    require(context.get("queued") == 0 and context.get("running") == 0, "Context was not idle")
    active = [row for row in focus.get("profiles", []) if row.get("profile") == focus.get("active_profile")]
    require(len(active) == 1 and active[0].get("tasks") == FOCUS_TASKS, "the running Focus item is not a rank read")
    warm = Path(prior["work"]) / "warm-completions.closed.json"
    if warm.is_file():
        summary = read_json(warm)
        require(summary.get("complete") is True and summary.get("dropped") == 0, "warm completions did not close")
    return {"lifetime": previous, "closed_sha256": digest, "final_status": final,
            "focus": {"queued": 0, "running": 1, "tasks": FOCUS_TASKS}, "context": {"queued": 0, "running": 0},
            "service_exit_code": 0, "restart_blockers": prior["restart_blockers"], "canceled_jobs": [],
            "reason": "one-time recorded exception (primary agent, 22 November 2026): one in-flight Focus rank "
                      "read from a no-session Search replay at the stop; everything else settled",
            "checked_utc": utc()}


def attach_verification(handle, life):
    """Copy the harness's post-restart verification receipt (verify_restart.py) into the handle at close."""
    path = Path(life) / "post-restart-verification.json"
    if path.is_file():
        result = read_json(path)
        handle["post_restart_verification"] = {"path": str(path), "sha256": sha256(path),
                                               "passed": result.get("passed"), "problems": result.get("problems")}


def restart_request(fixtures, previous, folder, accept_unidle_close=None):
    """Pin the original Ready/Closed and the previous lifetime's final stage status.

    ``accept_unidle_close`` (``<lifetime>=<closed.json sha256>``) applies a recorded exception from
    UNIDLE_CLOSE_EXCEPTIONS when prior_settled refuses; the checked record is written beside the request.
    """
    prior = read_json(lifetime_dir(previous, fixtures) / "handle.json")
    if not prior_settled(prior):
        require(accept_unidle_close is not None, "the previous lifetime did not close cleanly")
        name, _, digest = accept_unidle_close.partition("=")
        require(name == previous, "the close exception names another lifetime than --restart-from")
        write_json(Path(folder) / "restart-exception.json", unidle_close_exception(prior, previous, digest))
    origin = lifetime_dir(prior["control_owner"], fixtures) / "fixture"
    status = Path(prior["final_status"]["path"])
    request = {name: {"path": str(path), "sha256": sha256(path)}
               for name, path in (("ready", origin / "ready.json"), ("closed", origin / "closed.json"),
                                  ("status", status))}
    pinned = write_json(folder / "restart-request.json", request)
    return prior, pinned


def worker(args):
    life = Path(args.lifetime_path)
    require(os.environ.get(OWNER_VARIABLE) == str(life), "the worker runs only under its owned Job")
    images = image_pins(Path(args.image))
    inputs = {"path": str(args.inputs), "sha256": args.inputs_sha256}
    pin(inputs)
    work, logs, temp = life / "fixture", life / "commands", life / "temp"
    temp.mkdir()
    seconds = args.seconds
    deadline = time.monotonic() + seconds
    extra = {"AIRK_NATIVE_DISPATCH_OBSERVER": "true" if args.dispatch_observer else "false"}
    prior = None
    if args.restart_from:
        prior, restart = restart_request(Path(args.fixtures), args.restart_from, life,
                                         getattr(args, "accept_unidle_close", None))
        control = Path(prior["control"])
        extra.update(AIRK_ARBITER_EVALUATION_RESTART=restart["path"], AIRK_ARBITER_EVALUATION_RESTART_SHA256=restart["sha256"])
        control_owner = prior["control_owner"]
    else:
        control = work / "control"
        control_owner = life.name
    extra.update(selector_extras(args.selector, bool(args.restart_from), args.change_set_inputs,
                                 args.change_set_inputs_sha256, args.answer_layers, args.seed_cpu))
    extra.update(search_extras(getattr(args, "search", None)))
    env, overrides = service_environment(work, seconds, inputs, temp, extra)
    paths = {"cli": images["cli"]["path"], "control": str(control), "env": env}
    # Long waits end before the shutdown margin, so settlement and shutdown fit inside the lifetime.
    commands = Commands(paths, logs, deadline - SHUTDOWN_MARGIN)
    commands.stop_file = life / "STOP"
    nat = commands.native
    handle_path = life / "handle.json"
    handle = {"schema": HANDLE_SCHEMA, "state": "starting", "lifetime": life.name, "started_utc": utc(),
              "image": images, "cli": images["cli"]["path"], "control": str(control),
              "control_owner": control_owner, "restart_from": args.restart_from, "work": str(work),
              "selector": SELECTORS[args.selector],
              "environment": {k: v for k, v in overrides.items() if k.startswith("AIRK_MCP")},
              "service_environment": overrides, "inputs": inputs, "stop_file": str(life / "STOP"),
              "search": getattr(args, "search", None),
              "deadline_utc": (datetime.datetime.now(datetime.timezone.utc)
                               + datetime.timedelta(seconds=seconds - SHUTDOWN_MARGIN)).isoformat(),
              "helpers": {"fixture.py": sha256(Path(__file__)), "run_compact_answer.py": sha256(NATIVE),
                          "provision-static-onnx-runtime.py": sha256(OWNER)},
              "indexes": {}, "stages": {}}
    if (life / "restart-exception.json").is_file():
        handle["restart_exception"] = read_json(life / "restart-exception.json")
        handle["restart_exception"]["receipt"] = {"path": str(life / "restart-exception.json"),
                                                  "sha256": sha256(life / "restart-exception.json")}
    write_json(handle_path, handle, replace=True)
    command = [images["service"]["path"], "--ignored", "--exact", SELECTORS[args.selector], "--test-threads", "1"]
    ready = None
    reason = None
    with (life / "service.log").open("xb") as log:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            nat["wait_observed"](life, lambda: True if (work / "ready.json").is_file() else None, process,
                                 min(deadline, time.monotonic() + 1800), "fixture readiness")
            ready = read_json(work / "ready.json")
            require(ready["health"]["process_id"] == process.pid
                    and ready["health"]["executable_sha256"] == images["service"]["sha256"]
                    and ready["inputs_sha256"] == inputs["sha256"] and control.samefile(ready["control_root"]),
                    "ready receipt differs from the launched fixture")
            if prior is not None:
                saved = read_json(work / "restart-owner.json")
                require(Path(saved["control"]).samefile(control) and not (work / "control").exists(),
                        "same-install restart did not bind the saved control")
            handle.update(state="indexing", service_pid=process.pid, instance=ready["health"]["instance"],
                          ready={"path": str(work / "ready.json"), "sha256": sha256(work / "ready.json")},
                          profiles=ready["profiles"])
            write_json(handle_path, handle, replace=True)
            states = Path(args.fixtures) / "indexes"
            states.mkdir(exist_ok=True)
            # MiniLM is installed once per installation, through the first index's project.
            installed = states / "minilm.state.json"
            vectors = None
            earlier = [read_json(path) for path in sorted(states.glob("*.state.json")) if path != installed]
            if not installed.is_file() and any(state.get("vectors_installed") for state in earlier):
                write_json(installed, {"installed_by": "an earlier lifetime (recorded in its repository state)",
                                       "at": utc(), "minilm": str(MINILM)})
            if args.repositories and not installed.is_file():
                vectors = life / "semantic-drafts"
                nat["run_logged"](
                    nat["ignored"](images["service"]["path"], "emit_private_selected_vector_configuration"),
                    logs, "emit-minilm", env | {"AIRK_SELECTED_MINILM_DIRECTORY": str(MINILM),
                                                "AIRK_SELECTED_VECTOR_DRAFT_OUTPUT": str(vectors)}, deadline)
            # Phase 1 runs every repository's setup, trims and lexical pass in turn; the service keeps
            # building the earlier repositories' vectors meanwhile. Phase 2 waits for each to be ready.
            roots = {}
            for repo in args.repositories:
                record = {"repository": repo}
                handle["indexes"][repo] = record
                if repo in SNAPSHOTS:
                    root, snapshot = ensure_snapshot(repo, Path(args.fixtures))
                    record["snapshot"] = snapshot
                    commit = snapshot["commit"]
                else:
                    root, commit = REPOSITORIES[repo], git_commit(REPOSITORIES[repo])
                    record["indexed_in_place"] = True
                roots[repo] = (root, commit)
                ensure_index(commands, repo, root, states, states / f"{repo}.state.json", vectors, record, commit,
                             phase="lexical", installed=installed)
                vectors = None
                write_json(handle_path, handle, replace=True)
            # After a restart the service may resume only some projects' embedding series; resume
            # every idle, not-ready index now so all build concurrently while phase 2 waits in turn.
            handle["semantic_resumed"] = resume_semantic(commands, states, args.repositories)
            # Clients may query as soon as every index has a lexical publication: state "ready" and
            # queryable true; "semantic" says per index whether its MiniLM vectors are complete.
            handle["semantic"] = {}
            for repo in args.repositories:
                status = tool_body(commands.tool(f"queryable-{repo}", "airk_status",
                                                 handle["indexes"][repo]["index_id"], {"response_mode": "full"})[0])
                handle["indexes"][repo]["lexical_publication_token"] = status["publication"]["token"]
                handle["semantic"][repo] = status["semantic_readiness"]["state"]
            handle.update(state="ready", queryable=True, ready_utc=utc())
            write_json(handle_path, handle, replace=True)
            for repo in args.repositories:
                root, commit = roots[repo]
                ensure_index(commands, repo, root, states, states / f"{repo}.state.json", None,
                             handle["indexes"][repo], commit, phase="semantic")
                handle["semantic"][repo] = "ready"
                handle["indexes"][repo]["semantic_ready_utc"] = utc()
                write_json(handle_path, handle, replace=True)
            handle.update(semantic_ready_utc=utc())
            write_json(handle_path, handle, replace=True)
            stop = life / "STOP"
            try:
                nat["wait_observed"](life, lambda: True if stop.is_file() else None, process,
                                     deadline - SHUTDOWN_MARGIN, "stop file", file_changes=0x1)
                reason = "stop file"
            except TimeoutError:
                reason = "lifetime deadline"
        except BaseException as error:
            reason = f"error: {type(error).__name__}: {error}"
            handle["error"] = reason
            raise
        finally:
            handle["stop_reason"] = reason
            if process.poll() is None and (work / "ready.json").is_file():
                try:
                    # A same-install restart refuses unfinished work. After a clean stop the
                    # work is allowed to finish; after an error (or if it will not finish) it
                    # is canceled and the next lifetime queues it again.
                    commands.deadline = min(time.monotonic() + 2700, deadline - 30)
                    states_dir = Path(args.fixtures) / "indexes"
                    settled = None
                    natural = min(1800, deadline - time.monotonic() - 600)
                    if reason == "stop file" and natural > 60:
                        try:
                            settled = settle_projects(commands, states_dir, cancel=False, seconds=natural)
                        except ValueError:
                            settled = None
                    if settled is None:
                        settled = settle_projects(commands, states_dir, cancel=True, seconds=900)
                    handle["canceled_jobs"], handle["settle_seconds"] = settled
                    final, _ = commands.run("final-status", "arbiter", "status-legacy")
                    handle["final_status"] = {"path": str(final), "sha256": sha256(final)}
                    state = read_json(final)
                    handle["final_stage_idle"] = all(state[s]["queued"] == state[s]["running"] == 0
                                                     for s in ("focus", "context"))
                finally:
                    commands.run("shutdown", "service", "shutdown")
            handle["service_exit_code"] = process.wait(timeout=COMMAND_SECONDS)
            if (control / "service-1.4.sqlite3").is_file():
                blockers = restart_blockers(control)
                handle["restart_blockers"] = blockers
                handle["restartable"] = not blockers["unfinished_jobs"] and not blockers["setup_pending"]
            attach_verification(handle, life)
            write_json(handle_path, handle, replace=True)
    require(handle.get("restartable") is True,
            "the installation would refuse a same-install restart: " + json.dumps(handle.get("restart_blockers")))
    closed = read_json(work / "closed.json")
    require(ready is not None and closed["instance"] == ready["health"]["instance"]
            and closed["shutdown_completed"] is True and closed["service_lease_reacquired"] is True,
            "the private lease was not naturally released")
    settlement = {"closed": {"path": str(work / "closed.json"), "sha256": sha256(work / "closed.json")}}
    for name in ("warm-completions.closed.json", "composed-observer.closed.json"):
        if (work / name).is_file():
            summary = read_json(work / name)
            require(summary["complete"] is True and summary["dropped"] == 0, name + " did not close completely")
            settlement[name] = {"path": str(work / name), "sha256": sha256(work / name)}
    require(handle["service_exit_code"] == 0 and handle.get("final_stage_idle") is True,
            "fixture exit or stage settlement failed")
    handle.update(state="closed", closed_utc=utc(), settlement=settlement)
    write_json(handle_path, handle, replace=True)


def start(args):
    owner = runpy.run_path(str(OWNER))
    fixtures = Path(args.fixtures)
    life = lifetime_dir(args.lifetime, fixtures)
    require(fixtures.is_dir() and not life.exists(), "a fresh lifetime folder under an existing fixtures root is required")
    require(0 < args.seconds <= MAX_SECONDS, f"the lifetime is at most {MAX_SECONDS} s")
    for repo in args.repositories:
        require(repo in REPOSITORIES, "unknown repository " + repo)
    life.mkdir()
    argv = [sys.executable, "-I", "-B", "-S", str(Path(__file__).resolve()), "--fixtures", str(fixtures),
            "worker", "--lifetime-path", str(life), "--image", str(Path(args.image).resolve()), "--inputs", str(args.inputs),
            "--inputs-sha256", args.inputs_sha256, "--seconds", str(args.seconds),
            "--repositories", ",".join(args.repositories)]
    argv += ["--selector", args.selector] + selector_arguments(args)
    if args.restart_from:
        argv += ["--restart-from", args.restart_from]
        if args.accept_unidle_close:
            prior = read_json(lifetime_dir(args.restart_from, fixtures) / "handle.json")
            name, _, digest = args.accept_unidle_close.partition("=")
            require(name == args.restart_from and not prior_settled(prior),
                    "--accept-unidle-close applies only to the --restart-from lifetime when it did not close cleanly")
            unidle_close_exception(prior, name, digest)  # validated before launch; the worker checks again
            argv += ["--accept-unidle-close", args.accept_unidle_close]
    if not args.dispatch_observer:
        argv.append("--no-dispatch-observer")
    before = sha256(Path(__file__))
    result = owner["run_evaluation_fixture"](argv, ROOT, life / "outer.log", args.seconds + 600,
                                             os.environ | {OWNER_VARIABLE: str(life)})
    result["fixture_sha256_unchanged"] = sha256(Path(__file__)) == before
    write_json(life / "owned.json", result)
    handle = read_json(life / "handle.json") if (life / "handle.json").is_file() else {}
    ok = (owner["successful"](result) and result["owned_job_final_active_processes"] == 0
          and result["owned_job_final_member_pids"] == [] and handle.get("state") == "closed")
    print(json.dumps({"lifetime": life.name, "state": handle.get("state"), "settled": ok,
                      "stop_reason": handle.get("stop_reason")}))
    require(ok, "the fixture lifetime did not settle; see " + str(life))


def load_handle(args, states=("ready",)):
    life = lifetime_dir(args.lifetime, Path(args.fixtures))
    handle = read_json(life / "handle.json")
    require(handle["schema"] == HANDLE_SCHEMA and handle["state"] in states,
            f"lifetime state is {handle['state']}, expected {states}")
    return life, handle


def client_commands(life, handle, folder, seconds):
    paths = {"cli": handle["cli"], "control": handle["control"], "env": client_environment(handle)}
    return Commands(paths, life / folder, time.monotonic() + seconds)


def stop(args):
    life = lifetime_dir(args.lifetime, Path(args.fixtures))
    require((life / "handle.json").is_file(), "no such lifetime")
    with (life / "STOP").open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(utc() + "\n")
    print(json.dumps({"lifetime": life.name, "stop_file": str(life / "STOP")}))


# --- stages -----------------------------------------------------------------------------------


def profile_tasks(work, ready, manifest):
    for number, entry in enumerate(ready["profiles"]):
        if entry["manifest"] == manifest:
            profile = read_json(work / entry["snapshot"])
            require(profile["manifest"] == manifest and profile["qualification"] is None
                    and sha256(work / entry["snapshot"]) == entry["profile_sha256"], "profile snapshot changed")
            return profile["tasks"], number
    raise ValueError("profile is not part of this fixture installation: " + manifest)


EMITTER = "emit_private_arbiter_configuration"


def external_declaration(roots):
    """The read-only declaration the Rust emitter requires for checkouts outside artifacts."""
    # The emitter requires the declaration to name exactly the roots outside artifacts.
    artifacts = (ROOT / "artifacts").resolve()
    outside = [str(root) for root in roots if not Path(root).resolve().is_relative_to(artifacts)]
    return json.dumps({"mode": "read-only", "source_roots": outside})


def emitter_environment(handle, scopes, stage, enabled, manifest, prefix, roots):
    """Environment for the fixture's own configuration emitter (it writes <work>/<prefix>.configure.json)."""
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("AIRK_")}
    env.update(handle["service_environment"])
    env.update(AIRK_ARBITER_EVALUATION_SCOPES=scopes["path"], AIRK_ARBITER_EVALUATION_SCOPES_SHA256=scopes["sha256"],
               AIRK_ARBITER_EVALUATION_PROFILE=manifest, AIRK_ARBITER_EVALUATION_STAGE=stage,
               AIRK_ARBITER_EVALUATION_ENABLED="true" if enabled else "false",
               AIRK_ARBITER_EVALUATION_CONTROL_PREFIX=prefix,
               AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES=external_declaration(roots))
    return env


def wait_warm(work, ready, stage, manifest, revision, seconds):
    end = time.monotonic() + seconds
    journal = work / "warm-completions.jsonl"
    while True:
        if journal.is_file():
            for line in journal.read_bytes().splitlines():
                row = json.loads(line)
                if row["stage"] == stage and row["accepted_configure_revision"] == revision:
                    require(row["profile_manifest"] == manifest and row["observer_id"] == ready["warm_observer_id"],
                            "warm completion identity differs")
                    require(row["outcome"] == "ready", "native warm failed: " + json.dumps(row)[:2000])
                    return row
        require(time.monotonic() < end, f"{stage} warm completion did not arrive within {seconds} s")
        time.sleep(0.5)


def set_stage(life, handle, profile_name, enabled, repositories, seconds=900):
    """Enable or disable one stage for the named repositories and wait until the stage settles."""
    profile = PROFILES[profile_name]
    if profile["device"] != "cpu":
        raise NotImplementedError(f"{profile_name}: {profile['missing']}")
    stage, manifest = profile["stage"], profile["manifest"]
    work = Path(handle["work"])
    ready = read_json(work / "ready.json")
    tasks, _ = profile_tasks(work, ready, manifest)
    commands = client_commands(life, handle, "stage-commands", seconds + 60)
    index_ids = sorted(handle["indexes"][repo]["index_id"] for repo in repositories)
    roots = [handle["indexes"][repo]["root"] for repo in repositories]
    label = ("enable-" if enabled else "disable-") + profile_name
    prefix = f"{commands.prefix}-{label}"[:64]
    scopes = write_json(commands.folder / f"{prefix}.scopes.json", {"source_roots": roots})
    started = time.monotonic()
    commands.run("emit-" + label, command=[handle["image"]["service"]["path"], "--ignored", "--exact",
                                           "service_runtime::evaluation_fixture_tests::" + EMITTER,
                                           "--test-threads", "1"],
                 env=emitter_environment(handle, scopes, stage, enabled, manifest, prefix, roots))
    path = work / f"{prefix}.configure.json"
    request = read_json(path)
    require(request["stage"] == stage and request["enabled"] is enabled and request["selected_profile"] == manifest
            and sorted(entry["index_id"] for entry in request["indexes"]) == index_ids
            and all(entry["tasks"] == tasks for entry in request["indexes"]),
            "emitted enrollment differs from the requested repositories")
    applied = commands.json("configure-" + label, "arbiter", "configure", "--legacy", "--file", str(path))
    revision = applied["configuration"]["revision"]
    warm = wait_warm(work, ready, stage, manifest, revision, seconds) if enabled else None
    end = time.monotonic() + seconds
    while True:
        state = commands.json("stage-after", "arbiter", "status-legacy")
        stage_state = state[stage]
        idle = stage_state["queued"] == stage_state["running"] == 0
        wanted = "ready" if enabled else "disabled"
        if idle and stage_state["availability"] == wanted:
            break
        require(time.monotonic() < end, f"{stage} did not reach {wanted}")
        time.sleep(0.5)
    require(state["configuration"]["revision"] == revision and state["configuration"][stage]["enabled"] is enabled
            and (not enabled or stage_state["active_profile"] == stage_state["selected_profile"] == manifest),
            "stage state differs from the accepted configuration")
    record = {"profile": profile_name, "manifest": manifest, "enabled": enabled, "revision": revision,
              "repositories": list(repositories), "seconds": time.monotonic() - started, "at": utc(),
              "warm_sequence": None if warm is None else warm.get("sequence")}
    return record


def stage(args):
    life, handle = load_handle(args)
    repositories = args.repositories or list(handle["indexes"])
    record = set_stage(life, handle, args.profile, args.enable, repositories)
    log = life / "stages.jsonl"
    with log.open("a", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(record, sort_keys=True) + "\n")
    print(json.dumps(record))


# --- index progress, late additions -------------------------------------------------------------


def registry_job_events(control, since_ms):
    """(project, job, kind, event, observed_ms) rows from the service registry, read-only."""
    import sqlite3
    database = Path(control) / "service-1.4.sqlite3"
    connection = sqlite3.connect(f"file:{database.as_posix()}?mode=ro", uri=True, timeout=5)
    try:
        return connection.execute(
            "SELECT project, job, job_kind, kind, observed_ms FROM service_events "
            "WHERE observed_ms >= ?1 AND job IS NOT NULL ORDER BY seq", (since_ms,)).fetchall()
    finally:
        connection.close()


def jobs_per_hour(events, binding, kind, window_ms, now_ms):
    done = [row for row in events if row[0] == binding and row[2] == kind and row[3] == "job_succeeded"
            and row[4] >= now_ms - window_ms]
    return len(done) * 3_600_000 / window_ms


def project(samples, now, horizon=1800, minimum=900):
    """Finish time from the completed-artifact rate over the last ``horizon`` seconds of samples."""
    # Only samples of the newest sample's lifetime: a restart gap is not indexing throughput.
    lifetime = samples[-1].get("lifetime") if samples else None
    recent = [s for s in samples if now - s["unix"] <= horizon and s.get("lifetime") == lifetime] or samples[-1:]
    if (len(recent) < 2 or recent[-1]["unix"] - recent[0]["unix"] < minimum
            or recent[-1]["completed"] <= recent[0]["completed"]):
        return None
    rate = (recent[-1]["completed"] - recent[0]["completed"]) / (recent[-1]["unix"] - recent[0]["unix"])
    remaining = recent[-1]["selected"] - recent[-1]["completed"]
    return {"artifacts_per_hour": round(rate * 3600, 1), "remaining_artifacts": remaining,
            "projected_finish_utc": datetime.datetime.fromtimestamp(now + remaining / rate,
                                                                    datetime.timezone.utc).isoformat(),
            "projected_hours": round(remaining / rate / 3600, 2)}


def next_restart(handle, fixtures):
    planned = fixtures / "planned-restart.json"
    if planned.is_file():
        row = read_json(planned)
        if row.get("lifetime") == handle["lifetime"]:
            return row["planned_restart_utc"]
    return handle.get("deadline_utc")


def progress(args):
    """Sample each index's semantic progress, append it to progress-samples.jsonl, rewrite progress.json."""
    life, handle = load_handle(args, ("indexing", "ready"))
    fixtures = Path(args.fixtures)
    commands = client_commands(life, handle, "progress-commands", 600)
    now = time.time()
    samples_path = fixtures / "progress-samples.jsonl"
    history = [json.loads(line) for line in samples_path.read_text(encoding="utf-8").splitlines()
               if line.strip()] if samples_path.is_file() else []
    try:
        events = registry_job_events(handle["control"], int((now - 3600) * 1000))
    except Exception as error:  # the registry may be briefly locked; the tool status still counts
        events, event_error = [], f"{type(error).__name__}: {error}"
    else:
        event_error = None
    rows, out = [], {}
    pid = handle.get("service_pid")
    for state_path in sorted((fixtures / "indexes").glob("*.state.json")):
        state = read_json(state_path)
        if "index_id" not in state:
            continue
        repo = state["repository"]
        try:
            body = tool_body(commands.tool("progress-status", "airk_status", state["index_id"],
                                           {"response_mode": "full"})[0])
        except ValueError as error:
            out[repo] = {"error": str(error)[:500]}
            continue
        readiness = body["semantic_readiness"]
        sample = {"unix": now, "utc": utc(), "lifetime": life.name, "repository": repo,
                  "state": readiness["state"], "completed": readiness.get("completed_artifacts", 0),
                  "selected": readiness.get("selected_artifacts", 0), "vectors": body.get("indexed_vectors"),
                  "chunks": body.get("chunks"), "files": body.get("files"),
                  "vectors_jobs_per_hour_last_20_min": round(jobs_per_hour(events, state["binding"], "vectors",
                                                                           1_200_000, int(now * 1000)), 1),
                  "vectors_jobs_per_hour_last_60_min": round(jobs_per_hour(events, state["binding"], "vectors",
                                                                           3_600_000, int(now * 1000)), 1)}
        rows.append(sample)
        series = [s for s in history if s["repository"] == repo] + [sample]
        out[repo] = {**sample, "projection": project(series, now) if readiness["state"] != "ready" else None}
    cpu = None
    if pid:
        try:
            listing = subprocess.run(["powershell", "-NoProfile", "-Command",
                                      f"(Get-Process -Id {int(pid)}).TotalProcessorTime.TotalSeconds"],
                                     capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
            cpu = float(listing.stdout.strip())
        except (OSError, ValueError, subprocess.TimeoutExpired):
            cpu = None
    with samples_path.open("a", encoding="utf-8", newline="\n") as stream:
        for sample in rows:
            stream.write(json.dumps(dict(sample, service_cpu_seconds=cpu), sort_keys=True) + "\n")
    report = {"schema": "airk.private.answer-eval-index-progress/1", "written_utc": utc(), "lifetime": life.name,
              "service_pid": pid, "service_cpu_seconds": cpu, "registry_event_error": event_error,
              "image": {"label": handle["image"]["label"], "service_sha256": handle["image"]["service"]["sha256"]},
              "handle_state": handle["state"], "queryable": handle.get("queryable", False),
              "semantic": handle.get("semantic"),
              # The lifetime ends at its deadline; a chained restart follows. An earlier planned
              # restart is announced through progress-notes.json "planned_restart_utc".
              "next_restart_utc": next_restart(handle, fixtures),
              "events": read_json(fixtures / "progress-notes.json") if (fixtures / "progress-notes.json").is_file() else [],
              "indexes": out, "note": "vectors totals are not known in advance; completed/selected count "
                                      "artifacts (files) whose vectors are published"}
    write_json(fixtures / "progress.json", report, replace=True)
    print(json.dumps({repo: {k: v for k, v in row.items() if k in ("state", "completed", "selected", "vectors",
                                                                    "projection")} for repo, row in out.items()}))


def pid_alive(pid):
    import ctypes
    handle = ctypes.windll.kernel32.OpenProcess(0x1000, False, int(pid))  # PROCESS_QUERY_LIMITED_INFORMATION
    if not handle:
        return False
    code = ctypes.c_ulong()
    ctypes.windll.kernel32.GetExitCodeProcess(handle, ctypes.byref(code))
    ctypes.windll.kernel32.CloseHandle(handle)
    return code.value == 259  # STILL_ACTIVE


def wait_condition(life, handle, commands, fixtures):
    """One check: returns (reason, details) when the overnight wait should end, else (None, details)."""
    handle = read_json(life / "handle.json")
    details = {"state": handle.get("state"), "semantic": handle.get("semantic")}
    if handle.get("stop_reason") or handle.get("state") == "closed":
        return "lifetime_closed", details | {"stop_reason": handle.get("stop_reason")}
    deadline = datetime.datetime.fromisoformat(handle["deadline_utc"])
    left = (deadline - datetime.datetime.now(datetime.timezone.utc)).total_seconds()
    details["seconds_to_deadline"] = round(left)
    if left <= 20 * 60:
        return "near_deadline", details
    if handle.get("service_pid") and not pid_alive(handle["service_pid"]):
        return "service_died", details
    log = life / "service.log"
    tail = log.read_bytes()[-65536:].decode("utf-8", "replace") if log.is_file() else ""
    if "panicked" in tail or "\nError:" in tail or "FAILED" in tail:
        return "service_log_error", details | {"log_tail": tail[-2000:]}
    ready = True
    for repo, record in handle.get("indexes", {}).items():
        state = read_json(fixtures / "indexes" / f"{repo}.state.json")
        status = tool_body(commands.tool("wait-status", "airk_status", state["index_id"], {"response_mode": "full"})[0])
        jobs = unfinished_jobs(commands.json("wait-progress", "--service-project", state["binding"], "progress"))
        busy = [job["kind"] for job in jobs.values() if job["kind"] in ("index", "vectors", "verify", "rebuild")]
        details[repo] = {"semantic": status["semantic_readiness"]["state"], "busy": busy}
        if status["semantic_readiness"]["state"] != "ready" or busy:
            ready = False
            continue
        search = tool_body(commands.tool("wait-search", "airk_search", state["index_id"],
                                         {"query": "source", "limit": 1, "max_bytes": 4096,
                                          "response_mode": "full"})[0])
        details[repo]["served_mode"] = search["execution"]["served_mode"]
        ready = ready and search["execution"]["served_mode"] == "hybrid"
    if ready and handle.get("indexes"):
        return "all_ready", details
    return None, details


def wait(args):
    """Block until the lifetime nears its deadline or closes, every index is ready and idle, the service
    fails, or ``--hours`` pass; sample progress every ``--interval`` seconds. Prints the reason."""
    fixtures = Path(args.fixtures)
    life = lifetime_dir(args.lifetime, fixtures)
    handle = read_json(life / "handle.json")
    end = time.monotonic() + args.hours * 3600
    commands = client_commands(life, handle, "wait-commands", args.hours * 3600 + 600)
    while True:
        try:
            progress(argparse.Namespace(fixtures=fixtures, lifetime=args.lifetime))
        except Exception as error:  # progress is advisory; the conditions below decide
            print(json.dumps({"progress_error": str(error)[:300]}), flush=True)
        try:
            reason, details = wait_condition(life, handle, commands, fixtures)
        except Exception as error:
            reason, details = "check_error", {"error": f"{type(error).__name__}: {error}"[:2000]}
        if reason is None and time.monotonic() >= end:
            reason = "hours_elapsed"
        if reason is not None:
            row = {"at": utc(), "lifetime": args.lifetime, "reason": reason, "details": details}
            with (fixtures / "overnight.jsonl").open("a", encoding="utf-8", newline="\n") as stream:
                stream.write(json.dumps(row, sort_keys=True) + "\n")
            print(json.dumps(row))
            return
        time.sleep(args.interval)


def add_index(args):
    """Set up and lexically index one more repository in a running lifetime (vectors follow in the
    service). The lifetime's worker settles its jobs at shutdown like any other index."""
    life, handle = load_handle(args, ("indexing", "ready"))
    repo = args.repository
    require(repo in REPOSITORIES and repo not in handle["indexes"], "repository is unknown or already in the lifetime")
    fixtures = Path(args.fixtures)
    commands = client_commands(life, handle, f"add-{repo}-commands", 6 * 3600)
    commands.stop_file = life / "STOP"
    if repo in SNAPSHOTS:
        root, snapshot = ensure_snapshot(repo, fixtures)
        commit = snapshot["commit"]
    else:
        root, commit = REPOSITORIES[repo], git_commit(REPOSITORIES[repo])
    record = {"repository": repo, "indexed_in_place": repo not in SNAPSHOTS}
    require((fixtures / "indexes" / "minilm.state.json").is_file(), "MiniLM is not installed in this installation")
    ensure_index(commands, repo, root, fixtures / "indexes", fixtures / "indexes" / f"{repo}.state.json", None,
                 record, commit, phase="lexical")
    print(json.dumps(record))


def settle_index(args):
    """Cancel one repository's unfinished jobs (index, vectors, verify, ...) and wait until it stays idle."""
    life, handle = load_handle(args, ("indexing", "ready"))
    commands = client_commands(life, handle, f"settle-{args.repository}-commands", 1800)
    states = Path(args.fixtures) / "indexes"
    state = read_json(states / f"{args.repository}.state.json")
    folder = life / "settle-scope"
    folder.mkdir(exist_ok=True)
    scoped = folder / f"{args.repository}.state.json"
    write_json(scoped, state, replace=True)
    canceled, seconds = settle_projects(commands, folder, cancel=True, seconds=900)
    scoped.unlink()
    print(json.dumps({"repository": args.repository, "canceled": canceled, "seconds": round(seconds, 1)}))


# --- leak probe ---------------------------------------------------------------------------------


def excluded(path, repo="airk"):
    path = str(PurePosixPath(path.replace("\\", "/")))
    for kind, rule in EXCLUSIONS[repo]:
        if path == rule or (kind == "subtree" and path.startswith(rule + "/")):
            return True
    return False


def hit_paths(value):
    """Every path a tool result names (hits, records, files)."""
    found = []

    def visit(item):
        if isinstance(item, dict):
            for key in ("path", "file", "relative_path"):
                if isinstance(item.get(key), str):
                    found.append(item[key])
            for child in item.values():
                visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)
    visit(value)
    return found


def scenario_probes(evaluation=EVALUATION):
    """airk leak_probe phrases: from the scenario freeze when it exists (each file re-pinned), else a scan."""
    freeze = evaluation / "scenario-freeze.json"
    rows = []
    if freeze.is_file():
        for relative, digest in sorted(read_json(freeze)["files"].items()):
            path = evaluation / PurePosixPath(relative)
            require(sha256(path) == digest, "frozen scenario changed: " + relative)
            scenario = read_json(path)
            if scenario.get("repository") == "airk" and scenario.get("leak_probe"):
                rows.append({"source": relative, "phrase": scenario["leak_probe"], "sha256": digest})
        return rows
    for folder in ("scenarios", "sealed"):
        for path in sorted((evaluation / folder).glob("*/*.json")):
            scenario = read_json(path)
            if scenario.get("repository") == "airk" and scenario.get("leak_probe"):
                rows.append({"source": str(path.relative_to(evaluation)), "phrase": scenario["leak_probe"]})
    return rows


class CursorExpired(Exception):
    pass


def list_files_once(commands, index_id, prefix, page_limit):
    paths, cursor, pages = [], None, 0
    while True:
        arguments = {"limit": page_limit, "response_mode": "full"}
        if prefix:
            arguments["prefix"] = prefix
        if cursor:
            arguments["cursor"] = cursor
        try:
            envelope, out, _ = commands.tool("list-files", "airk_list_files", index_id, arguments)
        except ValueError:
            stdout = sorted(commands.folder.glob(f"{commands.prefix}-*-list-files.stdout"))[-1]
            if "cursor expired" in stdout.read_text(encoding="utf-8", errors="replace"):
                raise CursorExpired(prefix) from None
            raise
        body = tool_body(envelope)
        entries = body.get("paths") or body.get("files") or body.get("entries") or []
        paths.extend(entry["path"] if isinstance(entry, dict) else entry for entry in entries)
        pages += 1
        cursor = body.get("cursor") or body.get("next_cursor")
        require(pages < 400, "file listing did not finish")
        if not cursor:
            return paths, pages


def list_all_files(commands, index_id, page_limit=100, top_entries=None, attempts=8):
    """Every indexed path. A listing cursor expires when a new publication lands (for example while
    vectors build), so an expired listing restarts; with ``top_entries`` (the root's top-level names)
    each prefix is listed and restarted on its own, which keeps every listing short."""
    prefixes = [None] if not top_entries else sorted(top_entries)
    paths, pages, restarts = set(), 0, 0
    for prefix in prefixes:
        for attempt in range(attempts):
            try:
                found, count = list_files_once(commands, index_id, prefix, page_limit)
                break
            except CursorExpired:
                restarts += 1
                require(attempt + 1 < attempts, f"file listing kept expiring for prefix {prefix!r}")
        if prefix:
            # A prefix is a string prefix; keep only paths under this top-level entry.
            found = [p for p in found if p == prefix.rstrip("/") or p.startswith(prefix)]
        paths.update(found)
        pages += count
    commands.listing_restarts = restarts
    return sorted(paths), pages


def top_level_entries(root):
    """Top-level names of a source root ("dir/" for directories) for a partitioned listing."""
    return [entry.name + "/" if entry.is_dir() else entry.name for entry in Path(root).iterdir()
            if entry.name not in (".git",)]


def leak_probe(args):
    # --while-indexing probes the lexical publication before semantic readiness; the result says so.
    life, handle = load_handle(args, ("ready", "indexing") if args.while_indexing else ("ready",))
    record = handle["indexes"].get("airk") or read_json(Path(args.fixtures) / "indexes" / "airk.state.json")
    index_id = record["index_id"]
    probes = scenario_probes(Path(args.evaluation))
    source = "scenario leak_probe phrases"
    if not probes:
        probes = [{"source": "fallback phrase from an excluded file", "phrase": phrase} for phrase in args.phrase]
        source = "fallback phrases from excluded files"
    require(probes, "no scenario probes and no fallback phrases")
    commands = client_commands(life, handle, "leak-probe-commands", 1800)
    rows, failures = [], []
    for probe in probes:
        envelope, path, elapsed = commands.tool("search", "airk_search", index_id,
                                                {"query": probe["phrase"], "limit": 16, "response_mode": "full"})
        body = tool_body(envelope)
        named = sorted(set(hit_paths(body.get("hits", body))))
        bad = [p for p in named if excluded(p)]
        rows.append({**probe, "hits": len(body.get("hits", [])), "paths": named, "excluded_hits": bad,
                     "result": {"path": str(path), "sha256": sha256(path)}, "elapsed_ms": elapsed // 1_000_000})
        failures += [{"probe": probe["phrase"], "path": p} for p in bad]
    listed, pages = list_all_files(commands, index_id, top_entries=top_level_entries(snapshot_root("airk", Path(args.fixtures))))
    indexed_excluded = sorted(p for p in listed if excluded(p))
    result = {"schema": "airk.private.answer-eval-leak-probe/1", "lifetime": life.name, "at": utc(),
              "index_id": index_id, "publication_token": record.get("publication_token"),
              "handle_state": handle["state"],
              "probe_source": source, "probes": rows, "listed_files": len(listed), "list_pages": pages,
              "indexed_excluded_paths": indexed_excluded, "exclusions": EXCLUSIONS["airk"],
              "passed": not failures and not indexed_excluded, "failures": failures}
    number = 1
    while (FIXTURES / f"leak-probe-{number}.json").exists():
        number += 1
    pinned = write_json(Path(args.fixtures) / f"leak-probe-{number}.json", result)
    print(json.dumps({"passed": result["passed"], "probes": len(rows), "listed_files": len(listed), **pinned}))
    require(result["passed"], "leak probe failed: an excluded path is served")


# --- tool smoke -------------------------------------------------------------------------------------


def smoke_calls(index, question):
    sample = index.get("sample_file")
    return [
        ("airk_search", {"query": question, "limit": 8, "response_mode": "full"}),
        ("airk_parse", {"schema": "airk.parse.request/1", "mode": "execute", "text": question,
                        "response_mode": "full"}),
        ("airk_answer", None),
        ("airk_read_file", {"path": sample, "start_line": 1, "end_line": 40, "response_mode": "full"}),
        ("airk_context", {"path": sample, "line": 10, "before": 5, "after": 5, "response_mode": "full"}),
        ("airk_outline", {"path": sample, "response_mode": "full"}),
        ("airk_symbols", {"name": index.get("sample_symbol") or "main", "response_mode": "full"}),
        ("airk_references", {"name": index.get("sample_symbol") or "main", "response_mode": "full"}),
    ]


def answer_arguments(work, question):
    template = read_json(work / "context-request.json")
    return template | {"question": question}


def smoke(args):
    # --while-indexing smokes the repositories already ready while others still build their vectors.
    life, handle = load_handle(args, ("ready", "indexing") if args.while_indexing else ("ready",))
    selected = args.repositories or [repo for repo, row in handle["indexes"].items() if row.get("publication_token")]
    require(selected and all(handle["indexes"].get(repo, {}).get("publication_token") for repo in selected),
            "every smoked repository must have a ready semantic publication")
    work = Path(handle["work"])
    commands = client_commands(life, handle, "smoke-commands", 4 * 3600)
    questions = {"airk": "How does a source file become searchable: scanning, chunking, embedding and publication?",
                 "bookbuilder": "How is a manuscript project exported to a document file?"}
    samples = {"airk": ("crates/airk-core/src/source_policy.rs", "validate_root_path"),
               "bookbuilder": (None, None)}
    rows = []

    def call(repo, phase, tool, arguments):
        index_id = handle["indexes"][repo]["index_id"]
        label = f"{tool}-{repo}-{phase}".replace("+", "-")
        args_file = None
        if arguments is None:
            args_file = commands.folder / f"{commands.prefix}-answer-{repo}-{phase.replace('+', '-')}.args.json"
            write_json(args_file, answer_arguments(work, questions[repo]))
        try:
            envelope, out, elapsed = commands.tool(label, tool, index_id, arguments, args_file=args_file)
        except ValueError:
            # The CLI exits non-zero on a tool error; keep the error result as the smoke observation.
            out = sorted(commands.folder.glob(f"{commands.prefix}-*-{label}.stdout"))[-1]
            envelope = read_json(out) if out.stat().st_size else {"isError": True}
            elapsed = read_json(out.with_name(out.name.replace(".stdout", ".command.json")))["elapsed_ns"] \
                if out.with_name(out.name.replace(".stdout", ".command.json")).is_file() else 0
            envelope.setdefault("isError", True)
        row = {"repository": repo, "phase": phase, "tool": tool, "elapsed_ms": round(elapsed / 1e6, 1),
               "bytes": out.stat().st_size, "is_error": envelope.get("isError") is True,
               "result": {"path": str(out), "sha256": sha256(out)}}
        if tool == "airk_answer" and not row["is_error"]:
            body = tool_body(envelope)
            row.update(stop=body.get("stop"), records=len(body.get("records", [])),
                       assessments=len(body.get("assessment_history", [])))
        rows.append(row)
        return envelope

    for repo in selected:
        index = dict(handle["indexes"][repo])
        sample, symbol = samples[repo]
        if sample is None:
            listing = tool_body(commands.tool("sample-files", "airk_list_files", index["index_id"],
                                              {"limit": 100, "response_mode": "full"})[0])
            entries = listing.get("paths") or listing.get("files") or listing.get("entries") or []
            names = [e["path"] if isinstance(e, dict) else e for e in entries]
            sample = next((n for n in names if n.endswith(".cs")), names[0] if names else None)
        index["sample_file"], index["sample_symbol"] = sample, symbol
        if symbol is None and sample:
            outline = commands.tool("sample-outline", "airk_outline", index["index_id"],
                                    {"path": sample, "response_mode": "full"})[0]
            text = json.dumps(outline)
            for key in ('"name": "', '"name":"'):
                at = text.find(key)
                if at >= 0:
                    index["sample_symbol"] = text[at + len(key):text.find('"', at + len(key))].split("\\")[0]
                    break
        for tool, arguments in smoke_calls(index, questions[repo]):
            call(repo, "no-model", tool, arguments)
    stages = []
    if args.with_models:
        # Other clients may share this lifetime: a stage that is already on stays as it is, the
        # enrollment always names every index of the lifetime, and only a stage this smoke turned on
        # is turned off again.
        everyone = list(handle["indexes"])

        def ensure_on(name):
            profile = PROFILES[name]
            state = commands.json("smoke-stage-state", "arbiter", "status-legacy")
            current = state["configuration"][profile["stage"]]
            enrolled = {entry["index_id"] for entry in current.get("indexes", [])}
            wanted = {handle["indexes"][repo]["index_id"] for repo in selected}
            if current["enabled"] and current["selected_profile"] == profile["manifest"] and wanted <= enrolled \
                    and state[profile["stage"]]["availability"] == "ready":
                stages.append({"profile": name, "already_enabled": True, "at": utc()})
                return False
            stages.append(set_stage(life, handle, name, True, everyone))
            return True

        focus_mine = ensure_on("focus-static1024-cpu")
        for repo in selected:
            call(repo, "focus", "airk_answer", None)
        if args.compact:
            compact_mine = ensure_on("context-compact-cpu")
            for repo in selected:
                call(repo, "focus+compact", "airk_answer", None)
            if compact_mine:
                stages.append(set_stage(life, handle, "context-compact-cpu", False, everyone))
        if focus_mine and not args.leave_stages:
            stages.append(set_stage(life, handle, "focus-static1024-cpu", False, everyone))
    result = {"schema": "airk.private.answer-eval-tool-smoke/1", "lifetime": life.name, "at": utc(),
              "image": handle["image"]["label"], "calls": rows, "stages": stages,
              "profiles": {name: {k: v for k, v in row.items()} for name, row in PROFILES.items()}}
    pinned = write_json(Path(args.output), result)
    print(json.dumps({"calls": len(rows), "errors": sum(r["is_error"] for r in rows), **pinned}))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--fixtures", type=Path, default=FIXTURES, help="the private fixture root")
    sub = parser.add_subparsers(dest="command", required=True)
    begin = sub.add_parser("start", help="own one lifetime; blocks until it closes")
    begin.add_argument("--image", type=Path, required=True)
    begin.add_argument("--lifetime", required=True)
    begin.add_argument("--restart-from", help="a closed earlier lifetime whose installation and indexes are reused")
    add_selector_arguments(begin)
    begin.add_argument("--seconds", type=int, default=MAX_SECONDS)
    begin.add_argument("--inputs", type=Path, default=DEFAULT_INPUTS)
    begin.add_argument("--inputs-sha256", default=DEFAULT_INPUTS_SHA256)
    begin.add_argument("--repositories", type=lambda v: [x for x in v.split(",") if x], default=["airk", "bookbuilder"])
    begin.add_argument("--no-dispatch-observer", dest="dispatch_observer", action="store_false")
    begin.add_argument("--accept-unidle-close", metavar="LIFETIME=SHA256",
                       help="a recorded one-time exception from UNIDLE_CLOSE_EXCEPTIONS for --restart-from")
    hidden = sub.add_parser("worker")
    for name in ("--lifetime-path", "--image", "--inputs", "--inputs-sha256"):
        hidden.add_argument(name, required=True)
    hidden.add_argument("--seconds", type=int, required=True)
    hidden.add_argument("--restart-from")
    add_selector_arguments(hidden)
    hidden.add_argument("--repositories", type=lambda v: [x for x in v.split(",") if x], default=[])
    hidden.add_argument("--no-dispatch-observer", dest="dispatch_observer", action="store_false")
    hidden.add_argument("--accept-unidle-close")
    halt = sub.add_parser("stop", help="write the lifetime's stop file")
    halt.add_argument("--lifetime", required=True)
    staged = sub.add_parser("stage", help="enable or disable one stage with a named profile")
    staged.add_argument("--lifetime", required=True)
    staged.add_argument("--profile", choices=sorted(PROFILES), required=True)
    mode = staged.add_mutually_exclusive_group(required=True)
    mode.add_argument("--enable", action="store_true")
    mode.add_argument("--disable", dest="enable", action="store_false")
    staged.add_argument("--repositories", type=lambda v: [x for x in v.split(",") if x])
    probe = sub.add_parser("leak-probe", help="fail if the airk index serves an excluded path")
    probe.add_argument("--lifetime", required=True)
    probe.add_argument("--evaluation", type=Path, default=EVALUATION)
    probe.add_argument("--phrase", action="append", default=[], help="fallback phrase when no scenario exists")
    probe.add_argument("--while-indexing", action="store_true", help="allow a lifetime still building vectors")
    check = sub.add_parser("smoke", help="time one call of each read tool per index")
    check.add_argument("--lifetime", required=True)
    check.add_argument("--output", type=Path, required=True)
    check.add_argument("--with-models", action="store_true")
    check.add_argument("--compact", action="store_true")
    check.add_argument("--while-indexing", action="store_true")
    check.add_argument("--leave-stages", action="store_true", help="keep a stage this smoke enabled")
    check.add_argument("--repositories", type=lambda v: [x for x in v.split(",") if x])
    sampled = sub.add_parser("progress", help="sample semantic progress into progress.json")
    sampled.add_argument("--lifetime", required=True)
    added = sub.add_parser("add-index", help="set up and lexically index one more repository in a running lifetime")
    added.add_argument("--lifetime", required=True)
    added.add_argument("--repository", required=True)
    settled = sub.add_parser("settle-index", help="cancel one repository's unfinished jobs and wait for idle")
    settled.add_argument("--lifetime", required=True)
    settled.add_argument("--repository", required=True)
    waited = sub.add_parser("wait", help="block until a lifetime needs attention (overnight driver)")
    waited.add_argument("--lifetime", required=True)
    waited.add_argument("--hours", type=float, default=6.0)
    waited.add_argument("--interval", type=int, default=300)
    args = parser.parse_args()
    require(os.name == "nt", "the private fixture launcher runs on Windows")
    {"start": start, "worker": worker, "stop": stop, "stage": stage, "leak-probe": leak_probe,
     "smoke": smoke, "progress": progress, "add-index": add_index, "settle-index": settle_index, "wait": wait}[args.command](args)


if __name__ == "__main__":
    main()
