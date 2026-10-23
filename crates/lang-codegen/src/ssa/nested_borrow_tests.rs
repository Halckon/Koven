//! SPEC-0307: query verified SSA from ordinary source, without IR mutation.
use super::{
    borrow_memory_test_support::single, borrow_storage_contract_tests::unit, model::*,
    verify::verify_program,
};
use crate::native_nested_borrow_cases::Case;
use std::collections::BTreeSet;

fn producer(function: &Function, entity: EntityId) -> Option<&Instruction> {
    match function.entity(entity).unwrap().definition {
        Definition::InstructionResult { instruction, .. } => function.instruction(instruction),
        Definition::BlockParameter { .. } => None,
    }
}

fn end(function: &Function, loan: LoanId) -> usize {
    let ends = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ends.len(),
        1,
        "each caller loan ends exactly once: {loan:?}"
    );
    ends[0]
}

fn parent(function: &Function, loan: LoanId) -> Option<LoanId> {
    match producer(function, EntityId::Loan(loan))?.operation {
        Operation::BorrowCall { source, .. } | Operation::SharedReborrow { source } => Some(source),
        Operation::SharedFieldLoan { base, .. } | Operation::SharedHeapFieldLoan { base, .. } => {
            Some(base)
        }
        _ => None,
    }
}

fn owner(function: &Function, mut loan: LoanId) -> Option<ValueId> {
    let mut seen = BTreeSet::new();
    while let Some(source) = parent(function, loan) {
        assert!(seen.insert(loan), "loan ancestry is acyclic");
        loan = source;
    }
    let Operation::BorrowBegin { place, .. } = producer(function, EntityId::Loan(loan))?.operation
    else {
        return None;
    };
    match producer(function, EntityId::Place(place))?.operation {
        Operation::RootPlace { owner } => Some(owner),
        _ => None,
    }
}

fn check_different_outer_lifetimes(module: &Module, entry: &Function, asap: bool) {
    let borrowed = entry
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            let Operation::BorrowCall { source, .. } = instruction.operation else {
                return None;
            };
            let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                unreachable!()
            };
            Some((index, *result, source))
        })
        .collect::<Vec<_>>();
    let [first, second] = borrowed.as_slice() else {
        panic!("two borrowed results")
    };
    let direct = |name: &str| {
        let calls = entry
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                let Operation::DirectCall {
                    callee,
                    ref arguments,
                    ..
                } = instruction.operation
                else {
                    return None;
                };
                module
                    .function(callee)
                    .unwrap()
                    .name
                    .contains(name)
                    .then_some((index, arguments))
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1, "exactly one {name} consumer");
        calls[0]
    };
    let (count_index, count_arguments) = direct("count");
    let (pair_index, pair_arguments) = direct("pair");
    assert_eq!(count_arguments.as_slice(), [EntityId::Loan(second.1)]);
    assert_eq!(pair_arguments[0], EntityId::Loan(first.1));
    assert!(
        first.0 < second.0 && second.0 < count_index && count_index < pair_index,
        "source evaluation order is independent of the two consumer lifetimes"
    );
    assert!(count_index < end(entry, second.1) && end(entry, second.1) < pair_index);
    assert!(pair_index < end(entry, first.1));
    for (result, source, consumer, next_consumer) in [
        (second.1, second.2, count_index, Some(pair_index)),
        (first.1, first.2, pair_index, None),
    ] {
        let mut child = result;
        let mut ancestor = Some(source);
        while let Some(source) = ancestor {
            assert!(
                producer(entry, EntityId::Loan(source)).is_some(),
                "these source chains are newly created"
            );
            assert!(consumer < end(entry, child) && end(entry, child) < end(entry, source));
            if let Some(next) = next_consumer {
                assert!(
                    end(entry, source) < next,
                    "count's source chain ends before pair consumes its own result"
                );
            }
            child = source;
            ancestor = parent(entry, source);
        }
    }
    let root = owner(entry, first.1).unwrap();
    assert_eq!(
        owner(entry, second.1),
        Some(root),
        "both results protect the same owner"
    );
    if asap {
        let drops = entry
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                matches!(instruction.operation, Operation::Drop { owner } if owner == root)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1, "the last shared owner is cleaned once");
        assert!(
            end(entry, first.2) < drops[0] && end(entry, second.2) < drops[0],
            "the earlier count boundary cannot drop the owner retained by pair"
        );
    } else {
        let (consume_index, arguments) = direct("consume");
        assert_eq!(arguments.as_slice(), [EntityId::Value(root)]);
        assert!(end(entry, first.2) < consume_index && end(entry, second.2) < consume_index);
    }
}

