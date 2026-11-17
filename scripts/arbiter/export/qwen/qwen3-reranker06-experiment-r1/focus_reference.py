# SPDX-License-Identifier: Apache-2.0
"""Private Focus reference: official pair compiler and relevance-only two-logit ranking.

No model/runtime is imported by this pure module. NN execution belongs to the
separately reviewed root-owned export/qualification command.
"""
from dataclasses import dataclass
import math

MODEL = 'Qwen/Qwen3-Reranker-0.6B'
REVISION = 'e61197ed45024b0ed8a2d74b80b4d909f1255473'
FALSE_TOKEN = 2152
TRUE_TOKEN = 9693
INSTRUCTION = 'Given a web search query, retrieve relevant passages that answer the query'
PREFIX = '<|im_start|>system\nJudge whether the Document meets the requirements based on the Query and the Instruct provided. Note that the answer can only be "yes" or "no".<|im_end|>\n<|im_start|>user\n'
SUFFIX = '<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n'

@dataclass(frozen=True)
class Pair:
    input_ids: tuple[int, ...]
    attention_mask: tuple[int, ...]
    position_ids: tuple[int, ...]
    observed_tokens: int
    padded_tokens: int
    query_bytes: int
    document_bytes: int

def compile_pair(tokenizer, query, document, maximum):
    if not isinstance(query, str) or not query.strip() or not isinstance(document, str) or not document.strip():
        raise ValueError('Focus requires the original nonempty query and returned document text')
    if type(maximum) is not int or not 1 <= maximum <= 2048:
        raise ValueError('Focus pair exceeds the reviewed 2048-token ceiling')
    for text, expected in [('no', FALSE_TOKEN), ('yes', TRUE_TOKEN)]:
        if tokenizer.encode(text, add_special_tokens=False) != [expected]:
            raise ValueError('Official single-token readout identity differs')
    if tokenizer.pad_token_id is None:
        raise ValueError('Official tokenizer has no padding identity')
    body = f'<Instruct>: {INSTRUCTION}\n<Query>: {query}\n<Document>: {document}'
    # The publisher's Transformers reference tokenizes each of these exact
    # components separately, then concatenates before left padding.
    ids = (tokenizer.encode(PREFIX, add_special_tokens=False)
           + tokenizer.encode(body, add_special_tokens=False)
           + tokenizer.encode(SUFFIX, add_special_tokens=False))
    if len(ids) > maximum:
        raise ValueError(f'Focus pair needs {len(ids)} tokens; maximum={maximum}; no hidden truncation')
    pad = maximum - len(ids)
    return Pair(tuple([tokenizer.pad_token_id] * pad + ids), tuple([0] * pad + [1] * len(ids)),
                tuple(range(maximum)), len(ids), maximum,
                len(query.encode('utf8')), len(document.encode('utf8')))

def relevance_score(logit_no, logit_yes):
    if isinstance(logit_no, bool) or isinstance(logit_yes, bool):
        raise ValueError('Focus logits must be finite numeric observations')
    try:
        negative, positive = float(logit_no), float(logit_yes)
    except (ValueError, TypeError) as error:
        raise ValueError('Focus logits must be finite numeric observations') from error
    if not math.isfinite(negative) or not math.isfinite(positive):
        raise ValueError('Focus logits must be finite numeric observations')
    maximum = max(negative, positive)
    no, yes = math.exp(negative - maximum), math.exp(positive - maximum)
    return yes / (no + yes)

def rank_complete_pool(candidate_ids, logits, maximum_candidates=64):
    if type(maximum_candidates) is not int or not 1 <= maximum_candidates <= 64:
        raise ValueError('Focus requires the owning bounded profile candidate allowance')
    if not candidate_ids or len(candidate_ids) > maximum_candidates or len(candidate_ids) != len(logits):
        raise ValueError('Focus must rank the complete original bounded pool')
    if any(not isinstance(item, str) or not item for item in candidate_ids) or len(set(candidate_ids)) != len(candidate_ids):
        raise ValueError('Focus candidate identities must remain unique and immutable')
    rows = []
    for rank, (candidate, pair) in enumerate(zip(candidate_ids, logits)):
        if not isinstance(pair, (list, tuple)) or len(pair) != 2:
            raise ValueError('Focus requires exactly no/yes raw logits for each candidate')
        rows.append({'candidate_id': candidate, 'native_rank': rank, 'logit_no': pair[0],
                     'logit_yes': pair[1], 'relevance_score': relevance_score(*pair)})
    return sorted(rows, key=lambda row: (-row['relevance_score'], row['native_rank']))
