# SPDX-License-Identifier: Apache-2.0
"""Pure helpers for the Context slot diagnosis: prompt layout, permutations, outcome labels and score splits.

Nothing here reads a model, a tokenizer or a file. See README.md in this folder.
"""
import hashlib
import json
import statistics

# The Qwen single-choice template of crates/airk-retrieval/src/arbiter/compiler/qwen.rs.
SYSTEM_OPEN = "<|im_start|>system\n"
USER_OPEN = "<|im_start|>user\nState:\n"
QUESTION_OPEN = "Question:\n"
OPTIONS_OPEN = "Options:\n"
SUFFIX = "Choose one option symbol:<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
SYMBOLS = "ABCDEFGH"
TERMINALS = ("sufficient", "clarify", "no_justified_read")


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    sha = hashlib.sha256()
    with open(path, "rb") as file:
        for chunk in iter(lambda: file.read(1 << 24), b""):
            sha.update(chunk)
    return sha.hexdigest()


def pin(path):
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": digest(path)}


def encoded(text):
    """A string as the compiler writes it: compact JSON with non-ASCII characters kept."""
    return json.dumps(text, ensure_ascii=False)


def parse_option(line):
    """One option line, `SYMBOL = "id": "text"`, as [symbol, id, text]."""
    symbol, mark, rest = line.partition(" = ")
    require(bool(mark) and len(symbol) == 1 and symbol in SYMBOLS, "option line has no option symbol")
    decoder = json.JSONDecoder()
    name, end = decoder.raw_decode(rest)
    require(rest[end:end + 2] == ": ", "option line has no id separator")
    text, stop = decoder.raw_decode(rest, end + 2)
    require(stop == len(rest) and type(name) is str and type(text) is str, "option line has trailing or untyped content")
    return [symbol, name, text]


def render_prompt(parts):
    lines = [symbol + " = " + encoded(name) + ": " + encoded(text) for symbol, name, text in parts["options"]]
    return (parts["prefix"] + USER_OPEN + encoded(parts["state"]) + "\n" + QUESTION_OPEN + encoded(parts["question"])
            + "\n" + OPTIONS_OPEN + "\n".join(lines) + "\n" + SUFFIX)


def parse_prompt(prompt):
    """Split a compiled Qwen single-choice prompt into its template parts.

    The state and the question are decoded from their JSON string form. A prompt that does not render back to
    the same characters is refused, so every later edit starts from a layout that is known to be exact.
    """
    prefix, mark, rest = prompt.partition(USER_OPEN)
    require(bool(mark) and prefix.startswith(SYSTEM_OPEN) and prefix.endswith("<|im_end|>\n"),
            "prompt has no system turn followed by the user state")
    state, _, rest = rest.partition("\n")
    require(rest.startswith(QUESTION_OPEN), "prompt has no question after the state")
    question, _, rest = rest[len(QUESTION_OPEN):].partition("\n")
    require(rest.startswith(OPTIONS_OPEN) and rest.endswith("\n" + SUFFIX), "prompt has no option block and suffix")
    lines = rest[len(OPTIONS_OPEN):-len("\n" + SUFFIX)].split("\n")
    parts = {"prefix": prefix, "state": json.loads(state), "question": json.loads(question),
             "options": [parse_option(line) for line in lines]}
    require(type(parts["state"]) is str and type(parts["question"]) is str
            and [option[0] for option in parts["options"]] == list(SYMBOLS[:len(lines)])
            and render_prompt(parts) == prompt, "prompt does not round-trip through the template")
    return parts


def card_lines(roster):
    """The option id and text of every card, rebuilt from a roster.

    This restates `compact_prompt` in crates/airk-retrieval/src/arbiter/context_action.rs, so that the option
    block of a compiled prompt can be checked against the roster it was compiled from.
    """
    units = roster["basis"]["source_units_presented"]
    reads = roster["basis"]["read_choices_scored"]
    sets, lines = [], []
    for index, card in enumerate(roster["cards"]):
        hypotheses = [[units.index(item["evidence"]) + 1, item["role"], item["relevance"]]
                      for item in card["assessments"]]
        name = "none"
        if hypotheses:
            if hypotheses not in sets:
                sets.append(hypotheses)
            name = "h" + str(sets.index(hypotheses) + 1)
        action = card["action"]["kind"]
        if action == "read":
            action = "read r" + str(reads.index(card["action"]["choice"]) + 1)
        lines.append(["a" + str(index + 1),
                      action + "; gap " + card["gap"] + "; reason " + card["reason"] + "; hypotheses " + name])
    return lines


