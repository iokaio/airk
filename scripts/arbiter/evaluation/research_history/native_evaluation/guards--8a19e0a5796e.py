# SPDX-License-Identifier: Apache-2.0
"""Pure private TRAIN/resource guards; no Torch import or native DTO construction."""
from pathlib import Path
import math

def require(value, message):
    if not value: raise ValueError(message)

def validate_rows(rows, expected_ids):
    require(len(rows)==44 and len(expected_ids)==44 and len(set(expected_ids))==44, 'fixed44 TRAIN population required')
    require([item['native_row']['id'] for item in rows]==expected_ids, 'original TRAIN order differs')
    for item in rows:
        row, example = item['native_row'], item['example']
        require(row['split']=='train' and row['status']=='raw_scores_only', 'non-TRAIN or incomplete native row')
        require(row['id']==example['id'] and item['original_native_row_index']==example['native_row_index'], 'native row association differs')
        compiled=row['compiled'];cards=row['roster']['cards'];n=len(compiled['input_ids'])
        require(compiled['input_digest']==example['native_input_digest'], 'actual native compiled input differs')
        require(0<n<=2048 and len(compiled['attention_mask'])==len(compiled['position_ids'])==n, 'native tensor shape invalid')
        require(all(type(v) is int and v>=0 for v in compiled['input_ids']+compiled['position_ids'])
                and all(type(v) is int and v in (0,1) for v in compiled['attention_mask']), 'native tensor values invalid')
        require(type(compiled['decision_position']) is int and 0<=compiled['decision_position']<n, 'native decision index invalid')
        require(1<=len(cards)<=8 and len(compiled['candidate_positions'])==len(compiled['option_mask'])==len(cards)
                and all(type(v) is int and v>=0 for v in compiled['candidate_positions'])
                and all(v is True for v in compiled['option_mask']), 'native whole-card option mapping differs')
        require(example['negative_option_indices']==[] and example['semantic_labels'] is None, 'unknown cards cannot become semantic negatives')
        positives=example['positive_option_indices']
        require(positives and len(positives)==len(set(positives))
                and all(type(v) is int and 0<=v<len(cards) for v in positives), 'positive card index invalid')
        for ordinal in positives:
            card=cards[ordinal]
            require(card['action']['kind']=='read' and card['gap']=='unsupported'
                    and card['reason']=='additional_evidence_may_help'
                    and all(a['role']=='unknown' and a['relevance']=='uncertain' for a in card['assessments']),
                    'whole-card reference witness cannot certify semantic or terminal hypotheses')
        require(len(row['scores'])==len(cards) and all(math.isfinite(v) for v in row['scores']), 'native raw score shape invalid')

def admit_gpu(free_bytes,total_bytes,cap_bytes,headroom_bytes):
    require(0<cap_bytes<total_bytes and free_bytes>=cap_bytes+headroom_bytes, 'declared CUDA allocator+context headroom unavailable')

def isolated_output(output, owner, frozen):
    output=Path(output).resolve();owner=Path(owner).resolve()
    require(output.parent==owner and not output.exists(), 'fresh exact private output sibling required')
    for path in frozen:
        path=Path(path).resolve()
        require(not output.is_relative_to(path) and not path.is_relative_to(output), 'output overlaps frozen inputs')
    return output