fn check(program: &Program, case: &Case) {
    verify_program(program).unwrap();
    crate::llvm::render_verified_program(program).unwrap();
    let module = &program.modules[0];
    let name = if case.name == "borrow_entry_source" {
        "inspect"
    } else {
        "entry"
    };
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap();
    assert_eq!(entry.blocks.len(), 1, "{}: straight-line caller", case.name);
    let mut delivered = 0;
    for (index, instruction) in entry.instructions.iter().enumerate() {
        for entity in &instruction.results {
            let EntityId::Loan(loan) = entity else {
                continue;
            };
            assert!(index < end(entry, *loan));
            if let Some(source) = parent(entry, *loan)
                && producer(entry, EntityId::Loan(source)).is_some()
            {
                assert!(
                    end(entry, *loan) < end(entry, source),
                    "{}: child before ancestor",
                    case.name
                );
            }
            if let Some(root) = owner(entry, *loan) {
                for (cleanup, operation) in entry.instructions.iter().enumerate() {
                    if matches!(operation.operation, Operation::Drop { owner } if owner == root) {
                        assert!(
                            end(entry, *loan) < cleanup,
                            "{}: all associated loans end before owner drop",
                            case.name
                        );
                    }
                }
            }
        }
        let Operation::BorrowCall { source, .. } = instruction.operation else {
            continue;
        };
        let [EntityId::Loan(result)] = instruction.results.as_slice() else {
            panic!("borrowed storage is a Loan")
        };
        for (consumer, call) in entry.instructions.iter().enumerate() {
            let Operation::DirectCall {
                callee,
                ref arguments,
                ..
            } = call.operation
            else {
                continue;
            };
            if !arguments.contains(&EntityId::Loan(*result)) {
                continue;
            }
            assert!(module.function(callee).unwrap().borrow_return.is_none());
            assert!(index < consumer && consumer < end(entry, *result));
            if producer(entry, EntityId::Loan(source)).is_some() {
                assert!(
                    consumer < end(entry, source),
                    "source remains live through consumer"
                );
            }
            // A source nesting relation distinguishes direct consumption from explicit bindings.
            let inner = instruction.origin.span();
            let outer = call.origin.span();
            if inner.source_id() == outer.source_id()
                && outer.start() <= inner.start()
                && inner.end() <= outer.end()
            {
                delivered += 1;
                if producer(entry, EntityId::Loan(source)).is_some() {
                    assert!(end(entry, *result) < end(entry, source));
                }
            }
        }
    }
    assert_eq!(
        delivered > 0,
        case.nested,
        "{}: nested result reaches its DirectCall without an owned conversion",
        case.name
    );
    if matches!(
        case.name,
        "different_outer_lifetimes" | "different_outer_lifetimes_asap"
    ) {
        check_different_outer_lifetimes(module, entry, case.name.ends_with("_asap"));
    }
    if case.name == "borrow_entry_source" {
        let [EntityId::Loan(parameter)] = entry.blocks[0].parameters.as_slice() else {
            panic!("inspect receives a Borrow entry parameter");
        };
        assert!(
            !entry.instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *parameter)
            }),
            "the short call cannot end its caller's entry permission"
        );
        let (result, source) = entry
            .instructions
            .iter()
            .find_map(|instruction| {
                let Operation::BorrowCall { source, .. } = instruction.operation else {
                    return None;
                };
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    unreachable!()
                };
                Some((*result, source))
            })
            .unwrap();
        assert_eq!(
            source, *parameter,
            "the result reuses the actual Borrow entry loan"
        );
        let read = entry.instructions.iter().position(|instruction| {
            matches!(instruction.operation, Operation::PrintString { value } if value == *parameter)
        }).unwrap();
        assert!(
            end(entry, result) < read,
            "the entry permission remains readable after the short result ends"
        );
    }
    if matches!(
        case.name,
        "field_projection" | "resource_lexical_source" | "wrapper_field_payload"
    ) {
        let view = module
            .functions
            .iter()
            .find(|function| function.name.contains("view"))
            .unwrap();
        let returned = view.blocks[0].terminator.as_ref().unwrap();
        let TerminatorKind::BorrowReturn { loan } = returned.kind else {
            panic!("borrowed field return")
        };
        let Operation::SharedHeapFieldLoan { base, field } =
            producer(view, EntityId::Loan(loan)).unwrap().operation
        else {
            panic!("the actual returned storage is a field projection");
        };
        assert_eq!(field, 0, "Source.text is the projected field");
        assert_ne!(
            view.entity(EntityId::Loan(base)).unwrap().ty,
            view.entity(EntityId::Loan(loan)).unwrap().ty,
            "the source lease and String payload have different storage types"
        );
    }
    if case.name == "reuse_existing_borrow" {
        let borrowed = entry
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::BorrowCall { source, .. } = instruction.operation {
                    let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                        unreachable!()
                    };
                    Some((*result, source))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(borrowed.len(), 2);
        assert_eq!(
            borrowed[1].1, borrowed[0].0,
            "reuse the external borrowed storage"
        );
        let read = entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation,
            Operation::PrintString { value } if value == borrowed[0].0)
            })
            .unwrap();
        assert!(
            end(entry, borrowed[1].0) < read && read < end(entry, borrowed[0].0),
            "external parent survives the short use"
        );
    }
}

#[test]
fn nested_borrow_argument_single_source_matrix() {
    for case in crate::native_nested_borrow_cases::cases() {
        let program =
            single(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        check(&program, &case);
    }
}

#[test]
fn nested_borrow_argument_unit_source_matrix() {
    for case in crate::native_nested_borrow_cases::cases() {
        let program = unit(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        check(&program, &case);
    }
}