def body_kind(text):
    """The action of an option text: read, sufficient, clarify, no_justified_read or other."""
    action = text.split(";", 1)[0]
    if action.startswith("read r") and action[6:].isascii() and action[6:].isdigit():
        return "read"
    return action if action in TERMINALS else "other"


def card_kind(card):
    """A card's action, gap and reason, with `generic` when it is the generic Read the calibration rule covers."""
    generic = (card["action"]["kind"] == "read" and card["gap"] == "unsupported"
               and card["reason"] == "additional_evidence_may_help"
               and all(item["role"] == "unknown" and item["relevance"] == "uncertain" for item in card["assessments"]))
    return card["action"]["kind"] + "/" + card["gap"] + "/" + card["reason"] + ("/generic" if generic else "")


def read_form(read):
    """A verified read row as form:operation, for example literal:search or target:context."""
    require(type(read) is list and len(read) >= 3 and read[0] in ("literal", "target"), "unexpected verified read row")
    return read[0] + ":" + read[1]


def checked_sources(sources, count):
    require(sorted(sources) == list(range(count)), "sources are not a permutation of the slots")
    return list(sources)


def rotation(count, shift):
    """Sources of a rotation: every body moves `shift` slots towards the end and wraps to the start."""
    require(count >= 1 and 0 <= shift < count, "a rotation shift must be within the slot count")
    return [(slot - shift) % count for slot in range(count)]


def reversal(count):
    return [count - 1 - slot for slot in range(count)]


def moved_options(options, sources, move_names):
    """Options with their bodies permuted: slot j keeps its symbol and shows the body of slot sources[j].

    The body is the option text. With `move_names` the option id travels with the text, otherwise it stays
    with the symbol, which is what a roster built in another card order would produce.
    """
    checked_sources(sources, len(options))
    return [[options[slot][0], options[source][1] if move_names else options[slot][1], options[source][2]]
            for slot, source in enumerate(sources)]


def moved_reads(state, sources):
    """The state text with its verified reads permuted: read number j+1 becomes the read that was sources[j]+1.

    The path table is rebuilt in first-use order, evidence first and then the reads in their new order, as the
    native prompt builder would write it for that order. The identity permutation returns the same characters.
    """
    doc = json.loads(state)
    paths, reads = doc["paths"], doc["verified_reads"]
    checked_sources(sources, len(reads))
    table = []

    def index(path):
        if path not in table:
            table.append(path)
        return table.index(path)

    evidence = [[index(paths[row[0]])] + row[1:] for row in doc["indexed_evidence"]]
    moved = []
    for source in sources:
        read = reads[source]
        moved.append(read[:2] + [index(paths[read[2]])] + read[3:] if read_form(read).startswith("literal:") else read)
    require(sorted(table) == sorted(paths), "the path table names a path that no evidence or read uses")
    doc["paths"], doc["indexed_evidence"], doc["verified_reads"] = table, evidence, moved
    return json.dumps(doc, ensure_ascii=False, separators=(",", ":"))


FAMILIES = ("cards", "cards_ids", "reads", "letters")


