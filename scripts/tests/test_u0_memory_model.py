"""U0 §5（MM1–MM6）有界执行图内存模型单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "u0_memory_model.py"
SPEC = importlib.util.spec_from_file_location("u0_memory_model", PATH)
MM = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MM
SPEC.loader.exec_module(MM)


class U0MemoryModelTests(unittest.TestCase):
    def test_mm1_mm4_unsynchronized_write_read_is_data_race(self):
        """MM1 / MM4：无同步非原子一写一读构成 UB6 数据竞争，没有良性竞争例外。"""
        g = MM.ExecutionGraph(
            "unsync_race",
            (
                MM.MemEvent("w1", "T1", 1, "write", "x", value=1),
                MM.MemEvent("r1", "T2", 1, "read", "x", value=1),
            ),
        )
        self.assertEqual([("r1", "w1")], MM.detect_data_races(g))

    def test_mm3_message_passing_relaxed_races_but_release_acquire_is_race_free(self):
        """MM3：Relaxed 消息传递不建立 happens-before 仍报数据竞争；Release->Acquire 建立 hb 后无竞争。"""
        mp_relaxed = MM.ExecutionGraph(
            "mp_relaxed",
            (
                MM.MemEvent("w_data", "T1", 1, "write", "data", value=42),
                MM.MemEvent("w_flag", "T1", 2, "atomic_write", "flag", order="Relaxed", value=1),
                MM.MemEvent("r_flag", "T2", 1, "atomic_read", "flag", order="Relaxed", value=1, reads_from="w_flag"),
                MM.MemEvent("r_data", "T2", 2, "read", "data", value=42),
            ),
        )
        self.assertEqual([("r_data", "w_data")], MM.detect_data_races(mp_relaxed))

        mp_rel_acq = MM.ExecutionGraph(
            "mp_rel_acq",
            (
                MM.MemEvent("w_data", "T1", 1, "write", "data", value=42),
                MM.MemEvent("w_flag", "T1", 2, "atomic_write", "flag", order="Release", value=1),
                MM.MemEvent("r_flag", "T2", 1, "atomic_read", "flag", order="Acquire", value=1, reads_from="w_flag"),
                MM.MemEvent("r_data", "T2", 2, "read", "data", value=42),
            ),
        )
        self.assertEqual([], MM.detect_data_races(mp_rel_acq))

    def test_mm2_spawn_join_channel_mutex_establish_happens_before(self):
        """MM2：spawn / join / channel send->recv / mutex unlock->lock 均建立跨线程 happens-before。"""
        for tag in ("spawn", "join", "channel", "mutex"):
            with self.subTest(sync=tag):
                g = MM.ExecutionGraph(
                    f"sync_{tag}",
                    (
                        MM.MemEvent("w_buf", "T1", 1, "write", "buf", value=7),
                        MM.MemEvent("rel", "T1", 2, "sync_rel", tag, sync_tag=tag),
                        MM.MemEvent("acq", "T2", 1, "sync_acq", tag, sync_tag=tag),
                        MM.MemEvent("r_buf", "T2", 2, "read", "buf", value=7),
                    ),
                )
                self.assertEqual([], MM.detect_data_races(g))

    def test_mm6_volatile_orders_volatile_only_and_does_not_synchronize_threads(self):
        """MM6：易失访问之间禁止重排，但与普通非原子访问可重排，且不提供跨线程 happens-before。"""
        v1 = MM.MemEvent("v1", "T1", 1, "volatile_write", "mmio_ctrl", order="Volatile", value=1)
        v2 = MM.MemEvent("v2", "T1", 2, "volatile_write", "mmio_go", order="Volatile", value=1)
        plain = MM.MemEvent("p1", "T1", 0, "write", "dma_buf", value=99)
        self.assertFalse(MM.check_volatile_reorder_allowed(v1, v2))
        self.assertTrue(MM.check_volatile_reorder_allowed(plain, v1))

        volatile_mp = MM.ExecutionGraph(
            "volatile_mp",
            (
                MM.MemEvent("w_data", "T1", 1, "write", "data", value=42),
                MM.MemEvent("v_flag", "T1", 2, "volatile_write", "flag", order="Volatile", value=1),
                MM.MemEvent("vr_flag", "T2", 1, "volatile_read", "flag", order="Volatile", value=1, reads_from="v_flag"),
                MM.MemEvent("r_data", "T2", 2, "read", "data", value=42),
            ),
        )
        races = MM.detect_data_races(volatile_mp)
        self.assertIn(("r_data", "w_data"), races)
        self.assertIn(("v_flag", "vr_flag"), races)


if __name__ == "__main__":
    unittest.main()
