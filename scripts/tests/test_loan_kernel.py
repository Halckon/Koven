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
        """可复制、可逃逸、可转移都向能力推导问：让推导器一律回答“满足”，R1 的相应拒绝必须消失。"""
        text = (ROOT / "cases" / "R1.lk").read_text(encoding="utf-8")
        original = CHECKER.CAP.Engine.requires
        CHECKER.CAP.Engine.requires = lambda self, ty, cap, params=(): CHECKER.CAP.ALWAYS
        try:
            actual = {rule for rule, _, _ in CHECKER.check(text)}
        finally:
            CHECKER.CAP.Engine.requires = original
        self.assertEqual({"S5", "CAP"}, actual, "只剩没有上界的类型参数引出的两类拒绝")

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

    def test_leaving_the_function_destroys_what_it_still_owns(self):
        """K22：操作失败时函数从那里离开，仍拥有的值逆序析构。去掉离开时的检查，失败出口上的拒绝全部消失，

        连同函数正常结束时“来源先于析构可观察的持有者被析构”的两处；代码块结束处的那一处不受影响。
        """
        text = (ROOT / "cases" / "V1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.leave
        CHECKER.Checker.leave = lambda self, state, kind, line: None
        try:
            actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        finally:
            CHECKER.Checker.leave = original
        lost = expected(text) - actual
        self.assertEqual({found for found in expected(text) if found[0] == "K22"} | {("K9", 33), ("K9", 70)}, lost)
        self.assertEqual(set(), actual - expected(text))
        self.assertEqual(10, sum(rule == "K22" for rule, _ in expected(text)))

    def test_an_observable_holder_lives_until_it_is_destroyed(self):
        """之二 R1.2：带 deinit 的持有者活到析构点。把一切都判为可提前析构，V1 的拒绝全部消失。"""
        text = (ROOT / "cases" / "V1.lk").read_text(encoding="utf-8")
        original = CHECKER.Checker.has
        CHECKER.Checker.has = lambda self, cap, ty: cap == "EarlyDrop" or original(self, cap, ty)
        try:
            self.assertEqual([], CHECKER.check(text))
        finally:
            CHECKER.Checker.has = original

    def test_the_scope_waits_for_its_children_before_their_sources_die(self):
        """作用域库：子任务的来源之所以安全，是因为作用域的析构等齐全部子任务，而它活到析构点。

        去掉作用域的 deinit，“作用域还在时写被借出的变量”与“来源声明在作用域之后”都不再被拒绝。
        """
        text = (ROOT / "cases" / "W1.lk").read_text(encoding="utf-8")
        relaxed = text.replace("cell view class Scope<view T>(n: Int) deinit", "cell view class Scope<view T>(n: Int)")
        self.assertNotEqual(text, relaxed)
        actual = {(rule, line) for rule, line, _ in CHECKER.check(relaxed)}
        lost = expected(text) - actual
        self.assertEqual({"K1", "K9", "K22"}, {rule for rule, _ in lost})
        self.assertEqual(3, len(lost))
        self.assertEqual(set(), actual - expected(text))

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
                deinits = {name[len("deinit "):]: (kinds, program) for name, (kinds, program, accepted)
                           in lowered.items() if name.startswith("deinit ") and accepted}
                for name, (types, program, accepted) in lowered.items():
                    if accepted:
                        with self.subTest(case=path.stem, fun=name, early=early):
                            self.assertEqual({}, self.verify(types, program, name, deinits))
                            checked += 1
        self.assertGreater(checked, 250, "被接受的函数应当都检验到了")

    @staticmethod
    def verify(types, program, name, deinits):
        """函数直接执行；deinit 函数体放进“声明一个该类型的值然后析构它”的程序里执行。

        函数里析构到的值，凡是写出了 deinit 函数体的，检验器都真的执行那段函数体。
        """
        DP = CHECKER.DP
        if name.startswith("deinit "):
            owner = name[len("deinit "):]
            program = [DP.Stmt("let", 0, var="value", ty=owner), DP.Stmt("drop", 0, var="value")]
        used = {ty: deinits[ty] for ty in types if ty in deinits}
        for kinds, _ in used.values():  # 函数体里 this 的类型
            types = {**kinds, **types}
        return DP.Verifier(types, deinits={ty: body for ty, (_, body) in used.items()}).run_all(program)

    def test_a_deinit_body_owns_the_fields_of_this(self):
        """deinit 函数体走同一条路：this 拥有各字段，移出的由函数体交出去，余下的在函数体结束时析构。

        借用原型若忘了字段已被移出，检验器独立报重复析构；若把 this 当作不拥有字段，余下的字段就泄漏。
        """
        text = (ROOT / "cases" / "Y1.lk").read_text(encoding="utf-8")

        def found(fun):
            lowered = {}
            CHECKER.check(text, lowered=lowered)
            types, program, _ = lowered[fun]
            deinits = {fun[len("deinit "):]: (types, program)}
            return {name.split()[0] for name in self.verify(types, program, fun, deinits)}

        self.assertEqual(set(), found("deinit Taker"))
        self.assertEqual(set(), found("deinit OneBranch"))
        self.assertEqual(set(), found("deinit Guard"))
        original = CHECKER.Checker.take

        def forgetful(checker, expr, state):
            value = original(checker, expr, state)
            state[expr.place.root].gone.clear()
            return value

        CHECKER.Checker.take = forgetful
        try:
            self.assertEqual({"V2"}, found("deinit Guard"))
        finally:
            CHECKER.Checker.take = original
        original = CHECKER.Checker.owes
        CHECKER.Checker.owes = lambda checker, var: var.ty.name != "Taker" and original(checker, var)
        try:
            self.assertEqual({"V1"}, found("deinit Taker"))
        finally:
            CHECKER.Checker.owes = original

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
        self.assertEqual({("movedOnOneBranch", "f"), ("destroyedWhileSuspended", "g"), ("deinit OneBranch", "this"),
                          ("openInOneArm", "this")},
                         flagged, "带标志的都是带 deinit 的值，或余下的字段里有这样的值")
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

    def test_duplicate_parameter_cannot_overwrite_the_owned_receiver(self):
        source = (ROOT / "cases/Z1.lk").read_text(encoding="utf-8").replace(
            "fun close(own this: Conn)", "fun close(own this: Conn, borrow this: Conn)")
        with self.assertRaisesRegex(CHECKER.SyntaxProblem, "第 7 行.*重复形参 this"):
            CHECKER.check(source)

    def test_open_receiver_transfers_the_cleanup_at_each_call(self):
        lowered = {}
        CHECKER.check((ROOT / "cases/Z1.lk").read_text(encoding="utf-8"), lowered=lowered)
        types, program, accepted = lowered["close"]
        self.assertTrue(accepted)
        self.assertEqual([("this", ["sock", "buf"])], [(s.var, s.names) for s in program if s.kind == "open"])
        calls = [s for s in program if s.kind == "call"]
        self.assertEqual([[('buf', False), ('sock', False)], [('buf', False)]], [s.cleanup for s in calls])
        self.assertEqual(["buf"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        calls[0].cleanup.pop()  # 只在移交 sock 前的失败出口漏掉它
        self.assertEqual({"V1"}, {v.split()[0] for v in CHECKER.DP.Verifier(types).run_all(program)})

    def test_one_arm_open_keeps_distinct_cleanup_before_and_after_join(self):
        lowered = {}
        CHECKER.check((ROOT / "cases/Z2.lk").read_text(encoding="utf-8"), lowered=lowered)
        types, program, accepted = lowered["openInOneArm"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        opened = next(s for s in branch.body if s.kind == "open")
        self.assertEqual(("this", ["sock", "buf"]), (opened.var, opened.names))
        then_call, else_call, joined_call = [next(s for s in path if s.kind == "call")
                                              for path in (branch.body, branch.orelse, program)]
        then_drops = [s.var for s in branch.body if s.kind == "drop"]
        self.assertEqual(["buf", "sock"], then_drops)
        self.assertFalse(any(s.kind in ("open", "drop") for s in branch.orelse))
        self.assertEqual([("buf", False), ("sock", False), ("outer", False)], then_call.cleanup)
        self.assertEqual([("outer", False), ("this", False)], else_call.cleanup)
        self.assertEqual([("outer", False), ("this", True)], joined_call.cleanup)
        drops = [s for s in program if s.kind == "drop"]
        self.assertEqual([("outer", False), ("this", True)], [(s.var, s.if_owned) for s in drops])

        def verify():
            return {name.split()[0]: path for name, path in CHECKER.DP.Verifier(types).run_all(program).items()}

        # 两次 let、分支、臂内调用、合流后调用；调用选择 0 为成功，1 为失败。
        self.assertEqual({}, verify())
        drops[-1].if_owned = False  # 仅在拆解臂的正常出口重复析构 this
        self.assertEqual({"V2": [(0, 2)] * 5}, verify())
        drops[-1].if_owned = True
        joined_call.cleanup.pop()  # 仅在未拆解臂的合流后失败出口漏掉 this
        self.assertEqual({"V1": [(0, 2), (0, 2), (1, 2), (0, 2), (1, 2)]}, verify())

    def test_open_in_loop_restores_ownership_before_the_back_edge(self):
        lowered = {}
        CHECKER.check((ROOT / "cases/Z3.lk").read_text(encoding="utf-8"), lowered=lowered)
        self.assertFalse(lowered["repeatWithoutRestoring"][2])
        types, program, accepted = lowered["restoreEachTime"]
        self.assertTrue(accepted)
        loop = next(s for s in program if s.kind == "loop")
        self.assertEqual([("open", "this"), ("call", ""), ("move", "sock"), ("move", "buf"), ("set", "this")],
                         [(s.kind, s.var) for s in loop.body])
        self.assertEqual([("buf", False), ("sock", False)], loop.body[1].cleanup)
        self.assertEqual([("this", False)], next(s for s in program if s.kind == "call").cleanup)
        self.assertEqual([("this", False)], [(s.var, s.if_owned) for s in program if s.kind == "drop"])

        def verify(plan):
            return {name.split()[0]: path for name, path in CHECKER.DP.Verifier(types).run_all(plan).items()}

        self.assertEqual({}, verify(program))  # 检验器穷举 0、1、2 轮及各调用的成败
        # 参数 let 成功、零轮、循环后的调用成功：去掉最终析构会漏掉原值。
        self.assertEqual({"V1": [(0, 2), (0, 3), (0, 2)]}, verify([s for s in program if s.kind != "drop"]))
        # 第一轮成功重建，第二轮拆解后调用失败：漏掉 sock 会泄漏。
        loop.body[1].cleanup.pop()
        self.assertEqual({"V1": [(0, 2), (2, 3), (0, 2), (1, 2)]}, verify(program))

    def test_open_in_loop_requires_restoration_on_both_arms(self):
        lowered = {}
        CHECKER.check((ROOT / "cases/Z4.lk").read_text(encoding="utf-8"), lowered=lowered)
        for name in ("restoreOnlyLeft", "restoreOnlyRight"):
            self.assertFalse(lowered[name][2], name)
        types, program, accepted = lowered["restoreEitherArm"]
        self.assertTrue(accepted)
        loop = next(s for s in program if s.kind == "loop")
        branch = next(s for s in loop.body if s.kind == "if")
        left, right = [next(s for s in arm if s.kind == "call") for arm in (branch.body, branch.orelse)]
        self.assertEqual([("buf", False), ("sock", False)], left.cleanup)
        self.assertEqual([("this", False)], right.cleanup)
        self.assertEqual([("this", False)], next(s for s in program if s.kind == "call").cleanup)

        def verify():
            return {name.split()[0]: path for name, path in CHECKER.DP.Verifier(types).run_all(program).items()}

        self.assertEqual({}, verify())
        # 参数绑定成功、两轮、第一轮左臂调用成功；单独破坏第二轮所走分支的失败清理。
        prefix = [(0, 2), (2, 3), (0, 2), (0, 2)]
        for arm, call in enumerate((left, right)):
            with self.subTest(arm=arm):
                removed = call.cleanup.pop()
                try:
                    self.assertEqual({"V1": prefix + [(arm, 2), (1, 2)]}, verify())
                finally:
                    call.cleanup.append(removed)

    def test_the_verifier_catches_a_field_dropped_after_it_was_moved_out(self):
        """部分移出进入同一条路：借用原型若忘了字段已被移出，它定下的析构会把移走的字段再析构一次。"""
        self.assertEqual(set(), self.violations("X1.lk", "bothFields"))
        self.assertEqual(set(), self.violations("X1.lk", "observableField"))
        original = CHECKER.Checker.take

        def forgetful(checker, expr, state):
            value = original(checker, expr, state)
            state[expr.place.root].gone.clear()
            return value

        CHECKER.Checker.take = forgetful
        try:
            self.assertEqual({"V2"}, self.violations("X1.lk", "bothFields"))
        finally:
            CHECKER.Checker.take = original

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
