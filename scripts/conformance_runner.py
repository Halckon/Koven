#!/usr/bin/env python3
"""Public-CLI dual-mode execution, byte-exact oracle comparator, and runtime suite evaluation."""

from __future__ import annotations

import base64
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import tempfile
from typing import Any

from conformance_catalogs import (
    ABORT_FRAME_RE,
    BACKEND_OBJECT_RE,
    BACKEND_RE,
    DIAGNOSTIC_RE,
    FOUNDATIONAL_BATCHES,
    FOUNDATIONAL_GROUP_B_PROBES,
    FOUNDATIONAL_GROUP_C_PROBES,
    OWNERSHIP_ICE_RE,
    P2_GROUP_A_TARGET_MIN,
    PROBE_BATCH_CATALOG,
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
    sha256_file,
)


def detect_host() -> dict[str, str]:
    uname = platform.uname()
    return {
        "system": uname.system,
        "machine": uname.machine,
        "platform": f"{uname.system}-{uname.machine}",
    }


def detect_git_commit(root: Path) -> str:
    env_commit = os.environ.get("KOVEN_COMPILER_COMMIT", "").strip()
    if env_commit:
        return env_commit
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short=8", "HEAD"],
            cwd=root,
            stderr=subprocess.DEVNULL,
            timeout=5.0,
        )
        commit = out.decode("ascii", errors="ignore").strip()
        if commit:
            return commit
    except Exception:
        pass
    return "unknown"


def extract_diagnostic_codes(stderr: bytes) -> list[str]:
    """Return distinct error[Lxxxx] codes emitted on stderr in order of appearance."""
    return list(dict.fromkeys(m.decode("ascii") for m in DIAGNOSTIC_RE.findall(stderr)))


def classify_raw(
    returncode: int | None,
    timed_out: bool,
    stdout: bytes,
    stderr: bytes,
) -> str:
    """Classify a single invocation using separated raw stdout and stderr bytes."""
    if timed_out or returncode == 124:
        return "timeout"
    if returncode is None:
        return "ERR"
    if returncode == 0:
        return "OK" if stdout else "OK(silent)"

    is_sigabrt = returncode in (134, -int(signal.SIGABRT))
    if is_sigabrt and stderr:
        first_line = stderr.split(b"\n", 1)[0].rstrip(b"\r")
        if first_line.startswith(b"abort: "):
            msg = first_line[len(b"abort: ") :].decode("utf-8", errors="replace")
            msg = msg.replace("\t", " ")
            return f"abort:{msg}"

    combined = stdout + stderr
    ice_match = OWNERSHIP_ICE_RE.search(combined)
    if ice_match:
        return "ICE:ownership"
    be_match = BACKEND_RE.search(combined)
    if be_match:
        return f"BE:{be_match.group(1).decode('ascii')}"
    object_match = BACKEND_OBJECT_RE.search(combined)
    if object_match:
        return f"BE:{object_match.group(1).decode('ascii')}"
    diag_match = DIAGNOSTIC_RE.search(stderr)
    if diag_match:
        return f"FE:{diag_match.group(1).decode('ascii')}"
    if not combined:
        return f"abort(exit {returncode},no msg)"
    return "ERR"


def build_project_source(raw_source: bytes) -> tuple[bytes, dict[str, Any]]:
    """Prepend `package app\\n` and record exact transformation metadata."""
    transformed = PROJECT_PACKAGE_HEADER + raw_source
    transform = {
        "kind": "prepend_package_header",
        "header_utf8": PROJECT_PACKAGE_HEADER.decode("utf-8"),
        "header_sha256": sha256_bytes(PROJECT_PACKAGE_HEADER),
        "target_relpath": "src/app/main.ko",
        "entry": "app.main",
        "project_toml_sha256": sha256_bytes(PROJECT_TOML_BYTES),
        "original_source_sha256": sha256_bytes(raw_source),
        "transformed_source_sha256": sha256_bytes(transformed),
    }
    return transformed, transform


