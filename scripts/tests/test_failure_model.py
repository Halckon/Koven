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


class ScopeCancelDrainExtendedModel(unittest.TestCase):
    """3A 更广覆盖：丢 Job_A 不取消作为独立事件、子 B 清理期重新阻塞、子 B 产生额外普通失败 err_B 的多错仲裁对照。"""

    SAFE_FIRST = {MODEL.DRAINED_PROPAGATED_ERR_A, MODEL.MULTI_ERR_KEPT_FIRST, MODEL.DEINIT_FAIL_ABORTED}
    SAFE_AGG = {MODEL.DRAINED_PROPAGATED_ERR_A, MODEL.MULTI_ERR_AGGREGATED, MODEL.DEINIT_FAIL_ABORTED}
    EXPECTED = {
        "丢Job即取消子任务": ({MODEL.DROP_JOB_CANCELLED}, set(), SAFE_FIRST),
        "清理期重新阻塞不可唤醒": (set(), {MODEL.HANG}, SAFE_FIRST),
        "清理期重新阻塞超时释放父帧": ({MODEL.UAF}, set(), SAFE_FIRST),
        "多错仲裁用后发错误覆盖首错": ({MODEL.LOST_FIRST_ERROR}, set(), {MODEL.DRAINED_PROPAGATED_ERR_A, MODEL.DEINIT_FAIL_ABORTED}),
        "多错仲裁吞掉子deinit失败": ({MODEL.DEINIT_FAIL_SUPPRESSED}, set(), {MODEL.DRAINED_PROPAGATED_ERR_A, MODEL.MULTI_ERR_KEPT_FIRST}),
        "候选策略1：丢Job不取消、重阻塞可唤醒、多错保留首错、deinit失败终止": (set(), set(), SAFE_FIRST),
        "候选策略2：丢Job不取消、重阻塞可唤醒、多错聚合err_A+err_B、deinit失败终止": (set(), set(), SAFE_AGG),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.SCOPE_CANCEL_DRAIN_EXTENDED_POLICIES))
        for name, policy in MODEL.SCOPE_CANCEL_DRAIN_EXTENDED_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.scope_cancel_drain_extended_model(policy)))

    def test_reblock_bounded_state_graph_is_acyclic(self):
        """reblocked_once 有界化后，安全候选策略的状态转移图严格无环（不存在 cleaning <-> reblocked 无限循环）。"""
        policy = MODEL.SCOPE_CANCEL_DRAIN_EXTENDED_POLICIES["候选策略1：丢Job不取消、重阻塞可唤醒、多错保留首错、deinit失败终止"]
        captured = {}
        original_explore = MODEL.explore

        def capture(initial, events, violations, final_notes):
            captured["initial"] = initial
            captured["events"] = events
            return original_explore(initial, events, violations, final_notes)

        MODEL.explore = capture
        try:
            MODEL.scope_cancel_drain_extended_model(policy)
        finally:
            MODEL.explore = original_explore

        visiting, visited = set(), set()

        def dfs(state):
            visiting.add(state)
            for _, nxt in captured["events"](state):
                self.assertNotIn(nxt, visiting, f"发现状态环：{state} -> {nxt}")
                if nxt not in visited:
                    dfs(nxt)
            visiting.remove(state)
            visited.add(state)

        dfs(captured["initial"])


