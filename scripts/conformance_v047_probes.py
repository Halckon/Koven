#!/usr/bin/env python3
"""Post-baseline Guide v0.46/v0.47 and V2 negative coverage probes (batch `z`)."""

from __future__ import annotations

from typing import Any


POST_BASELINE_PROBES: dict[str, dict[str, Any]] = {
    "z01_neg_invalid_int_suffix": {
        "kind": "compile_fail",
        "diagnostic": "L0008",
        "guide_chapter": "01-lexical.md",
        "oracle_derivation": "Guide 01/03 forbids undefined numeric suffix `d` (`10d`), emitting INVALID_NUMERIC_LITERAL L0008",
    },
    "z02_neg_move_closure_borrow_capture": {
        "kind": "compile_fail",
        "diagnostic": "L0138",
        "guide_chapter": "07-calls-lambdas-closures.md",
        "oracle_derivation": "Guide 07/10 forbids a `move` closure from capturing a Borrow parameter, emitting ILLEGAL_OWNED_CAPTURE L0138",
    },
    "z03_neg_box_ordinary_class": {
        "kind": "compile_fail",
        "diagnostic": "L0117",
        "guide_chapter": "08-class-family-members.md",
        "oracle_derivation": "Guide 08 restricts Box<T> to value class or enum class; Box(Ordinary(1)) emits INVALID_BOX_ARGUMENT L0117",
    },
    "z04_neg_destructure_discard": {
        "kind": "compile_fail",
        "diagnostic": "L0042",
        "guide_chapter": "11-copyability-layout-construction.md",
        "oracle_derivation": "Guide 11 forbids `_` discard in local val destructuring (`val (_, b)`), emitting UNSUPPORTED_DESTRUCTURING_FORM L0042",
    },
    "z05_neg_redeclare_result": {
        "kind": "compile_fail",
        "diagnostic": "L0150",
        "guide_chapter": "13-program-runtime-standard-library.md",
        "oracle_derivation": "Guide 13 / SPEC-0349 forbids user top-level redeclaration of builtin Result, emitting EXACT_IMPORT_BINDING_CONFLICT L0150",
    },
    "z06_text_lines_split": {
        "kind": "known_unimplemented",
        "allowed_markers": ["FE:L0148", "BE:UnsupportedNode", "BE:MissingFact"],
        "target_stdout": "text-ok\n",
        "guide_chapter": "13-program-runtime-standard-library.md",
        "roadmap_blockers": ["L5"],
        "oracle_derivation": "Guide 13 v0.47 koven.text lines()/split()/parseLong('42') yields 42L and prints 'text-ok' (blocked by L5)",
    },
    "z07_io_path_join": {
        "kind": "known_unimplemented",
        "allowed_markers": ["FE:L0148", "BE:UnsupportedNode", "BE:MissingFact"],
        "target_stdout": "io-path\n",
        "guide_chapter": "13-program-runtime-standard-library.md",
        "roadmap_blockers": ["L6"],
        "oracle_derivation": "Guide 13 v0.47 koven.io Path.join('dir', 'file.txt') yields Ok('dir/file.txt') and prints 'io-path' (blocked by L6)",
    },
    "z08_interface_delegation": {
        "kind": "run_ok",
        "stdout": "delegated\n",
        "guide_chapter": "08-class-family-members.md",
        "oracle_derivation": "Guide 08 ordinary class interface delegation `Greeter by inner` forwards label() and prints 'delegated'",
    },
    "z09_named_destructure": {
        "kind": "run_ok",
        "stdout": "named-destr\n",
        "guide_chapter": "11-copyability-layout-construction.md",
        "oracle_derivation": "Guide 11 value class named destructuring `val (right, left) = Pair2(1, 2)` binds by field name and prints 'named-destr'",
    },
    "z10_main_result_ok": {
        "kind": "known_unimplemented",
        "allowed_markers": ["FE:L0080", "BE:UnsupportedNode", "BE:MissingFact"],
        "target_stdout": "main-result\n",
        "guide_chapter": "13-program-runtime-standard-library.md",
        "roadmap_blockers": ["L21"],
        "oracle_derivation": "Guide 13 conventional `fun main(): Result<Unit, String>` returning Ok(Unit) prints 'main-result' and exits 0 (blocked by L21)",
    },
}

POST_BASELINE_GUIDE_CHAPTERS: dict[str, str] = {
    name: spec["guide_chapter"] for name, spec in POST_BASELINE_PROBES.items()
}

POST_BASELINE_ROADMAP_BLOCKERS: dict[str, list[str]] = {
    name: list(spec["roadmap_blockers"])
    for name, spec in POST_BASELINE_PROBES.items()
    if spec["kind"] == "known_unimplemented"
}

POST_BASELINE_ORACLE_DERIVATIONS: dict[str, str] = {
    name: spec["oracle_derivation"] for name, spec in POST_BASELINE_PROBES.items()
}