def run_command_raw(
    argv: list[str],
    cwd: Path,
    timeout_sec: float,
    raw_artifact_prefix: Path | None = None,
) -> dict[str, Any]:
    """Execute command and capture raw stdout/stderr bytes via disk files, exit status, and timeout flag."""
    out_file = (
        Path(f"{raw_artifact_prefix}.stdout")
        if raw_artifact_prefix is not None
        else cwd / ".cmd.stdout"
    )
    err_file = (
        Path(f"{raw_artifact_prefix}.stderr")
        if raw_artifact_prefix is not None
        else cwd / ".cmd.stderr"
    )
    out_file.parent.mkdir(parents=True, exist_ok=True)
    with out_file.open("wb") as out_fh, err_file.open("wb") as err_fh:
        proc = subprocess.Popen(
            argv,
            cwd=cwd,
            stdout=out_fh,
            stderr=err_fh,
            start_new_session=True,
        )
        try:
            returncode: int | None = proc.wait(timeout=timeout_sec)
            timed_out = False
        except subprocess.TimeoutExpired:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait()
            returncode = 124
            timed_out = True

    stdout = out_file.read_bytes()
    stderr = err_file.read_bytes()
    if raw_artifact_prefix is None:
        out_file.unlink(missing_ok=True)
        err_file.unlink(missing_ok=True)
    else:
        Path(f"{raw_artifact_prefix}.status.json").write_text(
            json.dumps({"exit": returncode, "timed_out": timed_out}) + "\n",
            encoding="utf-8",
        )

    marker = classify_raw(returncode, timed_out, stdout, stderr)
    return {
        "argv": list(argv),
        "exit": returncode,
        "timed_out": timed_out,
        "stdout_bytes": stdout,
        "stderr_bytes": stderr,
        "stdout_len": len(stdout),
        "stderr_len": len(stderr),
        "stdout_sha256": sha256_bytes(stdout),
        "stderr_sha256": sha256_bytes(stderr),
        "stdout_b64": base64.b64encode(stdout).decode("ascii"),
        "stderr_b64": base64.b64encode(stderr).decode("ascii"),
        "marker": marker,
    }


def execute_item(
    kovenc: Path,
    name: str,
    source_path: Path,
    timeout_sec: float,
    evidence_dir: Path | None = None,
) -> dict[str, Any]:
    """Run a probe or acceptance program in both single-file and project modes."""
    raw_source = source_path.read_bytes()
    source_sha256 = sha256_bytes(raw_source)

    single_prefix = (evidence_dir / "raw" / f"{name}.single") if evidence_dir else None
    project_prefix = (evidence_dir / "raw" / f"{name}.project") if evidence_dir else None

    with tempfile.TemporaryDirectory(prefix=f"koven-conf-{name}-") as tmp:
        tmp_root = Path(tmp)

        single_dir = tmp_root / "single"
        single_dir.mkdir(parents=True, exist_ok=True)
        single_file = single_dir / source_path.name
        single_file.write_bytes(raw_source)
        single_res = run_command_raw(
            [str(kovenc), "run", source_path.name],
            cwd=single_dir,
            timeout_sec=timeout_sec,
            raw_artifact_prefix=single_prefix,
        )

        project_dir = tmp_root / "project"
        app_dir = project_dir / "src" / "app"
        app_dir.mkdir(parents=True, exist_ok=True)
        (project_dir / "project.toml").write_bytes(PROJECT_TOML_BYTES)
        transformed_bytes, transform_meta = build_project_source(raw_source)
        (app_dir / "main.ko").write_bytes(transformed_bytes)
        project_res = run_command_raw(
            [str(kovenc), "run", "--project", "project.toml", "--entry", "app.main"],
            cwd=project_dir,
            timeout_sec=timeout_sec,
            raw_artifact_prefix=project_prefix,
        )

    return {
        "name": name,
        "source_path": str(source_path),
        "source_sha256": source_sha256,
        "project_transform": transform_meta,
        "single": single_res,
        "project": project_res,
    }


