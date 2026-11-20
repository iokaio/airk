# SPDX-License-Identifier: Apache-2.0
"""Verify techqa confirmation-source ranges through a private native service."""
from pathlib import Path
import runpy

reader = runpy.run_path(str(Path(__file__).with_name("native_cohort_source_reads.py")))
COHORT = reader["Cohort"](
    name="techqa", corpus_directory="techqa-exact-gold-source-corpus-r1",
    manifest_sha256="a670e02fe6abdaab184a6950c33d74caf4ef4b972530a1aff300d68dd51b44c8",
    case_count=76, document_count=71, selector_field="question_id",
    receipt_identity={"ordered_menu_freeze_sha256": "50f876ebb9cf3a8328314e7be3e01f343b2d1b67cd7979fe2aaceeb2c82064f7"},
)


if __name__ == "__main__":
    reader["run_cohort"](COHORT, Path(__file__).resolve())
