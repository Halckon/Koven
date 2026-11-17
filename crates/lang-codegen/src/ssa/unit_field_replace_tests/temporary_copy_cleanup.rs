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
