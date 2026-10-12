//! SPEC-0299 normal source → unmodified LLVM/object → native, without instrumentation.
use super::resource_exchange_tests::run_unit;
use crate::native_owned_map_borrow_cases as cases;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

fn run(source: &str, expected: &[u8], unit: bool) {
    eprintln!("owned-map-borrow native unit={unit}; source={source}");
    if unit {
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        let provider = format!("package p\n{provider}");
        let consumer = format!("package q\nimport p.*\nfun entry(){entry}");
        for constants in [false, true] {
            let (run, _) = run_unit(&provider, &consumer, constants);
            assert_success(&run, expected);
        }
    } else {
        assert_success(
            &emit_link_and_run("owned-map-borrow.ko", source, "entry"),
            expected,
        );
    }
}
#[test]
fn owned_map_borrow_native_same_source_core() {
    for unit in [false, true] {
        for grouped in [false, true] {
            for (payload, expected) in [
                ("String", "value\nvalue\nvalue\nend\n"),
                (
                    "Text",
                    "borrowed inline\nborrowed inline\nowned inline\nend\n",
                ),
                ("Resource", "value\nvalue\nvalue\nend\ndrop\n"),
            ] {
                run(&cases::core(payload, grouped), expected.as_bytes(), unit);
            }
        }
    }
}
#[test]
fn owned_map_borrow_native_prefix_scopes_missing_and_resource_lifetime() {
    for unit in [false, true] {
        for (source, expected) in [
            (cases::PAIR, "prefix\nvalue\nvalue\nvalue\n"),
            (
                cases::SCOPES,
                "first\nsecond\nfirst\nsecond\nfirst\nsecond\n",
            ),
            (cases::MISSING, "missing\nend\n"),
            (
                cases::RESOURCE_SCOPE,
                "removed drop\nremoved drop\nend\nremoved drop\nouter drop\n",
            ),
        ] {
            run(source, expected.as_bytes(), unit);
        }
    }
}
#[test]
fn owned_map_borrow_native_last_use_and_separate_inline_read_helpers() {
    for unit in [false, true] {
        for (payload, expected) in [
            ("String", "value\nvalue\nend\n"),
            ("Text", "borrowed inline\nborrowed inline\nend\n"),
            ("Resource", "value\nvalue\nend\ndrop\n"),
        ] {
            run(&cases::last_use(payload), expected.as_bytes(), unit);
        }
        // These helpers exercise different pre-existing callee field-reading paths.
        run(
            &cases::inline_field(unit),
            b"borrowed inline\nborrowed inline\nowned inline\nend\n",
            unit,
        );
        // Explicit source clone reads the String field; it is not an implicit bridge clone.
        run(
            &cases::inline_explicit_clone(),
            b"value\nvalue\nowned inline\nend\n",
            unit,
        );
    }
}
#[test]
fn owned_map_borrow_native_missing_assertion_has_defined_abort() {
    let run = emit_link_and_run("owned-map-missing.ko", cases::ABORT, "entry");
    assert!(!run.status.success());
    assert!(run.stdout.is_empty());
    let (provider, entry) = cases::ABORT.split_once("fun entry()").unwrap();
    for constants in [false, true] {
        let (run, _) = run_unit(
            &format!("package p\n{provider}"),
            &format!("package q\nimport p.*\nfun entry(){entry}"),
            constants,
        );
        assert!(!run.status.success());
        assert!(run.stdout.is_empty());
    }
}

#[test]
fn owned_map_borrow_resource_plain_native_baseline_probe() {
    run(
        &cases::core("Resource", false),
        b"value\nvalue\nvalue\nend\ndrop\n",
        false,
    );
}
