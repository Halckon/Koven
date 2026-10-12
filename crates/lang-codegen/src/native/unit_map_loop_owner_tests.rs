//! SPEC-0300: unmodified ordinary-source native paths, without allocator instrumentation.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
const NORMAL: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/normal-exhaustion.ko");
const EXPECTED: &str = concat!(
    "dead\npresent\ndead\npresent\nmissing\nmissing\ndead\n",
    "live\npresent\nlive\npresent\nmissing\nmissing\nlive\n",
    "dead\nput\ndead\nput\nput\nput\ndead\n",
    "live\nput\nlive\nput\nput\nput\nlive\n",
);

#[test]
fn single_map_loop_original_native_repeats_remove() {
    let run = emit_link_and_run("map-loop-original.ko", ORIGINAL, "main");
    assert_success(&run, b"present\nmissing\n");
}

#[test]
fn unit_map_loop_original_native_is_independent() {
    assert_units(ORIGINAL, "present\nmissing\n");
}

#[test]
fn single_map_loop_normal_native_zero_one_many_dead_and_live() {
    let run = emit_link_and_run("map-loop-normal.ko", NORMAL, "main");
    assert_success(&run, EXPECTED.as_bytes());
}

#[test]
fn unit_map_loop_normal_native_zero_one_many_dead_and_live() {
    assert_units(NORMAL, EXPECTED);
}

fn assert_units(source: &str, expected: &str) {
    let provider = format!("package p\n{source}");
    let consumer = "package q\nfun entry(): Unit { p.main() }";
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, consumer, constants);
        assert_success(&run, expected.as_bytes());
    }
}
