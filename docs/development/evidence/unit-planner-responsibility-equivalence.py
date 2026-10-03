#!/usr/bin/env python3
"""Read-only byte-level verifier for the bounded P2 unit planner relocation.

Run from the checkout, or keep this script and its companion JSON below its root.
The script reads Git and source files and prints a concise verification result. It
never writes files, invokes Cargo, stages changes, or accesses the network.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

BASE = "6cc87fefe7d6486912d91966f751256ca256c0a0"
ROOT_SOURCE = "crates/lang-codegen/src/ssa/unit_plan.rs"
ROOT_ANCHOR = "/// 防止 unit-wide 泛型实例图被合法但病态的源码无界扩张。"
CODEGEN_DIRECTORY = "crates/lang-codegen"


def digest(value: bytes | str) -> str:
    if isinstance(value, str):
        value = value.encode("utf-8")
    return hashlib.sha256(value).hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def repository_root() -> Path:
    for anchor in (Path.cwd(), Path(__file__).resolve().parent):
        result = subprocess.run(
            ["git", "-C", str(anchor), "rev-parse", "--show-toplevel"],
            text=True, capture_output=True, check=False,
        )
        if result.returncode == 0:
            candidate = Path(result.stdout.strip())
            if (candidate / ROOT_SOURCE).is_file():
                return candidate
    raise ValueError("Run from the Koven checkout or place this script below its repository root")


def git_bytes(repo: Path, *arguments: str) -> bytes:
    return subprocess.check_output(["git", "-C", str(repo), *arguments])


def source_text(repo: Path, relative: str) -> str:
    path = Path(relative)
    require(not path.is_absolute() and ".." not in path.parts, f"Non-relative evidence path: {relative}")
    return (repo / path).read_bytes().decode("utf-8")


def item(source: str, name: str, kind: str = "fn") -> dict:
    pattern = (
        r"^(?P<visibility>pub(?:\([^\n]*?\))? )?"
        + re.escape(kind) + r" " + re.escape(name) + r"\b"
    )
    matches = list(re.finditer(pattern, source, re.MULTILINE))
    require(len(matches) == 1, f"Expected exactly one top-level {kind} {name}")
    match = matches[0]
    start = match.start()
    while start > 0:
        previous = source.rfind("\n", 0, start - 1) + 1
        line = source[previous:start - 1]
        if line.startswith("///") or line.startswith("#["):
            start = previous
        else:
            break
    end_match = re.search(r"^}\n", source[match.end():], re.MULTILINE)
    require(end_match is not None, f"Missing complete item closing brace: {name}")
    end = match.end() + end_match.end()
    visibility = (match.group("visibility") or "").strip() or "private"
    return {
        "name": name, "kind": kind, "start": start, "end": end,
        "declaration_offset": match.start() - start,
        "visibility": visibility, "text": source[start:end],
    }


def declaration_prefix(visibility: str, kind: str, name: str) -> str:
    return ("" if visibility == "private" else visibility + " ") + kind + " " + name


def normalize_approved_visibility(actual: dict, record: dict) -> str:
    # Only this named declaration's explicitly recorded prefix can change.
    # No token-wide deletion, pub-token regex replacement, or attribute stripping.
    require(actual["visibility"] == record["new_visibility"], f"Unexpected new visibility: {record['name']}")
    before = declaration_prefix(record["old_visibility"], record["kind"], record["name"])
    after = declaration_prefix(record["new_visibility"], record["kind"], record["name"])
    offset = actual["declaration_offset"]
    text = actual["text"]
    require(text[offset:offset + len(after)] == after, f"Declaration prefix mismatch: {record['name']}")
    return text[:offset] + before + text[offset + len(after):]


def verify(repo: Path, expected: dict) -> dict:
    require(expected["base"] == BASE, "Companion JSON does not use the fixed reviewed baseline")
    before = git_bytes(repo, "show", f"{BASE}:{ROOT_SOURCE}").decode("utf-8")
    after = source_text(repo, ROOT_SOURCE)
    require(digest(before) == expected["source_before_sha256"], "Baseline source hash mismatch")
    require(before[:before.index(ROOT_ANCHOR)] == expected["root_wiring_before"], "Unexpected old root wiring")
    require(after[:after.index(ROOT_ANCHOR)] == expected["root_wiring_after"], "Unexpected new root wiring")

    source_files = []
    for entry in expected["source_files"]:
        text = source_text(repo, entry["path"])
        actual = {"path": entry["path"], "sha256": digest(text), "physical_lines": len(text.splitlines())}
        require(actual == entry, f"Frozen source file changed: {entry['path']}")
        source_files.append(actual)

    moved = []
    spans = []
    for record in expected["moved_items"]:
        original = item(before, record["name"], record["kind"])
        current = item(source_text(repo, record["destination"]), record["name"], record["kind"])
        require(original["visibility"] == record["old_visibility"], f"Unexpected old visibility: {record['name']}")
        normalized = normalize_approved_visibility(current, record)
        require(normalized == original["text"], f"Item body, documentation or attributes changed: {record['name']}")
        actual = {
            "name": record["name"], "kind": record["kind"], "destination": record["destination"],
            "old_visibility": original["visibility"], "new_visibility": current["visibility"],
            "original_start_line": before.count("\n", 0, original["start"]) + 1,
            "original_sha256": digest(original["text"]), "new_sha256": digest(current["text"]),
            "normalized_sha256": digest(normalized),
        }
        require(actual == record, f"Companion item record mismatch: {record['name']}")
        moved.append(actual)
        spans.append((original["start"], original["end"], normalized, record["name"]))
    require(len({entry["name"] for entry in moved}) == len(moved), "Duplicate moved item record")
    for path in {entry["destination"] for entry in moved}:
        expected_names = {entry["name"] for entry in moved if entry["destination"] == path}
        source = source_text(repo, path)
        actual_names = set(re.findall(r"^(?:pub(?:\([^\n]*?\))? )?(?:fn|enum) (\w+)", source, re.MULTILINE))
        require(actual_names == expected_names, f"Missing or extra top-level child item: {path}")

    # Reconstruct every original byte from actual current retained segments and
    # visibility-normalized complete moved items, including original separators.
    remaining = after[after.index(ROOT_ANCHOR):]
    header_end = before.index(ROOT_ANCHOR)
    restored = before[:header_end]
    cursor = header_end
    for start, end, normalized, name in sorted(spans):
        retained = before[cursor:start]
        require(remaining.startswith(retained), f"Retained root code changed before {name}")
        restored += remaining[:len(retained)] + normalized
        remaining = remaining[len(retained):]
        if before[end:end + 1] == "\n":
            restored += "\n"
            end += 1
        cursor = end
    require(remaining == before[cursor:], "Retained root tail changed")
    restored += remaining
    require(restored == before, "Full original-source inverse reconstruction failed")

    retained_functions = []
    names = re.findall(r"^(?:pub(?:\([^\n]*?\))? )?fn (\w+)", after, re.MULTILINE)
    for name in names:
        original = item(before, name)
        current = item(after, name)
        require(original["text"] == current["text"], f"Retained root function changed: {name}")
        retained_functions.append({"name": name, "visibility": original["visibility"], "sha256": digest(current["text"])})
    require(retained_functions == expected["retained_root_functions"], "Retained root function manifest mismatch")

    protected = []
    for entry in expected["protected_codegen_files"]:
        before_bytes = git_bytes(repo, "show", f"{BASE}:{entry['path']}")
        current_bytes = (repo / entry["path"]).read_bytes()
        actual = {"path": entry["path"], "sha256": digest(current_bytes)}
        require(before_bytes == current_bytes and actual == entry, f"Protected codegen file changed: {entry['path']}")
        protected.append(actual)
    base_paths = set(git_bytes(repo, "ls-tree", "-r", "--name-only", BASE, "--", CODEGEN_DIRECTORY).decode().splitlines())
    require(base_paths == {entry["path"] for entry in protected} | {ROOT_SOURCE}, "Protected baseline manifest is incomplete")
    current_paths = set(git_bytes(repo, "ls-files", "--cached", "--others", "--exclude-standard", "--", CODEGEN_DIRECTORY).decode().splitlines())
    require(current_paths == base_paths | {entry["path"] for entry in source_files}, "Unexpected new codegen file")

    test_names = []
    for path in sorted((repo / "crates/lang-codegen/src/ssa/unit_plan_tests").glob("*.rs")):
        for name in re.findall(r"^#\[test\]\nfn (\w+)", path.read_text(), re.MULTILINE):
            test_names.append(f"ssa::unit_plan_tests::{path.stem}::{name}")
    require(test_names == expected["planner_test_names"], "Planner test identity manifest changed")

    report = {
        "base": BASE, "scope": expected["scope"], "source_before_sha256": digest(before),
        "root_wiring_before": before[:before.index(ROOT_ANCHOR)],
        "root_wiring_after": after[:after.index(ROOT_ANCHOR)],
        "inverse_reconstruction_sha256": digest(restored), "inverse_reconstruction_byte_identical": True,
        "source_files": source_files, "moved_items": moved, "retained_root_functions": retained_functions,
        "protected_codegen_files": protected, "planner_test_names": test_names,
        "counts": {"moved_items": len(moved), "retained_root_functions": len(retained_functions),
                   "protected_codegen_files": len(protected), "planner_tests": len(test_names)},
        "approved_wiring_changes": expected["approved_wiring_changes"],
        "limitations": expected["limitations"],
    }
    require(report == expected, "Complete verification report differs from the companion JSON")
    return report


def main() -> int:
    try:
        evidence_path = Path(__file__).resolve().with_suffix(".json")
        expected = json.loads(evidence_path.read_text())
        report = verify(repository_root(), expected)
        print("PASS: 36 moved items and 10 retained root functions byte-identical; "
              "complete inverse reconstruction; 215 codegen files and 48 test identities unchanged; sidecar matches")
        return 0
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"unit planner relocation verification failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
