use super::super::{
    model::{
        Block, Edge, EntityId, Function, Operation, PlaceAccess, ScalarConstant, TerminatorKind,
    },
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey},
    source::SourceMap,
    type_checking::standard_environments,
};

#[test]
fn original_nullable_partial_owned_call_releases_once() {
    let original = include_str!(
        "../../../../lang-frontend/tests/ownership_nullable_when/fixtures/partial-own-call.ko"
    );
    for rooted in [false, true] {
        for (initializer, allocations) in [("null", &[][..]), ("Node()", &[0][..])] {
            for flag in [false, true] {
                let text = format!(
                    "{original}\nfun entry(): Unit {{ val item: Node? = {initializer}; test({flag}, item) }}"
                );
                let mut sources = SourceMap::new();
                let path = "partial-own-call.ko";
                let (source, file) = parsed(&mut sources, path, &text);
                assert_eq!(
                    sources.source_text(source).unwrap().get(..original.len()),
                    Some(original)
                );
                let inputs = [if rooted {
                    SourceUnitInput::new("root", path, source, &file)
                } else {
                    SourceUnitInput::memory_document(source, &file)
                }];
                let (env, types) = standard_environments();
                let (names, typed, owned) = analyze(&sources, &inputs, &env, &types);
                let index = names.names().index();
                let [unit] = index.source_units() else {
                    panic!("one real input")
                };
                assert_eq!(unit.source_id(), source);
                assert_eq!(matches!(unit.key(), SourceUnitKey::MemoryDocument), !rooted);
                let entry = index
                    .declarations()
                    .iter()
                    .find(|d| d.name() == "entry")
                    .unwrap()
                    .id();
                let (program, entry) = lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
                .expect("exact partial owned call lowers through Unit");
                super::super::verify::verify_program(&program).unwrap();
                let module = &program.modules[entry.module().index()];
                let find = |prefix: &str| {
                    let mut functions = module
                        .functions
                        .iter()
                        .filter(|f| sources.slice(f.origin.span()).unwrap().starts_with(prefix));
                    let function = functions.next().unwrap();
                    assert!(functions.next().is_none());
                    function
                };
                let caller = find("fun test(");
                let consume = find("fun take(");
                assert_paths(caller, consume);
                let wrapper = module.function(entry).unwrap();
                let [call]: [_; 1] = wrapper
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("the wrapper calls the exact original caller");
                let Operation::DirectCall {
                    callee, arguments, ..
                } = &call.operation
                else {
                    unreachable!()
                };
                assert_eq!(*callee, caller.id());
                let [EntityId::Loan(flag_arg), EntityId::Value(nullable_arg)] =
                    arguments.as_slice()
                else {
                    panic!("real flag and nullable owner")
                };
                let place = wrapper
                    .instructions
                    .iter()
                    .find_map(|i| {
                        if i.results.as_slice() != [EntityId::Loan(*flag_arg)] {
                            return None;
                        }
                        let Operation::BorrowBegin { place: source, .. } = i.operation else {
                            panic!("wrapper flag borrow")
                        };
                        Some(source)
                    })
                    .unwrap();
                let flag_value = wrapper
                    .instructions
                    .iter()
                    .find_map(|i| {
                        if i.results.as_slice() != [EntityId::Place(place)] {
                            return None;
                        }
                        let Operation::RootPlace { owner: value } = i.operation else {
                            panic!("wrapper flag place")
                        };
                        Some(value)
                    })
                    .unwrap();
                assert!(
                    wrapper
                        .instructions
                        .iter()
                        .any(|i| i.results.as_slice() == [EntityId::Value(flag_value)]
                            && i.operation == Operation::Constant(ScalarConstant::Boolean(flag)))
                );
                let argument_def = wrapper
                    .instructions
                    .iter()
                    .find(|i| i.results.as_slice() == [EntityId::Value(*nullable_arg)])
                    .unwrap();
                if allocations.is_empty() {
                    assert!(matches!(
                        argument_def.operation,
                        Operation::NullableNull { .. }
                    ));
                } else {
                    let Operation::NullableWrap { owner, .. } = argument_def.operation else {
                        panic!("real constructor owner is wrapped")
                    };
                    assert!(
                        wrapper
                            .instructions
                            .iter()
                            .any(|i| i.results.as_slice() == [EntityId::Value(owner)]
                                && matches!(i.operation, Operation::HeapAllocate { .. }))
                    );
                }
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                if allocations.is_empty() {
                    // The standard address ledger requires a positive allocation count.
                    super::super::unit_field_replace_tests::link_and_run(
                        &llvm
                            .replace("@malloc(", "@unexpected_malloc(")
                            .replace("@free(", "@unexpected_free("),
                        r#"
#include <stdlib.h>
void *unexpected_malloc(size_t bytes) { (void)bytes; abort(); }
void unexpected_free(void *pointer) { (void)pointer; abort(); }
"#,
                        b"",
                    );
                } else {
                    assert_success(&run_counted_allocations_in_order(&llvm, allocations), b"");
                }
            }
        }
    }
}

