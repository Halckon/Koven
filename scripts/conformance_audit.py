#!/usr/bin/env python3
"""Corpus polarity, Foundational P2 cohort, Litmus, roadmap stage/phase, and V3 sanitizer audits."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from conformance_catalogs import (
    DIAGNOSTIC_SYMBOL_CATALOG,
    FOUNDATIONAL_BATCHES,
    FOUNDATIONAL_GROUP_B_PROBES,
    FOUNDATIONAL_GROUP_C_PROBES,
    GUIDE_CHAPTER_DOMAINS,
    ORACLE_ORIGIN_DESCRIPTIONS,
    P2_GROUP_A_TARGET_MIN,
    PROBE_BATCH_CATALOG,
    PROGRAM_PREREQUISITE_PROBES,
    UNIMPLEMENTED_MARKER_CATALOG,
    VALID_GUIDE_CHAPTERS,
    classify_probe_batch,
)
from conformance_expectations import (
    GUIDE_LITMUS_CATALOG,
    GUIDE_VERSION_BOUNDARY_CATALOG,
    V3_SANITIZER_RESOURCE_CATALOG,
    resolve_spec_doc_relpath,
)


def summarize_chapter_polarity(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Compute per-chapter positive (run_ok) vs negative (compile_fail + runtime_abort) polarity coverage."""
    probes = expectations["probes"]
    chapter_stats: dict[str, dict[str, Any]] = {
        ch: {
            "total": 0,
            "run_ok": 0,
            "compile_fail": 0,
            "runtime_abort": 0,
            "known_unimplemented": 0,
            "cap_passed": 0,
            "positive_active": 0,
            "negative_active": 0,
            "has_positive": False,
            "has_negative": False,
            "polarity_status": "unimplemented_only",
        }
        for ch in VALID_GUIDE_CHAPTERS
    }
    for spec in probes.values():
        ch = spec.get("guide_chapter", "05-declarations-callables.md")
        row = chapter_stats[ch]
        row["total"] += 1
        kind = spec["kind"]
        row[kind] = row.get(kind, 0) + 1
        if kind in ("run_ok", "compile_fail", "runtime_abort"):
            row["cap_passed"] += 1

    for ch, row in chapter_stats.items():
        pos = row["run_ok"]
        neg = row["compile_fail"] + row["runtime_abort"]
        row["positive_active"] = pos
        row["negative_active"] = neg
        row["has_positive"] = pos > 0
        row["has_negative"] = neg > 0
        if pos > 0 and neg > 0:
            row["polarity_status"] = "both_positive_and_negative"
        elif pos > 0:
            row["polarity_status"] = "positive_only"
        elif neg > 0:
            row["polarity_status"] = "negative_only"
        else:
            row["polarity_status"] = "unimplemented_only"
    return chapter_stats


def summarize_negative_oracle_catalog(expectations: dict[str, Any]) -> dict[str, Any]:
    """Aggregate compile_fail diagnostic codes and runtime_abort contracts across the probe corpus."""
    probes = expectations["probes"]
    by_diagnostic: dict[str, dict[str, Any]] = {}
    by_abort: list[dict[str, Any]] = []

    for name, spec in sorted(probes.items()):
        ch = spec.get("guide_chapter", "05-declarations-callables.md")
        if spec["kind"] == "compile_fail":
            codes = spec.get("all_diagnostics", [spec["diagnostic"]])
            for code in codes:
                bucket = by_diagnostic.setdefault(
                    code,
                    {
                        "code": code,
                        "symbol": DIAGNOSTIC_SYMBOL_CATALOG.get(code, "UNKNOWN"),
                        "chapters": [],
                        "probes": [],
                    },
                )
                if ch not in bucket["chapters"]:
                    bucket["chapters"].append(ch)
                bucket["probes"].append(name)
        elif spec["kind"] == "runtime_abort":
            by_abort.append(
                {
                    "probe": name,
                    "abort_message": spec["abort_message"],
                    "guide_chapter": ch,
                    "derivation": spec.get("oracle_derivation", ""),
                }
            )

    return {
        "diagnostics": dict(sorted(by_diagnostic.items())),
        "runtime_aborts": by_abort,
    }


