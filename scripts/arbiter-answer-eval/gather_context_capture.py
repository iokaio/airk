# SPDX-License-Identifier: Apache-2.0
"""Capture first native Gather Context deliveries on a verified no-model fixture."""
from __future__ import annotations

from pathlib import Path
import time

import common
from gather_context_native import capture_case, capture_failure, verify_observation
from gather_context_replay import (CASES, campaign_pin, load_pinned, read_ceiling,
                                   read_spec, require, source_root, write_new)
import loop_reply
import replay
import settled


def _scope(handle: common.Handle, repositories: dict) -> list[dict]:
    stages = handle.live_stages()
    observations = []
    for repo, expected in repositories.items():
        observed = settled.snapshot(handle, repo)
        verify_observation({**expected, "repo": repo}, observed, stages)
        observations.append({"repo": repo, "stages": stages, "observation": observed})
    return observations


def _native_call(handle: common.Handle, repo: str, instruction: str, defaults: dict, call_timeout: float):
    tool, arguments = replay.canonical_first_call("answer", instruction, defaults)
    arguments["response_mode"] = "full"
    started = time.monotonic()
    deadline = started + min(call_timeout, arguments["budget"]["deadline_ms"] / 1000)

    def call(operation, inputs):
        left = deadline - time.monotonic()
        if left <= 0:
            return common.CallOutcome(False, None, b"", b"", 0, None, "deadline")
        return common.call_tool(handle, operation, repo, inputs, left, strip_env=common.AGENT_STRIPPED_ENV)

    outcome, retries = replay._retrying(call, tool, arguments, time.sleep, handle.index_id(repo))
    outcome, tickets, more = replay.attach_tickets(
        call, tool, arguments, outcome, index_id=handle.index_id(repo))
    return outcome, {"tool": tool, "arguments": arguments, "typed_retries": retries + more,
                     "tickets": tickets, "elapsed_ms": int((time.monotonic() - started) * 1000)}


def capture(*, campaign: Path, spec_path: Path, output: Path, arm: str, handle_path: Path,
            handle_sha: str, native_sha: str) -> dict:
    """Run an admitted native arm and retain exact result bytes and normalized witnesses."""
    campaign = campaign.resolve(strict=True)
    require(output == output.resolve(strict=True) and output.is_relative_to(campaign)
            and output.is_dir() and not any(output.iterdir()), "capture directory must be fresh and private")
    require(arm in ("control", "candidate"), "invalid capture arm")
    spec, spec_sha = read_spec(spec_path)
    freeze = load_pinned(campaign, spec["scenario_freeze"], "scenario freeze")
    repositories, _, _ = read_ceiling(campaign, spec, freeze)
    history = load_pinned(campaign, spec["ceiling_disposition"], "ceiling disposition")
    historical = load_pinned(campaign, campaign_pin(campaign, **history["spec"]), "archived spec")
    handle_path = handle_path.resolve(strict=True)
    raw_handle = load_pinned(campaign, campaign_pin(campaign, str(handle_path), handle_sha), "runtime handle")
    handle = common.load_handle(handle_path)
    physical_indexes = {value["index_id"] for value in raw_handle["indexes"].values()}
    require(handle.sha256 == handle_sha
            and physical_indexes == {row["index_id"] for row in repositories.values()}
            and all(handle.index_id(repo) == row["index_id"] for repo, row in repositories.items()),
            "runtime handle or logical scope crossed its admission")
    service = raw_handle["image"]["service"]
    cli = raw_handle["image"]["cli"]
    native = Path(service["path"])
    cli_path = Path(cli["path"])
    require(native.is_absolute() and native == native.resolve(strict=True)
            and cli_path.is_absolute() and cli_path == cli_path.resolve(strict=True)
            and Path(handle.cli[0]) == cli_path and len(handle.cli) == 1
            and service["sha256"] == native_sha == common.sha256_file(native)
            and cli["sha256"] == common.sha256_file(cli_path),
            "native image or paired runtime client differs")
    ident = {"schema": "airk.private.gather-context-capture/1", "arm": arm, "spec_sha256": spec_sha,
             "service_sha256": native_sha, "cli_sha256": cli["sha256"], "handle_sha256": handle_sha,
             "image": raw_handle["image"], "start": _scope(handle, repositories)}
    roots = {repo: source_root(campaign, repo, pin) for repo, pin in repositories.items()}
    defaults = historical["answer_defaults"]
    timeout = historical["results"][0]["call_timeout"]
    cases = []
    for repo, slot in CASES:
        pin = repositories[repo]
        relative = f"scenarios/{repo}/{slot}.json"
        scenario = load_pinned(campaign, {"path": relative, "sha256": freeze["files"][relative]}, "development case")
        case_id = f"{repo}-{slot}"
        require((scenario["id"], scenario["set"], scenario["repository"], scenario["source_commit"])
                == (case_id, "development", repo, pin["commit"]), "capture case identity differs")
        case = {"id": case_id, "repo": repo, "source_commit": pin["commit"],
                "publication": pin["publication"], "index_id": pin["index_id"]}
        outcome, dispatch = _native_call(handle, repo, scenario["instruction"], defaults, timeout)
        raw = output / f"{case_id}.reply.json"
        write_new(raw, outcome.stdout)
        text = common.envelope_text(outcome.envelope)
        body = common.envelope_body(outcome.envelope)
        elapsed = dispatch["elapsed_ms"]
        if outcome.ok and not loop_reply.is_ticket(body):
            row = capture_case(case, body, text, elapsed_ms=elapsed, source_root=roots[repo])
        else:
            reason = ("timeout" if (outcome.error == "deadline" or (outcome.error or "").startswith("tool call exceeded ")
                                    or loop_reply.is_ticket(body))
                      else "refused")
            detail = {"exit_code": outcome.exit_code, "error": outcome.error, "envelope": body}
            row = capture_failure(case, reason, detail, elapsed_ms=elapsed, delivered_text=text)
        row.update(transport_request=dispatch, raw_reply_sha256=common.sha256_bytes(outcome.stdout))
        write_new(output / f"{case_id}.case.json", common.canonical_json(row) + b"\n")
        cases.append(row)
        print(f"{arm} {case_id} {row['disposition']} · {elapsed} ms", flush=True)
    ident.update(end=_scope(handle, repositories), cases=cases)
    write_new(output / "capture.json", common.canonical_json(ident) + b"\n")
    return ident
