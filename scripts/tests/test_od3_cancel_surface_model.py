"""OD3（3A 取消接口三方案）嵌套 scoped + 泛型 retry<T, E> 与 E2 ? 约束单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

PATH = Path(__file__).resolve().parents[1] / "loan_kernel" / "od3_cancel_surface_model.py"
SPEC = importlib.util.spec_from_file_location("od3_cancel_surface_model", PATH)
OD3 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = OD3
SPEC.loader.exec_module(OD3)


class Od3CancelSurfaceModelTests(unittest.TestCase):
    def test_ci_without_predicate_retries_cancelled_as_business_error(self):
        """OD3 §3.3：方案 C-I 仅带 FromCancelled 时，泛型 retry<T, E> 会误把取消当作业务错误重试。"""
        trace = OD3.run_ci_nested_retry("cancel_first", has_is_cancelled_predicate=False, max_attempts=3)
        self.assertEqual(3, trace.attempts)
        self.assertTrue(trace.retried_on_cancel)
        self.assertTrue(trace.cancel_turned_into_business_err)

        # 补加额外 IsCancelled 谓词接口后才可避免重试，但仍对 E 施加额外接口约束。
        fixed = OD3.run_ci_nested_retry("cancel_first", has_is_cancelled_predicate=True, max_attempts=3)
        self.assertEqual(1, fixed.attempts)
        self.assertFalse(fixed.retried_on_cancel)
        self.assertTrue(fixed.required_extra_e_trait)

    def test_ciia_and_ciib_preserve_cancel_and_never_retry_with_zero_e_constraints(self):
        """OD3 §3.3：C-II-a 与 C-II-b 对 E 零约束，取消先提交时仅执行 1 次并原样保留 Cancelled；两者在 ? 原生传播上不等价。"""
        ta = OD3.run_ciia_nested_retry("cancel_first", max_attempts=3)
        tb = OD3.run_ciib_nested_retry("cancel_first", max_attempts=3)

        self.assertEqual(1, ta.attempts)
        self.assertEqual(OD3.Cancelled(), ta.outcome)
        self.assertFalse(ta.retried_on_cancel)
        self.assertFalse(ta.required_extra_e_trait)
        self.assertFalse(ta.native_question_mark_for_cancel)

        self.assertEqual(1, tb.attempts)
        self.assertEqual(OD3.Err(OD3.Cancelled()), tb.outcome)
        self.assertFalse(tb.retried_on_cancel)
        self.assertFalse(tb.required_extra_e_trait)
        self.assertTrue(tb.native_question_mark_for_cancel)

    def test_err_first_preserves_primary_business_error_and_allows_retry(self):
        """OD2 B1 + OD3：业务错误 err_A 先提交时保留确切业务错误并允许泛型重试中间件按策略重试。"""
        ta = OD3.run_ciia_nested_retry("err_first", max_attempts=3)
        tb = OD3.run_ciib_nested_retry("err_first", max_attempts=3)
        self.assertEqual(3, ta.attempts)
        self.assertEqual(OD3.Err("AppError.IoTimeout"), ta.outcome)
        self.assertEqual(3, tb.attempts)
        self.assertEqual(OD3.Ok(OD3.Err("AppError.IoTimeout")), tb.outcome)

    def test_e2_question_mark_forbids_implicit_cancel_to_business_error(self):
        """之十一 E2 + OD3 硬约束：? 错误转换不得将 Cancelled 隐式转为业务错误 E。"""
        self.assertIsNotNone(OD3.check_e2_question_mark_conversion("Cancelled", "AppError"))
        self.assertIsNone(OD3.check_e2_question_mark_conversion("IoError", "AppError"))
        self.assertIsNone(OD3.check_e2_question_mark_conversion("Cancelled", "Cancelled"))


if __name__ == "__main__":
    unittest.main()