def variant_plan(options, reads, families):
    """The permuted inputs to build for one row, as (name, family, sources), after the identity.

    - `cards`: every rotation of the option texts and their reversal. Symbols and option ids stay in place.
    - `cards_ids`: rotations by 1 and 3 and the reversal, with the option id travelling with its text.
    - `reads`: every rotation of the verified reads in the state. The options stay as they are, so each read
      card names another read.
    - `letters`: rotations by 1 and 3 of the option symbols. Every line keeps its place, id and text and takes
      another symbol. The native compiler never writes this; it separates the symbol from the line position.

    `sources` is indexed by slot, and a slot is a symbol: the score at index j belongs to symbol j. For the
    first three families the symbol stays on its line, and sources[j] is the original slot whose body that line
    shows. For `letters`, sources[j] is the line that symbol j is attached to.
    """
    require(set(families) <= set(FAMILIES), "unknown variant family")
    plan = [("identity", "identity", list(range(options)))]
    if "cards" in families:
        plan += [("cards_rot" + str(shift), "cards", rotation(options, shift)) for shift in range(1, options)]
        plan.append(("cards_reverse", "cards", reversal(options)))
    if "cards_ids" in families:
        plan += [("cards_ids_rot" + str(shift), "cards_ids", rotation(options, shift))
                 for shift in (1, 3) if shift < options]
        plan.append(("cards_ids_reverse", "cards_ids", reversal(options)))
    if "reads" in families:
        plan += [("reads_rot" + str(shift), "reads", rotation(reads, shift)) for shift in range(1, reads)]
    if "letters" in families:
        plan += [("letters_rot" + str(shift), "letters", rotation(options, shift))
                 for shift in (1, 3) if shift < options]
    return plan


def moved_symbols(options, sources):
    """Options with their symbols permuted: symbol j goes to line sources[j], and every line keeps its id and text."""
    checked_sources(sources, len(options))
    return [[options[sources.index(line)][0], name, text] for line, (_, name, text) in enumerate(options)]


def variant_prompt(parts, family, sources):
    """The prompt of one variant, from the parts of the original prompt."""
    if family == "identity":
        require(list(sources) == list(range(len(parts["options"]))), "the identity moves nothing")
        return render_prompt(parts)
    if family in ("cards", "cards_ids"):
        return render_prompt(parts | {"options": moved_options(parts["options"], sources, family == "cards_ids")})
    if family == "letters":
        return render_prompt(parts | {"options": moved_symbols(parts["options"], sources)})
    require(family == "reads", "unknown variant family")
    return render_prompt(parts | {"state": moved_reads(parts["state"], sources)})


def full_sources(sources, count):
    """A permutation of the first slots extended to all slots, the remaining ones staying in place."""
    require(len(sources) <= count, "more sources than slots")
    return checked_sources(list(sources) + list(range(len(sources), count)), count)


def order(scores):
    """Slots by descending score. The lower slot comes first on a tie."""
    return sorted(range(len(scores)), key=lambda slot: (-scores[slot], slot))


def best(scores, slots):
    """The best-scoring slot among the given ones, the lowest on a tie. None when there is no slot."""
    return min(slots, key=lambda slot: (-scores[slot], slot)) if slots else None


def follow(before, after, sources):
    """How a tracked choice behaved under a permutation.

    `before` is the slot it held in the original input and `after` the slot chosen in the permuted one.
    `content` means the choice went to the slot that now shows the same body. `slot` means it stayed on the
    original slot although that slot now shows another body. `neither` is anything else. When the body did not
    move, the two cannot be told apart, and the label is `unmoved_kept` or `unmoved_lost`.
    """
    target = sources.index(before)
    if target == before:
        return "unmoved_kept" if after == before else "unmoved_lost"
    return "content" if after == target else "slot" if after == before else "neither"


def centred(scores):
    """Scores minus their mean. A shift common to all options does not change the choice."""
    mean = sum(scores) / len(scores)
    return [score - mean for score in scores]


def spread(values):
    return {"count": len(values), "mean": statistics.fmean(values),
            "sd": statistics.stdev(values) if len(values) > 1 else 0.0, "min": min(values), "max": max(values)}


