# SPDX-License-Identifier: Apache-2.0
"""Apply the primary-reviewed exact proposals without Cargo or regeneration."""
import hashlib
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parents[3]
proposal = Path(__file__).resolve().parent / "dependency-source-review"
review_bytes = (proposal / "review.json").read_bytes()
review = json.loads(review_bytes)
policy_path = root / "supply-chain/source-policy.json"
config_path = root / "supply-chain/config.toml"
sha = lambda data: hashlib.sha256(data).hexdigest()
before = {name: sha((root / name).read_bytes()) for name in (
    "Cargo.lock", "supply-chain/source-policy.json", "supply-chain/config.toml",
    "supply-chain/imports.lock", "supply-chain/audits.toml")}
assert before["Cargo.lock"] == review["lock_sha256"]
assert before["supply-chain/source-policy.json"] == review["source_policy_before_sha256"]
policy = json.loads(policy_path.read_bytes())
original_rows = list(policy["packages"])
old_config = config_path.read_bytes()
old_toml = tomllib.loads(old_config.decode())
proposed = tomllib.loads((proposal / "exemptions-proposal.toml").read_text(encoding="utf-8"))
assert sum(len(v) for v in proposed["exemptions"].values()) == 54
for row in review["packages"]:
    identity = row["identity"]
    exception = proposed["exemptions"][identity["name"]][0]
    assert exception["version"] == identity["version"]
    assert row["exception_rationale_and_limits"] in exception["notes"]
    assert identity not in policy["packages"]
    assert all(e["version"] != identity["version"] for e in old_toml.get("exemptions",{}).get(identity["name"],[]))
    policy["packages"].append(identity)
assert policy["packages"][:len(original_rows)] == original_rows
policy_path.write_text(json.dumps(policy,indent=2)+"\n",encoding="utf-8",newline="\n")
addition = (proposal / "exemptions-proposal.toml").read_text(encoding="utf-8").replace(
    "PROPOSED exact-version exception", "Reviewed limited exact-version exception")
assert "PROPOSED" not in addition
config_path.write_bytes(old_config.rstrip(b"\r\n")+b"\n\n"+addition.encode())
new_toml = tomllib.loads(config_path.read_text(encoding="utf-8"))
assert sum(map(len,new_toml["exemptions"].values()))-sum(map(len,old_toml["exemptions"].values())) == 54
for key,value in old_toml.items():
    if key != "exemptions":
        assert new_toml[key] == value
for key,value in old_toml["exemptions"].items():
    assert new_toml["exemptions"][key][:len(value)] == value
public = root / "supply-chain/reviews"
public.mkdir(exist_ok=True)
review["status"] = "REVIEWED_LIMITED_EXACT_VERSION_EXCEPTIONS"
review["proposal_evidence_sha256"] = sha(review_bytes)
review["decision_scope"] = "Primary independently reviewed and accepted exact-version exceptions with documented limitations; not a full safe-to-deploy audit, human signature, advisory clearance or publisher trust."
destination = public / "native-https-2026-11-07.json"
destination.write_text(json.dumps(review,indent=2)+"\n",encoding="utf-8",newline="\n")
for name in ("Cargo.lock","supply-chain/imports.lock","supply-chain/audits.toml"):
    assert sha((root/name).read_bytes()) == before[name], name
result={"before":before,"after":{name:sha((root/name).read_bytes()) for name in before},"source_rows_added":54,"exact_exemptions_added":54,"public_review_sha256":sha(destination.read_bytes())}
(proposal/"applied-receipt.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
print(json.dumps(result,indent=2))
