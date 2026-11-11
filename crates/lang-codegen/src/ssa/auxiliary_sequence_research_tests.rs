//! Ordinary research over real A(auxiliary)/B(binding) uses; no new receipt authority.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
use std::collections::BTreeSet;

#[test]
fn ordinary_auxiliary_consumer_sequences_preserve_real_lifetimes() {
    let mut words = Vec::new();
    for len in 2..=4 {
        for bits in 1..(1usize << len) - 1 {
            words.push(
                (0..len)
                    .map(|i| if bits & (1 << i) == 0 { 'A' } else { 'B' })
                    .collect::<String>(),
            );
        }
    }
    assert_eq!(words.len(), 22);
    // All 44 unique-binding positions within the A1..8 budget, plus all
    // ten A9 positions. Keep the original multi-B words as negative evidence.
    for count in 1..=9 {
        for before in 0..=count {
            let word = format!("{}B{}", "A".repeat(before), "A".repeat(count - before));
            if !words.contains(&word) {
                words.push(word);
            }
        }
    }
    assert_eq!(words.len(), 67);
    assert_eq!(
        words
            .iter()
            .filter(|word| word.bytes().filter(|c| *c == b'B').count() == 1 && word.len() <= 9)
            .count(),
        44
    );
    for word in words {
        for source_index in 0..2 {
            let (parameters, arguments) = if source_index == 0 {
                ("source: String, auxiliary: String", "owner, auxiliary")
            } else {
                ("auxiliary: String, source: String", "auxiliary, owner")
            };
            let calls = word
                .chars()
                .map(|c| {
                    if c == 'A' {
                        "consume(auxiliary)"
                    } else {
                        "consume(item)"
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            let text = format!(
                "fun view({parameters}): borrow String from source = source\nfun consume(value: String): Unit {{}}\nfun run(): Unit {{val owner=\"source\"; val auxiliary=\"auxiliary\"; borrow val item=view({arguments}); {calls}}}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "aux-sequence.ko", &text);
            let (environment, types) = standard_environments();
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "aux-sequence.ko", source, &parsed),
                ),
                ("memory", SourceUnitInput::memory_document(source, &parsed)),
            ] {
                let inputs = [input];
                let (names, typed, on) =
                    unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
                let off = with_loan_observation(false, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .validate()
                        .unwrap()
                });
                assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
                let [catalog] = on.ownership().loan_action_observations() else {
                    panic!("one catalog")
                };
                let count = word.len() - 1;
                let qualified = word.starts_with('B')
                    && word[1..].bytes().all(|c| c == b'A')
                    && (2..=8).contains(&count);
                let regions = catalog.auxiliary_sequence_regions();
                assert_eq!(
                    regions.len(),
                    usize::from(qualified),
                    "{word}/source{source_index}/{layout}"
                );
                if let [region] = regions {
                    assert!(catalog.owns_auxiliary_sequence_region(region));
                    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(region.source_parameter(), Some(source_index));
                    assert_ne!(region.source_owner(), region.auxiliary_owner());
                    assert_eq!(region.uses().len(), count);
                    let ids = region.action_ids().collect::<Vec<_>>();
                    assert_eq!(ids.len(), 9 + 2 * count);
                    let actions = ids
                        .iter()
                        .map(|id| catalog.action(id).unwrap())
                        .collect::<Vec<_>>();
                    assert!(
                        matches!(actions[source_index],UnitLoanAction::Begin(loan) if loan.call()==region.source_call())
                    );
                    assert!(
                        matches!(actions[2],UnitLoanAction::BorrowHandoffCommitted(fact) if fact.binding()==region.binding())
                    );
                    assert!(
                        matches!(actions[7],UnitLoanAction::Cleanup{action:lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(fact),..} if fact.target()==lang_frontend::ownership_checking::UnitDropTarget::Named(region.root()))
                    );
                    for (i, used) in region.uses().iter().enumerate() {
                        assert_eq!(used.begin(), ids[8 + 2 * i]);
                        assert_eq!(used.end(), ids[9 + 2 * i]);
                        assert_eq!(Some(used.owner()), region.auxiliary_owner());
                        assert_eq!(
                            catalog.action(&used.begin()),
                            Some(&UnitLoanAction::Begin(used.loan().clone()))
                        );
                        assert!(
                            matches!(catalog.action(&used.end()),Some(UnitLoanAction::Cleanup{point,action:lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(loan)}) if loan==used.loan() && *point==lang_frontend::ownership_checking::UnitDropPoint::CallReturn(used.loan().call()))
                        );
                        assert!(
                            region.uses()[..i]
                                .iter()
                                .all(|previous| previous.loan().call() != used.loan().call()
                                    && previous.begin() != used.begin()
                                    && previous.end() != used.end())
                        );
                    }
                    assert!(
                        matches!(actions.last().unwrap(),UnitLoanAction::Cleanup{action:lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(fact),..} if Some(fact.target())==region.auxiliary_root().map(lang_frontend::ownership_checking::UnitDropTarget::Named))
                    );
                    assert!(catalog.later_auxiliary_binding_regions().is_empty());
                    assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                }

                let a_count = word.bytes().filter(|c| *c == b'A').count();
                let b_count = word.len() - a_count;
                let prefix = word.find('B').unwrap();
                let interleaved = catalog.interleaved_auxiliary_regions();
                assert_eq!(
                    interleaved.len(),
                    usize::from(b_count == 1 && prefix > 0 && a_count <= 8)
                );
                if let [region] = interleaved {
                    use lang_frontend::ownership_checking::UnitAuxiliarySourceState;
                    assert!(catalog.owns_interleaved_auxiliary_region(region));
                    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(region.binding_use_position(), prefix);
                    assert_eq!(region.source_parameter(), Some(source_index));
                    assert_eq!(region.uses().len(), a_count);
                    let ids = region.action_ids().collect::<Vec<_>>();
                    assert_eq!(ids.len(), 9 + 2 * a_count);
                    let actions = ids
                        .iter()
                        .map(|id| catalog.action(id).unwrap())
                        .collect::<Vec<_>>();
                    let source_drop = actions.iter().position(|action| matches!(action,
                        UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(fact), .. }
                        if fact.target() == lang_frontend::ownership_checking::UnitDropTarget::Named(region.root()))).unwrap();
                    let aux_drop = actions.iter().position(|action| matches!(action,
                        UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(fact), .. }
                        if Some(fact.target()) == region.auxiliary_root().map(lang_frontend::ownership_checking::UnitDropTarget::Named))).unwrap();
                    assert_eq!(aux_drop < source_drop, prefix == a_count);
                    for (j, used) in region.uses().iter().enumerate() {
                        assert_eq!(Some(used.owner()), region.auxiliary_owner());
                        let begin = ids.iter().position(|id| *id == used.begin()).unwrap();
                        let end = ids.iter().position(|id| *id == used.end()).unwrap();
                        assert_eq!(end, begin + 1);
                        assert_eq!(actions[begin], &UnitLoanAction::Begin(used.loan().clone()));
                        assert!(
                            matches!(actions[end], UnitLoanAction::Cleanup { point, action: lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(loan) }
                            if loan == used.loan() && *point == lang_frontend::ownership_checking::UnitDropPoint::CallReturn(used.loan().call()))
                        );
                        if j > 0 {
                            assert!(
                                ids.iter()
                                    .position(|id| *id == region.uses()[j - 1].end())
                                    .unwrap()
                                    < begin
                            );
                        }
                        match used.source_state() {
                            UnitAuxiliarySourceState::BeforeBinding {
                                owner,
                                source_begin,
                                handoff,
                            } => {
                                assert!(j < prefix && end < source_drop);
                                assert_eq!(Some(owner), region.source_owner());
                                assert_eq!(*source_begin, ids[source_index]);
                                assert_eq!(*handoff, ids[2]);
                            }
                            UnitAuxiliarySourceState::AfterBinding { retirement, drop } => {
                                assert!(j >= prefix && source_drop < begin);
                                assert_eq!(*drop, ids[source_drop]);
                                assert_eq!(*retirement, ids[source_drop - 1]);
                            }
                        }
                    }
                    assert!(catalog.auxiliary_sequence_regions().is_empty());
                    assert!(catalog.later_auxiliary_binding_regions().is_empty());
                    assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                }

                let entry = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "run")
                    .unwrap()
                    .id();
                let mut previous = None;
                for (setting, owned) in [("on", &on), ("off", &off)] {
                    let (
                        (((((lowered, verified), old), asap), later), sequence),
                        interleaved_receipts,
                    ) = crate::ssa::refine::interleaved_aux_receipt::observe(|| {
                        crate::ssa::refine::auxiliary_sequence_receipt::observe(|| {
                            crate::ssa::refine::later_aux_receipt::observe(|| {
                                crate::ssa::refine::distinct_aux_receipt::observe(|| {
                                    crate::ssa::refine::aux_binding_receipt::observe(|| {
                                        crate::ssa::unit_lower::observe_final_verifier(|| {
                                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                                &sources, &inputs, &names, &types, &typed, owned,
                                                entry,
                                            )
                                        })
                                    })
                                })
                            })
                        })
                    });
                    let (program, entry) = lowered.unwrap_or_else(|error| {
                        panic!("{word}/source{source_index}/{layout}/{setting}: {error:?}")
                    });
                    assert_eq!(
                        interleaved_receipts.len(),
                        usize::from(setting == "on" && b_count == 1 && prefix > 0 && a_count <= 8),
                        "new interleaved receipt"
                    );
                    assert_eq!(
                        sequence.len(),
                        usize::from(setting == "on" && qualified),
                        "new dynamic sequence receipt"
                    );
                    assert_eq!(verified.len(), 1);
                    assert!(old.is_empty());
                    assert!(asap.is_empty());
                    assert_eq!(later.len(), usize::from(setting == "on" && word == "BA"));
                    verify_program(&program).unwrap();
                    check(&program, entry, &word, source_index);
                    if let [receipt] = sequence.as_slice() {
                        let region = &regions[0];
                        assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
                        assert_eq!(
                            receipt.calls,
                            region
                                .uses()
                                .iter()
                                .map(|u| u.loan().call())
                                .collect::<Vec<_>>()
                        );
                        assert_eq!(receipt.callable, region.callable());
                        assert_eq!(receipt.caller, entry);
                        assert_eq!(receipt.binding, region.binding());
                        assert_eq!(receipt.root, region.root());
                        assert_eq!(Some(receipt.auxiliary_root), region.auxiliary_root());
                        assert_eq!(receipt.source_call, region.source_call());
                        assert_eq!(receipt.consumer_call, region.consumer_call());
                        assert_eq!(receipt.entry_statement, region.entry_statement());
                        assert_eq!(receipt.completion_statement, region.completion_statement());
                        assert_eq!(receipt.instructions.len(), 11 + 4 * count);
                        assert_eq!(receipt.entities.len(), 7 + 2 * count);
                        assert_eq!(
                            receipt.entities.iter().collect::<BTreeSet<_>>().len(),
                            receipt.entities.len()
                        );
                        let function = program.modules[0].function(entry).unwrap();
                        assert!(
                            function.blocks[0]
                                .instructions
                                .windows(11 + 4 * count)
                                .any(|w| w == receipt.instructions)
                        );
                        let actual = receipt
                            .instructions
                            .iter()
                            .map(|id| function.instruction(*id).unwrap())
                            .collect::<Vec<_>>();
                        assert!(actual.iter().all(|i| i.block == function.blocks[0].id));
                        let [
                            EntityId::Value(owner),
                            EntityId::Value(aux_owner),
                            EntityId::Place(place),
                            EntityId::Loan(source),
                            EntityId::Place(aux_place),
                            EntityId::Loan(aux_loan),
                            EntityId::Loan(result),
                        ] = &receipt.entities[..7]
                        else {
                            panic!("seven prefix identities")
                        };
                        for (offset, owner, place, loan) in [
                            (source_index * 2, owner, place, source),
                            ((1 - source_index) * 2, aux_owner, aux_place, aux_loan),
                        ] {
                            assert_eq!(
                                actual[offset].operation,
                                Operation::RootPlace { owner: *owner }
                            );
                            assert_eq!(actual[offset].results, [EntityId::Place(*place)]);
                            assert_eq!(
                                actual[offset + 1].operation,
                                Operation::BorrowBegin {
                                    place: *place,
                                    kind: LoanKind::Shared
                                }
                            );
                            assert_eq!(actual[offset + 1].results, [EntityId::Loan(*loan)]);
                        }
                        assert!(
                            matches!(&actual[4].operation,Operation::BorrowCall{source:s,arguments,..} if s==source && arguments[source_index]==EntityId::Loan(*source)&&arguments[1-source_index]==EntityId::Loan(*aux_loan))
                        );
                        assert_eq!(actual[4].results, [EntityId::Loan(*result)]);
                        assert_eq!(
                            actual[5].operation,
                            Operation::BorrowEnd { loan: *aux_loan }
                        );
                        assert!(
                            matches!(&actual[6].operation,Operation::DirectCall{arguments,..} if arguments==&[EntityId::Loan(*result)])
                        );
                        assert_eq!(actual[7].operation, Operation::BorrowEnd { loan: *result });
                        assert_eq!(actual[8].operation, Operation::BorrowEnd { loan: *source });
                        assert_eq!(actual[9].operation, Operation::Drop { owner: *owner });
                        for i in 0..count {
                            let [EntityId::Place(place), EntityId::Loan(loan)] =
                                &receipt.entities[7 + 2 * i..9 + 2 * i]
                            else {
                                panic!("use pair")
                            };
                            let offset = 10 + 4 * i;
                            assert_eq!(
                                actual[offset].operation,
                                Operation::RootPlace { owner: *aux_owner }
                            );
                            assert_eq!(actual[offset].results, [EntityId::Place(*place)]);
                            assert_eq!(
                                actual[offset + 1].operation,
                                Operation::BorrowBegin {
                                    place: *place,
                                    kind: LoanKind::Shared
                                }
                            );
                            assert_eq!(actual[offset + 1].results, [EntityId::Loan(*loan)]);
                            assert!(
                                matches!(&actual[offset+2].operation,Operation::DirectCall{arguments,..} if arguments==&[EntityId::Loan(*loan)])
                            );
                            assert_eq!(
                                actual[offset + 3].operation,
                                Operation::BorrowEnd { loan: *loan }
                            );
                        }
                        assert_eq!(
                            actual[10 + 4 * count].operation,
                            Operation::Drop { owner: *aux_owner }
                        );
                    }
                    if let [receipt] = interleaved_receipts.as_slice() {
                        let region = &interleaved[0];
                        assert_eq!(receipt.prefix, prefix);
                        let consumer = 6 + 4 * prefix + usize::from(prefix == a_count);
                        let auxiliary_drop = if prefix == a_count {
                            6 + 4 * a_count
                        } else {
                            10 + 4 * a_count
                        };
                        assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
                        assert_eq!(
                            receipt.calls,
                            region
                                .uses()
                                .iter()
                                .map(|u| u.loan().call())
                                .collect::<Vec<_>>()
                        );
                        assert_eq!(receipt.callable, region.callable());
                        assert_eq!(receipt.caller, entry);
                        assert_eq!(receipt.binding, region.binding());
                        assert_eq!(receipt.root, region.root());
                        assert_eq!(Some(receipt.auxiliary_root), region.auxiliary_root());
                        assert_eq!(receipt.source_call, region.source_call());
                        assert_eq!(receipt.consumer_call, region.consumer_call());
                        assert_eq!(receipt.entry_statement, region.entry_statement());
                        assert_eq!(receipt.completion_statement, region.completion_statement());
                        assert_eq!(receipt.instructions.len(), 11 + 4 * a_count);
                        assert_eq!(receipt.entities.len(), 7 + 2 * a_count);
                        assert_eq!(
                            receipt.entities.iter().collect::<BTreeSet<_>>().len(),
                            receipt.entities.len()
                        );
                        let function = program.modules[0].function(entry).unwrap();
                        assert!(
                            function.blocks[0]
                                .instructions
                                .windows(11 + 4 * a_count)
                                .any(|w| w == receipt.instructions)
                        );
                        let actual = receipt
                            .instructions
                            .iter()
                            .map(|id| function.instruction(*id).unwrap())
                            .collect::<Vec<_>>();
                        assert!(actual.iter().all(|i| i.block == function.blocks[0].id));
                        let [
                            EntityId::Value(owner),
                            EntityId::Value(aux_owner),
                            EntityId::Place(place),
                            EntityId::Loan(source),
                            EntityId::Place(aux_place),
                            EntityId::Loan(aux_loan),
                            EntityId::Loan(result),
                        ] = &receipt.entities[..7]
                        else {
                            panic!("seven prefix identities")
                        };
                        for (offset, owner, place, loan) in [
                            (source_index * 2, owner, place, source),
                            ((1 - source_index) * 2, aux_owner, aux_place, aux_loan),
                        ] {
                            assert_eq!(
                                actual[offset].operation,
                                Operation::RootPlace { owner: *owner }
                            );
                            assert_eq!(actual[offset].results, [EntityId::Place(*place)]);
                            assert_eq!(
                                actual[offset + 1].operation,
                                Operation::BorrowBegin {
                                    place: *place,
                                    kind: LoanKind::Shared
                                }
                            );
                            assert_eq!(actual[offset + 1].results, [EntityId::Loan(*loan)]);
                        }
                        assert!(
                            matches!(&actual[4].operation,Operation::BorrowCall{source:s,arguments,..} if s==source && arguments[source_index]==EntityId::Loan(*source)&&arguments[1-source_index]==EntityId::Loan(*aux_loan))
                        );
                        assert_eq!(actual[4].results, [EntityId::Loan(*result)]);
                        assert_eq!(
                            actual[5].operation,
                            Operation::BorrowEnd { loan: *aux_loan }
                        );
                        assert!(
                            matches!(&actual[consumer].operation,Operation::DirectCall{arguments,..} if arguments==&[EntityId::Loan(*result)])
                        );
                        assert_eq!(
                            actual[consumer + 1].operation,
                            Operation::BorrowEnd { loan: *result }
                        );
                        assert_eq!(
                            actual[consumer + 2].operation,
                            Operation::BorrowEnd { loan: *source }
                        );
                        assert_eq!(
                            actual[consumer + 3].operation,
                            Operation::Drop { owner: *owner }
                        );
                        for i in 0..a_count {
                            let [EntityId::Place(place), EntityId::Loan(loan)] =
                                &receipt.entities[7 + 2 * i..9 + 2 * i]
                            else {
                                panic!("use pair")
                            };
                            let offset = if i < prefix { 6 + 4 * i } else { 10 + 4 * i };
                            assert_eq!(
                                actual[offset].operation,
                                Operation::RootPlace { owner: *aux_owner }
                            );
                            assert_eq!(actual[offset].results, [EntityId::Place(*place)]);
                            assert_eq!(
                                actual[offset + 1].operation,
                                Operation::BorrowBegin {
                                    place: *place,
                                    kind: LoanKind::Shared
                                }
                            );
                            assert_eq!(actual[offset + 1].results, [EntityId::Loan(*loan)]);
                            assert!(
                                matches!(&actual[offset+2].operation,Operation::DirectCall{arguments,..} if arguments==&[EntityId::Loan(*loan)])
                            );
                            assert_eq!(
                                actual[offset + 3].operation,
                                Operation::BorrowEnd { loan: *loan }
                            );
                        }
                        assert_eq!(
                            actual[auxiliary_drop].operation,
                            Operation::Drop { owner: *aux_owner }
                        );
                    }

                    let output = (
                        crate::ssa::render_program(&program),
                        crate::llvm::render_verified_program(&program).unwrap(),
                    );
                    if setting == "off" {
                        assert_eq!(previous.as_ref(), Some(&output));
                    } else {
                        previous = Some(output);
                    }
                }
            }
        }
    }
}

