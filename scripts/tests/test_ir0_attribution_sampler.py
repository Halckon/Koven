"""P0-2：IR0 §1 所有权与降低修复历史归因统计（C1–C4）与止损门槛单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "ir0_attribution_sampler.py"
SPEC = importlib.util.spec_from_file_location("ir0_attribution_sampler", PATH)
ATTR = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = ATTR
SPEC.loader.exec_module(ATTR)

# 真实 main 采样窗口 92383a60..e428bc5d（800 commits，PR #441 -> PR #501）中的全部 56 条 IR0 范围内 fix 提交
REAL_WINDOW_56_COMMITS = (
    ("03a5b3f2", "fix(codegen): use pub(crate) on native_bounded::execute_binary for integration tests", ("crates/lang-codegen/src/ssa/native_bounded.rs",)),
    ("978a24f4", "fix(range): address PR #473 review P2 findings", ("crates/lang-frontend/src/ownership_checking/preflight.rs", "crates/lang-codegen/src/ssa/borrow_result_support.rs")),
    ("fec99e0d", "fix(frontend): lazily box EscapeWitnesses in dataflow State to avoid stack overflow", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/escape_witness.rs",)),
    ("618f56d9", "fix(frontend): address PR #451 P2 review findings on loop escape witness and coverage", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/escape_witness/loop_shadow.rs", "crates/lang-codegen/src/ssa/unit_field_replace_tests.rs")),
    ("2f162ddd", "fix(codegen): check singleton exits before admitting the call prefix", ("crates/lang-codegen/src/ssa/refine/subset/tagged.rs",)),
    ("92cb7fc4", "fix(codegen): unwind singleton arguments while preserving return loans", ("crates/lang-frontend/src/ownership_checking/lowering_facts.rs", "crates/lang-codegen/src/ssa/refine/subset/callable/unwind.rs")),
    ("46b348c3", "fix(codegen): merge tagged nullable floats on Compatibility paths", ("crates/lang-codegen/src/ssa/unit_lower/control.rs",)),
    ("35c2f53f", "fix(codegen): require a reached when for temporary iteration routing", ("crates/lang-codegen/src/ssa/refine/subset.rs", "crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs")),
    ("c077368b", "fix(codegen): keep float lowering out of neutral support", ("crates/lang-codegen/src/ssa/expression_core.rs", "crates/lang-codegen/src/ssa/refine/build/expression.rs", "crates/lang-codegen/src/ssa/unit_lower.rs")),
    ("1fa838ee", "fix(compiler): recover reviewed R2–R4 contracts and lowering", ("crates/lang-frontend/src/ownership_checking/lowering_facts.rs", "crates/lang-codegen/src/ssa/refine.rs", "crates/lang-codegen/src/ssa/unit_lower.rs")),
    ("aa7cc7a8", "fix: retain source origin for the checked ABI entry block", ("crates/lang-codegen/src/ssa/unit_lower/planned_declarations.rs",)),
    ("36f088c7", "fix: lend concrete closure for borrowed result auxiliary view", ("crates/lang-codegen/src/ssa/refine/build/callable/result_auxiliary.rs",)),
    ("93d070d4", "fix(codegen): admit one transparent group in terminal val", ("crates/lang-codegen/src/ssa/refine/subset/whole_nominal_exit.rs",)),
    ("6d203139", "fix(codegen): admit exact terminal Int val cleanup in refine", ("crates/lang-codegen/src/ssa/refine/subset/whole_nominal_exit.rs", "crates/lang-codegen/src/ssa/unit_lower/planned_body.rs")),
    ("3f1e8974", "fix(codegen): admit proven whole-temp borrow cleanup in a direct tail call", ("crates/lang-codegen/src/ssa/refine/subset/whole_nominal_exit.rs",)),
    ("89e473a9", "fix(frontend): preserve callable escape contracts and retire proven legacy consumers", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/closure.rs", "crates/lang-codegen/src/ssa/callback_replacement_ssa_contract.rs")),
    ("6573c9f3", "fix(lang-codegen): preserve unit_lower fallback lowering for rc, container size, and map", ("crates/lang-codegen/src/ssa/unit_lower.rs", "crates/lang-codegen/src/ssa/unit_lower/map.rs")),
    ("a64c99b6", "fix(lang-codegen): remove unused imports in unit_lower construction and container", ("crates/lang-codegen/src/ssa/unit_lower/construction.rs",)),
    ("4cbaf35f", "fix(compiler): close first-round layered review blockers", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/drop_planner/owner_transport.rs", "crates/lang-codegen/src/ssa/refine.rs", "crates/lang-codegen/src/ssa/verify_ownership.rs")),
    ("62ee41e5", "fix(lang-codegen): preserve refine route boundaries in CI", ("crates/lang-codegen/src/ssa/refine/subset.rs",)),
    ("79297893", "fix(lang-codegen): reconcile callable lowering with main", ("crates/lang-codegen/src/ssa/refine/subset/callable.rs",)),
    ("a072c390", "fix(frontend): index receiver activation witnesses once", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/drop_planner/receiver_loan_end.rs",)),
    ("7b8990ad", "fix(frontend): reject escaped nullable alias control results", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/non_null_assertion.rs",)),
    ("71fe9558", "fix(codegen): keep pure borrow-forward trees behind their receipt gates", ("crates/lang-codegen/src/ssa/provider/borrow_result.rs",)),
    ("c26047e6", "fix(codegen): lend disjoint MoveOnly fields at ordinary inout calls", ("crates/lang-codegen/src/ssa/refine/inout_field.rs", "crates/lang-codegen/src/ssa/verify_ownership/inout.rs")),
    ("51845889", "fix(ownership): keep large AST frames off short-circuit recursion", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs",)),
    ("0078eb0c", "fix(compiler): retire immediate lambda environments at call return", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/drop_planner/pending_call.rs", "crates/lang-codegen/src/ssa/refine/build/callable/invoke.rs")),
    ("3bcd444a", "fix(compiler): preserve nullable borrow and resource cleanup contracts", ("crates/lang-frontend/src/ownership_checking/lowering_facts.rs", "crates/lang-codegen/src/ssa/refine.rs", "crates/lang-codegen/src/ssa/verify_ownership.rs")),
    ("182a0a00", "fix(frontend): reject escaping Elvis construction arguments", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/construction.rs",)),
    ("44e93c05", "fix(frontend): retain immutable nullable alias escape witnesses", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/escape_witness.rs",)),
    ("90ebf74f", "fix(frontend): reject escaping Elvis Value arguments", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs",)),
    ("320b84df", "fix(frontend): check escaping nullable control extraction", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/control.rs",)),
    ("b93bd914", "fix(frontend): evaluate element assignment prefixes before rhs", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/assignment.rs",)),
    ("59c269e1", "fix(frontend): reject escaping Elvis fallback closure tails", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs",)),
    ("b3deb0e6", "fix(frontend): reject borrowed closure element storage", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/assignment.rs",)),
    ("15595b2f", "fix(frontend): diagnose unsupported nested Rc payload copies", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/rc.rs",)),
    ("f45d23fb", "fix(frontend): diagnose unsupported nested Rc sharing", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/rc.rs",)),
    ("2fd89ba1", "fix: roll back range uses with failed assignment loans", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/assignment.rs",)),
    ("a951dc6c", "fix: preserve actual range call identity in grouped iteration", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/iteration.rs", "crates/lang-codegen/src/ssa/refine_tests/grouped_range_iteration.rs")),
    ("5c140cbb", "fix: retain backing root protection for grouped range sources", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/call_contract.rs", "crates/lang-codegen/src/ssa/refine_tests/grouped_range_source.rs")),
    ("6964506c", "fix: preserve real range roots through grouped receivers", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/receiver.rs", "crates/lang-codegen/src/ssa/refine_tests/container_tests.rs")),
    ("c5ae9142", "fix: preserve actual range call identity through grouped arguments", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs", "crates/lang-codegen/src/ssa/refine_tests/container_tests.rs")),
    ("823df340", "fix: release interrupted range source loans on jump paths", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/traversal.rs",)),
    ("d16d7613", "fix(codegen): preserve String result loans in comparisons and when", ("crates/lang-codegen/src/ssa/provider/borrow_result.rs",)),
    ("b5262a0f", "fix: stop construction facts at unreachable operands", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/construction.rs",)),
    ("96de2e58", "fix: admit proven terminating class construction prefixes", ("crates/lang-codegen/src/ssa/refine/subset/heap.rs",)),
    ("7dc47f42", "fix: lower terminated construction field receivers as prefixes", ("crates/lang-codegen/src/ssa/refine/build/aggregate_value.rs", "crates/lang-codegen/src/ssa/refine/subset/heap/terminated_construction.rs")),
    ("61bbb425", "fix: lower non-dispatching factory field receivers as prefixes", ("crates/lang-codegen/src/ssa/refine/build/aggregate_value.rs", "crates/lang-codegen/src/ssa/refine/subset/heap/temporary_field.rs")),
    ("5fbf3a7d", "fix(codegen): repair post-merge refine visibility, imports and ordinary-lowering guard", ("crates/lang-codegen/src/ssa/refine/body.rs",)),
    ("dd3d2432", "fix: clean up owned factory results after copied field reads", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/drop_planner/field_read.rs", "crates/lang-codegen/src/ssa/refine/build/aggregate_value.rs")),
    ("4b689170", "fix: release temporary constructor parents after copied field reads", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/drop_planner/field_read.rs", "crates/lang-codegen/src/ssa/refine/build/aggregate_value.rs")),
    ("0ad4e5d4", "fix(frontend): reject consuming noncopyable fields of temporary owners", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/place_access.rs",)),
    ("13d91839", "fix(frontend): gate loop successor cleanup on reachable exits", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/drop_planner.rs",)),
    ("9d10fa43", "fix(frontend): roll back failed assignment borrow call uses", ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/assignment.rs",)),
    ("f00860fe", "fix(codegen): require final Unit types before E2 preparation", ("crates/lang-codegen/src/ssa/two_source_unit.rs",)),
    ("91657fc8", "fix(codegen): declare abort for borrowed Map require wrappers", ("crates/lang-codegen/src/ssa/verify_ownership/closure.rs",)),
)


class Ir0AttributionSamplerTests(unittest.TestCase):
    def test_parse_and_classify_c1_c2_c3_c4_and_filter_non_fix_or_unrelated(self):
        """只计入触及 ownership_checking 或 codegen/ssa 的 fix 提交，并互斥归入 C1–C4。"""
        log_text = """
