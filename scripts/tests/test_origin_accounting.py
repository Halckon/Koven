"""来源记账的生成用例：按“存在哪里、经几手到达、所指在哪里”枚举，预期由规则推出。"""
import importlib.util
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"


def load(name):
    spec = importlib.util.spec_from_file_location(f"loan_kernel_{name}", ROOT / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECKER = load("checker")
CASES = load("origin_cases")
MARK = re.compile(r"//\s*ERR\s+(\w+)")
HELD_INOUT = "inout 参数" + CASES.HELD_BY_CALLER


def expected(text):
    return {(m.group(1), n) for n, line in enumerate(text.splitlines(), start=1) if (m := MARK.search(line))}


def mismatches():
    """与标记不符的生成用例：名字 -> (预期, 实得)。"""
    found = {}
    for name, text in CASES.cases():
        actual = {(rule, line) for rule, line, _ in CHECKER.check(text)}
        if actual != expected(text):
            found[name] = (expected(text), actual)
    return found


class OriginAccountingCases(unittest.TestCase):
    def test_every_generated_case_matches_its_markers(self):
        """读出借用后动它的来源一律被拒绝，来源无关的对照一律被接受；拒绝的位置与规则都须相符。"""
        self.assertEqual([], CHECKER.check(CASES.PRELUDE), "公共声明本身须被接受，否则对照程序没有意义")
        names = [name for name, _ in CASES.cases()]
        self.assertGreater(len(names), 1000)
        self.assertEqual({}, mismatches())

    def test_no_axis_value_is_left_without_a_case(self):
        """不适用的组合都带着说明；任何一个轴上的取值都不能整个落空，否则那条轴等于没有枚举。"""
        names = [name for name, _ in CASES.cases()]
        for value in (*CASES.STORAGES, *CASES.BUILDS, *CASES.HOPS, *CASES.LOCATIONS):
            with self.subTest(value=value):
                self.assertTrue(any(value in name.split("/") for name in names))
        for name, reason in CASES.skipped():
            self.assertTrue(reason, name)

    def test_origins_flowed_into_the_callers_holder_stay_live(self):
        """K20 存活：调用方的持有者在返回之后还在，流进它的来源不能随本函数里的变量一起死掉。

        去掉这一条，放入之后经 inout 参数写来源的程序全部被接受，其余结果不变。
        """
        original = CHECKER.Checker.live_loans

        def without(self, state):
            self.flowed_in = lambda state: ()
            try:
                return original(self, state)
            finally:
                del self.flowed_in

        CHECKER.Checker.live_loans = without
        try:
            wrong = mismatches()
        finally:
            CHECKER.Checker.live_loans = original
        after = [name for name, _ in CASES.cases() if name.endswith("/拒绝-放入后写来源")]
        self.assertTrue(after)
        self.assertLessEqual(set(after), set(wrong))
        for name, (wanted, actual) in wrong.items():
            with self.subTest(case=name):
                self.assertEqual(HELD_INOUT, name.split("/")[3])
                self.assertEqual(set(), actual, "只应少报")

    def test_fields_beyond_a_borrow_field_are_read_from_their_owner(self):
        """K20 现查：视图经借用字段指向另一个视图时，那个视图的字段来源在它的拥有者上，本视图名下只有旧副本。

        只读副本，经独占字段改写所指视图之后再把外层视图整个交出去（&h），新来源就丢了。
        """
        original = CHECKER.Checker.current_fields
        CHECKER.Checker.current_fields = lambda self, state, targets, carried, name: self.held_fields(
            state, targets, carried)
        try:
            wrong = mismatches()
        finally:
            CHECKER.Checker.current_fields = original
        self.assertTrue(wrong)
        for name, (wanted, actual) in wrong.items():
            with self.subTest(case=name):
                self.assertEqual(("独占字段所指的另一个视图", "经调用返回"), tuple(name.split("/")[::2][:2]))
                self.assertEqual(set(), actual, "只应少报")

    def test_containers_are_not_escapable_by_their_type(self):
        """裁决（2026-11-16）：流向的目标须是类型上能携带来源的值，所以逃逸性只问类型，不问账。

        容器的不可逃逸来自能力推导（以携带来源的类型实例化的 view class C<view T>）：让推导器把两个容器
        判为可逃逸，“交给可逃逸的类型参数”与“存进普通类的字段”就不再在标记处被拒绝。
        """
        original = CHECKER.CAP.Engine.requires
        CHECKER.CAP.Engine.requires = lambda self, ty, cap, params=(): CHECKER.CAP.ALWAYS if (
            cap == "Escapable" and ty.name in ("Bag", "Slot")) else original(self, ty, cap, params)
        try:
            wrong = mismatches()
        finally:
            CHECKER.CAP.Engine.requires = original
        typed = {name for name, _ in CASES.escapes() if "交给可逃逸的类型参数" in name or "存进普通类的字段" in name}
        self.assertEqual(6, len(typed))
        for name in typed:
            with self.subTest(case=name):
                wanted, actual = wrong[name]
                self.assertEqual(set(), wanted & actual, "标记处不再被拒绝")


if __name__ == "__main__":
    unittest.main()
