# SPDX-License-Identifier: Apache-2.0
"""Private deterministic F2 admission mechanics. No tokenizer, model or score calls."""
import ast
import copy
import hashlib
import json
import math
import re
from collections import Counter

DOMAIN = 'airk.f2.exact-human-binary-pilot.split/1\nseed1908\n'
ROLES = ('TRAIN', 'DEV', 'CAL_RESERVED', 'CONFIRMATION_RESERVED', 'RESEARCH_TEST_RESERVED')

def digest(raw):
    return hashlib.sha256(raw).hexdigest()

def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()

class DSU:
    def __init__(self, keys):
        self.parent = {k: k for k in keys}
    def find(self, key):
        while self.parent[key] != key:
            self.parent[key] = self.parent[self.parent[key]]
            key = self.parent[key]
        return key
    def union(self, a, b):
        a, b = self.find(a), self.find(b)
        self.parent[max(a, b)] = min(a, b)
    def components(self):
        groups = {}
        for k in sorted(self.parent):
            groups.setdefault(self.find(k), []).append(k)
        return {k: '|'.join(v) for v in groups.values() for k in v}

class Alpha(ast.NodeTransformer):
    """Only bound local identifiers rename; globals, attributes/constants stay literal."""
    def __init__(self, node):
        names = []
        for n in ast.walk(node):
            value = n.arg if isinstance(n, ast.arg) else n.id if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store) else None
            if value is not None and value not in names:
                names.append(value)
        self.names = {x: 'local_' + str(i) for i, x in enumerate(names)}
    def visit_Name(self, node):
        node.id = self.names.get(node.id, node.id)
        return node
    def visit_arg(self, node):
        node.arg = self.names.get(node.arg, node.arg)
        return self.generic_visit(node)
    def visit_FunctionDef(self, node):
        node.name = 'function'
        if node.body and isinstance(node.body[0], ast.Expr) and isinstance(node.body[0].value, ast.Constant) and isinstance(node.body[0].value.value, str):
            node.body = node.body[1:]
        return self.generic_visit(node)
    visit_AsyncFunctionDef = visit_FunctionDef

def normalized_node(node):
    return Alpha(node).visit(copy.deepcopy(node))

def node_hash(node):
    return digest(ast.dump(node, annotate_fields=True, include_attributes=False).encode())

def fingerprints(source):
    tree = ast.parse(source)
    output = set()
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        if sum(1 for _ in ast.walk(node)) < 25:
            continue
        normalized = normalized_node(node)
        output.add(('alpha_function', node_hash(normalized)))
        # Partial-copy constraints are intentionally conservative. Shared snippets
        # only co-locate; they never create labels or establish semantic identity.
        for parent in ast.walk(normalized):
            statements = getattr(parent, 'body', None)
            if not isinstance(statements, list) or not all(isinstance(x, ast.stmt) for x in statements):
                continue
            for start in range(len(statements)):
                for size in range(2, min(5, len(statements) - start) + 1):
                    block = ast.Module(body=statements[start:start + size], type_ignores=[])
                    if sum(1 for _ in ast.walk(block)) >= 40:
                        output.add(('alpha_partial_2to5_statements_min40nodes', node_hash(block)))
    return output

def roles_for(components, forced_reserved, quarantine):
    """No counts, labels, outcomes or desired sizes participate in permutation."""
    eligible = sorted(set(components) - set(forced_reserved) - set(quarantine), key=lambda k: digest((DOMAIN + k).encode()))
    n = len(eligible)
    # Fixed component fractions before any score:60/15/10/10/5. All source was
    # upstream TRAIN and audited: the final slot is research-only, never FINAL.
    cutoffs = [math.floor(n * p) for p in (.60, .75, .85, .95)]
    roles = {}
    for i, k in enumerate(eligible):
        roles[k] = ROLES[sum(i >= c for c in cutoffs)]
    roles.update({k: 'RESEARCH_TEST_RESERVED' for k in forced_reserved if k not in quarantine})
    roles.update({k: 'QUARANTINE_SOURCE_PARSE_OR_EXPOSURE' for k in quarantine})
    return roles

