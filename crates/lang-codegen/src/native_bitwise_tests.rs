use super::emit_link_and_run;

#[test]
fn bitwise_native_width_boundaries_masked_counts_and_eager_operands() {
    let (functions, checks, expected) = crate::bitwise_test_support::binary_fixture("");
    let source = format!("{functions}\nfun entry(): Unit {{\n{checks}\n}}\n");
    let run = emit_link_and_run("bitwise.ko", &source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_inv_native_boundaries_and_single_evaluation() {
    let (functions, checks, expected) = crate::bitwise_test_support::inv_fixture("");
    let source = format!("{functions}\nfun entry(): Unit {{\n{checks}\n}}\n");
    let run = emit_link_and_run("integer-inv.ko", &source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_inv_native_literal_chains_and_last_use_fields() {
    let source = format!(
        "{}\nfun entry(): Unit {{ exercise() }}",
        crate::bitwise_test_support::inv_receiver_fixture()
    );
    let run = emit_link_and_run("inv-receivers.ko", &source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        crate::bitwise_test_support::INV_RECEIVER_STDOUT.as_bytes()
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_native_guide_litmus_12_runs_the_normative_source() {
    let run = emit_link_and_run(
        "guide-litmus-12.ko",
        crate::bitwise_test_support::guide_litmus_12(),
        "main",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"Mask initialized\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_native_operands_preserve_control_exit_and_order() {
    let (functions, checks, expected) = crate::bitwise_test_support::control_fixture();
    let source = format!("{functions}\nfun entry(): Unit {{ {checks} }}");
    let run = emit_link_and_run("bitwise-control.ko", &source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn bitwise_native_const_and_runtime_agree_with_independent_oracles() {
    let (functions, checks, expected) = crate::bitwise_test_support::constant_runtime_fixture("");
    let source = format!("{functions}\nfun entry(): Unit {{\n{checks}\n}}");
    let run = emit_link_and_run("const-runtime-bitwise.ko", &source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}
