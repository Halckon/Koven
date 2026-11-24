"""失败与清理模型：每种候选政策违反哪些不变量是预期的一部分，政策改动后须同步这里。"""
import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "failure_model.py"
SPEC = importlib.util.spec_from_file_location("failure_model", PATH)
MODEL = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODEL
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


class ScopeCancelDrainModel(unittest.TestCase):
    """3A 确定场景：丢弃 Job_A 后子 A 失败产生 err_A，子 B 显式经 blocked -> cleaning -> done/deinit_failed。"""

    BOTH_TERMINALS = {MODEL.DRAINED_PROPAGATED_ERR_A, MODEL.DEINIT_FAIL_ABORTED}
    EXPECTED = {
        "唤醒即视作结束": ({MODEL.UAF}, set(), BOTH_TERMINALS),
        "阻塞中超时释放父帧": ({MODEL.UAF}, set(), BOTH_TERMINALS),
        "清理中超时释放父帧": ({MODEL.UAF}, set(), BOTH_TERMINALS),
        "取消结果覆盖首错": ({MODEL.WRONG_ERROR}, set(), {MODEL.DEINIT_FAIL_ABORTED}),
        "首错覆盖子deinit失败": ({MODEL.DEINIT_FAIL_SUPPRESSED}, set(), {MODEL.DRAINED_PROPAGATED_ERR_A}),
        "等子实际终止、保留err_A、deinit失败终止进程": (set(), set(), BOTH_TERMINALS),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.SCOPE_CANCEL_DRAIN_POLICIES))
        for name, policy in MODEL.SCOPE_CANCEL_DRAIN_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.scope_cancel_drain_model(policy)))


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
        "操作持有缓冲区，随帧销毁其余内容": (set(), set(), {MODEL.LEAK}),
        "操作持有缓冲区，且有后继回收": (set(), set(), set()),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.IO_POLICIES))
        for name, policy in MODEL.IO_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.io_model(policy)))


class MultiIoModel(unittest.TestCase):
    """多请求、证据丢失与代际校验：按 (id, gen) 逐项回收，无证据则保留不误释放。"""

    EXPECTED = {
        "不核对代际": ({MODEL.STALE_RECLAIM}, set(), set()),
        "任一终结即整批回收": ({MODEL.WRITE_AFTER_FREE}, set(), set()),
        "证据丢失后超时回收": ({MODEL.WRITE_AFTER_FREE}, set(), set()),
        "按身份与代际逐项回收（证据不丢失）": (set(), set(), set()),
        "按身份与代际逐项回收（证据可丢失，丢失即保留）": (set(), set(), {MODEL.LEAK}),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.MULTI_IO_POLICIES))
        for name, policy in MODEL.MULTI_IO_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.multi_io_model(policy)))

    def test_batch_reclaim_flags_lost_peer_slot_on_both_branches(self):
        """任一槽终结触发整批回收时，另一侧已丢失终结证据（lost）的槽同样须报 WRITE_AFTER_FREE。"""
        policy = MODEL.MultiIoPolicy(check_generation=True, reclaim_without_evidence=False,
                                     batch_reclaim_on_any=True, evidence_can_lose=True)
        captured = {}
        original_explore = MODEL.explore

        def capture(initial, events, violations, final_notes):
            captured["events"] = events
            captured["violations"] = violations
            return original_explore(initial, events, violations, final_notes)

        MODEL.explore = capture
        try:
            MODEL.multi_io_model(policy)
        finally:
            MODEL.explore = original_explore

        for start, event_name in (
            (("terminal", 1, "lost", False), "槽0(g1):整批回收全部槽位"),
            (("lost", 1, "terminal", False), "槽1:整批回收全部槽位"),
        ):
            with self.subTest(start=start):
                transitions = dict(captured["events"](start))
                self.assertIn(event_name, transitions)
                self.assertIn(MODEL.WRITE_AFTER_FREE, captured["violations"](transitions[event_name]))


class SupervisionModel(unittest.TestCase):
    """监督边界与多线程终止窗口：边界须隔离可变状态，终止窗口须冻结所有线程的释放。"""

    EXPECTED = {
        "边界共享可变状态": ({MODEL.CORRUPT_STATE}, set(), set()),
        "隔离边界但终止窗口未冻结他线程": ({MODEL.WRITE_AFTER_FREE}, set(), set()),
        "隔离边界、监督恢复且全线程冻结": (set(), set(), set()),
        "隔离边界、升级终止且全线程冻结": (set(), set(), set()),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.SUPERVISION_POLICIES))
        for name, policy in MODEL.SUPERVISION_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.supervision_model(policy)))


