//! Ordinary Unit fallback boundary for an owned, non-resource Boolean receiver.
use crate::ssa::loan_region_ssa_tests::*;

pub(super) const SOURCE: &str =
    "class Node { fun touch(): Boolean = true }\nfun run(own node: Node): Unit { node.touch() }";

#[test]
fn boolean_receiver_compatibility_preserves_identity_and_releases_owner() {
    // Keep the exact original caller; only the second pass supplies a native entry.
    for native in [false, true] {
        let text = if native {
            format!("{SOURCE}\nfun entry(): Unit {{ run(Node()) }}")
        } else {
            SOURCE.to_owned()
        };
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "boolean_receiver.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "boolean_receiver.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let entry_name = if native { "entry" } else { "run" };
            let declarations = names.names().index().declarations();
            let entry = declarations
                .iter()
                .find(|declaration| declaration.name() == entry_name)
                .unwrap()
                .id();
            let ((lowered, routed), receipts) =
                crate::ssa::refine::receiver_receipt::observe(|| {
                    crate::ssa::refine::observe_routing(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, &owned, entry,
                        )
                    })
                });
            let (program, entry) = lowered.unwrap();
            let [module] = program.modules.as_slice() else {
                panic!("one module");
            };
            let caller = if native {
                let wrapper = module.function(entry).unwrap();
                let calls = wrapper
                    .instructions
                    .iter()
                    .filter_map(|instruction| match instruction.operation {
                        Operation::DirectCall {
                            callee,
                            receiver: None,
                            ..
                        } => Some(callee),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [caller] = calls.as_slice() else {
                    panic!("wrapper must call the original caller exactly once");
                };
                *caller
            } else {
                entry
            };
            let run = module.function(caller).unwrap();
            assert!(
                !routed.contains(&run.name),
                "Boolean caller stays on Compatibility"
            );
            if !native {
                assert!(routed.is_empty());
                assert!(receipts.is_empty());
            }
            let [receiver_end] = owned.ownership().receiver_call_return_ends() else {
                panic!("one real receiver end");
            };
            let owner_drop = owned
                .ownership()
                .drops()
                .iter()
                .find(|drop| {
                    run.origin.span().start() <= drop.value_origin().start()
                        && drop.value_origin().end() <= run.origin.span().end()
                })
                .expect("one named owner drop for the original caller");
            let [block] = run.blocks.as_slice() else {
                panic!("one straight-line caller block");
            };
            let [EntityId::Value(owner)] = block.parameters.as_slice() else {
                panic!("one owner argument");
            };
            let EntityType::Value(node) = run.entity(EntityId::Value(*owner)).unwrap().ty else {
                panic!("owned Node");
            };
            assert!(matches!(
                module.types[node.index()],
                SsaTypeKind::HeapOwner { .. }
            ));
            let [root, begin, call, end, drop] = run.instructions.as_slice() else {
                panic!("exact receiver lifecycle: {:?}", run.instructions);
            };
            assert_eq!(
                block.instructions,
                [root.id, begin.id, call.id, end.id, drop.id]
            );
            let [EntityId::Place(place)] = root.results.as_slice() else {
                panic!("one root place");
            };
            let [EntityId::Loan(loan)] = begin.results.as_slice() else {
                panic!("one shared loan");
            };
            let [EntityId::Value(result)] = call.results.as_slice() else {
                panic!("one result");
            };
            assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
            assert_eq!(
                begin.operation,
                Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared
                }
            );
            let Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } = &call.operation
            else {
                panic!("member call");
            };
            assert_eq!(*receiver, Some(EntityId::Loan(*loan)));
            assert!(arguments.is_empty());
            let EntityType::Value(boolean) = run.entity(EntityId::Value(*result)).unwrap().ty
            else {
                panic!("owned Boolean result");
            };
            assert_eq!(module.types[boolean.index()], SsaTypeKind::Boolean);
            let method = module.function(*callee).unwrap();
            assert_eq!(method.return_types, [boolean]);
            assert!(method.instructions.iter().any(|instruction| {
                instruction.operation == Operation::Constant(ScalarConstant::Boolean(true))
            }));
            assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
            assert_eq!(drop.operation, Operation::Drop { owner: *owner });
            for (instruction, origin) in [
                (root, receiver_end.receiver().begin_span()),
                (begin, receiver_end.receiver().begin_span()),
                (call, receiver_end.receiver().end_span()),
                (end, receiver_end.receiver().end_span()),
                (drop, owner_drop.value_origin()),
            ] {
                assert_eq!(instruction.origin, Origin::Source(origin));
            }
            verify_program(&program).unwrap();
            if native {
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
                    &llvm,
                    &[0],
                );
                crate::native_tests::boxed_enum_tests::assert_success(&run, b"");
            } else {
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}
