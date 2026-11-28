"""Unit tests for SPEC-0494 (scripts/check_syntax_gate.py)."""

from pathlib import Path
import shutil
import sys
import tempfile
import unittest

SCRIPTS_DIR = Path(__file__).resolve().parents[1]
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import check_syntax_gate  # noqa: E402

COMPLETE_PROPOSAL = """# Sample Surface Syntax Proposal

## 表面语法增量门禁

1. **受影响入口与规则 ID**：`parse_expression`（F3c 规则 `E-14`），对应 Guide 04。
2. **纯 CFG 与冲突变化**：离线参考文法增量 0 个新 shift/reduce 冲突，不得用词法吞换行掩盖。
3. **软关键字、词法与换行影响**：无新增软词，换行规则不变。
4. **恢复边界与资源合同影响（R1–R5）**：不改变 caller hard stop 与线性预算。
5. **正反例与边界见证矩阵**：已附合法、跨行、未闭合与互斥正反例。
6. **下游同步范围**：同步更新 Formatter（F4c）、F4a 语料与 Tree-sitter（F4b）。
"""


class SyntaxGateCheckerTests(unittest.TestCase):
    def test_repository_f5_templates_pass(self):
        errors = check_syntax_gate.check_repository_templates()
        self.assertEqual(errors, [])
        self.assertEqual(check_syntax_gate.main(["--self-check-template"]), 0)

    def test_template_self_check_rejects_missing_spec_acceptance_item(self):
        with tempfile.TemporaryDirectory(prefix="koven-f5-tpl-") as tmpdir:
            root = Path(tmpdir)
            for rel in (
                "docs/proposals/README.md",
                "docs/specs/TEMPLATE.md",
                "docs/proposals/parser-formal-verification/syntax-gate-checklist.md",
            ):
                dst = root / rel
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(check_syntax_gate.REPO_ROOT / rel, dst)
            tpl = root / "docs/specs/TEMPLATE.md"
            mutated = "\n".join(
                line for line in tpl.read_text(encoding="utf-8").splitlines()
                if not line.startswith("- [ ] 表面语法增量门禁")
            ) + "\n备注：仅提及表面语法四字但无验收勾选框。\n"
            tpl.write_text(mutated, encoding="utf-8")
            errors = check_syntax_gate.check_repository_templates(root)
            self.assertTrue(any("TEMPLATE.md" in err for err in errors), errors)

    def test_complete_proposal_passes(self):
        errors = check_syntax_gate.check_proposal_text(COMPLETE_PROPOSAL, "sample.md", strict=False)
        self.assertEqual(errors, [])

    def test_non_syntax_spec_na_passes(self):
        doc = "# Backend Optimization Spec\n\n## 表面语法增量门禁\n\n不适用（N/A）：本 Spec 不涉及表面语法变更。\n"
        self.assertEqual(check_syntax_gate.check_proposal_text(doc, "backend.md", strict=False), [])

    def test_partial_na_with_placeholder_or_single_field_does_not_bypass_gate(self):
        doc = "# Partial Proposal\n\n## 表面语法增量门禁\n\n- 下游同步：N/A\n- 冲突变化：TODO\n"
        errors = check_syntax_gate.check_proposal_text(doc, "partial.md", strict=False)
        self.assertTrue(any("unresolved placeholder" in err for err in errors), errors)
        self.assertTrue(any("missing required F5 field" in err for err in errors), errors)

    def test_missing_each_required_field_is_rejected_even_with_cross_field_keywords(self):
        lines = COMPLETE_PROPOSAL.splitlines()
        for idx, line in enumerate(lines):
            if line.startswith(("1. ", "2. ", "3. ", "4. ", "5. ", "6. ")):
                mutated = "\n".join(lines[:idx] + lines[idx + 1 :])
                with self.subTest(omitted=line[:20]):
                    errors = check_syntax_gate.check_proposal_text(mutated, "mutated.md", strict=False)
                    self.assertTrue(errors, f"expected failure when omitting: {line}")

    def test_empty_field_headings_are_rejected(self):
        empty_doc = """## 表面语法增量门禁

1. **受影响入口与规则 ID**：
2. **纯 CFG 与冲突变化**：
3. **软关键字、词法与换行影响**：
4. **恢复边界与资源合同影响（R1–R5）**：
5. **正反例与边界见证矩阵**：
6. **下游同步范围**：
"""
        errors = check_syntax_gate.check_proposal_text(empty_doc, "empty.md", strict=False)
        self.assertEqual(len(errors), 6, errors)
        self.assertTrue(all("empty content" in err for err in errors), errors)

    def test_placeholder_markers_including_cjk_adjacent_are_rejected(self):
        for marker in ("TBD", "TODO", "[待填]", "TODO待确认", "请TODO", "待补充TBD"):
            with self.subTest(marker=marker):
                mutated = COMPLETE_PROPOSAL + f"\n补充说明：{marker}\n"
                errors = check_syntax_gate.check_proposal_text(mutated, "placeholder.md", strict=False)
                self.assertTrue(any("unresolved placeholder" in err for err in errors), errors)

    def test_cli_check_file_roundtrip(self):
        with tempfile.TemporaryDirectory(prefix="koven-f5-") as tmpdir:
            good = Path(tmpdir) / "good.md"
            bad = Path(tmpdir) / "bad.md"
            good.write_text(COMPLETE_PROPOSAL, encoding="utf-8")
            bad.write_text("## 表面语法增量门禁\n\n1. 受影响入口：TBD\n", encoding="utf-8")
            self.assertEqual(check_syntax_gate.main(["--check-file", str(good)]), 0)
            self.assertEqual(check_syntax_gate.main(["--check-file", str(bad)]), 1)


if __name__ == "__main__":
    unittest.main()
