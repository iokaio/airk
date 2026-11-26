# SPDX-License-Identifier: Apache-2.0
"""Freeze evaluator-only references and compare Gather Context development captures."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys

import common
from prepare_complete_evidence import PreparationError, pinned_json, reopen_exact, source_span, token_parts

SPEC_SCHEMA = "airk.private.gather-context-replay-spec/1"
REFERENCE_SCHEMA = "airk.private.gather-context-reference/1"
AUTHORS = ("openai", "anthropic")
PHYSICAL = {"bookbuilder": "bookbuilder-v2", "munarium": "munarium"}
CASES = tuple((repo, slot) for repo in common.REPOS for slot in common.SLOTS)
SHA256 = re.compile(r"[0-9a-f]{64}")
COMMIT = re.compile(r"[0-9a-f]{40}")


def require(condition: bool, reason: str) -> None:
    common.require(condition, reason)


def checked_pin(root: Path, pin: dict, label: str) -> tuple[Path, str]:
    require(set(pin) == {"path", "sha256"} and isinstance(pin["path"], str)
            and isinstance(pin["sha256"], str) and SHA256.fullmatch(pin["sha256"]) is not None,
            f"invalid {label} pin")
    return reopen_exact(root, pin["path"]), pin["sha256"]


def load_pinned(root: Path, pin: dict, label: str) -> dict:
    path, digest = checked_pin(root, pin, label)
    return pinned_json(path, digest, label)


def read_spec(path: Path) -> tuple[dict, str]:
    raw = path.read_bytes()
    spec = json.loads(raw)
    require(spec.get("schema") == SPEC_SCHEMA, "unknown Gather Context replay spec")
    return spec, common.sha256_bytes(raw)


def campaign_pin(root: Path, path: str, sha256: str) -> dict:
    absolute = Path(path)
    require(absolute.is_absolute() and absolute.is_relative_to(root), "archived path escaped campaign")
    return {"path": absolute.relative_to(root).as_posix(), "sha256": sha256}


def historical_inputs(root: Path, history: dict) -> tuple[dict, list]:
    spec = load_pinned(root, campaign_pin(root, **history["spec"]), "archived spec")
    prompt_pin = campaign_pin(root, spec["prompts_freeze"], spec["codex_config"]["freeze_sha256"])
    prompts = load_pinned(root, prompt_pin, "archived prompts")
    path, digest = checked_pin(root, campaign_pin(root, **history["settled"]), "settlements")
    raw = path.read_bytes()
    require(common.sha256_bytes(raw) == digest, "archived settlement digest changed")
    return prompts, [json.loads(line) for line in raw.splitlines() if line.strip()]


def prepare_spec(root: Path, repo_root: Path, disposition: dict) -> dict:
    history = load_pinned(root, disposition, "ceiling disposition")
    prompts, settlements = historical_inputs(root, history)
    expected_cases = {f"{repo}-{slot}" for repo, slot in CASES}
    require(set(prompts["complete_evidence"]) == expected_cases, "archived prompts have wrong case roster")
    repositories = {}
    for repo in common.REPOS:
        starts = [row for row in settlements if (row.get("repo"), row.get("phase"), row.get("settled"))
                  == (repo, "start", True)]
        require(len(starts) == 1, f"expected one settled start for {repo}")
        published = starts[0]
        input_pin = prompts["complete_evidence"][f"{repo}-{common.SLOTS[0]}"]
        archived = load_pinned(root, campaign_pin(root, input_pin["manifest"], input_pin["manifest_sha256"]),
                               "archived essential inputs")
        receipt = campaign_pin(root, input_pin["source_root"] + ".json", archived["source_export_receipt_sha256"])
        repositories[repo] = {"commit": archived["source_commit"], "publication": published["publication_token"],
                              "index_id": published["index_id"], "binding": published["binding"],
                              "export_receipt": receipt}
    documents = {author: f"docs/arbiter/retrieval-strategy-{author}.md" for author in AUTHORS}
    return {"schema": SPEC_SCHEMA, "ceiling_disposition": disposition,
            "scenario_freeze": campaign_pin(root, **history["scenario_freeze"]),
            "repositories": repositories,
            "author_documents": {author: {"path": path, "sha256": common.sha256_file(repo_root / path)}
                                 for author, path in documents.items()}}


def read_ceiling(root: Path, spec: dict, freeze: dict) -> tuple[dict, dict, dict]:
    history = load_pinned(root, spec["ceiling_disposition"], "ceiling disposition")
    require(history.get("schema") == "airk.private.answer-eval-ceiling-disposition/1"
            and history.get("status") == "below-threshold"
            and history["scenario_freeze"]["sha256"] == spec["scenario_freeze"]["sha256"],
            "ceiling disposition does not bind the development freeze")
    prompts, settlements = historical_inputs(root, history)
    expected = {}
    for repo in common.REPOS:
        pin = spec["repositories"][repo]
        token_parts(pin["publication"])
        require(COMMIT.fullmatch(pin["commit"]) is not None
                and pin["commit"] == freeze[repo + "_commit"], f"source commit differs for {repo}")
        for phase in ("start", "end"):
            require(any(row.get("settled") is True
                        and (row.get("repo"), row.get("phase"), row.get("publication_token"),
                             row.get("index_id"), row.get("binding"))
                        == (repo, phase, pin["publication"], pin["index_id"], pin["binding"])
                        for row in settlements), f"{repo} {phase} settlement missing or stale")
        expected[repo] = pin
    records = {case["scenario_id"]: case for case in history["cases"]}
    require(len(history["cases"]) == len(records) == len(CASES)
            and set(records) == {f"{repo}-{slot}" for repo, slot in CASES},
            "ceiling disposition has an incorrect case roster")
    return expected, records, prompts["complete_evidence"]


def load_author_traces(repo_root: Path, documents: dict) -> dict:
    from gather_context_traces import parse_traces

    require(set(documents) == set(AUTHORS), "two author documents are required")
    traces = {}
    for author in AUTHORS:
        path, digest = checked_pin(repo_root, documents[author], f"{author} document")
        raw = path.read_bytes()
        require(common.sha256_bytes(raw) == digest, f"{author} document digest changed")
        traces[author] = parse_traces(author, raw.decode("utf-8"))
        require(set(traces[author]) == set(CASES), f"{author} trace case roster differs")
    return traces


def source_root(campaign: Path, repo: str, pin: dict) -> Path:
    receipt = load_pinned(campaign, pin["export_receipt"], f"{repo} export receipt")
    require(receipt["repository"] == PHYSICAL[repo] and receipt["commit"] == pin["commit"]
            and SHA256.fullmatch(receipt["archive"]["sha256"]) is not None,
            f"export receipt authority differs for {repo}")
    path = Path(receipt["root"])
    require(path.is_absolute() and path.is_relative_to(campaign)
            and path.resolve(strict=True) == path and path.is_dir(), f"unsafe export root for {repo}")
    return path


def case_obligations(scenario: dict, root: Path, archived: dict) -> list[dict]:
    witnesses = {}
    for span in archived["spans"]:
        for note_id in span["essential_ids"]:
            require(note_id not in witnesses, "duplicate archived essential")
            witnesses[note_id] = span
    obligations = []
    for note in scenario["notes"]:
        if note["mark"] != "essential":
            continue
        span = witnesses.pop(note["id"], None)
        require(span is not None and span["path"] == note["path"]
                and [span["start_line"], span["end_line"]] == note["lines"],
                "essential witness differs from the independent freeze")
        checked, _ = source_span(root, note["path"], *note["lines"], span["source_sha256"])
        require(checked["excerpt_sha256"] == span["excerpt_sha256"], "source excerpt digest differs")
        obligations.append({"id": note["id"], "fact": note["fact"], "witness": checked,
                            "alternate_locations_for_review": note.get("alternatives") or []})
    require(len(obligations) >= 3 and not witnesses and
            len({obligation["id"] for obligation in obligations}) == len(obligations),
            "archived essential roster differs")
    return obligations


def freeze_reference(campaign: Path, repo_root: Path, spec: dict, spec_sha: str) -> dict:
    campaign = campaign.resolve(strict=True)
    freeze = load_pinned(campaign, spec["scenario_freeze"], "scenario freeze")
    require(freeze.get("schema") == "airk.private.answer-eval-scenario-freeze/1", "wrong scenario freeze")
    require(set(spec["repositories"]) == set(common.REPOS), "the two development repositories are required")
    repositories, history, archives = read_ceiling(campaign, spec, freeze)
    require(set(archives) == set(history), "archived source inputs have an incorrect case roster")
    traces = load_author_traces(repo_root, spec["author_documents"])
    roots = {repo: source_root(campaign, repo, pin) for repo, pin in repositories.items()}
    rows = []
    for repo, slot in CASES:
        case_id = f"{repo}-{slot}"
        relative = f"scenarios/{repo}/{slot}.json"
        scenario_sha = freeze["files"][relative]
        scenario = load_pinned(campaign, {"path": relative, "sha256": scenario_sha}, "development scenario")
        pin = repositories[repo]
        require((scenario["set"], scenario["repository"], scenario["slot"], scenario["id"],
                 scenario["source_commit"]) == ("development", repo, slot, case_id, pin["commit"]),
                f"development scenario identity differs for {case_id}")
        author_paths = {author: traces[author][repo, slot] for author in AUTHORS}
        for author, trace in author_paths.items():
            require(trace["instruction"].split() == scenario["instruction"].split(),
                    f"{author} instruction differs for {case_id}")
        archive_pin = archives[case_id]
        require(Path(archive_pin["source_root"]) == roots[repo]
                and archive_pin["publication_token"] == pin["publication"],
                f"archived source root or publication differs for {case_id}")
        archive = campaign_pin(campaign, archive_pin["manifest"], archive_pin["manifest_sha256"])
        archived = load_pinned(campaign, archive, "archived essential inputs")
        require((archived["repo"], archived["slot"], archived["scenario_sha256"],
                 archived["source_commit"], archived["publication_token"],
                 archived["source_export_receipt_sha256"])
                == (repo, slot, scenario_sha, pin["commit"], pin["publication"],
                    pin["export_receipt"]["sha256"]),
                f"archived source input authority differs for {case_id}")
        obligations = case_obligations(scenario, roots[repo], archived)
        require(set(history[case_id]["first_dispatch"]["essential_ids"]) ==
                {obligation["id"] for obligation in obligations}, f"first-delivery roster differs for {case_id}")
        rows.append({"id": case_id, "repo": repo, "slot": slot, "source_commit": pin["commit"],
                     "publication": pin["publication"], "index_id": pin["index_id"],
                     "instruction_sha256": common.sha256_bytes(scenario["instruction"].encode("utf-8")),
                     "scenario_sha256": scenario_sha, "author_paths": author_paths,
                     "obligations": obligations})
    return {"schema": REFERENCE_SCHEMA, "spec_sha256": spec_sha, "cases": rows}


def verify_presentations(campaign: Path, spec: dict, reference: dict) -> dict:
    from complete_evidence import CompleteEvidence
    from gather_context_presentation import render
    from run_session import verify_complete_evidence

    freeze = load_pinned(campaign, spec["scenario_freeze"], "scenario freeze")
    repositories, history, archives = read_ceiling(campaign, spec, freeze)
    rows = []
    for case in reference["cases"]:
        case_id, repo = case["id"], case["repo"]
        recorded = history[case_id]
        require(recorded["repo"] == repo and recorded["status"] == "eligible",
                f"archived actor receipt is ineligible for {case_id}")
        archive = archives[case_id]
        path, digest = checked_pin(campaign, campaign_pin(campaign, archive["manifest"],
                                   archive["manifest_sha256"]), "archived essential inputs")
        treatment = CompleteEvidence.from_manifest(
            path.read_bytes(), expected_sha256=digest,
            source_root=source_root(campaign, repo, repositories[repo]),
            repo=repo, slot=case["slot"], scenario_sha256=case["scenario_sha256"],
            publication_token=case["publication"],
            essential_ids=[obligation["id"] for obligation in case["obligations"]],
        )
        directory = Path(recorded["session"])
        session_pin = campaign_pin(campaign, str(directory / "session.json"), recorded["receipt_hashes"]["session"])
        session = load_pinned(campaign, session_pin, "archived actor session")
        observed = verify_complete_evidence(directory, session, treatment)
        require(observed == recorded["first_dispatch"], f"archived dispatch differs for {case_id}")
        result_dir = directory / "wrapper" / "results"
        native = common.read_json(result_dir / "turn-01.native.stdout.json")
        text = common.envelope_text(common.read_json(result_dir / "turn-01.stdout.json"))
        pair = render(native, text.encode("utf-8"), treatment=treatment,
                      recorded_output_sha256=observed["output_sha256"])
        rows.append({"id": case_id, "bytes": len(pair.original), "original": pair.original_output_sha256,
                     "grouped": pair.grouped_output_sha256, "changed": pair.original != pair.grouped})
    return {"schema": "airk.private.gather-context-presentation-audit/1",
            "spec_sha256": reference["spec_sha256"], "cases": rows}


def write_new(path: Path, data: bytes) -> None:
    with path.open("xb") as output:
        output.write(data)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare-spec")
    prepare.add_argument("--campaign-root", required=True, type=Path)
    prepare.add_argument("--disposition-sha256", required=True)
    prepare.add_argument("--output", type=Path)
    prepare.add_argument("--verify-presentations", action="store_true")
    prepare.add_argument("--verify-exports-with-git", type=Path)
    freeze = commands.add_parser("freeze-reference")
    freeze.add_argument("--campaign-root", required=True, type=Path)
    freeze.add_argument("--spec", required=True, type=Path)
    output = freeze.add_mutually_exclusive_group(required=True)
    output.add_argument("--output", type=Path)
    output.add_argument("--preflight", action="store_true")
    compare = commands.add_parser("compare")
    for option in ("reference", "control", "candidate", "output"):
        compare.add_argument(f"--{option}", required=True, type=Path)
    report = commands.add_parser("report")
    report.add_argument("--comparison", required=True, type=Path)
    report.add_argument("--output", required=True, type=Path)
    judgment = report.add_mutually_exclusive_group(required=True)
    judgment.add_argument("--judged", type=Path)
    judgment.add_argument("--unjudged", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.command == "compare":
            from gather_context_compare import compare

            result = compare(*(common.read_json(path) for path in (args.reference, args.control, args.candidate)))
            write_new(args.output, common.canonical_json(result) + b"\n")
            return 0
        if args.command == "report":
            from gather_context_compare import report

            judged = common.read_json(args.judged) if args.judged else None
            result = report(common.read_json(args.comparison), judged=judged)
            write_new(args.output, result.encode("utf-8"))
            return 0
        root = Path(__file__).resolve().parents[2]
        if args.command == "prepare-spec":
            disposition = {"path": "passes/07-windows-complete-evidence-r1/ceiling-disposition.json",
                           "sha256": args.disposition_sha256}
            campaign = args.campaign_root.resolve(strict=True)
            spec = prepare_spec(campaign, root, disposition)
            data = common.canonical_json(spec) + b"\n"
            reference = freeze_reference(campaign, root, spec, common.sha256_bytes(data))
            if args.verify_exports_with_git:
                from gather_context_export import verify_export

                for repo, pin in spec["repositories"].items():
                    receipt = load_pinned(campaign, pin["export_receipt"], f"{repo} export receipt")
                    export = source_root(campaign, repo, pin)
                    result = verify_export(export, receipt, git=args.verify_exports_with_git)
                    print(f"Verified {result['files']} export files for {repo}")
            if args.verify_presentations:
                audit = verify_presentations(campaign, spec, reference)
                changed = sum(row["changed"] for row in audit["cases"])
                print(f"Verified {len(audit['cases'])} first deliveries and {changed} changed presentations")
            if args.output:
                write_new(args.output, data)
            print(f"Verified spec for {len(reference['cases'])} development cases")
            return 0
        spec, digest = read_spec(args.spec)
        reference = freeze_reference(args.campaign_root, root, spec, digest)
        if args.output:
            write_new(args.output, common.canonical_json(reference) + b"\n")
        print(f"Checked {len(reference['cases'])} development cases against the two author traces")
    except (OSError, KeyError, TypeError, ValueError, common.HarnessError, PreparationError) as error:
        print(f"Gather Context replay refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
