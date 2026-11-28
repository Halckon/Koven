"""v1 冻结前兼容性约束（Z4、Z5、Z6、Z8、Z9、Z11）与真实 conformance/expectations.json 审计单测。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
PATH = REPO_ROOT / "scripts" / "loan_kernel" / "v1_freeze_audit.py"
SPEC = importlib.util.spec_from_file_location("v1_freeze_audit", PATH)
Z = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = Z
SPEC.loader.exec_module(Z)


class V1FreezeAuditTests(unittest.TestCase):
    def test_z9_live_conformance_expectations_audit_and_clean_target(self):
        """Z9 验收第一步：只读核对真实 conformance/expectations.json 并检出内部产物引用与内嵌 known_unimplemented。"""
        live = Z.audit_conformance_expectations(REPO_ROOT / "conformance" / "expectations.json")
        self.assertGreater(live.total_entries, 50)
        self.assertGreater(len(live.known_unimplemented_ids), 0)
        self.assertGreater(len(live.internal_marker_ids), 0)
        self.assertGreater(len(live.diagnostic_symbol_ids), 0)
        self.assertFalse(live.is_z9_clean)

        clean_matrix = {
            "schema": "koven.conformance-expectations.v2",
            "probes": {
                "ok_case": {"kind": "run_ok", "stdout": "hi\n"},
                "err_case": {"kind": "compile_fail", "diagnostic": "L0084"},
            },
            "programs": {},
        }
        clean = Z.audit_conformance_expectations_data(clean_matrix)
        self.assertTrue(clean.is_z9_clean)
        self.assertEqual(2, clean.total_entries)

    def test_z4_hashable_opaque_in_v1(self):
        self.assertIsNone(Z.check_z4_hashable_usage("structural_derive"))
        self.assertIsNone(Z.check_z4_hashable_usage("generic_bound"))
        self.assertIsNotNone(Z.check_z4_hashable_usage("manual_impl"))
        self.assertIsNotNone(Z.check_z4_hashable_usage("call_hash_code_member"))

    def test_z5_explicit_stream_close_and_deinit_best_effort(self):
        s1 = Z.StreamHandle("f1")
        self.assertEqual(("Ok(Unit)", None), s1.close())
        status, err = s1.close()
        self.assertEqual("ERR", status)
        self.assertIn("Z5_DOUBLE_CLOSE", err)

        s2 = Z.StreamHandle("f2")
        self.assertEqual(("Err(IoError)", None), s2.close(fail_io=True))
        s2.deinit(fail_io=True)
        self.assertEqual(1, s2.close_attempts)

        s3 = Z.StreamHandle("f3")
        s3.deinit(fail_io=True)
        self.assertFalse(s3.open)
        self.assertTrue(s3.last_close_error_swallowed)

    def test_z6_main_result_error_requires_only_display(self):
        self.assertIsNone(Z.check_z6_main_error_bounds(frozenset({"Display"})))
        self.assertIsNotNone(Z.check_z6_main_error_bounds(frozenset()))
        self.assertIsNotNone(Z.check_z6_main_error_bounds(frozenset({"Display", "Error"})))

    def test_z8_manifest_language_version_three_cases(self):
        ver, warn, err = Z.check_z8_manifest_language_version({})
        self.assertEqual("1.0", ver)
        self.assertIsNotNone(warn)
        self.assertIsNone(err)

        ver2, warn2, err2 = Z.check_z8_manifest_language_version({"language_version": "1.0"})
        self.assertEqual(("1.0", None, None), (ver2, warn2, err2))

        ver3, warn3, err3 = Z.check_z8_manifest_language_version({"language_version": "9.9"})
        self.assertIsNone(ver3)
        self.assertIsNone(warn3)
        self.assertIn("Z8_UNKNOWN_VERSION", err3)
        self.assertIn("1.0", err3)

    def test_z11_directory_list_fails_whole_call_on_non_utf8(self):
        ok, err = Z.check_z11_directory_list((b"a.txt", b"b.kv"))
        self.assertEqual(["a.txt", "b.kv"], ok)
        self.assertIsNone(err)

        bad, err2 = Z.check_z11_directory_list((b"a.txt", b"\xffinvalid"))
        self.assertIsNone(bad)
        self.assertIn("Z11_NON_UTF8_FILENAME", err2)


if __name__ == "__main__":
    unittest.main()
