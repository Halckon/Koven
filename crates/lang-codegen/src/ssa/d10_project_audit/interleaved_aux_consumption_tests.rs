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
                check(reverse, source_second, "AB", same_bytes);
            }
        }
    }
}
#[test]
fn reacquisition_straddles_binding_retirement() {
    for reverse in [false, true] {
        for source_second in [false, true] {
            for same_bytes in [false, true] {
                check(reverse, source_second, "ABA", same_bytes);
            }
        }
    }
}
#[test]
fn other_interleavings_keep_bounded_certificate_absent() {
    check(false, true, "AAB", false);
    check(true, false, "ABAA", true);
}
fn check(reverse: bool, source_second: bool, word: &str, same_bytes: bool) {
    let mut sources = SourceMap::new();
    let parameters = if source_second {
        "aux:String,source:String"
    } else {
        "source:String,aux:String"
    };
    let provider_text = format!(
        "fun view({parameters}):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun later(item:String):Unit {{}}\nfun after(item:String):Unit {{}}"
    );
    let (provider_id, provider) =
        unit_lower_test_support::parsed(&mut sources, "aux-provider.ko", &provider_text);
    let extra = !matches!(word, "AB" | "ABA");
    let count = word.chars().filter(|c| *c == 'A').count();
    let prefix = word.find('B').unwrap();
    let mut auxiliary_index = 0;
    let uses = word
        .chars()
        .map(|c| {
            if c == 'B' {
                "consume(item)"
            } else {
                auxiliary_index += 1;
                if auxiliary_index == 1 {
                    "later(auxiliary)"
                } else {
                    "after(auxiliary)"
                }
            }
        })
        .collect::<Vec<_>>()
        .join(";");
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
        "fun run():Unit {{val owner=\"aux λ\\n\";val auxiliary=\"{aux_text}\";borrow val item=view({arguments});{uses}}}"
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
    assert_eq!(regions[0].uses().len(), count);
    assert_eq!(regions[0].binding_use_position(), prefix);
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
        assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        let f = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.instructions.len(), 13 + 4 * count);
        assert_eq!(
            (f.values.len(), f.places.len(), f.loans.len()),
            (2, 2 + count, 3 + count)
        );
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
        assert!(
            matches!(f.instructions[if count == 1 { 15 } else { 14 }].operation,Operation::BorrowEnd{loan} if loan==*source)
        );
        let Operation::Drop { owner: aux_owner } =
            f.instructions[if count == 1 { 12 } else { 20 }].operation
        else {
            panic!("aux drop");
        };
        let Operation::Drop {
            owner: source_owner,
        } = f.instructions[if count == 1 { 16 } else { 15 }].operation
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
        if count == 2 {
            use lang_frontend::ownership_checking::UnitAuxiliarySourceState;
            assert!(matches!(regions[0].uses()[1].source_state(),
                UnitAuxiliarySourceState::AfterBinding { retirement, drop, .. }
                    if *retirement == ids[8] && *drop == ids[9]));
            let Operation::RootPlace { owner } = f.instructions[16].operation else {
                panic!("post-retirement root");
            };
            assert_eq!(owner, aux_owner);
            let Operation::BorrowEnd { loan: last } = f.instructions[19].operation else {
                panic!("post-retirement end");
            };
            assert_ne!(last, later_loan);
            assert_ne!(last, *source);
            assert_ne!(EntityId::Loan(last), aux);
            assert!(
                audit
                    .direct_edges
                    .contains(&(declaration("run"), declaration("after")))
            );
            let Operation::DirectCall {
                callee: first_callee,
                ..
            } = f.instructions[10].operation
            else {
                panic!("first auxiliary call");
            };
            let Operation::DirectCall {
                callee: second_callee,
                ..
            } = f.instructions[18].operation
            else {
                panic!("second auxiliary call");
            };
            assert_ne!(first_callee, second_callee);
        }
    }
    for receipt in receipts {
        assert_eq!(receipt.ids, regions[0].action_ids().collect::<Vec<_>>());
        assert_eq!(receipt.instructions.len(), 11 + 4 * count);
    }
}
