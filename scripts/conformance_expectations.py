#!/usr/bin/env python3
"""Expectations schema validation, static catalog invariant checks, and static derivation."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
from typing import Any

from conformance_catalogs import (
    CONFIRMED_STDOUT_ORACLES,
    DIAGNOSTIC_SYMBOL_CATALOG,
    EXPECTED_MULTI_DIAGNOSTICS,
    GUIDE_CHAPTER_DOMAINS,
    MAIN_ENTRY_RE,
    PACKAGE_DIRECTIVE_RE,
    PROBE_BATCH_CATALOG,
    PROBE_GUIDE_CHAPTERS,
    PROGRAM_GUIDE_CHAPTERS,
    PROGRAM_ORACLE_DERIVATIONS,
    PROGRAM_PREREQUISITE_PROBES,
    PROGRAM_README_CATALOG,
    PROGRAM_ROADMAP_BLOCKERS,
    SCHEMA_EXPECTATIONS,
    SEMANTIC_ORACLE_DERIVATIONS,
    SIMPLE_PRINTLN_LITERAL_RE,
    UNIMPLEMENTED_ROADMAP_BLOCKERS,
    V046_PROBE_OVERRIDES,
    VALID_EXPECTATION_KINDS,
    VALID_GUIDE_CHAPTERS,
    VALID_ORACLE_ORIGINS,
    VALID_UNIMPLEMENTED_MARKERS,
    classify_probe_batch,
    classify_probe_guide_chapter,
)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def resolve_spec_doc_relpath(spec_id: str, repo_root: Path) -> tuple[str, str] | None:
    """Locate SPEC-xxxx in docs/specs/active or docs/archive/specs and return (relpath, status)."""
    if not re.fullmatch(r"SPEC-[0-9]{4}", spec_id):
        return None
    num = spec_id.split("-", 1)[1]
    active_dir = repo_root / "docs" / "specs" / "active"
    archive_dir = repo_root / "docs" / "archive" / "specs"
    if active_dir.is_dir():
        matches = sorted(active_dir.glob(f"{num}-*.md"))
        if matches:
            return f"docs/specs/active/{matches[0].name}", "active"
    if archive_dir.is_dir():
        matches = sorted(archive_dir.glob(f"{num}-*.md"))
        if matches:
            return f"docs/archive/specs/{matches[0].name}", "archived"
    return None


GUIDE_LITMUS_CATALOG: dict[int, dict[str, Any]] = {
    1: {
        "title": "基础函数与局部变量",
        "guide_chapter": "05-declarations-callables.md",
        "probes": ["p01_int_print", "p24_recursion", "q10_var_reassign_string", "v01_assign_unit_value"],
    },
    2: {
        "title": "换行分句与表达式续行",
        "guide_chapter": "04-expressions-operators.md",
        "probes": ["p03_long_uint", "t07_short_circuit_plain", "v01_assign_unit_value", "v02_assign_chain"],
    },
    3: {
        "title": "控制流与循环结构",
        "guide_chapter": "06-blocks-control-flow.md",
        "probes": ["p07_while_var", "p48_nested_loops_break", "q13_if_expr_value", "v04_stmt_if_mixed"],
    },
    4: {
        "title": "枚举类型与穷尽 when 匹配",
        "guide_chapter": "06-blocks-control-flow.md",
        "probes": ["e01_enum_when_bool_return", "e04_enum_when_int_return", "p09_enum_payload", "p49_when_enum_no_payload", "q08_enum_string_payload"],
    },
    5: {
        "title": "类、值类与伴随对象",
        "guide_chapter": "08-class-family-members.md",
        "probes": ["p12_class_inout", "p28_companion", "p44_method_on_value", "q12_companion_const"],
    },
    6: {
        "title": "泛型、接口与单态化",
        "guide_chapter": "03-types-generics.md",
        "probes": ["p10_generic_fn", "p11_interface_bound", "q16_interface_static", "t03_vmethod_generic"],
    },
    7: {
        "title": "线性所有权与移动语义",
        "guide_chapter": "10-ownership-borrowing-drop.md",
        "probes": ["p21_box_enum", "p25_linked_iter", "p32_deinit", "q19_deep_return_cleanup"],
    },
    8: {
        "title": "借用检查与在位可变引用",
        "guide_chapter": "10-ownership-borrowing-drop.md",
        "probes": ["p12c_inout_param", "p26_borrow_return", "q09_inout_class_param", "q20_replace_field"],
    },
    9: {
        "title": "空安全、流类型与非空断言",
        "guide_chapter": "09-nullability-errors.md",
        "probes": ["p14_nullable", "p14b_nullable_simple", "q05_nullable_param", "q06_nullable_return_bang", "x02_string_nullable_return"],
    },
    10: {
        "title": "函数类型与闭包捕获",
        "guide_chapter": "07-calls-lambdas-closures.md",
        "probes": ["p17_lambda", "p17b_lambda_nocapture", "p18_move_closure", "p42_lambda_mutate", "q01_lambda_local", "q02_lambda_arg_inline"],
    },
    11: {
        "title": "顺序集合遍历与结构解构",
        "guide_chapter": "12-collections-destructuring.md",
        "probes": ["p19_mutablelist", "p34_for_range", "p38_list_of_strings", "q15_borrow_param_string_list", "w01_map_for_sum"],
    },
    12: {
        "title": "关联常量与标准库互操作",
        "guide_chapter": "13-program-runtime-standard-library.md",
        "probes": ["p31_string_ops", "q12_companion_const", "s01_result_builtin"],
    },
}


GUIDE_VERSION_BOUNDARY_CATALOG: dict[str, dict[str, Any]] = {
    "v0.44": {
        "summary": "封闭 if/when join、赋值返回 Unit、while 事实、IntRange/LongRange 与 for、八整数 checked 转换、插入序 Map 遍历",
        "probes": [
            "v01_assign_unit_value",
            "v02_assign_chain",
            "v03_join_nullable",
            "v04_stmt_if_mixed",
            "v06_for_range_sum",
            "v07_range_max",
            "v08_long_range",
            "v11_convert_ok",
            "v12_convert_abort",
            "v13_while_nullable_int",
            "v15_map_for",
            "w01_map_for_sum",
            "w02_map_for_order",
            "w06_map_mutate_in_loop",
        ],
    },
    "v0.45": {
        "summary": "单文件与 project 统一 compilation-unit 管线、显式 koven.algorithms.take 导入、enum subject when 两模式零差异",
        "probes": [
            "e01_enum_when_bool_return",
            "e02_enum_when_stmt_println",
            "e03_enum_when_bool_val",
            "e04_enum_when_int_return",
            "p49_when_enum_no_payload",
            "p50_enum_method",
            "y01_import_take_for_sum",
            "y02_take_no_import",
            "y03_method_take_for",
            "y05_take_borrow_val_call",
        ],
    },
    "v0.46": {
        "summary": "Abort 输出合同 (SPEC-0355)、Result 内建与无参 is Result.Ok (SPEC-0349)、顶层仅 const val (SPEC-0343)、匿名 companion { }",
        "probes": [
            "p02_interp",
            "p04_overflow",
            "p16_result_q",
            "p23_global_var",
            "p27_thread",
            "p28_companion",
            "p35_value_eq",
            "p37_is_generic_case",
            "p45_abort_message",
            "q12_companion_const",
            "s01_result_builtin",
            "v12_convert_abort",
        ],
    },
    "v0.47": {
        "summary": "Guide 13 文本字节基础、可信标准 String Borrow 扩展与 Result<String, String> 载荷 when 闭环 (SPEC-0368)",
        "probes": [
            "p16b_result_string",
            "p31_string_ops",
            "p46_string_field_class",
            "q10_var_reassign_string",
            "x02_string_nullable_return",
        ],
    },
}

V3_SANITIZER_RESOURCE_CATALOG: dict[str, dict[str, Any]] = {
    "deterministic_deinit_drop": {
        "title": "确定性 `deinit` 析构与提前返回栈展开清理",
        "sanitizer_focus": "ASan `drop` (heap-use-after-free) / LSan 零泄漏",
        "probes": [
            "p32_deinit",
            "q19_deep_return_cleanup",
        ],
        "programs": ["raii_state_machine"],
    },
    "heap_class_rc_and_box": {
        "title": "堆分配 `class`、`Rc<T>` 共享引用计数、`Box<T>` 递归枚举与 `replace`",
        "sanitizer_focus": "ASan `user` / `runtime` 与 LSan `leak` 对照",
        "probes": [
            "p12_class_inout",
            "p12b_class_field_assign",
            "p21_box_enum",
            "p22_rc",
            "p25b_linked_recursive",
            "p40_nested_generic_class",
            "p44b_method_on_class",
            "p46_string_field_class",
            "q04_rc_simple",
            "q09_inout_class_param",
            "q20_replace_field",
        ],
        "programs": ["expr_eval", "linked_stack"],
    },
    "string_and_text_owner": {
        "title": "堆拥有 `String` 克隆、拼接、重赋值、枚举行李与可空返回",
        "sanitizer_focus": "ASan 堆缓冲区边界 / LSan 字符串所有权释放",
        "probes": [
            "p02_interp",
            "p16b_result_string",
            "p26_borrow_return",
            "p31_string_ops",
            "p41_when_string",
            "q08_enum_string_payload",
            "q10_var_reassign_string",
            "x02_string_nullable_return",
        ],
        "programs": ["config_parse"],
    },
    "collection_list_and_map_provider": {
        "title": "`List` / `MutableList` / `Map` / `MutableMap` 扩容、覆盖、删除与遍历析构",
        "sanitizer_focus": "SPEC-0288 `map-fixture` 与 12 组 `provider-fixtures` (`singleton`/`overwrite`/`growth`)",
        "probes": [
            "p19_mutablelist",
            "p38_list_of_strings",
            "p47_list_index_string",
            "q15_borrow_param_string_list",
            "t05_map_loop_continue",
            "t10_map_nested_if_break",
            "u02_list_add_loop_string",
            "v15_map_for",
            "v22_range_with_map",
            "w01_map_for_sum",
            "w02_map_for_order",
            "w03_map_overwrite_remove",
            "w08_map_string_values",
            "w09_map_temp_source",
            "w11_word_count",
        ],
        "programs": ["word_count"],
    },
    "closure_env_and_thread_spawn": {
        "title": "捕获闭包环境、`move` 闭包所有权转移与 `thread(move { ... })`",
        "sanitizer_focus": "ASan 闭包环境生命周期 / 跨线程 `Send` 所有权传递",
        "probes": [
            "p17_lambda",
            "p18_move_closure",
            "p27_thread",
            "q01_lambda_local",
            "q02_lambda_arg_inline",
        ],
        "programs": [],
    },
    "checked_abort_and_overflow": {
        "title": "受检整数溢出、`error(msg)` 与显式数值转换 `SIGABRT` 中止边界",
        "sanitizer_focus": "Guide 09 D12 `abort: <msg>` + 栈帧确定性退出",
        "probes": [
            "p04_overflow",
            "p45_abort_message",
            "v12_convert_abort",
        ],
        "programs": ["varint_codec"],
    },
}

def load_expectations(path: Path, corpus_root: Path | None = None) -> dict[str, Any]:
    """Load and validate machine-readable conformance expectations."""
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("schema") != SCHEMA_EXPECTATIONS:
        raise ValueError(f"invalid expectations schema in {path}")
    for section in ("probes", "programs"):
        items = data.get(section)
        if not isinstance(items, dict) or not items:
            raise ValueError(f"expectations.{section} must be a non-empty object")
        for name, spec in items.items():
            if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9_]+", name):
                raise ValueError(f"invalid item name in {section}: {name!r}")
            if not isinstance(spec, dict) or "kind" not in spec or "source_sha256" not in spec:
                raise ValueError(f"{section}.{name} requires kind and source_sha256")
            kind = spec["kind"]
            if kind not in VALID_EXPECTATION_KINDS:
                raise ValueError(f"{section}.{name} has unsupported kind {kind!r}")
            if not isinstance(spec["source_sha256"], str) or not re.fullmatch(
                r"[0-9a-f]{64}", spec["source_sha256"]
            ):
                raise ValueError(f"{section}.{name} has invalid source_sha256")
            if kind == "run_ok":
                if section == "programs":
                    if "expected_file" not in spec or "expected_sha256" not in spec:
                        raise ValueError(
                            f"programs.{name} requires expected_file and expected_sha256"
                        )
                elif (
                    not isinstance(spec.get("stdout"), str)
                    or not spec["stdout"]
                    or not spec["stdout"].endswith("\n")
                ):
                    raise ValueError(
                        f"probes.{name} with run_ok requires non-empty stdout ending with newline"
                    )
                if section == "probes":
                    out_text = spec["stdout"]
                    out_bytes = out_text.encode("utf-8")
                    if "expected_lines" in spec and spec["expected_lines"] != len(out_text.splitlines()):
                        raise ValueError(
                            f"expected_lines mismatch for probes.{name}: expected {spec['expected_lines']}, got {len(out_text.splitlines())}"
                        )
                    if "expected_bytes" in spec and spec["expected_bytes"] != len(out_bytes):
                        raise ValueError(
                            f"expected_bytes mismatch for probes.{name}: expected {spec['expected_bytes']}, got {len(out_bytes)}"
                        )
            elif kind == "compile_fail":
                if any(
                    k in spec
                    for k in ("stdout", "target_stdout", "expected_lines", "expected_bytes")
                ):
                    raise ValueError(
                        f"{section}.{name} with compile_fail must not define stdout/expected_lines/expected_bytes"
                    )
                if not isinstance(spec.get("diagnostic"), str) or not re.fullmatch(
                    r"L[0-9]{4}", spec["diagnostic"]
                ):
                    raise ValueError(
                        f"{section}.{name} with compile_fail requires diagnostic Lxxxx"
                    )
                if "diagnostic_symbol" in spec:
                    expected_sym = DIAGNOSTIC_SYMBOL_CATALOG.get(spec["diagnostic"])
                    if not isinstance(spec["diagnostic_symbol"], str) or (
                        expected_sym is not None and spec["diagnostic_symbol"] != expected_sym
                    ):
                        raise ValueError(
                            f"{section}.{name} has invalid diagnostic_symbol {spec['diagnostic_symbol']!r} for {spec['diagnostic']}"
                        )
                if "all_diagnostics" in spec:
                    all_diags = spec["all_diagnostics"]
                    if (
                        not isinstance(all_diags, list)
                        or not all_diags
                        or not all(
                            isinstance(c, str) and re.fullmatch(r"L[0-9]{4}", c)
                            for c in all_diags
                        )
                        or all_diags[0] != spec["diagnostic"]
                    ):
                        raise ValueError(
                            f"{section}.{name} has invalid all_diagnostics {all_diags!r}"
                        )
            elif kind == "runtime_abort":
                if any(
                    k in spec
                    for k in ("stdout", "target_stdout", "expected_lines", "expected_bytes")
                ):
                    raise ValueError(
                        f"{section}.{name} with runtime_abort must not define stdout/expected_lines/expected_bytes"
                    )
                if not isinstance(spec.get("abort_message"), str) or not spec["abort_message"]:
                    raise ValueError(
                        f"{section}.{name} with runtime_abort requires abort_message"
                    )
            elif kind == "known_unimplemented":
                allowed = spec.get("allowed_markers")
                if (
                    not isinstance(allowed, list)
                    or not allowed
                    or len(allowed) != len(set(allowed))
                    or not all(
                        isinstance(m, str) and m in VALID_UNIMPLEMENTED_MARKERS
                        for m in allowed
                    )
                ):
                    raise ValueError(
                        f"{section}.{name} with known_unimplemented has invalid allowed_markers {allowed!r}"
                    )
                bm = spec.get("baseline_marker")
                if (
                    isinstance(bm, str)
                    and not spec.get("v046_baseline_exempt", False)
                    and bm in VALID_UNIMPLEMENTED_MARKERS
                    and bm not in allowed
                ):
                    raise ValueError(
                        f"{section}.{name} with known_unimplemented must include non-exempt baseline_marker {bm!r} in allowed_markers"
                    )
                if "target_stdout" in spec:
                    ts = spec["target_stdout"]
                    if not isinstance(ts, str) or not ts or not ts.endswith("\n"):
                        raise ValueError(
                            f"{section}.{name} with known_unimplemented has invalid target_stdout {ts!r} (must be non-empty and end with newline)"
                        )
                    if section == "probes":
                        ts_bytes = ts.encode("utf-8")
                        if "expected_lines" in spec and spec["expected_lines"] != len(ts.splitlines()):
                            raise ValueError(
                                f"expected_lines mismatch for probes.{name}: expected {spec['expected_lines']}, got {len(ts.splitlines())}"
                            )
                        if "expected_bytes" in spec and spec["expected_bytes"] != len(ts_bytes):
                            raise ValueError(
                                f"expected_bytes mismatch for probes.{name}: expected {spec['expected_bytes']}, got {len(ts_bytes)}"
                            )
                if "roadmap_blockers" in spec:
                    blockers = spec["roadmap_blockers"]
                    if (
                        not isinstance(blockers, list)
                        or not blockers
                        or not all(isinstance(b, str) and b.strip() for b in blockers)
                    ):
                        raise ValueError(
                            f"{section}.{name} with known_unimplemented has invalid roadmap_blockers {blockers!r}"
                        )
            for src_metric_key in ("source_lines", "source_bytes"):
                if src_metric_key in spec and (
                    not isinstance(spec[src_metric_key], int)
                    or isinstance(spec[src_metric_key], bool)
                    or spec[src_metric_key] < 1
                ):
                    raise ValueError(f"{section}.{name} has invalid {src_metric_key}")
            if "baseline_marker" in spec and (
                not isinstance(spec["baseline_marker"], str)
                or not spec["baseline_marker"].strip()
            ):
                raise ValueError(f"{section}.{name} has invalid baseline_marker")
            if "oracle_origin" in spec and (
                not isinstance(spec["oracle_origin"], str)
                or spec["oracle_origin"] not in VALID_ORACLE_ORIGINS
            ):
                raise ValueError(
                    f"{section}.{name} has invalid oracle_origin {spec.get('oracle_origin')!r}"
                )
            if "spec_refs" in spec:
                srefs = spec["spec_refs"]
                if (
                    not isinstance(srefs, list)
                    or not srefs
                    or len(srefs) != len(set(srefs))
                    or not all(
                        isinstance(s, str) and re.fullmatch(r"SPEC-[0-9]{4}", s)
                        for s in srefs
                    )
                ):
                    raise ValueError(f"{section}.{name} has invalid spec_refs {srefs!r}")
                if corpus_root is not None:
                    docs_root = corpus_root.parent / "docs"
                    if (docs_root / "specs").is_dir():
                        for sref in srefs:
                            if resolve_spec_doc_relpath(sref, corpus_root.parent) is None:
                                raise ValueError(
                                    f"{section}.{name} references missing spec document for {sref}"
                                )
            elif (
                spec.get("v046_baseline_exempt")
                and corpus_root is not None
                and (corpus_root.parent / "docs" / "specs").is_dir()
            ):
                raise ValueError(
                    f"{section}.{name} with v046_baseline_exempt requires non-empty spec_refs"
                )
            if "oracle_derivation" in spec and (
                not isinstance(spec["oracle_derivation"], str)
                or not spec["oracle_derivation"].strip()
            ):
                raise ValueError(f"{section}.{name} has invalid oracle_derivation")
            if "guide_chapter" in spec:
                ch = spec["guide_chapter"]
                if not isinstance(ch, str) or ch not in VALID_GUIDE_CHAPTERS:
                    raise ValueError(f"{section}.{name} has invalid guide_chapter {ch!r}")
                if corpus_root is not None:
                    guide_dir = corpus_root.parent / "docs" / "guide"
                    if guide_dir.is_dir():
                        ch_path = guide_dir / ch
                        if not ch_path.is_file():
                            raise ValueError(
                                f"{section}.{name} references missing Guide chapter {ch_path}"
                            )
                        expected_domain = GUIDE_CHAPTER_DOMAINS[ch]
                        ch_h1 = next(
                            (
                                ln
                                for ln in ch_path.read_text(encoding="utf-8").splitlines()
                                if ln.startswith("# ")
                            ),
                            "",
                        )
                        if not ch_h1.endswith(f"：{expected_domain}"):
                            raise ValueError(
                                f"Guide chapter {ch} H1 {ch_h1!r} does not match expected domain {expected_domain!r}"
                            )
            if section == "probes":
                inferred_batch = classify_probe_batch(name)
                if "probe_batch" in spec:
                    pb = spec["probe_batch"]
                    if (
                        not isinstance(pb, str)
                        or pb not in PROBE_BATCH_CATALOG
                        or pb != inferred_batch
                    ):
                        raise ValueError(
                            f"probes.{name} has invalid probe_batch {pb!r} (expected {inferred_batch!r})"
                        )
            elif "probe_batch" in spec:
                raise ValueError(f"programs.{name} must not define probe_batch")

            if corpus_root is not None:
                src_file = (
                    corpus_root / "probes" / f"{name}.ko"
                    if section == "probes"
                    else corpus_root / "programs" / name / "main.ko"
                )
                if not src_file.is_file():
                    raise ValueError(f"missing source file for {section}.{name}: {src_file}")
                src_bytes = src_file.read_bytes()
                if not src_bytes or not src_bytes.endswith(b"\n") or b"\r" in src_bytes:
                    raise ValueError(
                        f"source file for {section}.{name} must be non-empty, use LF line endings, and end with trailing newline"
                    )
                actual_sha = sha256_bytes(src_bytes)
                if actual_sha != spec["source_sha256"]:
                    raise ValueError(
                        f"source_sha256 mismatch for {section}.{name}: expected {spec['source_sha256']}, got {actual_sha}"
                    )
                src_text = src_bytes.decode("utf-8")
                if "source_bytes" in spec and spec["source_bytes"] != len(src_bytes):
                    raise ValueError(
                        f"source_bytes mismatch for {section}.{name}: expected {spec['source_bytes']}, got {len(src_bytes)}"
                    )
                if "source_lines" in spec and spec["source_lines"] != len(src_text.splitlines()):
                    raise ValueError(
                        f"source_lines mismatch for {section}.{name}: expected {spec['source_lines']}, got {len(src_text.splitlines())}"
                    )
                if PACKAGE_DIRECTIVE_RE.search(src_text):
                    raise ValueError(
                        f"{section}.{name} must not contain a package directive (collides with project-mode package app header)"
                    )
                main_matches = MAIN_ENTRY_RE.findall(src_text)
                if len(main_matches) != 1:
                    raise ValueError(
                        f"{section}.{name} must declare a single fun main(...) entry point (found {len(main_matches)})"
                    )
                if section == "programs" and "expected_file" in spec:
                    exp_file = corpus_root / spec["expected_file"]
                    if not exp_file.is_file():
                        raise ValueError(
                            f"missing expected_file for programs.{name}: {exp_file}"
                        )
                    exp_bytes = exp_file.read_bytes()
                    if not exp_bytes or not exp_bytes.endswith(b"\n"):
                        raise ValueError(
                            f"expected_file for programs.{name} must be non-empty and end with trailing newline"
                        )
                    if sha256_bytes(exp_bytes) != spec["expected_sha256"]:
                        raise ValueError(f"expected_sha256 mismatch for programs.{name}")
                    exp_text = exp_bytes.decode("utf-8")
                    if "expected_bytes" in spec and spec["expected_bytes"] != len(exp_bytes):
                        raise ValueError(
                            f"expected_bytes mismatch for programs.{name}: expected {spec['expected_bytes']}, got {len(exp_bytes)}"
                        )
                    if "expected_lines" in spec and spec["expected_lines"] != len(exp_text.splitlines()):
                        raise ValueError(
                            f"expected_lines mismatch for programs.{name}: expected {spec['expected_lines']}, got {len(exp_text.splitlines())}"
                        )
                    spec["stdout"] = exp_text
                    spec["target_stdout"] = exp_text

    if corpus_root is not None:
        actual_probes = {p.stem for p in (corpus_root / "probes").glob("*.ko")}
        expected_probes = set(data["probes"].keys())
        missing = sorted(actual_probes - expected_probes)
        extra = sorted(expected_probes - actual_probes)
        if missing or extra:
            raise ValueError(
                f"probe inventory mismatch: missing_in_expectations={missing}, extra_in_expectations={extra}"
            )
        programs_dir = corpus_root / "programs"
        if programs_dir.is_dir():
            actual_progs = {
                d.name
                for d in programs_dir.iterdir()
                if d.is_dir() and (d / "main.ko").is_file() and (d / "expected.txt").is_file()
            }
            expected_progs = set(data["programs"].keys())
            missing_progs = sorted(actual_progs - expected_progs)
            extra_progs = sorted(expected_progs - actual_progs)
            if missing_progs or extra_progs:
                raise ValueError(
                    f"program inventory mismatch: missing_in_expectations={missing_progs}, extra_in_expectations={extra_progs}"
                )

    # Validate that every roadmap_blocker exists in docs/roadmap/v1.json and is not prematurely all done
    if corpus_root is not None:
        v1_json_path = corpus_root.parent / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
            for section in ("probes", "programs"):
                for name, spec in data[section].items():
                    if spec["kind"] == "known_unimplemented":
                        blockers = spec.get("roadmap_blockers")
                        if not blockers or not isinstance(blockers, list):
                            raise ValueError(
                                f"{section}.{name} with known_unimplemented missing roadmap_blockers in production corpus"
                            )
                        for b in blockers:
                            if b not in roadmap_items:
                                raise ValueError(
                                    f"{section}.{name} references unknown roadmap blocker {b!r} not found in {v1_json_path.name}"
                                )
                        if not any(roadmap_items[b].get("status") != "done" for b in blockers):
                            raise ValueError(
                                f"{section}.{name} has all roadmap_blockers marked done in {v1_json_path.name}: must transition out of known_unimplemented"
                            )
            verify_static_catalog_invariants(data)
    return data


def verify_static_catalog_invariants(expectations: dict[str, Any]) -> None:
    """Verify that static catalog tables in conformance_gate.py have no stale keys or unimplemented drift."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    probe_keys = set(probes.keys())
    prog_keys = set(programs.keys())
    ku_probes = {k for k, v in probes.items() if v["kind"] == "known_unimplemented"}
    if ku_probes != set(UNIMPLEMENTED_ROADMAP_BLOCKERS.keys()):
        stale = sorted(set(UNIMPLEMENTED_ROADMAP_BLOCKERS.keys()) - ku_probes)
        missing = sorted(ku_probes - set(UNIMPLEMENTED_ROADMAP_BLOCKERS.keys()))
        raise ValueError(
            f"UNIMPLEMENTED_ROADMAP_BLOCKERS drift: stale={stale}, missing={missing}"
        )
    for label, catalog in (
        ("PROBE_GUIDE_CHAPTERS", PROBE_GUIDE_CHAPTERS),
        ("CONFIRMED_STDOUT_ORACLES", CONFIRMED_STDOUT_ORACLES),
        ("SEMANTIC_ORACLE_DERIVATIONS", SEMANTIC_ORACLE_DERIVATIONS),
        ("V046_PROBE_OVERRIDES", V046_PROBE_OVERRIDES),
        ("EXPECTED_MULTI_DIAGNOSTICS", EXPECTED_MULTI_DIAGNOSTICS),
    ):
        extra = sorted(set(catalog.keys()) - probe_keys)
        if extra:
            raise ValueError(f"{label} contains unknown probe keys: {extra}")
    for label, catalog in (
        ("PROGRAM_GUIDE_CHAPTERS", PROGRAM_GUIDE_CHAPTERS),
        ("PROGRAM_ROADMAP_BLOCKERS", PROGRAM_ROADMAP_BLOCKERS),
        ("PROGRAM_ORACLE_DERIVATIONS", PROGRAM_ORACLE_DERIVATIONS),
    ):
        if set(catalog.keys()) != prog_keys:
            raise ValueError(f"{label} keys do not match programs inventory")
    if list(GUIDE_LITMUS_CATALOG.keys()) != list(range(1, 13)):
        raise ValueError("GUIDE_LITMUS_CATALOG must define Litmus 1..12 in order")
    for lit_id, lit_spec in GUIDE_LITMUS_CATALOG.items():
        if lit_spec.get("guide_chapter") not in VALID_GUIDE_CHAPTERS:
            raise ValueError(f"Litmus {lit_id} has invalid guide_chapter")
        lit_probes = lit_spec.get("probes", [])
        if not lit_probes or any(p not in probe_keys for p in lit_probes):
            raise ValueError(f"Litmus {lit_id} references unknown probes in {lit_probes!r}")
    guide15_path = (
        Path(__file__).resolve().parents[1]
        / "docs"
        / "guide"
        / "15-conformance-and-staging.md"
    )
    if guide15_path.is_file():
        g15_text = guide15_path.read_text(encoding="utf-8")
        found_litmus = {
            int(m.group(1)): m.group(2).strip()
            for m in re.finditer(r"(?m)^### Litmus ([0-9]+):\s*(.+)$", g15_text)
        }
        expected_litmus = {
            lit_id: spec["title"] for lit_id, spec in GUIDE_LITMUS_CATALOG.items()
        }
        if found_litmus != expected_litmus:
            raise ValueError(
                f"Guide 15 Litmus headings mismatch: found={found_litmus}, expected={expected_litmus}"
            )
        for ver_key in GUIDE_VERSION_BOUNDARY_CATALOG:
            if f"## {ver_key} 启用边界" not in g15_text:
                raise ValueError(f"Guide 15 missing heading '## {ver_key} 启用边界'")
    if list(GUIDE_VERSION_BOUNDARY_CATALOG.keys()) != ["v0.44", "v0.45", "v0.46", "v0.47"]:
        raise ValueError("GUIDE_VERSION_BOUNDARY_CATALOG must define v0.44..v0.47 in order")
    for ver_key, ver_spec in GUIDE_VERSION_BOUNDARY_CATALOG.items():
        v_probes = ver_spec.get("probes", [])
        if not v_probes or any(p not in probe_keys for p in v_probes):
            raise ValueError(f"Guide version boundary {ver_key} references unknown probes in {v_probes!r}")
    if set(PROGRAM_PREREQUISITE_PROBES.keys()) != prog_keys:
        raise ValueError("PROGRAM_PREREQUISITE_PROBES keys do not match programs inventory")
    for prog_name, pre_probes in PROGRAM_PREREQUISITE_PROBES.items():
        if not pre_probes or any(p not in probe_keys for p in pre_probes):
            raise ValueError(
                f"PROGRAM_PREREQUISITE_PROBES[{prog_name!r}] references unknown probes in {pre_probes!r}"
            )
    expected_v3_cats = [
        "deterministic_deinit_drop",
        "heap_class_rc_and_box",
        "string_and_text_owner",
        "collection_list_and_map_provider",
        "closure_env_and_thread_spawn",
        "checked_abort_and_overflow",
    ]
    if list(V3_SANITIZER_RESOURCE_CATALOG.keys()) != expected_v3_cats:
        raise ValueError("V3_SANITIZER_RESOURCE_CATALOG must define the 6 canonical V3 categories in order")
    v3_covered_programs: set[str] = set()
    for cat_key, cat_spec in V3_SANITIZER_RESOURCE_CATALOG.items():
        c_probes = cat_spec.get("probes", [])
        c_progs = cat_spec.get("programs", [])
        if not c_probes or any(p not in probe_keys for p in c_probes):
            raise ValueError(f"V3_SANITIZER_RESOURCE_CATALOG[{cat_key!r}] references unknown probes in {c_probes!r}")
        if any(pr not in prog_keys for pr in c_progs):
            raise ValueError(f"V3_SANITIZER_RESOURCE_CATALOG[{cat_key!r}] references unknown programs in {c_progs!r}")
        v3_covered_programs.update(c_progs)
    if v3_covered_programs != prog_keys:
        raise ValueError(f"V3_SANITIZER_RESOURCE_CATALOG programs coverage mismatch: {sorted(v3_covered_programs)}")
    if set(PROGRAM_README_CATALOG.keys()) != prog_keys:
        raise ValueError("PROGRAM_README_CATALOG keys do not match programs inventory")


