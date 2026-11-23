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

    def test_drop_capabilities_come_from_the_derivation_engine(self):
        """可否隐式析构、可否遗忘、可否提前析构都由能力推导按字段得出：装着“必须消费”的值的类型同样不能被隐式析构。

        让推导器一律回答“满足”，D5、D6 的拒绝全部消失。
        """
        text = read("D3_partial_and_open.dp")
        self.assertEqual("reject D5", outcomes(text)["boxedTicketDropped"])
        original = DP.CAP.Engine.requires
        DP.CAP.Engine.requires = lambda self, ty, cap, params=(): DP.CAP.ALWAYS
        try:
            relaxed = outcomes(text)
        finally:
            DP.CAP.Engine.requires = original
        self.assertEqual(set(), {name for name, result in relaxed.items() if result in ("reject D5", "reject D6")})
        self.assertIn("reject D5", outcomes(text).values())
        self.assertIn("reject D6", outcomes(text).values())

    def test_keeping_everything_until_scope_end_is_also_valid(self):
        """统一规则只“允许”提前析构：一律留到作用域结束的规划同样满足全部不变量。"""
        for name in ("D1_basics.dp", "D2_conditional_move.dp", "D3_partial_and_open.dp", "D4_deinit_body.dp",
                     "D5_field_order_and_generic_deinit.dp"):
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

    def test_open_needs_one_distinct_destination_per_field(self):
        """绕过规划器：即使上游连同清理一起漏掉字段，检验器也不能接受 open。"""
        types, _ = DP.parse("type File deinit\ntype Conn(a: File, b: File) deinit\n")
        for names in (["a", "b"], ["a"], ["a", "b", "extra"], ["a", "a"], ["a", "prior"]):
            with self.subTest(names=names):
                plan = [DP.Stmt("let", 0, var="prior", ty="File"), DP.Stmt("let", 1, var="c", ty="Conn"),
                        DP.Stmt("open", 2, var="c", names=names)]
                plan += [DP.Stmt("drop", 3, var=n) for n in reversed(list(dict.fromkeys(names)))]
                plan.append(DP.Stmt("drop", 4, var="prior"))
                found = DP.Verifier(types, explore_failure=False).run_all(plan)
                self.assertEqual(set() if names == ["a", "b"] else {"V6"}, {v.split()[0] for v in found})

    def test_open_reusing_an_expired_name_uses_its_new_drop_order(self):
        types, _ = DP.parse("type File deinit\ntype Conn(a: File, b: File) deinit\n")
        inner = [DP.Stmt("let", 1, var="x", ty="File"), DP.Stmt("drop", 2, var="x")]
        plan = [DP.Stmt("block", 1, body=inner), DP.Stmt("let", 3, var="f", ty="File"),
                DP.Stmt("let", 4, var="c", ty="Conn"), DP.Stmt("open", 5, var="c", names=["x", "b"])]
        plan += [DP.Stmt("drop", 6, var=n) for n in ("b", "x", "f")]
        self.assertEqual({}, DP.Verifier(types, explore_failure=False).run_all(plan))

    def test_invalid_open_stops_only_its_own_path(self):
        types, _ = DP.parse("type Buf\ntype Conn(a: Buf, b: Buf) deinit\n")
        bad = [DP.Stmt("let", 1, var="f", ty="Buf"), DP.Stmt("drop", 2, var="f"),
               DP.Stmt("drop", 3, var="f"), DP.Stmt("let", 4, var="c", ty="Conn"),
               DP.Stmt("open", 5, var="c", names=["a"])]
        other = [DP.Stmt("let", 6, var="leaked", ty="Buf")]
        found = DP.Verifier(types, explore_failure=False).run_all([DP.Stmt("if", 1, body=bad, orelse=other)])
        self.assertEqual({"V2": [(0, 2)], "V6": [(0, 2)], "V1": [(1, 2)]},
                         {v.split()[0]: path for v, path in found.items()})

    def test_open_transfers_field_cleanup_on_success_and_failure(self):
        types, funs = DP.parse(read("D3_partial_and_open.dp"))
        plan = DP.Planner(types, DP.Policy(early="none")).plan(funs["closeKeepsOrder"])
        verify = lambda body: {v.split()[0] for v in DP.Verifier(types).run_all(body)}
        self.assertEqual(set(), verify(plan))
        self.assertEqual({"V1"}, verify([s for s in plan if not (s.kind == "drop" and s.var == "buf")]))
        self.assertEqual({"V2"}, verify(plan + [DP.Stmt("drop", 0, var="c")]))
        call = next(s for s in plan if s.kind == "call")
        self.assertEqual(["buf", "sock", "f"], [name for name, _ in call.cleanup])
        call.cleanup = call.cleanup[1:]  # 仅破坏失败清理，正常出口不变
        self.assertEqual({"V1"}, verify(plan))

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

    def traces_of(self, filename, fun, explore_failure=False):
        types, funs = DP.parse(read(filename))
        policy = DP.Policy()
        deinits, _, _ = DP.plan_deinits(types, policy)
        planned = DP.Planner(types, policy).plan(funs[fun])
        DP.sync_instantiated_deinits(types, deinits, policy)
        verifier = DP.Verifier(types, explore_failure=explore_failure, deinits=deinits)
        self.assertEqual({}, verifier.run_all(planned))
        return [trace for _, trace in verifier.traces]

    def test_single_value_field_drop_order_and_partial_move_traces(self):
        """单个值内部字段按声明逆序析构；带 deinit 时先跑本体再按逆序析构剩余字段。"""
        f = "D5_field_order_and_generic_deinit.dp"
        self.assertEqual([["p.second:drop(FileB)", "p.first:drop(FileA)"]],
                         self.traces_of(f, "pairDropsFieldsInReverseOrder"))
        self.assertEqual([["o.right:drop(FileC)", "o.left.second:drop(FileB)", "o.left.first:drop(FileA)"]],
                         self.traces_of(f, "nestedDropsFieldsInReverseOrder"))
        self.assertEqual([["x:drop(FileB)", "p.first:drop(FileA)"]],
                         self.traces_of(f, "partialMoveSecondDropsTakenThenFirst"))
        self.assertEqual([["x:drop(FileA)", "p.second:drop(FileB)"]],
                         self.traces_of(f, "partialMoveFirstDropsTakenThenSecond"))
        self.assertEqual([["d:deinit(OrderedDeinit)", "this.second:drop(FileB)", "this.first:drop(FileA)"]],
                         self.traces_of(f, "deinitRunsBeforeRemainingFields"))
        self.assertEqual([["d:deinit(MoveSecondDeinit)", "this.first:drop(FileA)"]],
                         self.traces_of(f, "deinitMovesSecondThenDropsFirst"))
        self.assertEqual([
            ["d:deinit(BranchDeinit)", "this.second:drop(FileB)"],
            ["d:deinit(BranchDeinit)", "this.second:drop(FileB)", "this.first:drop(FileA)"],
        ], self.traces_of(f, "deinitBranchMovesFirst"))

    def test_generic_deinit_instantiation_traces_and_drop_flags(self):
        """泛型 deinit 按未约束可观察参数规划，并在实例化后按声明逆序析构载荷与字段。"""
        f = "D5_field_order_and_generic_deinit.dp"
        self.assertEqual([
            ["b2:deinit(Box<FileA>)", "this.value:drop(FileA)", "b1:deinit(Box<Buf>)"],
        ], self.traces_of(f, "genericBoxWithBufAndFile"))
        self.assertEqual([
            ["w:deinit(Wrapper<FileB>)", "this.payload:drop(FileB)", "this.first:drop(FileA)"],
        ], self.traces_of(f, "genericWrapperDropsPayloadBeforeFirst"))
        self.assertEqual([
            ["b:deinit(BranchBox<FileA>)"],
            ["b:deinit(BranchBox<FileA>)", "this.value:drop(FileA)"],
        ], self.traces_of(f, "genericBranchBoxWithFile"))
        self.assertEqual([
            ["g:deinit(ScopeGuard<FileA, FileB>)"],
        ], self.traces_of(f, "genericScopeGuardMovesPayloadAndAction"))
        # ScopeGuard.deinit 中 call 失败时，析构尚未移交的 act 与 v（按逆序）
        failure_traces = self.traces_of(f, "genericScopeGuardMovesPayloadAndAction", explore_failure=True)
        self.assertIn(["g:deinit(ScopeGuard<FileA, FileB>)"], failure_traces)


if __name__ == "__main__":
    unittest.main()
