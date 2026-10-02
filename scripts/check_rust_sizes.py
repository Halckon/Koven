#!/usr/bin/env python3
"""Ratchet handwritten Rust physical lines against an explicit Git base."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path, PurePosixPath

LIMIT = 1000
POLICY = "scripts/rust-size-policy.json"


class PolicyError(ValueError):
    """An invalid policy or unavailable comparison must fail closed."""


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(["git", "-C", str(root), *args], capture_output=True)
    if result.returncode:
        raise PolicyError(result.stderr.decode("utf-8", errors="replace").strip())
    return result.stdout


def physical_lines(content: bytes) -> int:
    """Count LF-delimited physical lines, including a nonempty final fragment."""
    return content.count(b"\n") + int(bool(content) and not content.endswith(b"\n"))


def comparison_base(root: Path, reference: str) -> str:
    requested = git(root, "rev-parse", "--verify", "--end-of-options",
                    f"{reference}^{{commit}}").decode().strip()
    bases = git(root, "merge-base", "--all", "HEAD", requested).decode().splitlines()
    if len(bases) != 1:
        raise PolicyError("comparison requires exactly one merge-base; fetch complete history")
    return bases[0]


def tree_blobs(root: Path, commit: str) -> dict[str, tuple[str, str]]:
    entries = {}
    for entry in git(root, "ls-tree", "-rz", "--full-tree", commit).split(b"\0"):
        if entry:
            metadata, raw_path = entry.split(b"\t", 1)
            mode, kind, oid = metadata.decode().split()
            if kind == "blob":
                entries[raw_path.decode()] = (mode, oid)
    return entries


def read_blob(root: Path, oid: str) -> bytes:
    return git(root, "cat-file", "blob", oid)


def reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise PolicyError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def valid_path(path: str) -> bool:
    parsed = PurePosixPath(path)
    return (bool(path) and not parsed.is_absolute() and parsed.as_posix() == path
            and ".." not in parsed.parts and "\\" not in path
            and not any(char in path for char in "*?[]") and path.endswith(".rs"))


def parse_policy(content: bytes) -> dict:
    policy = json.loads(content, object_pairs_hook=reject_duplicate_keys)
    if not isinstance(policy, dict) or set(policy) != {"version", "baseline", "exceptions", "generated"}:
        raise PolicyError("policy requires version, baseline, exceptions and generated only")
    if type(policy["version"]) is not int or policy["version"] != 1:
        raise PolicyError("unsupported policy version")
    for section in ("baseline", "exceptions", "generated"):
        if not isinstance(policy[section], dict):
            raise PolicyError(f"{section} must be an object keyed by exact Rust paths")
        for path, entry in policy[section].items():
            if not valid_path(path):
                raise PolicyError(f"{section}: invalid exact Rust path {path!r}")
            if section == "baseline":
                if type(entry) is not int or entry <= LIMIT:
                    raise PolicyError(f"baseline {path}: count must exceed {LIMIT}")
                continue
            fields = ({"max_lines", "reason", "owner", "split_plan", "review"}
                      if section == "exceptions" else {"inputs", "generator", "version"})
            if not isinstance(entry, dict) or set(entry) != fields:
                raise PolicyError(f"{section} {path}: requires {sorted(fields)}")
            for field in sorted(fields - {"max_lines"}):
                if not isinstance(entry[field], str) or not entry[field].strip():
                    raise PolicyError(f"{section} {path}: {field} must be nonempty")
            if section == "exceptions" and (type(entry["max_lines"]) is not int or entry["max_lines"] <= LIMIT):
                raise PolicyError(f"exceptions {path}: max_lines must exceed {LIMIT}")
    overlap = set(policy["generated"]) & (set(policy["baseline"]) | set(policy["exceptions"]))
    if overlap:
        raise PolicyError(f"generated paths cannot also be handwritten allowances: {sorted(overlap)}")
    return policy


def working_sizes(root: Path) -> dict[str, int]:
    paths = set(git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z").decode().split("\0"))
    sizes = {}
    for path in sorted(paths):
        if not path.endswith(".rs"):
            continue
        source = root / path
        if source.is_symlink():
            raise PolicyError(f"Rust symlink is not a supported handwritten source: {path}")
        if not source.exists():  # Tracked deletion is not a size violation.
            continue
        if not source.is_file() or not source.resolve().is_relative_to(root.resolve()):
            raise PolicyError(f"Rust source must be a regular file inside the repository: {path}")
        sizes[path] = physical_lines(source.read_bytes())
    return sizes


def renames(root: Path, base: str) -> dict[str, str]:
    """Only Git's detected renames inherit identity; copies never do."""
    fields = git(root, "-c", "diff.renameLimit=0", "diff", "--name-status", "-z",
                 "--find-renames=50%", base, "--").decode().split("\0")
    mapping = {}
    index = 0
    while index < len(fields) and fields[index]:
        status = fields[index]
        index += 1
        source = fields[index]
        index += 1
        if status.startswith(("R", "C")):
            destination = fields[index]
            index += 1
            if status.startswith("R"):
                mapping[destination] = source
    return mapping


