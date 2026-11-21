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
                          ("openInOneArm", "this"), ("openSharedInOneArm", "this"), ("openSharedInOneArm", "f"),
                          ("writeAfterJoinedRead", "this"), ("writeAfterJoinedRead", "f"),
                          ("writeAfterConditionalDeliveryRead", "f"),
                          ("readAfterTwoDeliveries", "f"), ("readAfterTwoDeliveries", "g"),
                          ("deliverToSeparateSlots", "f"), ("deliverToSeparateSlots", "g"),
                          ("deliverThroughAlias", "f"), ("deliverThroughAlias", "g"),
                          ("openExclusiveInOneArm", "this"), ("openExclusiveInOneArm", "f"),
                          ("moveExclusiveOutOfArm", "this"), ("moveExclusiveOutOfArm", "f"),
                          ("conditionalReleaseKeepsDropFlag", "guard")},
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

    def test_open_transfers_a_shared_view_and_its_cleanup(self):
        source = (ROOT / "cases/Z5.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["dropsFieldsOnFailure"]
        self.assertTrue(accepted)
        self.assertTrue(lowered["forwardsSlice"][2])
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False), ("slice", False)], call.cleanup)
        self.assertEqual(["file", "slice"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.pop()  # 只在调用失败时漏掉视图的清理
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_one_arm_open_with_shared_origin_has_path_specific_cleanup(self):
        lowered = {}
        CHECKER.check((ROOT / "cases/Z6.lk").read_text(encoding="utf-8"), lowered=lowered)
        types, program, accepted = lowered["openSharedInOneArm"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        left, right, joined = [next(s for s in path if s.kind == "call")
                               for path in (branch.body, branch.orelse, program)]
        self.assertEqual([("file", False), ("slice", False), ("a", False)], left.cleanup)
        self.assertEqual([("f", False), ("a", False), ("this", False)], right.cleanup)
        self.assertEqual([("f", True), ("a", False), ("this", True)], joined.cleanup)

        def verify():
            return {v.split()[0]: path for v, path in CHECKER.DP.Verifier(types).run_all(program).items()}

        self.assertEqual({}, verify())
        left.cleanup.remove(("slice", False))
        # 三个参数绑定成功，进入左臂，第一次调用失败。
        self.assertEqual({"V1": [(0, 2)] * 4 + [(1, 2)]}, verify())
        left.cleanup.insert(1, ("slice", False))
        joined.cleanup.remove(("this", True))
        # 三个参数绑定成功，右臂的两个调用成功，合流后的调用失败。
        self.assertEqual({"V1": [(0, 2)] * 3 + [(1, 2), (0, 2), (0, 2), (1, 2)]}, verify())

    def test_opened_view_keeps_its_source_after_leaving_the_arm(self):
        source = (ROOT / "cases/Z7.lk").read_text(encoding="utf-8")
        left_only = source.replace("  if (*) {", "  {")
        # 删除左臂的语句但保留行号，便于与合流用例核对同一写入点。
        right_only = "\n".join("" if line.startswith("    ") else line
                               for line in left_only.splitlines())
        a, b = [i for i, line in enumerate(source.splitlines(), 1) if "// ERR K1" in line]
        for program, lines in ((source, {a, b}), (left_only, {a}), (right_only, {b})):
            self.assertEqual({("K1", line) for line in lines},
                             {(rule, line) for rule, line, _ in CHECKER.check(program)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["writeAfterJoinedRead"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        inside = next(s for s in branch.body if s.kind == "call")
        joined = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False), ("out", False), ("b", False), ("a", False)], inside.cleanup)
        self.assertEqual([("out", False), ("f", True), ("b", False), ("a", False), ("this", True)], joined.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        inside.cleanup.remove(("out", False))
        # 五个绑定成功，进入拆解臂，转移后调用失败。
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2)] * 6 + [(1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_opened_view_flows_into_the_callers_holder(self):
        source = (ROOT / "cases/Z8.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        self.assertFalse(lowered["missingFlow"][2])
        types, program, accepted = lowered["deliver"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("open", "this"), ("move", "slice")],
                         [(s.kind, s.var) for s in program[1:program.index(call)]])
        self.assertEqual([("file", False)], call.cleanup)
        self.assertEqual(["file"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.clear()  # slice 已移交，只有 file 还归本函数。
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

        types, program, accepted = lowered["writeAfterDeliveredRead"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("dst", False), ("b", False), ("a", False)], call.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))

    def test_conditional_delivery_keeps_old_and_possible_new_origins(self):
        source = (ROOT / "cases/Z9.lk").read_text(encoding="utf-8")
        called = source.replace("  if (*) {", "  {")
        skipped = "\n".join("" if line.startswith("    deliver(") else line for line in called.splitlines())
        a, b = [i for i, line in enumerate(source.splitlines(), 1) if "// ERR K1" in line]
        for path, program, lines in (("join", source, {a, b}), ("called", called, {a, b}), ("skipped", skipped, {b})):
            with self.subTest(path=path):
                self.assertEqual({("K1", line) for line in lines},
                                 {(rule, line) for rule, line, _ in CHECKER.check(program)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["writeAfterConditionalDeliveryRead"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        delivery = next(s for s in branch.body if s.kind == "call")
        joined = next(s for s in program if s.kind == "call")
        self.assertEqual([("dst", False), ("b", False), ("a", False)], delivery.cleanup)
        self.assertEqual([("dst", False), ("f", True), ("b", False), ("a", False)], joined.cleanup)

        def verify():
            return {v.split()[0]: path for v, path in CHECKER.DP.Verifier(types).run_all(program).items()}

        self.assertEqual({}, verify())
        # 四个绑定成功。分别检查跳过移交后合流失败，以及移交调用自身失败。
        joined.cleanup.remove(("f", True))
        self.assertEqual({"V1": [(0, 2)] * 4 + [(1, 2), (1, 2)]}, verify())
        joined.cleanup.insert(1, ("f", True))
        delivery.cleanup.remove(("dst", False))
        self.assertEqual({"V1": [(0, 2)] * 5 + [(1, 2)]}, verify())

    def test_two_deliveries_join_distinct_sources_in_the_same_slot(self):
        source = (ROOT / "cases/Z10.lk").read_text(encoding="utf-8")
        read = "  use(dst.source.len)\n"
        writes = "".join(f"  update(&mut {name})\n" for name in ("a", "b", "old"))
        before = source.replace(read + writes, writes + read)
        a, b, old = [i for i, line in enumerate(before.splitlines(), 1) if line.startswith("  update(")]
        # 固定一臂时只清空控制行和另一条调用，保留写入点行号。
        for path, skip, lines in (("join", None, {a, b, old}), ("left", "b", {a, old}), ("right", "a", {b, old})):
            with self.subTest(path=path):
                program = before if skip is None else "\n".join(
                    "" if line in ("  if (*) {", "  } else {", "  }")
                    or line.startswith(f"    deliver(Holder(Slice(&{skip})") else line
                    for line in before.splitlines())
                self.assertEqual({("K1", line) for line in lines},
                                 {(rule, line) for rule, line, _ in CHECKER.check(program)})

        lowered = {}
        self.assertEqual([], CHECKER.check(source, lowered=lowered))
        types, program, accepted = lowered["readAfterTwoDeliveries"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        calls = [next(s for s in arm if s.kind == "call") for arm in (branch.body, branch.orelse)]
        joined = next(s for s in program if s.kind == "call")
        tail = [(name, False) for name in ("old", "b", "a")]
        self.assertEqual([("dst", False), ("g", False)] + tail, calls[0].cleanup)
        self.assertEqual([("dst", False), ("f", False)] + tail, calls[1].cleanup)
        self.assertEqual([("dst", False), ("g", True), ("f", True)] + tail, joined.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        # 六个绑定成功；走被改动的臂，移交调用失败，漏掉仍由调用方拥有的 File。
        for arm, call in enumerate(calls):
            with self.subTest(arm=arm):
                retained = call.cleanup.pop(1)
                try:
                    found = CHECKER.DP.Verifier(types).run_all(program)
                    self.assertEqual({"V1": [(0, 2)] * 6 + [(arm, 2), (1, 2)]},
                                     {v.split()[0]: path for v, path in found.items()})
                finally:
                    call.cleanup.insert(1, retained)

    def test_two_deliveries_keep_separate_slots_origins(self):
        source = (ROOT / "cases/Z11.lk").read_text(encoding="utf-8")
        writes = "".join(f"  update(&mut {name})\n" for name in ("a", "b", "l", "r"))
        for slot, protected in (("left", ("a", "l")), ("right", ("b", "r"))):
            read = f"  use({slot}.source.len)\n"
            after = source.replace("  use(left.source.len)\n", read)
            before = after.replace(read + writes, writes + read)
            lines = {line: i for i, line in enumerate(before.splitlines(), 1)}
            with self.subTest(slot=slot):
                self.assertEqual([], CHECKER.check(after))
                self.assertEqual({("K1", lines[f"  update(&mut {name})"]) for name in protected},
                                 {(rule, line) for rule, line, _ in CHECKER.check(before)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["deliverToSeparateSlots"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        left, right = [next(s for s in arm if s.kind == "call") for arm in (branch.body, branch.orelse)]
        joined = next(s for s in program if s.kind == "call")
        slots = [("right", False), ("left", False)]
        tail = [(name, False) for name in ("r", "l", "b", "a")]
        self.assertEqual(slots + [("g", False)] + tail, left.cleanup)
        self.assertEqual(slots + [("f", False)] + tail, right.cleanup)
        self.assertEqual(slots + [("g", True), ("f", True)] + tail, joined.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        for name, arm in (("g", 0), ("f", 1)):
            with self.subTest(omitted=name):
                index = joined.cleanup.index((name, True))
                retained = joined.cleanup.pop(index)
                try:
                    found = CHECKER.DP.Verifier(types).run_all(program)
                    # 八个绑定成功、选臂、臂内调用成功、合流调用失败。
                    self.assertEqual({"V1": [(0, 2)] * 8 + [(arm, 2), (0, 2), (1, 2)]},
                                     {v.split()[0]: path for v, path in found.items()})
                finally:
                    joined.cleanup.insert(index, retained)

    def test_deliveries_through_an_alias_keep_origins_on_the_selected_slots(self):
        source = (ROOT / "cases/Z12.lk").read_text(encoding="utf-8")
        left_arm = "    deliver(Holder(Slice(&a), move f), target)"
        right_arm = ("    target = &mut right", "    deliver(Holder(Slice(&b), move g), target)")
        path_cases = (("join", (), ("a", "l"), ("b", "r")),
                      ("left", right_arm, ("a", "l"), ("r",)),
                      ("right", (left_arm,), ("l",), ("b", "r")))
        writes = "".join(f"  update(&mut {name})\n" for name in ("a", "b", "l", "r"))
        for path, omit, protects_left, protects_right in path_cases:
            program = source
            if omit:
                control = ("  if (*) {", "  } else {", "  }")
                program = "\n".join("" if line in control + omit else line for line in source.splitlines()) + "\n"
            for slot, protected in (("left", protects_left), ("right", protects_right)):
                with self.subTest(path=path, slot=slot):
                    read = f"  use({slot}.source.len)\n"
                    after = program.replace("  use(left.source.len)\n", read)
                    before = after.replace(read + writes, writes + read)
                    lines = {line: i for i, line in enumerate(before.splitlines(), 1)}
                    self.assertEqual([], CHECKER.check(after))
                    self.assertEqual({("K1", lines[f"  update(&mut {name})"]) for name in protected},
                                     {(rule, line) for rule, line, _ in CHECKER.check(before)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["deliverThroughAlias"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        calls = [next(s for s in arm if s.kind == "call") for arm in (branch.body, branch.orelse)]
        joined = next(s for s in program if s.kind == "call")
        slots = [("right", False), ("left", False)]
        tail = [(name, False) for name in ("r", "l", "b", "a")]
        self.assertEqual(slots + [("g", False)] + tail, calls[0].cleanup)
        self.assertEqual(slots + [("f", False)] + tail, calls[1].cleanup)
        self.assertEqual(slots + [("g", True), ("f", True)] + tail, joined.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        for arm, (call, slot) in enumerate(zip(calls, ("left", "right"))):
            with self.subTest(omitted=slot):
                entry = (slot, False)
                index = call.cleanup.index(entry)
                call.cleanup.pop(index)
                try:
                    found = CHECKER.DP.Verifier(types).run_all(program)
                    # 八个拥有绑定成功，选臂，移交调用失败；别名不引入新义务。
                    self.assertEqual({"V1": [(0, 2)] * 8 + [(arm, 2), (1, 2)]},
                                     {v.split()[0]: path for v, path in found.items()})
                finally:
                    call.cleanup.insert(index, entry)

    def test_delivery_through_a_joined_alias_flows_to_each_possible_slot(self):
        source = (ROOT / "cases/Z13.lk").read_text(encoding="utf-8")
        writes = "".join(f"  update(&mut {name})\n" for name in ("a", "l", "r"))
        left_only = "\n".join(
            "" if line in ("  if (*) {", "    target = &mut right", "  }") else line
            for line in source.splitlines()) + "\n"
        right_only = source.replace("  if (*) {", "  {")
        path_cases = (("join", source, ("a", "l"), ("a", "r")),
                      ("left", left_only, ("a", "l"), ("r",)),
                      ("right", right_only, ("l",), ("a", "r")))
        for path, program, protects_left, protects_right in path_cases:
            for slot, protected in (("left", protects_left), ("right", protects_right)):
                with self.subTest(path=path, slot=slot):
                    read = f"  use({slot}.source.len)\n"
                    after = program.replace("  use(left.source.len)\n", read)
                    before = after.replace(read + writes, writes + read)
                    lines = {line: i for i, line in enumerate(before.splitlines(), 1)}
                    self.assertEqual([], CHECKER.check(after))
                    self.assertEqual({("K1", lines[f"  update(&mut {name})"]) for name in protected},
                                     {(rule, line) for rule, line, _ in CHECKER.check(before)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["deliverThroughJoinedAlias"]
        self.assertTrue(accepted)
        delivery, after_delivery = [s for s in program if s.kind == "call"][:2]
        expected_cleanup = [(name, False) for name in ("right", "left", "r", "l", "a")]
        self.assertEqual(expected_cleanup, delivery.cleanup)
        self.assertEqual(expected_cleanup, after_delivery.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        for slot in ("left", "right"):
            with self.subTest(omitted=slot):
                entry = (slot, False)
                index = delivery.cleanup.index(entry)
                delivery.cleanup.pop(index)
                try:
                    found = CHECKER.DP.Verifier(types).run_all(program)
                    # 六个拥有绑定成功，经过选别名的分支，合流后移交调用失败。
                    self.assertEqual({"V1": [(0, 2)] * 7 + [(1, 2)]},
                                     {v.split()[0]: path for v, path in found.items()})
                finally:
                    delivery.cleanup.insert(index, entry)

    def test_open_transfers_an_exclusive_view_and_its_cleanup(self):
        source = (ROOT / "cases/Z14.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        self.assertTrue(lowered["forwardsExclusiveRef"][2])
        self.assertTrue(lowered["extractMutSlice"][2])
        self.assertTrue(lowered["callerWritesAfterReturnedExclusiveViewEnds"][2])
        types, program, accepted = lowered["dropsExclusiveFieldsOnFailure"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False), ("slice", False)], call.cleanup)
        self.assertEqual(["file", "slice"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.pop()  # 只在调用失败时漏掉独占视图的清理
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_one_arm_open_and_branch_exit_for_exclusive_view(self):
        source = (ROOT / "cases/Z15.lk").read_text(encoding="utf-8")
        # 仅取 moveExclusiveOutOfArm 构造合流、固定左臂、固定右臂对照。
        head = source.split("fun openExclusiveInOneArm")[0]
        move_fun = "fun moveExclusiveOutOfArm" + source.split("fun moveExclusiveOutOfArm")[1]
        base = head + move_fun
        use_out = "  update(out.source)\n"
        probes = "  use(a.len)\n  update(&mut a)\n  use(b.len)\n  update(&mut b)\n"
        before = base.replace(use_out + probes, probes + use_out)
        left_only = before.replace("  if (*) {", "  {")
        right_only = "\n".join("" if line.startswith("    ") else line for line in left_only.splitlines()) + "\n"
        lines = {line: i for i, line in enumerate(before.splitlines(), 1)}
        a_lines = {lines["  use(a.len)"], lines["  update(&mut a)"]}
        b_lines = {lines["  use(b.len)"], lines["  update(&mut b)"]}
        for path, program, wanted in (("join", before, a_lines | b_lines),
                                      ("left", left_only, a_lines),
                                      ("right", right_only, b_lines)):
            with self.subTest(path=path):
                self.assertEqual({("K1", line) for line in wanted},
                                 {(rule, line) for rule, line, _ in CHECKER.check(program)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        types, program, accepted = lowered["openExclusiveInOneArm"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        left, right, joined = [next(s for s in path if s.kind == "call")
                               for path in (branch.body, branch.orelse, program)]
        self.assertEqual([("file", False), ("slice", False), ("a", False)], left.cleanup)
        self.assertEqual([("f", False), ("a", False), ("this", False)], right.cleanup)
        self.assertEqual([("f", True), ("a", False), ("this", True)], joined.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        left.cleanup.remove(("slice", False))
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2)] * 4 + [(1, 2)]}, {v.split()[0]: path for v, path in found.items()})
        left.cleanup.insert(1, ("slice", False))
        joined.cleanup.remove(("this", True))
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2)] * 3 + [(1, 2), (0, 2), (0, 2), (1, 2)]},
                         {v.split()[0]: path for v, path in found.items()})

        types, program, accepted = lowered["moveExclusiveOutOfArm"]
        self.assertTrue(accepted)
        branch = next(s for s in program if s.kind == "if")
        inside = next(s for s in branch.body if s.kind == "call")
        joined = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False), ("out", False), ("b", False), ("a", False)], inside.cleanup)
        self.assertEqual([("out", False), ("f", True), ("b", False), ("a", False), ("this", True)], joined.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        inside.cleanup.remove(("out", False))
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2)] * 6 + [(1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_opened_exclusive_view_flows_into_direct_and_joined_alias_slots(self):
        source = (ROOT / "cases/Z16.lk").read_text(encoding="utf-8")
        # 仅取 deliverExclusiveThroughJoinedAlias 构造合流、固定左、固定右三条路径的读写对照。
        head = source.split("fun missingExclusiveFlow")[0]
        alias_fun = "fun deliverExclusiveThroughJoinedAlias" + source.split("fun deliverExclusiveThroughJoinedAlias")[1]
        base = head + alias_fun
        probes = "".join(f"  use({n}.len)\n  update(&mut {n})\n" for n in ("a", "l", "r"))
        left_only = "\n".join(
            "" if line in ("  if (*) {", "    target = &mut right", "  }") else line
            for line in base.splitlines()) + "\n"
        right_only = base.replace("  if (*) {", "  {")
        path_cases = (("join", base, ("a", "l"), ("a", "r")),
                      ("left", left_only, ("a", "l"), ("r",)),
                      ("right", right_only, ("l",), ("a", "r")))
        for path, program, protects_left, protects_right in path_cases:
            for slot, protected in (("left", protects_left), ("right", protects_right)):
                with self.subTest(path=path, slot=slot):
                    use_slot = f"  update({slot}.source)\n"
                    after = program.replace("  update(left.source)\n", use_slot)
                    before = after.replace(use_slot + probes, probes + use_slot)
                    lines = {line: i for i, line in enumerate(before.splitlines(), 1)}
                    wanted = {lines[f"  use({n}.len)"] for n in protected} | {lines[f"  update(&mut {n})"] for n in protected}
                    self.assertEqual([], CHECKER.check(after))
                    self.assertEqual({("K1", line) for line in wanted},
                                     {(rule, line) for rule, line, _ in CHECKER.check(before)})

        lowered = {}
        CHECKER.check(source, lowered=lowered)
        self.assertFalse(lowered["missingExclusiveFlow"][2])
        self.assertTrue(lowered["deliverExclusiveAfterRead"][2])
        types, program, accepted = lowered["deliverMut"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False)], call.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.clear()
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

        types, program, accepted = lowered["deliverExclusiveThroughJoinedAlias"]
        self.assertTrue(accepted)
        delivery, after_delivery = [s for s in program if s.kind == "call"][:2]
        expected_cleanup = [(name, False) for name in ("right", "left", "r", "l", "a")]
        self.assertEqual(expected_cleanup, delivery.cleanup)
        self.assertEqual(expected_cleanup, after_delivery.cleanup)
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        for slot in ("left", "right"):
            with self.subTest(omitted=slot):
                entry = (slot, False)
                index = delivery.cleanup.index(entry)
                delivery.cleanup.pop(index)
                try:
                    found = CHECKER.DP.Verifier(types).run_all(program)
                    self.assertEqual({"V1": [(0, 2)] * 7 + [(1, 2)]},
                                     {v.split()[0]: path for v, path in found.items()})
                finally:
                    delivery.cleanup.insert(index, entry)

    def test_open_transfers_multi_field_and_nested_views(self):
        source = (ROOT / "cases/Z17.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        for name in ("nestedReleasedAfterLastUse", "forwardNestedRo", "forwardNestedRw", "deliverOnlyRo"):
            self.assertTrue(lowered[name][2], name)
        for name in ("wrongFieldContractRejected", "localNestedEscapeRejected", "unsupportedCellSubView"):
            self.assertFalse(lowered[name][2], name)
        types, program, accepted = lowered["dropsNestedFieldsOnFailure"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False), ("nested", False)], call.cleanup)
        self.assertEqual(["file", "nested"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.pop()  # 仅在失败清单中漏掉嵌套子视图
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_open_preserves_deinit_subview_observability_and_cleanup(self):
        source = (ROOT / "cases/Z18.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        for name in ("writeAfterReleasingOpenedDeinitSubView", "openedDeinitSubViewDropsBeforeEarlierSource",
                     "conditionalReleaseKeepsDropFlag"):
            self.assertTrue(lowered[name][2], name)
        for name in ("openedDeinitSubViewStaysAliveUntilDrop", "moveOpenedDeinitSubViewToOlderSlotRejected"):
            self.assertFalse(lowered[name][2], name)
        types, program, accepted = lowered["conditionalReleaseKeepsDropFlag"]
        self.assertTrue(accepted)
        deinits = {"GuardSlice": lowered["deinit GuardSlice"][:2]}
        self.assertEqual({}, self.verify(types, program, "conditionalReleaseKeepsDropFlag", deinits))
        joined = next(s for s in program if s.kind == "call")
        drops = [s for s in program if s.kind == "drop"]
        self.assertEqual([("file", False), ("guard", True)], joined.cleanup)
        self.assertEqual([("file", False), ("guard", True)], [(s.var, s.if_owned) for s in drops])
        joined.cleanup.pop()  # 未移走路径在合流后失败时漏掉 guard
        found = self.verify(types, program, "conditionalReleaseKeepsDropFlag", deinits)
        self.assertEqual({"V1": [(0, 2), (1, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})
        joined.cleanup.append(("guard", True))
        drops[-1].if_owned = False  # 已移走路径在正常退出时重复析构 guard
        found = self.verify(types, program, "conditionalReleaseKeepsDropFlag", deinits)
        self.assertEqual({"V2": [(0, 2), (0, 2), (0, 2), (0, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_open_transfers_views_nested_across_borrow_fields(self):
        source = (ROOT / "cases/Z19.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        for name in ("forwardRoLink", "writeCallerViewThroughOpenedRwLink", "dropsRwLinkFieldsOnFailure"):
            self.assertTrue(lowered[name][2], name)
        for name in ("roOverMutCannotWriteInner", "missingRoLinkContract", "missingFlowThroughOpenedRwLink"):
            self.assertFalse(lowered[name][2], name)
        types, program, accepted = lowered["dropsRwLinkFieldsOnFailure"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("file", False), ("link", False)], call.cleanup)
        self.assertEqual(["file", "link"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.pop()  # 仅在失败清单中漏掉借用嵌套子视图
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_open_transfers_carrier_and_recursive_sub_views(self):
        source = (ROOT / "cases/Z20.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        for name in ("forwardOpenedBag", "dropsBagFieldsOnFailure", "forwardOpenedNodeSecond", "dropsNodeFieldsOnFailure"):
            self.assertTrue(lowered[name][2], name)
        for name in ("missingOpenedBagContract", "localOpenedBagEscape", "wrongOpenedNodeContract"):
            self.assertFalse(lowered[name][2], name)
        for fun, sub in (("dropsBagFieldsOnFailure", "bag"), ("dropsNodeFieldsOnFailure", "node")):
            with self.subTest(fun=fun):
                types, program, accepted = lowered[fun]
                self.assertTrue(accepted)
                call = next(s for s in program if s.kind == "call")
                self.assertEqual([("file", False), (sub, False)], call.cleanup)
                self.assertEqual(["file", sub], [s.var for s in program if s.kind == "drop"])
                self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
                call.cleanup.pop()  # 仅在失败清单中漏掉拆出的容器或递归子视图
                found = CHECKER.DP.Verifier(types).run_all(program)
                self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_open_transfers_direct_borrow_fields_without_owning_cleanup(self):
        source = (ROOT / "cases/Z21.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        for name in ("forwardRoDirect", "forwardRwDirect", "writeCallerThroughOpenedViewDirect",
                     "dropsOnlyOwnedFileOnFailure"):
            self.assertTrue(lowered[name][2], name)
        for name in ("missingRoDirectContract", "localRwDirectEscape", "missingFlowThroughOpenedViewDirect"):
            self.assertFalse(lowered[name][2], name)
        types, program, accepted = lowered["dropsOnlyOwnedFileOnFailure"]
        self.assertTrue(accepted)
        call = next(s for s in program if s.kind == "call")
        # 借用绑定 source 不带析构义务，失败与正常出口均只清理 file。
        self.assertEqual([("file", False)], call.cleanup)
        self.assertEqual(["file"], [s.var for s in program if s.kind == "drop"])
        self.assertEqual({}, CHECKER.DP.Verifier(types).run_all(program))
        call.cleanup.clear()
        found = CHECKER.DP.Verifier(types).run_all(program)
        self.assertEqual({"V1": [(0, 2), (1, 2)]}, {v.split()[0]: path for v, path in found.items()})

    def test_replace_leaves_empty_value_and_keeps_owner_drop_obligation(self):
        source = (ROOT / "cases/Z22.lk").read_text(encoding="utf-8")
        lowered = {}
        CHECKER.check(source, lowered=lowered)
        for name in ("deinit Unlinker", "replaceKeepsOwnerAndTakesNewObligation",
                     "replaceInLoopKeepsOwnerComplete", "replaceViaInout",
                     "replaceAllowedWhileOtherFieldBorrowed"):
            self.assertTrue(lowered[name][2], name)
        for name in ("cannotReplaceViaBorrow", "cannotReplaceWhileFieldBorrowed",
                     "cannotReplaceNonEmptyField", "cannotReplaceBorrowField"):
            self.assertFalse(lowered[name][2], name)
        deinits = {"Unlinker": lowered["deinit Unlinker"][:2]}
        types, program, accepted = lowered["replaceKeepsOwnerAndTakesNewObligation"]
        self.assertTrue(accepted)
        self.assertEqual({}, self.verify(types, program, "replaceKeepsOwnerAndTakesNewObligation", deinits))
        call = next(s for s in program if s.kind == "call")
        self.assertEqual([("n", False), ("u", False)], call.cleanup)
        # 两个 let（u 与 n）成功后，mayFail 失败：漏掉较晚的 n 同时违背逆序（V4）与泄漏（V1），且失败清理真实执行 deinit Unlinker；
        # 漏掉较早的 u 则仅报泄漏（V1）。
        for name, expected_violations in (("n", {"V1": [(0, 2), (0, 2), (1, 2), (0, 2), (0, 2)],
                                                 "V4": [(0, 2), (0, 2), (1, 2), (0, 2), (0, 2)]}),
                                          ("u", {"V1": [(0, 2), (0, 2), (1, 2)]})):
            with self.subTest(omitted=name):
                entry = (name, False)
                index = call.cleanup.index(entry)
                call.cleanup.pop(index)
                try:
                    found = self.verify(types, program, "replaceKeepsOwnerAndTakesNewObligation", deinits)
                    self.assertEqual(expected_violations, {v.split()[0]: path for v, path in found.items()})
                finally:
                    call.cleanup.insert(index, entry)

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
