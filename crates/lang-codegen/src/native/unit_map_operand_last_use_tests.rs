//! Normal source only: original object/link/run; no IR editing or allocation instrumentation.
use super::{assert_success, emit_link_and_run};

const SOURCE: &str = include_str!("../../tests/fixtures/map-return-last-use-0318/cardinality.ko");
const EXPECTED: &[u8] =
    include_bytes!("../../tests/fixtures/map-return-last-use-0318/cardinality.stdout");

#[test]
fn single_map_operand_last_use_native_cardinality() {
    let run = emit_link_and_run("map-return-last-use.ko", SOURCE, "main");
    assert_success(&run, EXPECTED);
}

fn unit(constants: bool) {
    let provider = format!("package p\n{SOURCE}");
    let (run, _) = super::super::resource_exchange_tests::run_unit(
        &provider,
        "package q\nfun entry(): Unit { p.main() }",
        constants,
    );
    assert_success(&run, EXPECTED);
}

#[test]
fn unit_map_operand_last_use_native_basic_cardinality() {
    unit(false);
}

#[test]
fn unit_map_operand_last_use_native_constant_cardinality() {
    unit(true);
}
