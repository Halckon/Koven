#!/usr/bin/env python3
"""SPEC-0495: Reversible shadow pilot and dual-host A/B harness for E2 two-source Unit CI.

Responsibilities:
1. Drive both baseline (358 Cargo calls) and candidate (5 Cargo --no-run builds + 5 Cargo
   launch probes + 358 direct binary calls) from one shared 179-entry manifest, verified
   against scripts/check_e2_two_source_unit_ci.sh without faking CI or GITHUB_ACTIONS.
2. Lock each configuration's compiled test binary via Cargo JSON (host/target triple,
   package_id, target name/kind, resolved features, test profile, debuginfo, build flags)
   accepting only a unique existing executable.
3. Verify runtime environment equivalence via a real Cargo launch probe (target.<triple>.runner)
   against a direct launch probe (cwd, allowlisted Cargo/toolchain env vars, dynamic library
   search paths + resolved libLLVM, and actual llvm-config/clang), never printing raw
   environments or secrets.
4. Enforce hard anti-skip validation (list hit == 1, execution hit == 1, exit 0 alone is
   insufficient) and first-class expected-failure classification + failure witness checking.
5. Compare baseline (Arm A) vs candidate (Arm B) ledgers and emit per-host cold/warm telemetry
   (build/probe/list/run wall-clock, child peak RSS, disk usage, OOM/retry flags, and optional
   GitHub Actions queue/job/reference step wall-clock).
"""

from __future__ import annotations

import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "scripts/e2_two_source_unit_manifest.json"
MANIFEST_SCHEMA = "koven.e2-two-source-shadow-manifest.v1"
LEDGER_SCHEMA = "koven.e2-two-source-shadow-ledger.v1"
COMPARISON_SCHEMA = "koven.e2-two-source-shadow-comparison.v1"

# Strict exact-key allowlist for probe & build snapshots. Never use broad prefixes like
# startswith("CARGO_") which could capture CARGO_REGISTRY_TOKEN or credentials.
EXACT_CARGO_PACKAGE_ENV_KEYS = (
    "CARGO",
    "CARGO_MANIFEST_DIR",
    "CARGO_MANIFEST_PATH",
    "CARGO_PKG_AUTHORS",
    "CARGO_PKG_DESCRIPTION",
    "CARGO_PKG_HOMEPAGE",
    "CARGO_PKG_LICENSE",
    "CARGO_PKG_LICENSE_FILE",
    "CARGO_PKG_NAME",
    "CARGO_PKG_README",
    "CARGO_PKG_REPOSITORY",
    "CARGO_PKG_RUST_VERSION",
    "CARGO_PKG_VERSION",
    "CARGO_PKG_VERSION_MAJOR",
    "CARGO_PKG_VERSION_MINOR",
    "CARGO_PKG_VERSION_PATCH",
    "CARGO_PKG_VERSION_PRE",
    "CARGO_CRATE_NAME",
    "CARGO_BIN_NAME",
    "OUT_DIR",
)

EXACT_BUILD_AND_TOOLCHAIN_ENV_KEYS = (
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "CARGO_INCREMENTAL",
    "CARGO_BUILD_JOBS",
    "CARGO_PROFILE_DEV_DEBUG",
    "CARGO_PROFILE_TEST_DEBUG",
    "CARGO_TERM_COLOR",
    "RUST_BACKTRACE",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC",
    "RUSTDOC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "LLVM_SYS_211_PREFIX",
    "CI",
    "GITHUB_ACTIONS",
    "RUNNER_OS",
    "RUNNER_ARCH",
    "RUNNER_TEMP",
)

DYLIB_PATH_ENV_KEYS = (
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
)

SECRET_KEY_SUBSTRINGS = (
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "PRIVATE",
    "CREDENTIAL",
    "AUTH",
    "COOKIE",
    "HEADER",
    "SESSION",
)


class ShadowPilotError(RuntimeError):
    """Structured fail-closed pilot error retaining host, config_id, and test_name."""

    def __init__(
        self,
        kind: str,
        detail: str,
        *,
        host: str | None = None,
        config_id: str | None = None,
        test_name: str | None = None,
        evidence: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(f"[{kind}] host={host} config={config_id} test={test_name}: {detail}")
        self.kind = kind
        self.detail = detail
        self.host = host
        self.config_id = config_id
        self.test_name = test_name
        self.evidence = evidence or {}

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": self.kind,
            "detail": self.detail,
            "host": self.host,
            "config_id": self.config_id,
            "test_name": self.test_name,
            "evidence": self.evidence,
        }


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def write_json_atomic(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    tmp_path = Path(tmp_name)
    try:
        data = (json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=True) + "\n").encode("utf-8")
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
        os.replace(tmp_path, path)
    finally:
        tmp_path.unlink(missing_ok=True)


def is_safe_allowlisted_key(key: str) -> bool:
    if key in EXACT_CARGO_PACKAGE_ENV_KEYS or key in EXACT_BUILD_AND_TOOLCHAIN_ENV_KEYS:
        return True
    upper = key.upper()
    if any(marker in upper for marker in SECRET_KEY_SUBSTRINGS):
        return False
    if key.startswith("CARGO_BIN_EXE_"):
        return True
    return False


def extract_allowlisted_env(env: dict[str, str]) -> dict[str, str]:
    selected: dict[str, str] = {}
    for key in sorted(env):
        if is_safe_allowlisted_key(key):
            selected[key] = env[key]
    return selected


def split_search_path(raw: str | None) -> list[str]:
    if not raw:
        return []
    return [part for part in raw.split(os.pathsep) if part]


def load_manifest(path: Path = DEFAULT_MANIFEST) -> dict[str, Any]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    validate_manifest_structure(raw)
    return raw


def validate_manifest_structure(manifest: dict[str, Any]) -> None:
    if not isinstance(manifest, dict) or manifest.get("schema") != MANIFEST_SCHEMA:
        raise ShadowPilotError("manifest_invalid", f"expected schema {MANIFEST_SCHEMA}")
    if manifest.get("cargo_jobs") != 1:
        raise ShadowPilotError("manifest_invalid", "cargo_jobs must remain locked to 1 (-j1)")
    configs = manifest.get("configs")
    if not isinstance(configs, list) or not configs:
        raise ShadowPilotError("manifest_invalid", "configs must be a non-empty list")
    seen_configs: set[str] = set()
    total = 0
    for cfg in configs:
        cid = cfg.get("config_id")
        if not isinstance(cid, str) or not cid or cid in seen_configs:
            raise ShadowPilotError("manifest_invalid", f"invalid or duplicate config_id: {cid!r}")
        seen_configs.add(cid)
        for req in ("package", "target_name", "target_kind", "cargo_target_args", "cargo_features_arg", "expected_features", "entries"):
            if req not in cfg:
                raise ShadowPilotError("manifest_invalid", f"missing {req}", config_id=cid)
        if sorted(cfg["cargo_features_arg"].split()) != sorted(cfg["expected_features"]):
            raise ShadowPilotError("manifest_invalid", "expected_features disagrees with cargo_features_arg", config_id=cid)
        entries = cfg["entries"]
        if not isinstance(entries, list) or not entries:
            raise ShadowPilotError("manifest_invalid", "entries must be non-empty", config_id=cid)
        seen_tests: set[tuple[str, bool]] = set()
        for entry in entries:
            tname = entry.get("test_name")
            if not isinstance(tname, str) or not tname:
                raise ShadowPilotError("manifest_invalid", "empty test_name", config_id=cid)
            if entry.get("exact") is not True:
                raise ShadowPilotError("manifest_invalid", "every E2 entry must use --exact", config_id=cid, test_name=tname)
            ignored = entry.get("ignored")
            if not isinstance(ignored, bool):
                raise ShadowPilotError("manifest_invalid", "ignored must be bool", config_id=cid, test_name=tname)
            outcome = entry.get("expected_outcome")
            if outcome not in ("pass", "expected_failure"):
                raise ShadowPilotError("manifest_invalid", f"unsupported expected_outcome {outcome!r}", config_id=cid, test_name=tname)
            witnesses = entry.get("expected_failure_substrings")
            if not isinstance(witnesses, list) or any(not isinstance(w, str) or not w for w in witnesses):
                raise ShadowPilotError("manifest_invalid", "invalid expected_failure_substrings", config_id=cid, test_name=tname)
            if outcome == "expected_failure" and not witnesses:
                raise ShadowPilotError("manifest_invalid", "expected_failure requires non-empty expected_failure_substrings", config_id=cid, test_name=tname)
            key = (tname, ignored)
            if key in seen_tests:
                raise ShadowPilotError("manifest_invalid", "duplicate test entry in config", config_id=cid, test_name=tname)
            seen_tests.add(key)
            total += 1
    if manifest.get("total_configs") != len(configs) or manifest.get("total_entries") != total:
        raise ShadowPilotError("manifest_invalid", "total_configs / total_entries mismatch")


def entry_filter_args(entry: dict[str, Any]) -> list[str]:
    args = ["--exact"]
    if entry["ignored"]:
        args.append("--ignored")
    return args


