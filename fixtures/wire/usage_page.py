# SPDX-License-Identifier: Apache-2.0
"""Generate the usage-page codec vectors (split contract C1, docs/split/contracts.md §1).

The Rust owner is `airk_core::usage_page` (`Part`, `Record`, `Page`, `page`,
`assemble`). This generator restates the wire format below and computes every
expected byte string, digest, cursor and assembled document itself. Nothing
here is read back from the code under test.

Canonical bytes (the serde_json compact form with its default `Map`):
  * no whitespace; objects write their keys in ascending UTF-8 byte order
    (serde_json is built without `preserve_order` in this workspace, so a
    `Value` object is a sorted map); struct fields keep declaration order;
  * strings escape `"` and `\\`, write U+0008/0009/000A/000C/000D as
    `\\b \\t \\n \\f \\r`, every other code point below U+0020 as `\\u00xx`
    (lowercase hex), and every other character as raw UTF-8 (U+007F included);
  * integers are decimal. Floating-point numbers are outside this fixture: their
    shortest-round-trip formatting is not restated here.

`Part` is externally tagged snake_case: `{"key":s}`, `{"index":n}`,
`{"string_chunk":{"offset":n}}`. `Record` is `{"path":[...],"value":v}`.
`Page` is `{"digest","items","cursor","record_bytes","total_items"}`; the two
scope headers are omitted when absent (unscoped pages only in this fixture).

Paging rule: the document digest is the lowercase SHA-256 hex of its canonical
bytes. A value whose record encodes to at most 16384 bytes is one record;
otherwise an object splits into its keys (ascending), an array into its
indexes, and a string into chunks of 2000 characters whose `offset` is the
UTF-8 byte offset of the chunk. A page budget is 20480..=49152 bytes; records
are taken in order while their summed encoded size stays within the budget
minus 1024 bytes. A page that does not reach the last record carries the
cursor `airk-u1:<digest>:<next record index>`; a cursor is accepted only with
that prefix, the current digest, a positive in-range index and no extra field.

Assembly writes each record's value at its path into a document that starts as
null: a key creates an object, an index creates or extends an array with
nulls, and a trailing string chunk appends to the string at that path only at
exactly its current byte length. A non-null target, a crossed container, a
chunk that is not last, or an index of 4096 or more is refused.

Large inputs are written as `{"$repeat": s, "count": n}`, which every consumer
expands to `s` repeated `n` times before use. Pages longer than 4096 bytes keep
their exact bytes as `bytes_sha256` and `bytes_length`; shorter ones as `bytes`.
Refusals record only that the call is refused; error messages are not a contract.

    python fixtures/wire/usage_page.py          # rewrite the JSON
    python fixtures/wire/usage_page.py --check  # exit 1 on drift
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

OUTPUT = Path(__file__).with_name("usage_page.json")
SCHEMA = "airk.wire-fixture.usage-page/1"
MAX_RECORD_BYTES = 16 << 10
CHUNK_CHARACTERS = 2000
PAGE_OVERHEAD = 1024
MIN_PAGE, MAX_PAGE = 20 << 10, 48 << 10
MAX_RECORDS = 4096
INLINE_PAGE_BYTES = 4096
SHORT_ESCAPES = {0x08: "\\b", 0x09: "\\t", 0x0A: "\\n", 0x0C: "\\f", 0x0D: "\\r", 0x22: '\\"', 0x5C: "\\\\"}


class Struct:
    """A serde struct: fields serialize in declaration order, unlike a Value object."""

    def __init__(self, *fields):
        self.fields = fields


class Refused(Exception):
    pass


def encode(value):
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        raise ValueError("floating-point formatting is outside this fixture")
    if isinstance(value, str):
        out = []
        for character in value:
            point = ord(character)
            if point in SHORT_ESCAPES:
                out.append(SHORT_ESCAPES[point])
            elif point < 0x20:
                out.append("\\u%04x" % point)
            else:
                out.append(character)
        return '"' + "".join(out) + '"'
    if isinstance(value, list):
        return "[" + ",".join(encode(item) for item in value) + "]"
    if isinstance(value, dict):
        keys = sorted(value, key=lambda key: key.encode("utf-8"))
        return "{" + ",".join(encode(key) + ":" + encode(value[key]) for key in keys) + "}"
    if isinstance(value, Struct):
        return "{" + ",".join(encode(name) + ":" + encode(item) for name, item in value.fields) + "}"
    raise TypeError(type(value))


def size(value):
    return len(encode(value).encode("utf-8"))


def part(kind, value):
    return {"string_chunk": {"offset": value}} if kind == "string_chunk" else {kind: value}


def record(path, value):
    return Struct(("path", list(path)), ("value", value))


def expand(value):
    if isinstance(value, dict):
        if set(value) == {"$repeat", "count"}:
            return value["$repeat"] * value["count"]
        return {key: expand(item) for key, item in value.items()}
    if isinstance(value, list):
        return [expand(item) for item in value]
    return value


def split(value, path, records):
    whole = record(path, value)
    if size(whole) <= MAX_RECORD_BYTES:
        records.append(whole)
        return
    if isinstance(value, str):
        offset = 0
        for start in range(0, len(value), CHUNK_CHARACTERS):
            chunk = value[start:start + CHUNK_CHARACTERS]
            piece = record(path + [part("string_chunk", offset)], chunk)
            if size(piece) > MAX_RECORD_BYTES:
                raise Refused("chunk exceeds record budget")
            records.append(piece)
            offset += len(chunk.encode("utf-8"))
    elif isinstance(value, dict):
        for key in sorted(value, key=lambda key: key.encode("utf-8")):
            split(value[key], path + [part("key", key)], records)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            split(item, path + [part("index", index)], records)
    else:
        raise Refused("scalar exceeds record budget")


def digest(value):
    return hashlib.sha256(encode(value).encode("utf-8")).hexdigest()


def page(value, cursor, max_bytes):
    """Return (page struct, item records) or raise Refused."""
    if not MIN_PAGE <= max_bytes <= MAX_PAGE:
        raise Refused("budget")
    document_digest = digest(value)
    offset = 0
    if cursor is not None:
        fields = cursor.split(":")
        if (len(cursor) > 100 or len(fields) != 3 or fields[0] != "airk-u1" or fields[1] != document_digest
                or not fields[2].isdigit() or int(fields[2]) <= 0):
            raise Refused("cursor")
        offset = int(fields[2])
    records = []
    split(value, [], records)
    if offset >= len(records):
        raise Refused("cursor beyond snapshot")
    items, used = [], 0
    for item in records[offset:]:
        if used + size(item) > max_bytes - PAGE_OVERHEAD:
            break
        used += size(item)
        items.append(item)
    if not items:
        raise Refused("no record fits")
    following = offset + len(items)
    next_cursor = f"airk-u1:{document_digest}:{following}" if following < len(records) else None
    result = Struct(("digest", document_digest), ("items", items), ("cursor", next_cursor),
                    ("record_bytes", used), ("total_items", len(records)))
    if size(result) > max_bytes:
        raise Refused("framing exceeds budget")
    return result, items


def plain(value):
    """A Struct as the JSON value a consumer parses (for item records in the fixture)."""
    if isinstance(value, Struct):
        return {name: plain(item) for name, item in value.fields}
    if isinstance(value, list):
        return [plain(item) for item in value]
    return value


def assemble(records):
    document = [None]

    def place(holder, key, path, value, chunk):
        target = holder[key]
        if not path:
            if chunk is not None:
                if target is None:
                    target = ""
                if not isinstance(target, str) or len(target.encode("utf-8")) != chunk:
                    raise Refused("string chunk")
                if not isinstance(value, str):
                    raise Refused("chunk value")
                holder[key] = target + value
                return
            if target is not None:
                raise Refused("overlap")
            holder[key] = value
            return
        head, rest = path[0], path[1:]
        if "string_chunk" in head:
            raise Refused("chunk must end its path")
        if "key" in head:
            if target is None:
                target = holder[key] = {}
            if not isinstance(target, dict):
                raise Refused("crossed object")
            target.setdefault(head["key"], None)
            place(target, head["key"], rest, value, chunk)
        else:
            index = head["index"]
            if index >= MAX_RECORDS:
                raise Refused("index budget")
            if target is None:
                target = holder[key] = []
            if not isinstance(target, list):
                raise Refused("crossed array")
            target.extend([None] * (index + 1 - len(target)))
            place(target, index, rest, value, chunk)

    for item in records:
        path, chunk = item["path"], None
        if path and "string_chunk" in path[-1]:
            path, chunk = path[:-1], path[-1]["string_chunk"]["offset"]
        place(document, 0, path, item["value"], chunk)
    return document[0]


def page_case(name, rule, document, max_bytes, pages):
    """`pages` is how many pages the walk from the first cursor must take (a reviewed count)."""
    value = expand(document)
    walked, cursor, items = [], None, []
    while True:
        result, records = page(value, cursor, max_bytes)
        text = encode(result)
        entry = {"cursor": cursor, "digest": result.fields[0][1], "next_cursor": result.fields[2][1],
                 "record_bytes": result.fields[3][1], "total_items": result.fields[4][1],
                 "paths": [plain(item)["path"] for item in records]}
        if len(text.encode("utf-8")) <= INLINE_PAGE_BYTES:
            entry["bytes"] = text
        else:
            entry["bytes_sha256"] = hashlib.sha256(text.encode("utf-8")).hexdigest()
            entry["bytes_length"] = len(text.encode("utf-8"))
        walked.append(entry)
        items.extend(plain(item) for item in records)
        cursor = result.fields[2][1]
        if cursor is None:
            break
    if len(walked) != pages:
        raise SystemExit(f"{name}: walked {len(walked)} pages, reviewed {pages}")
    if assemble(items) != value:
        raise SystemExit(f"{name}: generator reassembly differs")
    return {"name": name, "rule": rule, "input": document, "max_bytes": max_bytes, "pages": walked}


def page_cases():
    yield page_case("single_record", "a small document is one root record", {"project": "legacy", "value": {"all_fields": [1, 2, 3]}},
                    20 << 10, 1)
    yield page_case("escapes_and_order", "string escapes, raw UTF-8 and sorted object keys",
                    {"text": "quote \" backslash \\ newline \n tab \t back \b feed \f return \r bell \u0007 del \u007f 月 \U0001f600 /",
                     "empty": {}, "list": [], "null": None, "flag": True, "negative": -5, "zero": 0}, 20 << 10, 1)
    split_document = {"a": {"$repeat": "x", "count": 9000}, "b": [{"$repeat": "y", "count": 9000}, {"$repeat": "w", "count": 9000}],
                      "c": {"$repeat": "z", "count": 9000}}
    yield page_case("split_object_and_array", "an oversized object splits by key and an oversized array by index; two pages",
                    split_document, 20 << 10, 2)
    yield page_case("split_at_maximum_budget", "the same records fit one page at the 48 KiB maximum", split_document, 48 << 10, 1)
    yield page_case("string_chunks_multibyte", "an oversized string splits into 2000-character chunks at UTF-8 byte offsets",
                    {"export": {"$repeat": "\"月\",report\n", "count": 1100}}, 20 << 10, 1)


def refused_pages():
    split_document = {"a": {"$repeat": "x", "count": 9000}, "b": [{"$repeat": "y", "count": 9000}, {"$repeat": "w", "count": 9000}],
                      "c": {"$repeat": "z", "count": 9000}}
    current = digest(expand(split_document))
    small = {"project": "legacy"}
    for name, rule, document, cursor, max_bytes in [
        ("budget_below_minimum", "page budget must be at least 20 KiB", small, None, (20 << 10) - 1),
        ("budget_above_maximum", "page budget must be at most 48 KiB", small, None, (48 << 10) + 1),
        ("cursor_other_digest", "a changed document restarts pagination", split_document, "airk-u1:" + "0" * 64 + ":2", 20 << 10),
        ("cursor_zero", "a cursor index is positive", split_document, f"airk-u1:{current}:0", 20 << 10),
        ("cursor_at_end", "a cursor index is below the record count", split_document, f"airk-u1:{current}:4", 20 << 10),
        ("cursor_extra_field", "a cursor has exactly three fields", split_document, f"airk-u1:{current}:2:0", 20 << 10),
        ("cursor_other_prefix", "a cursor starts with airk-u1", split_document, f"airk-u2:{current}:2", 20 << 10),
    ]:
        try:
            page(expand(document), cursor, max_bytes)
        except Refused:
            yield {"name": name, "rule": rule, "input": document, "cursor": cursor, "max_bytes": max_bytes}
            continue
        raise SystemExit(f"{name}: the stated rule does not refuse this vector")


def key(name):
    return {"key": name}


def index(position):
    return {"index": position}


def chunk(offset):
    return {"string_chunk": {"offset": offset}}


def assemble_cases():
    accepted = [
        ("key_then_index", "an index extends a new array with nulls", [{"path": [key("a"), index(2)], "value": 1}],
         {"a": [None, None, 1]}),
        ("indexes_out_of_order", "records may arrive in any index order",
         [{"path": [key("a"), index(1)], "value": "b"}, {"path": [key("a"), index(0)], "value": "a"}], {"a": ["a", "b"]}),
        ("string_chunks_byte_offsets", "chunk offsets count UTF-8 bytes",
         [{"path": [key("s"), chunk(0)], "value": "月"}, {"path": [key("s"), chunk(3)], "value": "x"}], {"s": "月x"}),
        ("root_record", "an empty path is the whole document", [{"path": [], "value": {"k": 1}}], {"k": 1}),
        ("empty_containers", "empty objects and arrays are values, not absences",
         [{"path": [key("e")], "value": {}}, {"path": [key("l")], "value": []}], {"e": {}, "l": []}),
    ]
    refused = [
        ("overlap", "a path is written once", [{"path": [key("a")], "value": 1}, {"path": [key("a")], "value": 2}]),
        ("chunk_gap", "chunks are contiguous", [{"path": [key("s"), chunk(0)], "value": "ab"}, {"path": [key("s"), chunk(3)], "value": "c"}]),
        ("chunk_character_offset", "a character offset is not a byte offset",
         [{"path": [key("s"), chunk(0)], "value": "月"}, {"path": [key("s"), chunk(1)], "value": "x"}]),
        ("crossed_object", "a scalar is not an object", [{"path": [key("a")], "value": 1}, {"path": [key("a"), key("b")], "value": 2}]),
        ("crossed_array", "an object is not an array", [{"path": [key("a")], "value": {}}, {"path": [key("a"), index(0)], "value": 1}]),
        ("chunk_not_last", "a chunk ends its path", [{"path": [chunk(0), key("a")], "value": "x"}]),
        ("chunk_onto_scalar", "a chunk appends only to a string", [{"path": [key("a")], "value": 1}, {"path": [key("a"), chunk(0)], "value": "x"}]),
        ("index_budget", "an index is below 4096", [{"path": [index(4096)], "value": 1}]),
    ]
    for name, rule, records, document in accepted:
        if assemble(records) != document:
            raise SystemExit(f"{name}: the stated rule does not produce the reviewed document")
        yield {"name": name, "rule": rule, "records": records, "document": document}
    for name, rule, records in refused:
        try:
            assemble(records)
        except Refused:
            yield {"name": name, "rule": rule, "records": records, "refused": True}
            continue
        raise SystemExit(f"{name}: the stated rule does not refuse this vector")


def digest_cases():
    for name, text in [
        ("empty_object", "{}"),
        ("unsorted_keys", '{"b":1,"a":{"d":2,"c":3},"A":[true,null]}'),
        ("whitespace_and_escapes", '{ "s" : "\\u00e9\\u0001\\/" , "n" : -7 }'),
    ]:
        value = json.loads(text)
        yield {"name": name, "input_json": text, "canonical": encode(value), "sha256": digest(value)}


REJECTED = [
    ("record_unknown_field", "record", '{"path":[],"value":1,"extra":true}'),
    ("record_missing_value", "record", '{"path":[]}'),
    ("part_unknown_variant", "record", '{"path":[{"field":"a"}],"value":1}'),
    ("part_negative_index", "record", '{"path":[{"index":-1}],"value":1}'),
    ("page_unknown_field", "page", '{"digest":"00","items":[],"cursor":null,"record_bytes":0,"total_items":0,"extra":1}'),
]


def render():
    document = {
        "schema": SCHEMA,
        "rust": "airk_core::usage_page",
        "generator": "fixtures/wire/usage_page.py",
        "pages": list(page_cases()),
        "refused_pages": list(refused_pages()),
        "assemble": list(assemble_cases()),
        "digests": list(digest_cases()),
        "rejected": [{"name": name, "kind": kind, "json": text} for name, kind, text in REJECTED],
    }
    return json.dumps(document, indent=1, ensure_ascii=True, sort_keys=True) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    rendered = render()
    parsed = json.loads(rendered)
    names = [entry["name"] for group in ("pages", "refused_pages", "assemble", "digests", "rejected") for entry in parsed[group]]
    if len(names) != len(set(names)):
        raise SystemExit("duplicate case names")
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != rendered:
            print(f"{OUTPUT} is stale; rerun without --check and review the diff", file=sys.stderr)
            return 1
        return 0
    OUTPUT.write_bytes(rendered.encode("utf-8"))
    print(f"{len(names)} cases -> {OUTPUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
