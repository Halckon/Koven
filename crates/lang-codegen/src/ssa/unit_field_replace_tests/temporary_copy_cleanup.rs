//! Copy a temporary class field, then consume the existing parent cleanup fact.
use super::*;
use crate::ssa::{model::EntityId, refine, verify::verify_program};

#[test]
fn direct_constructor_field_copy_reuses_refine_cleanup() {
    for receiver in ["Holder(7)", "(Holder(7))", "((Holder(7)))"] {
        for deinit in ["", " { deinit() {} }"] {
            let source = format!(
                "class Holder(val value: Int){deinit}\nfun entry(): Int = {receiver}.value"
            );
            for constants in [false, true] {
                let (result, routed) = refine::observe_routing(|| lower(&source, constants));
                let (program, _) = result.expect("complete constructor field facts lower");
                assert!(
                    routed
                        .iter()
                        .any(|name| name.starts_with("koven.test.entry.d")),
                    "{routed:?}"
                );
                assert_copy_cleanup(&program, false);
            }
        }
    }
}

#[test]
fn compatibility_route_remains_closed() {
    for constants in [false, true] {
        let source = "class Holder(val value: Int)\nfun entry(): Int = Holder(7).value";
        let result = refine::with_per_family_lowering(|| lower(source, constants));
        assert_eq!(
            result
                .err()
                .expect("compatibility route remains unsupported")
                .kind,
            LoweringErrorKind::UnsupportedNode
        );
    }
}

fn assert_copy_cleanup(program: &Program, factory: bool) {
    verify_program(program).unwrap();
    let function = entry(program);
    let allocations = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            if factory {
                matches!(i.operation, Operation::DirectCall { .. })
            } else {
                matches!(i.operation, Operation::HeapAllocate { .. })
            }
        })
        .collect::<Vec<_>>();
    let [(allocation_index, allocation)] = allocations.as_slice() else {
        panic!("one constructor evaluation");
    };
    let [EntityId::Value(owner)] = allocation.results.as_slice() else {
        panic!("heap owner");
    };
    let reads = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, instruction)| match instruction.operation {
            Operation::HeapFieldRead { receiver, .. } => Some((i, receiver)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(read, loan)] = reads.as_slice() else {
        panic!("one actual field read");
    };
    let end = function
        .instructions
        .iter()
        .position(|instruction| instruction.operation == Operation::BorrowEnd { loan: *loan })
        .unwrap();
    let drops = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| instruction.operation == Operation::Drop { owner: *owner })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("exactly one cleanup of constructed parent");
    };
    assert!(allocation_index < read && *read < end && end < *drop);
    crate::llvm::render_verified_program(program).unwrap();
}

#[test]
fn owned_factory_field_copy_reuses_call_and_result_cleanup() {
    for receiver in ["make(7)", "(make(7))", "((make(7)))"] {
        for deinit in ["", " { deinit() {} }"] {
            let source = format!(
                "class Holder(val value: Int){deinit}\nfun make(own n: Int): Holder = Holder(n)\nfun entry(): Int = {receiver}.value"
            );
            for constants in [false, true] {
                let (result, routed) = refine::observe_routing(|| lower(&source, constants));
                let (program, _) = result.expect("owned factory result lowers");
                assert!(
                    routed
                        .iter()
                        .any(|name| name.starts_with("koven.test.entry.d")),
                    "{routed:?}"
                );
                assert_copy_cleanup(&program, true);
            }
        }
    }
}

#[test]
fn factory_borrow_argument_and_result_are_each_released_once() {
    let source = "class Holder(val value: Int) { deinit() {} }\nfun make(prefix: String, own n: Int): Holder = Holder(n)\nfun entry(): Int = (make(\"prefix\", 7)).value";
    for constants in [false, true] {
        let (program, _) = lower(source, constants).expect("factory keeps argument cleanup");
        assert_copy_cleanup(&program, true);
        let function = entry(&program);
        let strings = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
            .collect::<Vec<_>>();
        let [string] = strings.as_slice() else {
            panic!("one argument owner");
        };
        let [EntityId::Value(owner)] = string.results.as_slice() else {
            panic!("string result");
        };
        let drops = function
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, instruction)| {
                (instruction.operation == Operation::Drop { owner: *owner }).then_some(i)
            })
            .collect::<Vec<_>>();
        let [drop] = drops.as_slice() else {
            panic!("one argument cleanup");
        };
        let call = function
            .instructions
            .iter()
            .position(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .unwrap();
        let read = function
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation, Operation::HeapFieldRead { .. })
            })
            .unwrap();
        assert!(call < *drop && *drop < read);
    }
}

