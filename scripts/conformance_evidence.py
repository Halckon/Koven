#!/usr/bin/env python3
"""Offline evidence bundle verification, dual-host parity comparison, and TSV matrix checks."""

from __future__ import annotations

import base64
import json
from pathlib import Path
import re
from typing import Any

from conformance_catalogs import (
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
