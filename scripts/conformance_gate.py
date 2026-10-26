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
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import tempfile
from typing import Any


SCHEMA_EXPECTATIONS = "koven.conformance-expectations.v1"
SCHEMA_RESULTS = "koven.conformance-results.v1"

PROJECT_TOML_BYTES = (
    b'schema = "koven.project"\n'
    b"version = 1\n\n"
    b"[project]\n"
    b'name = "probe"\n'
    b'source-roots = ["src"]\n'
)
PROJECT_PACKAGE_HEADER = b"package app\n"

DIAGNOSTIC_RE = re.compile(rb"(?m)^error\[(L[0-9]{4})\](?:\s|:)")
BACKEND_RE = re.compile(rb"lowering failed with ([A-Za-z]+)")
BACKEND_OBJECT_RE = re.compile(rb"native object ([A-Za-z]+):")
OWNERSHIP_ICE_RE = re.compile(rb"ownership checking failed: [a-z -]+")
SIMPLE_PRINTLN_LITERAL_RE = re.compile(r'println\("([^"\\$]*)"\)')

# Multi-diagnostic compile_fail probes where more than one distinct error[Lxxxx] code is emitted on stderr
EXPECTED_MULTI_DIAGNOSTICS: dict[str, list[str]] = {
    "p25_linked_iter": ["L0133", "L0132"],
}

VALID_EXPECTATION_KINDS = frozenset(
    {
        "run_ok",
        "compile_fail",
        "runtime_abort",
        "known_unimplemented",
        "unreviewed",
    }
)

# Human-audited exact stdout oracles for multi-statement / loop / variable-print probes in conformance/probes
CONFIRMED_STDOUT_ORACLES: dict[str, str] = {
    "p07_while_var": "while\n",
    "p10_generic_fn": "generic\nint\n",
    "p19_mutablelist": "list\n",
    "p26_borrow_return": "borrowed\n",
    "p29_default_super": "ab\n",
    "p31_string_ops": "abc\n",
    "p32_deinit": "body\nb\na\n",
    "p34_for_range": "for-range\n",
    "p38_list_of_strings": "x\ny\n",
    "p46_string_field_class": "ann\n",
    "p47_list_index_string": "b\n",
    "p48_nested_loops_break": "nested\n",
    "q10_var_reassign_string": "abc\n",
    "q11_while_list_index": "index-loop\n",
    "q13_if_expr_value": "big\n",
    "q14_when_subjectless": "mid\n",
    "q15_borrow_param_string_list": "borrow-list\n",
    "q16_interface_static": "dog\n",
    "q19_deep_return_cleanup": "b\na\na\n",
    "q20_replace_field": "old\nnew\n",
    "t05_map_loop_continue": "map-continue\n",
    "t06_list_loop_return": "loop-return\n",
    "t08_vmethod_string": "pos\n",
    "t10_map_nested_if_break": "map-nested-break\n",
    "u02_list_add_loop_string": "x\ny\n",
    "v06_for_range_sum": "for-range\n",
    "v07_range_max": "range-max\n",
    "v08_long_range": "long-range\n",
    "v09_reverse_empty": "reverse-empty\n",
    "v15_map_for": "b\na\n",
    "v17_convert_in_loop": "conv-loop\n",
    "v18_while_nullable_class": "while-class\n",
    "v20_range_in_list": "range-in-list\n",
    "v21_for_range_return": "range-return\n",
    "v22_range_with_map": "range-map\n",
    "v23_nested_range_break": "nested-range\n",
    "w01_map_for_sum": "map-sum\n",
    "w02_map_for_order": "map-order\n",
    "w03_map_overwrite_remove": "map-reorder\n",
    "w04_map_for_break": "map-break\n",
    "w05_map_for_return": "map-return\n",
    "w07_map_nested": "map-nested\n",
    "w08_map_string_values": "two\none\n",
    "w09_map_temp_source": "map-temp\n",
    "w10_map_for_continue_count": "map-continue2\n",
    "w11_word_count": "word-count\n",
    "w12": "b\nafter\n",
    "w13": "seen\nend\n",
    "w14": "end\n",
    "w15": "seen\nend\n",
    "w16": "borrow-param-sum\n",
    "w17": "b\nend\n",
    "w18": "end\n",
    "w19": "b\nend\n",
    "x03_if_call_borrow_arg": "obs\ndone\n",
    "y05_take_borrow_val_call": "a\nb\n",
    "y06_take_borrow_val_for": "a\nb\n",
}