def is_internal_error_marker(marker: str) -> bool:
    return marker in ("BE:InvalidSsa", "ICE:ownership", "ERR", "timeout") or marker.startswith(
        "abort(exit "
    )


def evaluate_mode_against_oracle(
    mode_res: dict[str, Any],
    expectation: dict[str, Any],
) -> tuple[str, str | None]:
    """Evaluate one mode (single or project) against the machine-readable expectation."""
    kind = expectation["kind"]
    marker = mode_res["marker"]
    exit_code = mode_res["exit"]
    stdout = mode_res["stdout_bytes"]
    stderr = mode_res["stderr_bytes"]

    if kind == "unreviewed":
        if is_internal_error_marker(marker):
            return "internal_error", f"unreviewed item hit internal error: {marker}"
        return "unreviewed", "expectation is unreviewed"

    if is_internal_error_marker(marker):
        return "internal_error", f"internal error or timeout: {marker}"

    if kind == "run_ok":
        expected_stdout = expectation["stdout"].encode("utf-8")
        expected_stderr = expectation.get("stderr", "").encode("utf-8")
        if exit_code != 0:
            return "regression", f"expected exit 0, got exit {exit_code} ({marker})"
        if stdout != expected_stdout:
            return (
                "wrong_output",
                f"exit 0 stdout mismatch: expected {expected_stdout!r}, got {stdout!r}",
            )
        if stderr != expected_stderr:
            return (
                "wrong_output",
                f"exit 0 stderr mismatch: expected {expected_stderr!r}, got {stderr!r}",
            )
        return "pass_run", None

    if kind == "compile_fail":
        expected_code = expectation["diagnostic"]
        expected_marker = f"FE:{expected_code}"
        if exit_code == 0:
            return "regression", f"expected compile_fail {expected_marker}, but exited 0"
        if stdout != b"":
            return "wrong_output", f"compile_fail produced unexpected stdout: {stdout!r}"
        if marker != expected_marker:
            return "regression", f"expected {expected_marker}, got {marker}"
        expected_codes = expectation.get("all_diagnostics", [expected_code])
        emitted_codes = extract_diagnostic_codes(stderr)
        if emitted_codes != expected_codes:
            return (
                "regression",
                f"expected error diagnostics {expected_codes}, got {emitted_codes}",
            )
        return "pass_diagnostic", None

    if kind == "runtime_abort":
        expected_msg = expectation["abort_message"]
        expected_marker = f"abort:{expected_msg}"
        if exit_code == 0:
            return "regression", f"expected runtime_abort {expected_marker}, but exited 0"
        if stdout != b"":
            return "wrong_output", f"runtime_abort produced unexpected stdout: {stdout!r}"
        if marker != expected_marker:
            return "regression", f"expected {expected_marker}, got {marker}"
        if not ABORT_FRAME_RE.search(stderr):
            return (
                "wrong_output",
                "runtime_abort stderr missing '  at <location>' stack frame per Guide 09 D12",
            )
        return "pass_abort", None

    if kind == "known_unimplemented":
        allowed_markers = expectation.get(
            "allowed_markers", ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165", "FE:L0080"]
        )
        if exit_code == 0:
            target_stdout = expectation.get("target_stdout")
            if target_stdout is not None and stdout != target_stdout.encode("utf-8"):
                return (
                    "wrong_output",
                    f"known_unimplemented item exited 0 with wrong stdout: expected {target_stdout.encode('utf-8')!r}, got {stdout!r}",
                )
            if stderr != b"":
                return (
                    "wrong_output",
                    f"known_unimplemented item exited 0 with non-empty stderr: {stderr!r}",
                )
            return (
                "unexpected_pass",
                "known_unimplemented item now exits 0; promote expectation to run_ok after review",
            )
        if marker in allowed_markers:
            return "known_unimplemented", None
        return (
            "regression",
            f"known_unimplemented marker changed: expected one of {allowed_markers}, got {marker}",
        )

    raise ValueError(f"unsupported expectation kind: {kind}")


