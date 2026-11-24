#!/usr/bin/env python3
"""SPEC-0494 (F5): Executable structural checker for Koven surface-syntax gate proposals.

Verifies:
1. Repository F5 template integrity (`--self-check-template`): `docs/proposals/README.md`,
   `docs/specs/TEMPLATE.md`, and `docs/proposals/parser-formal-verification/syntax-gate-checklist.md`
   preserve all 6 mandatory F5 surface-syntax gate fields and acceptance checklist prompts.
2. Proposal / Spec document validation (`--check-file <path>`): any surface-syntax
   proposal or Spec carrying a `表面语法` gate section (or checked under `--strict`)
   must populate all 6 F5 fields in distinct, non-empty items without unresolved
   placeholders (`TBD`/`TODO`/`FIXME`/`[待填]`/`[待补充]`).
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys

REPO_ROOT = Path(__file__).resolve().parents[1]

# Each entry: (canonical field name, label-matching regex on the item heading/prefix)
REQUIRED_FIELDS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("受影响入口与规则 ID", re.compile(r"受影响入口|规则\s*ID")),
    ("纯 CFG 与冲突变化", re.compile(r"纯\s*CFG|冲突变化|冲突")),
    ("软关键字、词法与换行影响", re.compile(r"软关键字|软词|换行影响")),
    ("恢复边界与资源合同影响（R1–R5）", re.compile(r"恢复边界|资源合同|R1[–-]R5")),
    ("正反例与边界见证矩阵", re.compile(r"正反例|边界见证|见证矩阵")),
    ("下游同步范围", re.compile(r"下游同步")),
)

# Use ASCII word boundaries so CJK-adjacent placeholders like `TODO待确认` or `请TODO` are caught.
PLACEHOLDER_RE = re.compile(
    r"(?<![A-Za-z0-9_])(?:TBD|TODO|FIXME)(?![A-Za-z0-9_])|\[待填\]|\[待补充\]",
    re.IGNORECASE,
)
GATE_HEADING_RE = re.compile(r"^#{2,3}\s+.*表面语法.*门禁.*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*]|\d+\.)\s+(.*)$")
NA_STANDALONE_RE = re.compile(
    r"^(?:[-*]\s+)?(?:不适用|N/A)(?:[：:（(].*)?$",
    re.IGNORECASE,
)


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


def parse_field_entries(section: str) -> list[tuple[str, str]]:
    """Extract (label, body) pairs from numbered/bulleted list items or Markdown table rows."""
    entries: list[tuple[str, str]] = []
    lines = section.splitlines()
    i = 0
    while i < len(lines):
        raw = lines[i]
        stripped = raw.strip()
        # Markdown table row: | label | body |
        if stripped.startswith("|") and stripped.endswith("|"):
            cells = [c.strip() for c in stripped.strip("|").split("|")]
            if len(cells) >= 2 and not all(set(c) <= {"-", ":", " "} for c in cells):
                entries.append((cells[0], " ".join(cells[1:]).strip()))
            i += 1
            continue
        m = LIST_ITEM_RE.match(raw)
        if m:
            first_line = m.group(1).strip()
            # Collect indented continuation lines belonging to this list item.
            cont_lines: list[str] = []
            j = i + 1
            while j < len(lines):
                nxt = lines[j]
                if not nxt.strip() or nxt.lstrip().startswith("#") or LIST_ITEM_RE.match(nxt):
                    break
                if nxt.startswith((" ", "\t")):
                    cont_lines.append(nxt.strip())
                    j += 1
                else:
                    break
            full_item = " ".join([first_line, *cont_lines]).strip()
            # Split at the first colon (`：` or `:`) to separate field label from body.
            colon_match = re.search(r"[：:]", full_item)
            if colon_match:
                label_part = full_item[: colon_match.start()].strip(" *`")
                body_part = full_item[colon_match.end() :].strip(" *`")
            else:
                label_part = full_item.strip(" *`")
                body_part = ""
            entries.append((label_part, body_part))
            i = j
            continue
        i += 1
    return entries


def check_proposal_text(text: str, label: str, strict: bool = False) -> list[str]:
    errors: list[str] = []
    section = extract_gate_section(text)
    if section is None:
        if not strict:
            return []
        section = text

    # Always check placeholders across the entire gate section first (never bypassed by N/A).
    for idx, line in enumerate(section.splitlines(), start=1):
        match = PLACEHOLDER_RE.search(line)
        if match:
            errors.append(
                f"{label}: unresolved placeholder '{match.group(0)}' in F5 gate section (line {idx})"
            )

    # Allow non-syntax Specs to exempt the entire section ONLY when the section has no
    # unresolved placeholders, no numbered/field entries, and explicitly opens with N/A / 不适用.
    non_heading_lines = [
        line.strip()
        for line in section.splitlines()[1:]
        if line.strip() and not line.strip().startswith("#")
    ]
    if (
        not strict
        and not errors
        and len(non_heading_lines) == 1
        and NA_STANDALONE_RE.match(non_heading_lines[0])
    ):
        return []

    entries = parse_field_entries(section)
    used_indices: set[int] = set()
    for field_name, label_re in REQUIRED_FIELDS:
        matched_idx = None
        for idx, (entry_label, _body) in enumerate(entries):
            if idx in used_indices:
                continue
            if label_re.search(entry_label):
                matched_idx = idx
                break
        if matched_idx is None:
            errors.append(f"{label}: missing required F5 field '{field_name}'")
            continue
        used_indices.add(matched_idx)
        entry_label, entry_body = entries[matched_idx]
        if not entry_body:
            errors.append(
                f"{label}: F5 field '{field_name}' (matched '{entry_label}') has empty content"
            )

    return errors


def check_repository_templates(repo_root: Path = REPO_ROOT) -> list[str]:
    errors: list[str] = []
    proposals_readme = repo_root / "docs" / "proposals" / "README.md"
    spec_template = repo_root / "docs" / "specs" / "TEMPLATE.md"
    checklist = (
        repo_root
        / "docs"
        / "proposals"
        / "parser-formal-verification"
        / "syntax-gate-checklist.md"
    )

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
    template_item_re = re.compile(
        r"^- \[ \] 表面语法增量门禁.*规则归属.*冲突.*软词.*恢复边界.*正反例",
        re.MULTILINE,
    )
    if not template_item_re.search(template_text):
        errors.append(
            f"{spec_template}: missing F5 acceptance checklist item "
            "('- [ ] 表面语法增量门禁...')"
        )

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
        errors.extend(
            check_proposal_text(path.read_text(encoding="utf-8"), raw_path, strict=args.strict)
        )

    if errors:
        for err in errors:
            print(f"ERROR: {err}", file=sys.stderr)
        return 1

    print("F5 surface-syntax gate check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