# Documented Guide v0.46 probe changes and post-aef3f4f8 merged Spec advancements
V046_PROBE_OVERRIDES: dict[str, dict[str, Any]] = {
    "p28_companion": {
        "kind": "known_unimplemented",
        "allowed_markers": ["BE:UnsupportedNode", "BE:UnsupportedSource", "BE:MissingFact", "FE:L0165"],
        "target_stdout": "companion\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 anonymous companion { } syntax; SPEC-0323 fixed static role, native N06 pending",
    },
    "q12_companion_const": {
        "kind": "run_ok",
        "stdout": "cconst\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 anonymous companion { } syntax; associated constant read succeeds",
    },
    "p23_global_var": {
        "kind": "compile_fail",
        "diagnostic": "L0167",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D4 (SPEC-0343): top-level runtime var rejected with L0167",
    },
    "p04_overflow": {
        "kind": "runtime_abort",
        "abort_message": "integer overflow in +",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D12 (SPEC-0355): checked + overflow emits structured abort message",
    },
    "p45_abort_message": {
        "kind": "runtime_abort",
        "abort_message": "boom",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D12 (SPEC-0355): error() emits structured abort message",
    },
    "v12_convert_abort": {
        "kind": "runtime_abort",
        "abort_message": "value out of range for ULong",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D12 (SPEC-0355): checked conversion emits structured abort message",
    },
    "p16_result_q": {
        "kind": "known_unimplemented",
        "allowed_markers": ["FE:L0165", "BE:UnsupportedNode", "BE:MissingFact"],
        "target_stdout": "question\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 E4 (SPEC-0349): Result in builtin env; ? lowering pending N08",
    },
    "p16b_result_string": {
        "kind": "known_unimplemented",
        "allowed_markers": ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165"],
        "target_stdout": "good\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 E4 (SPEC-0349): Result in builtin env; String payload lowering pending N08",
    },
    "p37_is_generic_case": {
        "kind": "run_ok",
        "stdout": "case-noargs\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0349: parameterless is Result.Ok infers root type args from subject",
    },
    "s01_result_builtin": {
        "kind": "run_ok",
        "stdout": "result-builtin\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 E4 (SPEC-0349): Result/Ok/Err injected into builtin environment",
    },
    "p14_nullable": {
        "kind": "run_ok",
        "stdout": "smart\nbangbang\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: E3 ownership refinement nullable pilot",
    },
    "p14b_nullable_simple": {
        "kind": "run_ok",
        "stdout": "smart\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: E3 ownership refinement nullable pilot",
    },
    "p14c_nullable_int": {
        "kind": "run_ok",
        "stdout": "nint\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: inline nullable Int? in E3 refinement layer",
    },
    "q05_nullable_param": {
        "kind": "run_ok",
        "stdout": "non-null\nnull\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: E3 ownership refinement nullable pilot",
    },
    "v03_join_nullable": {
        "kind": "run_ok",
        "stdout": "join-nullable\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: inline nullable Int? return in E3 refinement layer",
    },
    "v13_while_nullable_int": {
        "kind": "run_ok",
        "stdout": "while-nullable\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: inline nullable Int? in while loop in E3 refinement layer",
    },
    "x02_string_nullable_return": {
        "kind": "run_ok",
        "stdout": "hit\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: pointer-niche inline nullable String? return in E3 refinement layer",
    },
    "p08_when_int": {
        "kind": "run_ok",
        "stdout": "two\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: subject when (Int subject equality) in E3 refinement layer",
    },
    "v16_when_in_range": {
        "kind": "run_ok",
        "stdout": "small\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: subject when (Int subject `in` Range) in E3 refinement layer",
    },
    "v19_stmt_when_mixed": {
        "kind": "run_ok",
        "stdout": "stmt-when-mixed\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: subject when (statement when with mixed branch types) in E3 refinement layer",
    },
    "e01_enum_when_bool_return": {
        "kind": "run_ok",
        "stdout": "e1\n",
        "v046_baseline_exempt": True,
        "note": "Enum when regression fixed on main",
    },
    "e02_enum_when_stmt_println": {
        "kind": "run_ok",
        "stdout": "e2\n",
        "v046_baseline_exempt": True,
        "note": "Enum when statement regression fixed on main",
    },
    "e03_enum_when_bool_val": {
        "kind": "run_ok",
        "stdout": "e5\n",
        "v046_baseline_exempt": True,
        "note": "Enum when bool val regression fixed on main",
    },
    "p49_when_enum_no_payload": {
        "kind": "run_ok",
        "stdout": "green\n",
        "v046_baseline_exempt": True,
        "note": "Payloadless enum when: main prints name(Color.Green), so the when branch yields green",
    },
    "p50_enum_method": {
        "kind": "run_ok",
        "stdout": "enum-method\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: enum method when (this) with is-tests and implicit-this payload reads in E3 refinement layer",
    },
    "u01_enum_case_borrow_arg": {
        "kind": "run_ok",
        "stdout": "enum-arg\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0320: enum case borrow argument classification",
    },
    "y01_import_take_for_sum": {
        "kind": "run_ok",
        "stdout": "take\n",
        "v046_baseline_exempt": True,
        "note": "take() lowered on main; for-loop sums the first two elements and prints take",
    },
    "y03_method_take_for": {
        "kind": "run_ok",
        "stdout": "take-method\n",
        "v046_baseline_exempt": True,
        "note": "take() method call lowered on main; for-loop sums the first two elements and prints take-method",
    },
    "y04_take_for_println": {
        "kind": "run_ok",
        "stdout": "a\nb\n",
        "v046_baseline_exempt": True,
        "note": "take() lowered on main; for-loop prints the first two elements",
    },
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


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
                elif not isinstance(spec.get("stdout"), str) or not spec["stdout"]:
                    raise ValueError(f"probes.{name} with run_ok requires non-empty stdout")
            elif kind == "compile_fail":
                if not isinstance(spec.get("diagnostic"), str) or not re.fullmatch(
                    r"L[0-9]{4}", spec["diagnostic"]
                ):
                    raise ValueError(
                        f"{section}.{name} with compile_fail requires diagnostic Lxxxx"
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
                if not isinstance(spec.get("abort_message"), str) or not spec["abort_message"]:
                    raise ValueError(
                        f"{section}.{name} with runtime_abort requires abort_message"
                    )
            if "oracle_origin" in spec and (
                not isinstance(spec["oracle_origin"], str) or not spec["oracle_origin"].strip()
            ):
                raise ValueError(f"{section}.{name} has invalid oracle_origin")

            if corpus_root is not None:
                src_file = (
                    corpus_root / "probes" / f"{name}.ko"
                    if section == "probes"
                    else corpus_root / "programs" / name / "main.ko"
                )
                if not src_file.is_file():
                    raise ValueError(f"missing source file for {section}.{name}: {src_file}")
                actual_sha = sha256_file(src_file)
                if actual_sha != spec["source_sha256"]:
                    raise ValueError(
                        f"source_sha256 mismatch for {section}.{name}: expected {spec['source_sha256']}, got {actual_sha}"
                    )
                if section == "programs" and "expected_file" in spec:
                    exp_file = corpus_root / spec["expected_file"]
                    if not exp_file.is_file():
                        raise ValueError(
                            f"missing expected_file for programs.{name}: {exp_file}"
                        )
                    if sha256_file(exp_file) != spec["expected_sha256"]:
                        raise ValueError(f"expected_sha256 mismatch for programs.{name}")
                    spec["stdout"] = exp_file.read_text(encoding="utf-8")
                    spec["target_stdout"] = spec["stdout"]

    if corpus_root is not None:
        actual_probes = {p.stem for p in (corpus_root / "probes").glob("*.ko")}
        expected_probes = set(data["probes"].keys())
        missing = sorted(actual_probes - expected_probes)
        extra = sorted(expected_probes - actual_probes)
        if missing or extra:
            raise ValueError(
                f"probe inventory mismatch: missing_in_expectations={missing}, extra_in_expectations={extra}"
            )
    return data


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
    for path in sorted(probes_dir.glob("*.ko")):
        name = path.stem
        raw_bytes = path.read_bytes()
        src_text = raw_bytes.decode("utf-8")
        src_sha = sha256_bytes(raw_bytes)
        base_single, base_project = baseline[name]
        assert base_single == base_project, f"baseline mismatch for {name}"

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

        if name in V046_PROBE_OVERRIDES:
            entry = dict(V046_PROBE_OVERRIDES[name])
            entry["source_sha256"] = src_sha
            entry["oracle_origin"] = "v046_spec_contract"
            probes_out[name] = entry
            continue

        if base_single == "OK":
            if single_static_stdout is not None:
                probes_out[name] = {
                    "kind": "run_ok",
                    "source_sha256": src_sha,
                    "stdout": single_static_stdout,
                    "oracle_origin": stdout_origin,
                }
            else:
                probes_out[name] = {
                    "kind": "unreviewed",
                    "source_sha256": src_sha,
                    "baseline_marker": "OK",
                    "note": "multi-line or computed println output requires human oracle confirmation",
                }
        elif base_single.startswith("FE:"):
            code = base_single.split(":", 1)[1]
            if code == "L0165" or name in ("p27_thread", "s02_pair_builtin"):
                entry = {
                    "kind": "known_unimplemented",
                    "source_sha256": src_sha,
                    "allowed_markers": [
                        base_single,
                        "BE:UnsupportedNode",
                        "BE:MissingFact",
                    ],
                    "oracle_origin": "baseline_unimplemented_audited",
                }
                if single_static_stdout is not None:
                    entry["target_stdout"] = single_static_stdout
                probes_out[name] = entry
            else:
                entry = {
                    "kind": "compile_fail",
                    "source_sha256": src_sha,
                    "diagnostic": code,
                    "oracle_origin": "baseline_diagnostic_audited",
                }
                if name in EXPECTED_MULTI_DIAGNOSTICS:
                    entry["all_diagnostics"] = list(EXPECTED_MULTI_DIAGNOSTICS[name])
                probes_out[name] = entry
        elif base_single.startswith("BE:"):
            entry = {
                "kind": "known_unimplemented",
                "source_sha256": src_sha,
                "allowed_markers": ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165"],
                "oracle_origin": "baseline_unimplemented_audited",
            }
            if single_static_stdout is not None:
                entry["target_stdout"] = single_static_stdout
            probes_out[name] = entry
        else:
            probes_out[name] = {
                "kind": "unreviewed",
                "source_sha256": src_sha,
                "baseline_marker": base_single,
                "note": "requires human oracle confirmation",
            }

    programs_out: dict[str, Any] = {}
    for prog_dir in sorted(programs_dir.iterdir()):
        if not prog_dir.is_dir():
            continue
        main_ko = prog_dir / "main.ko"
        expected_txt = prog_dir / "expected.txt"
        if not main_ko.is_file() or not expected_txt.is_file():
            continue
        programs_out[prog_dir.name] = {
            "kind": "known_unimplemented",
            "source_sha256": sha256_file(main_ko),
            "expected_file": f"programs/{prog_dir.name}/expected.txt",
            "expected_sha256": sha256_file(expected_txt),
            "allowed_markers": [
                "FE:L0080",
                "FE:L0084",
                "FE:L0085",
                "FE:L0165",
                "BE:UnsupportedNode",
                "BE:MissingFact",
            ],
            "oracle_origin": "expected_txt_file",
        }

    return {
        "schema": SCHEMA_EXPECTATIONS,
        "probes": probes_out,
        "programs": programs_out,
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


def evaluate_suite(
    kovenc: Path,
    corpus_root: Path,
    expectations_path: Path,
    baseline_path: Path | None = None,
    selected_probes: list[str] | None = None,
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
    for item in probe_comparisons + program_comparisons:
        st = item["status"]
        counts[st] = counts.get(st, 0) + 1
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

    return {
        "total_probes": len(probe_comparisons),
        "probe_capability_passed": probe_cap_passed,
        "total_programs": len(program_comparisons),
        "program_run_passed": program_run_passed,
        "counts": counts,
        "gate_passed": len(failures) == 0,
        "failures": failures,
    }


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
    args = parser.parse_args(argv)

    if args.init_expectations:
        baseline_file = args.baseline or (args.corpus / "baseline-aef3f4f8.tsv")
        derived = derive_static_expectations(args.corpus, baseline_file)
        args.expectations.write_text(
            json.dumps(derived, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        print(
            f"conformance: initialized {len(derived['probes'])} probes and "
            f"{len(derived['programs'])} programs in {args.expectations}"
        )
        return 0

    ledger = evaluate_suite(
        kovenc=args.cli,
        corpus_root=args.corpus,
        expectations_path=args.expectations,
        baseline_path=args.baseline,
        selected_probes=args.probes,
        include_programs=not args.no_programs,
        timeout_sec=args.timeout,
        strict_unreviewed=not args.allow_unreviewed,
        compiler_commit=args.compiler_commit,
        evidence_dir=args.evidence,
        jobs=args.jobs,
    )
    summary = ledger["summary"]
    print(
        f"conformance: probes_cap_passed={summary['probe_capability_passed']}/{summary['total_probes']} "
        f"programs_passed={summary['program_run_passed']}/{summary['total_programs']} "
        f"counts={json.dumps(summary['counts'], sort_keys=True)}"
    )
    if not summary["gate_passed"]:
        for failure in summary["failures"]:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
