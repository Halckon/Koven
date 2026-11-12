//! A captured certificate must cover the complete supplied function, without producer plans.
use super::*;

#[test]
fn ordinary_local_loop_receipt_rejects_uncovered_actual_block() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(flag: Boolean): Unit {val source=\"value\";borrow val item=view(source);while(flag){consume(item);consume(item)}}";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "whole-loop.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "whole-loop.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let ((result, verified), receipts) = crate::ssa::refine::loop_receipt::observe(|| {
            crate::ssa::unit_lower::observe_final_verifier(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            })
        });
        let (program, _) = result.unwrap();
        assert_eq!(verified.len(), 1);
        let [receipt] = receipts.as_slice() else {
            panic!("one verified loop receipt");
        };
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == receipt.caller)
            .unwrap();
        let detached = receipt.clone();
        detached
            .validate(function)
            .expect("genuine complete function passes without Plans/current");
        let mut extended = function.clone();
        let origin = function.blocks[0].origin.clone();
        let extra = extended.add_block(Vec::new(), origin.clone()).unwrap();
        extended
            .set_terminator(extra, TerminatorKind::Return { values: Vec::new() }, origin)
            .unwrap();
        assert_eq!(extended.blocks.len(), function.blocks.len() + 1);
        assert!(
            extended.block(extra).unwrap().terminator.is_some(),
            "not a malformed missing-terminator placeholder"
        );
        assert!(
            detached.validate(&extended).is_err(),
            "certificate must reject an additional actual block outside its coverage"
        );
    }
}
