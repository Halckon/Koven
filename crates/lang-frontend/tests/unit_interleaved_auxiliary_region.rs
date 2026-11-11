//! Ordinary original-source capability checkpoint, never synthetic relationship assembly.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitAuxiliarySourceState, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction,
        UnitLoanAction, UnitLoanActionCoverage, check_compilation_unit_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn ordinary_interleaved_auxiliary_requires_live_and_retired_source() {
    for count in 1..=8 {
        for prefix in 1..=count {
            for second in [false, true] {
                let (parameters, arguments) = if second {
                    ("aux: String, source: String", "aux, owner")
                } else {
                    ("source: String, aux: String", "owner, aux")
                };
                let mut statements = vec!["consume(aux)"; count];
                statements.insert(prefix, "consume(item)");
                let suffix = statements.join("; ");
                let text = format!(
                    "fun view({parameters}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val aux = \"aux\"; borrow val item = view({arguments}); {suffix} }}"
                );
                let mut sources = SourceMap::new();
                let source = sources.add_source("interleaved-aux.ko", &text).unwrap();
                let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
                assert!(parsed.diagnostics().is_empty());
                let (environment, types) = standard_environments();
                for (layout, input) in [
                    (
                        "rooted",
                        SourceUnitInput::new("root", "interleaved-aux.ko", source, &parsed),
                    ),
                    ("memory", SourceUnitInput::memory_document(source, &parsed)),
                ] {
                    let inputs = [input];
                    let index = index_compilation_unit(&sources, &inputs).unwrap();
                    let names =
                        resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                            .unwrap()
                            .validate()
                            .unwrap();
                    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                        .unwrap()
                        .validate()
                        .unwrap();
                    let on =
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                            .validate()
                            .unwrap();
                    let off = with_loan_observation(false, || {
                        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                            .unwrap()
                            .validate()
                            .unwrap()
                    });
                    assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
                    assert_eq!(
                        on.ownership().owner_observations(),
                        off.ownership().owner_observations()
                    );
                    assert!(off.ownership().loan_action_observations().is_empty());
                    let [catalog] = on.ownership().loan_action_observations() else {
                        panic!("catalog")
                    };
                    assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                    let [relation] = catalog.distinct_auxiliary_retirements() else {
                        panic!("relation")
                    };
                    assert!(relation.auxiliary_drop().is_none());
                    assert_eq!(relation.source_parameter(), usize::from(second));
                    assert_eq!(
                        catalog.interleaved_auxiliary_regions().len(),
                        1,
                        "n={count}/prefix={prefix}/{layout}/second={second}: actual interleaved certificate"
                    );
                    let region = &catalog.interleaved_auxiliary_regions()[0];
                    assert!(catalog.owns_interleaved_auxiliary_region(region));
                    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(region.binding_use_position(), prefix);
                    assert_eq!(region.source_parameter(), Some(usize::from(second)));
                    assert_eq!(region.source_owner(), Some(relation.source_owner()));
                    assert_eq!(region.auxiliary_owner(), Some(relation.auxiliary_owner()));
                    assert!(catalog.auxiliary_sequence_regions().is_empty());
                    assert!(catalog.later_auxiliary_binding_regions().is_empty());
                    let ids = region.action_ids().collect::<Vec<_>>();
                    assert_eq!(ids.len(), 9 + 2 * count);
                    assert_eq!(ids[usize::from(second)], relation.source_begin());
                    assert_eq!(ids[1 - usize::from(second)], relation.auxiliary_begin());
                    assert_eq!(ids[2], relation.handoff());
                    assert_eq!(region.uses().len(), count);
                    // Independent literal event order: initial two Begins/Handoff, then
                    // A=(Begin,End,[last auxiliary Drop]), B=(Begin,Return,End,Retire,Drop).
                    let mut cursor = 3;
                    let mut a = 0;
                    let mut source_retirement = None;
                    let mut source_drop = None;
                    let mut auxiliary_drop = None;
                    for (position, statement) in statements.iter().enumerate() {
                        if position == prefix {
                            assert_eq!(*statement, "consume(item)");
                            assert!(
                                matches!(catalog.action(&ids[cursor]), Some(UnitLoanAction::Begin(loan))
                            if loan.call() == region.consumer_call())
                            );
                            assert!(
                                matches!(catalog.action(&ids[cursor + 1]), Some(UnitLoanAction::BorrowBindingUseReturned(returned))
                            if returned.binding() == region.binding() && returned.begin() == Some(ids[cursor].clone())
                            && returned.source_begin() == Some(relation.source_begin())
                            && returned.handoff() == Some(relation.handoff()))
                            );
                            assert!(
                                matches!(catalog.action(&ids[cursor + 2]), Some(UnitLoanAction::BorrowBindingEnd(end))
                            if end.binding() == region.binding())
                            );
                            assert!(
                                matches!(catalog.action(&ids[cursor + 3]), Some(UnitLoanAction::BorrowBindingSourceRetired(retired))
                            if retired.binding_end() == Some(ids[cursor + 2].clone())
                            && retired.use_return() == Some(ids[cursor + 1].clone())
                            && retired.source_begin() == Some(relation.source_begin()))
                            );
                            assert!(
                                matches!(catalog.action(&ids[cursor + 4]), Some(UnitLoanAction::Cleanup {action: UnitIterationCleanupAction::Drop(drop), ..})
                            if drop.target() == UnitDropTarget::Named(region.root()))
                            );
                            source_retirement = Some(cursor + 3);
                            source_drop = Some(cursor + 4);
                            cursor += 5;
                        } else {
                            assert_eq!(*statement, "consume(aux)");
                            let actual = &region.uses()[a];
                            assert_eq!(actual.begin(), ids[cursor]);
                            assert_eq!(actual.end(), ids[cursor + 1]);
                            assert_eq!(Some(actual.owner()), region.auxiliary_owner());
                            assert_eq!(
                                catalog.action(&ids[cursor]),
                                Some(&UnitLoanAction::Begin(actual.loan().clone()))
                            );
                            assert!(
                                matches!(catalog.action(&ids[cursor + 1]), Some(UnitLoanAction::Cleanup {point, action: UnitIterationCleanupAction::EndCallLoan(loan)})
                            if loan == actual.loan() && *point == UnitDropPoint::CallReturn(loan.call()))
                            );
                            match actual.source_state() {
                                UnitAuxiliarySourceState::BeforeBinding {
                                    owner,
                                    source_begin,
                                    handoff,
                                } => {
                                    assert!(a < prefix);
                                    assert_eq!(Some(owner), region.source_owner());
                                    assert_eq!(*source_begin, relation.source_begin());
                                    assert_eq!(*handoff, relation.handoff());
                                    assert!(source_retirement.is_none());
                                }
                                UnitAuxiliarySourceState::AfterBinding { retirement, drop } => {
                                    assert!(a >= prefix);
                                    assert_eq!(*retirement, ids[source_retirement.unwrap()]);
                                    assert_eq!(*drop, ids[source_drop.unwrap()]);
                                }
                            }
                            cursor += 2;
                            a += 1;
                            if a == count {
                                assert!(
                                    matches!(catalog.action(&ids[cursor]), Some(UnitLoanAction::Cleanup {point,action:UnitIterationCleanupAction::Drop(drop)})
                                if Some(drop.target()) == region.auxiliary_root().map(UnitDropTarget::Named)
                                && *point == UnitDropPoint::CallReturn(actual.loan().call()))
                                );
                                auxiliary_drop = Some(cursor);
                                cursor += 1;
                            }
                        }
                    }
                    assert_eq!(a, count);
                    assert_eq!(cursor, ids.len());
                    assert_eq!(
                        auxiliary_drop.unwrap() < source_drop.unwrap(),
                        prefix == count
                    );
                }
            }
        }
    }
}