def compare_item(
    execution: dict[str, Any],
    expectation: dict[str, Any],
    baseline_row: tuple[str, str] | None = None,
) -> dict[str, Any]:
    """Compare single and project execution results against expectation and optional baseline."""
    single_res = execution["single"]
    project_res = execution["project"]

    single_verdict, single_reason = evaluate_mode_against_oracle(single_res, expectation)
    project_verdict, project_reason = evaluate_mode_against_oracle(project_res, expectation)

    mode_consistent = single_res["marker"] == project_res["marker"]
    if single_res["exit"] == 0 and project_res["exit"] == 0:
        if single_res["stdout_bytes"] != project_res["stdout_bytes"]:
            mode_consistent = False

    reasons: list[str] = []
    if single_reason:
        reasons.append(f"single: {single_reason}")
    if project_reason:
        reasons.append(f"project: {project_reason}")
    if not mode_consistent:
        reasons.append(
            f"mode_mismatch: single={single_res['marker']} (stdout_sha256={single_res['stdout_sha256'][:12]}) "
            f"vs project={project_res['marker']} (stdout_sha256={project_res['stdout_sha256'][:12]})"
        )

    baseline_regression = False
    if baseline_row is not None and not expectation.get("v046_baseline_exempt", False):
        base_single, base_project = baseline_row
        if base_single == "OK" and single_res["marker"] != "OK":
            baseline_regression = True
            reasons.append(f"baseline regression in single: was OK, now {single_res['marker']}")
        if base_project == "OK" and project_res["marker"] != "OK":
            baseline_regression = True
            reasons.append(f"baseline regression in project: was OK, now {project_res['marker']}")

    verdicts = {single_verdict, project_verdict}
    if "internal_error" in verdicts:
        status = "internal_error"
    elif "wrong_output" in verdicts:
        status = "wrong_output"
    elif not mode_consistent:
        status = "mode_mismatch"
    elif "regression" in verdicts or baseline_regression:
        status = "regression"
    elif "unreviewed" in verdicts:
        status = "unreviewed"
    elif "unexpected_pass" in verdicts:
        status = "unexpected_pass"
    elif single_verdict == project_verdict and single_verdict in (
        "pass_run",
        "pass_diagnostic",
        "pass_abort",
        "known_unimplemented",
    ):
        status = single_verdict
    else:
        status = "regression"

    capability_passed = status in ("pass_run", "pass_diagnostic", "pass_abort")
    gate_ok = status in ("pass_run", "pass_diagnostic", "pass_abort", "known_unimplemented")

    return {
        "name": execution["name"],
        "expectation_kind": expectation["kind"],
        "guide_chapter": expectation.get("guide_chapter"),
        "probe_batch": expectation.get("probe_batch"),
        "baseline_marker": baseline_row[0] if baseline_row else expectation.get("baseline_marker"),
        "v046_baseline_exempt": bool(expectation.get("v046_baseline_exempt", False)),
        "single_marker": single_res["marker"],
        "project_marker": project_res["marker"],
        "single_verdict": single_verdict,
        "project_verdict": project_verdict,
        "mode_consistent": mode_consistent,
        "status": status,
        "capability_passed": capability_passed,
        "gate_ok": gate_ok,
        "reasons": reasons,
    }


def resolve_default_cli(root: Path) -> Path:
    env_cli = os.environ.get("KOVENC", "").strip()
    if env_cli:
        return Path(env_cli)
    release_cli = root / "target/release/kovenc"
    debug_cli = root / "target/debug/kovenc"
    if release_cli.is_file():
        return release_cli
    if debug_cli.is_file():
        return debug_cli
    return release_cli


