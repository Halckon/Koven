//! Ordinary original-source evidence; no fact rewriting or forced routing.
use super::*;
#[test]
fn original_binding_lifecycle_records_real_actions_and_ssa() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "binding-lifecycle.ko", text);
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    println!(
        "actions={:#?}",
        owned.ownership().loan_action_observations()
    );
    println!("handoffs={:#?}", owned.ownership().borrow_call_handoffs());
    println!(
        "transports={:#?}",
        owned.ownership().borrow_binding_transports()
    );
    println!(
        "bindings={:#?}",
        owned.ownership().borrow_results().bindings()
    );
    println!(
        "binding_ends={:#?}",
        owned.ownership().borrow_results().ends()
    );
    let run = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap();
    let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            run.id(),
        )
    });
    let (program, entry) = lowered.expect("original ordinary binding source lowers");
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    println!("routes={routes:?}; entry={entry:?}");
    println!("{}", crate::ssa::render_program(&program));
}