def parse_tsv_matrix(
    text: str, expected_names: set[str] | None = None
) -> dict[str, tuple[str, str]]:
    """Parse a 3-column TSV matrix and reject duplicate, malformed, or missing probe rows."""
    rows: dict[str, tuple[str, str]] = {}
    for line_no, raw_line in enumerate(text.splitlines(), 1):
        if not raw_line.strip():
            raise ValueError(f"line {line_no}: empty line in TSV matrix")
        parts = raw_line.split("\t")
        if len(parts) != 3 or any(not p for p in parts):
            raise ValueError(
                f"line {line_no}: expected 3 non-empty TSV columns, got {raw_line!r}"
            )
        name, single, project = parts
        if name in rows:
            raise ValueError(f"line {line_no}: duplicate probe row {name!r}")
        rows[name] = (single, project)

    if expected_names is not None:
        missing = sorted(expected_names - set(rows.keys()))
        extra = sorted(set(rows.keys()) - expected_names)
        if missing:
            raise ValueError(f"missing probe rows in matrix: {missing}")
        if extra:
            raise ValueError(f"unexpected probe rows in matrix: {extra}")
    return rows


def derive_static_expectations(
    corpus_root: Path,
    baseline_path: Path,
    use_confirmed_oracles: bool = True,
) -> dict[str, Any]:
    """Build machine-readable expectations from source analysis + baseline without running compiler."""
    baseline = parse_tsv_matrix(baseline_path.read_text(encoding="utf-8"))
    probes_dir = corpus_root / "probes"
    programs_dir = corpus_root / "programs"

    probes_out: dict[str, Any] = {}
    probe_src_metrics: dict[str, tuple[int, int]] = {}
    for path in sorted(probes_dir.glob("*.ko")):
        name = path.stem
        # 命名违规与 baseline 缺项都要给出可读诊断，不能落到 baseline[name] 的 KeyError
        classify_probe_batch(name)
        if name not in baseline:
            raise ValueError(
                f"probes.{name} is missing from baseline matrix {baseline_path.name}; "
                f"add a baseline row for the probe or rename the source file"
            )
        raw_bytes = path.read_bytes()
        if not raw_bytes or not raw_bytes.endswith(b"\n") or b"\r" in raw_bytes:
            raise ValueError(
                f"source file for probes.{name} must be non-empty, use LF line endings, and end with trailing newline"
            )
        src_text = raw_bytes.decode("utf-8")
        probe_src_metrics[name] = (len(src_text.splitlines()), len(raw_bytes))
        src_sha = sha256_bytes(raw_bytes)
        base_single, base_project = baseline[name]
        if base_single != base_project:
            raise ValueError(
                f"baseline mode mismatch for probes.{name}: single={base_single!r} vs project={base_project!r}"
            )

        literals = SIMPLE_PRINTLN_LITERAL_RE.findall(src_text)
        total_printlns = src_text.count("println(")
        has_repeated_print_risk = any(
            token in src_text
            for token in ("for (", "for(", "while (", "while(", "deinit(")
        )
        is_single_literal = (
            len(literals) == 1
            and total_printlns == 1
            and not has_repeated_print_risk
        )
        single_static_stdout = (
            f"{literals[0]}\n"
            if is_single_literal
            else (CONFIRMED_STDOUT_ORACLES.get(name) if use_confirmed_oracles else None)
        )
        stdout_origin = (
            "single_println_literal"
            if is_single_literal
            else ("source_trace_audited" if single_static_stdout is not None else None)
        )
        chapter = classify_probe_guide_chapter(name, src_text)
        derivation = SEMANTIC_ORACLE_DERIVATIONS.get(
            name,
            f"single literal println({literals[0]!r})" if is_single_literal else None,
        )

        if name in V046_PROBE_OVERRIDES:
            entry = dict(V046_PROBE_OVERRIDES[name])
            entry["source_sha256"] = src_sha
            entry["guide_chapter"] = chapter
            entry["baseline_marker"] = base_single
            entry["oracle_origin"] = "v046_spec_contract"
            spec_refs = list(
                dict.fromkeys(re.findall(r"SPEC-[0-9]{4}", entry.get("note", "")))
            )
            if spec_refs:
                entry["spec_refs"] = spec_refs
            if entry.get("kind") == "compile_fail" and entry.get("diagnostic") in DIAGNOSTIC_SYMBOL_CATALOG:
                entry["diagnostic_symbol"] = DIAGNOSTIC_SYMBOL_CATALOG[entry["diagnostic"]]
            if entry.get("kind") == "known_unimplemented":
                entry["roadmap_blockers"] = list(UNIMPLEMENTED_ROADMAP_BLOCKERS.get(name, ["N02"]))
            if derivation is not None:
                entry["oracle_derivation"] = derivation
            probes_out[name] = entry
            continue

        if base_single == "OK":
            if single_static_stdout is not None:
                entry = {
                    "kind": "run_ok",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "baseline_marker": "OK",
                    "stdout": single_static_stdout,
                    "oracle_origin": stdout_origin,
                }
                if derivation is not None:
                    entry["oracle_derivation"] = derivation
                probes_out[name] = entry
            else:
                probes_out[name] = {
                    "kind": "unreviewed",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "baseline_marker": "OK",
                    "note": "multi-line or computed println output requires human oracle confirmation",
                }
        elif base_single.startswith("FE:"):
            code = base_single.split(":", 1)[1]
            if code == "L0165" or name in ("p27_thread", "s02_pair_builtin"):
                entry = {
                    "kind": "known_unimplemented",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "baseline_marker": base_single,
                    "allowed_markers": [
                        base_single,
                        "BE:UnsupportedNode",
                        "BE:MissingFact",
                    ],
                    "oracle_origin": "baseline_unimplemented_audited",
                    "roadmap_blockers": list(UNIMPLEMENTED_ROADMAP_BLOCKERS.get(name, ["N02"])),
                }
                if single_static_stdout is not None:
                    entry["target_stdout"] = single_static_stdout
                if derivation is not None:
                    entry["oracle_derivation"] = derivation
                probes_out[name] = entry
            else:
                entry = {
                    "kind": "compile_fail",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "baseline_marker": base_single,
                    "diagnostic": code,
                    "oracle_origin": "baseline_diagnostic_audited",
                }
                if code in DIAGNOSTIC_SYMBOL_CATALOG:
                    entry["diagnostic_symbol"] = DIAGNOSTIC_SYMBOL_CATALOG[code]
                if name in EXPECTED_MULTI_DIAGNOSTICS:
                    entry["all_diagnostics"] = list(EXPECTED_MULTI_DIAGNOSTICS[name])
                    entry["all_diagnostic_symbols"] = [
                        DIAGNOSTIC_SYMBOL_CATALOG[c] for c in EXPECTED_MULTI_DIAGNOSTICS[name]
                    ]
                if derivation is not None:
                    entry["oracle_derivation"] = derivation
                probes_out[name] = entry
        elif base_single.startswith("BE:"):
            entry = {
                "kind": "known_unimplemented",
                "source_sha256": src_sha,
                "guide_chapter": chapter,
                "baseline_marker": base_single,
                "allowed_markers": ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165"],
                "oracle_origin": "baseline_unimplemented_audited",
                "roadmap_blockers": list(UNIMPLEMENTED_ROADMAP_BLOCKERS.get(name, ["N02"])),
            }
            if single_static_stdout is not None:
                entry["target_stdout"] = single_static_stdout
            if derivation is not None:
                entry["oracle_derivation"] = derivation
            probes_out[name] = entry
        else:
            probes_out[name] = {
                "kind": "unreviewed",
                "source_sha256": src_sha,
                "guide_chapter": chapter,
                "baseline_marker": base_single,
                "note": "requires human oracle confirmation",
            }

    for name, entry in probes_out.items():
        src_lines, src_bytes_len = probe_src_metrics[name]
        entry["source_lines"] = src_lines
        entry["source_bytes"] = src_bytes_len
        out_text = entry.get("stdout") or entry.get("target_stdout")
        if isinstance(out_text, str) and out_text:
            entry["expected_lines"] = len(out_text.splitlines())
            entry["expected_bytes"] = len(out_text.encode("utf-8"))

    programs_out: dict[str, Any] = {}
    for prog_dir in sorted(programs_dir.iterdir()):
        if not prog_dir.is_dir():
            continue
        main_ko = prog_dir / "main.ko"
        expected_txt = prog_dir / "expected.txt"
        if not main_ko.is_file() or not expected_txt.is_file():
            continue
        main_bytes = main_ko.read_bytes()
        if not main_bytes or not main_bytes.endswith(b"\n") or b"\r" in main_bytes:
            raise ValueError(
                f"source file for programs.{prog_dir.name} must be non-empty, use LF line endings, and end with trailing newline"
            )
        main_text = main_bytes.decode("utf-8")
        exp_bytes = expected_txt.read_bytes()
        exp_text = exp_bytes.decode("utf-8")
        prog_entry: dict[str, Any] = {
            "kind": "known_unimplemented",
            "source_sha256": sha256_bytes(main_bytes),
            "source_lines": len(main_text.splitlines()),
            "source_bytes": len(main_bytes),
            "guide_chapter": PROGRAM_GUIDE_CHAPTERS.get(
                prog_dir.name, "13-program-runtime-standard-library.md"
            ),
            "expected_file": f"programs/{prog_dir.name}/expected.txt",
            "expected_sha256": sha256_bytes(exp_bytes),
            "expected_lines": len(exp_text.splitlines()),
            "expected_bytes": len(exp_bytes),
            "allowed_markers": [
                "FE:L0080",
                "FE:L0084",
                "FE:L0085",
                "FE:L0165",
                "BE:UnsupportedNode",
                "BE:MissingFact",
            ],
            "oracle_origin": "expected_txt_file",
            "roadmap_blockers": list(PROGRAM_ROADMAP_BLOCKERS.get(prog_dir.name, ["N02"])),
        }
        if prog_dir.name in PROGRAM_ORACLE_DERIVATIONS:
            prog_entry["oracle_derivation"] = PROGRAM_ORACLE_DERIVATIONS[prog_dir.name]
        programs_out[prog_dir.name] = prog_entry

    return {
        "schema": SCHEMA_EXPECTATIONS,
        "probes": probes_out,
        "programs": programs_out,
    }
