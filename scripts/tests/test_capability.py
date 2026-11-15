"""能力推导原型：.cap 文件里的 expect 与 `// ERR` 即预期；另验证“加一行即得一项能力”。"""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("capability", ROOT / "capability.py")
CAP = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAP)


def engine(text, capabilities=None):
    decls, _ = CAP.Parser(text).program()
    return CAP.Engine(decls, capabilities)


class CapabilityCases(unittest.TestCase):
    def test_every_case_matches_its_expectations(self):
        cases = sorted(ROOT.glob("capability_cases/*.cap"))
        self.assertGreaterEqual(len(cases), 5)
        for path in cases:
            with self.subTest(case=path.stem):
                self.assertEqual([], CAP.check(path.read_text(encoding="utf-8")))

    def test_a_wrong_expectation_is_reported(self):
        """用例必须能失败：把一条预期写反，检查器要报告它。"""
        self.assertTrue(CAP.check("class File(fd: Int) deinit\nexpect EarlyDrop File = yes\n"))

    def test_a_new_capability_is_one_table_row(self):
        """之四 C0 的验收：新增一项能力只需一行描述，即获得推导、条件与解释路径。"""
        caps = dict(CAP.CAPABILITIES)
        caps["Printable"] = CAP.capability("Printable", ("Int", "String"), ("List", "Nullable"))
        made = engine("class Tag(name: String)\nclass Node<T>(item: T, next: Node<T>?)\nclass Raw(p: RawPtr<Int>)\n", caps)
        self.assertEqual("if T: Printable", CAP.show(made.conds["Node", "Printable"], ["T"]))
        self.assertEqual(CAP.ALWAYS, made.requires(CAP.Ty("List", (CAP.Ty("Tag"),)), "Printable"))
        self.assertEqual(["Raw.p", "RawPtr<Int>（不满足 Printable）"], made.why_not(CAP.Ty("Raw"), "Printable"))

    def test_weak_makes_rc_observable_by_changing_one_row(self):
        """之四 R1.3：Weak 出现后，Rc 的析构时机可被观察。改 Rc 的一条叶子公理即可，其余推导随之改变。"""
        text = "class Cache(hits: Rc<Int>)\n"
        self.assertEqual(CAP.ALWAYS, engine(text).conds["Cache", "EarlyDrop"])
        caps = dict(CAP.CAPABILITIES)
        early = caps["EarlyDrop"]
        caps["EarlyDrop"] = CAP.Capability("EarlyDrop", {k: v for k, v in early.leaves.items() if k != "Rc"},
                                           early.deinit_ok, early.kinds)
        self.assertIs(CAP.NEVER, engine(text, caps).conds["Cache", "EarlyDrop"])


if __name__ == "__main__":
    unittest.main()