def summarize_baseline_evolution(expectations: dict[str, Any]) -> dict[str, list[str]]:
    """Classify all probes by their transition from historical baseline_marker to current expectation kind."""
    buckets: dict[str, list[str]] = {
        "baseline_ok_preserved": [],
        "promoted_to_run_ok": [],
        "upgraded_to_structured_abort": [],
        "upgraded_to_compile_fail": [],
        "baseline_compile_fail_preserved": [],
        "remaining_known_unimplemented": [],
        "regressed_from_baseline_ok": [],
    }
    for name, spec in sorted(expectations["probes"].items()):
        bm = spec.get("baseline_marker", "")
        kind = spec["kind"]
        if bm == "OK" and kind == "run_ok":
            buckets["baseline_ok_preserved"].append(name)
        elif bm == "OK" and kind != "run_ok":
            buckets["regressed_from_baseline_ok"].append(name)
        elif bm != "OK" and kind == "run_ok":
            buckets["promoted_to_run_ok"].append(name)
        elif kind == "runtime_abort":
            buckets["upgraded_to_structured_abort"].append(name)
        elif kind == "compile_fail" and spec.get("v046_baseline_exempt"):
            buckets["upgraded_to_compile_fail"].append(name)
        elif kind == "compile_fail":
            buckets["baseline_compile_fail_preserved"].append(name)
        elif kind == "known_unimplemented":
            buckets["remaining_known_unimplemented"].append(name)
    return buckets


def summarize_v046_spec_traceability(
    expectations: dict[str, Any], repo_root: Path | None = None
) -> dict[str, dict[str, Any]]:
    """Aggregate all v046_baseline_exempt probes by their governing SPEC-xxxx contract IDs."""
    root = repo_root or Path(__file__).resolve().parents[1]
    by_spec: dict[str, dict[str, Any]] = {}
    for name, spec in sorted(expectations["probes"].items()):
        if not spec.get("v046_baseline_exempt"):
            continue
        for sref in spec.get("spec_refs", []):
            resolved = resolve_spec_doc_relpath(sref, root)
            relpath, status = resolved if resolved is not None else ("", "unknown")
            bucket = by_spec.setdefault(
                sref,
                {
                    "spec_id": sref,
                    "doc_relpath": relpath,
                    "status": status,
                    "probes": [],
                    "kinds": {},
                },
            )
            bucket["probes"].append(name)
            k = spec["kind"]
            bucket["kinds"][k] = bucket["kinds"].get(k, 0) + 1
    return dict(sorted(by_spec.items()))


