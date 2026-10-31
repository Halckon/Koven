#!/usr/bin/env python3
"""Public-entry conformance runner, byte-exact oracle comparator, and regression gate.

Responsibilities (V2 first slice):
1. Execute probes and acceptance programs via public CLI (`kovenc run`) in both
   single-file mode and single-file project mode (`package app` + `project.toml`),
   preserving raw stdout/stderr bytes, exact exit status, and timeout state separately.
2. Bind compiler binary SHA-256, git commit, host platform, original source SHA-256,
   and explicit project-mode transformation metadata (`prepend_package_header`).
3. Compare execution results against machine-readable expectations (`expectations.json`)
   and historical baselines (`baseline-*.tsv`), strictly distinguishing:
   - `pass_run` (exit 0, empty stderr, exact byte match on stdout including trailing newline)
   - `pass_diagnostic` (non-zero exit, empty stdout, expected `FE:Lxxxx` diagnostic)
   - `pass_abort` (SIGABRT / exit 134, expected `abort: <message>` header on stderr)
   - `known_unimplemented` (matches allowed unimplemented markers; NEVER counted as capability pass)
   - `unreviewed` (oracle not yet human-confirmed; blocks strict gate, never auto-blessed)
   - `wrong_output` (exit 0 with mismatched stdout, missing trailing newline, silent output, or dirty stderr)
   - `mode_mismatch` (single and project differ in marker or stdout bytes)
   - `regression` (expected pass or historical baseline OK regressed to failure; both-rejected never counts as pass)
   - `internal_error` (`BE:InvalidSsa`, `ICE:ownership`, `timeout`, unexplained `ERR`, or silent crash)
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
from typing import Any

_SCRIPTS_DIR = str(Path(__file__).resolve().parent)
if _SCRIPTS_DIR not in sys.path:
    sys.path.insert(0, _SCRIPTS_DIR)

from conformance_catalogs import *  # noqa: F401,F403
from conformance_expectations import *  # noqa: F401,F403
from conformance_runner import *  # noqa: F401,F403
from conformance_evidence import *  # noqa: F401,F403
from conformance_audit import *  # noqa: F401,F403


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


def render_oracle_audit_markdown(expectations: dict[str, Any]) -> str:
    """Render deterministic Markdown audit ledger from expectations.json."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    chapter_stats = summarize_chapter_polarity(expectations)
    neg_catalog = summarize_negative_oracle_catalog(expectations)
    base_evo = summarize_baseline_evolution(expectations)
    spec_trace = summarize_v046_spec_traceability(expectations)
    origin_stats = summarize_oracle_origins(expectations)
    shape_stats = summarize_output_shape_distribution(expectations)
    batch_stats = summarize_probe_batches(expectations)
    marker_stats = summarize_unimplemented_markers(expectations)
    foundational_stats = summarize_foundational_cohort(expectations)
    v2_readiness = summarize_v2_chapter_readiness(expectations)

    lines = [
        "# Conformance 强 Oracle 语义推导与 Guide v0.46 章节覆盖对账账本",
        "",
        "> 本文件由 `python3 scripts/conformance_gate.py --init-expectations` 从 `conformance/expectations.json` 确定性生成，并由 `scripts/check_docs.py`（`DocsChecker.check_conformance_corpus`）与 `scripts/tests/test_conformance_gate.py` 自动校验同步。",
        "",
        "## 1. Guide v0.46 章节覆盖与正/负向极性汇总（154 Probes）",
        "",
        "| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) | 正/负向极性状态 (`polarity_status`) |",
        "|---|---:|---:|---:|---:|---:|---:|---|",
    ]
    tot_all = tot_ok = tot_cf = tot_ab = tot_ku = tot_cap = 0
    both_chapters: list[str] = []
    pos_only_chapters: list[str] = []
    for ch in VALID_GUIDE_CHAPTERS:
        st = chapter_stats[ch]
        tot_all += st["total"]
        tot_ok += st["run_ok"]
        tot_cf += st["compile_fail"]
        tot_ab += st["runtime_abort"]
        tot_ku += st["known_unimplemented"]
        tot_cap += st["cap_passed"]
        pol = st["polarity_status"]
        if pol == "both_positive_and_negative":
            both_chapters.append(ch)
        elif pol == "positive_only":
            pos_only_chapters.append(ch)
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {st['total']} | {st['run_ok']} | {st['compile_fail']} | "
            f"{st['runtime_abort']} | {st['known_unimplemented']} | {st['cap_passed']} / {st['total']} | `{pol}` |"
        )
    lines.append(
        f"| **合计** | **{tot_all}** | **{tot_ok}** | **{tot_cf}** | **{tot_ab}** | **{tot_ku}** | **{tot_cap} / {tot_all}** | **{len(both_chapters)} 双极性 / {len(pos_only_chapters)} 仅正向** |"
    )
    lines.extend(
        [
            "",
            f"- **正/负向双极性覆盖章节（{len(both_chapters)} / {len(VALID_GUIDE_CHAPTERS)}）**：" + ", ".join(f"`{c}`" for c in both_chapters) + "（同时具备 `run_ok` 正向运行探针与 `compile_fail` / `runtime_abort` 负向拒绝或中止探针）。",
            f"- **仅正向覆盖章节（{len(pos_only_chapters)} / {len(VALID_GUIDE_CHAPTERS)}，待后续补充独立负向诊断探针）**：" + ", ".join(f"`{c}`" for c in pos_only_chapters) + "（当前语料库内仅有 `run_ok` 与 `known_unimplemented` 探针，尚无独立 `compile_fail` / `runtime_abort` 探针）。",
            "",
            "### 1.1 负向诊断码与运行时中止契约矩阵（14 `compile_fail` + 3 `runtime_abort`）",
            "",
            "| 负向契约标识 | `diagnostic.rs` 符号常量 / 中止类别 | 覆盖 Guide 章节 | 关联 Probe 数量 | 绑定 Probe 列表 |",
            "|---|---|---|---:|---|",
        ]
    )
    for code, info in neg_catalog["diagnostics"].items():
        ch_links = ", ".join(f"[{c}](../docs/guide/{c})" for c in info["chapters"])
        p_list = ", ".join(f"`{p}`" for p in info["probes"])
        lines.append(
            f"| `FE:{code}` | `{info['symbol']}` | {ch_links} | {len(info['probes'])} | {p_list} |"
        )
    for ab in neg_catalog["runtime_aborts"]:
        ch = ab["guide_chapter"]
        lines.append(
            f"| `abort:{ab['abort_message']}` | `SIGABRT (exit 134)` | [{ch}](../docs/guide/{ch}) | 1 | `{ab['probe']}` |"
        )
    lines.extend(
        [
            "",
            "### 1.2 历史基线 `baseline-aef3f4f8.tsv` 演进与零退化对账汇总（154 Probes）",
            "",
            "| 演进分类 (`baseline_evolution`) | 历史基线状态 (`baseline-aef3f4f8.tsv`) | 当前强 Oracle 状态 (`expectations.json`) | Probe 数量 | 代表性 / 完整 Probe 列表 |",
            "|---|---|---|---:|---|",
            f"| `baseline_ok_preserved` | `OK` | `run_ok`（精确字节匹配） | {len(base_evo['baseline_ok_preserved'])} | 历史 `OK` 基线 100% 保持通过（含 `q12_companion_const` 匿名 `companion {{ }}` 语法更新） |",
            f"| `promoted_to_run_ok` | `BE:InvalidSsa` / `BE:MissingFact` / `BE:UnsupportedNode` / `FE:L0080` / `FE:L0111` | `run_ok`（新晋能力通过） | {len(base_evo['promoted_to_run_ok'])} | " + ", ".join(f"`{p}`" for p in base_evo["promoted_to_run_ok"]) + " |",
            f"| `upgraded_to_structured_abort` | `abort(exit 1,no msg)` | `runtime_abort`（`SIGABRT` + `abort: <msg>`） | {len(base_evo['upgraded_to_structured_abort'])} | " + ", ".join(f"`{p}`" for p in base_evo["upgraded_to_structured_abort"]) + " |",
            f"| `upgraded_to_compile_fail` | `FE:L0165` | `compile_fail`（`FE:L0167` 规范拒绝） | {len(base_evo['upgraded_to_compile_fail'])} | " + ", ".join(f"`{p}`" for p in base_evo["upgraded_to_compile_fail"]) + " |",
            f"| `baseline_compile_fail_preserved` | `FE:L0080`..`FE:L0164` | `compile_fail`（精确诊断序列匹配） | {len(base_evo['baseline_compile_fail_preserved'])} | " + ", ".join(f"`{p}`" for p in base_evo["baseline_compile_fail_preserved"]) + " |",
            f"| `remaining_known_unimplemented` | `BE:UnsupportedNode` / `FE:L0165` / `FE:L0080` / `ERR` | `known_unimplemented`（100% 绑定 `target_stdout` 与 `roadmap_blockers`） | {len(base_evo['remaining_known_unimplemented'])} | " + ", ".join(f"`{p}`" for p in base_evo["remaining_known_unimplemented"]) + " |",
            f"| `regressed_from_baseline_ok` | `OK` | 非 `run_ok`（基线退化） | {len(base_evo['regressed_from_baseline_ok'])} | 无（零基线退化） |",
            "",
            "### 1.3 Guide v0.46 豁免项 (`v046_spec_contract`) 与 Spec 交付合同溯源矩阵（35 Probes）",
            "",
            "| Spec 合同编号 | 规范文档路径 | 合同状态 | 关联豁免 Probe 数 | 交付后分布 | 绑定的 Probe 列表 |",
            "|---|---|---|---:|---|---|",
        ]
    )
    for sref, info in spec_trace.items():
        doc_link = (
            f"[{info['doc_relpath'].split('/')[-1]}](../{info['doc_relpath']})"
            if info["doc_relpath"]
            else "-"
        )
        kinds_summary = ", ".join(
            f"`{k}`: {cnt}" for k, cnt in sorted(info["kinds"].items())
        )
        plist = ", ".join(f"`{p}`" for p in info["probes"])
        lines.append(
            f"| `{sref}` | {doc_link} | `{info['status']}` | {len(info['probes'])} | {kinds_summary} | {plist} |"
        )

    lines.extend(
        [
            "",
            "### 1.4 强 Oracle 溯源分类矩阵（`oracle_origin` Taxonomy，154 Probes + 6 Programs）",
            "",
            "| 溯源分类 (`oracle_origin`) | 审计验证契约说明 | Probe 数 | Program 数 | 期望类型分布 (`kind`) |",
            "|---|---|---:|---:|---|",
        ]
    )
    for origin, info in origin_stats.items():
        kinds_repr = ", ".join(
            f"`{k}`: {cnt}" for k, cnt in sorted(info["kinds"].items())
        )
        lines.append(
            f"| `{origin}` | {info['description']} | {info['probes']} | {info['programs']} | {kinds_repr} |"
        )

    ml_summary = ", ".join(
        f"`{m['name']}` ({m['lines']}L/{m['bytes']}B)" for m in shape_stats["multi_line_probes"]
    )
    lines.extend(
        [
            "",
            "### 1.5 标准输出规模与多行形态分布矩阵（`expected_lines` / `expected_bytes`）",
            "",
            "| 输出形态分类 | 适用范围 | 条目数量 | 累计期望行数 | 累计期望字节数 | 代表性 / 完整条目明细 |",
            "|---|---|---:|---:|---:|---|",
            f"| `single_line_stdout` | 1 行输出 Probe (`run_ok` + `known_unimplemented`) | {len(shape_stats['single_line_probes'])} | {len(shape_stats['single_line_probes'])} | {shape_stats['total_probe_bytes'] - sum(m['bytes'] for m in shape_stats['multi_line_probes'])} B | 单行精确字节匹配（全部以 `\\n` 结尾） |",
            f"| `multi_line_stdout` | ≥2 行输出 Probe (`run_ok` + `known_unimplemented`) | {len(shape_stats['multi_line_probes'])} | {sum(m['lines'] for m in shape_stats['multi_line_probes'])} | {sum(m['bytes'] for m in shape_stats['multi_line_probes'])} B | {ml_summary} |",
            f"| `zero_stdout_negative` | 零标准输出负向 Probe (`compile_fail` + `runtime_abort`) | {len(shape_stats['zero_stdout_probes'])} | 0 | 0 B | " + ", ".join(f"`{p}`" for p in shape_stats["zero_stdout_probes"]) + " |",
            f"| `acceptance_programs` | V1 验收程序 (`expected.txt`) | {len(programs)} | {shape_stats['total_program_lines']} | {shape_stats['total_program_bytes']} B | " + ", ".join(f"`{k}` ({v['expected_lines']}L/{v['expected_bytes']}B)" for k, v in sorted(programs.items())) + " |",
            "",
            "### 1.6 Probe 批次前缀与演进关注面分布矩阵（11 Batches，154 Probes）",
            "",
            "| 批次前缀 (`probe_batch`) | 关注面与批次说明 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) | 批次状态 (`batch_status`) |",
            "|---|---|---:|---:|---:|---:|---:|---:|---|",
        ]
    )
    all_passed_batches = sum(
        1 for b in batch_stats.values() if b["batch_status"] == "all_passed"
    )
    partial_batches = sum(
        1 for b in batch_stats.values() if b["batch_status"] == "partial"
    )
    for b_key, b_row in batch_stats.items():
        lines.append(
            f"| `{b_key}` | {b_row['description']} | {b_row['total']} | {b_row['run_ok']} | "
            f"{b_row['compile_fail']} | {b_row['runtime_abort']} | {b_row['known_unimplemented']} | "
            f"{b_row['cap_passed']} / {b_row['total']} | `{b_row['batch_status']}` |"
        )
    lines.append(
        f"| **合计** | **11 个演进批次** | **{tot_all}** | **{tot_ok}** | **{tot_cf}** | **{tot_ab}** | **{tot_ku}** | **{tot_cap} / {tot_all}** | **{all_passed_batches} 全通过 / {partial_batches} 部分通过** |"
    )
    lines.extend(
        [
            "",
            "### 1.7 未实现允许记号（`allowed_markers`）与编译阶段分布矩阵（7 Markers）",
            "",
            "| 允许记号 (`allowed_marker`) | 归属编译阶段 | 记号契约说明 | 基线命中数 (`baseline_marker`) | 授权 Probe 数 | 授权 Program 数 |",
            "|---|---|---|---:|---:|---:|",
        ]
    )
    for m_key, m_row in marker_stats.items():
        lines.append(
            f"| `{m_key}` | {m_row['stage']} | {m_row['description']} | {m_row['baseline_count']} | "
            f"{len(m_row['probes'])} | {len(m_row['programs'])} |"
        )

    grp_a = foundational_stats["groups"]["A"]
    grp_b = foundational_stats["groups"]["B"]
    grp_c = foundational_stats["groups"]["C"]
    lines.extend(
        [
            "",
            f"### 1.8 初始 {foundational_stats['total_foundational']} 探针（`p`/`q`/`r`/`s` 批次 A/B/C 三组）与 P2 阶段门槛对账矩阵",
            "",
            "| 分组 | 分组定义与审计口径 | 总数 | 可运行 (`run_ok` + `runtime_abort`) | `compile_fail` | `known_unimplemented` | 阶段门槛 / 当前状态说明 |",
            "|---|---|---:|---:|---:|---:|---|",
            f"| **A 组** | {grp_a['label']} | {grp_a['total']} | {grp_a['runnable']} ({len(grp_a['run_ok'])} + {len(grp_a['runtime_abort'])}) | {len(grp_a['compile_fail'])} (`q17_generic_class`) | {len(grp_a['known_unimplemented'])} | P2 门槛 `≥ {foundational_stats['p2_target_min']} / {grp_a['total']}`（当前 `{grp_a['runnable']} / {grp_a['total']}`，距门槛差 `{foundational_stats['p2_gap_to_target']}` 项） |",
            f"| **B 组** | {grp_b['label']} | {grp_b['total']} | {grp_b['runnable']} | {len(grp_b['compile_fail'])} | {len(grp_b['known_unimplemented'])} | `8 / 8 (100%)` 精确命中期望前端诊断码 (`pass_diagnostic`) |",
            f"| **C 组** | {grp_c['label']} | {grp_c['total']} | {grp_c['runnable']} (`p37_is_generic_case`) | {len(grp_c['compile_fail'])} (`p23_global_var`) | {len(grp_c['known_unimplemented'])} (`p35_value_eq`) | `p23` 规范拒绝 (`FE:L0167`)、`p37` 能力通过 (`SPEC-0349`)、`p35` 待 `L1` |",
            "",
            "### 1.9 Guide 13 章 V2 终态完备性（正反例双极性 + 零未实现）差距矩阵",
            "",
            "| Guide 章节 | 规范领域 | 正向已通 (`run_ok`) | 负向契约 (`compile_fail` / `abort`) | 剩余 `known_unimplemented` | 路线图阻塞项 | 关联 V1 验收程序 | V2 章节就绪状态 (`v2_status`) |",
            "|---|---|---:|---|---|---|---|---|",
        ]
    )
    for ch in VALID_GUIDE_CHAPTERS:
        r_row = v2_readiness[ch]
        neg_repr = (
            f"{r_row['negative_active']} (" + ", ".join(f"`{c}`" for c in r_row["negative_contracts"]) + ")"
            if r_row["negative_contracts"]
            else "0（待补反例）"
        )
        ku_repr = (
            f"{r_row['known_unimplemented']} (" + ", ".join(f"`{p}`" for p in r_row["unimplemented_probes"]) + ")"
            if r_row["unimplemented_probes"]
            else "0"
        )
        blk_repr = (
            ", ".join(f"`{b}`" for b in r_row["roadmap_blockers"])
            if r_row["roadmap_blockers"]
            else "-"
        )
        prog_repr = (
            ", ".join(f"`{p}`" for p in r_row["acceptance_programs"])
            if r_row["acceptance_programs"]
            else "-"
        )
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {r_row['domain']} | {r_row['positive_active']} | "
            f"{neg_repr} | {ku_repr} | {blk_repr} | {prog_repr} | `{r_row['v2_status']}` |"
        )

    litmus_align = summarize_guide_litmus_alignment(expectations)
    lines.extend(
        [
            "",
            "### 1.10 Guide 15 规范性 Litmus 1–12 与端到端 Conformance Probe 对齐矩阵",
            "",
            "| Litmus 编号与主题 | 归属 Guide 章节 | 关联端到端 Probe | 能力通过 (`capability_passed`) | 剩余未实现与阻塞项 | 状态 |",
            "|---|---|---|---:|---|---|",
        ]
    )
    for lit_id, l_row in litmus_align.items():
        ch = l_row["guide_chapter"]
        p_repr = ", ".join(f"`{p}`" for p in l_row["probes"])
        rem_repr = (
            ", ".join(f"`{p}`" for p in l_row["unimplemented_probes"])
            + " (" + ", ".join(f"`{b}`" for b in l_row["roadmap_blockers"]) + ")"
            if l_row["unimplemented_probes"]
            else "-"
        )
        lines.append(
            f"| **Litmus {lit_id}**：{l_row['title']} | [{ch}](../docs/guide/{ch}) | {p_repr} | "
            f"{l_row['cap_passed_count']} / {len(l_row['probes'])} | {rem_repr} | `{l_row['status']}` |"
        )

    ver_bounds = summarize_guide_version_boundaries(expectations)
    lines.extend(
        [
            "",
            "### 1.11 Guide 15 版本启用边界（`v0.44` → `v0.45` → `v0.46` → `v0.47`）与 Probe 验收对账矩阵",
            "",
            "| Guide 启用边界 | 核心启用语义与阶段合同 | 关联验收 Probe 数 | 能力通过 (`capability_passed`) | 剩余未实现与阻塞项 | 状态 |",
            "|---|---|---:|---:|---|---|",
        ]
    )
    for ver_key, v_row in ver_bounds.items():
        rem_v = (
            ", ".join(f"`{p}`" for p in v_row["unimplemented_probes"])
            + " (" + ", ".join(f"`{b}`" for b in v_row["roadmap_blockers"]) + ")"
            if v_row["unimplemented_probes"]
            else "-"
        )
        lines.append(
            f"| **{ver_key} 启用边界** | {v_row['summary']} | {len(v_row['probes'])} | "
            f"{v_row['cap_passed_count']} / {len(v_row['probes'])} | {rem_v} | `{v_row['status']}` |"
        )

    src_scale = summarize_corpus_source_scale(expectations)
    lines.extend(
        [
            "",
            "### 1.12 语料库源码规模、LF 尾换行卫生契约与章节源码分布汇总",
            "",
            "| Guide v0.46 章节 | 领域主题 | Probe 数量 | Probe 源码规模 (`lines / bytes`) | 关联验收程序 | Program 源码规模 (`lines / bytes`) | 章节源码合计 |",
            "|---|---|---:|---:|---|---:|---:|",
        ]
    )
    for ch in VALID_GUIDE_CHAPTERS:
        sc_row = src_scale["by_chapter"][ch]
        prog_s = ", ".join(f"`{p}`" for p in sc_row["programs"]) if sc_row["programs"] else "-"
        prog_scale_s = (
            f"{sc_row['program_source_lines']} 行 / {sc_row['program_source_bytes']} B"
            if sc_row["program_count"] > 0
            else "0 行 / 0 B"
        )
        ch_tot_l = sc_row["probe_source_lines"] + sc_row["program_source_lines"]
        ch_tot_b = sc_row["probe_source_bytes"] + sc_row["program_source_bytes"]
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {sc_row['domain']} | {sc_row['probe_count']} | "
            f"{sc_row['probe_source_lines']} 行 / {sc_row['probe_source_bytes']} B | {prog_s} | "
            f"{prog_scale_s} | {ch_tot_l} 行 / {ch_tot_b} B |"
        )
    lines.append(
        f"| **合计** | **13 个核心章节** | **{src_scale['total_probe_count']}** | "
        f"**{src_scale['total_probe_source_lines']} 行 / {src_scale['total_probe_source_bytes']} B** | "
        f"**{src_scale['total_program_count']} Programs** | "
        f"**{src_scale['total_program_source_lines']} 行 / {src_scale['total_program_source_bytes']} B** | "
        f"**{src_scale['total_corpus_source_lines']} 行 / {src_scale['total_corpus_source_bytes']} B** |"
    )

    v3_cov = summarize_v3_sanitizer_coverage(expectations)
    lines.extend(
        [
            "",
            "### 1.13 V3 原生内存安全（ASan / LSan）资源生命周期与堆所有者 Probe 覆盖矩阵",
            "",
            "| V3 资源安全分类 | 覆盖语义与生命周期特征 | `check_native_sanitizers.py` 关注面 | 关联 Probe 就绪率 | 待解封 Probe 与阻塞项 | 关联 V1 验收程序 | 状态 |",
            "|---|---|---|---:|---|---|---|",
        ]
    )
    for cat_key, v3_row in v3_cov.items():
        rem_v3 = (
            ", ".join(f"`{p}`" for p in v3_row["unimplemented_probes"])
            + " (" + ", ".join(f"`{b}`" for b in v3_row["roadmap_blockers"]) + ")"
            if v3_row["unimplemented_probes"]
            else "-"
        )
        prog_v3 = ", ".join(f"`{pr}`" for pr in v3_row["programs"]) if v3_row["programs"] else "-"
        lines.append(
            f"| `{cat_key}` | {v3_row['title']} | {v3_row['sanitizer_focus']} | "
            f"{len(v3_row['runnable_probes'])} / {len(v3_row['probes'])} | {rem_v3} | {prog_v3} | `{v3_row['status']}` |"
        )

    syntax_align = summarize_guide_syntax_index_alignment(expectations)
    tot_syntax_entries = sum(r["entry_count"] for r in syntax_align.values())
    lines.extend(
        [
            "",
            f"### 1.14 Guide 14 语法索引（{tot_syntax_entries} 项语法形式）与 Conformance 章节探针对账矩阵",
            "",
            "| 语法索引领域 (`14-syntax-index.md`) | 语法条目数 | 唯一规则归属章节 | 覆盖语法形式说明 | 归属章节 Probe 通过率 | 状态 |",
            "|---|---:|---|---|---:|---|",
        ]
    )
    for dom, sx_row in syntax_align.items():
        ch_links = ", ".join(f"[{c}](../docs/guide/{c})" for c in sx_row["chapters"])
        lines.append(
            f"| **{dom}** | {sx_row['entry_count']} | {ch_links} | {sx_row['summary']} | "
            f"{sx_row['cap_passed_count']} / {sx_row['probe_count']} | `{sx_row['status']}` |"
        )

    # Load v1.json roadmap metadata for blocker names and titles if available
    v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
    roadmap_items: dict[str, dict[str, Any]] = {}
    if v1_json_path.is_file():
        v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
        roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}

    lines.extend(
        [
            "",
            "## 2. 六个 V1 验收程序（Acceptance Programs）",
            "",
            "| 程序 | 归属章节 | 当前状态 | 路线图阻塞项 | 期望输出文件 | 期望输出规模 | 允许的未实现记号 |",
            "|---|---|---|---|---|---|---|",
        ]
    )
    for name, spec in sorted(programs.items()):
        ch = spec.get("guide_chapter", "13-program-runtime-standard-library.md")
        exp_file = spec["expected_file"]
        exp_size = (
            f"{spec['expected_lines']} 行 / {spec['expected_bytes']} B"
            if "expected_lines" in spec and "expected_bytes" in spec
            else "-"
        )
        allowed = ", ".join(f"`{m}`" for m in spec.get("allowed_markers", []))
        blockers = ", ".join(f"`{b}`" for b in spec.get("roadmap_blockers", []))
        lines.append(
            f"| `{name}` | [{ch}](../docs/guide/{ch}) | `{spec['kind']}` | {blockers} | [{exp_file}]({exp_file}) | {exp_size} | {allowed} |"
        )

    prog_blockers = summarize_program_roadmap_blockers(expectations, roadmap_items)
    lines.extend(
        [
            "",
            f"### 2.1 六个 V1 验收程序按路线图阻塞项（`v1.json`）归类对账（{len(prog_blockers)} Roadmap Blockers）",
            "",
            "| 路线图条目 ID | 路线图条目标题 | 状态 | 阻塞 Program 数 | 阻塞的验收程序 (`programs`) | 同时阻塞的 Probe 数 |",
            "|---|---|---|---:|---|---:|",
        ]
    )
    for b_id, b_info in prog_blockers.items():
        prog_list = ", ".join(f"`{p}`" for p in b_info["programs"])
        lines.append(
            f"| `{b_id}` | {b_info['title']} | `{b_info['status']}` | {len(b_info['programs'])} | {prog_list} | {len(b_info['probes'])} |"
        )

    prog_prereqs = summarize_program_probe_prerequisites(expectations)
    lines.extend(
        [
            "",
            "### 2.2 六个 V1 验收程序与前置核心能力 Probe 对账矩阵",
            "",
            "| 验收程序 | 归属章节 | 前置 Probe 通过率 | 已通过前置 Probe (`run_ok` / `compile_fail` / `abort`) | 待解封前置 Probe (`known_unimplemented`) | 路线图阻塞项 |",
            "|---|---|---:|---|---|---|",
        ]
    )
    for prog_name, pr_row in prog_prereqs.items():
        ch = pr_row["guide_chapter"]
        passed_s = ", ".join(f"`{p}`" for p in pr_row["passed_probes"])
        unimpl_s = ", ".join(f"`{p}`" for p in pr_row["unimplemented_probes"])
        blk_s = ", ".join(f"`{b}`" for b in pr_row["roadmap_blockers"])
        lines.append(
            f"| `{prog_name}` | [{ch}](../docs/guide/{ch}) | {len(pr_row['passed_probes'])} / {len(pr_row['prerequisite_probes'])} | "
            f"{passed_s} | {unimpl_s} | {blk_s} |"
        )

    # 3. Aggregate every known_unimplemented probe by roadmap blocker
    blocker_probes: dict[str, list[str]] = {}
    for name, spec in sorted(probes.items()):
        if spec["kind"] == "known_unimplemented":
            for b in spec.get("roadmap_blockers", []):
                blocker_probes.setdefault(b, []).append(name)
    unimplemented_total = sum(
        1 for spec in probes.values() if spec["kind"] == "known_unimplemented"
    )

    lines.extend(
        [
            "",
            f"## 3. {unimplemented_total} 个 `known_unimplemented` Probe 按路线图阻塞项（`v1.json`）归类对账",
            "",
            "| 路线图条目 ID | 路线图条目标题 | 状态 | 关联 Probe 总数 | 阻塞的 Probe 列表 |",
            "|---|---|---|---:|---|",
        ]
    )
    for b_id in sorted(blocker_probes.keys()):
        item = roadmap_items.get(b_id, {})
        title = item.get("title", "").replace("|", "\\|")
        status = item.get("status", "unknown")
        plist = blocker_probes[b_id]
        pnames = ", ".join(f"`{p}`" for p in plist)
        lines.append(
            f"| `{b_id}` | {title} | `{status}` | {len(plist)} | {pnames} |"
        )

    unlock_impact = summarize_blocker_unlock_impact(expectations, roadmap_items)
    lines.extend(
        [
            "",
            f"### 3.1 路线图阻塞项解封收益分析（单阻塞独立解封 vs 联合阻塞与 P2 门槛贡献，{len(unlock_impact)} Blockers）",
            "",
            "| 路线图条目 | 阶段 | 状态 | 独立解封 Probe (`sole_probes`) | 联合阻塞 Probe (`co_probes`) | P2 A 组贡献数 | 关联 V1 验收程序 |",
            "|---|---|---|---|---|---:|---|",
        ]
    )
    for b_id, u_row in unlock_impact.items():
        sole_repr = (
            f"{len(u_row['sole_probes'])} (" + ", ".join(f"`{p}`" for p in u_row["sole_probes"]) + ")"
            if u_row["sole_probes"]
            else "0"
        )
        co_repr = (
            f"{len(u_row['co_probes'])} (" + ", ".join(f"`{p}`" for p in u_row["co_probes"]) + ")"
            if u_row["co_probes"]
            else "0"
        )
        prog_repr = (
            ", ".join(f"`{p}`" for p in u_row["programs"])
            if u_row["programs"]
            else "-"
        )
        lines.append(
            f"| `{b_id}` | `{u_row['stage']}` | `{u_row['status']}` | {sole_repr} | {co_repr} | "
            f"{len(u_row['p2_group_a_probes'])} | {prog_repr} |"
        )

    stage_prog = summarize_stage_progression(expectations, roadmap_items)
    lines.extend(
        [
            "",
            "### 3.2 路线图阶段（`P2` → `P3` → `P4`）递进解封与里程碑投射矩阵",
            "",
            "| 阶段里程碑 | 阶段核心目标 | 本阶段清零阻塞项 | 本阶段新增解封 Probe | 累计能力通过 (`capability_passed`) | P2 A 组可运行 (`≥ 70 / 74`) | V1 验收程序就绪 |",
            "|---|---|---|---|---:|---:|---:|",
        ]
    )
    for s_row in stage_prog:
        s_blks = ", ".join(f"`{b}`" for b in s_row["stage_blockers"]) if s_row["stage_blockers"] else "-"
        s_probes = (
            f"+{len(s_row['newly_unlocked_probes'])} (" + ", ".join(f"`{p}`" for p in s_row["newly_unlocked_probes"]) + ")"
            if s_row["newly_unlocked_probes"]
            else "0"
        )
        lines.append(
            f"| **{s_row['stage']} 完成** | {s_row['description']} | {s_blks} | {s_probes} | "
            f"**{s_row['projected_probe_cap']} / {s_row['total_probes']}** | "
            f"**{s_row['projected_group_a_runnable']} / {s_row['group_a_total']}** | "
            f"**{s_row['projected_programs_ready']} / {s_row['total_programs']}** |"
        )

    area_impact = summarize_roadmap_area_impact(expectations, roadmap_items)
    lines.extend(
        [
            "",
            f"### 3.3 路线图领域（`area`）与编译器阶段（`phases`）阻塞分布矩阵（{len(area_impact)} Areas）",
            "",
            "| 路线图领域 (`area`) | 领域职责说明 | 涉及编译阶段 (`phases`) | 关联阻塞项 | 阻塞 Probe 数 | 阻塞 Program 数 |",
            "|---|---|---|---|---:|---:|",
        ]
    )
    for a_key, a_row in area_impact.items():
        ph_repr = ", ".join(f"`Phase {p}`" for p in a_row["phases"])
        blks_repr = ", ".join(f"`{b}`" for b in a_row["blockers"])
        lines.append(
            f"| `{a_key}` | {a_row['description']} | {ph_repr} | {blks_repr} | "
            f"{len(a_row['probes'])} | {len(a_row['programs'])} |"
        )

    promo_contracts = summarize_unimplemented_promotion_contracts(expectations)
    lines.extend(
        [
            "",
            f"### 3.4 {len(promo_contracts)} 个 `known_unimplemented` Probe 晋升契约与目标输出（`target_stdout`）对账表",
            "",
            "| Probe | Guide 章节 | P2 分组 | 基线记号 | 允许记号 (`allowed_markers`) | 路线图阻塞项 | 目标输出 (`target_stdout`) | 规模 |",
            "|---|---|:---:|---|---|---|---|---:|",
        ]
    )
    for p_row in promo_contracts:
        ch = p_row["guide_chapter"]
        allowed_s = ", ".join(f"`{m}`" for m in p_row["allowed_markers"])
        blks_s = ", ".join(f"`{b}`" for b in p_row["roadmap_blockers"])
        escaped_out = p_row["target_stdout"].encode("unicode_escape").decode("ascii")
        lines.append(
            f"| `{p_row['name']}` | [{ch}](../docs/guide/{ch}) | `{p_row['foundational_group']}` | "
            f"`{p_row['baseline_marker']}` | {allowed_s} | {blks_s} | `{escaped_out}` | "
            f"{p_row['expected_lines']}L / {p_row['expected_bytes']}B |"
        )

    phase_matrix = summarize_compiler_phase_matrix(expectations, roadmap_items)
    lines.extend(
        [
            "",
            "### 3.5 编译器阶段（`Phase 2` – `Phase 5`）负向契约与未实现阻塞全景矩阵",
            "",
            "| 编译器阶段 | 阶段职责范围 | 已生效负向 Probe 数 | 覆盖负向契约 (`FE:Lxxxx` / `abort`) | 关联路线图阻塞项 | 阻塞 Probe 数 | 阻塞 Program 数 |",
            "|---|---|---:|---|---|---:|---:|",
        ]
    )
    for ph_id, ph_row in phase_matrix.items():
        neg_c = (
            ", ".join(f"`{c}`" for c in ph_row["active_negative_contracts"])
            if ph_row["active_negative_contracts"]
            else "-"
        )
        blks_c = ", ".join(f"`{b}`" for b in ph_row["blockers"]) if ph_row["blockers"] else "-"
        lines.append(
            f"| **{ph_row['title']}** | {ph_row['description']} | {len(ph_row['active_negative_probes'])} | "
            f"{neg_c} | {blks_c} | {len(ph_row['blocked_probes'])} | {len(ph_row['blocked_programs'])} |"
        )

    lines.extend(
        [
            "",
            "## 4. 154 个 Probe 逐项强 Oracle 与语义推导账本",
            "",
            "| Probe | Guide 章节 | 类型 (`kind`) | 溯源 (`oracle_origin`) | 期望结果 / 诊断码 | 语义推导 / 规则说明 (`oracle_derivation` / `note`) |",
            "|---|---|---|---|---|---|",
        ]
    )
    for name, spec in sorted(probes.items()):
        ch = spec.get("guide_chapter", "")
        kind = spec["kind"]
        origin = spec.get("oracle_origin", "")
        if kind == "run_ok":
            oracle_repr = f"`{spec['stdout'].encode('unicode_escape').decode('ascii')}`"
        elif kind == "compile_fail":
            diags = spec.get("all_diagnostics", [spec["diagnostic"]])
            oracle_repr = ", ".join(f"`FE:{d}`" for d in diags)
        elif kind == "runtime_abort":
            oracle_repr = f"`abort:{spec['abort_message']}`"
        else:
            markers_repr = ", ".join(f"`{m}`" for m in spec.get("allowed_markers", []))
            target_out = spec.get("target_stdout")
            if target_out is not None:
                escaped_target = target_out.encode("unicode_escape").decode("ascii")
                oracle_repr = f"{markers_repr} → target `{escaped_target}`"
            else:
                oracle_repr = markers_repr
        deriv = (spec.get("oracle_derivation") or spec.get("note") or "-").replace("|", "\\|")
        lines.append(
            f"| `{name}` | [{ch}](../docs/guide/{ch}) | `{kind}` | `{origin}` | {oracle_repr} | {deriv} |"
        )
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cli",
        type=Path,
        default=resolve_default_cli(root),
        help="path to kovenc binary (auto-detects KOVENC, target/release/kovenc, or target/debug/kovenc)",
    )
    parser.add_argument(
        "--corpus",
        type=Path,
        default=root / "conformance",
        help="path to conformance directory",
    )
    parser.add_argument(
        "--expectations",
        type=Path,
        default=root / "conformance/expectations.json",
        help="path to machine-readable expectations JSON",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help="optional historical baseline TSV for regression comparison",
    )
    parser.add_argument(
        "--evidence",
        type=Path,
        default=None,
        help="directory to write raw stdout/stderr bytes, results.json, and matrix.tsv",
    )
    parser.add_argument(
        "--probe",
        action="append",
        dest="probes",
        help="run only selected probe name(s) (repeatable)",
    )
    parser.add_argument(
        "--batch",
        action="append",
        dest="batches",
        help="run only probes in selected batch prefix(es) p/q/r/s/t/u/v/w/x/y/e (repeatable)",
    )
    parser.add_argument(
        "--chapter",
        action="append",
        dest="chapters",
        help="run only probes in selected Guide chapter filename or 01..13 prefix (repeatable)",
    )
    parser.add_argument(
        "--program",
        action="append",
        dest="programs",
        help="run only selected acceptance program(s) (repeatable)",
    )
    parser.add_argument(
        "--no-programs",
        action="store_true",
        help="skip the 6 acceptance programs",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.environ.get("PROBE_TIMEOUT", "60")),
        help="per-invocation timeout in seconds",
    )
    parser.add_argument(
        "--jobs",
        "-j",
        type=int,
        default=int(
            os.environ.get("KOVEN_CONFORMANCE_JOBS", str(min(os.cpu_count() or 4, 4)))
        ),
        help="number of parallel worker threads (deterministic output order)",
    )
    parser.add_argument(
        "--allow-unreviewed",
        action="store_true",
        help="do not fail the gate on unreviewed items (observation mode)",
    )
    parser.add_argument(
        "--compiler-commit",
        default=detect_git_commit(root),
        help="compiler git commit SHA to bind in results.json",
    )
    parser.add_argument(
        "--init-expectations",
        action="store_true",
        help="derive static expectations.json from corpus sources + baseline TSV without running compiler",
    )
    parser.add_argument(
        "--verify-evidence",
        type=Path,
        default=None,
        help="verify an existing evidence bundle directory against expectations and optional baseline without running compiler",
    )
    parser.add_argument(
        "--compare-evidence",
        nargs=2,
        type=Path,
        metavar=("EVIDENCE_A", "EVIDENCE_B"),
        default=None,
        help="verify two evidence bundle directories and assert cross-host parity across all probes and programs",
    )
    parser.add_argument(
        "--check-matrix",
        type=Path,
        default=None,
        help="compare a 3-column TSV matrix against expectations.json and optional baseline without running compiler",
    )
    args = parser.parse_args(argv)

    if args.init_expectations:
        baseline_file = args.baseline or (args.corpus / "baseline-aef3f4f8.tsv")
        try:
            derived = derive_static_expectations(args.corpus, baseline_file)
            args.expectations.write_text(
                json.dumps(derived, ensure_ascii=True, indent=2) + "\n",
                encoding="utf-8",
            )
            if (args.corpus / "baseline-aef3f4f8.tsv").is_file():
                (args.corpus / "ORACLE_AUDIT.md").write_text(
                    render_oracle_audit_markdown(derived),
                    encoding="utf-8",
                )
                readme_path = args.corpus / "README.md"
                if readme_path.is_file():
                    readme_path.write_text(
                        sync_readme_chapter_table(
                            readme_path.read_text(encoding="utf-8"), derived
                        ),
                        encoding="utf-8",
                    )
        except (ValueError, OSError) as exc:
            # CLI 边界：把推导或写入错误（命名违规、baseline 缺项、README 表缺失等）渲染为单行诊断，不暴露 traceback
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        print(
            f"conformance: initialized {len(derived['probes'])} probes and "
            f"{len(derived['programs'])} programs in {args.expectations}"
        )
        return 0

    if args.verify_evidence is not None:
        try:
            verified = verify_evidence_bundle(
                evidence_dir=args.verify_evidence,
                expectations_path=args.expectations,
                corpus_root=args.corpus,
                baseline_path=args.baseline,
                strict_unreviewed=not args.allow_unreviewed,
            )
        except (ValueError, OSError) as exc:
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        v_sum = verified["summary"]
        print(
            f"conformance: verified evidence bundle {args.verify_evidence} "
            f"probes_cap_passed={v_sum['probe_capability_passed']}/{v_sum['total_probes']} "
            f"programs_passed={v_sum['program_run_passed']}/{v_sum['total_programs']}"
        )
        return 0 if v_sum["gate_passed"] else 1

    if args.compare_evidence is not None:
        dir_a, dir_b = args.compare_evidence
        try:
            parity = compare_dual_host_evidence(
                evidence_dir_a=dir_a,
                evidence_dir_b=dir_b,
                expectations_path=args.expectations,
                corpus_root=args.corpus,
                baseline_path=args.baseline,
                strict_unreviewed=not args.allow_unreviewed,
            )
        except (ValueError, OSError) as exc:
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        print(
            f"conformance: dual-host parity verified commit={parity['commit']} "
            f"hosts={parity['host_a']['system']}/{parity['host_b']['system']} "
            f"probes_cap_passed={parity['probe_capability_passed']}/{parity['total_probes']} "
            f"programs_passed={parity['program_run_passed']}/{parity['total_programs']}"
        )
        return 0 if parity["gate_passed"] else 1

    if args.check_matrix is not None:
        try:
            m_res = compare_tsv_matrix_against_expectations(
                matrix_path=args.check_matrix,
                expectations_path=args.expectations,
                corpus_root=args.corpus,
                baseline_path=args.baseline,
            )
        except (ValueError, OSError) as exc:
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        print(
            f"conformance: checked matrix {args.check_matrix} "
            f"probes_cap_passed={m_res['probe_capability_passed']}/{m_res['total_probes']} "
            f"counts={json.dumps(m_res['counts'], sort_keys=True)}"
        )
        if not m_res["gate_passed"]:
            for failure in m_res["failures"]:
                print(f"FAIL: {failure}", file=sys.stderr)
            return 1
        return 0

    try:
        ledger = evaluate_suite(
            kovenc=args.cli,
            corpus_root=args.corpus,
            expectations_path=args.expectations,
            baseline_path=args.baseline,
            selected_probes=args.probes,
            selected_batches=args.batches,
            selected_chapters=args.chapters,
            selected_programs=args.programs,
            include_programs=not args.no_programs,
            timeout_sec=args.timeout,
            strict_unreviewed=not args.allow_unreviewed,
            compiler_commit=args.compiler_commit,
            evidence_dir=args.evidence,
            jobs=args.jobs,
        )
    except (ValueError, OSError) as exc:
        print(f"conformance: error: {exc}", file=sys.stderr)
        return 1
    summary = ledger["summary"]
    print(
        f"conformance: probes_cap_passed={summary['probe_capability_passed']}/{summary['total_probes']} "
        f"programs_passed={summary['program_run_passed']}/{summary['total_programs']} "
        f"counts={json.dumps(summary['counts'], sort_keys=True)}"
    )
    if not summary["gate_passed"]:
        for up_name in summary.get("unexpected_passes", []):
            print(
                f"PROMOTE: {up_name} exited 0 with exact target_stdout in both modes; "
                f"update V046_PROBE_OVERRIDES and run --init-expectations",
                file=sys.stderr,
            )
        for failure in summary["failures"]:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
