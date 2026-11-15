//! Distinct-owner independent source/auxiliary capabilities and their distinct end times.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn auxiliary_reacquisition_ends_while_returned_binding_remains_live() {
    for reverse in [false, true] {
        for source_second in [false, true] {
            for same_bytes in [false, true] {
                check(reverse, source_second, false, same_bytes);
            }
        }
    }
}
#[test]
fn longer_interleaving_keeps_bounded_ab_certificate_absent() {
    check(false, true, true, false);
}
fn check(reverse: bool, source_second: bool, extra: bool, same_bytes: bool) {
    let mut sources = SourceMap::new();
    let parameters = if source_second {
        "aux:String,source:String"
    } else {
        "source:String,aux:String"
    };
    let provider_text = format!(
        "fun view({parameters}):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun later(item:String):Unit {{}}"
    );
    let (provider_id, provider) =
        unit_lower_test_support::parsed(&mut sources, "aux-provider.ko", &provider_text);
    let extra_use = if extra { ";consume(auxiliary)" } else { "" };
    let arguments = if source_second {
        "auxiliary,owner"
    } else {
        "owner,auxiliary"
    };
    let aux_text = if same_bytes {
        "aux λ\\n"
    } else {
        "other λ\\n"
    };
    let text = format!(
        "fun run():Unit {{val owner=\"aux λ\\n\";val auxiliary=\"{aux_text}\";borrow val item=view({arguments});later(auxiliary);consume(item){extra_use}}}"
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
        .flat_map(|c| c.interleaved_auxiliary_regions())
        .collect();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].uses().len(), if extra { 2 } else { 1 });
    assert_eq!(regions[0].binding_use_position(), 1);
    use lang_frontend::ownership_checking::UnitAuxiliarySourceState;
    let ids = regions[0].action_ids().collect::<Vec<_>>();
    assert!(matches!(regions[0].uses()[0].source_state(),
        UnitAuxiliarySourceState::BeforeBinding {owner, source_begin, handoff}
        if Some(owner) == regions[0].source_owner()
            && *source_begin == ids[usize::from(source_second)] && *handoff == ids[2]));
    let ((results, audits), receipts) =
        crate::ssa::refine::interleaved_aux_receipt::observe(|| {
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
    assert_eq!(receipts.len(), 2);
    for (result, audit) in results.into_iter().zip(audits) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let certified = audit
            .borrow_edges
            .contains(&(declaration("run"), declaration("view")));
        if extra {
            assert!(!certified);
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("missing/duplicate entry call receipt:")),
                "{:?}",
                audit.unsupported
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("unclassified operation: BorrowCall")),
                "{:?}",
                audit.unsupported
            );
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
        assert_eq!(audit.unsupported.len(), 1, "{:?}", audit.unsupported);
        assert!(audit.unsupported[0].starts_with("missing/duplicate entry call receipt:"));
        let f = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.instructions.len(), 17);
        assert_eq!((f.values.len(), f.places.len(), f.loans.len()), (2, 3, 4));
        use crate::ssa::model::EntityId;
        let Operation::BorrowCall {
            source, arguments, ..
        } = &f.instructions[6].operation
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
            matches!(f.instructions[7].operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==aux)
        );
        assert!(matches!(f.instructions[15].operation,Operation::BorrowEnd{loan} if loan==*source));
        let Operation::Drop { owner: aux_owner } = f.instructions[12].operation else {
            panic!("aux drop");
        };
        let Operation::Drop {
            owner: source_owner,
        } = f.instructions[16].operation
        else {
            panic!("source drop");
        };
        assert_ne!(aux_owner, source_owner);
        let Operation::RootPlace { owner } = f.instructions[8].operation else {
            panic!("later place");
        };
        assert_eq!(owner, aux_owner);
        let Operation::BorrowEnd { loan: later_loan } = f.instructions[11].operation else {
            panic!("later end");
        };
        assert_ne!(EntityId::Loan(later_loan), aux);
        assert_ne!(later_loan, *source);
    }
    for receipt in receipts {
        assert_eq!(receipt.ids, regions[0].action_ids().collect::<Vec<_>>());
        assert_eq!(receipt.instructions.len(), if extra { 19 } else { 15 });
    }
}