def summarize_oracle_origins(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Aggregate probes and programs by their canonical oracle_origin classification."""
    order = (
        "single_println_literal",
        "source_trace_audited",
        "v046_spec_contract",
        "baseline_diagnostic_audited",
        "baseline_unimplemented_audited",
        "expected_txt_file",
    )
    summary: dict[str, dict[str, Any]] = {
        origin: {
            "origin": origin,
            "description": ORACLE_ORIGIN_DESCRIPTIONS[origin],
            "probes": 0,
            "programs": 0,
            "kinds": {},
        }
        for origin in order
    }
    for section, key in (("probes", "probes"), ("programs", "programs")):
        for spec in expectations[section].values():
            origin = spec.get("oracle_origin")
            if origin not in summary:
                continue
            bucket = summary[origin]
            bucket[key] += 1
            k = spec["kind"]
            bucket["kinds"][k] = bucket["kinds"].get(k, 0) + 1
    return summary


def summarize_output_shape_distribution(expectations: dict[str, Any]) -> dict[str, Any]:
    """Classify all 154 probes and 6 programs by their stdout line/byte shape."""
    single_line_probes: list[str] = []
    multi_line_probes: list[dict[str, Any]] = []
    zero_stdout_probes: list[str] = []
    total_probe_lines = 0
    total_probe_bytes = 0

    for name, spec in sorted(expectations["probes"].items()):
        out_text = spec.get("stdout") or spec.get("target_stdout")
        if isinstance(out_text, str) and out_text:
            ln_cnt = spec.get("expected_lines", len(out_text.splitlines()))
            by_cnt = spec.get("expected_bytes", len(out_text.encode("utf-8")))
            total_probe_lines += ln_cnt
            total_probe_bytes += by_cnt
            if ln_cnt == 1:
                single_line_probes.append(name)
            else:
                multi_line_probes.append(
                    {
                        "name": name,
                        "kind": spec["kind"],
                        "lines": ln_cnt,
                        "bytes": by_cnt,
                    }
                )
        else:
            zero_stdout_probes.append(name)

    total_prog_lines = sum(
        spec.get("expected_lines", 0) for spec in expectations["programs"].values()
    )
    total_prog_bytes = sum(
        spec.get("expected_bytes", 0) for spec in expectations["programs"].values()
    )
    return {
        "single_line_probes": single_line_probes,
        "multi_line_probes": multi_line_probes,
        "zero_stdout_probes": zero_stdout_probes,
        "total_probe_lines": total_probe_lines,
        "total_probe_bytes": total_probe_bytes,
        "total_program_lines": total_prog_lines,
        "total_program_bytes": total_prog_bytes,
    }


def summarize_probe_batches(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Aggregate all probes by their canonical single-letter batch prefix (p..e)."""
    summary: dict[str, dict[str, Any]] = {
        batch: {
            "batch": batch,
            "description": desc,
            "total": 0,
            "run_ok": 0,
            "compile_fail": 0,
            "runtime_abort": 0,
            "known_unimplemented": 0,
            "unreviewed": 0,
            "cap_passed": 0,
            "batch_status": "empty",
            "probes": [],
        }
        for batch, desc in PROBE_BATCH_CATALOG.items()
    }
    for name, spec in sorted(expectations["probes"].items()):
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        row = summary[batch]
        row["total"] += 1
        row["probes"].append(name)
        kind = spec["kind"]
        row[kind] = row.get(kind, 0) + 1
        if kind in ("run_ok", "compile_fail", "runtime_abort"):
            row["cap_passed"] += 1

    for row in summary.values():
        if row["total"] > 0 and row["cap_passed"] == row["total"]:
            row["batch_status"] = "all_passed"
        elif row["total"] > 0:
            row["batch_status"] = "partial"
    return summary


def summarize_unimplemented_markers(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Aggregate known_unimplemented probes and programs by allowed unimplemented marker."""
    summary: dict[str, dict[str, Any]] = {
        marker: {
            "marker": marker,
            "stage": stage,
            "description": desc,
            "probes": [],
            "programs": [],
            "baseline_count": 0,
        }
        for marker, (stage, desc) in UNIMPLEMENTED_MARKER_CATALOG.items()
    }
    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        bm = spec.get("baseline_marker")
        if bm in summary:
            summary[bm]["baseline_count"] += 1
        for m in spec.get("allowed_markers", []):
            if m in summary:
                summary[m]["probes"].append(name)
    for name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for m in spec.get("allowed_markers", []):
            if m in summary:
                summary[m]["programs"].append(name)
    return summary


def summarize_program_roadmap_blockers(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Aggregate all known_unimplemented acceptance programs by their roadmap_blockers."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    by_blocker: dict[str, dict[str, Any]] = {}
    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            bucket = by_blocker.setdefault(
                b_id,
                {
                    "id": b_id,
                    "title": item.get("title", "").replace("|", "\\|"),
                    "status": item.get("status", "unknown"),
                    "programs": [],
                    "probes": [],
                },
            )
            bucket["programs"].append(prog_name)

    for probe_name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            if b_id in by_blocker:
                by_blocker[b_id]["probes"].append(probe_name)

    return dict(sorted(by_blocker.items()))


def summarize_foundational_cohort(expectations: dict[str, Any]) -> dict[str, Any]:
    """Classify the 85 foundational p/q/r/s probes into Groups A (74), B (8), and C (3) for P2 gate tracking."""
    groups: dict[str, dict[str, Any]] = {
        "A": {
            "label": "A 组：合法基础程序（P2 门槛基数）",
            "total": 0,
            "runnable": 0,
            "run_ok": [],
            "runtime_abort": [],
            "compile_fail": [],
            "known_unimplemented": [],
        },
        "B": {
            "label": "B 组：应拒基础探针（静态/借用/类型拒绝）",
            "total": 0,
            "runnable": 0,
            "run_ok": [],
            "runtime_abort": [],
            "compile_fail": [],
            "known_unimplemented": [],
        },
        "C": {
            "label": "C 组：v0.44 待裁决项（已在 v0.46 规范定案）",
            "total": 0,
            "runnable": 0,
            "run_ok": [],
            "runtime_abort": [],
            "compile_fail": [],
            "known_unimplemented": [],
        },
    }
    for name, spec in sorted(expectations["probes"].items()):
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        if batch not in FOUNDATIONAL_BATCHES:
            continue
        if name in FOUNDATIONAL_GROUP_B_PROBES:
            grp = groups["B"]
        elif name in FOUNDATIONAL_GROUP_C_PROBES:
            grp = groups["C"]
        else:
            grp = groups["A"]
        grp["total"] += 1
        kind = spec["kind"]
        grp[kind].append(name)
        if kind in ("run_ok", "runtime_abort"):
            grp["runnable"] += 1

    group_a = groups["A"]
    return {
        "total_foundational": sum(g["total"] for g in groups.values()),
        "p2_target_min": P2_GROUP_A_TARGET_MIN,
        "p2_gap_to_target": max(0, P2_GROUP_A_TARGET_MIN - group_a["runnable"]),
        "p2_gate_met": group_a["runnable"] >= P2_GROUP_A_TARGET_MIN,
        "groups": groups,
    }


def summarize_v2_chapter_readiness(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Evaluate each of the 13 Guide chapters against V2 exit criteria (positive + negative polarity and 0 known_unimplemented)."""
    chapter_polarity = summarize_chapter_polarity(expectations)
    readiness: dict[str, dict[str, Any]] = {}
    for ch in VALID_GUIDE_CHAPTERS:
        pol = chapter_polarity[ch]
        neg_contracts: list[str] = []
        unimpl_probes: list[str] = []
        blockers: set[str] = set()
        programs_in_ch: list[str] = []

        for name, spec in sorted(expectations["probes"].items()):
            if spec.get("guide_chapter") != ch:
                continue
            if spec["kind"] == "compile_fail":
                for code in spec.get("all_diagnostics", [spec["diagnostic"]]):
                    tag = f"FE:{code}"
                    if tag not in neg_contracts:
                        neg_contracts.append(tag)
            elif spec["kind"] == "runtime_abort":
                tag = f"abort:{spec['abort_message']}"
                if tag not in neg_contracts:
                    neg_contracts.append(tag)
            elif spec["kind"] == "known_unimplemented":
                unimpl_probes.append(name)
                blockers.update(spec.get("roadmap_blockers", []))

        for prog_name, prog_spec in sorted(expectations["programs"].items()):
            if prog_spec.get("guide_chapter") == ch:
                programs_in_ch.append(prog_name)

        has_pos = pol["has_positive"]
        has_neg = pol["has_negative"]
        ku_count = pol["known_unimplemented"]
        if has_pos and has_neg and ku_count == 0:
            v2_status = "v2_probe_ready"
        elif has_pos and has_neg and ku_count > 0:
            v2_status = "blocked_by_roadmap"
        elif not has_neg and ku_count > 0:
            v2_status = "needs_negative_and_roadmap"
        else:
            v2_status = "needs_negative_only"

        readiness[ch] = {
            "chapter": ch,
            "domain": GUIDE_CHAPTER_DOMAINS[ch],
            "total_probes": pol["total"],
            "positive_active": pol["positive_active"],
            "negative_active": pol["negative_active"],
            "known_unimplemented": ku_count,
            "negative_contracts": neg_contracts,
            "unimplemented_probes": unimpl_probes,
            "roadmap_blockers": sorted(blockers),
            "acceptance_programs": programs_in_ch,
            "v2_status": v2_status,
        }
    return readiness


def summarize_blocker_unlock_impact(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Analyze sole-blocker immediate probe unlocks vs co-blocked items and P2 Group A gate contribution per roadmap item."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    impact: dict[str, dict[str, Any]] = {}

    def _ensure_bucket(b_id: str) -> dict[str, Any]:
        if b_id not in impact:
            item = roadmap_items.get(b_id, {})
            impact[b_id] = {
                "id": b_id,
                "stage": item.get("stage", "-"),
                "status": item.get("status", "unknown"),
                "title": item.get("title", "").replace("|", "\\|"),
                "sole_probes": [],
                "co_probes": [],
                "p2_group_a_probes": [],
                "programs": [],
            }
        return impact[b_id]

    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        blockers = spec.get("roadmap_blockers", [])
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        is_group_a = (
            batch in FOUNDATIONAL_BATCHES
            and name not in FOUNDATIONAL_GROUP_B_PROBES
            and name not in FOUNDATIONAL_GROUP_C_PROBES
        )
        for b_id in blockers:
            bucket = _ensure_bucket(b_id)
            if len(blockers) == 1:
                bucket["sole_probes"].append(name)
            else:
                bucket["co_probes"].append(name)
            if is_group_a:
                bucket["p2_group_a_probes"].append(name)

    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            bucket = _ensure_bucket(b_id)
            bucket["programs"].append(prog_name)

    return dict(sorted(impact.items()))


ROADMAP_STAGE_CATALOG: dict[str, str] = {
    "P2": "Native 后端能力补齐（泛型、可空/Result `?`、闭包、浮点/字符与接口分派）",
    "P3": "语言糖、协议与标准库收口（Equatable/Display 插值、位运算、多载荷 enum 与 Pair/to）",
    "P4": "并发与扩展（原生线程 thread 与 JoinHandle）",
}


def summarize_stage_progression(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    """Project cumulative probe capability, Foundational Group A runnable count, and V1 program unlocks across stages P2 -> P3 -> P4."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    probes = expectations["probes"]
    programs = expectations["programs"]
    base_cap = sum(
        1 for s in probes.values() if s["kind"] in ("run_ok", "compile_fail", "runtime_abort")
    )
    cohort = summarize_foundational_cohort(expectations)
    base_group_a = cohort["groups"]["A"]["runnable"]
    base_progs = sum(1 for s in programs.values() if s["kind"] == "run_ok")

    cumulative_stages: set[str] = set()
    prev_unlocked_probes: set[str] = set()
    prev_unlocked_progs: set[str] = set()
    rows: list[dict[str, Any]] = []

    for stage_id, stage_desc in ROADMAP_STAGE_CATALOG.items():
        cumulative_stages.add(stage_id)
        stage_blockers = sorted(
            b_id
            for b_id, item in roadmap_items.items()
            if item.get("stage") == stage_id
            and any(
                b_id in s.get("roadmap_blockers", [])
                for s in list(probes.values()) + list(programs.values())
                if s["kind"] == "known_unimplemented"
            )
        )
        now_unlocked_probes: list[str] = []
        cum_group_a_gain = 0
        for name, spec in sorted(probes.items()):
            if spec["kind"] != "known_unimplemented":
                continue
            blockers = spec.get("roadmap_blockers", [])
            if blockers and all(
                roadmap_items.get(b, {}).get("stage", "P2") in cumulative_stages
                for b in blockers
            ):
                if name not in prev_unlocked_probes:
                    now_unlocked_probes.append(name)
                    prev_unlocked_probes.add(name)
                batch = spec.get("probe_batch") or classify_probe_batch(name)
                if (
                    batch in FOUNDATIONAL_BATCHES
                    and name not in FOUNDATIONAL_GROUP_B_PROBES
                    and name not in FOUNDATIONAL_GROUP_C_PROBES
                ):
                    cum_group_a_gain += 1

        now_unlocked_progs: list[str] = []
        for prog_name, spec in sorted(programs.items()):
            if spec["kind"] != "known_unimplemented":
                continue
            blockers = spec.get("roadmap_blockers", [])
            if blockers and all(
                roadmap_items.get(b, {}).get("stage", "P2") in cumulative_stages
                for b in blockers
            ):
                if prog_name not in prev_unlocked_progs:
                    now_unlocked_progs.append(prog_name)
                    prev_unlocked_progs.add(prog_name)

        rows.append(
            {
                "stage": stage_id,
                "description": stage_desc,
                "stage_blockers": stage_blockers,
                "newly_unlocked_probes": now_unlocked_probes,
                "newly_unlocked_programs": now_unlocked_progs,
                "projected_probe_cap": base_cap + len(prev_unlocked_probes),
                "total_probes": len(probes),
                "projected_group_a_runnable": base_group_a + cum_group_a_gain,
                "group_a_total": cohort["groups"]["A"]["total"],
                "projected_programs_ready": base_progs + len(prev_unlocked_progs),
                "total_programs": len(programs),
            }
        )
    return rows


ROADMAP_AREA_CATALOG: dict[str, str] = {
    "types": "类型系统、标量字面量 (`Double`/`Char`)、位运算与 `Equatable`/`Comparable` 协议 (`L1`, `L3b`, `N05`)",
    "control": "多载荷与递归 enum 构造及 `when` 解构分派 (`L7`, `N02`)",
    "closures": "函数引用 `::fn` 与移动闭包调用 (`N04`)",
    "classes": "泛型 Interface 上界分派与匿名 `companion` 关联调用 (`N06`)",
    "errors": "可空链式访问、Elvis `?:` 与 `Result` `?` 传播 (`N03`, `N08`)",
    "ownership": "嵌套泛型 class 字段借用、可变写入与所有权证明 (`N01`)",
    "collections": "内建 `Pair` 构造与中缀 `to` 糖 (`S6`)",
    "text-io": "`Display` 协议与字符串插值求值 (`L2`)",
    "stdlib": "递归堆分配容器 `Box<T>` 与原生线程 `thread`/`JoinHandle` (`N07`, `T1`)",
}


def summarize_roadmap_area_impact(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Aggregate known_unimplemented probes and acceptance programs by roadmap area and compiler phases."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    by_area: dict[str, dict[str, Any]] = {
        area_key: {
            "area": area_key,
            "description": desc,
            "blockers": [],
            "phases": [],
            "probes": [],
            "programs": [],
        }
        for area_key, desc in ROADMAP_AREA_CATALOG.items()
    }
    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            area = item.get("area", "types")
            bucket = by_area.setdefault(
                area,
                {
                    "area": area,
                    "description": area,
                    "blockers": [],
                    "phases": [],
                    "probes": [],
                    "programs": [],
                },
            )
            if b_id not in bucket["blockers"]:
                bucket["blockers"].append(b_id)
                bucket["blockers"].sort()
            for ph in item.get("phases", []):
                if ph not in bucket["phases"]:
                    bucket["phases"].append(ph)
                    bucket["phases"].sort()
            if name not in bucket["probes"]:
                bucket["probes"].append(name)

    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            area = item.get("area", "types")
            bucket = by_area[area]
            if b_id not in bucket["blockers"]:
                bucket["blockers"].append(b_id)
                bucket["blockers"].sort()
            for ph in item.get("phases", []):
                if ph not in bucket["phases"]:
                    bucket["phases"].append(ph)
                    bucket["phases"].sort()
            if prog_name not in bucket["programs"]:
                bucket["programs"].append(prog_name)

    return by_area


def summarize_unimplemented_promotion_contracts(
    expectations: dict[str, Any],
) -> list[dict[str, Any]]:
    """Return structured promotion contract rows for all remaining known_unimplemented probes."""
    rows: list[dict[str, Any]] = []
    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        if batch not in FOUNDATIONAL_BATCHES:
            cohort_grp = "-"
        elif name in FOUNDATIONAL_GROUP_B_PROBES:
            cohort_grp = "B"
        elif name in FOUNDATIONAL_GROUP_C_PROBES:
            cohort_grp = "C"
        else:
            cohort_grp = "A"
        rows.append(
            {
                "name": name,
                "guide_chapter": spec["guide_chapter"],
                "probe_batch": batch,
                "foundational_group": cohort_grp,
                "baseline_marker": spec.get("baseline_marker", "-"),
                "allowed_markers": list(spec.get("allowed_markers", [])),
                "roadmap_blockers": list(spec.get("roadmap_blockers", [])),
                "target_stdout": spec.get("target_stdout", ""),
                "expected_lines": spec.get("expected_lines", 0),
                "expected_bytes": spec.get("expected_bytes", 0),
            }
        )
    return rows


COMPILER_PHASE_CATALOG: dict[int, tuple[str, str]] = {
    2: ("Phase 2 前端类型与解析", "名称解析、类型检查、运算符/协议约束与最终类型准入 (`crates/lang-frontend/src/type_checking`)"),
    3: ("Phase 3 所有权与析构", "借用冲突、部分移动、生命周期证明与逆序 `deinit` 调度 (`crates/lang-frontend/src/ownership`)"),
    4: ("Phase 4 SSA / LLVM / Runtime", "SSA 构建、所有权校验、LLVM IR 生成、原生链接与 `abort` 合同 (`crates/lang-codegen`)"),
    5: ("Phase 5 标准库与并发", "内建容器、文本/数值协议、`Box`/`Rc` 与原生线程运行时 (`crates/lang-std`)"),
}


def summarize_compiler_phase_matrix(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[int, dict[str, Any]]:
    """Summarize active negative diagnostics/aborts and remaining roadmap blockers across compiler Phases 2..5."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    phase_3_diags = {"L0132", "L0133", "L0135", "L0164"}
    phases_out: dict[int, dict[str, Any]] = {
        ph: {
            "phase": ph,
            "title": title,
            "description": desc,
            "active_negative_probes": [],
            "active_negative_contracts": [],
            "blockers": [],
            "blocked_probes": [],
            "blocked_programs": [],
        }
        for ph, (title, desc) in COMPILER_PHASE_CATALOG.items()
    }

    for name, spec in sorted(expectations["probes"].items()):
        kind = spec["kind"]
        if kind == "compile_fail":
            codes = spec.get("all_diagnostics", [spec["diagnostic"]])
            target_ph = 3 if any(c in phase_3_diags for c in codes) else 2
            phases_out[target_ph]["active_negative_probes"].append(name)
            for c in codes:
                tag = f"FE:{c}"
                if tag not in phases_out[target_ph]["active_negative_contracts"]:
                    phases_out[target_ph]["active_negative_contracts"].append(tag)
        elif kind == "runtime_abort":
            phases_out[4]["active_negative_probes"].append(name)
            tag = f"abort:{spec['abort_message']}"
            if tag not in phases_out[4]["active_negative_contracts"]:
                phases_out[4]["active_negative_contracts"].append(tag)
        elif kind == "known_unimplemented":
            for b_id in spec.get("roadmap_blockers", []):
                item = roadmap_items.get(b_id, {})
                for ph in item.get("phases", []):
                    if ph not in phases_out:
                        continue
                    if b_id not in phases_out[ph]["blockers"]:
                        phases_out[ph]["blockers"].append(b_id)
                        phases_out[ph]["blockers"].sort()
                    if name not in phases_out[ph]["blocked_probes"]:
                        phases_out[ph]["blocked_probes"].append(name)

    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            for ph in item.get("phases", []):
                if ph not in phases_out:
                    continue
                if b_id not in phases_out[ph]["blockers"]:
                    phases_out[ph]["blockers"].append(b_id)
                    phases_out[ph]["blockers"].sort()
                if prog_name not in phases_out[ph]["blocked_programs"]:
                    phases_out[ph]["blocked_programs"].append(prog_name)

    return phases_out


def summarize_guide_litmus_alignment(expectations: dict[str, Any]) -> dict[int, dict[str, Any]]:
    """Cross-reference Guide 15 Litmus 1..12 topics against their mapped end-to-end conformance probes."""
    probes = expectations["probes"]
    out: dict[int, dict[str, Any]] = {}
    for lit_id, spec in GUIDE_LITMUS_CATALOG.items():
        mapped = list(spec["probes"])
        cap_passed = [
            p for p in mapped if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in mapped if probes[p]["kind"] == "known_unimplemented"]
        blockers = sorted(
            {b for p in unimpl for b in probes[p].get("roadmap_blockers", [])}
        )
        out[lit_id] = {
            "id": lit_id,
            "title": spec["title"],
            "guide_chapter": spec["guide_chapter"],
            "probes": mapped,
            "cap_passed_count": len(cap_passed),
            "unimplemented_probes": unimpl,
            "roadmap_blockers": blockers,
            "status": "all_passed" if not unimpl else "partial",
        }
    return out


def summarize_guide_version_boundaries(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Cross-reference Guide 15 version enablement boundaries (v0.44..v0.47) against their conformance probes."""
    probes = expectations["probes"]
    out: dict[str, dict[str, Any]] = {}
    for ver_key, spec in GUIDE_VERSION_BOUNDARY_CATALOG.items():
        mapped = list(spec["probes"])
        cap_passed = [
            p for p in mapped if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in mapped if probes[p]["kind"] == "known_unimplemented"]
        blockers = sorted(
            {b for p in unimpl for b in probes[p].get("roadmap_blockers", [])}
        )
        out[ver_key] = {
            "version": ver_key,
            "summary": spec["summary"],
            "probes": mapped,
            "cap_passed_count": len(cap_passed),
            "unimplemented_probes": unimpl,
            "roadmap_blockers": blockers,
            "status": "all_passed" if not unimpl else "partial",
        }
    return out


def summarize_program_probe_prerequisites(
    expectations: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    """Cross-reference the 6 V1 acceptance programs against their atomic prerequisite conformance probes."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    out: dict[str, dict[str, Any]] = {}
    for prog_name, pre_list in sorted(PROGRAM_PREREQUISITE_PROBES.items()):
        passed = [
            p for p in pre_list if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in pre_list if probes[p]["kind"] == "known_unimplemented"]
        out[prog_name] = {
            "program": prog_name,
            "guide_chapter": programs[prog_name]["guide_chapter"],
            "roadmap_blockers": list(programs[prog_name].get("roadmap_blockers", [])),
            "prerequisite_probes": list(pre_list),
            "passed_probes": passed,
            "unimplemented_probes": unimpl,
        }
    return out


def summarize_corpus_source_scale(expectations: dict[str, Any]) -> dict[str, Any]:
    """Aggregate source line and byte metrics across all 154 probes, 6 programs, and 13 Guide chapters."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    by_chapter: dict[str, dict[str, Any]] = {
        ch: {
            "chapter": ch,
            "domain": GUIDE_CHAPTER_DOMAINS[ch],
            "probe_count": 0,
            "probe_source_lines": 0,
            "probe_source_bytes": 0,
            "program_count": 0,
            "program_source_lines": 0,
            "program_source_bytes": 0,
            "programs": [],
        }
        for ch in VALID_GUIDE_CHAPTERS
    }
    tot_probe_lines = 0
    tot_probe_bytes = 0
    for name, spec in sorted(probes.items()):
        ch = spec.get("guide_chapter", "05-declarations-callables.md")
        sl = int(spec.get("source_lines", 0))
        sb = int(spec.get("source_bytes", 0))
        tot_probe_lines += sl
        tot_probe_bytes += sb
        if ch in by_chapter:
            by_chapter[ch]["probe_count"] += 1
            by_chapter[ch]["probe_source_lines"] += sl
            by_chapter[ch]["probe_source_bytes"] += sb

    tot_prog_lines = 0
    tot_prog_bytes = 0
    for name, spec in sorted(programs.items()):
        ch = spec.get("guide_chapter", "13-program-runtime-standard-library.md")
        sl = int(spec.get("source_lines", 0))
        sb = int(spec.get("source_bytes", 0))
        tot_prog_lines += sl
        tot_prog_bytes += sb
        if ch in by_chapter:
            by_chapter[ch]["program_count"] += 1
            by_chapter[ch]["program_source_lines"] += sl
            by_chapter[ch]["program_source_bytes"] += sb
            by_chapter[ch]["programs"].append(name)

    return {
        "total_probe_count": len(probes),
        "total_probe_source_lines": tot_probe_lines,
        "total_probe_source_bytes": tot_probe_bytes,
        "total_program_count": len(programs),
        "total_program_source_lines": tot_prog_lines,
        "total_program_source_bytes": tot_prog_bytes,
        "total_corpus_source_lines": tot_probe_lines + tot_prog_lines,
        "total_corpus_source_bytes": tot_probe_bytes + tot_prog_bytes,
        "by_chapter": by_chapter,
    }


def summarize_v3_sanitizer_coverage(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Cross-reference V3 native sanitizer (ASan/LSan) resource-lifecycle categories against probes and programs."""
    probes = expectations["probes"]
    out: dict[str, dict[str, Any]] = {}
    for cat_key, spec in V3_SANITIZER_RESOURCE_CATALOG.items():
        c_probes = list(spec["probes"])
        c_progs = list(spec["programs"])
        runnable = [
            p for p in c_probes if probes[p]["kind"] in ("run_ok", "runtime_abort")
        ]
        unimpl = [p for p in c_probes if probes[p]["kind"] == "known_unimplemented"]
        blockers = sorted(
            {b for p in unimpl for b in probes[p].get("roadmap_blockers", [])}
        )
        out[cat_key] = {
            "category": cat_key,
            "title": spec["title"],
            "sanitizer_focus": spec["sanitizer_focus"],
            "probes": c_probes,
            "runnable_probes": runnable,
            "unimplemented_probes": unimpl,
            "roadmap_blockers": blockers,
            "programs": c_progs,
            "status": "all_runnable" if not unimpl else "partial",
        }
    return out
