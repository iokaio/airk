# SPDX-License-Identifier: Apache-2.0
"""Collect completed R14 reports only. Never copy private fixture/index trees."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import subprocess
import sys

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("r14_transfer", HERE / "neo-transfer-and-launch-r14.py")
TRANSFER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRANSFER)

REPORTS = {
    "batch-receipt.json", "checkpoint-regression-junit.xml", "nextest-junit.xml",
    "original-pre14-fixture-report.json", "debug-schema1-upgrade.json",
    "debug-pre14-adoption.json", "debug-pre14-corrupt-copy-restoration.json",
    "release-model-clients.json", "release-semantic-client-counts.json", "release-retrieval.json",
    *[f"release-semantic-client-counts-cases/projects-1-clients-{count}.json" for count in (1, 10, 64)],
    "fixture-cleanup-receipts/manifest.json",
    "issuer-rotation-private/issuer-rotation-report.json",
    *[f"nextest-{label}.toml" for label in ("retirement", "shipping", "checkpoint", "workspace")],
    *[f"nextest-{label}-store/ci/junit.xml" for label in ("retirement", "shipping", "checkpoint", "workspace")],
}


def allowed(name, stages):
    path = PurePosixPath(name)
    if path.is_absolute() or any(part in ("", ".", "..") for part in name.split("/")) or "\\" in name:
        return False
    if len(path.parts) == 2 and path.parts[0] == "fixture-cleanup-receipts" and path.stem.isdigit() and path.suffix == ".json":
        return True
    if name in REPORTS:
        return True
    if len(path.parts) == 1 and name in {stage + ".log" for stage in stages}:
        return True
    return len(path.parts) == 2 and path.parts[0] == "cargo-timings" and path.name.startswith("cargo-timing") and path.suffix == ".html"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--active-receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args(argv)
    active = json.loads(args.active_receipt.read_text())
    campaign = PurePosixPath(active["campaign"])
    if campaign.parent != PurePosixPath(TRANSFER.ROOT) or not campaign.name.startswith("one-airk-1814-final-r14-"):
        raise ValueError("Only the explicit R14 campaign may be collected")
    if not args.execute:
        print(json.dumps({"status": "PLAN_ONLY_NO_SSH", "campaign": str(campaign)}))
        return 0
    destination = args.output.absolute()
    if destination.parent.resolve() != HERE or destination.exists() or destination.is_symlink():
        raise ValueError("Choose a new immediate child of the R14 preparation directory")
    ssh = ["ssh", "-i", TRANSFER.KEY, "-oBatchMode=yes", "-oStrictHostKeyChecking=yes", TRANSFER.HOST, TRANSFER.PYTHON, "-"]
    # The remote inventory reads explicit report paths only. It does not walk
    # issuer-rotation-private, whose children contain private state and indexes.
    code = """import hashlib,json,os,re
from pathlib import Path
campaign=Path(CAMPAIGN)
root=campaign/'evidence'
assert campaign.is_dir() and not campaign.is_symlink() and root.is_dir() and not root.is_symlink()
phase=(campaign/'phase.txt').read_text().strip()
assert phase in ('complete-passed','complete-failed'), 'Collect only a fully settled batch'
state=json.loads((root/'batch-receipt.json').read_text())
stages=[row['stage'] for row in state['commands']]
assert all(re.fullmatch('[a-zA-Z0-9_-]+', stage) for stage in stages)
names=set(REPORTS)|{stage+'.log' for stage in stages}
for row in state.get('fixture_cleanup_reports',[]):
 name=row['path']
 assert re.fullmatch('fixture-cleanup-receipts/[0-9]+\\.json',name)
 names.add(name)
timings=root/'cargo-timings'
if timings.is_dir() and not timings.is_symlink():names.update('cargo-timings/'+p.name for p in timings.iterdir() if p.name.startswith('cargo-timing') and p.suffix=='.html')
files=[]
for name in sorted(names):
 path=root/name
 if not path.exists():continue
 assert path.is_file() and not any(p.is_symlink() for p in (path,*path.parents)), 'Redirected report refused'
 assert path.stat().st_size<=128*1024*1024, 'Report exceeds collection budget'
 with path.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
 files.append({'path':name,'bytes':path.stat().st_size,'sha256':digest})
assert sum(row['bytes'] for row in files)<=512*1024*1024
print(json.dumps({'schema':1,'campaign':str(campaign),'phase':phase,'files':files,'private_fixtures_copied':False,'cargo_cleanup':state.get('cargo_cleanup'),'owned_descendants_settled':state.get('all_owned_command_descendants_settled'),'evidence_preserved_before_clean':state.get('evidence_preserved_before_clean')}))
""".replace("CAMPAIGN", repr(str(campaign))).replace("REPORTS", repr(sorted(REPORTS)))
    result = subprocess.run(ssh, input=code, capture_output=True, text=True, check=True, timeout=120)
    inventory = json.loads(result.stdout)
    # Get the batch receipt first to authenticate the dynamic stage-log names.
    receipt_row = next(row for row in inventory["files"] if row["path"] == "batch-receipt.json")
    destination.mkdir(mode=0o700)
    def copy(row):
        name = row["path"]
        local = destination / name
        local.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        remote = f"{TRANSFER.HOST}:{campaign}/evidence/{name}"
        subprocess.run(["scp", "-i", TRANSFER.KEY, "-oBatchMode=yes", "-oStrictHostKeyChecking=yes", remote, str(local)], check=True, timeout=120)
        if local.is_symlink() or local.stat().st_size != row["bytes"] or TRANSFER.sha(local) != row["sha256"]:
            raise ValueError("Collected report changed: " + name)
    copy(receipt_row)
    state = json.loads((destination / "batch-receipt.json").read_text())
    stages = [row["stage"] for row in state["commands"]]
    for row in inventory["files"]:
        if not allowed(row["path"], stages):
            raise ValueError("Remote collection escaped report allowlist")
        if row is not receipt_row:
            copy(row)
    (destination / "collection-receipt.json").write_text(json.dumps(inventory, indent=2) + "\n")
    print(json.dumps({"collected": str(destination), "files": len(inventory["files"]), "cargo_cleanup": inventory["cargo_cleanup"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