COMMIT 1111111 fix(ssa): handle MissingFact on nullable enum drop
crates/lang-codegen/src/ssa/drop.rs

COMMIT 2222222 fix(codegen): lower UnsupportedNode for when subject
crates/lang-codegen/src/ssa/lower.rs

COMMIT 3333333 fix(ownership): align drop order between frontend and ssa
crates/lang-frontend/src/ownership_checking/drops.rs
crates/lang-codegen/src/ssa/drops.rs

COMMIT 4444444 fix(ownership): improve diagnostic span on move error
crates/lang-frontend/src/ownership_checking/diag.rs

COMMIT 5555555 feat(ssa): add new optimization pass
crates/lang-codegen/src/ssa/opt.rs

COMMIT 6666666 fix(docs): typo in readme
docs/README.md
"""
        records = ATTR.parse_git_log_text(log_text)
        self.assertEqual(6, len(records))
        summary = ATTR.summarize_attribution(records, stop_loss_threshold=0.5)
        self.assertEqual(4, summary.total_fix_commits)
        self.assertEqual({"C1": 1, "C2": 1, "C3": 1, "C4": 1}, summary.counts)
        self.assertAlmostEqual(0.75, summary.graph_solvable_ratio)
        self.assertEqual("PROCEED_IR1_ADR", summary.recommendation)

    def test_stop_loss_downgrades_when_c4_dominates(self):
        """当 C1+C2+C3 合计占比低于止损门槛时，建议降级为仅参考检查器使用。"""
        records = [
            ATTR.CommitRecord("aaa", "fix(ownership): tweak span offset", ("crates/lang-frontend/src/ownership_checking/a.rs",)),
            ATTR.CommitRecord("bbb", "fix(ssa): fix ABI register width", ("crates/lang-codegen/src/ssa/abi.rs",)),
            ATTR.CommitRecord("ccc", "fix(ssa): fix MissingFact on loop", ("crates/lang-codegen/src/ssa/loop.rs",)),
        ]
        summary = ATTR.summarize_attribution(records, stop_loss_threshold=0.5)
        self.assertEqual(3, summary.total_fix_commits)
        self.assertEqual({"C1": 0, "C2": 1, "C3": 0, "C4": 2}, summary.counts)
        self.assertAlmostEqual(1 / 3, summary.graph_solvable_ratio)
        self.assertEqual("DOWNGRADE_REFERENCE_ONLY", summary.recommendation)

    def test_path_aware_classification_catches_real_c2_c3_and_filters_test_only_c1(self):
        """路径特征可识别 lowering_facts/refine-subset/grouped/Elvis，且不会把仅触及测试侧的改动误判为 C1。"""
        c2_rec = ATTR.CommitRecord(
            "92cb7fc4",
            "fix(codegen): unwind singleton arguments while preserving return loans",
            ("crates/lang-frontend/src/ownership_checking/lowering_facts.rs", "crates/lang-codegen/src/ssa/refine/subset/callable/unwind.rs"),
        )
        c3_rec = ATTR.CommitRecord(
            "2f162ddd",
            "fix(codegen): check singleton exits before admitting the call prefix",
            ("crates/lang-codegen/src/ssa/refine/subset/tagged.rs",),
        )
        c4_test_only_be = ATTR.CommitRecord(
            "618f56d9",
            "fix(frontend): address PR #451 P2 review findings on loop escape witness and coverage",
            ("crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/escape_witness/loop_shadow.rs", "crates/lang-codegen/src/ssa/unit_field_replace_tests.rs"),
        )
        c4_unused_import = ATTR.CommitRecord(
            "a64c99b6",
            "fix(lang-codegen): remove unused imports in unit_lower construction and container",
            ("crates/lang-codegen/src/ssa/unit_lower/construction.rs",),
        )
        self.assertEqual("C2", ATTR.classify_commit(c2_rec).category)
        self.assertEqual("C3", ATTR.classify_commit(c3_rec).category)
        self.assertEqual("C4", ATTR.classify_commit(c4_test_only_be).category)
        self.assertEqual("C4", ATTR.classify_commit(c4_unused_import).category)

    def test_real_main_window_56_commits_crosses_stop_loss_threshold(self):
        """真实主干采样区间 92383a60..e428bc5d（56 条范围提交）跨过 50% 止损门槛，结论为 PROCEED_IR1_ADR。"""
        records = [ATTR.CommitRecord(sha, subj, files) for sha, subj, files in REAL_WINDOW_56_COMMITS]
        summary = ATTR.summarize_attribution(records, stop_loss_threshold=0.5)
        self.assertEqual(56, summary.total_fix_commits)
        self.assertEqual("PROCEED_IR1_ADR", summary.recommendation)
        self.assertGreaterEqual(summary.graph_solvable_ratio, 0.60)
        self.assertGreater(summary.counts["C2"], 0)
        self.assertGreater(summary.counts["C3"], 0)


if __name__ == "__main__":
    unittest.main()
