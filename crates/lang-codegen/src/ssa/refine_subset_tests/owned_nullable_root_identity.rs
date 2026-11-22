//! Original Node-only fixtures. Follow the exact nullable proof and owner over SSA edges.
use crate::ssa::{
    model::{Edge, EntityId, Function, Operation, TerminatorKind},
    unit_lower_test_support::{analyze, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::UnitNullableWhenUseKind,
    source::SourceMap,
    type_checking::{NullableWhenSubjectCategory, standard_environments},
};
use std::collections::BTreeSet;

#[test]
fn original_owned_nullable_when_read_reuses_one_owner_and_take_consumes_only_present_arm() {
    let read = "class Node()\nvalue class Token(val item: Int)\n\
             fun observe(node: Node): Unit {}\n\
             fun inspect(own node: Node?): Unit {\n\
                 when (node) { null -> {}; else -> { observe(node) } }\n\
                 when (node) { null -> {}; else -> { observe(node) } }\n\
             }";
    let take = "class Node()\nvalue class Token(val item: Int)\nfun consume(own node: Node): Unit {}\nfun inspect(own node: Node?): Unit { when (node) { null -> {}; else -> { consume(node) } } }";
    for (text, consuming) in [(read, false), (take, true)] {
        for rooted in [false, true] {
            let mut sources = SourceMap::new();
            let path = "lowering.ko";
            let (source, file) = parsed(&mut sources, path, text);
            let inputs = [if rooted {
                SourceUnitInput::new("root", path, source, &file)
            } else {
                SourceUnitInput::memory_document(source, &file)
            }];
            let (env, types) = standard_environments();
            let (names, typed, owned) = analyze(&sources, &inputs, &env, &types);
            let plans = owned.ownership().nullable_whens();
            let expected = if consuming { 1 } else { 2 };
            assert_eq!(plans.len(), expected);
            let symbol = plans[0].descriptor().stable_symbol().unwrap();
            for plan in plans {
                let descriptor = plan.descriptor();
                assert_eq!(
                    descriptor.source_category(),
                    NullableWhenSubjectCategory::OwnedRoot
                );
                assert_eq!(descriptor.stable_symbol(), Some(symbol));
                let [usage] = plan.uses() else {
                    panic!("one exact branch call use");
                };
                assert_eq!(
                    usage.kind(),
                    if consuming {
                        UnitNullableWhenUseKind::Consume
                    } else {
                        UnitNullableWhenUseKind::Observe
                    }
                );
            }
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "inspect")
                .unwrap()
                .id();
            let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources, &inputs, &names, &types, &typed, &owned, entry,
                )
            });
            let (program, id) =
                lowered.unwrap_or_else(|error| panic!("routes={routes:?}: {error:?}"));
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let module = program.module(id.module()).unwrap();
            let function = module.function(id).unwrap();
            assert!(routes.contains(&function.name));
            let original = function
                .block(function.entry_block().unwrap())
                .unwrap()
                .parameters[0];
            let arms = function
                .blocks
                .iter()
                .filter_map(|block| {
                    let TerminatorKind::NullableBranch {
                        owner,
                        view,
                        when_null,
                        when_non_null,
                    } = &block.terminator.as_ref()?.kind
                    else {
                        return None;
                    };
                    Some((*owner, *view, when_null, when_non_null))
                })
                .collect::<Vec<_>>();
            assert_eq!(arms.len(), expected);
            let mut proofs = BTreeSet::new();
            for (owner, view, absent, present) in arms {
                assert!(transported(
                    function,
                    EntityId::Value(owner),
                    original,
                    &mut BTreeSet::new()
                ));
                assert!(proofs.insert(view));
                assert!(!absent.arguments.contains(&EntityId::Loan(view)));
                let body = function.block(present.target).unwrap();
                assert_eq!(body.parameters.last(), Some(&EntityId::Loan(view)));
                let call = body
                    .instructions
                    .iter()
                    .map(|id| function.instruction(*id).unwrap())
                    .find(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .unwrap();
                let Operation::DirectCall {
                    callee, arguments, ..
                } = &call.operation
                else {
                    unreachable!()
                };
                let target = module.function(*callee).unwrap();
                assert!(target.name.contains(if consuming {
                    ".consume.d"
                } else {
                    ".observe.d"
                }));
                if consuming {
                    let takes = function
                        .instructions
                        .iter()
                        .filter(|i| matches!(i.operation, Operation::NullableTake { .. }))
                        .collect::<Vec<_>>();
                    let [take] = takes.as_slice() else {
                        panic!("single actual take");
                    };
                    assert_eq!(take.block, body.id);
                    assert!(
                        matches!(take.operation, Operation::NullableTake { owner, proof }
                        if proof == view && transported(function, EntityId::Value(owner), original, &mut BTreeSet::new()))
                    );
                    assert_eq!(take.results, *arguments);
                } else {
                    assert_eq!(arguments, &[EntityId::Loan(view)]);
                    assert!(!function.instructions.iter().any(|i| matches!(
                        i.operation,
                        Operation::NullableTake { .. } | Operation::SharedRetain { .. }
                    )));
                    let ends = body.instructions.iter().map(|id| function.instruction(*id).unwrap()).filter(|i| matches!(i.operation, Operation::BorrowEnd { loan } if loan == view)).collect::<Vec<_>>();
                    let [end] = ends.as_slice() else {
                        panic!("one end on each distinct proof");
                    };
                    let positions = |id| {
                        body.instructions
                            .iter()
                            .position(|actual| *actual == id)
                            .unwrap()
                    };
                    assert!(positions(call.id) < positions(end.id));
                }
            }
        }
    }
}

fn transported(
    function: &Function,
    entity: EntityId,
    origin: EntityId,
    seen: &mut BTreeSet<EntityId>,
) -> bool {
    use crate::ssa::model::Definition;
    if entity == origin {
        return true;
    }
    if !seen.insert(entity) {
        return false;
    }
    let Some(Definition::BlockParameter { block, index }) =
        function.entity(entity).map(|data| data.definition)
    else {
        return false;
    };
    let mut incoming = Vec::<&Edge>::new();
    for term in function.blocks.iter().filter_map(|b| b.terminator.as_ref()) {
        match &term.kind {
            TerminatorKind::Branch(edge) if edge.target == block => incoming.push(edge),
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            }
            | TerminatorKind::NullableBranch {
                when_non_null: when_true,
                when_null: when_false,
                ..
            } => {
                incoming.extend(
                    [when_true, when_false]
                        .into_iter()
                        .filter(|edge| edge.target == block),
                );
            }
            _ => {}
        }
    }
    !incoming.is_empty()
        && incoming.into_iter().all(|edge| {
            edge.arguments
                .get(index)
                .is_some_and(|&argument| transported(function, argument, origin, &mut seen.clone()))
        })
}
