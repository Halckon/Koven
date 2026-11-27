"""U0 AL1/AL2/AL7 与 UB1/UB2/UB4/UB5/FS8 可执行参考解释器单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1] / "loan_kernel"
SPEC = importlib.util.spec_from_file_location("loan_kernel_u0_raw_model", ROOT / "u0_raw_model.py")
U0 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = U0
SPEC.loader.exec_module(U0)


class U0RawModelTests(unittest.TestCase):
    def test_al1_ub1_shared_derived_write_rejected_on_non_cell(self):
        """AL1 / UB1：对非 RawCell 类型经共享借用派生裸指针执行写入必须报 UB1。"""
        interp = U0.U0RawInterpreter()
        interp.alloc_owner("x", initialized=True, is_cell=False)
        shared_loan = interp.borrow("x", "Shared")
        ptr = interp.address_of("x", derived_mode="Shared", derived_loan_id=shared_loan)
        interp.raw_read(ptr)
        self.assertEqual([], interp.violations)
        interp.raw_write(ptr)
        self.assertEqual(["UB1"], [v.code for v in interp.violations])

    def test_al1_al7_rawcell_allows_shared_and_cell_write_unless_exclusive_active(self):
        """AL7：RawCell 允许经 Shared 或 Cell 路径写入，但两条路径均不得与存活 Exclusive loan 冲突。"""
        for mode in ("Shared", "Cell"):
            with self.subTest(mode=mode):
                interp = U0.U0RawInterpreter()
                interp.alloc_owner("cell", initialized=True, is_cell=True)
                shared_loan = interp.borrow("cell", "Shared")
                ptr = interp.address_of("cell", derived_mode=mode, derived_loan_id=shared_loan)
                interp.raw_write(ptr)
                self.assertEqual([], interp.violations)
                interp.end_loan(shared_loan)

                excl_loan = interp.borrow("cell", "Exclusive")
                interp.raw_write(ptr)
                self.assertEqual(["UB1"], [v.code for v in interp.violations])
                interp.end_loan(excl_loan)

    def test_al1_exclusive_derived_ptr_allows_own_ancestor_blocks_conflicting_child(self):
        """AL1：inout 派生裸指针不与自身祖先独占 loan 冲突，但与存活子 loan 冲突（UB1），子 loan 结束后恢复合法。"""
        interp = U0.U0RawInterpreter()
        interp.alloc_owner("pair", fields=("a", "b"), initialized=True)
        excl_parent = interp.borrow("pair", "Exclusive")
        ptr_a = interp.address_of("pair.a", derived_mode="Exclusive", derived_loan_id=excl_parent)
        interp.raw_write(ptr_a)
        self.assertEqual([], interp.violations)

        child = interp.borrow("pair.a", "Shared", parent_id=excl_parent)
        interp.raw_write(ptr_a)
        self.assertEqual(["UB1"], [v.code for v in interp.violations])

        interp.violations.clear()
        interp.end_loan(child)
        interp.raw_write(ptr_a)
        self.assertEqual([], interp.violations)
        interp.end_loan(excl_parent)
        interp.exit_scope()
        self.assertEqual([], interp.violations)

    def test_al1_ub1_read_conflicts_with_unrelated_exclusive_loan(self):
        """AL1 / UB1：持有无关 Exclusive loan 期间经裸指针读重叠字段报 UB1。"""
        interp = U0.U0RawInterpreter()
        interp.alloc_owner("buf", fields=("head",), initialized=True)
        ptr = interp.address_of("buf.head", derived_mode="Shared")
        excl = interp.borrow("buf", "Exclusive")
        interp.raw_read(ptr)
        self.assertEqual(["UB1"], [v.code for v in interp.violations])
        interp.end_loan(excl)

    def test_al2_address_of_installs_drop_barrier_and_catches_ub2_after_drop_or_move(self):
        """AL2 / U1 R1.2：addressOf 建立 place 级析构屏障阻止提前析构；析构或整体移出后解引用报 UB2。"""
        interp = U0.U0RawInterpreter()
        interp.alloc_owner("x", initialized=True)
        ptr = interp.address_of("x", derived_mode="Shared")
        interp.drop_owner("x", early=True)
        self.assertEqual(["AL2_BARRIER"], [v.code for v in interp.violations])
        self.assertTrue(interp.owners["x"].alive)

        interp.violations.clear()
        interp.exit_scope()
        self.assertEqual([], interp.violations)
        interp.raw_read(ptr)
        self.assertEqual(["UB2"], [v.code for v in interp.violations])

        interp2 = U0.U0RawInterpreter()
        interp2.alloc_owner("y", initialized=True)
        ptr_y = interp2.address_of("y", derived_mode="Exclusive")
        interp2.move_owner("y")
        interp2.raw_write(ptr_y)
        self.assertEqual(["UB2"], [v.code for v in interp2.violations])

    def test_al2_ub5_uninit_alloc_and_ptr_read_write_lifecycle(self):
        """AL2 / MM7 / UB5：刚分配未初始化存储、ptr_read 后未回填读取/析构，以及聚合根与子字段的双向未初始化检查。"""
        # 1. 刚分配未初始化存储直接读取 -> UB5
        interp_uninit = U0.U0RawInterpreter()
        interp_uninit.alloc_owner("raw_buf", initialized=False)
        p_raw = interp_uninit.address_of("raw_buf", derived_mode="Exclusive")
        interp_uninit.raw_read(p_raw)
        self.assertEqual(["UB5"], [v.code for v in interp_uninit.violations])

        # 2. 合法 replace（含资源句柄）：ptr_read 移交旧句柄，ptr_write 分配新句柄，exit_scope 零违规（不误报 UB4）
        interp_ok = U0.U0RawInterpreter()
        interp_ok.alloc_owner("slot", initialized=True, resource_handle=True)
        old_handle = interp_ok.owners["slot"].handle_id
        p_slot = interp_ok.address_of("slot", derived_mode="Exclusive")
        interp_ok.ptr_read(p_slot, into_owner="taken")
        self.assertEqual("Uninit", interp_ok.owners["slot"].slots["slot"])
        self.assertIsNone(interp_ok.owners["slot"].handle_id)
        self.assertEqual(old_handle, interp_ok.owners["taken"].handle_id)
        interp_ok.ptr_write(p_slot)
        self.assertEqual("Init", interp_ok.owners["slot"].slots["slot"])
        self.assertIsNotNone(interp_ok.owners["slot"].handle_id)
        self.assertNotEqual(old_handle, interp_ok.owners["slot"].handle_id)
        interp_ok.exit_scope()
        self.assertEqual([], interp_ok.violations)

        # 3. ptr_read 移出后未回填就再次读取或直接离开作用域析构 -> UB5
        interp_bad = U0.U0RawInterpreter()
        interp_bad.alloc_owner("slot", initialized=True)
        p_bad = interp_bad.address_of("slot", derived_mode="Exclusive")
        interp_bad.ptr_read(p_bad)
        interp_bad.raw_read(p_bad)
        interp_bad.exit_scope()
        self.assertEqual(["UB5", "UB5"], [v.code for v in interp_bad.violations])

        # 4. 移出字段 pair.a 后读取整个聚合 pair -> UB5
        interp_agg1 = U0.U0RawInterpreter()
        interp_agg1.alloc_owner("pair", fields=("a", "b"), initialized=True)
        p_a = interp_agg1.address_of("pair.a", derived_mode="Exclusive")
        p_whole = interp_agg1.address_of("pair", derived_mode="Exclusive")
        interp_agg1.ptr_read(p_a)
        interp_agg1.raw_read(p_whole)
        self.assertEqual(["UB5"], [v.code for v in interp_agg1.violations])

        # 5. 移出整个聚合 pair 后读取字段 pair.a -> UB5
        interp_agg2 = U0.U0RawInterpreter()
        interp_agg2.alloc_owner("pair", fields=("a", "b"), initialized=True)
        p_whole2 = interp_agg2.address_of("pair", derived_mode="Exclusive")
        p_a2 = interp_agg2.address_of("pair.a", derived_mode="Exclusive")
        interp_agg2.ptr_read(p_whole2)
        interp_agg2.raw_read(p_a2)
        self.assertEqual(["UB5"], [v.code for v in interp_agg2.violations])

    def test_al1_ub1_shared_loan_cannot_escalate_to_exclusive_raw_ptr(self):
        """AL1 / UB1：实际来源为 Shared loan 时，即使调用方将裸指针标为 Exclusive，写入仍须按真实派生权限报 UB1。"""
        interp = U0.U0RawInterpreter()
        interp.alloc_owner("x", initialized=True, is_cell=False)
        shared_loan = interp.borrow("x", "Shared")
        forged_ptr = interp.address_of("x", derived_mode="Exclusive", derived_loan_id=shared_loan)
        interp.raw_write(forged_ptr)
        self.assertEqual(["UB1"], [v.code for v in interp.violations])

        # 嵌套派生链：Exclusive 父 loan 派生 Shared 子 loan，再由子 loan 派生裸指针并标为 Exclusive，仍须报 UB1
        interp2 = U0.U0RawInterpreter()
        interp2.alloc_owner("y", initialized=True, is_cell=False)
        excl_parent = interp2.borrow("y", "Exclusive")
        shared_child = interp2.borrow("y", "Shared", parent_id=excl_parent)
        forged_nested = interp2.address_of("y", derived_mode="Exclusive", derived_loan_id=shared_child)
        interp2.raw_write(forged_nested)
        self.assertEqual(["UB1"], [v.code for v in interp2.violations])

    def test_al2_ub2_old_ptr_rejected_after_same_name_reallocation(self):
        """AL2 / UB2：旧 owner 析构后重新分配同名存储，旧指针仍须因分配实例不匹配报 UB2。"""
        interp = U0.U0RawInterpreter()
        interp.alloc_owner("x", initialized=True)
        old_ptr = interp.address_of("x", derived_mode="Exclusive")
        interp.exit_scope()
        self.assertEqual([], interp.violations)

        # 同名重新分配新实例
        interp.alloc_owner("x", initialized=True)
        interp.raw_read(old_ptr)
        interp.raw_write(old_ptr)
        self.assertEqual(["UB2", "UB2"], [v.code for v in interp.violations])

    def test_fs8_ub4_double_drop_and_safe_insert_order(self):
        """AL3 / FS3 / FS8 / UB4：复现 U0 §4 列表插入的错误次序（失败展开触发 UB4）vs 正确次序（零违规）。"""
        bad_fail = U0.simulate_fs8_insert_order(shift_before_clone=True, clone_fails=True)
        self.assertEqual(["UB4"], [v.code for v in bad_fail])

        bad_ok = U0.simulate_fs8_insert_order(shift_before_clone=True, clone_fails=False)
        self.assertEqual([], bad_ok)

        good_fail = U0.simulate_fs8_insert_order(shift_before_clone=False, clone_fails=True)
        self.assertEqual([], good_fail)

        good_ok = U0.simulate_fs8_insert_order(shift_before_clone=False, clone_fails=False)
        self.assertEqual([], good_ok)


if __name__ == "__main__":
    unittest.main()
