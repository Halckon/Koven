#!/usr/bin/env python3
"""SPEC-0494 (F5): Executable structural checker for Koven surface-syntax gate proposals.

Verifies:
1. Repository F5 template integrity (`--self-check-template`): `docs/proposals/README.md`
   and `docs/specs/TEMPLATE.md` preserve all 6 mandatory F5 surface-syntax gate fields
   and the review checklist established by SPEC-0480.
2. Proposal / Spec document validation (`--check-file <path>`): any surface-syntax
   proposal or Spec carrying a `表面语法` gate section (or checked under `--strict`)
   must populate all 6 F5 fields without unresolved placeholders (`TBD`/`TODO`/`[待填]`).
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys

REPO_ROOT = Path(__file__).resolve().parents[1]

REQUIRED_FIELDS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("受影响入口与规则 ID", ("受影响入口", "规则 ID", "parse_file", "parse_expression", "parse_declaration", "parse_block")),
    ("纯 CFG 与冲突变化", ("纯 CFG", "冲突变化", "shift/reduce", "reduce/reduce", "冲突")),
    ("软关键字、词法与换行影响", ("软关键字", "软词", "换行", "词法")),
    ("恢复边界与资源合同影响（R1–R5）", ("恢复边界", "R1–R5", "R1-R5", "资源合同")),
    ("正反例与边界见证矩阵", ("正反例", "边界见证", "见证矩阵")),
    ("下游同步范围", ("下游同步", "Formatter", "Tree-sitter", "F4a", "F4b", "F4c")),
)

PLACEHOLDER_RE = re.compile(r"\b(?:TBD|TODO|FIXME)\b|\[待填\]|\[待补充\]", re.IGNORECASE)
GATE_HEADING_RE = re.compile(r"^#{2,3}\s+.*表面语法.*门禁.*$")
NA_MARKER_RE = re.compile(r"(?:不适用|N/A)", re.IGNORECASE)


def extract_gate_section(text: str) -> str | None:
    lines = text.splitlines()
    start_idx = None
    start_level = None
    for idx, line in enumerate(lines):
        if GATE_HEADING_RE.match(line.strip()):
            start_idx = idx
            start_level = len(line.lstrip()) - len(line.lstrip().lstrip("#"))
            break
    if start_idx is None:
        return None
    collected = [lines[start_idx]]
    for line in lines[start_idx + 1 :]:
        stripped = line.lstrip()
        if stripped.startswith("#"):
            level = len(stripped) - len(stripped.lstrip("#"))
            if level <= start_level and len(stripped) > level and stripped[level] == " ":
                break
        collected.append(line)
    return "\n".join(collected)


def check_proposal_text(text: str, label: str, strict: bool = False) -> list[str]:
    errors: list[str] = []
    section = extract_gate_section(text)
    if section is None:
        if not strict:
            return []
        section = text

    # Allow non-syntax Specs to explicitly declare the surface-syntax gate N/A.
    non_heading_lines = [
        line.strip()
        for line in section.splitlines()[1:]
        if line.strip() and not line.strip().startswith("#")
    ]
    if (
        not strict
        and len(non_heading_lines) <= 2
        and any(NA_MARKER_RE.search(line) for line in non_heading_lines)
    ):
        return []

    for field_name, keywords in REQUIRED_FIELDS:
        if not any(kw in section for kw in keywords):
            errors.append(f"{label}: missing required F5 field '{field_name}'")

    for idx, line in enumerate(section.splitlines(), start=1):
        match = PLACEHOLDER_RE.search(line)
        if match:
            errors.append(
                f"{label}: unresolved placeholder '{match.group(0)}' in F5 gate section (line {idx})"
            )

    return errors


def check_repository_templates(repo_root: Path = REPO_ROOT) -> list[str]:
    errors: list[str] = []
    proposals_readme = repo_root / "docs" / "proposals" / "README.md"
    spec_template = repo_root / "docs" / "specs" / "TEMPLATE.md"
    checklist = repo_root / "docs" / "proposals" / "parser-formal-verification" / "syntax-gate-checklist.md"

    for path in (proposals_readme, spec_template, checklist):
        if not path.is_file():
            errors.append(f"missing F5 template document: {path}")

    if errors:
        return errors

    proposals_text = proposals_readme.read_text(encoding="utf-8")
    if "表面语法提案增量门禁" not in proposals_text:
        errors.append(f"{proposals_readme}: missing '表面语法提案增量门禁' section")
    errors.extend(check_proposal_text(proposals_text, str(proposals_readme), strict=True))

    template_text = spec_template.read_text(encoding="utf-8")
    if "表面语法" not in template_text:
        errors.append(f"{spec_template}: missing surface-syntax gate reference in Spec template")

    checklist_text = checklist.read_text(encoding="utf-8")
    errors.extend(check_proposal_text(checklist_text, str(checklist), strict=True))
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Verify F5 surface-syntax incremental gate templates and proposals."
    )
    parser.add_argument(
        "--self-check-template",
        action="store_true",
        help="Verify docs/proposals/README.md, docs/specs/TEMPLATE.md, and syntax-gate-checklist.md.",
    )
    parser.add_argument(
        "--check-file",
        action="append",
        default=[],
        help="Markdown proposal or Spec file to validate against the 6 F5 gate fields.",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Require all 6 F5 fields even if the file lacks a dedicated gate heading.",
    )
    args = parser.parse_args(argv)

    run_template = args.self_check_template or not args.check_file
    errors: list[str] = []
    if run_template:
        errors.extend(check_repository_templates(REPO_ROOT))
    for raw_path in args.check_file:
        path = Path(raw_path)
        if not path.is_file():
            errors.append(f"{raw_path}: file not found")
            continue
        errors.extend(check_proposal_text(path.read_text(encoding="utf-8"), raw_path, strict=args.strict))

    if errors:
        for err in errors:
            print(f"ERROR: {err}", file=sys.stderr)
        return 1

    print("F5 surface-syntax gate check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
