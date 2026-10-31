#!/usr/bin/env python3
"""Offline evidence bundle verification, dual-host parity comparison, and TSV matrix checks."""

from __future__ import annotations

import base64
import json
from pathlib import Path
import re
from typing import Any

from conformance_catalogs import (
    GUIDE_SYNTAX_INDEX_DOMAINS,
    MAIN_ENTRY_RE,
    PACKAGE_DIRECTIVE_RE,
    PROGRAM_GUIDE_CHAPTERS,
    PROGRAM_PREREQUISITE_PROBES,
    PROGRAM_README_CATALOG,
    PROGRAM_ROADMAP_BLOCKERS,
    PROJECT_PACKAGE_HEADER,
    PROJECT_TOML_BYTES,
    SCHEMA_RESULTS,
    VALID_GUIDE_CHAPTERS,
    classify_probe_batch,
)
from conformance_expectations import (
    load_expectations,
    parse_tsv_matrix,
    sha256_bytes,
)
from conformance_runner import (
    build_project_source,
    classify_raw,
    compare_item,
    is_internal_error_marker,
    summarize_comparisons,
)


def render_evidence_summary_markdown(ledger: dict[str, Any]) -> str:
    """Render deterministic Markdown execution summary (SUMMARY.md) for a conformance evidence bundle."""
    comp = ledger["compiler"]
    host = ledger["host"]
    summary = ledger["summary"]
    verdict_str = "PASS" if summary["gate_passed"] else "FAIL"
    fp2 = summary.get("foundational_p2", {})
    lines = [
        "# Conformance 回归验收门禁执行摘要（SUMMARY.md）",
        "",
        "| 维度 | 值 |",
        "|---|---|",
        f"| **门禁结论 (`gate_passed`)** | **{verdict_str}** |",
        f"| **编译器 Git 提交 (`commit`)** | `{comp['commit']}` |",
        f"| **编译器二进制 SHA-256** | `{comp['sha256']}` |",
        f"| **宿主平台 (`system` / `machine`)** | `{host['system']} / {host['machine']}` |",
        f"| **Probe 能力通过 (`probe_capability_passed`)** | **{summary['probe_capability_passed']} / {summary['total_probes']}** |",
        f"| **V1 验收程序通过 (`program_run_passed`)** | **{summary['program_run_passed']} / {summary['total_programs']}** |",
        f"| **P2 基础 A 组可运行 (`foundational_p2`)** | `{fp2.get('group_a_runnable_passed', 0)} / {fp2.get('group_a_evaluated', 0)}` (目标 `≥ {fp2.get('p2_target_min', 70)}`) |",
        "",
        "## 1. 比较器状态分布 (`counts`)",
        "",
        "| 状态 (`status`) | 数量 |",
        "|---|---:|",
    ]
    for k, v in sorted(summary["counts"].items()):
        lines.append(f"| `{k}` | {v} |")

    by_batch = summary.get("by_batch", {})
    if by_batch:
        lines.extend(
            [
                "",
                "## 2. 按 Probe 批次执行汇总 (`by_batch`)",
                "",
                "| 批次 (`probe_batch`) | 总数 | `pass_run` | `pass_diagnostic` | `pass_abort` | `known_unimplemented` | `capability_passed` |",
                "|---|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for b_key, b_row in by_batch.items():
            lines.append(
                f"| `{b_key}` | {b_row['total']} | {b_row['pass_run']} | {b_row['pass_diagnostic']} | "
                f"{b_row['pass_abort']} | {b_row['known_unimplemented']} | {b_row['capability_passed']} / {b_row['total']} |"
            )

    by_chapter = summary.get("by_chapter", {})
    if by_chapter:
        lines.extend(
            [
                "",
                "## 3. 按 Guide 章节执行汇总 (`by_chapter`)",
                "",
                "| Guide 章节 | 总数 | `pass_run` | `pass_diagnostic` | `pass_abort` | `known_unimplemented` | `capability_passed` |",
                "|---|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for ch_key, ch_row in by_chapter.items():
            lines.append(
                f"| `{ch_key}` | {ch_row['total']} | {ch_row['pass_run']} | {ch_row['pass_diagnostic']} | "
                f"{ch_row['pass_abort']} | {ch_row['known_unimplemented']} | {ch_row['capability_passed']} / {ch_row['total']} |"
            )

    if summary.get("unexpected_passes"):
        lines.extend(
            [
                "",
                "## 4. 新晋通过待晋升条目 (`unexpected_passes`)",
                "",
            ]
        )
        for up_name in summary["unexpected_passes"]:
            lines.append(
                f"- `{up_name}`：已在单文件与工程双模式下退出码 0 且精确匹配 `target_stdout`，请更新 `V046_PROBE_OVERRIDES` 并运行 `--init-expectations`。"
            )

    if summary.get("failures"):
        lines.extend(
            [
                "",
                "## 5. 阻断项明细 (`failures`)",
                "",
            ]
        )
        for fail_msg in summary["failures"]:
            lines.append(f"- `{fail_msg}`")

    lines.append("")
    return "\n".join(lines)


def verify_evidence_bundle(
    evidence_dir: Path,
    expectations_path: Path,
    corpus_root: Path | None = None,
    baseline_path: Path | None = None,
    strict_unreviewed: bool = True,
) -> dict[str, Any]:
    """Verify an offline evidence bundle's raw stdout/stderr/status files, matrix.tsv, and results.json against expectations."""
    evidence_dir = evidence_dir.resolve(strict=True)
    results_path = evidence_dir / "results.json"
    matrix_path = evidence_dir / "matrix.tsv"
    raw_dir = evidence_dir / "raw"
    if not results_path.is_file():
        raise ValueError(f"missing results.json in evidence bundle: {evidence_dir}")
    if not matrix_path.is_file():
        raise ValueError(f"missing matrix.tsv in evidence bundle: {evidence_dir}")
    if not raw_dir.is_dir():
        raise ValueError(f"missing raw/ directory in evidence bundle: {evidence_dir}")

    ledger = json.loads(results_path.read_text(encoding="utf-8"))
    if not isinstance(ledger, dict) or ledger.get("schema") != SCHEMA_RESULTS:
        raise ValueError(f"invalid results.json schema in {results_path}")

    compiler = ledger.get("compiler")
    if (
        not isinstance(compiler, dict)
        or not isinstance(compiler.get("sha256"), str)
        or not re.fullmatch(r"[0-9a-f]{64}", compiler["sha256"])
        or not isinstance(compiler.get("commit"), str)
        or not compiler["commit"].strip()
    ):
        raise ValueError("invalid compiler metadata in results.json")

    host = ledger.get("host")
    if (
        not isinstance(host, dict)
        or not isinstance(host.get("system"), str)
        or not host["system"].strip()
        or not isinstance(host.get("machine"), str)
        or not host["machine"].strip()
    ):
        raise ValueError("invalid host metadata in results.json")

    expectations = load_expectations(
        expectations_path,
        corpus_root=corpus_root.resolve(strict=True) if corpus_root is not None else None,
    )
    baseline = (
        parse_tsv_matrix(
            baseline_path.read_text(encoding="utf-8"),
            expected_names=set(expectations["probes"].keys()),
        )
        if baseline_path is not None
        else {}
    )

    def _verify_entry(
        entry: dict[str, Any],
        section: str,
        raw_prefix_name: str,
        exp_spec: dict[str, Any],
        base_row: tuple[str, str] | None,
    ) -> dict[str, Any]:
        if not isinstance(entry, dict) or "execution" not in entry or "comparison" not in entry:
            raise ValueError(f"malformed entry in results.json {section}")
        exec_rec = dict(entry["execution"])
        comp_rec = entry["comparison"]
        name = exec_rec.get("name")
        if exec_rec.get("source_sha256") != exp_spec["source_sha256"]:
            raise ValueError(
                f"source_sha256 mismatch in results.json for {section}.{name}: "
                f"recorded {exec_rec.get('source_sha256')}, expected {exp_spec['source_sha256']}"
            )
        pt = exec_rec.get("project_transform")
        if (
            not isinstance(pt, dict)
            or pt.get("kind") != "prepend_package_header"
            or pt.get("header_sha256") != sha256_bytes(PROJECT_PACKAGE_HEADER)
            or pt.get("project_toml_sha256") != sha256_bytes(PROJECT_TOML_BYTES)
            or pt.get("original_source_sha256") != exp_spec["source_sha256"]
        ):
            raise ValueError(f"invalid project_transform metadata for {section}.{name}")

        reconstructed_exec = dict(exec_rec)
        for mode in ("single", "project"):
            m_rec = exec_rec.get(mode)
            if not isinstance(m_rec, dict):
                raise ValueError(f"missing {mode} execution record for {section}.{name}")
            out_p = raw_dir / f"{raw_prefix_name}.{mode}.stdout"
            err_p = raw_dir / f"{raw_prefix_name}.{mode}.stderr"
            st_p = raw_dir / f"{raw_prefix_name}.{mode}.status.json"
            if not out_p.is_file() or not err_p.is_file() or not st_p.is_file():
                raise ValueError(f"missing raw artifact files for {section}.{name}.{mode}")
            out_bytes = out_p.read_bytes()
            err_bytes = err_p.read_bytes()
            st_data = json.loads(st_p.read_text(encoding="utf-8"))
            if st_data.get("exit") != m_rec.get("exit") or st_data.get("timed_out") != m_rec.get("timed_out"):
                raise ValueError(f"status.json mismatch for {section}.{name}.{mode}")
            if (
                len(out_bytes) != m_rec.get("stdout_len")
                or sha256_bytes(out_bytes) != m_rec.get("stdout_sha256")
                or base64.b64encode(out_bytes).decode("ascii") != m_rec.get("stdout_b64")
            ):
                raise ValueError(f"raw stdout mismatch for {section}.{name}.{mode}")
            if (
                len(err_bytes) != m_rec.get("stderr_len")
                or sha256_bytes(err_bytes) != m_rec.get("stderr_sha256")
                or base64.b64encode(err_bytes).decode("ascii") != m_rec.get("stderr_b64")
            ):
                raise ValueError(f"raw stderr mismatch for {section}.{name}.{mode}")
            recomputed_marker = classify_raw(
                m_rec["exit"], bool(m_rec["timed_out"]), out_bytes, err_bytes
            )
            if recomputed_marker != m_rec.get("marker"):
                raise ValueError(
                    f"marker mismatch for {section}.{name}.{mode}: recorded {m_rec.get('marker')!r}, recomputed {recomputed_marker!r}"
                )
            m_with_bytes = dict(m_rec)
            m_with_bytes["stdout_bytes"] = out_bytes
            m_with_bytes["stderr_bytes"] = err_bytes
            reconstructed_exec[mode] = m_with_bytes

        recomputed_comp = compare_item(reconstructed_exec, exp_spec, baseline_row=base_row)
        if recomputed_comp != comp_rec:
            raise ValueError(
                f"comparison mismatch for {section}.{name}: recorded {comp_rec}, recomputed {recomputed_comp}"
            )
        return recomputed_comp

    probe_entries = ledger.get("probes")
    program_entries = ledger.get("programs")
    if not isinstance(probe_entries, list) or not isinstance(program_entries, list):
        raise ValueError("results.json probes and programs must be lists")

    recomputed_probe_comps: list[dict[str, Any]] = []
    seen_probes: set[str] = set()
    for entry in probe_entries:
        name = entry.get("execution", {}).get("name")
        if not isinstance(name, str) or name not in expectations["probes"]:
            raise ValueError(f"unknown probe {name!r} in results.json")
        if name in seen_probes:
            raise ValueError(f"duplicate probe {name!r} in results.json")
        seen_probes.add(name)
        recomputed_probe_comps.append(
            _verify_entry(entry, "probes", name, expectations["probes"][name], baseline.get(name))
        )

    recomputed_prog_comps: list[dict[str, Any]] = []
    seen_progs: set[str] = set()
    for entry in program_entries:
        name = entry.get("execution", {}).get("name")
        if not isinstance(name, str) or name not in expectations["programs"]:
            raise ValueError(f"unknown program {name!r} in results.json")
        if name in seen_progs:
            raise ValueError(f"duplicate program {name!r} in results.json")
        seen_progs.add(name)
        recomputed_prog_comps.append(
            _verify_entry(entry, "programs", f"program_{name}", expectations["programs"][name], None)
        )

    matrix_text = matrix_path.read_text(encoding="utf-8")
    if seen_probes:
        matrix_rows = parse_tsv_matrix(matrix_text, expected_names=seen_probes)
        for comp in recomputed_probe_comps:
            if matrix_rows.get(comp["name"]) != (comp["single_marker"], comp["project_marker"]):
                raise ValueError(f"matrix.tsv row mismatch for probe {comp['name']}")
    elif matrix_text.strip():
        raise ValueError("matrix.tsv must be empty when no probes were evaluated")

    recomputed_summary = summarize_comparisons(
        recomputed_probe_comps,
        recomputed_prog_comps,
        strict_unreviewed=strict_unreviewed,
    )
    if recomputed_summary != ledger.get("summary"):
        raise ValueError("summary mismatch in results.json")

    summary_md_path = evidence_dir / "SUMMARY.md"
    if summary_md_path.is_file():
        expected_summary_md = render_evidence_summary_markdown(ledger)
        if summary_md_path.read_text(encoding="utf-8") != expected_summary_md:
            raise ValueError("SUMMARY.md mismatch in evidence bundle")

    return ledger


def compare_dual_host_evidence(
    evidence_dir_a: Path,
    evidence_dir_b: Path,
    expectations_path: Path,
    corpus_root: Path | None = None,
    baseline_path: Path | None = None,
    strict_unreviewed: bool = True,
    require_same_commit: bool = True,
    require_distinct_systems: bool = False,
) -> dict[str, Any]:
    """Verify two evidence bundles (e.g. macOS and Linux CI runs) and assert cross-host parity across all probes and programs."""
    ledger_a = verify_evidence_bundle(
        evidence_dir=evidence_dir_a,
        expectations_path=expectations_path,
        corpus_root=corpus_root,
        baseline_path=baseline_path,
        strict_unreviewed=strict_unreviewed,
    )
    ledger_b = verify_evidence_bundle(
        evidence_dir=evidence_dir_b,
        expectations_path=expectations_path,
        corpus_root=corpus_root,
        baseline_path=baseline_path,
        strict_unreviewed=strict_unreviewed,
    )
    commit_a = ledger_a["compiler"]["commit"]
    commit_b = ledger_b["compiler"]["commit"]
    if require_same_commit and commit_a != commit_b:
        raise ValueError(
            f"dual-host commit mismatch: {commit_a!r} vs {commit_b!r}"
        )
    host_a = ledger_a["host"]
    host_b = ledger_b["host"]
    if require_distinct_systems and host_a["system"] == host_b["system"]:
        raise ValueError(
            f"dual-host comparison requires distinct host systems, both were {host_a['system']!r}"
        )

    for section in ("probes", "programs"):
        map_a = {e["execution"]["name"]: e for e in ledger_a[section]}
        map_b = {e["execution"]["name"]: e for e in ledger_b[section]}
        if set(map_a.keys()) != set(map_b.keys()):
            raise ValueError(
                f"dual-host {section} inventory mismatch: "
                f"only_in_a={sorted(set(map_a) - set(map_b))}, "
                f"only_in_b={sorted(set(map_b) - set(map_a))}"
            )
        for name in sorted(map_a.keys()):
            comp_a = map_a[name]["comparison"]
            comp_b = map_b[name]["comparison"]
            if comp_a != comp_b:
                raise ValueError(
                    f"dual-host comparison mismatch on {section}.{name}: "
                    f"{comp_a['status']} ({comp_a['single_marker']}/{comp_a['project_marker']}) vs "
                    f"{comp_b['status']} ({comp_b['single_marker']}/{comp_b['project_marker']})"
                )
            if comp_a["status"] == "pass_run":
                for mode in ("single", "project"):
                    sha_a = map_a[name]["execution"][mode]["stdout_sha256"]
                    sha_b = map_b[name]["execution"][mode]["stdout_sha256"]
                    if sha_a != sha_b:
                        raise ValueError(
                            f"dual-host stdout_sha256 mismatch on {section}.{name}.{mode}: {sha_a} vs {sha_b}"
                        )

    if ledger_a["summary"] != ledger_b["summary"]:
        raise ValueError("dual-host summary mismatch between evidence bundles")

    return {
        "parity_passed": True,
        "gate_passed": bool(ledger_a["summary"]["gate_passed"]) and bool(ledger_b["summary"]["gate_passed"]),
        "commit": commit_a,
        "host_a": host_a,
        "host_b": host_b,
        "total_probes": ledger_a["summary"]["total_probes"],
        "probe_capability_passed": ledger_a["summary"]["probe_capability_passed"],
        "total_programs": ledger_a["summary"]["total_programs"],
        "program_run_passed": ledger_a["summary"]["program_run_passed"],
    }


def verify_matrix_evidence_dir(
    evidence_dir: Path,
    expected_names: set[str] | None = None,
) -> dict[str, Any]:
    """Verify a TSV-mode conformance_matrix.sh evidence directory (manifest.json, matrix.tsv, and raw files)."""
    evidence_dir = evidence_dir.resolve(strict=True)
    manifest_path = evidence_dir / "manifest.json"
    matrix_path = evidence_dir / "matrix.tsv"
    raw_dir = evidence_dir / "raw"
    if not manifest_path.is_file():
        raise ValueError(f"missing manifest.json in matrix evidence dir: {evidence_dir}")
    if not matrix_path.is_file():
        raise ValueError(f"missing matrix.tsv in matrix evidence dir: {evidence_dir}")
    if not raw_dir.is_dir():
        raise ValueError(f"missing raw/ directory in matrix evidence dir: {evidence_dir}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if (
        not isinstance(manifest, dict)
        or manifest.get("schema") != "koven.conformance-matrix-manifest.v1"
        or manifest.get("matrix_file") != "matrix.tsv"
        or manifest.get("raw_dir") != "raw"
    ):
        raise ValueError(f"invalid manifest.json schema in {manifest_path}")

    rows = parse_tsv_matrix(matrix_path.read_text(encoding="utf-8"), expected_names=expected_names)
    if manifest.get("probe_count") != len(rows):
        raise ValueError(
            f"manifest probe_count mismatch: recorded {manifest.get('probe_count')}, matrix has {len(rows)}"
        )
    for name, (single_m, project_m) in sorted(rows.items()):
        for mode, expected_m in (("single", single_m), ("project", project_m)):
            out_p = raw_dir / f"{name}.{mode}.stdout"
            err_p = raw_dir / f"{name}.{mode}.stderr"
            st_p = raw_dir / f"{name}.{mode}.status.json"
            if not out_p.is_file() or not err_p.is_file() or not st_p.is_file():
                raise ValueError(f"missing raw file for {name}.{mode} in {raw_dir}")
            st_data = json.loads(st_p.read_text(encoding="utf-8"))
            recomputed = classify_raw(
                st_data.get("exit"),
                bool(st_data.get("timed_out")),
                out_p.read_bytes(),
                err_p.read_bytes(),
            )
            if recomputed != expected_m:
                raise ValueError(
                    f"matrix evidence marker mismatch for {name}.{mode}: matrix has {expected_m!r}, raw files classify as {recomputed!r}"
                )
    return {"manifest": manifest, "rows": rows}


def compare_tsv_matrix_against_expectations(
    matrix_path: Path,
    expectations_path: Path,
    corpus_root: Path | None = None,
    baseline_path: Path | None = None,
) -> dict[str, Any]:
    """Compare a 3-column TSV matrix against expectations.json and optional baseline TSV without requiring raw stdout files."""
    matrix_path = matrix_path.resolve(strict=True)
    expectations = load_expectations(
        expectations_path,
        corpus_root=corpus_root.resolve(strict=True) if corpus_root is not None else None,
    )
    baseline = (
        parse_tsv_matrix(
            baseline_path.read_text(encoding="utf-8"),
            expected_names=set(expectations["probes"].keys()),
        )
        if baseline_path is not None
        else {}
    )
    rows = parse_tsv_matrix(matrix_path.read_text(encoding="utf-8"))
    unknown = sorted(set(rows.keys()) - set(expectations["probes"].keys()))
    if unknown:
        raise ValueError(f"unknown probe names in TSV matrix: {unknown}")

    counts: dict[str, int] = {
        "pass_run": 0,
        "pass_diagnostic": 0,
        "pass_abort": 0,
        "known_unimplemented": 0,
        "mode_mismatch": 0,
        "regression": 0,
        "internal_error": 0,
        "unexpected_pass": 0,
    }
    failures: list[str] = []
    unexpected_passes: list[str] = []
    cap_passed = 0

    for name, (single_m, project_m) in sorted(rows.items()):
        spec = expectations["probes"][name]
        kind = spec["kind"]
        if is_internal_error_marker(single_m) or is_internal_error_marker(project_m):
            st = "internal_error"
            failures.append(f"{name} (internal_error): single={single_m}, project={project_m}")
        elif single_m != project_m:
            st = "mode_mismatch"
            failures.append(f"{name} (mode_mismatch): single={single_m} vs project={project_m}")
        elif kind == "run_ok":
            if single_m == "OK":
                st = "pass_run"
                cap_passed += 1
            else:
                st = "regression"
                failures.append(f"{name} (regression): expected OK, got {single_m}")
        elif kind == "compile_fail":
            exp_m = f"FE:{spec['diagnostic']}"
            if single_m == exp_m:
                st = "pass_diagnostic"
                cap_passed += 1
            else:
                st = "regression"
                failures.append(f"{name} (regression): expected {exp_m}, got {single_m}")
        elif kind == "runtime_abort":
            exp_m = f"abort:{spec['abort_message']}"
            if single_m == exp_m:
                st = "pass_abort"
                cap_passed += 1
            else:
                st = "regression"
                failures.append(f"{name} (regression): expected {exp_m}, got {single_m}")
        elif kind == "known_unimplemented":
            if single_m == "OK":
                st = "unexpected_pass"
                unexpected_passes.append(name)
                failures.append(f"{name} (unexpected_pass): now OK in both modes; promote to run_ok")
            elif single_m in spec.get("allowed_markers", []):
                st = "known_unimplemented"
            else:
                st = "regression"
                failures.append(
                    f"{name} (regression): expected one of {spec.get('allowed_markers')}, got {single_m}"
                )
        else:
            st = "regression"
            failures.append(f"{name} (unreviewed): {single_m}")

        if (
            name in baseline
            and not spec.get("v046_baseline_exempt", False)
            and baseline[name][0] == "OK"
            and (single_m != "OK" or project_m != "OK")
            and st != "regression"
        ):
            st = "regression"
            failures.append(f"{name} (baseline regression): was OK, now {single_m}/{project_m}")

        counts[st] = counts.get(st, 0) + 1

    return {
        "gate_passed": len(failures) == 0,
        "total_probes": len(rows),
        "probe_capability_passed": cap_passed,
        "counts": counts,
        "unexpected_passes": unexpected_passes,
        "failures": failures,
    }


def summarize_guide_syntax_index_alignment(
    expectations: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    """Cross-reference the 36 syntax entries across 14 domain groups in Guide 14 against target chapter probes."""
    probes = expectations["probes"]
    out: dict[str, dict[str, Any]] = {}
    for dom, (entry_count, chapters, summary) in GUIDE_SYNTAX_INDEX_DOMAINS.items():
        ch_set = set(chapters)
        dom_probes = sorted(
            name for name, spec in probes.items() if spec.get("guide_chapter") in ch_set
        )
        cap_passed = [
            p
            for p in dom_probes
            if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in dom_probes if probes[p]["kind"] == "known_unimplemented"]
        out[dom] = {
            "domain": dom,
            "entry_count": entry_count,
            "chapters": list(chapters),
            "summary": summary,
            "probe_count": len(dom_probes),
            "cap_passed_count": len(cap_passed),
            "unimplemented_count": len(unimpl),
            "status": "all_passed" if not unimpl else "partial",
        }
    return out


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


def summarize_project_transform_contracts(
    expectations: dict[str, Any],
    corpus_root: Path | None = None,
) -> dict[str, Any]:
    """Verify and summarize single-file vs project-mode (`package app` + `app.main`) transformation invariants across the corpus."""
    if corpus_root is None:
        default_corpus = Path(__file__).resolve().parents[1] / "conformance"
        if (default_corpus / "probes").is_dir():
            corpus_root = default_corpus

    probes = expectations["probes"]
    programs = expectations["programs"]
    explicit_import_probes: list[str] = []
    default_prelude_probes: list[str] = []

    for name, spec in sorted(probes.items()):
        if corpus_root is not None:
            src_file = corpus_root / "probes" / f"{name}.ko"
            raw_bytes = src_file.read_bytes()
            transformed_bytes, meta = build_project_source(raw_bytes)
            if meta["original_source_sha256"] != spec["source_sha256"]:
                raise ValueError(f"project transform original_source_sha256 mismatch for probes.{name}")
            if meta["transformed_source_sha256"] == meta["original_source_sha256"]:
                raise ValueError(f"project transform did not alter source hash for probes.{name}")
            t_text = transformed_bytes.decode("utf-8")
            if len(PACKAGE_DIRECTIVE_RE.findall(t_text)) != 1 or len(MAIN_ENTRY_RE.findall(t_text)) != 1:
                raise ValueError(f"project transform violated single package/main invariant for probes.{name}")
            has_import = bool(re.search(r"(?m)^\s*import\s+", t_text))
        else:
            has_import = name.startswith("y") and name != "y02_take_no_import"
        if has_import:
            explicit_import_probes.append(name)
        else:
            default_prelude_probes.append(name)

    if corpus_root is not None:
        for name, spec in sorted(programs.items()):
            src_file = corpus_root / "programs" / name / "main.ko"
            raw_bytes = src_file.read_bytes()
            transformed_bytes, meta = build_project_source(raw_bytes)
            if meta["original_source_sha256"] != spec["source_sha256"]:
                raise ValueError(f"project transform original_source_sha256 mismatch for programs.{name}")
            t_text = transformed_bytes.decode("utf-8")
            if len(PACKAGE_DIRECTIVE_RE.findall(t_text)) != 1 or len(MAIN_ENTRY_RE.findall(t_text)) != 1:
                raise ValueError(f"project transform violated single package/main invariant for programs.{name}")

    return {
        "header_utf8": PROJECT_PACKAGE_HEADER.decode("utf-8").strip(),
        "header_bytes": len(PROJECT_PACKAGE_HEADER),
        "header_sha256": sha256_bytes(PROJECT_PACKAGE_HEADER),
        "project_toml_sha256": sha256_bytes(PROJECT_TOML_BYTES),
        "target_relpath": "src/app/main.ko",
        "entry_selector": "app.main",
        "cohorts": {
            "probes_default_prelude": {
                "label": "无显式 `import` 探针（默认内建环境）",
                "count": len(default_prelude_probes),
                "cap_passed": sum(
                    1
                    for p in default_prelude_probes
                    if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
                ),
                "representative": "`p01_int_print`..`x03_if_call_borrow_arg`, `y02_take_no_import` (FE:L0080)",
            },
            "probes_explicit_import": {
                "label": "含显式 `import koven.algorithms.take` 探针",
                "count": len(explicit_import_probes),
                "cap_passed": sum(
                    1
                    for p in explicit_import_probes
                    if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
                ),
                "probes": explicit_import_probes,
                "representative": ", ".join(f"`{p}`" for p in explicit_import_probes),
            },
            "acceptance_programs": {
                "label": "V1 多函数/多类型验收程序（`programs/<name>/main.ko`）",
                "count": len(programs),
                "cap_passed": sum(1 for s in programs.values() if s["kind"] == "run_ok"),
                "representative": ", ".join(f"`{p}`" for p in sorted(programs.keys())),
            },
        },
    }


def render_readme_program_table(expectations: dict[str, Any]) -> str:
    """Render the V1 acceptance programs summary table embedded in conformance/README.md."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    lines = [
        "| 目录 | 题目 | 覆盖面 | 归属章节 | 当前状态 | 前置 Probe 就绪 | 路线图阻塞项 |",
        "|---|---|---|---|---|---:|---|",
    ]
    for prog_name, (title, coverage) in PROGRAM_README_CATALOG.items():
        spec = programs.get(prog_name, {})
        ch = spec.get(
            "guide_chapter",
            PROGRAM_GUIDE_CHAPTERS.get(prog_name, "13-program-runtime-standard-library.md"),
        )
        kind = spec.get("kind", "known_unimplemented")
        blockers = spec.get("roadmap_blockers", PROGRAM_ROADMAP_BLOCKERS.get(prog_name, []))
        blk_s = ", ".join(f"`{b}`" for b in blockers) if blockers else "-"
        pre_list = PROGRAM_PREREQUISITE_PROBES.get(prog_name, [])
        if pre_list and all(p in probes for p in pre_list):
            passed_cnt = sum(
                1
                for p in pre_list
                if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
            )
            pre_s = f"{passed_cnt} / {len(pre_list)}"
        else:
            pre_s = "-"
        lines.append(
            f"| `{prog_name}` | {title} | {coverage} | [{ch}](../docs/guide/{ch}) | `{kind}` | {pre_s} | {blk_s} |"
        )
    return "\n".join(lines)


def render_readme_chapter_table(expectations: dict[str, Any]) -> str:
    """Render the Guide chapter coverage summary table embedded in conformance/README.md."""
    from conformance_audit import summarize_chapter_polarity

    chapter_stats = summarize_chapter_polarity(expectations)
    lines = [
        "| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    tot_all = tot_ok = tot_cf = tot_ab = tot_ku = tot_cap = 0
    for ch in VALID_GUIDE_CHAPTERS:
        st = chapter_stats[ch]
        tot_all += st["total"]
        tot_ok += st["run_ok"]
        tot_cf += st["compile_fail"]
        tot_ab += st["runtime_abort"]
        tot_ku += st["known_unimplemented"]
        tot_cap += st["cap_passed"]
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {st['total']} | {st['run_ok']} | {st['compile_fail']} | "
            f"{st['runtime_abort']} | {st['known_unimplemented']} | {st['cap_passed']} / {st['total']} |"
        )
    lines.append(
        f"| **合计** | **{tot_all}** | **{tot_ok}** | **{tot_cf}** | **{tot_ab}** | **{tot_ku}** | **{tot_cap} / {tot_all}** |"
    )
    return "\n".join(lines)


README_PROGRAM_TABLE_RE = re.compile(
    r"(\| 目录 \| 题目 \| 覆盖面(?: \| 归属章节 \| 当前状态 \| 前置 Probe 就绪 \| 路线图阻塞项)? \|\n"
    r"\|---\|---\|---(?:\|---\|---\|---:\|---)?\|\n"
    r"(?:\| `(?:word_count|config_parse|expr_eval|raii_state_machine|varint_codec|linked_stack)` \|[^\n]+\n)+"
    r"\| `(?:word_count|config_parse|expr_eval|raii_state_machine|varint_codec|linked_stack)` \|[^\n]+)"
)

README_CHAPTER_TABLE_RE = re.compile(
    r"(\| Guide v0\.4[67] 章节 \| Probe 总数 \| `run_ok` \| `compile_fail` \| `runtime_abort` \| `known_unimplemented` \| 能力通过 \(`capability_passed`\) \|\n"
    r"\|---\|---:\|---:\|---:\|---:\|---:\|---:\|\n"
    r"(?:\| \[[^\n]+\n)+"
    r"\| \*\*合计\*\* \|[^\n]+)"
)


def sync_readme_chapter_table(readme_text: str, expectations: dict[str, Any]) -> str:
    """Replace the Guide chapter and V1 acceptance program summary tables in conformance/README.md with fresh counts."""
    table_md = render_readme_chapter_table(expectations)
    if not README_CHAPTER_TABLE_RE.search(readme_text):
        raise ValueError("conformance/README.md is missing the Guide chapter coverage table")
    updated = README_CHAPTER_TABLE_RE.sub(lambda _: table_md, readme_text, count=1)
    if README_PROGRAM_TABLE_RE.search(updated):
        prog_md = render_readme_program_table(expectations)
        updated = README_PROGRAM_TABLE_RE.sub(lambda _: prog_md, updated, count=1)
    return updated


def build_static_audit_summary(
    expectations: dict[str, Any],
    corpus_root: Path | None = None,
) -> dict[str, Any]:
    """Build a deterministic machine-readable JSON audit summary of the conformance corpus without invoking compiler."""
    from conformance_audit import (
        summarize_foundational_cohort,
        summarize_guide_litmus_alignment,
        summarize_stage_progression,
        summarize_v2_chapter_readiness,
        summarize_v3_sanitizer_coverage,
    )

    probes = expectations["probes"]
    programs = expectations["programs"]
    kind_counts: dict[str, int] = {
        "run_ok": 0,
        "compile_fail": 0,
        "runtime_abort": 0,
        "known_unimplemented": 0,
        "unreviewed": 0,
    }
    for spec in probes.values():
        k = spec["kind"]
        kind_counts[k] = kind_counts.get(k, 0) + 1
    cap_passed = (
        kind_counts["run_ok"] + kind_counts["compile_fail"] + kind_counts["runtime_abort"]
    )
    cohort = summarize_foundational_cohort(expectations)
    v2_ready = summarize_v2_chapter_readiness(expectations)
    litmus = summarize_guide_litmus_alignment(expectations)
    v3_cov = summarize_v3_sanitizer_coverage(expectations)
    stage_prog = summarize_stage_progression(expectations)
    pt_contracts = summarize_project_transform_contracts(expectations, corpus_root=corpus_root)

    return {
        "schema": "koven.conformance-audit-summary.v1",
        "total_probes": len(probes),
        "probe_capability_passed": cap_passed,
        "probe_kind_counts": kind_counts,
        "total_programs": len(programs),
        "program_run_ok": sum(1 for s in programs.values() if s["kind"] == "run_ok"),
        "foundational_p2": {
            "group_a_total": cohort["groups"]["A"]["total"],
            "group_a_runnable": cohort["groups"]["A"]["runnable"],
            "p2_target_min": cohort["p2_target_min"],
            "p2_gap_to_target": cohort["p2_gap_to_target"],
            "p2_gate_met": cohort["p2_gate_met"],
        },
        "v2_chapter_readiness": {
            "v2_probe_ready": [
                ch for ch, r in v2_ready.items() if r["v2_status"] == "v2_probe_ready"
            ],
            "blocked_by_roadmap": [
                ch for ch, r in v2_ready.items() if r["v2_status"] == "blocked_by_roadmap"
            ],
            "needs_negative_and_roadmap": [
                ch
                for ch, r in v2_ready.items()
                if r["v2_status"] == "needs_negative_and_roadmap"
            ],
        },
        "litmus_alignment": {
            "all_passed": [k for k, v in litmus.items() if v["status"] == "all_passed"],
            "partial": [k for k, v in litmus.items() if v["status"] == "partial"],
        },
        "v3_sanitizer_coverage": {
            "all_runnable": [
                k for k, v in v3_cov.items() if v["status"] == "all_runnable"
            ],
            "partial": [k for k, v in v3_cov.items() if v["status"] == "partial"],
        },
        "stage_progression": [
            {
                "stage": s["stage"],
                "projected_probe_cap": s["projected_probe_cap"],
                "projected_group_a_runnable": s["projected_group_a_runnable"],
                "projected_programs_ready": s["projected_programs_ready"],
            }
            for s in stage_prog
        ],
        "project_transform": {
            "header_utf8": pt_contracts["header_utf8"],
            "entry_selector": pt_contracts["entry_selector"],
            "explicit_import_probes": len(
                pt_contracts["cohorts"]["probes_explicit_import"]["probes"]
            ),
        },
    }