fn check(program: &Program, entry: FunctionId, word: &str, source_index: usize) {
    let function = program.modules[0].function(entry).unwrap();
    assert_eq!(function.blocks.len(), 1);
    let calls = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::BorrowCall { .. }))
        .collect::<Vec<_>>();
    let [(borrow_index, borrow)] = calls.as_slice() else {
        panic!("one initial borrowed call")
    };
    let Operation::BorrowCall {
        source, arguments, ..
    } = &borrow.operation
    else {
        unreachable!()
    };
    assert_eq!(arguments[source_index], EntityId::Loan(*source));
    let EntityId::Loan(aux) = arguments[1 - source_index] else {
        panic!("aux loan")
    };
    let [EntityId::Loan(result)] = borrow.results.as_slice() else {
        panic!("one result")
    };
    let root = |loan| {
        let (begin_index, begin) = function
            .instructions
            .iter()
            .enumerate()
            .find(|(_, i)| i.results == [EntityId::Loan(loan)])
            .unwrap();
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            panic!("real Shared Begin")
        };
        let (place_index, producer) = function
            .instructions
            .iter()
            .enumerate()
            .find(|(_, i)| i.results == [EntityId::Place(place)])
            .unwrap();
        let Operation::RootPlace { owner } = producer.operation else {
            panic!("real root")
        };
        assert!(place_index < begin_index);
        (owner, place, place_index, begin_index)
    };
    let end = |loan| {
        let values = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| i.operation == Operation::BorrowEnd { loan })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 1);
        values[0]
    };
    let drop = |owner| {
        let values = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| i.operation == Operation::Drop { owner })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 1);
        values[0]
    };
    let (source_owner, source_place, _, _) = root(*source);
    let (aux_owner, aux_place, _, _) = root(aux);
    assert_ne!(source_owner, aux_owner);
    let aux_end = end(aux);
    assert!(*borrow_index < aux_end);
    let uses = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    assert_eq!(uses.len(), word.len());
    let mut places = BTreeSet::from([source_place, aux_place]);
    let mut loans = BTreeSet::from([*source, aux, *result]);
    let mut last_a_end = aux_end;
    let mut last_b = 0;
    for ((index, instruction), letter) in uses.iter().zip(word.chars()) {
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &instruction.operation
        else {
            panic!("plain call")
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("one actual borrow argument")
        };
        assert!(instruction.results.is_empty());
        assert!(aux_end < *index);
        if letter == 'B' {
            assert_eq!(loan, result);
            last_b = *index;
        } else {
            assert!(loans.insert(*loan), "each A acquires a fresh loan");
            let (owner, place, place_index, begin_index) = root(*loan);
            assert_eq!(owner, aux_owner);
            assert!(places.insert(place), "each A has its own real RootPlace");
            let ended = end(*loan);
            assert!(last_a_end < place_index && begin_index < *index && *index < ended);
            assert_eq!(begin_index, place_index + 1);
            assert_eq!(*index, begin_index + 1);
            assert_eq!(ended, *index + 1);
            last_a_end = ended;
        }
    }
    let result_end = end(*result);
    let source_end = end(*source);
    let source_drop = drop(source_owner);
    let aux_drop = drop(aux_owner);
    assert!(last_b < result_end && result_end < source_end && source_end < source_drop);
    assert_eq!(result_end, last_b + 1);
    assert_eq!(source_end, result_end + 1);
    assert_eq!(source_drop, source_end + 1);
    assert_eq!(aux_drop, last_a_end + 1);
    let a_last = word.rfind('A').unwrap();
    let b_last = word.rfind('B').unwrap();
    assert_eq!(aux_drop < source_drop, a_last < b_last);
    // Retirement follows the actual last use before any subsequent statement call.
    if let Some((next, _)) = uses.get(b_last + 1) {
        assert!(source_drop < *next);
    }
    if let Some((next, _)) = uses.get(a_last + 1) {
        assert!(aux_drop < *next);
    }
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
            .count(),
        2 + word.bytes().filter(|c| *c == b'A').count()
    );
}
