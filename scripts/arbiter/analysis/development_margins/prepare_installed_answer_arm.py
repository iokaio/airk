# SPDX-License-Identifier: Apache-2.0
"""Build an installed timing arm from real native descriptors and original normal service registration outputs."""
import argparse
import json
from pathlib import Path
import runpy


HERE = Path(__file__).resolve().parent
timing = runpy.run_path(str(HERE / "installed_answer_timing.py"))
require = timing["require"]


def prepare(profile_path, profile_sha256, calibration_path, control, registrations, install_sha256):
    profile = timing["pin"](profile_path, profile_sha256)
    model = {"arbiter-context-qwen3-17b":"context_compact", "arbiter-context-qwen3-4b":"context_large"}[profile["model"]]
    provider = {"onnxruntime-cpu/1.30.0":"cpu","onnxruntime-cuda/1.30.0":"cuda"}[profile["runtime"]]
    roster = timing["pin"](timing["ROSTER"], timing["ROSTER_SHA"])
    require(roster == timing["freeze"]["freeze"](), "original workload differs")
    cal = timing["pin"](calibration_path, profile["calibration"])
    require(profile["qualification"] and cal["rules"], "qualified native profile with active joint calibration is missing")
    reg_refs, indices = {}, {}
    for case in roster["cases"]:
        name = case["case"]
        path = registrations / (name+"-register.stdout")
        raw, seal = timing["freeze"]["sealed"](path)
        receipt = timing["registration_receipt"](raw)
        require(Path(receipt["request"]["registration"]["root"]).samefile(case["source_root"]), "wrong native source registration")
        reg_refs[name] = {"path":str(path), "sha256":seal}
        indices[name] = receipt["index_id"]
    arm = {"provider":provider, "runtime":profile["runtime"], "model":model,"context_manifest":profile["manifest"],
           "control":str(control), "install_record_sha256":install_sha256, "profile_path":str(profile_path),
           "profile_sha256":profile_sha256, "calibration_path":str(calibration_path), "calibration_sha256":profile["calibration"],
           "registrations":reg_refs, "indexes":indices}
    timing["bind_arm"](arm,roster)
    timing["installed"](arm)
    return arm


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", required=True, type=Path)
    parser.add_argument("--profile-sha256",required=True)
    parser.add_argument("--calibration", required=True, type=Path)
    parser.add_argument("--control", required=True, type=Path)
    parser.add_argument("--registrations", required=True, type=Path)
    parser.add_argument("--install-sha256", required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.profile,args.profile_sha256,args.calibration,args.control,args.registrations,args.install_sha256),
                     sort_keys=True,allow_nan=False))
