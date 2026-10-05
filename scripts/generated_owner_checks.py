"""Independent acceptance and same-cause reduction of bounded owner programs."""
import difflib
import hashlib
import json
from pathlib import Path
import re
import time


class Failure(Exception):
    def __init__(self, stage, kind, witness, detail="", *, stable_witness=True):
        super().__init__(f"{stage}/{kind}/{witness}: {detail}")
        self.stage, self.kind, self.witness, self.detail = stage, kind, witness, detail
        self.stable_witness = stable_witness

    @property
    def fingerprint(self):
        return self.stage, self.kind, self.witness

    def record(self):
        return dict(stage=self.stage, kind=self.kind, witness=self.witness, detail=self.detail,
                    stable_witness=self.stable_witness)


def sanitizer_failure(stderr, detector):
    """Addresses stay in evidence; reduction needs a named Koven function role."""
    match = re.search(rb"ERROR: " + re.escape(detector.encode()) + rb": ([^\n]+)", stderr)
    if not match:
        return None
    category = ("detected memory leaks" if detector == "LeakSanitizer"
                and match[1].startswith(b"detected memory leaks") else match[1].split()[0].decode())
    primary_stack = re.split(rb"\n(?:freed by|previously allocated by|allocated by|SUMMARY:)",
                             stderr[match.end():], maxsplit=1)[0]
    top = re.search(rb"^\s*#0[^\n]*", primary_stack, re.M)
    frame = re.search(rb"\bin ([A-Za-z_][A-Za-z_0-9.]*)", top[0]) if top else None
    first = frame[1].decode() if frame else "unresolved-function"
    role = re.sub(r"^f\d+\.", "", first)
    kind = "asan_error" if detector == "AddressSanitizer" else "lsan_error"
    # inspect has one generated Leaf field-read role. A caller frame or numbered
    # drop glue does not identify which allocation failed; retain, do not reduce.
    return Failure("native", kind, f"{category}:{role}",
                   stderr.decode(errors="replace"), stable_witness=role == "inspect")


def check_diagnostics(directory, expected):
    stages = [row.split("\t") for row in (directory / "stages.tsv").read_text().splitlines()]
    order = ["parse", "names", "types", "ownership"]
    if any(len(row) != 2 for row in stages) or [row[0] for row in stages] != order[:len(stages)]:
        raise Failure("frontend", "tool_or_harness_failure", "stage-manifest")
    for stage, count in stages:
        if not count.isdecimal():
            raise Failure("frontend", "tool_or_harness_failure", "stage-count")
        if int(count) and stage != "ownership":
            raise Failure("frontend", "unexpected_frontend_rejection", stage)
    if len(stages) != 4:
        raise Failure("frontend", "tool_or_harness_failure", "unfinished-stages")
    diagnostics = []
    for row in (directory / "diagnostics.tsv").read_text().splitlines():
        fields = row.split("\t")
        if len(fields) != 5 or fields[0] != "ownership":
            raise Failure("frontend", "tool_or_harness_failure", "diagnostic-record")
        try:
            labels = [[int(value) for value in pair.split(":")] for pair in fields[4].split(",") if pair]
            if any(len(pair) != 2 for pair in labels):
                raise ValueError("invalid label")
            diagnostics.append(dict(code=fields[1], primary=[int(fields[2]), int(fields[3])], labels=labels))
        except ValueError as error:
            raise Failure("frontend", "tool_or_harness_failure", "diagnostic-span", str(error)) from error
    if len(diagnostics) != int(stages[-1][1]):
        raise Failure("frontend", "diagnostic_mismatch", "diagnostic-count")
    if expected and not diagnostics:
        raise Failure("frontend", "unexpected_acceptance", expected[0]["code"])
    if diagnostics and not expected:
        raise Failure("frontend", "unexpected_frontend_rejection", diagnostics[0]["code"])
    if diagnostics != expected:
        for key in ("code", "primary", "labels"):
            if [row[key] for row in diagnostics] != [row[key] for row in expected]:
                raise Failure("frontend", "diagnostic_mismatch", f"{expected[0]['code']}:{key}", repr(diagnostics))


