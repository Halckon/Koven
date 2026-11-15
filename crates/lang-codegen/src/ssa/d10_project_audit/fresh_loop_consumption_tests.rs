//! Normal no-carry sources, without native execution or manufactured evidence.
use super::*;
use crate::ssa::{refine::fresh_loop_receipt, unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn fresh_body_lease_closes_one_loop_across_source_and_parameter_order() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check(
                reverse_sources,
                reverse_parameters,
                "consume(item)",
                false,
                true,
            );
        }
    }
}
#[test]
fn original_multiple_consumers_and_source_segments_remain_outside_fresh_domain() {
    check(false, false, "consume(item);other(item)", false, false);
    check(false, false, "consume(item)", true, false);
}
fn check(
    reverse_sources: bool,
    reverse_parameters: bool,
    uses: &str,
    segments: bool,
    certified: bool,
) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "fresh-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}",
    );
    let parameters = if reverse_parameters {
        "flag:Boolean,source:String"
    } else {
        "source:String,flag:Boolean"
    };
    let segment = if segments { "consume(source);" } else { "" };
    let text = format!(
        "fun run({parameters}):Unit {{{segment}while(flag){{borrow val item=view(source);{uses}}};{segment}}}"
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "fresh-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "fresh-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "fresh-caller.ko", caller_id, &caller),
    ];
    if reverse_sources {
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
    let facts = owned.ownership().no_carry_loops();
    assert_eq!(facts.len(), 1);
    assert!(owned.ownership().owns_no_carry_loop(&facts[0]));
    let ((results, audits), receipts) = fresh_loop_receipt::observe(|| {
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
    assert_eq!(
        receipts.len(),
        2,
        "original route must still publish its genuine receipt"
    );
    for ((result, audit), receipt) in results.into_iter().zip(audits).zip(receipts) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipt.validate(function).unwrap();
        assert_eq!(receipt.loops.len(), 1);
        assert!(receipt.loops[0].fact.same_identity(&facts[0]));
        assert_eq!(receipt.loops[0].fact, facts[0]);
        assert!(
            function.blocks.len() >= 4,
            "entry, condition, body and zero-iteration exit"
        );
        let edge = (declaration("run"), declaration("view"));
        if certified {
            assert!(audit.borrow_edges.contains(&edge));
            assert!(
                audit
                    .direct_edges
                    .contains(&(declaration("run"), declaration("consume")))
            );
            assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        } else {
            assert!(!audit.borrow_edges.contains(&edge));
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("fresh no-carry missing/duplicate")),
                "{:?}",
                audit.unsupported
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.contains("unclassified") && g.contains("BorrowCall")),
                "{:?}",
                audit.unsupported
            );
        }
    }
}