fn assert_paths(test: &Function, consume: &Function) {
    let entry = test.block(test.entry_block().unwrap()).unwrap();
    let ops = |block: &Block| {
        block
            .instructions
            .iter()
            .map(|id| test.instruction(*id).unwrap())
            .collect::<Vec<_>>()
    };
    let block = |edge: &Edge| test.block(edge.target).unwrap();
    let carried = |edge: &Edge, entity: EntityId| {
        let mapped: Vec<_> = edge
            .arguments
            .iter()
            .zip(&block(edge).parameters)
            .filter_map(|(argument, parameter)| (*argument == entity).then_some(*parameter))
            .collect();
        let [mapped] = mapped.as_slice() else {
            panic!("identity crosses actual edge once")
        };
        *mapped
    };
    let [flag @ EntityId::Loan(_), EntityId::Value(parameter)] = entry.parameters.as_slice() else {
        panic!("original function parameters")
    };
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &entry.terminator.as_ref().unwrap().kind
    else {
        panic!("actual nullable split")
    };
    assert_eq!(owner, parameter);
    assert!(ops(entry).is_empty());
    let EntityId::Value(empty) = carried(when_null, EntityId::Value(*owner)) else {
        panic!("empty owner")
    };
    let [drop]: [_; 1] = ops(block(when_null))
        .try_into()
        .expect("only the empty wrapper is retired");
    assert_eq!(drop.operation, Operation::Drop { owner: empty });

    let non_null = block(when_non_null);
    let [read]: [_; 1] = ops(non_null)
        .try_into()
        .expect("only read the original flag");
    let EntityId::Loan(flag) = carried(when_non_null, *flag) else {
        panic!("same flag")
    };
    assert_eq!(
        read.operation,
        Operation::Read {
            source: PlaceAccess::Loan(flag)
        }
    );
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &non_null.terminator.as_ref().unwrap().kind
    else {
        panic!("actual flag split")
    };
    assert_eq!(read.results.as_slice(), [EntityId::Value(*condition)]);
    let owner = carried(when_non_null, EntityId::Value(*owner));
    let proof = EntityId::Loan(*view);
    let EntityId::Value(moved) = carried(when_true, owner) else {
        panic!("true owner")
    };
    let EntityId::Loan(true_proof) = carried(when_true, proof) else {
        panic!("same true proof")
    };
    let [take, call]: [_; 2] = ops(block(when_true))
        .try_into()
        .expect("only extraction and ownership handoff");
    assert_eq!(
        take.operation,
        Operation::NullableTake {
            owner: moved,
            proof: true_proof
        }
    );
    let [result @ EntityId::Value(_)] = take.results.as_slice() else {
        panic!("exact moved payload")
    };
    assert_eq!(
        call.operation,
        Operation::DirectCall {
            callee: consume.id(),
            receiver: None,
            arguments: vec![*result]
        }
    );
    let callee = consume.block(consume.entry_block().unwrap()).unwrap();
    let [EntityId::Value(transferred)] = callee.parameters.as_slice() else {
        panic!("callee owns node")
    };
    let [only_drop] = consume.instructions.as_slice() else {
        panic!("callee releases the transferred owner")
    };
    assert_eq!(
        only_drop.operation,
        Operation::Drop {
            owner: *transferred
        }
    );

    let EntityId::Loan(false_proof) = carried(when_false, proof) else {
        panic!("same false proof")
    };
    let EntityId::Value(kept) = carried(when_false, owner) else {
        panic!("same live owner on skipped call")
    };
    let [end, drop]: [_; 2] = ops(block(when_false))
        .try_into()
        .expect("skip call, retire proof, drop original owner");
    assert_eq!(end.operation, Operation::BorrowEnd { loan: false_proof });
    assert_eq!(drop.operation, Operation::Drop { owner: kept });
    assert_eq!(
        test.instructions.len(),
        6,
        "all ownership events are on the witnessed edges"
    );
}
