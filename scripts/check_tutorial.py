#!/usr/bin/env python3
"""Execute Markdown-only tutorial sources against complete public CLI contracts."""

import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def load_examples(root=ROOT):
    text = (root / "docs/tutorials/koven-tour.md").read_text()
    blocks = re.findall(r"^```koven ([a-z0-9-]+)\n(.*?)^```$", text, re.M | re.S)
    sources = dict(blocks)
    rows = json.loads((root / "docs/tutorials/examples.json").read_text())["examples"]
    ids = [row["id"] for row in rows]
    references = []
    for row in rows:
        files = row.get("files", {"source.ko": row["id"]})
        for path, source_id in files.items():
            parsed = PurePosixPath(path)
            if parsed.is_absolute() or ".." in parsed.parts or parsed.suffix != ".ko":
                raise ValueError("tutorial file must be a relative .ko path without traversal")
            references.append(source_id)
        if not files or row["id"] not in files.values():
            raise ValueError("example must reference its own source fence")
        if "files" in row and not row.get("entry"):
            raise ValueError("project example requires explicit entry")
        if "cases" in row:
            cases = row["cases"]
            if (row["status"] != "executable" or not isinstance(cases, list) or not cases
                    or any(key in row for key in ("args", "artifact", "run"))):
                raise ValueError("argv cases require an executable example with unambiguous outputs")
            for case in cases:
                if (not isinstance(case, dict) or set(case) != {"args", "artifact", "run"}
                        or not isinstance(case["args"], list)
                        or any(not isinstance(arg, str) for arg in case["args"])):
                    raise ValueError("argv case requires a string argument list and both outputs")
            if len({tuple(case["args"]) for case in cases}) != len(cases):
                raise ValueError("duplicate argv case")
    if (len(sources) != len(blocks) or len(set(ids)) != len(ids)
            or len(set(references)) != len(references) or set(references) != set(sources)):
        raise ValueError("tutorial source/contract identity mismatch or duplicate")
    if [row["status"] for row in rows].count("executable") != 15:
        raise ValueError("expected fifteen executable contracts")
    if [row["status"] for row in rows].count("diagnostic") != 4:
        raise ValueError("expected four diagnostic contracts")
    if [row["status"] for row in rows].count("planned") != 2:
        raise ValueError("expected two explicit planned examples")
    if any(row["status"] not in {"executable", "diagnostic", "planned"} for row in rows):
        raise ValueError("unknown tutorial status")
    return [(row, {path: sources[source_id] for path, source_id in
                   row.get("files", {"source.ko": row["id"]}).items()}) for row in rows]


def assert_output(command, expected, cwd):
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True)
    actual = dict(exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
    if actual != expected:
        raise AssertionError(f"{command}: expected {expected!r}, got {actual!r}")


def check(cli, selected=None):
    examples = load_examples()
    ids = {row["id"] for row, _ in examples}
    if selected is not None and (len(set(selected)) != len(selected) or not set(selected) <= ids):
        raise ValueError("unknown or duplicate tutorial selection")
    passed = 0
    planned = 0
    for row, sources in examples:
        if selected is not None and row["id"] not in selected:
            continue
        if row["status"] == "planned":
            print(f"planned: {row['id']} (not executed)")
            planned += 1
            continue
        for case in row.get("cases", [row]):
            with tempfile.TemporaryDirectory(prefix="koven-tutorial-") as temporary:
                directory = Path(temporary)
                for path, source in sources.items():
                    destination = directory / path
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    destination.write_text(source)
                inputs = ["source.ko"]
                if "files" in row:
                    (directory / "project.toml").write_text(
                        'schema = "koven.project"\nversion = 1\n\n[project]\n'
                        'name = "tutorial"\nsource-roots = ["src"]\n')
                    inputs = ["--project", "project.toml", "--entry", row["entry"]]
                build = [str(cli), "build", *inputs, "-o", "program"]
                if row["status"] == "diagnostic":
                    build.insert(1, "--message-format=json")
                assert_output(build, row["build"], directory)
                if row["status"] == "diagnostic":
                    if {p.name for p in directory.iterdir()} != {"source.ko"}:
                        raise AssertionError("failed tutorial build left artifacts")
                else:
                    assert_output([str(directory / "program"), *case["args"]], case["artifact"], directory)
                    assert_output([str(cli), "run", *inputs, "--", *case["args"]], case["run"], directory)
                passed += 1
                suffix = f" argv={case['args']!r}" if "cases" in row else ""
                print(f"passed: {row['id']}{suffix}")
    print(f"tutorial: {passed} executed cases; {planned} planned in selection")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/kovenc")
    parser.add_argument("--example", action="append", help="execute only this example ID; repeatable")
    args = parser.parse_args()
    check(args.cli.resolve(), args.example)
