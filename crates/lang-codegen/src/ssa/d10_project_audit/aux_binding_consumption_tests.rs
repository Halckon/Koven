//! Same-owner independent source/auxiliary capabilities and their distinct end times.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn auxiliary_early_end_and_retained_source_close_the_whole_caller() {
    for reverse in [false, true] {
        for source_second in [false, true] {
            check(reverse, source_second, false);
        }
    }
}
#[test]
fn extra_consumer_is_not_closed_by_the_ten_role_window() {
    check(false, true, true);
}
fn check(reverse: bool, source_second: bool, extra: bool) {
    let mut sources = SourceMap::new();
    let parameters = if source_second {
        "aux:String,source:String"
    } else {
        "source:String,aux:String"
    };
    let provider_text = format!(
        "fun view({parameters}):borrow String from source=source\nfun consume(item:String):Unit {{}}"
    );
    let (provider_id, provider) =
        unit_lower_test_support::parsed(&mut sources, "aux-provider.ko", &provider_text);
    let extra_use = if extra { ";consume(item)" } else { "" };
    let text = format!(
        "fun run():Unit {{val owner=\"aux λ\\n\";borrow val item=view(owner,owner);consume(item){extra_use}}}"
    );
    let (caller_id, caller) = unit_lower_test_support::parsed(&mut sources, "aux-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "aux-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "aux-caller.ko", caller_id, &caller),
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
    let regions: Vec<_> = owned
        .ownership()
        .loan_action_observations()
        .iter()
        .flat_map(|c| c.auxiliary_binding_regions())
        .collect();
    assert_eq!(regions.len(), usize::from(!extra));
    let ((results, audits), receipts) = crate::ssa::refine::aux_binding_receipt::observe(|| {
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
    for (result, audit) in results.into_iter().zip(audits) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let certified = audit
            .borrow_edges
            .contains(&(declaration("run"), declaration("view")));
        if extra {
            assert!(!certified);
            assert!(!audit.unsupported.is_empty());
            continue;
        }
        assert!(certified, "{:?}", audit.unsupported);
        assert!(
            !audit
                .unsupported
                .iter()
                .any(|g| g.contains("auxiliary binding")),
            "{:?}",
            audit.unsupported
        );
        // The independent entry-call candidate still lacks its own receipt; this
        // slice closes the leaf, not that separate candidate obligation.
        assert_eq!(audit.unsupported.len(), 1, "{:?}", audit.unsupported);
        assert!(audit.unsupported[0].starts_with("missing/duplicate entry call receipt:"));
        let f = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.instructions.len(), 11);
        assert_eq!((f.values.len(), f.places.len(), f.loans.len()), (1, 2, 3));
        use crate::ssa::model::EntityId;
        let Operation::BorrowCall {
            source, arguments, ..
        } = &f.instructions[5].operation
        else {
            panic!("source call");
        };
        let aux = arguments[usize::from(!source_second)];
        assert_ne!(EntityId::Loan(*source), aux);
        assert_eq!(
            arguments[usize::from(source_second)],
            EntityId::Loan(*source)
        );
        assert!(
            matches!(f.instructions[6].operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==aux)
        );
        assert!(matches!(f.instructions[9].operation,Operation::BorrowEnd{loan} if loan==*source));
    }
    for receipt in receipts {
        assert_eq!(receipt.ids, regions[0].action_ids().collect::<Vec<_>>());
        assert_eq!(receipt.instructions.len(), 10);
    }
}
