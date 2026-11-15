"""析构义务原型：每个函数头上的 `// => 预期` 即规划器与检验器合起来应得的结果。"""
import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("drop_plan", ROOT / "drop_plan.py")
DP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DP)

EXPECT = re.compile(r"fun (\w+) \{\s*// => (.+)")


def outcomes(text, policy=DP.Policy()):
    return {name: f"reject {rule}" if rule else ("violate " + " ".join(sorted(v.split()[0] for v in found))
                                                 if found else "ok")
            for name, rule, found, _ in DP.check(text, policy)}


def read(name):
    return (ROOT / "drop_cases" / name).read_text(encoding="utf-8")


class DropPlanCases(unittest.TestCase):
    def test_every_function_matches_its_expectation(self):
        cases = sorted(ROOT.glob("drop_cases/*.dp"))
        self.assertGreaterEqual(len(cases), 3)
        for path in cases:
            text = path.read_text(encoding="utf-8")
            expected = {m.group(1): m.group(2).strip() for m in map(EXPECT.search, text.splitlines()) if m}
            with self.subTest(case=path.stem):
                self.assertEqual(expected, outcomes(text))

    def test_keeping_everything_until_scope_end_is_also_valid(self):
        """统一规则只“允许”提前析构：一律留到作用域结束的规划同样满足全部不变量。"""
        for name in ("D1_basics.dp", "D2_conditional_move.dp", "D3_partial_and_open.dp"):
            lexical = outcomes(read(name), DP.Policy(early="none"))
            self.assertEqual(outcomes(read(name)), lexical, name)

    def test_early_drop_of_observable_values_is_detected(self):
        """把可观察的值也提前析构（变异）必须被检验器发现，否则“作用域结束处析构”没有被检验。"""
        found = outcomes(read("D1_basics.dp"), DP.Policy(early="all"))
        self.assertEqual("violate V3", found["lexical"])

    def test_dropping_at_branch_end_changes_observable_timing(self):
        """对可观察的值，在没有移动的分支末尾析构会使析构早于作用域结束；析构标志不会。"""
        found = outcomes(read("D2_conditional_move.dp"), DP.Policy(maybe_moved="branch_end"))
        self.assertEqual("violate V3", found["fileOneBranch"])
        self.assertEqual("ok", found["bufOneBranch"])
        self.assertEqual("reject D3", outcomes(read("D2_conditional_move.dp"),
                                               DP.Policy(maybe_moved="reject"))["fileOneBranch"])

    def test_missing_cleanup_is_detected(self):
        """失败出口上不清理、合流处忽略“可能已移走”（两种变异）都必须表现为泄漏。"""
        self.assertEqual("violate V1", outcomes(read("D1_basics.dp"),
                                                DP.Policy(failure_cleanup=False))["lexical"])
        self.assertEqual("violate V1", outcomes(read("D2_conditional_move.dp"),
                                                DP.Policy(maybe_moved="ignore"))["fileOneBranch"])

    def test_verifier_rejects_hand_written_wrong_plans(self):
        """检验器独立于规划器：手写的错误规划（顺序颠倒、重复析构）必须被它发现。"""
        types, _ = DP.parse("type File deinit\n")

        def run(*steps):
            body = [DP.Stmt(kind, 1, var=var, ty="File" if kind == "let" else "") for kind, var in steps]
            return {name.split()[0] for name in DP.Verifier(types, explore_failure=False).run_all(body)}

        self.assertEqual(set(), run(("let", "f"), ("let", "g"), ("drop", "g"), ("drop", "f")))
        self.assertEqual({"V4"}, run(("let", "f"), ("let", "g"), ("drop", "f"), ("drop", "g")))
        self.assertEqual({"V2"}, run(("let", "f"), ("drop", "f"), ("drop", "f")))
        self.assertEqual({"V5"}, run(("let", "f"), ("move", "f"), ("use", "f")))


if __name__ == "__main__":
    unittest.main()