class ScopeB1MultitaskModel(unittest.TestCase):
    """3A 欠项二完整验收（模型 A3）：双向竞态、3 子任务排空、带资源次错/未返回成功值恰好一次清理与父帧释放后安全返回。"""

    ALL_B1_NOTES = {
        MODEL.B1_CANCEL_WON_OVER_ERR,
        MODEL.B1_ERR_WON_OVER_CANCEL,
        MODEL.B1_CANCEL_WON_OVER_OK,
        MODEL.B1_OK_WON_OVER_CANCEL,
        MODEL.B1_MULTI_ERR_SECONDARY_CLEANED,
        MODEL.B1_DEINIT_ABORTED,
    }

    def test_safe_b1_policy_reaches_all_four_acceptance_classes_without_violations(self):
        policy = MODEL.SCOPE_B1_MULTITASK_POLICIES[
            "B1安全策略：提交全序、外部取消当步提交、三子任务排空、次错恰好一次清理、安全返回"
        ]
        report = MODEL.scope_b1_multitask_model(policy)
        self.assertEqual((set(), set(), self.ALL_B1_NOTES), outcome(report))

    def test_each_buggy_policy_violates_its_target_invariant(self):
        expected_violations = {
            "外部取消延迟提交": MODEL.CANCEL_WAITED_FOR_DRAIN,
            "后到事件覆盖已提交主结果": MODEL.OVERWROTE_COMMITTED_OUTCOME,
            "未返回次错或成功值泄漏资源": MODEL.UNDISCHARGED,
            "未返回次错或成功值重复清理": MODEL.DOUBLE_CLEANUP,
            "胜出返回值携带父帧借用": MODEL.DANGLING_RETURN,
            "第三子任务未终止提前释放父帧": MODEL.UAF,
            "吞掉子deinit失败": MODEL.DEINIT_FAIL_SUPPRESSED,
        }
        self.assertEqual(
            set(expected_violations) | {"B1安全策略：提交全序、外部取消当步提交、三子任务排空、次错恰好一次清理、安全返回"},
            set(MODEL.SCOPE_B1_MULTITASK_POLICIES),
        )
        for name, wanted in expected_violations.items():
            with self.subTest(policy=name):
                report = MODEL.scope_b1_multitask_model(MODEL.SCOPE_B1_MULTITASK_POLICIES[name])
                self.assertIn(wanted, report.violations)

    def test_safe_b1_state_graph_is_acyclic(self):
        policy = MODEL.SCOPE_B1_MULTITASK_POLICIES[
            "B1安全策略：提交全序、外部取消当步提交、三子任务排空、次错恰好一次清理、安全返回"
        ]
        captured = {}
        original_explore = MODEL.explore

        def capture(initial, events, violations, final_notes):
            captured["initial"] = initial
            captured["events"] = events
            return original_explore(initial, events, violations, final_notes)

        MODEL.explore = capture
        try:
            MODEL.scope_b1_multitask_model(policy)
        finally:
            MODEL.explore = original_explore

        visiting, visited = set(), set()

        def dfs(state):
            visiting.add(state)
            for _, nxt in captured["events"](state):
                self.assertNotIn(nxt, visiting, f"发现状态环：{state} -> {nxt}")
                if nxt not in visited:
                    dfs(nxt)
            visiting.remove(state)
            visited.add(state)

        dfs(captured["initial"])

    def test_cancel_winning_over_pending_ok_cannot_skip_cleanup(self):
        """仲裁前待提交成功值已持有清理义务：同轮取消胜出后，必须先清理 ok_val 才能释放父帧。"""
        policy = MODEL.SCOPE_B1_MULTITASK_POLICIES[
            "B1安全策略：提交全序、外部取消当步提交、三子任务排空、次错恰好一次清理、安全返回"
        ]
        captured = {}
        original_explore = MODEL.explore

        def capture(initial, events, violations, final_notes):
            captured["events"] = events
            return original_explore(initial, events, violations, final_notes)

        MODEL.explore = capture
        try:
            MODEL.scope_b1_multitask_model(policy)
        finally:
            MODEL.explore = original_explore

        race_state = ("join", "none", None, "done", "done", "done", False, "none", "none", "held", "alive", False, False, False)
        race_events = dict(captured["events"](race_state))
        self.assertIn("外部取消观测(当步提交cancelled)", race_events)
        self.assertIn("作用域:提交成功主结果ok", race_events)

        after_cancel = race_events["外部取消观测(当步提交cancelled)"]
        after_cancel_events = dict(captured["events"](after_cancel))
        self.assertNotIn("父:三子任务排空后释放父帧借用", after_cancel_events)
        self.assertIn("清理未返回的成功值ok资源", after_cancel_events)

        after_cleanup = after_cancel_events["清理未返回的成功值ok资源"]
        after_cleanup_events = dict(captured["events"](after_cleanup))
        self.assertIn("父:三子任务排空后释放父帧借用", after_cleanup_events)


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


