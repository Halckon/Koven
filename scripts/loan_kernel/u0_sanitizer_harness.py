"""U0 预 U1 阶段可复跑 C/消毒器代理反例套件（ASan / UBSan / TSan / SIGABRT）。

用于为 docs/proposals/u0-ub-harness-evidence.md 与
docs/proposals/unsafe-aliasing-contract-outline.md §9 提供仓库内可一键复跑的
UB2、UB3、UB4、UB6（真实数据竞争）、UB9（FFI2/FFI4 及 FFI5 终止基线对照）、
UB10（越界/未对齐）与 UB11 代理捕获验证。

仅使用 Python 3.9+ 标准库与系统 clang；所有编译产物在 TemporaryDirectory 中运行后自动清理。
"""
from __future__ import annotations

from dataclasses import dataclass
import os
import shutil
import subprocess
import tempfile
from typing import Dict, List, Optional, Tuple


@dataclass(frozen=True)
class SanitizerCase:
    name: str
    ub_code: str
    flags: Tuple[str, ...]
    expected_substrings: Tuple[str, ...]
    expect_nonzero: bool
    source: str
    env_extra: Tuple[Tuple[str, str], ...] = ()


CASES: Tuple[SanitizerCase, ...] = (
    SanitizerCase(
        name="UB2_uaf_asan",
        ub_code="UB2",
        flags=("-fsanitize=address", "-g", "-O0"),
        expected_substrings=("AddressSanitizer: heap-use-after-free",),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
int main(void) {
    int *p = (int *)malloc(sizeof(int));
    *p = 42;
    free(p);
    volatile int x = *p;
    return x;
}
""",
    ),
    SanitizerCase(
        name="UB3_invalid_bool_ubsan",
        ub_code="UB3",
        flags=("-fsanitize=undefined", "-fno-sanitize-recover=all", "-g", "-O0"),
        expected_substrings=("load of value 2, which is not a valid value for type",),
        expect_nonzero=True,
        source=r"""
#include <stdbool.h>
#include <string.h>
int main(void) {
    bool b = false;
    unsigned char bad = 2;
    memcpy(&b, &bad, 1);
    bool copy = b;
    return copy ? 1 : 0;
}
""",
    ),
    SanitizerCase(
        name="UB4_double_free_asan",
        ub_code="UB4",
        flags=("-fsanitize=address", "-g", "-O0"),
        expected_substrings=("AddressSanitizer: attempting double-free",),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
int main(void) {
    int *p = (int *)malloc(sizeof(int));
    *p = 7;
    free(p);
    free(p);
    return 0;
}
""",
    ),
    SanitizerCase(
        name="UB6_data_race_tsan",
        ub_code="UB6",
        flags=("-fsanitize=thread", "-pthread", "-g", "-O0"),
        expected_substrings=("ThreadSanitizer: data race",),
        expect_nonzero=True,
        env_extra=(("TSAN_OPTIONS", "halt_on_error=1:exitcode=66"),),
        source=r"""
#include <pthread.h>
#include <sched.h>
static int shared = 0;
static void *writer(void *arg) {
    (void)arg;
    for (int i = 0; i < 100000; i++) {
        shared = i;
        if ((i & 1023) == 0) sched_yield();
    }
    return 0;
}
int main(void) {
    pthread_t t;
    pthread_create(&t, 0, writer, 0);
    int acc = 0;
    for (int i = 0; i < 100000; i++) {
        acc += shared;
        if ((i & 1023) == 0) sched_yield();
    }
    pthread_join(t, 0);
    return acc == -1 ? 1 : 0;
}
""",
    ),
    SanitizerCase(
        name="UB9_ffi2_retained_borrow_asan",
        ub_code="UB9-FFI2",
        flags=("-fsanitize=address", "-g", "-O0"),
        expected_substrings=("AddressSanitizer: heap-use-after-free",),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
static int *retained = 0;
static void c_retain(const int *borrowed) { retained = (int *)borrowed; }
int main(void) {
    int *owner = (int *)malloc(sizeof(int));
    *owner = 10;
    c_retain(owner);
    free(owner);
    *retained = 99;
    return 0;
}
""",
    ),
    SanitizerCase(
        name="UB9_ffi4_non_nul_string_asan",
        ub_code="UB9-FFI4",
        flags=("-fsanitize=address", "-g", "-O0"),
        expected_substrings=("AddressSanitizer: heap-buffer-overflow",),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
#include <string.h>
int main(void) {
    char *koven_str = (char *)malloc(4);
    memcpy(koven_str, "abcd", 4);
    size_t n = strlen(koven_str);
    free(koven_str);
    return (int)n;
}
""",
    ),
    SanitizerCase(
        name="UB9_ffi5_abort_termination_baseline",
        ub_code="UB9-FFI5-baseline",
        flags=("-g", "-O0"),
        expected_substrings=(),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
typedef void (*cb_t)(void);
static void c_invoke(cb_t cb) { cb(); }
static void koven_cb_abort(void) { abort(); }
int main(void) {
    c_invoke(koven_cb_abort);
    return 0;
}
""",
    ),
    SanitizerCase(
        name="UB10_oob_asan",
        ub_code="UB10-oob",
        flags=("-fsanitize=address", "-g", "-O0"),
        expected_substrings=("AddressSanitizer: heap-buffer-overflow",),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
int main(void) {
    int *arr = (int *)malloc(4 * sizeof(int));
    for (int i = 0; i < 4; i++) arr[i] = i;
    volatile int v = arr[4];
    free(arr);
    return v;
}
""",
    ),
    SanitizerCase(
        name="UB10_misalign_ubsan",
        ub_code="UB10-align",
        flags=("-fsanitize=undefined", "-fno-sanitize-recover=all", "-g", "-O0"),
        expected_substrings=("load of misaligned address",),
        expect_nonzero=True,
        source=r"""
#include <stdint.h>
#include <stdlib.h>
int main(void) {
    unsigned char *buf = (unsigned char *)malloc(16);
    long long *misaligned = (long long *)(buf + 1);
    long long v = *misaligned;
    free(buf);
    return (int)v;
}
""",
    ),
    SanitizerCase(
        name="UB11_interior_dealloc_asan",
        ub_code="UB11",
        flags=("-fsanitize=address", "-g", "-O0"),
        expected_substrings=("AddressSanitizer: attempting free on address which was not malloc()-ed",),
        expect_nonzero=True,
        source=r"""
#include <stdlib.h>
int main(void) {
    int *p = (int *)malloc(4 * sizeof(int));
    free(p + 1);
    return 0;
}
""",
    ),
)


