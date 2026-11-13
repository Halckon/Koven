//! Same-source Unit coverage for the retired class/Box and String binary legacy tests.
//! Selected-root programs are not whole-file equivalence;
//! LLVM verification below is not native execution or allocation/free evidence.
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{
    model::{EntityId, EntityType, LoanKind, Operation, Origin, TerminatorKind},
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument, no wrapper or source split");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

#[test]
fn repeated_class_and_box_root_borrows_unit_comparison() {
    let text = SOURCE_CLASS;
    let unit = analyze(text);
    let declaration = unit.declaration("exercise", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(
        unit.sources().slice(span),
        Ok(&text[text.find("fun exercise").unwrap()..])
    );
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original exercise Unit root");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("root function");
    assert_eq!(function.origin, Origin::Source(span));
    assert!(!function.blocks.is_empty());
    let mut targets = Vec::new();
    for (name, source) in [
        ("inspect", "fun inspect(resource: Resource): Unit {}"),
        (
            "inspectBox",
            "fun inspectBox(resource: Box<Token>): Unit {}",
        ),
    ] {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        assert_eq!(unit.sources().slice(span), Ok(source));
        let callees = module
            .functions
            .iter()
            .filter(|callee| callee.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [callee] = callees.as_slice() else {
            panic!("one exact source callee");
        };
        let [parameter @ EntityId::Loan(_)] = callee.blocks[0].parameters.as_slice() else {
            panic!("one original Borrow parameter");
        };
        assert!(matches!(
            callee.entity(*parameter).expect("parameter").ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        targets.push(callee.id);
    }
    assert_ne!(targets[0], targets[1]);
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &instruction.operation
            {
                assert!(receiver.is_none());
                assert!(matches!(arguments.as_slice(), [EntityId::Loan(_)]));
                Some(*callee)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(calls, [targets[0], targets[0], targets[1], targets[1]]);
    let [block] = function.blocks.as_slice() else {
        panic!("straight-line root borrows stay in one execution block");
    };
    let operations = block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).expect("block instruction"))
        .collect::<Vec<_>>();
    let allocations = operations
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| match instruction.operation {
            Operation::HeapAllocate { .. } => Some((index, instruction.results[0])),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [
        (resource_at, EntityId::Value(resource)),
        (boxed_at, EntityId::Value(boxed)),
    ] = allocations.as_slice()
    else {
        panic!("Resource and Box each allocate one distinct root owner");
    };
    assert_ne!(resource, boxed);
    let mut call_index = 0;
    for (call_at, instruction) in operations.iter().enumerate() {
        let Operation::DirectCall {
            callee, arguments, ..
        } = &instruction.operation
        else {
            continue;
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("root loan")
        };
        assert_eq!(
            *callee,
            targets[call_index / 2],
            "callee order in execution block"
        );
        let (allocation_at, owner) = if call_index < 2 {
            (*resource_at, *resource)
        } else {
            (*boxed_at, *boxed)
        };
        let (borrow_at, place) = operations
            .iter()
            .enumerate()
            .find_map(|(index, instruction)| match instruction.operation {
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } if instruction.results == [EntityId::Loan(*loan)] => Some((index, place)),
                _ => None,
            })
            .expect("exact call loan is defined by a shared root borrow");
        let root_at = operations
            .iter()
            .position(|instruction| {
                instruction.operation == Operation::RootPlace { owner }
                    && instruction.results == [EntityId::Place(place)]
            })
            .expect("both calls borrow their actual Resource or Box owner");
        let end_at = operations
            .iter()
            .position(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
            .expect("exact loan ends in the same block");
        assert!(allocation_at < root_at && root_at < borrow_at && borrow_at < call_at);
        assert_eq!(
            end_at,
            call_at + 1,
            "call ends its own loan before later work"
        );
        call_index += 1;
    }
    assert_eq!(call_index, 4);
    verify_program(&program).expect("complete original Unit SSA verifies");
    let ssa = render_program(&program);
    for operation in ["root_place", "borrow.shared", "end_borrow"] {
        assert_eq_diagnostic!(ssa.matches(operation).count(), 4, "{operation}: {ssa}");
    }
    // Preserve the two per-callee call-count oracles without legacy @f0/@f1 assumptions.
    for target in targets {
        assert_eq_diagnostic!(
            ssa.matches(&format!("call @f{}(", target.index())).count(),
            2,
            "{ssa}"
        );
    }
    assert_diagnostic!(!ssa.contains("shared.payload_place"), "{ssa}");
    render_verified_program(&program).expect("original Unit LLVM verifies");
}

#[test]
fn string_binary_views_value_calls_and_returns_unit_comparison() {
    let text = SOURCE_STRING;
    let unit = analyze(text);
    let declaration = unit.declaration("combine", SymbolKind::Function);
    let span = unit.declaration_span(declaration);
    assert_eq!(
        unit.sources().slice(span),
        Ok(&text[text.find("fun combine").unwrap()..])
    );
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("original combine Unit root");
    let module = program.module(root.module()).expect("root module");
    let combine = module.function(root).expect("exact combine function");
    assert_eq!(combine.origin, Origin::Source(span));
    let [left @ EntityId::Loan(left_loan), right @ EntityId::Value(_)] =
        combine.blocks[0].parameters.as_slice()
    else {
        panic!("original left Borrow and own right Value parameters");
    };
    let identity = unit.declaration("identity", SymbolKind::Function);
    let identity_span = unit.declaration_span(identity);
    assert_eq!(
        unit.sources().slice(identity_span),
        Ok("fun identity(own input: String): String = input")
    );
    let callees = module
        .functions
        .iter()
        .filter(|callee| callee.origin == Origin::Source(identity_span))
        .collect::<Vec<_>>();
    let [callee] = callees.as_slice() else {
        panic!("one exact identity declaration");
    };
    assert!(matches!(
        callee.blocks[0].parameters.as_slice(),
        [EntityId::Value(_)]
    ));
    let calls = combine
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee: target,
                receiver,
                arguments,
            } if *target == callee.id => Some((receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(receiver, arguments)] = calls.as_slice() else {
        panic!("one actual identity call");
    };
    assert!(receiver.is_none());
    assert!(matches!(arguments.as_slice(), [EntityId::Value(_)]));
    assert_eq!(
        arguments.as_slice(),
        &[*right],
        "identity receives the original owned right"
    );
    let [block] = combine.blocks.as_slice() else {
        panic!("straight-line String fixture stays in one execution block");
    };
    let operations = block
        .instructions
        .iter()
        .map(|id| combine.instruction(*id).expect("block instruction"))
        .collect::<Vec<_>>();
    let (literal_at, literal) = operations.iter().enumerate().find(|(_, instruction)| {
        matches!(&instruction.operation, Operation::StringLiteral { bytes, .. } if bytes == b"!")
    }).expect("original suffix literal");
    let [EntityId::Value(suffix)] = literal.results.as_slice() else {
        panic!("owned literal")
    };
    let (concat_at, concat) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            instruction.operation
                == Operation::StringConcat {
                    left: *left,
                    right: EntityId::Value(*suffix),
                }
        })
        .expect("concat reads the actual entry loan and literal result");
    let drop_at = operations
        .iter()
        .position(|instruction| instruction.operation == Operation::Drop { owner: *suffix })
        .expect("exact suffix owner is dropped");
    assert!(literal_at < concat_at && concat_at < drop_at);
    let first_drop = operations
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .unwrap();
    assert!(concat_at < first_drop);
    let (equal_at, equal) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            instruction.operation
                == Operation::StringEqual {
                    left: concat.results[0],
                    right: *right,
                }
        })
        .expect("comparison reads the concat result and original right");
    let EntityId::Value(equal_result) = equal.results[0] else {
        panic!("Boolean result")
    };
    let not_at = operations
        .iter()
        .position(|instruction| {
            instruction.operation
                == Operation::BooleanNot {
                    operand: equal_result,
                }
        })
        .expect("inequality negates the exact comparison result");
    let (call_at, call) = operations
        .iter()
        .enumerate()
        .find(|(_, instruction)| {
            matches!(&instruction.operation, Operation::DirectCall { callee: target, arguments, .. }
            if *target == callee.id && arguments == &[*right])
        })
        .expect("identity receives the same owned right after comparison");
    assert!(drop_at < equal_at && equal_at < not_at && not_at < call_at);
    let [EntityId::Value(result)] = call.results.as_slice() else {
        panic!("identity result")
    };
    assert!(matches!(&block.terminator.as_ref().expect("return").kind,
        TerminatorKind::Return { values } if values == &[*result]));
    verify_program(&program).expect("complete original Unit SSA verifies");
    let ssa = render_program(&program);
    assert_diagnostic!(
        ssa.contains(&format!(
            "string.concat %l{}, %v{}",
            left_loan.index(),
            suffix.index()
        )),
        "{ssa}"
    );
    // Bind the rendered concat to the actual entry loan, never a legacy %l0 assumption.
    for operation in ["string.concat", "string.equal", "not ", "call @"] {
        assert_diagnostic!(ssa.contains(operation), "{operation}: {ssa}");
    }
    render_verified_program(&program).expect("original Unit LLVM verifies");
}

const SOURCE_CLASS: &str = "class Resource {}\n\
         value class Token(val item: Int)\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun inspectBox(resource: Box<Token>): Unit {}\n\
         fun exercise(): Unit {\n\
             val resource = Resource()\n\
             val first = inspect(resource)\n\
             val second = inspect(resource)\n\
             val boxed = Box(Token(1))\n\
             val third = inspectBox(boxed)\n\
             val fourth = inspectBox(boxed)\n\
         }";

const SOURCE_STRING: &str = "fun identity(own input: String): String = input\n\
         fun combine(left: String, own right: String): String {\n\
             val joined = left + \"!\"\n\
             val differs = joined != right\n\
             return identity(right)\n\
         }";

pub(in crate::ssa) mod nested_borrow_tests {
    //! SPEC-0307: query verified SSA from ordinary source, without IR mutation.
    use super::super::{
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
            Operation::BorrowCall { source, .. } | Operation::SharedReborrow { source } => {
                Some(source)
            }
            Operation::SharedFieldLoan { base, .. }
            | Operation::SharedHeapFieldLoan { base, .. } => Some(base),
            _ => None,
        }
    }

    fn owner(function: &Function, mut loan: LoanId) -> Option<ValueId> {
        let mut seen = BTreeSet::new();
        while let Some(source) = parent(function, loan) {
            assert!(seen.insert(loan), "loan ancestry is acyclic");
            loan = source;
        }
        let Operation::BorrowBegin { place, .. } =
            producer(function, EntityId::Loan(loan))?.operation
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
                        if matches!(operation.operation, Operation::Drop { owner } if owner == root)
                        {
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
            let program =
                unit(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
            check(&program, &case);
        }
    }
}
