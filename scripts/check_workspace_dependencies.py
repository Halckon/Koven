#!/usr/bin/env python3
"""Check only direct declared workspace dependencies from Cargo metadata."""

import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MEMBERS = frozenset({"lang-frontend", "lang-codegen", "lang-cli", "lang-lsp", "lang-std"})
EDGES = frozenset({("lang-codegen", "lang-frontend"), ("lang-cli", "lang-codegen"),
                   ("lang-cli", "lang-frontend"), ("lang-lsp", "lang-frontend")})


def check_metadata(metadata):
    errors = []
    ids = metadata["workspace_members"]
    packages = [p for p in metadata["packages"] if p["id"] in ids]
    names = [p["name"] for p in packages]
    if len(ids) != 5 or len(names) != 5 or set(names) != MEMBERS:
        errors.append("workspace must contain exactly the five approved members")
    paths = {}
    for package in packages:
        try:
            path = Path(package["manifest_path"]).resolve(strict=True)
            if path.name != "Cargo.toml" or not path.is_file():
                raise ValueError("not a manifest file")
            paths[path.parent] = package["name"]
        except (OSError, ValueError) as error:
            errors.append(f"{package['name']}: invalid manifest path: {error}")
    found = set()
    for package in packages:
        for dep in package["dependencies"]:
            path = dep.get("path")
            if path is None:
                if dep["name"] in MEMBERS:
                    errors.append(f"{package['name']}: internal dependency must use member path")
                continue
            try:
                target = paths.get(Path(path).resolve(strict=True))
            except (OSError, ValueError) as error:
                errors.append(f"{package['name']}: invalid dependency path: {error}")
                continue
            if target is None or dep["name"] != target:
                errors.append(f"{package['name']}: non-member or mismatched path dependency {dep['name']}")
                continue
            edge = (package["name"], target)
            if edge not in EDGES:
                errors.append(f"forbidden direct edge: {edge}")
            if edge in found:
                errors.append(f"duplicate direct declaration: {edge}")
            found.add(edge)
    for edge in sorted(EDGES - found):
        errors.append(f"missing direct edge: {edge}")
    return errors


if __name__ == "__main__":
    result = subprocess.run(["cargo", "metadata", "--locked", "--offline", "--no-deps",
                             "--format-version", "1"], cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        raise SystemExit(result.stderr)
    errors = check_metadata(json.loads(result.stdout))
    if errors:
        raise SystemExit("\n".join(errors))
    print("Direct declarations: 5 members, 4 allowed edges; transitive/patch/config/lock freshness not checked.")
