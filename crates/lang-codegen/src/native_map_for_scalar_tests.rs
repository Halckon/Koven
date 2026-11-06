//! Ordinary Map-for composition with retired integer/range families, without another
//! construct forcing refinement. Native cases only take successful runtime paths.

use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::name_resolution::SymbolKind;

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{model::Operation, render_program, verify::verify_program},
};

const MODES: [&str; 6] = [
    "borrow-map",
    "own-map",
    "borrow-mutable",
    "own-mutable",
    "temporary",
    "temporary-mutable",
];

fn source(mode: &str, before: &str, body: &str, after: &str) -> String {
    let mutable = mode.contains("mutable");
    let ty = if mutable { "MutableMap" } else { "Map" };
    let result = if mutable { "m" } else { "m.consume()" };
    let (parameter, iterable, argument) = if mode.starts_with("temporary") {
        (String::new(), "make()", "")
    } else {
        let own = if mode.starts_with("own-") { "own " } else { "" };
        (format!("{own}m: {ty}<Int, Int>"), "m", "make()")
    };
    format!(
        r#"fun make(): {ty}<Int, Int> {{
    var m = mutableMapOf<Int, Int>()
    m.put(1, 2)
    m.put(3, 4)
    return {result}
}}
fun scan({parameter}): Unit {{
    {before}
    for ((k, v) in {iterable}) {{ {body} }}
    {after}
}}
fun entry(): Unit {{ scan({argument}) }}
"#
    )
}

fn verified(mode: &str, text: &str, expected: fn(&Operation) -> bool) {
    let analysis = UnitSource::analyze("map_scalar.ko", text).expect("strict source admission");
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration("entry", SymbolKind::Function);
    let (program, entry) = analysis
        .lower_entry(declaration)
        .expect("Map/scalar composition lowers");
    verify_program(&program).expect("SSA verification");
    let (again, _) = analysis
        .lower_entry(declaration)
        .expect("deterministic second lowering");
    assert_text_eq!(render_program(&program), render_program(&again));
    let scan_span = analysis.declaration_span(analysis.declaration("scan", SymbolKind::Function));
    let scan = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.origin == crate::ssa::model::Origin::Source(scan_span))
        .expect("actual source scan is reachable");
    let operations: Vec<_> = scan
        .instructions
        .iter()
        .map(|instruction| &instruction.operation)
        .collect();
    assert!(
        operations
            .iter()
            .any(|op| matches!(op, Operation::MapIterationValueLoan { .. })),
        "{mode}: provider must remain present"
    );
    assert!(
        operations.iter().any(|op| expected(op)),
        "{mode}: scalar operation must remain in scan"
    );
    crate::llvm::render_verified_program_with_entry(&program, entry).expect("LLVM verification");
}

fn native(mode: &str, text: &str, stdout: &str) {
    let memory = super::emit_link_and_run("map_scalar.ko", text, "entry");
    super::assert_result(mode, "memory", &memory, stdout);
    let rooted = super::run_unit_with_entry(mode, "map_scalar.ko", text, "entry");
    super::assert_result(mode, "rooted", &rooted, stdout);
}

#[test]
fn map_for_scalar_invert_reaches_verified_llvm() {
    for mode in MODES {
        let text = source(
            mode,
            "",
            "val x = v.inv(); if (x == -3 || x == -5) { println(\"inv\") }",
            "",
        );
        verified(mode, &text, |op| matches!(op, Operation::IntegerNot { .. }));
    }
}

#[test]
fn map_for_scalar_invert_runs_natively() {
    for mode in MODES {
        let text = source(
            mode,
            "",
            "val x = v.inv(); if (x == -3 || x == -5) { println(\"inv\") }",
            "",
        );
        native(mode, &text, "inv\ninv\n");
    }
}

#[test]
fn map_for_scalar_conversion_reaches_verified_llvm() {
    for mode in MODES {
        let text = source(
            mode,
            "",
            "val x = v.toLong(); if (x == 2L || x == 4L) { println(\"long\") }",
            "",
        );
        verified(mode, &text, |op| {
            matches!(op, Operation::CheckedIntegerConvert { .. })
        });
    }
}

#[test]
fn map_for_scalar_conversion_runs_natively() {
    for mode in MODES {
        let text = source(
            mode,
            "",
            "val x = v.toLong(); if (x == 2L || x == 4L) { println(\"long\") }",
            "",
        );
        native(mode, &text, "long\nlong\n");
    }
}

#[test]
fn map_for_scalar_range_before_loop_reaches_verified_llvm() {
    for mode in MODES {
        let text = source(
            mode,
            "val range = 0..<3",
            "val x = v + 1; if (x == 3 || x == 5) { println(\"add\") }",
            "if (1 in range) { println(\"range\") }",
        );
        verified(mode, &text, |op| {
            matches!(op, Operation::IntegerRangeConstruct { .. })
        });
    }
}

#[test]
fn map_for_scalar_range_before_loop_runs_natively() {
    for mode in MODES {
        let text = source(
            mode,
            "val range = 0..<3",
            "val x = v + 1; if (x == 3 || x == 5) { println(\"add\") }",
            "if (1 in range) { println(\"range\") }",
        );
        native(mode, &text, "add\nadd\nrange\n");
    }
}

#[test]
fn map_for_scalar_plain_add_keeps_existing_native_behavior() {
    for mode in MODES {
        let text = source(
            mode,
            "",
            "val x = v + 1; if (x == 3 || x == 5) { println(\"add\") }",
            "",
        );
        native(mode, &text, "add\nadd\n");
    }
}

#[test]
fn map_for_scalar_invert_control_exits_run_natively() {
    for mode in MODES {
        for (exit, stdout) in [
            ("continue", "inv\ninv\nafter\n"),
            ("break", "inv\nafter\n"),
            ("return", "inv\n"),
        ] {
            let body =
                format!("val x = v.inv(); if (x == -3 || x == -5) {{ println(\"inv\") }}; {exit}");
            native(mode, &source(mode, "", &body, "println(\"after\")"), stdout);
        }
    }
}