def output_failure(expected, actual):
    """The missing event survives deletion of unrelated earlier operations."""
    before, after = expected.splitlines(), actual.splitlines()
    for tag, i, j, a, b in difflib.SequenceMatcher(a=before, b=after, autojunk=False).get_opcodes():
        if tag != "equal":
            event = before[i] if i < j else after[a]
            return Failure("native", "native_output_mismatch", event.decode("utf-8", errors="replace"),
                           f"expected {before[i:j]!r}, actual {after[a:b]!r}",
                           stable_witness=before.count(event) == 1)
    if expected != actual:
        return Failure("native", "native_output_mismatch", "line-ending-bytes",
                       f"expected {expected!r}, actual {actual!r}", stable_witness=False)
    raise ValueError("equal output is not an output failure")


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def seal_inputs(directory, names):
    (directory / "input-sha256.json").write_text(json.dumps(
        {name: sha256(directory / name) for name in sorted(names)}, indent=2) + "\n")


def verify_inputs(directory):
    hashes = json.loads((directory / "input-sha256.json").read_text())
    if not hashes:
        raise Failure("replay", "tool_or_harness_failure", "empty-inputs")
    for name, digest in hashes.items():
        if Path(name).name != name or sha256(directory / name) != digest:
            raise Failure("replay", "tool_or_harness_failure", "input-sha256", name)
    return hashes


def minimize(original, failure, candidates, replay, measure, max_candidates=32, seconds=120, *, deadline=None):
    """Keep the first failure and only accept strictly smaller, same-cause inputs."""
    started = time.monotonic()
    deadline = min(started + seconds, deadline) if deadline is not None else started + seconds
    attempts, current = [], original
    number = 0
    if not failure.stable_witness:
        return dict(original=original, original_failure=failure.record(), minimal=original,
                    status="minimization_incomplete", attempts=[], confirmation_count=0,
                    reason="no independently identified semantic witness", elapsed_seconds=0)

    def expired():
        return time.monotonic() >= deadline

    def confirmed(case):
        nonlocal number
        for _ in range(3):
            if expired():
                return "minimization_incomplete"
            result = replay(case, number)
            number += 1
            # A replay may consume the remaining execution budget, including
            # the third and final confirmation. Never turn that into success
            # or mistake the resulting timeout for a changed semantic cause.
            if expired():
                return "minimization_incomplete"
            if result is None or not result.stable_witness or result.fingerprint != failure.fingerprint:
                return "flaky"
        return "reproduced"

    reduction_error = None
    try:
        status = confirmed(original)
        if status == "reproduced":
            while True:
                smaller = False
                for candidate in candidates(current):
                    if expired():
                        status = "minimization_incomplete"
                        break
                    if measure(candidate) >= measure(current):
                        continue
                    if len(attempts) >= max_candidates or expired():
                        status = "minimization_incomplete"
                        break
                    result = replay(candidate, number)
                    number += 1
                    exhausted = expired()
                    accepted = (not exhausted and result is not None and result.stable_witness
                                and result.fingerprint == failure.fingerprint)
                    attempts.append(dict(candidate=candidate, accepted=accepted,
                                         failure=None if result is None else result.record()))
                    if exhausted:
                        status = "minimization_incomplete"
                        break
                    if accepted:
                        current, smaller = candidate, True
                        break
                if status != "reproduced" or not smaller:
                    break
            if status == "reproduced":
                status = confirmed(current)
    except Exception as error:
        # Reduction is secondary evidence. Preserve both the original failure
        # and every completed reduction if any callback or artifact write fails.
        status = "minimization_incomplete"
        reduction_error = dict(stage="minimization", kind="tool_or_harness_failure",
                               witness=type(error).__name__, detail=str(error))
    report = dict(original=original, original_failure=failure.record(), minimal=current,
                  status=status, attempts=attempts, confirmation_count=3 if status == "reproduced" else 0,
                  elapsed_seconds=time.monotonic() - started)
    if reduction_error is not None:
        report["reduction_error"] = reduction_error
    return report
