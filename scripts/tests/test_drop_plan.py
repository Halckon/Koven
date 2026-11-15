"""析构义务原型：每个函数头上的 `// => 预期` 即规划器与检验器合起来应得的结果。"""
import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("drop_plan", ROOT / "drop_plan.py")
DP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DP)

EXPECT = re.compile(r"(?:fun|deinit) (\w+) \{\s*// => (.+)")


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
        for name in ("D1_basics.dp", "D2_conditional_move.dp", "D3_partial_and_open.dp", "D4_deinit_body.dp"):
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

    def test_what_each_access_to_this_allows_in_deinit(self):
        """deinit 里 this 的三种访问方式：只读借用不能取走任何字段；独占借用只能取走有空值的字段；拥有才能取走任意字段。"""
        text = read("D4_deinit_body.dp")
        bodies = ("Conn", "Guard", "Taker", "Unlinker", "NoEmpty", "OneBranch")
        seen = {access: [outcomes(text, DP.Policy(this_access=access))[name] for name in bodies]
                for access in ("borrow", "inout", "own")}
        self.assertEqual(["ok", "reject D7", "reject D7", "reject D7", "reject D7", "reject D7"], seen["borrow"])
        self.assertEqual(["ok", "reject D7", "reject D7", "ok", "reject D8", "reject D7"], seen["inout"])
        self.assertEqual(["ok", "ok", "ok", "ok", "reject D8", "ok"], seen["own"])

    def test_a_failing_deinit_can_be_an_ordinary_failure_only_with_cleanup_at_drop_points(self):
        """把 deinit 在正常路径上的失败当作普通失败：每个析构点都要一份清理清单，否则其余的值泄漏。"""
        text = read("D4_deinit_body.dp")
        propagate = DP.Policy(deinit_failure="propagate")
        self.assertEqual(outcomes(text), outcomes(text, propagate))
        unplanned = outcomes(text, DP.Policy(deinit_failure="propagate", drop_cleanup=False))
        self.assertEqual("violate V1", unplanned["dropsOne"])
        self.assertEqual("violate V1", unplanned["dropsTwo"])

    def test_a_second_failure_during_cleanup_terminates_the_process(self):
        """清理途中 deinit 再次失败只能终止进程；政策为终止时，第一次失败就终止。"""
        types, funs = DP.parse(read("D4_deinit_body.dp"))

        def aborted(policy):
            deinits = {"Conn": DP.Planner(types, policy).plan(types["Conn"].body, this="Conn")}
            verifier = DP.Verifier(types, deinits=deinits, propagate=policy.deinit_failure == "propagate")
            found = verifier.run_all(DP.Planner(types, policy).plan(funs["dropsTwo"]))
            self.assertEqual({}, found)
            return verifier.aborted_paths

        # dropsTwo 有三处会运行 deinit 的清理或析构；propagate 之下只有“失败之后又失败”的路径终止进程
        self.assertGreater(aborted(DP.Policy()), aborted(DP.Policy(deinit_failure="propagate")))
        self.assertGreater(aborted(DP.Policy(deinit_failure="propagate")), 0)

    def test_verifier_runs_deinit_bodies(self):
        """检验器真的执行 deinit 函数体：把规划里对 this 余下字段的析构删掉，必须表现为泄漏。"""
        types, funs = DP.parse(read("D4_deinit_body.dp"))
        body = DP.Planner(types, DP.Policy()).plan(types["Conn"].body, this="Conn")
        holder = DP.Planner(types, DP.Policy()).plan(funs["dropsOne"])
        broken = [stmt for stmt in body if stmt.kind != "drop"]
        self.assertEqual({}, DP.Verifier(types, deinits={"Conn": body}).run_all(holder))
        self.assertEqual(["V1"], [name.split()[0] for name in DP.Verifier(types, deinits={"Conn": broken}).run_all(holder)])

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
