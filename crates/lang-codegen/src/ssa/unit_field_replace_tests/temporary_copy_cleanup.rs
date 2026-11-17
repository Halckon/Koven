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
                verify_program(&program).unwrap();
                let function = entry(&program);
                let allocations = function
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| matches!(i.operation, Operation::HeapAllocate { .. }))
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
                    .position(|instruction| {
                        instruction.operation == Operation::BorrowEnd { loan: *loan }
                    })
                    .unwrap();
                let drops = function
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, instruction)| {
                        instruction.operation == Operation::Drop { owner: *owner }
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let [drop] = drops.as_slice() else {
                    panic!("exactly one cleanup of constructed parent");
                };
                assert!(allocation_index < read && *read < end && end < *drop);
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}

#[test]
fn unsupported_factory_and_compatibility_routes_remain_closed() {
    let source = "class Holder(val value: Int)\nfun make(): Holder = Holder(7)\nfun entry(): Int = make().value";
    for constants in [false, true] {
        assert_eq!(
            lower(source, constants)
                .err()
                .expect("factory remains outside this slice")
                .kind,
            LoweringErrorKind::UnsupportedNode
        );
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
