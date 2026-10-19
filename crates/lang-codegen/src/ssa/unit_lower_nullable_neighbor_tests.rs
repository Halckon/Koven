//! Ordinary no-root-Borrow neighbor: a nullable owner remains live after a null check.

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
                println!("NO ROOT BORROW LLVM unit={}\n{llvm}", index == 1);
                Ok(())
            });
            println!("NO ROOT BORROW OUTCOME unit={} {result:?}", index == 1);
            result
        })
        .collect::<Vec<_>>();
    for result in verified {
        result.expect("ordinary null check preserves the later nullable owner return");
    }
}
