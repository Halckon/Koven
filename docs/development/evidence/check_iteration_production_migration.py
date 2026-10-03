#!/usr/bin/env python3
"""Read-only verification of the iteration production responsibility migration."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

BASE = "6cc87fefe7d6486912d91966f751256ca256c0a0"
SOURCE = Path("crates/lang-frontend/src/ownership_checking/checker/drop_planner/iteration.rs")
FILES = (
    SOURCE,
    SOURCE.with_suffix("") / "capture_graph.rs",
    SOURCE.with_suffix("") / "phi_state.rs",
    SOURCE.with_suffix("") / "phi_incoming.rs",
)
SIDECAR = Path(__file__).with_name("iteration-production-migration.json")

# These declarations moved from iteration into its private children. pub(super)
# restores their old effective iteration-only scope, without exposing crate APIs.
VISIBLE_FUNCTIONS = {
    "reachable", "capture_layout", "register_capture_layout", "published", "insert",
    "expand", "recursive_origin", "nodes_reaching_cycle", "owned_nodes_reaching_cycle",
    "coexisting_owned_roots", "conditional_nested_nodes", "cyclic_node",
    "finite_layout_order", "preallocate_closure_phis", "seed_phi_state",
    "exit_binding_required", "phi_selector_writes", "coexisting_capture_node",
    "record_phi_incoming", "record_exhaustion_incoming",
}
VISIBLE_STRUCT_FIELDS = {
    "PhiCaptureGraph": ("nodes", "by_closure"),
    "PhiCaptureNode": ("closure", "release_captures", "sources", "opaque_sources"),
}
# rustfmt wraps only these newly widened signatures and adds their optional
# final parameter comma. No comma in a body or expression is normalized.
WRAPPED_PARAMETERS = {
    "capture_layout", "coexisting_owned_roots", "record_exhaustion_incoming",
}
LEX = re.compile(
    r'\s+|//[^\n]*|/\*[\s\S]*?\*/|(?:b|c)?r(?P<hashes>\#*)"[\s\S]*?"(?P=hashes)'
    r'|(?:b|c)?"(?:\\.|[^"\\])*"|b?\'(?:\\.|[^\'\\])\''
    r'|[A-Za-z_][A-Za-z_0-9]*|[0-9]+|.',
    re.S,
)
DECLARATION = re.compile(
    r"(?m)^(?P<indent> *)(?P<visibility>pub\(super\) )?"
    r"(?P<kind>fn|struct|type) (?P<name>\w+)"
)


class MigrationError(Exception):
    pass


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise MigrationError(detail)


def repository_root() -> Path:
    for start in (Path(__file__).resolve().parent, Path.cwd().resolve()):
        for candidate in (start, *start.parents):
            if (candidate / SOURCE).is_file() and (candidate / ".git").exists():
                return candidate
    raise MigrationError("Cannot locate the repository from the script location or cwd")


def git(root: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=root)


def digest(value: str | bytes) -> str:
    return hashlib.sha256(value.encode() if isinstance(value, str) else value).hexdigest()


def tokens(source: str) -> list[tuple[str, int, int]]:
    # Preserve literal/comment contents and path tokens exactly. Whitespace
    # outside those tokens is the only ordinary formatting discarded here.
    return [
        (match.group(), match.start(), match.end())
        for match in LEX.finditer(source)
        if not match.group().isspace()
    ]


def parameter_end(values: list[str]) -> int | None:
    if "fn" not in values:
        return None
    function = values.index("fn")
    if values[function + 1] not in WRAPPED_PARAMETERS:
        return None
    start = values.index("(", function + 2)
    depth = 0
    for index in range(start, len(values)):
        if values[index] == "(":
            depth += 1
        elif values[index] == ")":
            depth -= 1
            if depth == 0:
                return index
    raise MigrationError("Unclosed function parameter list")


def final_parameter_comma(source: str) -> bool:
    values = [value for value, _, _ in tokens(source)]
    end = parameter_end(values)
    return end is not None and values[end - 1] == ","


def normalized(source: str) -> bytes:
    values = [value for value, _, _ in tokens(source)]
    end = parameter_end(values)
    if end is not None and values[end - 1] == ",":
        del values[end - 1]
    result = []
    index = 0
    while index < len(values):
        if values[index:index + 4] == ["pub", "(", "super", ")"]:
            index += 4
        else:
            result.append(values[index])
            index += 1
    return json.dumps(result, ensure_ascii=False, separators=(",", ":")).encode()


def items(source: str) -> dict:
    found = {}
    source_tokens = tokens(source)
    token_starts = {position: index for index, (_, position, _) in enumerate(source_tokens)}
    for match in DECLARATION.finditer(source):
        start = match.start()
        # Retain the complete pre-existing attribute and documentation block.
        while start > 0:
            previous_end = start - 1
            previous_start = source.rfind("\n", 0, previous_end) + 1
            previous = source[previous_start:previous_end].lstrip()
            if previous.startswith("///") or previous.startswith("#["):
                start = previous_start
            else:
                break
        keyword = source.find(match.group("kind"), match.start())
        token_index = token_starts[keyword]
        if match.group("kind") == "type":
            end = next(end for value, _, end in source_tokens[token_index:] if value == ";")
            body = None
        else:
            opening = next(
                index for index in range(token_index, len(source_tokens))
                if source_tokens[index][0] == "{"
            )
            body_start = source_tokens[opening][1]
            depth = 0
            for value, _, end in source_tokens[opening:]:
                if value == "{":
                    depth += 1
                elif value == "}":
                    depth -= 1
                if depth == 0:
                    break
            require(depth == 0, f"Unclosed body: {match.group('name')}")
            body = source[body_start:end]
        name = match.group("kind") + " " + match.group("name")
        require(name not in found, f"Duplicate item: {name}")
        visibility = {name: "pub(super)" if match.group("visibility") else "private"}
        if match.group("kind") == "struct":
            for field in re.finditer(r"(?m)^    (pub\(super\) )?(\w+):", body):
                visibility[f"{name}.{field.group(2)}"] = (
                    "pub(super)" if field.group(1) else "private"
                )
        found[name] = {
            "text": source[start:end],
            "body": body,
            "line": source.count("\n", 0, start) + 1,
            "visibility": visibility,
        }
    return found


def build_report(root: Path) -> dict:
    base_bytes = git(root, "show", f"{BASE}:{SOURCE}")
    before = items(base_bytes.decode())
    after = {}
    current_files = []
    for path in FILES:
        contents = (root / path).read_bytes()
        current_files.append({
            "path": str(path), "sha256": digest(contents),
            "physical_lines": len(contents.decode().splitlines()),
        })
        for name, item in items(contents.decode()).items():
            require(name not in after, f"Duplicate relocated item: {name}")
            after[name] = dict(item, path=str(path))
    require(set(before) == set(after), "Function/type/alias inventory changed")
    require(len(before) == 45, "Expected 45 complete function/type/alias blocks")
    require(sum(name.startswith("fn ") for name in before) == 38, "Expected 38 functions")
    rows = []
    changes = []
    comma_changes = []
    for name, old in before.items():
        new = after[name]
        require(old["visibility"].keys() == new["visibility"].keys(), f"Declarations changed: {name}")
        for declaration, visibility in old["visibility"].items():
            if visibility != new["visibility"][declaration]:
                changes.append({
                    "declaration": declaration,
                    "before": visibility, "after": new["visibility"][declaration],
                    "effective_scope_before": "iteration and descendants",
                    "effective_scope_after": "iteration and descendants",
                })
        old_comma = final_parameter_comma(old["text"])
        new_comma = final_parameter_comma(new["text"])
        if old_comma != new_comma:
            comma_changes.append({"item": name, "before": old_comma, "after": new_comma})
        require(normalized(old["text"]) == normalized(new["text"]), f"Item tokens changed: {name}")
        if name.startswith("fn "):
            require(old["body"] == new["body"], f"Function body bytes changed: {name}")
        rows.append({
            "item": name,
            "base_path": str(SOURCE), "base_line": old["line"],
            "after_path": new["path"], "after_line": new["line"],
            "base_raw_sha256": digest(old["text"]), "after_raw_sha256": digest(new["text"]),
            "base_normalized_sha256": digest(normalized(old["text"])),
            "after_normalized_sha256": digest(normalized(new["text"])),
            "body_byte_identical": old["body"] == new["body"],
            "base_body_sha256": digest(old["body"]) if old["body"] is not None else None,
            "after_body_sha256": digest(new["body"]) if new["body"] is not None else None,
        })
    allowed = {f"fn {name}" for name in VISIBLE_FUNCTIONS}
    for name, fields in VISIBLE_STRUCT_FIELDS.items():
        allowed.add(f"struct {name}")
        allowed.update(f"struct {name}.{field}" for field in fields)
    require({row["declaration"] for row in changes} == allowed, "Unexpected visibility changes")
    require(
        all(row["before"] == "private" and row["after"] == "pub(super)" for row in changes),
        "Only the enumerated private-to-pub(super) moves are permitted",
    )
    require(
        {row["item"] for row in comma_changes} == {f"fn {name}" for name in WRAPPED_PARAMETERS}
        and all(not row["before"] and row["after"] for row in comma_changes),
        "Unexpected function parameter formatting changes",
    )
    test_paths = git(root, "ls-tree", "-r", "--name-only", BASE, "--", str(SOURCE.with_suffix("")))
    tests = []
    for path in test_paths.decode().splitlines():
        old = git(root, "show", f"{BASE}:{path}")
        new = (root / path).read_bytes()
        require(old == new, f"Existing test/replay file changed: {path}")
        tests.append({"path": path, "sha256": digest(new)})
    require(len(tests) == 30, "Expected 30 unchanged test/replay files")
    return {
        "base": BASE, "base_source_sha256": digest(base_bytes),
        "normalization": (
            "Preserve all literal/comment/path/algorithm tokens. Omit whitespace outside tokens, "
            "the explicitly enumerated pub(super) visibility changes, and only the optional final "
            "parameter commas of the three explicitly enumerated rustfmt-wrapped signatures. "
            "Additionally compare all 38 complete function bodies byte-for-byte."
        ),
        "current_files": current_files,
        "visibility_changes": changes,
        "signature_final_comma_changes": comma_changes,
        "items": rows,
        "unchanged_test_and_replay_files": tests,
        "relative_path_resolution": [
            {
                "path": "super::origins::CapturedOrigins",
                "before": "drop_planner::origins::CapturedOrigins",
                "after": "iteration::origins private import -> drop_planner::origins::CapturedOrigins",
            },
            {
                "path": "super::Checker",
                "before": "drop_planner::Checker private import -> checker::Checker",
                "after": "iteration::Checker private import -> drop_planner::Checker -> checker::Checker",
            },
            {
                "path": "super::super::Checker",
                "before": "checker::Checker",
                "after": "drop_planner::Checker private import -> checker::Checker",
            },
        ],
    }


def main() -> int:
    try:
        report = build_report(repository_root())
        expected = json.loads(SIDECAR.read_text())
        require(report == expected, f"Computed evidence does not match {SIDECAR.name}")
    except (MigrationError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    print(
        "PASS: 38 byte-identical function bodies; 45 equivalent complete item blocks; "
        "28 enumerated visibility changes; 30 unchanged test/replay files; sidecar matches"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