def resolve_chapter_selector(selector: str) -> str:
    """Resolve a Guide chapter filename or 2-digit prefix ('01'..'13') to a canonical chapter filename."""
    cleaned = selector.strip()
    if cleaned in VALID_GUIDE_CHAPTERS:
        return cleaned
    if re.fullmatch(r"[0-9]{2}", cleaned):
        prefix = f"{cleaned}-"
        for ch in VALID_GUIDE_CHAPTERS:
            if ch.startswith(prefix):
                return ch
    raise ValueError(f"unknown Guide chapter selector: {selector!r}")


def evaluate_suite(
    kovenc: Path,
    corpus_root: Path,
    expectations_path: Path,
    baseline_path: Path | None = None,
    selected_probes: list[str] | None = None,
    selected_batches: list[str] | None = None,
    selected_chapters: list[str] | None = None,
    selected_programs: list[str] | None = None,
    include_programs: bool = True,
    timeout_sec: float = 60.0,
    strict_unreviewed: bool = True,
    compiler_commit: str = "unknown",
    evidence_dir: Path | None = None,
    jobs: int = 1,
) -> dict[str, Any]:
    """Run selected probes/programs, compare against expectations & baseline, and return full ledger."""
    if jobs < 1:
        raise ValueError(f"jobs must be >= 1, got {jobs}")
    if timeout_sec <= 0:
        raise ValueError(f"timeout_sec must be > 0, got {timeout_sec}")
    if selected_programs is not None and not include_programs:
        raise ValueError("cannot combine --program with --no-programs")
    kovenc = kovenc.resolve(strict=True)
    corpus_root = corpus_root.resolve(strict=True)
    expectations = load_expectations(expectations_path, corpus_root=corpus_root)
    baseline = (
        parse_tsv_matrix(
            baseline_path.read_text(encoding="utf-8"),
            expected_names=set(expectations["probes"].keys()),
        )
        if baseline_path is not None
        else {}
    )

    probe_names = sorted(expectations["probes"].keys())
    if selected_probes is not None:
        unknown = sorted(set(selected_probes) - set(probe_names))
        if unknown:
            raise ValueError(f"unknown probe names requested: {unknown}")
        if len(selected_probes) != len(set(selected_probes)):
            raise ValueError("duplicate probe names in selection")
        probe_names = [n for n in probe_names if n in set(selected_probes)]
    if selected_batches is not None:
        unknown_batches = sorted(set(selected_batches) - set(PROBE_BATCH_CATALOG.keys()))
        if unknown_batches:
            raise ValueError(f"unknown probe batches requested: {unknown_batches}")
        if len(selected_batches) != len(set(selected_batches)):
            raise ValueError("duplicate probe batches in selection")
        batch_set = set(selected_batches)
        probe_names = [
            n
            for n in probe_names
            if (expectations["probes"][n].get("probe_batch") or classify_probe_batch(n))
            in batch_set
        ]
    if selected_chapters is not None:
        resolved_chapters = [resolve_chapter_selector(c) for c in selected_chapters]
        if len(resolved_chapters) != len(set(resolved_chapters)):
            raise ValueError("duplicate Guide chapters in selection")
        chapter_set = set(resolved_chapters)
        probe_names = [
            n
            for n in probe_names
            if expectations["probes"][n].get("guide_chapter") in chapter_set
        ]
    if (
        selected_probes is not None
        or selected_batches is not None
        or selected_chapters is not None
    ) and not probe_names:
        raise ValueError("no probes matched the requested filters")

    if evidence_dir is not None:
        evidence_dir.mkdir(parents=True, exist_ok=True)

    initial_compiler_sha = sha256_file(kovenc)

    def _run_probe(name: str) -> dict[str, Any]:
        src_path = corpus_root / "probes" / f"{name}.ko"
        exec_record = execute_item(
            kovenc,
            name,
            src_path,
            timeout_sec=timeout_sec,
            evidence_dir=evidence_dir,
        )
        comp = compare_item(
            exec_record,
            expectations["probes"][name],
            baseline_row=baseline.get(name),
        )
        return {"execution": _strip_raw_bytes(exec_record), "comparison": comp}

    def _run_program(name: str) -> dict[str, Any]:
        src_path = corpus_root / "programs" / name / "main.ko"
        exec_record = execute_item(
            kovenc,
            f"program_{name}",
            src_path,
            timeout_sec=timeout_sec,
            evidence_dir=evidence_dir,
        )
        exec_record["name"] = name
        comp = compare_item(
            exec_record, expectations["programs"][name], baseline_row=None
        )
        return {"execution": _strip_raw_bytes(exec_record), "comparison": comp}

    program_names = sorted(expectations["programs"].keys()) if include_programs else []
    if selected_programs is not None:
        unknown_progs = sorted(set(selected_programs) - set(program_names))
        if unknown_progs:
            raise ValueError(f"unknown program names requested: {unknown_progs}")
        if len(selected_programs) != len(set(selected_programs)):
            raise ValueError("duplicate program names in selection")
        program_names = [n for n in program_names if n in set(selected_programs)]
    if jobs == 1:
        probe_results = [_run_probe(name) for name in probe_names]
        program_results = [_run_program(name) for name in program_names]
    else:
        with ThreadPoolExecutor(max_workers=jobs) as pool:
            probe_futures = [(name, pool.submit(_run_probe, name)) for name in probe_names]
            program_futures = [
                (name, pool.submit(_run_program, name)) for name in program_names
            ]
            probe_results = [fut.result() for _, fut in probe_futures]
            program_results = [fut.result() for _, fut in program_futures]

    if sha256_file(kovenc) != initial_compiler_sha:
        raise RuntimeError("compiler binary changed during conformance run")

    summary = summarize_comparisons(
        [r["comparison"] for r in probe_results],
        [r["comparison"] for r in program_results],
        strict_unreviewed=strict_unreviewed,
    )

    ledger = {
        "schema": SCHEMA_RESULTS,
        "compiler": {
            "path": str(kovenc),
            "sha256": initial_compiler_sha,
            "commit": compiler_commit,
        },
        "host": detect_host(),
        "summary": summary,
        "probes": probe_results,
        "programs": program_results,
    }

    if evidence_dir is not None:
        (evidence_dir / "results.json").write_text(
            json.dumps(ledger, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        tsv_lines = [
            f"{r['comparison']['name']}\t{r['comparison']['single_marker']}\t{r['comparison']['project_marker']}"
            for r in probe_results
        ]
        (evidence_dir / "matrix.tsv").write_text(
            "\n".join(tsv_lines) + ("\n" if tsv_lines else ""),
            encoding="utf-8",
        )
        from conformance_evidence import render_evidence_summary_markdown

        (evidence_dir / "SUMMARY.md").write_text(
            render_evidence_summary_markdown(ledger),
            encoding="utf-8",
        )

    return ledger


def _strip_raw_bytes(exec_record: dict[str, Any]) -> dict[str, Any]:
    out = dict(exec_record)
    for mode in ("single", "project"):
        m = dict(out[mode])
        m.pop("stdout_bytes", None)
        m.pop("stderr_bytes", None)
        out[mode] = m
    return out


def summarize_comparisons(
    probe_comparisons: list[dict[str, Any]],
    program_comparisons: list[dict[str, Any]],
    strict_unreviewed: bool = True,
) -> dict[str, Any]:
    counts: dict[str, int] = {
        "pass_run": 0,
        "pass_diagnostic": 0,
        "pass_abort": 0,
        "known_unimplemented": 0,
        "unreviewed": 0,
        "wrong_output": 0,
        "mode_mismatch": 0,
        "regression": 0,
        "internal_error": 0,
        "unexpected_pass": 0,
    }
    failures: list[str] = []
    unexpected_passes: list[str] = []
    for item in probe_comparisons + program_comparisons:
        st = item["status"]
        counts[st] = counts.get(st, 0) + 1
        if st == "unexpected_pass":
            unexpected_passes.append(item["name"])
        if st in (
            "wrong_output",
            "mode_mismatch",
            "regression",
            "internal_error",
            "unexpected_pass",
        ):
            failures.append(f"{item['name']} ({st}): {'; '.join(item['reasons'])}")
        elif st == "unreviewed" and strict_unreviewed:
            failures.append(
                f"{item['name']} (unreviewed): requires human oracle confirmation"
            )

    probe_cap_passed = sum(1 for item in probe_comparisons if item["capability_passed"])
    program_run_passed = sum(
        1 for item in program_comparisons if item["status"] == "pass_run"
    )

    by_chapter: dict[str, dict[str, int]] = {}
    for item in probe_comparisons:
        ch = item.get("guide_chapter") or "unassigned"
        bucket = by_chapter.setdefault(
            ch,
            {
                "total": 0,
                "pass_run": 0,
                "pass_diagnostic": 0,
                "pass_abort": 0,
                "known_unimplemented": 0,
                "capability_passed": 0,
            },
        )
        bucket["total"] += 1
        st = item["status"]
        if st in ("pass_run", "pass_diagnostic", "pass_abort", "known_unimplemented"):
            bucket[st] += 1
        if item["capability_passed"]:
            bucket["capability_passed"] += 1

    by_batch: dict[str, dict[str, int]] = {}
    for item in probe_comparisons:
        b_key = item.get("probe_batch") or classify_probe_batch(item["name"])
        b_bucket = by_batch.setdefault(
            b_key,
            {
                "total": 0,
                "pass_run": 0,
                "pass_diagnostic": 0,
                "pass_abort": 0,
                "known_unimplemented": 0,
                "capability_passed": 0,
            },
        )
        b_bucket["total"] += 1
        st = item["status"]
        if st in ("pass_run", "pass_diagnostic", "pass_abort", "known_unimplemented"):
            b_bucket[st] += 1
        if item["capability_passed"]:
            b_bucket["capability_passed"] += 1

    ordered_by_batch = {
        k: by_batch[k] for k in PROBE_BATCH_CATALOG if k in by_batch
    }
    for k in sorted(by_batch.keys()):
        if k not in ordered_by_batch:
            ordered_by_batch[k] = by_batch[k]

    p2_a_eval = p2_a_runnable = p2_b_eval = p2_b_diag = p2_c_eval = 0
    for item in probe_comparisons:
        name = item["name"]
        b_key = item.get("probe_batch") or classify_probe_batch(name)
        if b_key not in FOUNDATIONAL_BATCHES:
            continue
        if name in FOUNDATIONAL_GROUP_B_PROBES:
            p2_b_eval += 1
            if item["status"] == "pass_diagnostic":
                p2_b_diag += 1
        elif name in FOUNDATIONAL_GROUP_C_PROBES:
            p2_c_eval += 1
        else:
            p2_a_eval += 1
            if item["status"] in ("pass_run", "pass_abort"):
                p2_a_runnable += 1

    foundational_p2 = {
        "group_a_evaluated": p2_a_eval,
        "group_a_runnable_passed": p2_a_runnable,
        "group_b_evaluated": p2_b_eval,
        "group_b_diagnostic_passed": p2_b_diag,
        "group_c_evaluated": p2_c_eval,
        "p2_target_min": P2_GROUP_A_TARGET_MIN,
        "p2_gate_met": (
            p2_a_eval == 74
            and p2_a_runnable >= P2_GROUP_A_TARGET_MIN
            and p2_b_diag == len(FOUNDATIONAL_GROUP_B_PROBES)
        ),
    }

    return {
        "total_probes": len(probe_comparisons),
        "probe_capability_passed": probe_cap_passed,
        "total_programs": len(program_comparisons),
        "program_run_passed": program_run_passed,
        "counts": counts,
        "by_chapter": dict(sorted(by_chapter.items())),
        "by_batch": ordered_by_batch,
        "foundational_p2": foundational_p2,
        "unexpected_passes": unexpected_passes,
        "gate_passed": len(failures) == 0,
        "failures": failures,
    }
