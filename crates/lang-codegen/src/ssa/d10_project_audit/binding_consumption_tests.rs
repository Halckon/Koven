//! Real complete binding inputs, including an ordinary uncovered extra-consumer neighbor.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn complete_binding_closes_zero_parameter_entry_to_identity_and_forward() {
    for reverse in [false, true] {
        for forward in [false, true] {
            check(reverse, forward, false);
        }
    }
}
#[test]
fn extra_consumer_is_not_certified_by_a_seven_instruction_window() {
    check(false, true, true);
}
fn check(reverse: bool, forward: bool, extra: bool) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "binding-provider.ko",
        "fun identity(source:String):borrow String from source=source\nfun forward(source:String):borrow String from source=identity(source)",
    );
    let callee = if forward { "forward" } else { "identity" };
    let extra_use = if extra { ";consume(item)" } else { "" };
    let text = format!(
        "fun consume(item:String):Unit {{}}\nfun run():Unit {{val owner=\"kept λ\\n\";borrow val item={callee}(owner);consume(item){extra_use}}}"
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "binding-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "binding-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "binding-caller.ko", caller_id, &caller),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let originals: Vec<_> = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|c| c.binding_regions().iter())
        .collect();
    assert_eq!(originals.len(), usize::from(!extra));
    let ((programs, audits), receipts) = crate::ssa::refine::binding_receipt::observe(|| {
        observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(audits.len(), 2);
    assert_eq!(receipts.len(), if extra { 0 } else { 2 });
    if !extra {
        for receipt in &receipts {
            assert_eq!(receipt.ids, originals[0].action_ids().collect::<Vec<_>>());
            assert_eq!(receipt.instructions.len(), 7);
        }
    }
    for (result, audit) in programs.into_iter().zip(audits) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        if extra {
            assert!(!audit.unsupported.is_empty());
            assert!(
                !audit
                    .borrow_edges
                    .contains(&(declaration("run"), declaration(callee)))
            );
            continue;
        }
        assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        assert!(
            audit
                .borrow_edges
                .contains(&(declaration("run"), declaration(callee)))
        );
        assert!(
            audit
                .direct_edges
                .contains(&(declaration("run"), declaration("consume")))
        );
        if forward {
            assert!(
                audit
                    .borrow_edges
                    .contains(&(declaration("forward"), declaration("identity")))
            );
        }
        let caller = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        assert_eq!(caller.instructions.len(), 8);
        assert_eq!(caller.values.len(), 1);
        assert_eq!(caller.places.len(), 1);
        assert_eq!(caller.loans.len(), 2);
    }
}
