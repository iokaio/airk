# SPDX-License-Identifier: Apache-2.0
"""Synthetic inputs for the unit tests and the mock smoke run.

Everything here is invented: a tiny fictional repository ("lantern", a lamp
scheduler), a scenario with reference notes on it, a tools catalog whose
schemas mirror the shape of the AIRK catalog, and a mock handle that points
the wrapper at ``mock_cli.py``. No real repository text is used.

``python -I -B synthetic.py <dir>`` writes handle.json, tools-catalog.json,
canned.json and scenarios/airk/C1.json under <dir> for a smoke run.
"""

from __future__ import annotations

import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

SCHEDULER = "\n".join([
    "// lantern scheduler",
    "pub const MAX_LAMPS: usize = 64;",
    "",
    "pub struct Schedule {",
    "    pub lamps: Vec<Lamp>,",
    "}",
    "",
    "impl Schedule {",
    "    /// Refuses more than MAX_LAMPS lamps.",
    "    pub fn add(&mut self, lamp: Lamp) -> Result<(), Full> {",
    "        if self.lamps.len() >= MAX_LAMPS {",
    "            return Err(Full);",
    "        }",
    "        self.lamps.push(lamp);",
    "        Ok(())",
    "    }",
    "}",
    *[f"// filler {n}" for n in range(18, 61)],
])
CONTRACT = "\n".join([
    "# Scheduler contract",
    "",
    "A schedule never holds more than MAX_LAMPS lamps.",
    "Changing MAX_LAMPS changes the stored schedule format.",
    "The format version lives in format.rs.",
])
FORMAT = "\n".join([
    "// lantern format",
    "pub const FORMAT_VERSION: u32 = 3;",
    "pub fn encode(s: &Schedule) -> Vec<u8> { todo!() }",
])
TESTS = "\n".join([
    "#[test]",
    "fn refuses_the_sixty_fifth_lamp() {",
    "    let mut s = Schedule::default();",
    "    for _ in 0..64 { s.add(Lamp::default()).unwrap(); }",
    "    assert!(s.add(Lamp::default()).is_err());",
    "}",
])

FILES = {"src/scheduler.rs": SCHEDULER, "docs/scheduler/CONTRACT.md": CONTRACT,
         "src/format.rs": FORMAT, "tests/scheduler_limits.rs": TESTS}
INSTRUCTION = ("Raise the lantern lamp limit from 64 to 128 and update everything that depends on it, "
               "including the stored format and the tests.")


def canned(spans=None) -> dict:
    return {"files": FILES,
            "spans": spans if spans is not None else [{"path": "src/scheduler.rs", "lines": [1, 16]},
                                                      {"path": "docs/scheduler/CONTRACT.md", "lines": [1, 5]}],
            "symbols": [{"name": "MAX_LAMPS", "kind": "const", "path": "src/scheduler.rs", "lines": [2, 2]},
                        {"name": "Schedule::add", "kind": "method", "path": "src/scheduler.rs", "lines": [10, 16]},
                        {"name": "FORMAT_VERSION", "kind": "const", "path": "src/format.rs", "lines": [2, 2]},
                        {"name": "refuses_the_sixty_fifth_lamp", "kind": "function",
                         "path": "tests/scheduler_limits.rs", "lines": [2, 6]}]}


def scenario() -> dict:
    return {
        "schema": "airk.private.answer-eval-scenario/1", "id": "airk-C1", "repository": "airk",
        "set": "development", "slot": "C1", "kind": "coding", "instruction": INSTRUCTION,
        "leak_probe": "Raise the lantern lamp limit", "source_commit": "0" * 40,
        "notes": [
            {"id": "N1", "mark": "essential", "path": "src/scheduler.rs", "lines": [2, 2], "symbol": "MAX_LAMPS",
             "fact": "MAX_LAMPS is the constant 64 that bounds a schedule.", "alternatives": []},
            {"id": "N2", "mark": "essential", "path": "src/scheduler.rs", "lines": [10, 16], "symbol": "Schedule::add",
             "fact": "Schedule::add refuses a lamp once the schedule holds MAX_LAMPS lamps.", "alternatives": []},
            {"id": "N3", "mark": "essential", "path": "docs/scheduler/CONTRACT.md", "lines": [4, 5], "symbol": None,
             "fact": "Changing MAX_LAMPS changes the stored schedule format, whose version is in format.rs.",
             "alternatives": []},
            {"id": "N4", "mark": "essential", "path": "src/format.rs", "lines": [2, 2], "symbol": "FORMAT_VERSION",
             "fact": "FORMAT_VERSION is 3 and must be raised with a format change.", "alternatives": []},
            {"id": "N5", "mark": "helpful", "path": "tests/scheduler_limits.rs", "lines": [2, 6],
             "symbol": "refuses_the_sixty_fifth_lamp", "fact": "A test pins the 65th lamp refusal.",
             "alternatives": []},
        ],
    }


def _schema(properties: dict, required: list[str]) -> dict:
    return {"type": "object", "properties": {"index": {"type": "string"}, **properties}, "required": ["index", *required]}


def catalog() -> dict:
    s, i = {"type": "string"}, {"type": "integer"}
    tools = [
        ("airk_answer", "Answer a question with grounded evidence.",
         _schema({"schema": s, "question": s, "budget": {"type": "object"}, "response_mode": s},
                 ["schema", "question", "budget"])),
        ("airk_search", "Search the index.", _schema({"query": s, "limit": i, "max_bytes": i}, ["query"])),
        ("airk_parse", "Run a text, recipe or plan.", _schema({"schema": s, "text": s, "mode": s}, ["schema"])),
        ("airk_read_file", "Read a line range (1-based, inclusive).",
         _schema({"path": s, "start_line": i, "end_line": i}, ["path", "start_line", "end_line"])),
        ("airk_context", "Read lines around a line.", _schema({"path": s, "line": i, "before": i, "after": i},
                                                             ["path", "line"])),
        ("airk_references", "Find references to a name.", _schema({"name": s}, ["name"])),
        ("airk_symbols", "List symbols.", _schema({"name": s, "path": s}, [])),
        ("airk_outline", "Outline a file.", _schema({"path": s}, ["path"])),
        ("airk_status", "Status.", _schema({}, [])),
    ]
    return {"tools": [{"name": n, "description": d, "inputSchema": sch} for n, d, sch in tools]}


def write_fixture(directory: Path, spans=None) -> dict[str, Path]:
    directory = Path(directory)
    paths = {"canned": directory / "canned.json", "catalog": directory / "tools-catalog.json",
             "handle": directory / "lifetime" / "handle.json", "scenario": directory / "scenarios" / "airk" / "C1.json"}
    common.write_json(paths["canned"], canned(spans))
    common.write_json(paths["catalog"], catalog())
    common.write_json(paths["scenario"], scenario())
    common.write_json(paths["handle"], {
        "cli": [sys.executable, "-I", "-B", str(HERE / "mock_cli.py"), "--canned", str(paths["canned"])],
        "control": str(directory / "control"), "env": {}, "indexes": {"airk": "mock-airk", "bookbuilder": "mock-bb"}})
    return paths


if __name__ == "__main__":
    for name, path in write_fixture(Path(sys.argv[1])).items():
        print(name, path)
