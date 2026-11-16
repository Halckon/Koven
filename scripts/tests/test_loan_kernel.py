"""借用内核演算原型的用例：每个 .lk 文件里的 `// ERR 规则` 标记即预期的拒绝位置。"""
import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("loan_kernel_checker", ROOT / "checker.py")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)

MARK = re.compile(r"//\s*ERR\s+(\w+)")


def expected(text):
    """没有标记即预期接受；有标记则预期恰好在这些行、因这些规则被拒绝。"""
    return {
        (match.group(1), number)
        for number, line in enumerate(text.splitlines(), start=1)
        if (match := MARK.search(line))
    }


class LoanKernelCases(unittest.TestCase):
    def test_every_case_matches_its_markers(self):
        cases = sorted(ROOT.glob("cases/*.lk"))
        self.assertGreaterEqual(len(cases), 24, "用例清单不应少于演算页列出的数量")
        for path in cases:
            with self.subTest(case=path.stem):
                text = path.read_text(encoding="utf-8")
                actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
                self.assertEqual(expected(text), actual)

    def test_dropping_a_rule_is_detected(self):
        """去掉父持有者暂停（K4）之后，依赖它的用例必须失败，否则用例没有在检验规则。"""
        text = (ROOT / "cases" / "C1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.ancestors
        CHECKER.Checker.ancestors = lambda self, loans: set()
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.ancestors = original
        self.assertNotEqual(expected(text), actual)

    def test_dropping_the_slot_rule_is_detected(self):
        """去掉槽位检查（X2）之后，评审 R2 的反例必须不再被拒绝，否则 I2 没有在检验它。"""
        text = (ROOT / "cases" / "I2.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.bind_types
        CHECKER.Checker.bind_types = lambda self, fun, values, line: None
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.bind_types = original
        self.assertNotEqual(expected(text), actual)

    def test_origins_are_read_from_the_owner_not_from_a_stale_copy(self):
        """K20：来源只记在拥有存储的变量上。若经别名读到的是建立借用时的副本，写入之后的新来源就丢了。"""
        text = (ROOT / "cases" / "Q1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.held_fields
        CHECKER.Checker.held_fields = lambda self, state, targets, carried: CHECKER.copy_loans(carried or {})
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.held_fields = original
        self.assertLess(actual, expected(text), "只应少报：别名路径上的新来源不再受保护")

    def test_writing_through_a_borrow_records_the_origins(self):
        """去掉流向的记账之后，经借用写入的来源既不受保护也不被签名核对，Q1 必须失败。"""
        text = (ROOT / "cases" / "Q1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.record_flow
        CHECKER.Checker.record_flow = lambda self, state, targets, incoming, replace=False: None
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.record_flow = original
        self.assertEqual(set(), {found for found in actual if found[0] in ("K6", "K9")})
        self.assertNotEqual(expected(text), actual)

    def test_capabilities_come_from_the_derivation_engine(self):
        """可复制、可逃逸、可转移都向能力推导问：让推导器一律回答“满足”，R1 的相应拒绝必须消失。

        例外是带着借用的视图交给可逃逸的类型参数：类型一侧的拒绝消失了，值名下的账仍然拒绝它（K20）。
        """
        text = (ROOT / "cases" / "R1.lk").read_text(encoding="utf-8")
        original, carries = CHECKER.CAP.Engine.requires, CHECKER.Checker.carries
        CHECKER.CAP.Engine.requires = lambda self, ty, cap, params=(): CHECKER.CAP.ALWAYS
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
            CHECKER.Checker.carries = lambda self, value: False
            unaccounted = {rule for rule, _, _ in CHECKER.check(text)}
        finally:
            CHECKER.CAP.Engine.requires = original
            CHECKER.Checker.carries = carries
        self.assertEqual({("K17", 36), ("S5", 56), ("CAP", 64)}, actual)
        self.assertEqual({"S5", "CAP"}, unaccounted, "只剩没有上界的类型参数引出的两类拒绝")

    def test_flows_across_a_borrow_field_reach_the_owner(self):
        """流向越过借用字段时，账要记到那个字段所指的视图的拥有者上；不记，拥有者一侧就看不到新来源。"""
        text = (ROOT / "cases" / "Q2.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.flow_into
        CHECKER.Checker.flow_into = lambda self, state, targets, fields, class_name, declared, flowing: None
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.flow_into = original
        lost = expected(text) - actual
        self.assertEqual({"K1", "K6", "K9"}, {rule for rule, _ in lost})
        self.assertEqual(set(), actual - expected(text))

    def test_origins_beyond_the_recursion_point_fold_onto_the_field(self):
        """递归视图：参数名下更深的来源归到回到同一类型的那个字段名下；不这样做，它们就没有来源可查。"""
        text = (ROOT / "cases" / "U1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.folded
        CHECKER.Checker.folded = lambda self, fields, class_name, reached: fields
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.folded = original
        self.assertEqual({("K6", 9), ("K6", 12)}, expected(text) - actual)

    def test_two_phase_borrows_only_relax_the_receiver(self):
        """K21：去掉“先按共享预留”，接收者位置在实参里读自己的写法都被拒绝，其余结果不变。"""
        text = (ROOT / "cases" / "T1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.argument
        CHECKER.Checker.argument = lambda self, expr, mode, state, stored_kind=None, reserved=False: original(
            self, expr, mode, state, stored_kind)
        original_activate = CHECKER.Checker.activate
        CHECKER.Checker.activate = lambda self, *args: None
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.argument, CHECKER.Checker.activate = original, original_activate
        self.assertEqual({("K1", 12), ("K1", 13), ("K1", 40)}, actual - expected(text))
        self.assertEqual(set(), expected(text) - actual)

    def test_early_drop_never_changes_what_is_accepted(self):
        """统一析构规则：提前析构只是许可。打开它之后，每个用例被接受或拒绝的结果必须不变。"""
        total = []
        for path in sorted(ROOT.glob("cases/*.lk")):
            text = path.read_text(encoding="utf-8")
            with self.subTest(case=path.stem):
                lexical = {(rule, line) for rule, line, _ in CHECKER.check(text)}
                early = {(rule, line) for rule, line, _ in CHECKER.check(text, True, total)}
                self.assertEqual(lexical, early)
        self.assertGreater(len(total), 50, "提前析构应当在用例里真的发生过")

    def test_the_drops_it_decides_satisfy_the_drop_obligation_verifier(self):
        """合并原型：借用检查器定下的析构（作用域结束、提前、赋值、失败清理）交给析构义务的检验器独立检验。

        检验器不看借用，只执行带显式析构的程序：每份义务恰好履行一次、可观察的不提前且按逆序、
        不使用已析构的值。经借用的访问记为对 owner 的使用，所以“借用使 owner 保持存活”也在其中。
        """
        checked = 0
        for path in sorted(ROOT.glob("cases/*.lk")):
            text = path.read_text(encoding="utf-8")
            for early in (False, True):
                lowered = {}
                CHECKER.check(text, early, lowered=lowered)
                for name, (types, program, accepted) in lowered.items():
                    if accepted:
                        with self.subTest(case=path.stem, fun=name, early=early):
                            self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
                            checked += 1
        self.assertGreater(checked, 250, "被接受的函数应当都检验到了")

    def test_only_observable_values_need_a_drop_flag(self):
        """只在一条分支上移走：不可观察的值在另一条分支末尾补偿析构，可观察的值留到作用域结束并带析构标志。"""
        flagged = set()

        def walk(stmts, fun):
            for stmt in stmts:
                if stmt.kind == "drop" and stmt.if_owned:
                    flagged.add((fun, stmt.var))
                walk(stmt.body, fun)
                walk(stmt.orelse, fun)

        for path in sorted(ROOT.glob("cases/*.lk")):
            lowered = {}
            CHECKER.check(path.read_text(encoding="utf-8"), True, lowered=lowered)
            for name, (_, program, accepted) in lowered.items():
                if accepted:
                    walk(program, name)
        self.assertEqual({("movedOnOneBranch", "f"), ("destroyedWhileSuspended", "g")}, flagged, "两处都是带 deinit 的值")
        lowered = {}
        CHECKER.check((ROOT / "cases" / "S1.lk").read_text(encoding="utf-8"), True, lowered=lowered)
        listing = [line.split("失败时")[0].strip() for line in CHECKER.DP.show(lowered["plainMovedOnOneBranch"][1])]
        self.assertEqual(["let a Arr", "if {", "move a", "call", "} else {", "drop a", "}",
                          "let b Arr", "use b", "drop b"], listing)

    def violations(self, case, fun, early=True):
        lowered = {}
        CHECKER.check((ROOT / "cases" / case).read_text(encoding="utf-8"), early, lowered=lowered)
        types, program, _ = lowered[fun]
        return {name.split()[0] for name in CHECKER.DP.Verifier(types).run_all(program)}

    def test_the_verifier_catches_an_owner_dropped_under_a_live_loan(self):
        """去掉“存活的 loan 使 owner 保持存活”，检验器必须独立发现 owner 在被借用期间析构了。"""
        original = CHECKER.Checker.borrowed_from
        CHECKER.Checker.borrowed_from = lambda self, name, state: False
        try:
            self.assertEqual({"V5"}, self.violations("L1.lk", "loanKeepsOwnerAlive"))
        finally:
            CHECKER.Checker.borrowed_from = original
        self.assertEqual(set(), self.violations("L1.lk", "loanKeepsOwnerAlive"))

    def test_the_verifier_catches_an_observable_value_dropped_early(self):
        """让能力推导把一切都判为可提前析构，检验器必须发现可观察的值被提前析构了。"""
        original = CHECKER.Checker.has
        CHECKER.Checker.has = lambda self, cap, ty: cap != "EarlyDrop" and original(self, cap, ty)
        try:
            self.assertEqual(set(), self.violations("L1.lk", "observableWaits"), "判定为可观察时不提前")
        finally:
            CHECKER.Checker.has = original
        CHECKER.Checker.has = lambda self, cap, ty: cap == "EarlyDrop" or original(self, cap, ty)
        try:
            lowered = {}
            CHECKER.check((ROOT / "cases" / "L1.lk").read_text(encoding="utf-8"), True, lowered=lowered)
            _, program, _ = lowered["observableWaits"]
            # 检验器按真实的性质判断：File 声明了 deinit，Wrap 按值含有一个 File
            types = {name: CHECKER.DP.TypeDecl(name, [], deinit=name in ("File", "Wrap"))
                     for name in lowered["observableWaits"][0]}
            found = {name.split()[0] for name in CHECKER.DP.Verifier(types).run_all(program)}
        finally:
            CHECKER.Checker.has = original
        self.assertIn("V3", found)

    def test_a_live_loan_keeps_its_owner_alive(self):
        """owner 的析构点由“它与从它借出的一切都不再使用”决定；可观察的值不提前。"""
        drops = []
        CHECKER.check((ROOT / "cases" / "L1.lk").read_text(encoding="utf-8"), True, drops)
        first = [(name, line) for fun, name, line in drops if fun == "loanKeepsOwnerAlive"]
        self.assertEqual([("b", 9), ("a", 10), ("c", 11)], first)
        self.assertEqual([("a", 17)], [(n, l) for fun, n, l in drops if fun == "observableWaits"])

    def test_stable_frames_are_what_allow_loans_across_suspension(self):
        """之四 S0.1：帧地址稳定是跨挂起点的帧内借用合法的前提；假设帧可移动，同一批用例必须被拒绝。"""
        text = (ROOT / "cases" / "P1.lk").read_text(encoding="utf-8")
        frames = {}
        stable = {(rule, line) for rule, line, _ in CHECKER.check(text, frames=frames)}
        self.assertEqual((["a", "r"], ["a"]), frames["loanAcrossSuspend", 19])
        self.assertEqual((["a"], []), frames["loanEndsBeforeSuspend", 33], "借用已结束，帧里只剩 owner")
        movable = {(rule, line) for rule, line, _ in CHECKER.check(text, frames_movable=True)}
        # 第 38 行没有任何帧内借用变量：只是把局部按 inout 传给一个会挂起的函数，被调帧就持有指向调用方帧的借用
        self.assertEqual({("S1", 19), ("S1", 25), ("S1", 38)}, movable - stable)
        self.assertEqual(set(), stable - movable)

    def test_values_waiting_for_their_drop_stay_in_the_frame(self):
        """S0.3 的析构一侧：留在帧里的不只是还会用到的值，还有等着在作用域结束时析构的值。"""
        text = (ROOT / "cases" / "P4.lk").read_text(encoding="utf-8")
        frames = {}
        CHECKER.check(text, frames=frames)
        self.assertEqual((["f", "g"], []), frames["sameThreadIsFine", 42], "两个值都不再使用，但都还没析构")
        self.assertEqual((["a", "f", "g"], []), frames["sameThreadIsFine", 44])

    def test_plain_values_no_longer_needed_are_dropped_before_suspending(self):
        """S6：不可观察、不再使用的值在挂起前必须已析构，与是否打开提前析构无关；否则它们会被带到另一个线程。"""
        text = (ROOT / "cases" / "P4.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(text, early_drop=False, lowered=lowered)
        listing = [line.split("失败时")[0].strip() for line in CHECKER.DP.show(lowered["plainDeadBeforeSuspend"][1])]
        self.assertEqual(["let s Session", "use s", "drop s", "suspend"], listing)
        CHECKER.Checker.drop_before_suspend = False
        try:
            relaxed = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.drop_before_suspend = True
        self.assertEqual({("S5", 12)}, relaxed - expected(text), "若只是“允许”提前析构，这个值就跨过了挂起点")

    def test_a_frame_destroyed_while_suspended_drops_what_it_owns(self):
        """帧在挂起点被销毁而不再恢复时，按那一刻的状态析构；去掉挂起点上的清单，检验器必须报泄漏。"""
        lowered = {}
        CHECKER.check((ROOT / "cases" / "P4.lk").read_text(encoding="utf-8"), lowered=lowered)
        types, program, _ = lowered["destroyedWhileSuspended"]
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        for stmt in program:
            if stmt.kind == "suspend":
                stmt.cleanup = []
        self.assertEqual(["V1"], [name.split()[0] for name in CHECKER.DP.Verifier(types).run_all(program)])

    def test_only_parallel_functions_require_transferable_frames(self):
        """之七 J18：默认不换线程。把 parallel 去掉之后，P3 的全部拒绝都应消失。"""
        text = (ROOT / "cases" / "P3.lk").read_text(encoding="utf-8")
        self.assertEqual([], CHECKER.check(text.replace("parallel suspend fun", "suspend fun")))


if __name__ == "__main__":
    unittest.main()
