"""失败与清理模型：每种候选政策违反哪些不变量是预期的一部分，政策改动后须同步这里。"""
import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "failure_model.py"
SPEC = importlib.util.spec_from_file_location("failure_model", PATH)
MODEL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODEL)


def outcome(report):
    return set(report.violations), set(report.stuck), set(report.notes)


class ScopeModel(unittest.TestCase):
    """J48 第 7 条：先等齐全部子，再让父失败；缺任何一半都不安全。"""

    EXPECTED = {
        "立即展开": ({MODEL.UAF}, set(), set()),
        "等齐但不传播": ({MODEL.STALE}, {MODEL.HANG}, set()),
        "等齐并传播": (set(), {MODEL.HANG}, set()),
        "等齐、传播、取消可唤醒": (set(), set(), set()),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.SCOPE_POLICIES))
        for name, policy in MODEL.SCOPE_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.scope_model(policy)))

    def test_result_does_not_depend_on_the_number_of_children(self):
        for name, policy in MODEL.SCOPE_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.scope_model(policy, children=3)))


class IoModel(unittest.TestCase):
    """失败政策：无终结证据时保留资源；保留不了时终止进程，且终止生效前不再释放。"""

    EXPECTED = {
        "取消即返回": ({MODEL.WRITE_AFTER_FREE}, set(), {MODEL.LEAK}),
        "随帧销毁": ({MODEL.WRITE_AFTER_FREE}, set(), set()),
        "整帧保留": ({MODEL.UNDISCHARGED}, set(), set()),
        "部分销毁并保留缓冲区": (set(), set(), {MODEL.LEAK}),
        "部分销毁，帧在父存储里": ({MODEL.WRITE_AFTER_FREE}, set(), set()),
        "终止进程但窗口内继续展开": ({MODEL.WRITE_AFTER_FREE}, set(), set()),
        "保留优先，不能保留则终止": (set(), set(), {MODEL.LEAK}),
        "保留优先，不能保留则终止（帧在父存储里）": (set(), set(), set()),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.IO_POLICIES))
        for name, policy in MODEL.IO_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.io_model(policy)))


if __name__ == "__main__":
    unittest.main()
