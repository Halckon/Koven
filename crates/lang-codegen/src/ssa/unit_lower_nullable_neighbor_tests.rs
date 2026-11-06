//! Ordinary no-root-Borrow neighbor: a nullable owner remains live after a null check.

use crate::test_support::diagnostics::diagnostic;
#[test]
fn null_check_without_root_borrow_preserves_later_owner_return() {
    let source = include_str!("../../tests/fixtures/single-unit-0341/null_check_keeps_owner.ko");
    let outcomes = super::integer_range_value_tests::collect_sources(&[source.to_owned()]);
    assert_eq!(outcomes.len(), 2);
    let verified = outcomes
        .into_iter()
        .enumerate()
        .map(|(index, result)| {
            let result = result.and_then(|program| {
                super::verify::verify_program(&program)
                    .map_err(|error| format!("SSA verify: {error:?}"))?;
                let llvm = crate::llvm::render_verified_program(&program)
                    .map_err(|error| format!("LLVM: {error:?}"))?;
                diagnostic!("NO ROOT BORROW LLVM unit={}\n{llvm}", index == 1);
                Ok(())
            });
            diagnostic!("NO ROOT BORROW OUTCOME unit={} {result:?}", index == 1);
            result
        })
        .collect::<Vec<_>>();
    for result in verified {
        result.expect("ordinary null check preserves the later nullable owner return");
    }
}

#[test]
fn unrelated_nullable_when_does_not_perturb_nullable_if_in_single_and_project_pipelines() {
    let base_source = r#"class Node(val value: Int)
fun entry(own node: Node?): Node? {
    if (node != null) {
        var count: Int = 0
        while (count < 1) { count = count + 1 }
    }
    return node
}
"#;
    let sibling_when = r#"fun unrelated_when(x: Node?): Int {
    return when (x) {
        null -> 0
        else -> 1
    }
}
"#;
    let with_when_source = format!("{base_source}\n{sibling_when}");

    let outcomes_without =
        super::integer_range_value_tests::collect_sources(&[base_source.to_owned()]);
    let outcomes_with = super::integer_range_value_tests::collect_sources(&[with_when_source]);

    assert_eq!(outcomes_without.len(), 2);
    assert_eq!(outcomes_with.len(), 2);

    for (layout_idx, (res_without, res_with)) in
        outcomes_without.into_iter().zip(outcomes_with).enumerate()
    {
        let prog_without =
            res_without.unwrap_or_else(|e| panic!("layout {layout_idx} failed without: {e}"));
        let prog_with = res_with.unwrap_or_else(|e| panic!("layout {layout_idx} failed with: {e}"));

        super::verify::verify_program(&prog_without).expect("verify without");
        super::verify::verify_program(&prog_with).expect("verify with");

        let _llvm_without =
            crate::llvm::render_verified_program(&prog_without).expect("llvm without");
        let _llvm_with = crate::llvm::render_verified_program(&prog_with).expect("llvm with");

        let f_without = prog_without.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains("entry"))
            .expect("entry found in without");
        let f_with = prog_with.modules[0]
            .functions
            .iter()
            .find(|f| f.name.contains("entry"))
            .expect("entry found in with");

        assert_eq!(
            f_without.blocks.len(),
            f_with.blocks.len(),
            "blocks count matches"
        );
        assert_eq!(
            f_without.instructions.len(),
            f_with.instructions.len(),
            "instructions count matches"
        );

        let drops_without = f_without
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, super::model::Operation::Drop { .. }))
            .count();
        let drops_with = f_with
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, super::model::Operation::Drop { .. }))
            .count();
        assert_eq!(drops_without, drops_with, "drops count matches");
    }
}