@dataclass(frozen=True)
class CaseResult:
    name: str
    ub_code: str
    passed: bool
    returncode: int
    summary: str


def run_case(case: SanitizerCase, compiler: str = "clang") -> CaseResult:
    with tempfile.TemporaryDirectory(prefix=f"koven_u0_{case.name}_") as tmp:
        c_path = os.path.join(tmp, f"{case.name}.c")
        bin_path = os.path.join(tmp, case.name)
        with open(c_path, "w", encoding="utf-8") as f:
            f.write(case.source)
        comp = subprocess.run(
            [compiler, *case.flags, c_path, "-o", bin_path],
            capture_output=True,
            text=True,
        )
        if comp.returncode != 0:
            return CaseResult(
                name=case.name,
                ub_code=case.ub_code,
                passed=False,
                returncode=comp.returncode,
                summary=f"compile failed: {comp.stderr.strip()[:200]}",
            )
        env = os.environ.copy()
        for k, v in case.env_extra:
            env[k] = v
        run = subprocess.run([bin_path], capture_output=True, text=True, env=env)
        combined = run.stderr + "\n" + run.stdout
        rc_ok = (run.returncode != 0) if case.expect_nonzero else (run.returncode == 0)
        sub_ok = all(sub in combined for sub in case.expected_substrings)
        passed = rc_ok and sub_ok
        highlights = [
            ln.strip()
            for ln in combined.splitlines()
            if any(tag in ln for tag in ("ERROR:", "WARNING:", "runtime error:", "SUMMARY:"))
        ]
        summary = " | ".join(highlights[:2]) if highlights else f"exit={run.returncode}"
        return CaseResult(
            name=case.name,
            ub_code=case.ub_code,
            passed=passed,
            returncode=run.returncode,
            summary=summary,
        )


def run_all(compiler: Optional[str] = None) -> List[CaseResult]:
    cc = compiler or shutil.which("clang")
    if not cc:
        raise RuntimeError("clang not found in PATH")
    return [run_case(case, compiler=cc) for case in CASES]


def main() -> int:
    results = run_all()
    failed = 0
    for res in results:
        status = "PASS" if res.passed else "FAIL"
        if not res.passed:
            failed += 1
        print(f"[{status}] {res.name} ({res.ub_code}): exit={res.returncode} | {res.summary}")
    print(f"\nSummary: {len(results) - failed}/{len(results)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
