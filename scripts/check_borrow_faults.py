#!/usr/bin/env python3
"""Run only inventoried borrow fault tests, with exact discovery and CI-only execution."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SCOPES = {
    "lang-frontend": ("ownership_checking::",),
    "lang-codegen": ("ssa::refine::",),
}
FQN = re.compile(r"[A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)+")


def in_scope(package, name):
    return name.startswith(SCOPES[package])


def load_manifest(path):
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate manifest package: {key}")
            result[key] = value
        return result

    manifest = json.loads(Path(path).read_text(), object_pairs_hook=unique_object)
    if not isinstance(manifest, dict) or set(manifest) != set(SCOPES):
        raise ValueError("manifest must contain exactly lang-frontend and lang-codegen")
    for package, names in manifest.items():
        if not isinstance(names, list) or not names:
            raise ValueError(f"{package}: nonempty test list required")
        if any(not isinstance(n, str) or not FQN.fullmatch(n)
               or not in_scope(package, n) for n in names):
            raise ValueError(f"{package}: invalid or out-of-scope test FQN")
        if len(names) != len(set(names)):
            raise ValueError(f"{package}: duplicate test FQN")
    return manifest


def cargo_base(package):
    args = ["cargo", "test", "--locked", "-p", package, "--lib"]
    if package == "lang-codegen":
        args += ["--features", "loan-observation-test-support"]
    return args


def parse_list(output):
    names = re.findall(r"^([A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)+): test$",
                       output, re.MULTILINE)
    totals = re.findall(r"^(\d+) tests?, (\d+) benchmarks?$", output, re.MULTILINE)
    if len(totals) != 1 or int(totals[0][0]) != len(names) or int(totals[0][1]):
        raise ValueError("malformed or incomplete libtest listing")
    if len(names) != len(set(names)):
        raise ValueError("duplicate discovered test")
    return set(names)


def check_result(output, name):
    running = re.findall(r"^running (\d+) tests?$", output, re.MULTILINE)
    results = re.findall(
        r"^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
        r"(\d+) measured; (\d+) filtered out; finished in .+$", output, re.MULTILINE)
    passed = re.findall(r"^test (\S+) \.\.\. ok$", output, re.MULTILINE)
    if (running != ["1"] or len(results) != 1
            or results[0][:5] != ("ok", "1", "0", "0", "0") or passed != [name]):
        raise ValueError(f"{name}: expected exactly one executed passing test, ignored=0")


def invoke(args, runner):
    result = runner(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE, check=False)
    if result.returncode:
        raise ValueError(f"command failed ({result.returncode}): {' '.join(args)}\n"
                         f"{result.stdout}\n{result.stderr}")
    return result.stdout


def run(manifest, list_only=False, env=None, runner=subprocess.run):
    env = os.environ if env is None else env
    if not list_only and (env.get("CI") != "true" or env.get("GITHUB_ACTIONS") != "true"):
        raise ValueError("fault execution requires CI=true and GITHUB_ACTIONS=true; use --list-only locally")
    # Discover and verify both packages before executing any fault.
    for package, expected in manifest.items():
        output = invoke(cargo_base(package) + ["--", "--list", "--ignored"], runner)
        discovered = {n for n in parse_list(output) if in_scope(package, n)}
        if not discovered or discovered != set(expected):
            raise ValueError(f"{package}: ignored test inventory mismatch; "
                             f"missing={sorted(set(expected) - discovered)}, "
                             f"unlisted={sorted(discovered - set(expected))}")
    if not list_only:
        for package, names in manifest.items():
            for name in names:
                output = invoke(cargo_base(package) + ["--", name, "--exact", "--ignored",
                                                       "--format", "pretty"], runner)
                check_result(output, name)
                print(f"{package}/{name} executed=1 passed=1 ignored=0", flush=True)
    return sum(map(len, manifest.values()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=ROOT / "scripts/borrow_fault_tests.json")
    parser.add_argument("--list-only", action="store_true", help="compile/list only; never execute tests")
    args = parser.parse_args()
    try:
        count = run(load_manifest(args.manifest), args.list_only)
    except (ValueError, OSError) as error:
        print(f"borrow fault gate: {error}", file=sys.stderr)
        return 1
    print(f"Borrow fault inventory verified: {count}; "
          + ("listed only, none executed" if args.list_only else "all executed exactly once"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