def verify_shell_script_execution_branch_static(script_path: Path) -> int:
    """Verify bash syntax and exact 1:1 structural parity between --list and execution branches without faking CI."""
    if not script_path.is_file():
        raise ShadowPilotError("manifest_drift", f"missing source script {script_path}")

    syntax_proc = subprocess.run(
        ["bash", "-n", str(script_path)],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    if syntax_proc.returncode != 0:
        raise ShadowPilotError(
            "manifest_drift",
            f"shell syntax check failed on {script_path.name}: {syntax_proc.stderr.strip()}",
        )

    code_lines = [
        ln.strip()
        for ln in script_path.read_text(encoding="utf-8").splitlines()
        if ln.strip() and not ln.strip().startswith("#")
    ]
    list_re = re.compile(r"^listing=\$\((cargo test .+) --list\)$")
    grep_re = re.compile(r'^\[\[ \$\(grep -Fxc .+ <<< "\$listing"\) == 1 \]\]$')

    pairs = 0
    idx = 0
    while idx < len(code_lines):
        m = list_re.match(code_lines[idx])
        if m:
            expected_exec_cmd = m.group(1)
            if idx + 4 >= len(code_lines):
                raise ShadowPilotError(
                    "manifest_drift",
                    f"{script_path.name}: truncated execution block after listing command {code_lines[idx]!r}",
                )
            grep_ln = code_lines[idx + 1]
            if_ln = code_lines[idx + 2]
            exec_ln = code_lines[idx + 3]
            fi_ln = code_lines[idx + 4]
            if (
                not grep_re.match(grep_ln)
                or if_ln != 'if [[ "$mode" != --list-only ]]; then'
                or exec_ln != expected_exec_cmd
                or fi_ln != "fi"
            ):
                raise ShadowPilotError(
                    "manifest_drift",
                    f"{script_path.name}: execution branch diverges from --list branch: "
                    f"expected {expected_exec_cmd!r}, got block {[grep_ln, if_ln, exec_ln, fi_ln]!r}",
                )
            pairs += 1
            idx += 5
            continue
        idx += 1

    if pairs == 0:
        raise ShadowPilotError("manifest_drift", f"{script_path.name}: found zero paired cargo test --list blocks")

    total_cargo_tokens = len(re.findall(r"\bcargo\b", "\n".join(code_lines)))
    if total_cargo_tokens != 2 * pairs:
        raise ShadowPilotError(
            "manifest_drift",
            f"{script_path.name}: expected {2 * pairs} cargo invocations across {pairs} paired blocks, found {total_cargo_tokens}",
        )

    exec_if_count = sum(1 for ln in code_lines if ln == 'if [[ "$mode" != --list-only ]]; then')
    if exec_if_count != 1 + pairs:
        raise ShadowPilotError(
            "manifest_drift",
            f"{script_path.name}: expected 1 CI guard + {pairs} execution blocks ({1 + pairs} total), found {exec_if_count}",
        )
    return pairs


def capture_shell_script_list_calls(
    script_path: Path,
    root: Path = ROOT,
    *,
    env: dict[str, str] | None = None,
    list_only: bool = True,
) -> list[list[str]]:
    if not script_path.is_file():
        raise ShadowPilotError("manifest_drift", f"missing source script {script_path}")

    with tempfile.TemporaryDirectory(prefix="koven-e2-manifest-check-") as tmp:
        tmp_path = Path(tmp)
        log_file = tmp_path / "calls.tsv"
        cargo_stub = tmp_path / "cargo"
        cargo_stub.write_text(
            "#!/bin/sh\n"
            'printf "%s" "$1" >> "$KOVEN_STUB_LOG"\n'
            "shift\n"
            'for arg in "$@"; do printf "\\t%s" "$arg" >> "$KOVEN_STUB_LOG"; done\n'
            'printf "\\n" >> "$KOVEN_STUB_LOG"\n'
            'prev=""\n'
            'target=""\n'
            'is_list=0\n'
            'for arg in "$@"; do\n'
            '  if [ "$arg" = "--list" ]; then is_list=1; fi\n'
            '  if [ "$arg" = "--" ] && [ -z "$target" ]; then target="$prev"; fi\n'
            '  prev="$arg"\n'
            "done\n"
            'if [ "$is_list" = "1" ]; then\n'
            '  printf "%s: test\\n\\n1 test, 0 benchmarks\\n" "$target"\n'
            "else\n"
            '  printf "running 1 test\\ntest %s ... ok\\n\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\\n" "$target"\n'
            "fi\n",
            encoding="utf-8",
        )
        cargo_stub.chmod(0o755)

        run_env = dict(os.environ if env is None else env)
        if list_only:
            run_env.pop("CI", None)
            run_env.pop("GITHUB_ACTIONS", None)
            cmd = ["bash", str(script_path), "--list-only"]
        else:
            # Never synthesize CI or GITHUB_ACTIONS; caller must already hold real CI env.
            cmd = ["bash", str(script_path)]
        run_env["PATH"] = f"{tmp_path}{os.pathsep}{run_env.get('PATH', '')}"
        run_env["KOVEN_STUB_LOG"] = str(log_file)

        res = subprocess.run(
            cmd,
            cwd=root,
            env=run_env,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if res.returncode != 0:
            raise ShadowPilotError(
                "manifest_drift",
                f"{script_path.name} ({'list-only' if list_only else 'execute-dry-run'}) failed with exit {res.returncode}: {res.stderr.strip()}",
            )
        observed_lines = log_file.read_text(encoding="utf-8").splitlines() if log_file.is_file() else []
    return [line.split("\t") for line in observed_lines if line]


def verify_manifest_against_shell_script(
    manifest: dict[str, Any],
    root: Path = ROOT,
    *,
    env: dict[str, str] | None = None,
    observed_calls: list[list[str]] | None = None,
) -> dict[str, Any]:
    """Verify 1:1 ordering, package, target, features, test FQN, and BOTH --list and execution branches."""
    validate_manifest_structure(manifest)
    script_rel = manifest["source_script"]
    script_path = root / script_rel
    paired_blocks = verify_shell_script_execution_branch_static(script_path)

    effective_env = dict(os.environ if env is None else env)
    caller_supplied_calls = observed_calls is not None
    if observed_calls is None:
        observed_calls = capture_shell_script_list_calls(script_path, root, env=effective_env, list_only=True)

    expected_flat: list[tuple[str, str, list[str], list[str]]] = []
    for cfg in manifest["configs"]:
        for entry in cfg["entries"]:
            base_argv = [
                "test",
                "--locked",
                "-j1",
                "-p",
                cfg["package"],
                *cfg["cargo_target_args"],
                "--features",
                cfg["cargo_features_arg"],
                entry["test_name"],
                "--",
                *entry_filter_args(entry),
            ]
            expected_flat.append((cfg["config_id"], entry["test_name"], [*base_argv, "--list"], base_argv))

    if len(observed_calls) != len(expected_flat):
        raise ShadowPilotError(
            "manifest_drift",
            f"entry count mismatch: shell script issued {len(observed_calls)} list calls, manifest has {len(expected_flat)}",
        )

    for idx, (observed, (cid, tname, expected_list, _expected_run)) in enumerate(zip(observed_calls, expected_flat)):
        if observed != expected_list:
            raise ShadowPilotError(
                "manifest_drift",
                f"call #{idx} mismatch: shell={observed!r} vs manifest={expected_list!r}",
                config_id=cid,
                test_name=tname,
            )

    # In real GitHub Actions (without faking env vars), also dynamically dry-run the real execution branch
    dynamically_verified_exec = False
    if (
        not caller_supplied_calls
        and effective_env.get("CI") == "true"
        and effective_env.get("GITHUB_ACTIONS") == "true"
        and all(e["expected_outcome"] == "pass" for cfg in manifest["configs"] for e in cfg["entries"])
    ):
        exec_calls = capture_shell_script_list_calls(script_path, root, env=effective_env, list_only=False)
        expected_interleaved: list[tuple[str, str, list[str]]] = []
        for cid, tname, exp_list, exp_run in expected_flat:
            expected_interleaved.append((cid, tname, exp_list))
            expected_interleaved.append((cid, tname, exp_run))
        if len(exec_calls) != len(expected_interleaved):
            raise ShadowPilotError(
                "manifest_drift",
                f"execution-mode call count mismatch: shell issued {len(exec_calls)}, expected {len(expected_interleaved)}",
            )
        for idx, (obs, (cid, tname, exp)) in enumerate(zip(exec_calls, expected_interleaved)):
            if obs != exp:
                raise ShadowPilotError(
                    "manifest_drift",
                    f"execution-mode call #{idx} mismatch: shell={obs!r} vs manifest={exp!r}",
                    config_id=cid,
                    test_name=tname,
                )
        dynamically_verified_exec = True

    return {
        "verified_configs": len(manifest["configs"]),
        "verified_entries": len(expected_flat),
        "verified_paired_exec_blocks": paired_blocks,
        "dynamically_verified_exec_branch": dynamically_verified_exec,
        "source_script_sha256": sha256_file(script_path),
    }


def resolve_toolchain_commands(env: dict[str, str]) -> dict[str, Any]:
    prefix_raw = env.get("LLVM_SYS_211_PREFIX")
    llvm_config_path = Path(prefix_raw) / "bin/llvm-config" if prefix_raw else None
    llvm_clang_path = Path(prefix_raw) / "bin/clang" if prefix_raw else None

    def probe_version(binary: Path | str | None) -> tuple[str | None, str | None]:
        if binary is None:
            return None, None
        resolved = shutil.which(str(binary), path=env.get("PATH")) if not Path(binary).is_absolute() else (str(Path(binary)) if Path(binary).is_file() else None)
        if not resolved:
            return None, None
        try:
            proc = subprocess.run(
                [resolved, "--version"],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
        except OSError:
            return resolved, None
        first_line = proc.stdout.splitlines()[0].strip() if proc.stdout.strip() else ""
        return str(Path(resolved).resolve()), first_line

    llvm_cfg_resolved, llvm_cfg_ver = probe_version(llvm_config_path or "llvm-config")
    llvm_clang_resolved, llvm_clang_ver = probe_version(llvm_clang_path or "clang")
    host_clang_candidate = "/usr/bin/clang" if sys.platform == "darwin" else (llvm_clang_resolved or "/usr/bin/cc")
    host_c_resolved, host_c_ver = probe_version(host_clang_candidate)
    path_cc_resolved, path_cc_ver = probe_version("cc")

    return {
        "llvm_prefix": str(Path(prefix_raw).resolve()) if prefix_raw and Path(prefix_raw).exists() else prefix_raw,
        "llvm_config_path": llvm_cfg_resolved,
        "llvm_config_version": llvm_cfg_ver,
        "llvm_clang_path": llvm_clang_resolved,
        "llvm_clang_version": llvm_clang_ver,
        "host_linker_driver_path": host_c_resolved,
        "host_linker_driver_version": host_c_ver,
        "path_cc_path": path_cc_resolved,
        "path_cc_version": path_cc_ver,
    }


def inspect_resolved_llvm_dylibs(
    executable: Path,
    env: dict[str, str],
    *,
    cwd: Path | None = None,
) -> list[str]:
    """Inspect actual runtime dynamic linker bindings and file SHA256 for LLVM libraries."""
    if sys.platform == "darwin":
        tool = shutil.which("otool", path=env.get("PATH")) or "/usr/bin/otool"
        proc = subprocess.run([tool, "-L", str(executable)], env=env, capture_output=True, text=True, timeout=15, check=False)
        if proc.returncode != 0:
            raise ShadowPilotError(
                "environment_mismatch",
                f"otool -L failed ({proc.returncode}): {proc.stderr.strip()}",
            )
        declared_llvm = []
        for raw_line in proc.stdout.splitlines()[1:]:
            line = raw_line.strip()
            if "LLVM" in line or "llvm" in line:
                declared_llvm.append(line.split(" (", 1)[0].strip())

        # Invoke executable in harmless 0-hit --list mode with DYLD_PRINT_LIBRARIES=1 to capture
        # the exact library file path and Mach-O UUID actually loaded by macOS dyld at runtime.
        dyld_env = dict(env)
        dyld_env["DYLD_PRINT_LIBRARIES"] = "1"
        dyld_proc = subprocess.run(
            [str(executable), "--exact", "--list", "__koven_dyld_probe__"],
            cwd=cwd or executable.parent,
            env=dyld_env,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
        if dyld_proc.returncode != 0:
            raise ShadowPilotError(
                "environment_mismatch",
                f"macOS dyld runtime probe failed ({dyld_proc.returncode}): {dyld_proc.stderr[-400:].strip()}",
            )

        dyld_re = re.compile(r"^dyld\[\d+\]:\s+<([^>]+)>\s+(.+)$")
        loaded_llvm: list[str] = []
        for raw_err in dyld_proc.stderr.splitlines():
            m = dyld_re.match(raw_err.strip())
            if not m:
                continue
            uuid_str, loaded_raw = m.group(1), m.group(2).strip()
            bname = Path(loaded_raw).name
            if "LLVM" not in bname and "llvm" not in bname:
                continue
            if loaded_raw.startswith("/System/Library/") and not any(loaded_raw in d for d in declared_llvm):
                continue
            loaded_path = Path(loaded_raw)
            if loaded_path.is_file():
                real_p = loaded_path.resolve(strict=True)
                loaded_llvm.append(f"{real_p} [uuid={uuid_str}, sha256={sha256_file(real_p)}]")
            else:
                loaded_llvm.append(f"{loaded_raw} [uuid={uuid_str}]")

        if declared_llvm and not loaded_llvm:
            raise ShadowPilotError(
                "environment_mismatch",
                f"Mach-O declares LLVM dylibs {declared_llvm!r} but dyld reported none loaded at runtime",
            )
        return sorted(set(loaded_llvm))

    tool = shutil.which("ldd", path=env.get("PATH")) or "/usr/bin/ldd"
    proc = subprocess.run([tool, str(executable)], env=env, capture_output=True, text=True, timeout=15, check=False)
    if proc.returncode != 0:
        raise ShadowPilotError(
            "environment_mismatch",
            f"dynamic library inspection failed ({proc.returncode}): {proc.stderr.strip()}",
        )
    llvm_lines = []
    for raw_line in proc.stdout.splitlines():
        line = raw_line.strip()
        if "LLVM" in line or "llvm" in line:
            if "not found" in line:
                raise ShadowPilotError(
                    "environment_mismatch",
                    f"unresolved LLVM dynamic library in {executable}: {line}",
                )
            normalized = re.sub(r"\s+\(0x[0-9a-fA-F]+\)$", "", line)
            if "=>" in normalized:
                soname, rhs = [part.strip() for part in normalized.split("=>", 1)]
                rhs_path = Path(rhs)
                if rhs_path.is_file():
                    real_p = rhs_path.resolve(strict=True)
                    normalized = f"{soname} => {real_p} [sha256={sha256_file(real_p)}]"
            llvm_lines.append(normalized)
    return sorted(llvm_lines)


def build_probe_snapshot(
    *,
    executable: Path,
    cwd: Path,
    env: dict[str, str],
    inspect_dylibs: bool = True,
) -> dict[str, Any]:
    exe_resolved = executable.resolve(strict=True)
    cwd_resolved = cwd.resolve(strict=True)
    return {
        "executable": str(exe_resolved),
        "executable_sha256": sha256_file(exe_resolved),
        "cwd": str(cwd_resolved),
        "allowlisted_env": extract_allowlisted_env(env),
        "dylib_search_paths": {
            key: split_search_path(env.get(key)) for key in DYLIB_PATH_ENV_KEYS
        },
        "resolved_llvm_dylibs": inspect_resolved_llvm_dylibs(exe_resolved, env, cwd=cwd_resolved) if inspect_dylibs else [],
        "toolchain": resolve_toolchain_commands(env),
    }


def verify_probe_equivalence(
    cargo_probe: dict[str, Any],
    direct_probe: dict[str, Any],
    *,
    host: str,
    config_id: str,
) -> None:
    mismatches: list[str] = []
    for top_key in ("executable", "executable_sha256", "cwd", "resolved_llvm_dylibs"):
        if cargo_probe.get(top_key) != direct_probe.get(top_key):
            mismatches.append(top_key)

    cargo_env = cargo_probe.get("allowlisted_env", {})
    direct_env = direct_probe.get("allowlisted_env", {})
    for key in sorted(set(cargo_env) | set(direct_env)):
        if cargo_env.get(key) != direct_env.get(key):
            mismatches.append(f"allowlisted_env.{key}")

    cargo_dylib = cargo_probe.get("dylib_search_paths", {})
    direct_dylib = direct_probe.get("dylib_search_paths", {})
    for key in DYLIB_PATH_ENV_KEYS:
        if cargo_dylib.get(key) != direct_dylib.get(key):
            mismatches.append(f"dylib_search_paths.{key}")

    cargo_tc = cargo_probe.get("toolchain", {})
    direct_tc = direct_probe.get("toolchain", {})
    for key in sorted(set(cargo_tc) | set(direct_tc)):
        if cargo_tc.get(key) != direct_tc.get(key):
            mismatches.append(f"toolchain.{key}")

    if mismatches:
        raise ShadowPilotError(
            "environment_mismatch",
            f"Cargo launch probe and direct launch probe differ in: {', '.join(mismatches)}",
            host=host,
            config_id=config_id,
            evidence={"mismatched_fields": mismatches},
        )


def verify_binary_matches_host_triple(
    executable: Path,
    host_triple: str,
    *,
    host: str | None = None,
    config_id: str | None = None,
    allow_script_test_binary: bool = False,
) -> None:
    with executable.open("rb") as handle:
        header = handle.read(32)
    if allow_script_test_binary and header.startswith(b"#!"):
        return
    if "apple-darwin" in host_triple:
        # Mach-O 64-bit header: magic (MH_MAGIC_64=0xfeedfacf), cputype, cpusubtype, filetype (MH_EXECUTE=2)
        if len(header) < 32:
            raise ShadowPilotError(
                "artifact_lock_failed",
                f"executable {executable} is shorter than 32-byte Mach-O 64 header",
                host=host,
                config_id=config_id,
            )
        magic, cputype, _cpusubtype, filetype, *_ = struct.unpack("<IIIIIIII", header[:32])
        expected_cpu = (
            0x0100000C
            if host_triple.startswith("aarch64-")
            else (0x01000007 if host_triple.startswith("x86_64-") else None)
        )
        if expected_cpu is None or magic != 0xFEEDFACF or filetype != 2 or cputype != expected_cpu:
            raise ShadowPilotError(
                "artifact_lock_failed",
                f"Mach-O header mismatch for {host_triple}: magic={hex(magic)}, cputype={hex(cputype)}, filetype={filetype}",
                host=host,
                config_id=config_id,
            )
    elif "linux" in host_triple:
        # ELF64 little-endian header: magic \x7fELF, EI_CLASS=2 (64-bit), EI_DATA=1 (LSB), e_type in {ET_EXEC=2, ET_DYN=3}, e_machine
        if len(header) < 20 or header[:4] != b"\x7fELF" or header[4] != 2 or header[5] != 1:
            raise ShadowPilotError(
                "artifact_lock_failed",
                f"executable {executable} is not a 64-bit little-endian ELF binary for {host_triple}",
                host=host,
                config_id=config_id,
            )
        e_type, e_machine = struct.unpack("<HH", header[16:20])
        expected_machine = (
            62
            if host_triple.startswith("x86_64-")
            else (183 if host_triple.startswith("aarch64-") else None)
        )
        if expected_machine is None or e_type not in (2, 3) or e_machine != expected_machine:
            raise ShadowPilotError(
                "artifact_lock_failed",
                f"ELF header mismatch for {host_triple}: e_type={e_type}, e_machine={e_machine} (expected {expected_machine})",
                host=host,
                config_id=config_id,
            )
    else:
        raise ShadowPilotError(
            "artifact_lock_failed",
            f"unsupported host triple: {host_triple}",
            host=host,
            config_id=config_id,
        )


def extract_cargo_compiler_diagnostics(stdout: str, max_chars: int = 8000) -> str:
    """Extract rendered rustc/Cargo compiler-message diagnostics from --message-format=json stdout."""
    errors: list[str] = []
    others: list[str] = []
    for raw in stdout.splitlines():
        stripped = raw.strip()
        if not stripped or not stripped.startswith("{"):
            continue
        try:
            rec = json.loads(stripped)
        except json.JSONDecodeError:
            continue
        if rec.get("reason") != "compiler-message":
            continue
        msg = rec.get("message") or {}
        rendered = msg.get("rendered")
        if not isinstance(rendered, str) or not rendered.strip():
            continue
        level = msg.get("level")
        if level in ("error", "failure-note", "ice"):
            errors.append(rendered.rstrip())
        else:
            others.append(rendered.rstrip())
    ordered = errors if errors else others
    joined = "\n".join(ordered)
    if len(joined) > max_chars:
        return joined[-max_chars:]
    return joined


def select_unique_test_artifact(
    json_lines: list[str],
    expected: dict[str, Any],
    *,
    allow_script_test_binary: bool = False,
) -> dict[str, Any]:
    """Lock the unique Cargo test executable matching package, target, features, and profile."""
    host = expected.get("host")
    cid = expected.get("config_id")
    records: list[dict[str, Any]] = []
    for raw in json_lines:
        stripped = raw.strip()
        if not stripped or not stripped.startswith("{"):
            continue
        try:
            records.append(json.loads(stripped))
        except json.JSONDecodeError as exc:
            raise ShadowPilotError(
                "artifact_lock_failed",
                f"malformed Cargo JSON message: {exc}",
                host=host,
                config_id=cid,
            ) from exc

    finished = [r for r in records if r.get("reason") == "build-finished"]
    if len(finished) != 1 or finished[0].get("success") is not True:
        raise ShadowPilotError(
            "artifact_lock_failed",
            f"missing or unsuccessful Cargo build-finished record: {finished!r}",
            host=host,
            config_id=cid,
        )

    expected_manifest = str(Path(expected["manifest_path"]).resolve())
    expected_features = sorted(expected["expected_features"])
    expected_debuginfo = expected.get("expected_debuginfo")

    candidates: list[dict[str, Any]] = []
    package_test_artifacts: list[dict[str, Any]] = []

    for rec in records:
        if rec.get("reason") != "compiler-artifact":
            continue
        profile = rec.get("profile") or {}
        if profile.get("test") is True and rec.get("package_id") == expected["package_id"]:
            package_test_artifacts.append(rec)

        rec_manifest = rec.get("manifest_path")
        rec_manifest_resolved = str(Path(rec_manifest).resolve()) if rec_manifest else ""
        target = rec.get("target") or {}
        if (
            rec.get("package_id") == expected["package_id"]
            and rec_manifest_resolved == expected_manifest
            and target.get("name") == expected["target_name"]
            and target.get("kind") == expected["target_kind"]
            and profile.get("test") is True
        ):
            candidates.append(rec)

    if len(candidates) != 1:
        raise ShadowPilotError(
            "artifact_lock_failed",
            f"expected exactly 1 matching compiler-artifact for {expected['target_name']} {expected['target_kind']}, got {len(candidates)} (package test artifacts={len(package_test_artifacts)})",
            host=host,
            config_id=cid,
        )

    chosen = candidates[0]
    actual_features = sorted(chosen.get("features") or [])
    if actual_features != expected_features:
        raise ShadowPilotError(
            "artifact_lock_failed",
            f"resolved features mismatch: expected {expected_features}, got {actual_features}",
            host=host,
            config_id=cid,
        )

    profile = chosen.get("profile") or {}
    if expected_debuginfo is not None and profile.get("debuginfo") != expected_debuginfo:
        raise ShadowPilotError(
            "artifact_lock_failed",
            f"profile.debuginfo mismatch: expected {expected_debuginfo}, got {profile.get('debuginfo')}",
            host=host,
            config_id=cid,
        )

    exe_raw = chosen.get("executable")
    if not isinstance(exe_raw, str) or not exe_raw.strip():
        raise ShadowPilotError(
            "artifact_lock_failed",
            "matched compiler-artifact has empty/null executable",
            host=host,
            config_id=cid,
        )

    exe_path = Path(exe_raw)
    if not exe_path.is_file() or exe_path.is_symlink() or not os.access(exe_path, os.X_OK):
        raise ShadowPilotError(
            "artifact_lock_failed",
            f"matched executable is missing, symlink, or non-executable: {exe_path}",
            host=host,
            config_id=cid,
        )

    exe_resolved = exe_path.resolve(strict=True)
    if "target_dir" in expected:
        target_root = Path(expected["target_dir"]).resolve()
        if not exe_resolved.is_relative_to(target_root):
            raise ShadowPilotError(
                "artifact_lock_failed",
                f"executable {exe_resolved} is outside isolated CARGO_TARGET_DIR {target_root}",
                host=host,
                config_id=cid,
            )

    verify_binary_matches_host_triple(
        exe_resolved,
        expected["host_triple"],
        host=host,
        config_id=cid,
        allow_script_test_binary=allow_script_test_binary,
    )

    return {
        "package_id": chosen["package_id"],
        "manifest_path": expected_manifest,
        "target_name": chosen["target"]["name"],
        "target_kind": chosen["target"]["kind"],
        "features": actual_features,
        "profile": {
            "test": profile.get("test"),
            "debuginfo": profile.get("debuginfo"),
            "opt_level": profile.get("opt_level"),
        },
        "fresh": bool(chosen.get("fresh")),
        "executable": str(exe_resolved),
        "executable_sha256": sha256_file(exe_resolved),
    }


def parse_libtest_list_exact(
    stdout: str,
    expected_test: str,
    *,
    exit_code: int,
    host: str,
    config_id: str,
) -> int:
    if exit_code != 0:
        raise ShadowPilotError(
            "list_mismatch",
            f"--list exited with non-zero code {exit_code}",
            host=host,
            config_id=config_id,
            test_name=expected_test,
        )
    names = re.findall(r"^(\S+): test$", stdout, re.MULTILINE)
    totals = re.findall(r"^(\d+) tests?, (\d+) benchmarks?$", stdout, re.MULTILINE)
    if len(totals) != 1 or int(totals[0][1]) != 0 or int(totals[0][0]) != len(names):
        raise ShadowPilotError(
            "list_mismatch",
            f"malformed libtest --list summary: totals={totals!r}, names={names!r}",
            host=host,
            config_id=config_id,
            test_name=expected_test,
        )
    if names != [expected_test]:
        raise ShadowPilotError(
            "list_mismatch",
            f"expected exact 1 list hit [{expected_test!r}], got {names!r}",
            host=host,
            config_id=config_id,
            test_name=expected_test,
        )
    return 1


def parse_and_classify_execution(
    stdout: str,
    stderr: str,
    exit_code: int,
    entry: dict[str, Any],
    *,
    host: str,
    config_id: str,
) -> dict[str, Any]:
    """Verify exact 1 test execution and classify pass vs expected_failure."""
    tname = entry["test_name"]
    running = re.findall(r"^running (\d+) tests?$", stdout, re.MULTILINE)
    summaries = re.findall(
        r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;",
        stdout,
        re.MULTILINE,
    )
    def _diag_evidence() -> dict[str, str]:
        ev: dict[str, str] = {}
        if stdout.strip():
            ev["stdout_tail"] = stdout[-8000:]
        if stderr.strip():
            ev["stderr_tail"] = stderr[-4000:]
        return ev

    if running != ["1"] or len(summaries) != 1:
        raise ShadowPilotError(
            "missed_execution",
            f"expected running=['1'] and 1 libtest summary, got running={running!r}, summaries={summaries!r}, exit={exit_code}",
            host=host,
            config_id=config_id,
            test_name=tname,
            evidence=_diag_evidence(),
        )

    verdict, passed_s, failed_s, ignored_s, measured_s, filtered_s = summaries[0]
    passed, failed, ignored, measured, filtered = (
        int(passed_s),
        int(failed_s),
        int(ignored_s),
        int(measured_s),
        int(filtered_s),
    )
    if ignored != 0 or measured != 0 or (passed + failed) != 1:
        raise ShadowPilotError(
            "missed_execution",
            f"test did not execute exactly once (passed={passed}, failed={failed}, ignored={ignored}, measured={measured}, exit={exit_code})",
            host=host,
            config_id=config_id,
            test_name=tname,
            evidence=_diag_evidence(),
        )

    expected_line_status = "ok" if passed == 1 else "FAILED"
    status_hits = re.findall(
        rf"^test {re.escape(tname)} \.\.\. (ok|FAILED)$", stdout, re.MULTILINE
    )
    if status_hits != [expected_line_status]:
        raise ShadowPilotError(
            "missed_execution",
            f"missing exact per-test status line 'test {tname} ... {expected_line_status}', got {status_hits!r}",
            host=host,
            config_id=config_id,
            test_name=tname,
            evidence=_diag_evidence(),
        )

    expected_outcome = entry["expected_outcome"]
    combined_output = f"{stdout}\n{stderr}"

    if expected_outcome == "pass":
        if exit_code != 0 or verdict != "ok" or passed != 1 or failed != 0:
            raise ShadowPilotError(
                "unexpected_failure",
                f"expected passing test failed (exit={exit_code}, verdict={verdict})",
                host=host,
                config_id=config_id,
                test_name=tname,
                evidence=_diag_evidence(),
            )
        return {
            "executed_count": 1,
            "passed": 1,
            "failed": 0,
            "ignored": 0,
            "filtered_out": filtered,
            "verdict": verdict,
            "classification": "pass",
            "matched_failure_witnesses": [],
        }

    # expected_outcome == "expected_failure"
    if exit_code == 0 or verdict != "FAILED" or failed != 1 or passed != 0:
        raise ShadowPilotError(
            "unexpected_pass",
            f"expected-failure test unexpectedly passed (exit={exit_code}, verdict={verdict})",
            host=host,
            config_id=config_id,
            test_name=tname,
            evidence=_diag_evidence(),
        )
    missing_witnesses = [
        sub for sub in entry["expected_failure_substrings"] if sub not in combined_output
    ]
    if missing_witnesses:
        raise ShadowPilotError(
            "wrong_failure_evidence",
            f"expected-failure test lacked required failure evidence: {missing_witnesses!r}",
            host=host,
            config_id=config_id,
            test_name=tname,
            evidence=_diag_evidence(),
        )
    return {
        "executed_count": 1,
        "passed": 0,
        "failed": 1,
        "ignored": 0,
        "filtered_out": filtered,
        "verdict": verdict,
        "classification": "expected_failure_verified",
        "matched_failure_witnesses": list(entry["expected_failure_substrings"]),
    }


def rusage_maxrss_bytes(ru_maxrss: int) -> int:
    # macOS getrusage/wait4 reports bytes; Linux reports KiB.
    return int(ru_maxrss) if sys.platform == "darwin" else int(ru_maxrss) * 1024


def run_monitored_process(
    cmd: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    timeout_seconds: float = 1800.0,
) -> dict[str, Any]:
    """Run a child process in its own session, capturing wall-clock, wait4 peak RSS, and OOM hints."""
    with tempfile.TemporaryDirectory(prefix="koven-e2-proc-") as tmp:
        tmp_path = Path(tmp)
        out_path = tmp_path / "stdout.txt"
        err_path = tmp_path / "stderr.txt"
        timed_out = False

        started = time.monotonic()
        with out_path.open("wb") as out_f, err_path.open("wb") as err_f:
            proc = subprocess.Popen(
                cmd,
                cwd=cwd,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=out_f,
                stderr=err_f,
                start_new_session=True,
            )

            def _kill_on_timeout() -> None:
                nonlocal timed_out
                timed_out = True
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

            timer = threading.Timer(timeout_seconds, _kill_on_timeout)
            timer.start()
            try:
                _, status, rusage = os.wait4(proc.pid, 0)
                proc.returncode = os.waitstatus_to_exitcode(status)
            finally:
                timer.cancel()
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass

        elapsed = time.monotonic() - started
        stdout_text = out_path.read_bytes().decode("utf-8", errors="replace")
        stderr_text = err_path.read_bytes().decode("utf-8", errors="replace")
        exit_code = int(proc.returncode)
        sig = -exit_code if exit_code < 0 else None
        oom_suspected = (
            not timed_out
            and (
                sig == signal.SIGKILL
                or exit_code == 137
                or "signal: 9, SIGKILL" in stderr_text
                or "memory allocation of" in stderr_text
            )
        )
        return {
            "exit_code": exit_code,
            "signal": sig,
            "timed_out": timed_out,
            "oom_suspected": oom_suspected,
            "elapsed_seconds": elapsed,
            "peak_rss_bytes": rusage_maxrss_bytes(rusage.ru_maxrss),
            "stdout": stdout_text,
            "stderr": stderr_text,
        }


def directory_bytes(path: Path) -> int:
    if not path.exists():
        return 0
    total = 0
    for root_dir, _, files in os.walk(path):
        for fname in files:
            fpath = Path(root_dir) / fname
            try:
                if not fpath.is_symlink():
                    total += fpath.stat().st_size
            except FileNotFoundError:
                continue
    return total


def disk_snapshot(workspace: Path, target_dir: Path) -> dict[str, int]:
    usage = shutil.disk_usage(workspace)
    return {
        "fs_total_bytes": usage.total,
        "fs_used_bytes": usage.used,
        "fs_free_bytes": usage.free,
        "target_dir_bytes": directory_bytes(target_dir),
    }


def query_rustc_and_metadata(root: Path, env: dict[str, str], host_label: str) -> tuple[str, str, dict[str, dict[str, Any]]]:
    rustc_res = subprocess.run(
        ["rustc", "-vV"], cwd=root, env=env, capture_output=True, text=True, timeout=30, check=False
    )
    if rustc_res.returncode != 0:
        raise ShadowPilotError("toolchain_invalid", f"rustc -vV failed: {rustc_res.stderr}", host=host_label)
    hosts = re.findall(r"^host: (\S+)$", rustc_res.stdout, re.MULTILINE)
    releases = re.findall(r"^release: (\S+)$", rustc_res.stdout, re.MULTILINE)
    if len(hosts) != 1 or len(releases) != 1:
        raise ShadowPilotError("toolchain_invalid", f"unrecognized rustc -vV output: {rustc_res.stdout!r}", host=host_label)

    meta_res = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        cwd=root,
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if meta_res.returncode != 0:
        raise ShadowPilotError("toolchain_invalid", f"cargo metadata failed: {meta_res.stderr}", host=host_label)
    meta = json.loads(meta_res.stdout)
    packages = {pkg["name"]: pkg for pkg in meta.get("packages", [])}
    return hosts[0], releases[0], packages


def detect_git_sha(root: Path, env: dict[str, str]) -> str:
    res = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=root, env=env, capture_output=True, text=True, timeout=15, check=False
    )
    return res.stdout.strip() if res.returncode == 0 else "unknown"


def detect_git_tree(root: Path, env: dict[str, str]) -> str:
    res = subprocess.run(
        ["git", "rev-parse", "HEAD^{tree}"], cwd=root, env=env, capture_output=True, text=True, timeout=15, check=False
    )
    return res.stdout.strip() if res.returncode == 0 else "unknown"


def has_compiled_target_artifacts(target_dir: Path) -> bool:
    deps_dir = target_dir / "debug" / "deps"
    if not deps_dir.is_dir():
        return False
    for child in deps_dir.iterdir():
        if child.is_file() and (
            child.suffix in (".rlib", ".rmeta", ".so", ".dylib") or os.access(child, os.X_OK)
        ):
            return True
    return False


def detect_host_resources() -> dict[str, int]:
    cpus = os.cpu_count() or 1
    ram_bytes = 0
    try:
        if sys.platform == "darwin":
            out = subprocess.run(
                ["/usr/sbin/sysctl", "-n", "hw.memsize"],
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
            )
            if out.returncode == 0 and out.stdout.strip().isdigit():
                ram_bytes = int(out.stdout.strip())
        elif hasattr(os, "sysconf"):
            ram_bytes = int(os.sysconf("SC_PAGE_SIZE")) * int(os.sysconf("SC_PHYS_PAGES"))
    except (OSError, ValueError):
        ram_bytes = 0
    return {"cpu_count": cpus, "total_ram_bytes": ram_bytes}


def resolve_candidate_jobs_args(candidate_jobs: str) -> tuple[list[str], str]:
    if candidate_jobs in ("manifest", "1"):
        return ["-j1"], "-j1"
    if candidate_jobs == "default":
        return [], "default"
    if candidate_jobs.isdigit() and int(candidate_jobs) > 0:
        flag = f"-j{int(candidate_jobs)}"
        return [flag], flag
    raise ShadowPilotError("invalid_argument", f"unsupported candidate_jobs={candidate_jobs!r}")


def purge_artifacts_dir(artifacts_dir: Path) -> Path:
    resolved = artifacts_dir.resolve()
    resolved.mkdir(parents=True, exist_ok=True)
    (resolved / "ledger.json").unlink(missing_ok=True)
    (resolved / "failure.json").unlink(missing_ok=True)
    shutil.rmtree(resolved / "probes", ignore_errors=True)
    return resolved


def run_pilot(
    *,
    manifest: dict[str, Any],
    root: Path = ROOT,
    mode: str,
    list_only: bool,
    artifacts_dir: Path,
    env: dict[str, str] | None = None,
    verify_shell_parity: bool = True,
    allow_script_test_binary: bool = False,
    inspect_dylibs: bool = True,
    candidate_jobs: str = "manifest",
    formal_gate: bool = False,
) -> dict[str, Any]:
    """Execute either 'baseline' (Cargo calls) or 'candidate' (builds + probes + direct calls)."""
    artifacts_dir = purge_artifacts_dir(artifacts_dir)

    effective_env = dict(os.environ if env is None else env)
    host_label = effective_env.get("RUNNER_OS") or platform.system()
    git_sha = "unknown"
    git_tree = "unknown"
    host_triple = "unknown"
    manifest_sha256 = "unknown"
    config_records: list[dict[str, Any]] = []
    entry_records: list[dict[str, Any]] = []

    try:
        if mode not in ("baseline", "candidate"):
            raise ShadowPilotError("invalid_argument", f"unsupported mode: {mode!r}", host=host_label)

        if formal_gate and list_only:
            raise ShadowPilotError(
                "invalid_argument",
                "--formal-gate cannot be combined with --list-only",
                host=host_label,
            )

        if not list_only:
            if effective_env.get("CI") != "true" or effective_env.get("GITHUB_ACTIONS") != "true":
                raise ShadowPilotError(
                    "ci_required",
                    "E2 fault execution requires real CI=true and GITHUB_ACTIONS=true; use --list-only locally",
                    host=host_label,
                )

        validate_manifest_structure(manifest)
        if formal_gate:
            for cfg in manifest["configs"]:
                for entry in cfg["entries"]:
                    if entry.get("expected_outcome") != "pass":
                        raise ShadowPilotError(
                            "manifest_invalid",
                            f"--formal-gate requires every manifest entry to have expected_outcome='pass', got {entry.get('expected_outcome')!r}",
                            host=host_label,
                            config_id=cfg["config_id"],
                            test_name=entry["test_name"],
                        )
        manifest_canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode("utf-8")
        manifest_sha256 = sha256_bytes(manifest_canonical)
        parity_report = (
            verify_manifest_against_shell_script(manifest, root, env=effective_env)
            if verify_shell_parity
            else {"verified_configs": len(manifest["configs"]), "verified_entries": manifest["total_entries"]}
        )
        if formal_gate and parity_report.get("dynamically_verified_exec_branch") is not True:
            raise ShadowPilotError(
                "manifest_drift",
                "--formal-gate requires live dynamic execution-branch verification against the formal shell script",
                host=host_label,
            )

        target_dir = Path(effective_env.get("CARGO_TARGET_DIR") or (root / "target")).resolve()
        effective_env["CARGO_TARGET_DIR"] = str(target_dir)

        initial_disk = disk_snapshot(root, target_dir)
        cache_phase = effective_env.get("KOVEN_SHADOW_CACHE_PHASE", "unspecified")
        cache_hit = effective_env.get("KOVEN_SHADOW_CACHE_HIT", "unspecified")
        if cache_phase == "cold":
            if cache_hit == "true" or initial_disk["target_dir_bytes"] != 0:
                raise ShadowPilotError(
                    "cache_contract_violated",
                    f"cold phase requires cache_hit!=true and empty CARGO_TARGET_DIR, got hit={cache_hit!r}, bytes={initial_disk['target_dir_bytes']}",
                    host=host_label,
                )
        elif cache_phase == "warm" or cache_hit == "true":
            if initial_disk["target_dir_bytes"] <= 0 or not has_compiled_target_artifacts(target_dir):
                raise ShadowPilotError(
                    "cache_contract_violated",
                    f"warm / cache-hit run requires compiled artifacts in {target_dir / 'debug/deps'} (registry-only cache hit cannot be marked warm)",
                    host=host_label,
                )

        host_triple, rustc_release, packages = query_rustc_and_metadata(root, effective_env, host_label)
        git_sha = detect_git_sha(root, effective_env)
        git_tree = detect_git_tree(root, effective_env)

        raw_test_debug = effective_env.get("CARGO_PROFILE_TEST_DEBUG")
        expected_debuginfo = int(raw_test_debug) if raw_test_debug in ("0", "1", "2") else None

        if mode == "baseline":
            cargo_jobs_args, cargo_jobs_label = ["-j1"], "-j1"
        else:
            cargo_jobs_args, cargo_jobs_label = resolve_candidate_jobs_args(candidate_jobs)

        host_resources = detect_host_resources()

        build_flags_snapshot = {
            key: effective_env.get(key)
            for key in (
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_INCREMENTAL",
                "CARGO_PROFILE_DEV_DEBUG",
                "CARGO_PROFILE_TEST_DEBUG",
                "LLVM_SYS_211_PREFIX",
            )
        }
        build_flags_snapshot["cargo_jobs"] = cargo_jobs_label

        min_free_bytes = initial_disk["fs_free_bytes"]
        max_target_bytes = initial_disk["target_dir_bytes"]
        peak_rss_overall = 0
        oom_events = 0

        total_build_seconds = 0.0
        total_probe_seconds = 0.0
        total_list_seconds = 0.0
        total_run_seconds = 0.0
        pilot_started = time.monotonic()

        locked_by_target: dict[tuple[str, str, tuple[str, ...]], list[dict[str, Any]]] = {}
        suite_id = manifest.get("suite_id") or Path(manifest["source_script"]).stem
        formal_ci_step_name = manifest.get("formal_ci_step_name")
        for cfg in manifest["configs"]:
            cid = cfg["config_id"]
            pkg_name = cfg["package"]
            if pkg_name not in packages:
                raise ShadowPilotError("manifest_invalid", f"unknown workspace package {pkg_name}", host=host_label, config_id=cid)
            pkg_meta = packages[pkg_name]
            pkg_id = pkg_meta["id"]
            pkg_manifest_path = str(Path(pkg_meta["manifest_path"]).resolve())

            cfg_build_seconds = 0.0
            cfg_probe_seconds = 0.0
            cfg_list_seconds = 0.0
            cfg_run_seconds = 0.0
            cfg_peak_rss = 0
            locked_artifact: dict[str, Any] | None = None
            direct_cwd: Path | None = None
            direct_env: dict[str, str] | None = None

            if mode == "candidate":
                # 1. Build once with --no-run --message-format=json
                build_cmd = [
                    "cargo",
                    "test",
                    "--locked",
                    *cargo_jobs_args,
                    "-p",
                    pkg_name,
                    *cfg["cargo_target_args"],
                    "--features",
                    cfg["cargo_features_arg"],
                    "--no-run",
                    "--message-format=json",
                ]
                build_proc = run_monitored_process(build_cmd, cwd=root, env=effective_env, timeout_seconds=5400.0)
                cfg_build_seconds += build_proc["elapsed_seconds"]
                cfg_peak_rss = max(cfg_peak_rss, build_proc["peak_rss_bytes"])
                oom_events += int(build_proc["oom_suspected"])
                if build_proc["timed_out"] or build_proc["exit_code"] != 0:
                    compiler_diag = extract_cargo_compiler_diagnostics(build_proc["stdout"])
                    build_ev: dict[str, str] = {}
                    if compiler_diag:
                        build_ev["compiler_diagnostics"] = compiler_diag
                    if build_proc["stderr"].strip():
                        build_ev["stderr_tail"] = build_proc["stderr"][-4000:]
                    summary_tail = compiler_diag[-1200:] if compiler_diag else build_proc["stderr"][-1200:]
                    raise ShadowPilotError(
                        "artifact_lock_failed",
                        f"candidate build failed (exit={build_proc['exit_code']}, timed_out={build_proc['timed_out']}): {summary_tail}",
                        host=host_label,
                        config_id=cid,
                        evidence=build_ev,
                    )

                locked_artifact = select_unique_test_artifact(
                    build_proc["stdout"].splitlines(),
                    {
                        "host": host_label,
                        "config_id": cid,
                        "host_triple": host_triple,
                        "package_id": pkg_id,
                        "manifest_path": pkg_manifest_path,
                        "target_name": cfg["target_name"],
                        "target_kind": cfg["target_kind"],
                        "expected_features": cfg["expected_features"],
                        "expected_debuginfo": expected_debuginfo,
                        "target_dir": str(target_dir),
                    },
                    allow_script_test_binary=allow_script_test_binary,
                )
                target_bucket_key = (pkg_id, cfg["target_name"], tuple(cfg["target_kind"]))
                for prev_lock in locked_by_target.get(target_bucket_key, []):
                    if prev_lock["features"] != locked_artifact["features"] and (
                        prev_lock["executable"] == locked_artifact["executable"]
                        or prev_lock["executable_sha256"] == locked_artifact["executable_sha256"]
                    ):
                        raise ShadowPilotError(
                            "artifact_lock_failed",
                            f"distinct feature config {cid} reused executable or digest from {prev_lock['config_id']}: {locked_artifact['executable']}",
                            host=host_label,
                            config_id=cid,
                        )
                locked_by_target.setdefault(target_bucket_key, []).append(
                    {
                        "config_id": cid,
                        "features": locked_artifact["features"],
                        "executable": locked_artifact["executable"],
                        "executable_sha256": locked_artifact["executable_sha256"],
                    }
                )

                # 2. Real Cargo launch probe vs Direct launch probe
                first_entry = cfg["entries"][0]
                probe_dir = artifacts_dir / "probes" / cid
                probe_dir.mkdir(parents=True, exist_ok=True)
                cargo_probe_file = probe_dir / "cargo-probe.json"
                direct_probe_file = probe_dir / "direct-probe.json"

                runner_spec = json.dumps(
                    [
                        sys.executable,
                        str(Path(__file__).resolve()),
                        "internal-probe",
                        "--output",
                        str(cargo_probe_file),
                    ]
                    + ([] if inspect_dylibs else ["--no-inspect-dylibs"])
                )
                probe_cargo_cmd = [
                    "cargo",
                    "test",
                    "--locked",
                    *cargo_jobs_args,
                    "--config",
                    f"target.{host_triple}.runner={runner_spec}",
                    "-p",
                    pkg_name,
                    *cfg["cargo_target_args"],
                    "--features",
                    cfg["cargo_features_arg"],
                    first_entry["test_name"],
                    "--",
                    *entry_filter_args(first_entry),
                    "--list",
                ]
                c_probe_proc = run_monitored_process(probe_cargo_cmd, cwd=root, env=effective_env, timeout_seconds=600.0)
                cfg_probe_seconds += c_probe_proc["elapsed_seconds"]
                cfg_peak_rss = max(cfg_peak_rss, c_probe_proc["peak_rss_bytes"])
                oom_events += int(c_probe_proc["oom_suspected"])
                if c_probe_proc["exit_code"] != 0 or not cargo_probe_file.is_file():
                    raise ShadowPilotError(
                        "environment_mismatch",
                        f"Cargo launch probe failed (exit={c_probe_proc['exit_code']}): {c_probe_proc['stderr'][-500:]}",
                        host=host_label,
                        config_id=cid,
                    )
                cargo_probe = json.loads(cargo_probe_file.read_text(encoding="utf-8"))
                if (
                    cargo_probe.get("executable") != locked_artifact["executable"]
                    or cargo_probe.get("executable_sha256") != locked_artifact["executable_sha256"]
                ):
                    raise ShadowPilotError(
                        "artifact_lock_failed",
                        "Cargo launch probe executed a different binary than Cargo JSON locked",
                        host=host_label,
                        config_id=cid,
                    )

                # Replicate allowlisted runtime env & cwd for direct launches
                direct_cwd = Path(pkg_manifest_path).parent.resolve()
                direct_env = dict(effective_env)
                for key, val in cargo_probe.get("allowlisted_env", {}).items():
                    if is_safe_allowlisted_key(key):
                        direct_env[key] = val
                for dylib_key, parts in cargo_probe.get("dylib_search_paths", {}).items():
                    if dylib_key in DYLIB_PATH_ENV_KEYS and parts:
                        direct_env[dylib_key] = os.pathsep.join(parts)

                direct_probe_cmd = (
                    [
                        sys.executable,
                        str(Path(__file__).resolve()),
                        "internal-probe",
                        "--output",
                        str(direct_probe_file),
                    ]
                    + ([] if inspect_dylibs else ["--no-inspect-dylibs"])
                    + [
                        locked_artifact["executable"],
                        first_entry["test_name"],
                        *entry_filter_args(first_entry),
                        "--list",
                    ]
                )
                d_probe_proc = run_monitored_process(direct_probe_cmd, cwd=direct_cwd, env=direct_env, timeout_seconds=120.0)
                cfg_probe_seconds += d_probe_proc["elapsed_seconds"]
                cfg_peak_rss = max(cfg_peak_rss, d_probe_proc["peak_rss_bytes"])
                if d_probe_proc["exit_code"] != 0 or not direct_probe_file.is_file():
                    raise ShadowPilotError(
                        "environment_mismatch",
                        f"direct launch probe failed (exit={d_probe_proc['exit_code']}): {d_probe_proc['stderr'][-500:]}",
                        host=host_label,
                        config_id=cid,
                    )
                direct_probe = json.loads(direct_probe_file.read_text(encoding="utf-8"))
                verify_probe_equivalence(cargo_probe, direct_probe, host=host_label, config_id=cid)

            # 3. Execute entries sequentially in exact manifest order
            for local_idx, entry in enumerate(cfg["entries"]):
                tname = entry["test_name"]
                f_args = entry_filter_args(entry)

                if mode == "baseline":
                    list_cmd = [
                        "cargo",
                        "test",
                        "--locked",
                        "-j1",
                        "-p",
                        pkg_name,
                        *cfg["cargo_target_args"],
                        "--features",
                        cfg["cargo_features_arg"],
                        tname,
                        "--",
                        *f_args,
                        "--list",
                    ]
                    list_cwd = root
                    list_env = effective_env
                else:
                    assert locked_artifact is not None and direct_cwd is not None and direct_env is not None
                    list_cmd = [locked_artifact["executable"], tname, *f_args, "--list"]
                    list_cwd = direct_cwd
                    list_env = direct_env

                list_timeout = 5400.0 if (mode == "baseline" and local_idx == 0) else 300.0
                list_proc = run_monitored_process(list_cmd, cwd=list_cwd, env=list_env, timeout_seconds=list_timeout)
                cfg_list_seconds += list_proc["elapsed_seconds"]
                cfg_peak_rss = max(cfg_peak_rss, list_proc["peak_rss_bytes"])
                oom_events += int(list_proc["oom_suspected"])

                list_hits = parse_libtest_list_exact(
                    list_proc["stdout"],
                    tname,
                    exit_code=list_proc["exit_code"],
                    host=host_label,
                    config_id=cid,
                )

                rec_entry: dict[str, Any] = {
                    "global_index": entry["global_index"],
                    "config_id": cid,
                    "test_name": tname,
                    "ignored_filter": entry["ignored"],
                    "expected_outcome": entry["expected_outcome"],
                    "list_hits": list_hits,
                    "list_exit_code": list_proc["exit_code"],
                    "list_seconds": round(list_proc["elapsed_seconds"], 6),
                    "executed": not list_only,
                }

                if not list_only:
                    if mode == "baseline":
                        run_cmd = [
                            "cargo",
                            "test",
                            "--locked",
                            "-j1",
                            "-p",
                            pkg_name,
                            *cfg["cargo_target_args"],
                            "--features",
                            cfg["cargo_features_arg"],
                            tname,
                            "--",
                            *f_args,
                        ]
                        run_cwd = root
                        run_env = effective_env
                    else:
                        assert locked_artifact is not None and direct_cwd is not None and direct_env is not None
                        run_cmd = [locked_artifact["executable"], tname, *f_args]
                        run_cwd = direct_cwd
                        run_env = direct_env

                    run_proc = run_monitored_process(run_cmd, cwd=run_cwd, env=run_env, timeout_seconds=300.0)
                    cfg_run_seconds += run_proc["elapsed_seconds"]
                    cfg_peak_rss = max(cfg_peak_rss, run_proc["peak_rss_bytes"])
                    oom_events += int(run_proc["oom_suspected"])

                    classified = parse_and_classify_execution(
                        run_proc["stdout"],
                        run_proc["stderr"],
                        run_proc["exit_code"],
                        entry,
                        host=host_label,
                        config_id=cid,
                    )
                    rec_entry.update(
                        exit_code=run_proc["exit_code"],
                        signal=run_proc["signal"],
                        oom_suspected=run_proc["oom_suspected"],
                        run_seconds=round(run_proc["elapsed_seconds"], 6),
                        peak_rss_bytes=max(list_proc["peak_rss_bytes"], run_proc["peak_rss_bytes"]),
                        **classified,
                    )
                else:
                    rec_entry.update(
                        exit_code=None,
                        signal=None,
                        oom_suspected=list_proc["oom_suspected"],
                        run_seconds=0.0,
                        peak_rss_bytes=list_proc["peak_rss_bytes"],
                        executed_count=0,
                        classification="listed_only",
                        matched_failure_witnesses=[],
                    )

                entry_records.append(rec_entry)

            # Confirm locked executable was never overwritten or mutated during config run
            if mode == "candidate":
                assert locked_artifact is not None
                after_sha = sha256_file(Path(locked_artifact["executable"]))
                if after_sha != locked_artifact["executable_sha256"]:
                    raise ShadowPilotError(
                        "artifact_lock_failed",
                        f"locked executable mutated during execution of {cid}",
                        host=host_label,
                        config_id=cid,
                    )

            snap_after_cfg = disk_snapshot(root, target_dir)
            min_free_bytes = min(min_free_bytes, snap_after_cfg["fs_free_bytes"])
            max_target_bytes = max(max_target_bytes, snap_after_cfg["target_dir_bytes"])
            peak_rss_overall = max(peak_rss_overall, cfg_peak_rss)

            total_build_seconds += cfg_build_seconds
            total_probe_seconds += cfg_probe_seconds
            total_list_seconds += cfg_list_seconds
            total_run_seconds += cfg_run_seconds

            config_records.append(
                {
                    "config_id": cid,
                    "package": pkg_name,
                    "package_id": pkg_id,
                    "target_name": cfg["target_name"],
                    "target_kind": cfg["target_kind"],
                    "features": cfg["expected_features"],
                    "entry_count": len(cfg["entries"]),
                    "artifact_locked": locked_artifact is not None,
                    "probe_equivalent": mode == "candidate",
                    "locked_artifact": locked_artifact,
                    "build_seconds": round(cfg_build_seconds, 6),
                    "probe_seconds": round(cfg_probe_seconds, 6),
                    "list_seconds": round(cfg_list_seconds, 6),
                    "run_seconds": round(cfg_run_seconds, 6),
                    "peak_rss_bytes": cfg_peak_rss,
                    "disk_after_config": snap_after_cfg,
                }
            )

        final_disk = disk_snapshot(root, target_dir)
        min_free_bytes = min(min_free_bytes, final_disk["fs_free_bytes"])
        max_target_bytes = max(max_target_bytes, final_disk["target_dir_bytes"])

        ledger = {
            "schema": LEDGER_SCHEMA,
            "status": "pass",
            "suite_id": suite_id,
            "source_script": manifest["source_script"],
            "formal_ci_step_name": formal_ci_step_name,
            "mode": mode,
            "execution_scope": "list-only" if list_only else "execute",
            "git_sha": git_sha,
            "git_tree": git_tree,
            "host": host_label,
            "host_triple": host_triple,
            "rustc_release": rustc_release,
            "host_resources": host_resources,
            "manifest_sha256": manifest_sha256,
            "shell_parity": parity_report,
            "build_flags": build_flags_snapshot,
            "target_dir": str(target_dir),
            "cache_phase": effective_env.get("KOVEN_SHADOW_CACHE_PHASE", "unspecified"),
            "cache_hit": effective_env.get("KOVEN_SHADOW_CACHE_HIT", "unspecified"),
            "telemetry": {
                "total_wall_seconds": round(time.monotonic() - pilot_started, 6),
                "build_seconds": round(total_build_seconds, 6),
                "probe_seconds": round(total_probe_seconds, 6),
                "list_seconds": round(total_list_seconds, 6),
                "run_seconds": round(total_run_seconds, 6),
                "peak_child_rss_bytes": peak_rss_overall,
                "oom_suspected_count": oom_events,
                "retry_count": 0,
                "initial_disk": initial_disk,
                "final_disk": final_disk,
                "min_fs_free_bytes": min_free_bytes,
                "max_target_dir_bytes": max_target_bytes,
            },
            "configs": config_records,
            "entries": entry_records,
        }
        write_json_atomic(artifacts_dir / "ledger.json", ledger)
        return ledger
    except ShadowPilotError as err:
        (artifacts_dir / "ledger.json").unlink(missing_ok=True)
        failure_payload = {
            "schema": LEDGER_SCHEMA,
            "status": "failed",
            "mode": mode,
            "execution_scope": "list-only" if list_only else "execute",
            "git_sha": git_sha,
            "git_tree": git_tree,
            "host": host_label,
            "host_triple": host_triple,
            "manifest_sha256": manifest_sha256,
            "error": err.to_dict(),
            "completed_configs": config_records,
            "completed_entries": entry_records,
        }
        write_json_atomic(artifacts_dir / "failure.json", failure_payload)
        raise
    except BaseException:
        (artifacts_dir / "ledger.json").unlink(missing_ok=True)
        raise


def load_verified_ledger(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise ShadowPilotError("ab_mismatch", f"missing ledger file: {path}")
    sibling_failure = path.parent / "failure.json"
    if sibling_failure.exists():
        raise ShadowPilotError(
            "ab_mismatch",
            f"refusing stale ledger {path} because failure.json exists in {path.parent}",
        )
    return json.loads(path.read_text(encoding="utf-8"))


def resolve_run_arm_ledgers(
    download_dir: Path,
    *,
    run_id: int,
    max_attempt: int,
    hosts: tuple[str, ...] = ("macos-15", "ubuntu-26.04"),
    arms: tuple[str, ...] = ("baseline", "candidate"),
) -> dict[tuple[str, str], Path]:
    """Select each (host_os, arm)'s latest attempt <= max_attempt; fail closed if missing or failed."""
    if not download_dir.is_dir():
        raise ShadowPilotError("ab_mismatch", f"missing artifact download directory: {download_dir}")
    resolved: dict[tuple[str, str], Path] = {}
    for host_os in hosts:
        for arm in arms:
            prefix = f"e2-shadow-arm-{host_os}-{arm}-{run_id}-"
            candidates: list[tuple[int, Path]] = []
            for entry in download_dir.iterdir():
                if not entry.is_dir() or not entry.name.startswith(prefix):
                    continue
                suffix = entry.name[len(prefix) :]
                if suffix.isdigit():
                    attempt_num = int(suffix)
                    if 1 <= attempt_num <= max_attempt:
                        candidates.append((attempt_num, entry))
            if not candidates:
                raise ShadowPilotError(
                    "ab_mismatch",
                    f"no downloaded arm artifact for ({host_os}, {arm}) in run {run_id} (max_attempt={max_attempt})",
                    host=host_os,
                )
            latest_attempt, latest_dir = max(candidates, key=lambda item: item[0])
            ledger_file = latest_dir / "ledger.json"
            if (latest_dir / "failure.json").exists() or not ledger_file.is_file():
                raise ShadowPilotError(
                    "ab_mismatch",
                    f"latest attempt {latest_attempt} for ({host_os}, {arm}) failed or lacks ledger.json in {latest_dir}",
                    host=host_os,
                )
            resolved[(host_os, arm)] = ledger_file
    return resolved


def compare_ledgers(
    baseline: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    """Verify strict semantic equivalence between Arm A (baseline) and Arm B (candidate)."""
    host = baseline.get("host") or candidate.get("host") or "unknown"
    if baseline.get("schema") != LEDGER_SCHEMA or candidate.get("schema") != LEDGER_SCHEMA:
        raise ShadowPilotError("ab_mismatch", "invalid ledger schema", host=host)
    if baseline.get("status") != "pass" or candidate.get("status") != "pass":
        raise ShadowPilotError("ab_mismatch", "both baseline and candidate ledgers must have status=pass", host=host)
    if baseline.get("mode") != "baseline" or candidate.get("mode") != "candidate":
        raise ShadowPilotError("ab_mismatch", "expected baseline vs candidate ledgers", host=host)

    for identity_key in (
        "suite_id",
        "source_script",
        "git_sha",
        "git_tree",
        "host",
        "host_triple",
        "rustc_release",
        "manifest_sha256",
        "execution_scope",
    ):
        if baseline.get(identity_key) != candidate.get(identity_key):
            raise ShadowPilotError(
                "ab_mismatch",
                f"{identity_key} differs: baseline={baseline.get(identity_key)!r} vs candidate={candidate.get(identity_key)!r}",
                host=host,
            )

    if (
        baseline.get("cache_phase") != candidate.get("cache_phase")
        or baseline.get("cache_hit") != candidate.get("cache_hit")
    ):
        raise ShadowPilotError(
            "ab_mismatch",
            f"mixed cold/warm cache state across A/B arms invalidates comparison: "
            f"baseline=(phase={baseline.get('cache_phase')!r}, hit={baseline.get('cache_hit')!r}) vs "
            f"candidate=(phase={candidate.get('cache_phase')!r}, hit={candidate.get('cache_hit')!r})",
            host=host,
        )

    b_init_bytes = ((baseline.get("telemetry") or {}).get("initial_disk") or {}).get("target_dir_bytes", 0)
    c_init_bytes = ((candidate.get("telemetry") or {}).get("initial_disk") or {}).get("target_dir_bytes", 0)
    if baseline.get("cache_phase") == "warm" and (b_init_bytes <= 0 or c_init_bytes <= 0):
        raise ShadowPilotError(
            "ab_mismatch",
            f"warm comparison requires non-empty restored target dirs on both arms, got baseline={b_init_bytes}, candidate={c_init_bytes}",
            host=host,
        )
    if baseline.get("cache_phase") == "cold" and (b_init_bytes != 0 or c_init_bytes != 0):
        raise ShadowPilotError(
            "ab_mismatch",
            f"cold comparison requires empty initial target dirs on both arms, got baseline={b_init_bytes}, candidate={c_init_bytes}",
            host=host,
        )

    if baseline.get("target_dir") == candidate.get("target_dir"):
        raise ShadowPilotError(
            "ab_mismatch",
            f"A/B arms must use isolated CARGO_TARGET_DIRs, both used {baseline.get('target_dir')!r}",
            host=host,
        )

    b_flags = baseline.get("build_flags") or {}
    c_flags = candidate.get("build_flags") or {}
    fixed_flag_keys = (set(b_flags) | set(c_flags)) - {"cargo_jobs"}
    for flag_key in sorted(fixed_flag_keys):
        if b_flags.get(flag_key) != c_flags.get(flag_key):
            raise ShadowPilotError(
                "ab_mismatch",
                f"build_flags.{flag_key} differs: baseline={b_flags.get(flag_key)!r} vs candidate={c_flags.get(flag_key)!r}",
                host=host,
            )

    b_cfgs = baseline.get("configs", [])
    c_cfgs = candidate.get("configs", [])
    if len(b_cfgs) != len(c_cfgs) or [c["config_id"] for c in b_cfgs] != [c["config_id"] for c in c_cfgs]:
        raise ShadowPilotError("ab_mismatch", "config sequence mismatch between baseline and candidate", host=host)

    for c_cfg in c_cfgs:
        if not c_cfg.get("artifact_locked") or not c_cfg.get("probe_equivalent"):
            raise ShadowPilotError(
                "ab_mismatch",
                "candidate config missing artifact lock or runtime probe equivalence",
                host=host,
                config_id=c_cfg.get("config_id"),
            )

    b_entries = baseline.get("entries", [])
    c_entries = candidate.get("entries", [])
    if len(b_entries) != len(c_entries):
        raise ShadowPilotError(
            "ab_mismatch",
            f"entry count mismatch: baseline={len(b_entries)} vs candidate={len(c_entries)}",
            host=host,
        )

    classifications: dict[str, int] = {}
    is_execute = baseline["execution_scope"] == "execute"

    for b_row, c_row in zip(b_entries, c_entries):
        cid = b_row["config_id"]
        tname = b_row["test_name"]
        for key in ("global_index", "config_id", "test_name", "ignored_filter", "expected_outcome"):
            if b_row.get(key) != c_row.get(key):
                raise ShadowPilotError(
                    "ab_mismatch",
                    f"entry metadata {key} mismatch: {b_row.get(key)!r} vs {c_row.get(key)!r}",
                    host=host,
                    config_id=cid,
                    test_name=tname,
                )
        if b_row.get("list_hits") != 1 or c_row.get("list_hits") != 1:
            raise ShadowPilotError(
                "ab_mismatch",
                f"list_hits must both equal 1, got baseline={b_row.get('list_hits')} candidate={c_row.get('list_hits')}",
                host=host,
                config_id=cid,
                test_name=tname,
            )
        if is_execute:
            if b_row.get("executed_count") != 1 or c_row.get("executed_count") != 1:
                raise ShadowPilotError(
                    "ab_mismatch",
                    "both arms must execute each entry exactly once",
                    host=host,
                    config_id=cid,
                    test_name=tname,
                )
            if (
                b_row.get("exit_code") != c_row.get("exit_code")
                or b_row.get("classification") != c_row.get("classification")
                or b_row.get("matched_failure_witnesses") != c_row.get("matched_failure_witnesses")
            ):
                raise ShadowPilotError(
                    "ab_mismatch",
                    f"execution result mismatch: baseline=(exit={b_row.get('exit_code')}, class={b_row.get('classification')}) vs candidate=(exit={c_row.get('exit_code')}, class={c_row.get('classification')})",
                    host=host,
                    config_id=cid,
                    test_name=tname,
                    evidence={"baseline": b_row, "candidate": c_row},
                )
        cls = c_row["classification"]
        classifications[cls] = classifications.get(cls, 0) + 1

    b_tel = baseline["telemetry"]
    c_tel = candidate["telemetry"]
    return {
        "schema": COMPARISON_SCHEMA,
        "equivalent": True,
        "suite_id": baseline.get("suite_id"),
        "source_script": baseline.get("source_script"),
        "formal_ci_step_name": baseline.get("formal_ci_step_name"),
        "git_sha": baseline["git_sha"],
        "git_tree": baseline.get("git_tree"),
        "host": host,
        "host_triple": baseline["host_triple"],
        "execution_scope": baseline["execution_scope"],
        "cache_phase": {
            "baseline": baseline.get("cache_phase"),
            "candidate": candidate.get("cache_phase"),
        },
        "cache_hit": {
            "baseline": baseline.get("cache_hit"),
            "candidate": candidate.get("cache_hit"),
        },
        "cargo_jobs": {
            "baseline": (baseline.get("build_flags") or {}).get("cargo_jobs", "-j1"),
            "candidate": (candidate.get("build_flags") or {}).get("cargo_jobs", "-j1"),
        },
        "host_resources": {
            "baseline": baseline.get("host_resources"),
            "candidate": candidate.get("host_resources"),
        },
        "compared_configs": len(b_cfgs),
        "compared_entries": len(b_entries),
        "classifications": classifications,
        "telemetry_comparison": {
            "baseline": b_tel,
            "candidate": c_tel,
            "delta_wall_seconds": round(c_tel["total_wall_seconds"] - b_tel["total_wall_seconds"], 6),
            "note": "Single-run wall-clock delta is observational only, not a standalone acceptance verdict; pre-warmed formal CI step times must never be subtracted from isolated cold runs.",
        },
    }


def iso_duration_seconds(started: str | None, completed: str | None) -> float | None:
    if not started or not completed:
        return None
    t0 = datetime.fromisoformat(started.replace("Z", "+00:00"))
    t1 = datetime.fromisoformat(completed.replace("Z", "+00:00"))
    return round((t1 - t0).total_seconds(), 3)


CI_EVIDENCE_MARKER_RE = re.compile(
    r"^Code CI Evidence v1 PR([1-9][0-9]*) [0-9a-f]{40} [0-9a-f]{40} ([0-9a-f]{40}) [0-9a-f]{64}$"
)
CI_ARTIFACT_MERGE_SHA_RE = re.compile(
    r"^(?:word-frequency|conformance|native-sanitizers|generated-owners)-(?:Linux|macOS)-([0-9a-f]{40})$"
)


def fetch_workflow_metrics(
    repo: str,
    shadow_run_id: int,
    token: str,
    *,
    head_sha: str,
    checked_out_sha: str | None = None,
    reference_ci_run_id: int | None = None,
    formal_step_name: str = "Verify opt-in two-source Unit capability and atomicity",
    api_get_fn: Any = None,
) -> dict[str, Any]:
    """Fetch read-only GitHub Actions queue/wall-clock metrics for shadow run and validated reference ci.yml."""

    def default_api_get(path: str) -> dict[str, Any]:
        req = urllib.request.Request(
            f"https://api.github.com/repos/{repo}/{path.lstrip('/')}",
            headers={
                "Authorization": f"Bearer {token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read().decode("utf-8"))

    api_get = api_get_fn or default_api_get
    shadow_run_meta = api_get(f"actions/runs/{shadow_run_id}")
    shadow_event = shadow_run_meta.get("event")
    shadow_pr_numbers = [
        pr["number"]
        for pr in (shadow_run_meta.get("pull_requests") or [])
        if isinstance(pr, dict) and isinstance(pr.get("number"), int)
    ]

    shadow_jobs = api_get(f"actions/runs/{shadow_run_id}/jobs?per_page=100").get("jobs", [])
    shadow_summary = []
    for job in shadow_jobs:
        shadow_summary.append(
            {
                "name": job.get("name"),
                "status": job.get("status"),
                "conclusion": job.get("conclusion"),
                "queue_seconds": iso_duration_seconds(job.get("created_at"), job.get("started_at")),
                "wall_seconds": iso_duration_seconds(job.get("started_at"), job.get("completed_at")),
            }
        )

    def validate_and_load_ci_run(
        cand_id: int, cand_meta: dict[str, Any] | None = None
    ) -> tuple[bool, str, list[dict[str, Any]]]:
        meta = (
            cand_meta
            if (cand_meta is not None and "path" in cand_meta and "event" in cand_meta and "head_sha" in cand_meta)
            else api_get(f"actions/runs/{cand_id}")
        )
        wf_path = str(meta.get("path") or "").split("@", 1)[0]
        if wf_path != ".github/workflows/ci.yml":
            return False, f"run {cand_id} belongs to workflow {wf_path!r}, expected '.github/workflows/ci.yml'", []
        if meta.get("head_sha") != head_sha:
            return False, f"run {cand_id} head_sha={meta.get('head_sha')!r} != expected {head_sha!r}", []

        cand_event = meta.get("event")
        if shadow_event == "pull_request":
            if cand_event != "pull_request":
                return False, f"run {cand_id} has event={cand_event!r}, expected 'pull_request' to match PR merge checkout", []
            title = meta.get("display_title") or ""
            if shadow_pr_numbers and title.startswith("CI pull_request PR ") and title != f"CI pull_request PR {shadow_pr_numbers[0]}":
                return False, f"run {cand_id} belongs to {title!r}, expected PR {shadow_pr_numbers[0]}", []
        elif shadow_event and cand_event == "pull_request":
            return False, f"run {cand_id} is a pull_request merge run whereas shadow run event={shadow_event!r}", []

        ci_jobs = api_get(f"actions/runs/{cand_id}/jobs?per_page=100").get("jobs", [])
        if checked_out_sha and re.fullmatch(r"[0-9a-f]{40}", checked_out_sha):
            witness_shas: set[str] = set()
            for j in ci_jobs:
                jm = CI_EVIDENCE_MARKER_RE.match(str(j.get("name") or ""))
                if jm:
                    witness_shas.add(jm.group(2))
            artifacts = api_get(f"actions/runs/{cand_id}/artifacts?per_page=100").get("artifacts", [])
            for art in artifacts:
                am = CI_ARTIFACT_MERGE_SHA_RE.match(str(art.get("name") or ""))
                if am:
                    witness_shas.add(am.group(1))
            if witness_shas and any(w != checked_out_sha for w in witness_shas):
                return (
                    False,
                    f"run {cand_id} tested checkout SHA {sorted(witness_shas)!r} != shadow checked_out_sha {checked_out_sha!r}",
                    [],
                )
            has_completed_targeted = any(
                str(j.get("name") or "").startswith("Targeted Tests (") and j.get("conclusion") == "success"
                for j in ci_jobs
            )
            if (
                shadow_event == "pull_request"
                and checked_out_sha != head_sha
                and meta.get("status") == "completed"
                and has_completed_targeted
                and not witness_shas
            ):
                return (
                    False,
                    f"completed PR CI run {cand_id} has no checkout SHA witness confirming merge commit {checked_out_sha!r}",
                    [],
                )
        return True, "ok", ci_jobs

    ref_summary: list[dict[str, Any]] = []
    selected_ci_run_id: int | None = None
    selected_ci_jobs: list[dict[str, Any]] = []

    if reference_ci_run_id is not None:
        ok, reason, jobs = validate_and_load_ci_run(reference_ci_run_id, None)
        if not ok:
            raise ShadowPilotError("invalid_reference_run", reason)
        selected_ci_run_id = reference_ci_run_id
        selected_ci_jobs = jobs
    elif head_sha and head_sha != "unknown":
        runs = api_get(f"actions/workflows/ci.yml/runs?head_sha={head_sha}&per_page=20").get("workflow_runs", [])
        for cand in runs:
            cid = cand.get("id")
            if not isinstance(cid, int):
                continue
            ok, _reason, jobs = validate_and_load_ci_run(cid, cand)
            if ok:
                selected_ci_run_id = cid
                selected_ci_jobs = jobs
                break

    if selected_ci_run_id is not None:
        for job in selected_ci_jobs:
            name = job.get("name", "")
            if not name.startswith("Targeted Tests ("):
                continue
            e2_step_sec = None
            for step in job.get("steps") or []:
                if step.get("name") == formal_step_name:
                    e2_step_sec = iso_duration_seconds(step.get("started_at"), step.get("completed_at"))
                    break
            ref_summary.append(
                {
                    "ci_run_id": selected_ci_run_id,
                    "job_name": name,
                    "status": job.get("status"),
                    "conclusion": job.get("conclusion"),
                    "queue_seconds": iso_duration_seconds(job.get("created_at"), job.get("started_at")),
                    "job_wall_seconds": iso_duration_seconds(job.get("started_at"), job.get("completed_at")),
                    "e2_step_reference_seconds": e2_step_sec,
                    "reference_only_prewarmed_shared_target": True,
                }
            )

    return {
        "shadow_run_id": shadow_run_id,
        "head_sha": head_sha,
        "checked_out_sha": checked_out_sha,
        "shadow_jobs": shadow_summary,
        "formal_ci_reference": ref_summary,
        "warning": "Formal Targeted Tests E2 step runs after earlier composition/D10/E2 steps in a shared target dir; treat as reference only and never subtract directly from isolated cold runs.",
    }


def render_markdown_summary(comparisons: list[dict[str, Any]], workflow_metrics: dict[str, Any] | None = None) -> str:
    suite_label = (comparisons[0].get("suite_id") if comparisons else None) or "e2-two-source-unit"
    lines = [
        f"## SPEC-0495 E2 Shadow A/B Report (`{suite_label}`)",
        "",
        "| Suite | Scope | Host (CPUs / RAM) | Jobs (A/B) | Cache Phase (A/B) | Hit (A/B) | Listed / Exec | Verdict | Baseline Wall (s) | Cand Build+Probe (s) | Cand List+Run (s) | Cand Wall (s) | Peak Child RSS MiB (A / B) | Max Target GiB (A / B) | OOM / Retries |",
        "|---|---|---|---|---|---|---:|---|---:|---:|---:|---:|---|---|---|",
    ]
    for comp in comparisons:
        bt = comp["telemetry_comparison"]["baseline"]
        ct = comp["telemetry_comparison"]["candidate"]
        b_rss = bt["peak_child_rss_bytes"] / (1024 * 1024)
        c_rss = ct["peak_child_rss_bytes"] / (1024 * 1024)
        b_tgt = bt["max_target_dir_bytes"] / (1024**3)
        c_tgt = ct["max_target_dir_bytes"] / (1024**3)
        c_bp = ct["build_seconds"] + ct["probe_seconds"]
        c_lr = ct["list_seconds"] + ct["run_seconds"]
        oom_str = f"{bt['oom_suspected_count']}+{ct['oom_suspected_count']} / {bt['retry_count']}+{ct['retry_count']}"
        hr = (comp.get("host_resources") or {}).get("candidate") or {}
        cpus = hr.get("cpu_count", "?")
        ram_gib = f"{hr['total_ram_bytes'] / (1024**3):.1f}G" if hr.get("total_ram_bytes") else "?G"
        jobs = comp.get("cargo_jobs") or {"baseline": "-j1", "candidate": "-j1"}
        s_id = comp.get("suite_id") or suite_label
        scope = comp.get("execution_scope") or "execute"
        listed_n = comp["compared_entries"]
        exec_n = 0 if scope == "list-only" else listed_n
        verdict = "`LIST-ONLY PASS`" if scope == "list-only" else "`PASS`"
        lines.append(
            f"| `{s_id}` | `{scope}` | `{comp['host']}` (`{cpus}c / {ram_gib}`) | `{jobs['baseline']}` / `{jobs['candidate']}` | "
            f"`{comp['cache_phase']['baseline']}` / `{comp['cache_phase']['candidate']}` | "
            f"`{comp['cache_hit']['baseline']}` / `{comp['cache_hit']['candidate']}` | "
            f"{listed_n} / {exec_n} | {verdict} | "
            f"{bt['total_wall_seconds']:.2f} | {c_bp:.2f} | {c_lr:.2f} | {ct['total_wall_seconds']:.2f} | "
            f"{b_rss:.1f} / {c_rss:.1f} | {b_tgt:.2f} / {c_tgt:.2f} | `{oom_str}` |"
        )
    lines.extend(
        [
            "",
            "> Note: Pre-warmed `ci.yml` step timings are reference-only and must not be subtracted directly from isolated cold A/B runs; a single run is not a stability conclusion.",
        ]
    )
    if workflow_metrics and workflow_metrics.get("formal_ci_reference"):
        lines.extend(
            [
                "",
                "### Formal `ci.yml` Reference Timings (Same SHA, Shared Target — Reference Only)",
                "",
                "| Run ID | Job | Queue (s) | Whole Job Wall (s) | Original E2 Step (s) |",
                "|---:|---|---:|---:|---:|",
            ]
        )
        for row in workflow_metrics["formal_ci_reference"]:
            q = f"{row['queue_seconds']:.1f}" if row["queue_seconds"] is not None else "N/A"
            w = f"{row['job_wall_seconds']:.1f}" if row["job_wall_seconds"] is not None else "N/A"
            s = f"{row['e2_step_reference_seconds']:.1f}" if row["e2_step_reference_seconds"] is not None else "N/A"
            lines.append(f"| `{row['ci_run_id']}` | `{row['job_name']}` | {q} | {w} | {s} |")
    lines.append("")
    return "\n".join(lines)


def cmd_internal_probe(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="e2_two_source_shadow_pilot.py internal-probe")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--no-inspect-dylibs", action="store_true")
    parser.add_argument("target_cmd", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    cmd = list(args.target_cmd)
    if cmd and cmd[0] == "--":
        cmd = cmd[1:]
    if not cmd:
        raise SystemExit("internal-probe requires executable and arguments")
    exe = Path(cmd[0])
    snap = build_probe_snapshot(
        executable=exe,
        cwd=Path.cwd(),
        env=dict(os.environ),
        inspect_dylibs=not args.no_inspect_dylibs,
    )
    write_json_atomic(args.output, snap)
    completed = subprocess.run(cmd, check=False)
    return completed.returncode


def main(argv: list[str] | None = None) -> int:
    raw_argv = list(sys.argv[1:] if argv is None else argv)
    if raw_argv and raw_argv[0] == "internal-probe":
        return cmd_internal_probe(raw_argv[1:])

    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="subcommand", required=True)

    p_verify = sub.add_parser("verify-manifest", help="verify shared manifest against shell script")
    p_verify.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)

    p_run = sub.add_parser("run", help="run baseline or candidate arm")
    p_run.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    p_run.add_argument("--mode", choices=("baseline", "candidate"), required=True)
    p_run.add_argument(
        "--candidate-jobs",
        default="manifest",
        help="Cargo build jobs for candidate arm: 'manifest' (-j1), 'default' (all runner cores), or positive int",
    )
    p_run.add_argument("--list-only", action="store_true")
    p_run.add_argument(
        "--formal-gate",
        action="store_true",
        help="Strict formal CI gate mode: forbid --list-only, forbid expected_failure entries, and require dynamic shell execution-branch parity",
    )
    p_run.add_argument("--artifacts", type=Path, required=True)

    p_cmp = sub.add_parser("compare", help="compare baseline and candidate ledgers")
    p_cmp.add_argument("--baseline", type=Path, required=True)
    p_cmp.add_argument("--candidate", type=Path, required=True)
    p_cmp.add_argument("--output", type=Path, required=True)
    p_cmp.add_argument("--markdown", type=Path)

    p_cmp_run = sub.add_parser(
        "compare-run-downloads",
        help="resolve latest per-arm attempt ledgers from downloaded run artifacts and compare",
    )
    p_cmp_run.add_argument("--download-dir", type=Path, required=True)
    p_cmp_run.add_argument("--run-id", type=int, required=True)
    p_cmp_run.add_argument("--max-attempt", type=int, required=True)
    p_cmp_run.add_argument("--output-dir", type=Path, required=True)

    p_wf = sub.add_parser("collect-workflow-metrics", help="collect queue/job/step wall-clock via GitHub API")
    p_wf.add_argument("--repo", required=True)
    p_wf.add_argument("--shadow-run-id", type=int, required=True)
    p_wf.add_argument("--head-sha", required=True)
    p_wf.add_argument("--reference-ci-run-id", type=int)
    p_wf.add_argument("--comparisons", type=Path, nargs="*", default=[])
    p_wf.add_argument("--output", type=Path, required=True)
    p_wf.add_argument("--markdown", type=Path)

    args = parser.parse_args(raw_argv)
    try:
        if args.subcommand == "verify-manifest":
            manifest = load_manifest(args.manifest)
            report = verify_manifest_against_shell_script(manifest, ROOT)
            print(json.dumps(report, indent=2, sort_keys=True))
            return 0

        if args.subcommand == "run":
            purged_dir = purge_artifacts_dir(args.artifacts)
            try:
                manifest = load_manifest(args.manifest)
            except (OSError, json.JSONDecodeError, ShadowPilotError) as exc:
                err = (
                    exc
                    if isinstance(exc, ShadowPilotError)
                    else ShadowPilotError("manifest_invalid", f"failed to load manifest {args.manifest}: {exc}")
                )
                write_json_atomic(
                    purged_dir / "failure.json",
                    {
                        "schema": LEDGER_SCHEMA,
                        "status": "failed",
                        "mode": args.mode,
                        "execution_scope": "list-only" if args.list_only else "execute",
                        "error": err.to_dict(),
                    },
                )
                raise err from exc
            ledger = run_pilot(
                manifest=manifest,
                root=ROOT,
                mode=args.mode,
                list_only=args.list_only,
                artifacts_dir=purged_dir,
                candidate_jobs=args.candidate_jobs,
                formal_gate=args.formal_gate,
            )
            print(
                f"shadow-pilot {args.mode} ({ledger['execution_scope']}): "
                f"{len(ledger['configs'])} configs, {len(ledger['entries'])} entries verified in "
                f"{ledger['telemetry']['total_wall_seconds']:.2f}s"
            )
            return 0

        if args.subcommand == "compare":
            b_ledger = load_verified_ledger(args.baseline)
            c_ledger = load_verified_ledger(args.candidate)
            comp = compare_ledgers(b_ledger, c_ledger)
            write_json_atomic(args.output, comp)
            if args.markdown:
                args.markdown.parent.mkdir(parents=True, exist_ok=True)
                args.markdown.write_text(render_markdown_summary([comp]), encoding="utf-8")
            print(
                f"shadow-pilot A/B compare ({comp['host']}, {comp['execution_scope']}): "
                f"PASS ({comp['compared_entries']} entries equivalent)"
            )
            return 0

        if args.subcommand == "compare-run-downloads":
            resolved = resolve_run_arm_ledgers(
                args.download_dir, run_id=args.run_id, max_attempt=args.max_attempt
            )
            args.output_dir.mkdir(parents=True, exist_ok=True)
            for host_os in ("macos-15", "ubuntu-26.04"):
                b_ledger = load_verified_ledger(resolved[(host_os, "baseline")])
                c_ledger = load_verified_ledger(resolved[(host_os, "candidate")])
                comp = compare_ledgers(b_ledger, c_ledger)
                write_json_atomic(args.output_dir / f"comparison-{host_os}.json", comp)
                (args.output_dir / f"comparison-{host_os}.md").write_text(
                    render_markdown_summary([comp]), encoding="utf-8"
                )
            return 0

        if args.subcommand == "collect-workflow-metrics":
            token = os.environ.get("GH_TOKEN") or ""
            comps = [json.loads(p.read_text(encoding="utf-8")) for p in args.comparisons] if args.comparisons else []
            step_name = (
                (comps[0].get("formal_ci_step_name") if comps else None)
                or "Verify opt-in two-source Unit capability and atomicity"
            )
            checked_out_sha = comps[0].get("git_sha") if comps else None
            metrics = fetch_workflow_metrics(
                args.repo,
                args.shadow_run_id,
                token,
                head_sha=args.head_sha,
                checked_out_sha=checked_out_sha,
                reference_ci_run_id=args.reference_ci_run_id,
                formal_step_name=step_name,
            )
            write_json_atomic(args.output, metrics)
            if args.markdown and comps:
                args.markdown.parent.mkdir(parents=True, exist_ok=True)
                args.markdown.write_text(render_markdown_summary(comps, metrics), encoding="utf-8")
            return 0
    except ShadowPilotError as err:
        print(f"e2-shadow-pilot error: {err}", file=sys.stderr)
        ev = err.evidence or {}
        for diag_key, label in (
            ("compiler_diagnostics", "Cargo compiler diagnostics"),
            ("stdout_tail", "test/process stdout"),
            ("stderr_tail", "test/process stderr"),
        ):
            snippet = ev.get(diag_key)
            if isinstance(snippet, str) and snippet.strip():
                print(f"--- {label} ---\n{snippet}\n--- end {label} ---", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