def profile_share(vectors):
    """The share of the centred score variation that one fixed per-slot profile explains across rows.

    Each vector is centred, the mean centred score of every slot is the profile, and the share is the profile's
    sum of squares over the total. It is 1 when every row has the same centred scores and 0 when the slots have
    no common tendency. Slot and card are not separated here, because every row shows the cards in one order.
    """
    rows = [centred(vector) for vector in vectors]
    require(bool(rows) and len({len(row) for row in rows}) == 1, "vectors are absent or differ in length")
    profile = [statistics.fmean(row[slot] for row in rows) for slot in range(len(rows[0]))]
    total = sum(value * value for row in rows for value in row)
    return {"profile": profile, "share": len(rows) * sum(value * value for value in profile) / total if total else None}


def without_profile(vectors):
    """Centred vectors with the mean centred scores of all other rows removed, slot by slot.

    This is what remains of a row once a fixed per-slot tendency is taken away. The row itself is left out of
    the mean it is compared with, so that it does not explain its own scores. At least two rows are needed.
    """
    rows = [centred(vector) for vector in vectors]
    require(len(rows) >= 2 and len({len(row) for row in rows}) == 1, "fewer than two vectors, or differing lengths")
    width, others = len(rows[0]), len(rows) - 1
    totals = [sum(row[slot] for row in rows) for slot in range(width)]
    return [[row[slot] - (totals[slot] - row[slot]) / others for slot in range(width)] for row in rows]


def effects(forwards):
    """Split centred scores into a slot part and a body part over balanced permuted forwards.

    Each forward is (row, sources, scores): the row it belongs to, the body shown at every slot and one score
    per slot. Scores are centred within a forward. The design must be balanced within every row, so that each
    body is seen at each slot equally often; a full set of rotations is. The slot and body parts are then
    orthogonal and their sums of squares add up.

    Shares of the total sum of squares:
    - `slot_common`: the mean centred score of each slot over all forwards, the part that follows the slot
      whatever body it shows and whatever the row;
    - `slot_by_row`: what each row's own slot means add to that;
    - `body_common`: the mean centred score of each body over all forwards, a preference for a body whatever
      the row;
    - `body_by_row`: what each row's own body means add to that, the part that depends on the body and the row;
    - `residual`: the rest, which no slot or body mean of a row explains.
    """
    by_row = {}
    for row, sources, scores in forwards:
        checked_sources(sources, len(scores))
        by_row.setdefault(row, []).append((list(sources), centred(scores)))
    require(bool(by_row), "no forward given")
    count = len(next(iter(by_row.values()))[0][0])
    slots = range(count)
    total = slot_rows = body_rows = 0.0
    slot_sum, body_sum, row_body, seen = [0.0] * count, [0.0] * count, {}, 0
    for row, items in by_row.items():
        pairs = {}
        own_slot, own_body = [0.0] * count, [0.0] * count
        for sources, values in items:
            require(len(sources) == count, "forwards differ in slot count")
            for slot in slots:
                pairs[(slot, sources[slot])] = pairs.get((slot, sources[slot]), 0) + 1
                own_slot[slot] += values[slot]
                own_body[sources[slot]] += values[slot]
                total += values[slot] * values[slot]
        require(len(pairs) == count * count and len(set(pairs.values())) == 1,
                "the forwards of a row do not show every body at every slot equally often")
        seen += len(items)
        for slot in slots:
            slot_sum[slot] += own_slot[slot]
            body_sum[slot] += own_body[slot]
        own_slot = [value / len(items) for value in own_slot]
        own_body = [value / len(items) for value in own_body]
        slot_rows += len(items) * sum(value * value for value in own_slot)
        body_rows += len(items) * sum(value * value for value in own_body)
        row_body[row] = own_body
    slot_mean = [value / seen for value in slot_sum]
    body_mean = [value / seen for value in body_sum]
    slot_common = seen * sum(value * value for value in slot_mean)
    body_common = seen * sum(value * value for value in body_mean)
    require(total > 0, "all centred scores are zero")
    share = {"slot_common": slot_common / total, "slot_by_row": (slot_rows - slot_common) / total,
             "body_common": body_common / total, "body_by_row": (body_rows - body_common) / total,
             "residual": (total - slot_rows - body_rows) / total}
    return {"rows": len(by_row), "forwards": seen, "slot_mean": slot_mean, "body_mean": body_mean, "share": share,
            "row_body": row_body}
