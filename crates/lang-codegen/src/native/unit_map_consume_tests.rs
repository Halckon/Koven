//! SPEC-0291: unmodified source values and normal native exit after consume.
//! This does not observe insertion links, allocation identity or drop counts.

use super::resource_exchange_tests::run_unit;
use crate::native_tests::emit_link_and_run;

const SOURCE: &str = include_str!("../../tests/fixtures/map-consume-0291/owned-locals.ko");
const EXPECTED: &[u8] = include_bytes!("../../tests/fixtures/map-consume-0291/owned-locals.stdout");

#[test]
fn single_map_consume_preserves_values_and_normal_exit() {
    let run = emit_link_and_run("map-consume-owned-locals.ko", SOURCE, "main");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_map_consume_preserves_values_and_normal_exit() {
    let provider = format!("package p\n{SOURCE}");
    let consumer = "package q\nfun entry(): Unit { p.main() }";
    let (run, _) = run_unit(&provider, consumer, false);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

// Nullable String value fixture; bind only after its two-entry complete LLVM gate passes.
const NULLABLE_STRING_SOURCE: &str =
    include_str!("../../tests/fixtures/map-consume-0291/nullable-strings.ko");
const NULLABLE_STRING_EXPECTED: &[u8] =
    include_bytes!("../../tests/fixtures/map-consume-0291/nullable-strings.stdout");

#[test]
fn single_nullable_map_consume_preserves_strings_and_normal_exit() {
    let run = emit_link_and_run(
        "nullable-map-consume-strings.ko",
        NULLABLE_STRING_SOURCE,
        "main",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, NULLABLE_STRING_EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn unit_nullable_map_consume_preserves_strings_and_normal_exit() {
    let provider = format!("package p\n{NULLABLE_STRING_SOURCE}");
    let consumer = "package q\nfun entry(): Unit { p.main() }\n";
    let (run, _) = run_unit(&provider, consumer, false);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, NULLABLE_STRING_EXPECTED);
    assert!(run.stderr.is_empty(), "{run:?}");
}
