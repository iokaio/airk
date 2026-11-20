# SPDX-License-Identifier: Apache-2.0
"""Verify aws confirmation-source ranges through a private native service."""
from pathlib import Path
import runpy

reader = runpy.run_path(str(Path(__file__).with_name("native_cohort_source_reads.py")))
COHORT = reader["Cohort"](
    name="aws", corpus_directory="aws-unique-source-corpus-r1",
    manifest_sha256="ef2844ba2406de3b57ac1f5a6b13e582320bacae27779b704d38d4786375e882",
    case_count=41, document_count=27, selector_field="row_index",
    receipt_identity={"source_commit": "2d06ee9011f3192b2ff38c09f04e01f1ea9e0191"},
)


if __name__ == "__main__":
    reader["run_cohort"](COHORT, Path(__file__).resolve())
