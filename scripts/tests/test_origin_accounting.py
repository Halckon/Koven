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

    def test_branch_join_needs_both_predecessors(self):
        """只保留写入路径会漏掉旧来源；只保留未写路径会漏掉新来源。"""
        prefix = "分支合流/局部视图/"
        programs = {name.removeprefix(prefix): text for name, text in CASES.cases() if name.startswith(prefix)}
        self.assertEqual({"写旧来源", "写新来源", "写无关来源", "最后使用后写"}, set(programs))
        original = CHECKER.Checker.join
        try:
            for side, missed in ((0, "写旧来源"), (1, "写新来源")):
                CHECKER.Checker.join = lambda self, a, b: (a, b)[side]
                wrong = set()
                for name, text in programs.items():
                    actual = {(rule, n) for rule, n, _ in CHECKER.check(text)}
                    if actual != expected(text):
                        self.assertFalse(actual, "只应漏报，不应产生新的诊断")
                        wrong.add(name)
                self.assertEqual({missed}, wrong)
        finally:
            CHECKER.Checker.join = original

    def test_loop_backedge_carries_a_source_only_on_the_second_iteration(self):
        """用 if 保留零轮和一轮的出口，只剪掉回边：应恰好漏报第二轮来源。"""
        prefix = "循环回边/局部视图/"
        programs = {name.removeprefix(prefix): text for name, text in CASES.cases() if name.startswith(prefix)}
        self.assertEqual({"写零轮来源", "写一轮来源", "写二轮来源", "写无关来源", "最后使用后写"}, set(programs))
        for name, text in programs.items():
            with self.subTest(case=name):
                self.assertEqual(1, text.count("while (*)"))
                once = text.replace("while (*)", "if (*)")
                actual = {(rule, n) for rule, n, _ in CHECKER.check(once)}
                if name == "写二轮来源":
                    self.assertEqual({"K1"}, {rule for rule, _ in expected(text)})
                    self.assertEqual(set(), actual)
                else:
                    self.assertEqual(expected(text), actual)

    def test_callers_view_is_updated_through_a_returned_reborrow(self):
        """正反例必须经过局部重借写回，而不是退化成对 h 的直接调用。"""
        self.assert_returned_reborrow_cases("局部视图的借用字段", "h", {
            "直接读": "&h.a.len", "经调用返回": "readV(h)", "经视图字段作实参（字段）": "lenOf(h.a)",
        })

    def test_callers_nested_view_is_updated_through_its_own_reborrow(self):
        """只借 W 的按值字段，仍须从 W 读出新来源，包括把内层视图交给 readV。"""
        self.assert_returned_reborrow_cases("按值嵌套的视图", "&mut h.inner", {
            "直接读": "&h.inner.a.len", "经调用返回": "readW(h)",
            "经视图字段作实参（字段）": "lenOf(h.inner.a)", "经视图字段作实参（内层视图）": "readV(&h.inner)",
        })

    def test_callers_exclusive_field_reborrows_the_referent(self):
        self.assert_returned_reborrow_cases("独占字段所指的另一个视图", "h.target", {
            "直接读": "&h.target.a.len", "经调用返回": "readH(h)",
            "经视图字段作实参（字段）": "lenOf(h.target.a)", "经视图字段作实参（所指视图）": "readV(h.target)",
        })

    def test_owner_observes_write_through_exclusive_field_reborrow(self):
        """直接从 v 读，不能靠 h 名下的副本保护新来源。"""
        body = """fun f(): Unit {
  val s = Arr(1)
  val o = Arr(2)
  val v = V(&o)
  val h = H(&mut v)
  val w = pass(h.target)
  setV(w, &s)
  val e = readV(&v)
"""
        for tail in ("  use(e)\n  s.len = 9", "  s.len = 9  // ERR K1\n  use(e)"):
            text = CASES.PRELUDE + body + tail + "\n}\n"
            self.assertEqual(expected(text), {(r, n) for r, n, _ in CHECKER.check(text)})

    def test_callers_env_reborrow_flows_through_function_contract(self):
        """合同读回不能被直接读字段代替，二者分别列为读法。"""
        self.assert_returned_reborrow_cases("闭包环境", "h", {
            "直接读": "&h.a.len", "经调用返回": "call(h, envBody)", "经视图字段作实参（字段）": "lenOf(h.a)",
        }, reborrow="passEnv", setter="setEnv")

    def test_callers_bag_is_read_through_its_contract_after_reborrow(self):
        """不透明容器只有 from h，不要求声明一个不可见字段的返回来源。"""
        write = "val w = passBag(h)\n  put(w, V(s))"
        prefix = "类型参数的持有者/经借用值的流向/经调用返回/"
        wanted = {prefix + mode + " 参数" + CASES.HELD_BY_CALLER + "/" + verdict
                  for mode in ("borrow", "inout") for verdict in ("接受", "拒绝-未声明流向")}
        wanted.update(prefix + HELD_INOUT + "/" + verdict for verdict in ("拒绝-写来源", "拒绝-放入后写来源"))
        actual = {name: text for name, text in CASES.cases() if write in text}
        self.assertEqual(wanted, set(actual))
        for name, text in actual.items():
            with self.subTest(case=name):
                self.assertIn(f"{write}\n  val e = peekBag(h)\n", text)

    def test_local_bag_source_stays_live_until_peek_result_last_use(self):
        """局部 h 没有调用方的存活保证，只让 peek 的结果继续使用它。"""
        body = """fun f(): Unit {
  val s = Arr(1)
  val h = Bag(0)
  val w = passBag(&mut h)
  put(w, V(&s))
  val e = peekBag(&h)
"""
        for tail in ("  use(e)\n  s.len = 9", "  s.len = 9  // ERR K1\n  use(e)"):
            text = CASES.PRELUDE + body + tail + "\n}\n"
            self.assertEqual(expected(text), {(r, n) for r, n, _ in CHECKER.check(text)})

    def assert_returned_reborrow_cases(self, storage, target, reads, reborrow="pass", setter="setV"):
        wanted = set()
        for hop in reads:
            for mode in ("borrow", "inout"):
                location = mode + " 参数" + CASES.HELD_BY_CALLER
                prefix = f"{storage}/经借用值的流向/{hop}/{location}/"
                verdicts = ["接受", "拒绝-未声明流向", "拒绝-未声明来源"]
                if mode == "inout":
                    verdicts += ["拒绝-写来源", "拒绝-放入后写来源"]
                wanted.update(prefix + verdict for verdict in verdicts)
        write = f"val w = {reborrow}({target})\n  {setter}(w, s)"
        actual = {name: text for name, text in CASES.cases() if write in text}
        self.assertEqual(wanted, set(actual))
        for name, text in actual.items():
            hop = name.split("/")[2]
            with self.subTest(case=name):
                self.assertIn(f"{write}\n  val e = {reads[hop]}\n", text)

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