def assert_pair_scope(row):
    if row['rights_review'] != 'PERMITTED_PRIVATE_RESEARCH_SCOPE':
        raise ValueError('unknown rights')
    if row['native_visibility'] != 'accepted':
        raise ValueError('native refusal')
    if row['label'] not in (0, 1):
        raise ValueError('unknown relevance')
    if row.get('upstream_role') != 'publisher_TRAIN':
        raise ValueError('reserved source cannot become TRAIN')
    if row.get('unjudged_cross_pair', False):
        raise ValueError('cross pair has no human label')

def assert_compiled(compiled):
    # These inspect the existing Rust-emitted values, never reconstruct a DTO or
    # generate inputs. A changed instruction must return to the native compiler.
    if compiled['candidate_positions'] != [2152, 9693] or compiled['option_mask'] != [1, 1] or compiled['decision_position'] != 511:
        raise ValueError('native response ABI changed')
    ids, mask, pos = (compiled[k] for k in ('input_ids', 'attention_mask', 'position_ids'))
    if any(len(v) != 512 for v in (ids, mask, pos)):
        raise ValueError('no cropping or padding inference')
    live = sum(mask)
    if not 0 < live <= 512 or mask != [0] * (512-live) + [1] * live:
        raise ValueError('not exact native left padding')
    if not re.fullmatch('[0-9a-f]{64}', compiled['input_digest']):
        raise ValueError('unbound native input')
    return live

def supervised_target(label):
    if label not in (0, 1):
        raise ValueError('unknown is not a negative')
    return (2152, 9693)[label]

def decision_labels(label):
    # The 512 native prompt tokens are all masked. A response target is appended
    # only by a future training owner, which must verify shifted causal loss and
    # full-vocabulary head; this is not a changed native inference input.
    return [-100] * 512 + [supervised_target(label)]

def sign_bound(n, probability=.8):
    threshold = next((k for k in range(n // 2 + 1, n + 1) if 2 * sum(math.comb(n, i) for i in range(k, n + 1)) / 2**n <= .05), None)
    return {'independent_components_assumed': n, 'positive_sign_threshold': threshold,
            'two_sided_alpha': 0 if threshold is None else 2 * sum(math.comb(n, i) for i in range(threshold, n + 1)) / 2**n,
            'conditional_power_Pwin_0_8': 0 if threshold is None else sum(math.comb(n, i) * probability**i * (1-probability)**(n-i) for i in range(threshold, n+1))}

def metrics(labels, margins):
    """Pointwise diagnostic only; never treats singleton labels as ranking gold."""
    if len(labels) != len(margins) or not labels or any(y not in (0,1) for y in labels) or any(not math.isfinite(x) for x in margins):
        raise ValueError('incomplete or nonfinite scores')
    positive = [s for y,s in zip(labels,margins) if y]
    negative = [s for y,s in zip(labels,margins) if not y]
    if not positive or not negative:
        raise ValueError('both classes required')
    auc = sum((a>b)+.5*(a==b) for a in positive for b in negative)/(len(positive)*len(negative))
    # Fixed zero decision is a diagnostic; this does not create probability or a
    # learned acceptance cutoff. Tie at0 predicts negative by preregistration.
    tp=sum(y==1 and s>0 for y,s in zip(labels,margins));tn=sum(y==0 and s<=0 for y,s in zip(labels,margins))
    return {'pointwise_auroc':auc,'balanced_accuracy_at_fixed_zero':.5*(tp/len(positive)+tn/len(negative)),'ranking_or_terminal_quality_claim':False}

def require_execution_bindings(card):
    if card['root_review_accepted'] is not True:
        raise ValueError('candidate has no execution release')
    if card['instruction_selection']['native_F1_compilation'] is None or card['instruction_selection']['DEV_choice_receipt'] is None:
        raise ValueError('same native instruction must be selected before F2')
    if card['general_document_replay']['fraction'] is None or card['general_document_replay']['accepted_native_roster'] is None:
        raise ValueError('general replay is not zero by default')
    if card['training']['measured_probe_receipt'] is None:
        raise ValueError('bounded measured probe precedes longer pilot')
    if card.get('promotion',False):
        raise ValueError('this pointwise pilot cannot authorize promotion')