def check(root: Path, reference: str) -> tuple[list[str], list[str]]:
    base = comparison_base(root, reference)
    blobs = tree_blobs(root, base)
    policy = parse_policy((root / POLICY).read_bytes())
    previous = parse_policy(read_blob(root, blobs[POLICY][1])) if POLICY in blobs else None
    sizes = working_sizes(root)
    moved = renames(root, base)
    base_sizes = {}
    for path, (mode, oid) in blobs.items():
        if path.endswith(".rs"):
            if mode not in ("100644", "100755"):
                raise PolicyError(f"base Rust source is not a regular file: {path}")
            base_sizes[path] = physical_lines(read_blob(root, oid))
    errors = []
    baseline = policy["baseline"]
    if previous is None:
        expected = {path: lines for path, lines in base_sizes.items()
                    if lines > LIMIT and path not in policy["generated"]}
        if baseline != expected:
            errors.append("initial baseline must exactly match oversized handwritten files at the comparison base")
    else:
        for path, lines in baseline.items():
            original = moved.get(path, path)
            if lines > previous["baseline"].get(original, 0):
                errors.append(f"{path}: baseline cannot add/increase an allowance; use an explicit exception")
    for section in ("exceptions", "generated"):
        for path in policy[section]:
            if path not in sizes:
                errors.append(f"{section} {path}: stale path; remove or update the registry entry")
    report = [f"Rust size guard: base {base}; limit {LIMIT} physical lines"]
    handwritten = {path: lines for path, lines in sizes.items() if path not in policy["generated"]}
    oversized = 0
    for path, lines in handwritten.items():
        if lines <= LIMIT:
            continue
        oversized += 1
        old_path = moved.get(path, path)
        old_lines = base_sizes.get(old_path, 0)
        inherited = min(baseline.get(path, 0), old_lines)
        exception = policy["exceptions"].get(path)
        if exception and lines <= exception["max_lines"]:
            report.append(f"EXCEPTION {lines:5d} {path} (owner: {exception['owner']}; review: {exception['review']})")
        elif lines <= inherited:
            report.append(f"BASELINE  {lines:5d} {path} (base: {old_lines})")
        else:
            allowed = max(LIMIT, inherited, exception["max_lines"] if exception else 0)
            errors.append(f"{path}: {lines} lines exceeds allowed {allowed}"
                          f" (base: {old_lines}); add/update a bounded exception with reason, owner, split_plan and review")
    for path, entry in policy["generated"].items():
        if path in sizes:
            report.append(f"GENERATED {sizes[path]:5d} {path} ({entry['generator']} {entry['version']}; inputs: {entry['inputs']})")
    report.append(f"{len(handwritten)} handwritten Rust files; {oversized} over {LIMIT}; "
                  f"{len(policy['generated'])} explicitly registered generated files")
    return errors, report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="explicit Git ref; compare its unique merge-base with HEAD")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        errors, report = check(args.root, args.base)
    except (PolicyError, OSError, ValueError, KeyError) as error:
        print(f"Rust size guard error: {error}", file=sys.stderr)
        return 1
    print("\n".join(report))
    if errors:
        print("\n".join(f"ERROR {error}" for error in errors), file=sys.stderr)
        return 1
    print("Rust size guard passed; historical debt is reported, not silently cleared.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