class CleanupProgressModel(unittest.TestCase):
    """清理进度的记录与恢复：中途崩溃时须由后继凭持久化进度或幂等步骤续作，否则遗漏或重复释放。"""

    EXPECTED = {
        "无后继恢复": ({MODEL.UNDISCHARGED}, set(), set()),
        "无进度记录且非幂等": ({MODEL.DOUBLE_CLEANUP}, set(), set()),
        "持久化进度记录（非幂等步骤）": (set(), set(), set()),
        "无进度记录但步骤幂等": (set(), set(), set()),
        "非原子写进度且非幂等": ({MODEL.DOUBLE_CLEANUP}, set(), set()),
        "非原子写进度但步骤幂等": (set(), set(), set()),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.CLEANUP_PROGRESS_POLICIES))
        for name, policy in MODEL.CLEANUP_PROGRESS_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.cleanup_progress_model(policy)))

    def test_non_atomic_checkpoint_covers_both_resource_steps(self):
        """非原子写进度间隙同时覆盖资源 1 与资源 2：任一步释放后写进度前崩溃，非幂等重放均触发重复清理。"""
        non_idem = MODEL.CLEANUP_PROGRESS_POLICIES["非原子写进度且非幂等"]
        idem = MODEL.CLEANUP_PROGRESS_POLICIES["非原子写进度但步骤幂等"]
        r1_report = MODEL.cleanup_progress_model(non_idem)
        self.assertIn("主清理者:释放资源1后写进度前崩溃", r1_report.violations[MODEL.DOUBLE_CLEANUP])
        r2_report = MODEL.cleanup_progress_model(non_idem, initial=("primary", "freed", "held", 1))
        self.assertIn("主清理者:释放资源2后写进度前崩溃", r2_report.violations[MODEL.DOUBLE_CLEANUP])

        # 从默认初态 cp=0 出发，钉住资源 2 写进度前崩溃的完整 6 步可达路径（不改动模型政策与 explore()）。
        captured = {}
        original_explore = MODEL.explore

        def capture(initial, events, violations, final_notes):
            captured["initial"] = initial
            captured["events"] = events
            captured["violations"] = violations
            return original_explore(initial, events, violations, final_notes)

        MODEL.explore = capture
        try:
            MODEL.cleanup_progress_model(non_idem)
        finally:
            MODEL.explore = original_explore

        state = captured["initial"]
        self.assertEqual(("primary", "held", "held", 0), state)
        expected_r2_path = [
            "主清理者:释放资源1(进度未落盘)",
            "主清理者:写入资源1进度",
            "主清理者:释放资源2(进度未落盘)",
            "主清理者:释放资源2后写进度前崩溃",
            "后继:接管清理",
            "后继:执行资源2清理",
        ]
        for step in expected_r2_path:
            self.assertEqual([], captured["violations"](state))
            transitions = dict(captured["events"](state))
            self.assertIn(step, transitions)
            state = transitions[step]
        self.assertEqual(("done", "freed", "double_freed", 2), state)
        self.assertIn(MODEL.DOUBLE_CLEANUP, captured["violations"](state))

        self.assertEqual((set(), set(), set()), outcome(MODEL.cleanup_progress_model(idem)))
        self.assertEqual((set(), set(), set()), outcome(MODEL.cleanup_progress_model(idem, initial=("primary", "freed", "held", 1))))


class LockChannelModel(unittest.TestCase):
    """锁中毒与 channel 对端关闭：展开须同时释放锁、标记中毒并关闭发送端，否则损坏共享状态或永久阻塞。"""

    EXPECTED = {
        "不展开（原地停住）": (set(), {MODEL.LOCK_OR_CHAN_HANG}, set()),
        "展开释放并关通道但不标中毒": ({MODEL.CORRUPT_STATE}, set(), set()),
        "展开释放并标中毒但不关通道": (set(), {MODEL.LOCK_OR_CHAN_HANG}, set()),
        "展开释放、标中毒并关闭通道": (set(), set(), set()),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.LOCK_CHANNEL_POLICIES))
        for name, policy in MODEL.LOCK_CHANNEL_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.lock_channel_model(policy)))


if __name__ == "__main__":
    unittest.main()
