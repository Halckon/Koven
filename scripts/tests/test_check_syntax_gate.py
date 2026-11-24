"""Unit tests for SPEC-0494 (scripts/check_syntax_gate.py)."""

from pathlib import Path
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
2. **纯 CFG 与冲突变化**：离线参考文法增量 0 个新 shift/reduce 冲突。
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

    def test_complete_proposal_passes(self):
        errors = check_syntax_gate.check_proposal_text(COMPLETE_PROPOSAL, "sample.md", strict=False)
        self.assertEqual(errors, [])

    def test_non_syntax_spec_na_passes(self):
        doc = "# Backend Optimization Spec\n\n## 表面语法增量门禁\n\n不适用（N/A）：本 Spec 不涉及表面语法变更。\n"
        self.assertEqual(check_syntax_gate.check_proposal_text(doc, "backend.md", strict=False), [])

    def test_missing_each_required_field_is_rejected(self):
        lines = COMPLETE_PROPOSAL.splitlines()
        for idx, line in enumerate(lines):
            if line.startswith(("1. ", "2. ", "3. ", "4. ", "5. ", "6. ")):
                mutated = "\n".join(lines[:idx] + lines[idx + 1 :])
                with self.subTest(omitted=line[:20]):
                    errors = check_syntax_gate.check_proposal_text(mutated, "mutated.md", strict=False)
                    self.assertTrue(errors, f"expected failure when omitting: {line}")

    def test_placeholder_markers_are_rejected(self):
        for marker in ("TBD", "TODO", "[待填]"):
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
