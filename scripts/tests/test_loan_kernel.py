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

    def test_only_parallel_functions_require_transferable_frames(self):
        """之七 J18：默认不换线程。把 parallel 去掉之后，P3 的全部拒绝都应消失。"""
        text = (ROOT / "cases" / "P3.lk").read_text(encoding="utf-8")
        self.assertEqual([], CHECKER.check(text.replace("parallel suspend fun", "suspend fun")))


if __name__ == "__main__":
    unittest.main()
