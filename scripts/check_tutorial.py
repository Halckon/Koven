#!/usr/bin/env python3
"""Execute Markdown-only tutorial sources against complete public CLI contracts."""

import argparse
import json
from pathlib import Path
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
    if len(sources) != len(blocks) or len(set(ids)) != len(ids) or set(ids) != set(sources):
        raise ValueError("tutorial source/contract identity mismatch or duplicate")
    if [row["status"] for row in rows].count("executable") != 7:
        raise ValueError("expected seven executable contracts")
    if [row["status"] for row in rows].count("diagnostic") != 2:
        raise ValueError("expected two diagnostic contracts")
    if [row["status"] for row in rows].count("planned") != 1:
        raise ValueError("expected one explicit planned example")
    return [(row, sources[row["id"]]) for row in rows]


def assert_output(command, expected, cwd):
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True)
    actual = dict(exit=result.returncode, stdout=result.stdout, stderr=result.stderr)
    if actual != expected:
        raise AssertionError(f"{command}: expected {expected!r}, got {actual!r}")


def check(cli):
    passed = 0
    for row, source in load_examples():
        if row["status"] == "planned":
            print(f"planned: {row['id']} (not executed)")
            continue
        with tempfile.TemporaryDirectory(prefix="koven-tutorial-") as temporary:
            directory = Path(temporary)
            (directory / "source.ko").write_text(source)
            build = [str(cli), "build", "source.ko", "-o", "program"]
            if row["status"] == "diagnostic":
                build.insert(1, "--message-format=json")
            assert_output(build, row["build"], directory)
            if row["status"] == "diagnostic":
                if {p.name for p in directory.iterdir()} != {"source.ko"}:
                    raise AssertionError("failed tutorial build left artifacts")
            else:
                assert_output([str(directory / "program"), *row["args"]], row["artifact"], directory)
                assert_output([str(cli), "run", "source.ko", "--", *row["args"]], row["run"], directory)
            passed += 1
            print(f"passed: {row['id']}")
    print(f"tutorial: {passed} executed contracts; 1 planned")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/kovenc")
    check(parser.parse_args().cli.resolve())