class Fd0BoundedShutdownModel(unittest.TestCase):
    """OD6 FD0 停机阻塞边状态枚举（模型 G）：分离锁中毒、输出阻塞与停机信号起点（仅枚举状态，不证明真实墙钟时限）。"""

    EXPECTED = {
        "不标中毒且全阻塞边无界": (
            set(), {MODEL.FD0_SHUTDOWN_START_OUTPUT_STALLED, MODEL.FD0_SHUTDOWN_START_LOCK_STALLED}, set()),
        "仅靠LockError.Poisoned（锁与输出均无界）": (
            set(),
            {MODEL.FD0_SHUTDOWN_START_OUTPUT_STALLED, MODEL.FD0_SHUTDOWN_START_LOCK_STALLED},
            {MODEL.FD0_ALL_CHECKPOINTS_BOUNDED}),
        "锁路径全有界但主循环诊断输出无界": (
            set(), {MODEL.FD0_SHUTDOWN_START_OUTPUT_STALLED}, {MODEL.FD0_ALL_CHECKPOINTS_BOUNDED}),
        "主循环全有界但工作线程withLock无界": (
            set(), {MODEL.FD0_JOIN_ALL_STALLED}, {MODEL.FD0_ALL_CHECKPOINTS_BOUNDED}),
        "主循环与工作线程全有界但停机后锁无界": (
            set(), {MODEL.FD0_POST_JOIN_LOCK_STALLED}, {MODEL.FD0_ALL_CHECKPOINTS_BOUNDED}),
        "主循环与工作线程全有界但停机后诊断输出无界": (
            set(), {MODEL.FD0_POST_JOIN_OUTPUT_STALLED}, {MODEL.FD0_ALL_CHECKPOINTS_BOUNDED}),
        "标中毒且锁与输出全阻塞边均有界（OD6目标模型）": (
            set(), set(), {MODEL.FD0_ALL_CHECKPOINTS_BOUNDED}),
    }

    def test_each_policy_has_the_expected_outcome(self):
        self.assertEqual(set(self.EXPECTED), set(MODEL.FD0_SHUTDOWN_POLICIES))
        for name, policy in MODEL.FD0_SHUTDOWN_POLICIES.items():
            with self.subTest(policy=name):
                self.assertEqual(self.EXPECTED[name], outcome(MODEL.fd0_bounded_shutdown_model(policy)))

    def test_poisoned_lock_does_not_unblock_output_and_checkpoint1_waits_for_signal(self):
        """即使锁已标中毒，诊断输出阻塞仍会独立卡住检查点①或④；且置位 shutdownFlag 须以停机信号到达为起点。"""
        # 1. 锁已中毒 + 主循环输出无界：在持有者崩溃+输出阻塞分支上仍卡在检查点①前（输出阻塞）。
        out_loop_report = MODEL.fd0_bounded_shutdown_model(
            MODEL.FD0_SHUTDOWN_POLICIES["锁路径全有界但主循环诊断输出无界"])
        trace = out_loop_report.stuck[MODEL.FD0_SHUTDOWN_START_OUTPUT_STALLED]
        self.assertIn("场景:锁持有者异常终止+输出阻塞", trace)
        self.assertIn("停机信号到达(检查点①起点)", trace)

        # 2. 锁已中毒 + 停机后输出无界：在持有者崩溃+输出阻塞分支上仍卡在检查点④前（输出阻塞）。
        out_post_report = MODEL.fd0_bounded_shutdown_model(
            MODEL.FD0_SHUTDOWN_POLICIES["主循环与工作线程全有界但停机后诊断输出无界"])
        post_trace = out_post_report.stuck[MODEL.FD0_POST_JOIN_OUTPUT_STALLED]
        self.assertIn("场景:锁持有者异常终止+输出阻塞", post_trace)
        self.assertIn("主循环:joinAll() 返回(检查点③)", post_trace)

        # 3. 停机信号未到达前（signal == "pending"），poll_signal 不得置位 shutdownFlag。
        captured = {}
        original_explore = MODEL.explore

        def capture(initial, events, violations, final_notes):
            captured["events"] = events
            return original_explore(initial, events, violations, final_notes)

        MODEL.explore = capture
        try:
            MODEL.fd0_bounded_shutdown_model(MODEL.FD0_SHUTDOWN_POLICIES["标中毒且锁与输出全阻塞边均有界（OD6目标模型）"])
        finally:
            MODEL.explore = original_explore

        pending_events = dict(captured["events"](("live_stalled", "held", "ready", "pending", "poll_signal", "running")))
        self.assertNotIn("主循环:观测停机信号并置位 shutdownFlag(检查点①)", pending_events)
        arrived_events = dict(captured["events"](("live_stalled", "held", "ready", "arrived", "poll_signal", "running")))
        self.assertIn("主循环:观测停机信号并置位 shutdownFlag(检查点①)", arrived_events)


if __name__ == "__main__":
    unittest.main()