#[test]
fn diverging_factory_receiver_preserves_only_the_evaluated_prefix() {
    for argument in ["return 9", "stop()"] {
        let call = format!("make(\"prefix\", {argument})");
        for receiver in [call.clone(), format!("({call})"), format!("(({call}))")] {
            let source = format!(
                "class Holder(val value: Int)\nfun make(prefix: String, own n: Int): Holder = Holder(n)\nfun stop(): Nothing = stop()\nfun entry(): Int {{ return {receiver}.value }}"
            );
            for constants in [false, true] {
                let (result, routed) = refine::observe_routing(|| lower(&source, constants));
                let (program, _) = result.expect("non-dispatching factory prefix lowers");
                assert!(
                    routed
                        .iter()
                        .any(|name| name.starts_with("koven.test.entry.d")),
                    "{routed:?}"
                );
                verify_program(&program).unwrap();
                let function = entry(&program);
                assert!(function.instructions.iter().all(|i| !matches!(
                    i.operation,
                    Operation::HeapAllocate { .. } | Operation::HeapFieldRead { .. }
                )));
                for instruction in &function.instructions {
                    if let Operation::DirectCall { callee, .. } = instruction.operation {
                        let called = program
                            .modules
                            .iter()
                            .flat_map(|m| &m.functions)
                            .find(|f| f.id == callee)
                            .unwrap();
                        assert!(
                            called.name.contains(".stop."),
                            "unexpected call {}",
                            called.name
                        );
                    }
                }
                let strings = function
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                    .collect::<Vec<_>>();
                let [string] = strings.as_slice() else {
                    panic!("one evaluated prefix");
                };
                let [EntityId::Value(owner)] = string.results.as_slice() else {
                    panic!("prefix owner");
                };
                let drops = function
                    .instructions
                    .iter()
                    .filter(|i| i.operation == Operation::Drop { owner: *owner })
                    .count();
                assert_eq!(drops, usize::from(argument == "return 9"));
                if argument == "return 9" {
                    let place_instruction = function
                        .instructions
                        .iter()
                        .find(|i| i.operation == Operation::RootPlace { owner: *owner })
                        .unwrap();
                    let [EntityId::Place(place)] = place_instruction.results.as_slice() else {
                        panic!("prefix root place");
                    };
                    let borrow = function.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place: p, .. } if p == *place)).unwrap();
                    let [EntityId::Loan(loan)] = borrow.results.as_slice() else {
                        panic!("prefix loan");
                    };
                    let end = function
                        .instructions
                        .iter()
                        .position(|i| i.operation == Operation::BorrowEnd { loan: *loan })
                        .unwrap();
                    let drop = function
                        .instructions
                        .iter()
                        .position(|i| i.operation == Operation::Drop { owner: *owner })
                        .unwrap();
                    assert!(end < drop);
                }

                let returns = function
                    .blocks
                    .iter()
                    .filter_map(|block| block.terminator.as_ref())
                    .filter(|term| {
                        matches!(term.kind, crate::ssa::model::TerminatorKind::Return { .. })
                    })
                    .count();
                assert_eq!(returns, usize::from(argument == "return 9"));
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}

#[test]
fn field_prefix_admission_does_not_authorize_other_lenders() {
    for body in [
        "fun entry(): Int { make(\"prefix\", return 9); return 0 }",
        "fun number(prefix: String, own n: Int): Int = n\nfun entry(): Int = make(\"outer\", number(\"inner\", return 9)).value",
    ] {
        let source = format!(
            "class Holder(val value: Int)\nfun make(prefix: String, own n: Int): Holder = Holder(n)\n{body}"
        );
        for constants in [false, true] {
            let (result, routed) = refine::observe_routing(|| lower(&source, constants));
            assert!(
                !routed
                    .iter()
                    .any(|name| name.starts_with("koven.test.entry.d")),
                "{routed:?}"
            );
            match result {
                Ok((program, _)) => {
                    verify_program(&program).unwrap();
                    crate::llvm::render_verified_program(&program).unwrap();
                }
                Err(error) => assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode),
            }
        }
    }
}

mod constructor_prefix;
