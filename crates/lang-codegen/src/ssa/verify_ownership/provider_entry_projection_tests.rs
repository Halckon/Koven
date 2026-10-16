//! Synthetic entry-state guards, not end-to-end verifier rejection tests.
use super::{
    super::{Reference, State},
    EntryProjection,
};
use crate::ssa::model::{
    BlockId, Definition, Edge, EntityId, EntityType, Function, LoanId, LoanKind, Operation, Origin,
    PlaceId, Program, SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
};
use lang_frontend::source::SourceMap;

fn fixture() -> (Function, EntityType, SsaTypeId, SsaTypeId, Origin) {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("provider-projection.ko", "fixture")
        .unwrap();
    let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("provider-projection");
    let module = program.module_mut(module_id).unwrap();
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, int)
        .unwrap();
    let loan_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: array,
    };
    module
        .add_function("project", vec![], origin.clone())
        .unwrap();
    (module.functions.remove(0), loan_type, int, boolean, origin)
}

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(id) = entity else {
        panic!("expected loan")
    };
    id
}
fn place(entity: EntityId) -> PlaceId {
    let EntityId::Place(id) = entity else {
        panic!("expected place")
    };
    id
}
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("expected value")
    };
    id
}
fn reference(tokens: &[LoanId], live: bool) -> Reference {
    Reference {
        lineage: tokens.iter().copied().collect(),
        live,
    }
}
fn append(function: &mut Function, block: BlockId, op: Operation, ty: EntityType) -> EntityId {
    let (instruction, results) = function
        .append_instruction(block, op, vec![ty], function.origin.clone())
        .unwrap();
    assert_eq!(
        function.entity(results[0]).unwrap().definition,
        Definition::InstructionResult {
            instruction,
            index: 0
        }
    );
    assert_eq!(function.instruction(instruction).unwrap().block, block);
    results[0]
}
fn terminate(function: &mut Function, block: BlockId, kind: TerminatorKind) {
    function
        .set_terminator(block, kind, function.origin.clone())
        .unwrap();
}
fn end_instruction(function: &mut Function, block: BlockId, id: LoanId) {
    function
        .append_instruction(
            block,
            Operation::BorrowEnd { loan: id },
            vec![],
            function.origin.clone(),
        )
        .unwrap();
}
fn parameters(function: &Function, block: BlockId) -> Vec<EntityId> {
    let parameters = function.block(block).unwrap().parameters.clone();
    for (index, entity) in parameters.iter().enumerate() {
        assert_eq!(
            function.entity(*entity).unwrap().definition,
            Definition::BlockParameter { block, index }
        );
    }
    parameters
}

#[test]
fn place_only_seed_retains_non_dominating_ancestor_tokens() {
    let (mut function, ty, int, boolean, origin) = fixture();
    let entry = function
        .add_block(
            vec![ty, EntityType::Value(boolean), EntityType::Value(int)],
            origin.clone(),
        )
        .unwrap();
    let args = parameters(&function, entry);
    let left = function.add_block(vec![], origin.clone()).unwrap();
    let right = function.add_block(vec![], origin.clone()).unwrap();
    let join = function
        .add_block(vec![EntityType::Place(int)], origin)
        .unwrap();
    terminate(
        &mut function,
        entry,
        TerminatorKind::Conditional {
            condition: value(args[1]),
            when_true: Edge {
                target: left,
                arguments: vec![],
            },
            when_false: Edge {
                target: right,
                arguments: vec![],
            },
        },
    );
    let ancestor = loan(append(
        &mut function,
        left,
        Operation::SharedReborrow {
            source: loan(args[0]),
        },
        ty,
    ));
    let child = loan(append(
        &mut function,
        left,
        Operation::SharedReborrow { source: ancestor },
        ty,
    ));
    let old_place = append(
        &mut function,
        left,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(child),
            index: value(args[2]),
        },
        EntityType::Place(int),
    );
    end_instruction(&mut function, left, child);
    end_instruction(&mut function, left, ancestor);
    terminate(
        &mut function,
        left,
        TerminatorKind::Branch(Edge {
            target: join,
            arguments: vec![old_place],
        }),
    );
    let unrelated = loan(append(
        &mut function,
        right,
        Operation::SharedReborrow {
            source: loan(args[0]),
        },
        ty,
    ));
    let right_place = append(
        &mut function,
        right,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(unrelated),
            index: value(args[2]),
        },
        EntityType::Place(int),
    );
    end_instruction(&mut function, right, unrelated);
    terminate(
        &mut function,
        right,
        TerminatorKind::Branch(Edge {
            target: join,
            arguments: vec![right_place],
        }),
    );
    terminate(&mut function, join, TerminatorKind::Abort);
    let surviving = place(parameters(&function, join)[0]);
    let projection = EntryProjection::new(&function);
    assert!(!projection.dominators[join.index()].contains(&left.index()));
    assert!(!projection.available(&function, join, EntityId::Loan(ancestor)));
    assert!(!projection.available(&function, join, EntityId::Loan(child)));
    // The minimal synthetic place seed forces recursive discovery through child.
    let mut state = State::default();
    state.loans.insert(ancestor, reference(&[ancestor], false));
    state
        .loans
        .insert(child, reference(&[ancestor, child], false));
    state.loans.insert(unrelated, reference(&[unrelated], true));
    state.places.insert(surviving, reference(&[child], false));
    state
        .places
        .insert(place(old_place), reference(&[child], false));
    projection.retain_entry(&function, join, &mut state);
    let expected = State {
        loans: [
            (ancestor, reference(&[ancestor], false)),
            (child, reference(&[ancestor, child], false)),
        ]
        .into(),
        places: [(surviving, reference(&[child], false))].into(),
    };
    assert_eq!(state, expected);
    projection.retain_entry(&function, join, &mut state);
    assert_eq!(state, expected, "entry projection is idempotent");
}

#[test]
fn inactive_alias_survives_parallel_parameter_swap_on_backedge() {
    let (mut function, ty, int, _, origin) = fixture();
    let entry = function
        .add_block(vec![ty, ty, EntityType::Value(int)], origin.clone())
        .unwrap();
    let args = parameters(&function, entry);
    let header = function
        .add_block(vec![ty, ty, EntityType::Place(int)], origin)
        .unwrap();
    let params = parameters(&function, header);
    let initial_place = append(
        &mut function,
        entry,
        Operation::ContainerElementPlace {
            owner: args[1],
            index: value(args[2]),
        },
        EntityType::Place(int),
    );
    terminate(
        &mut function,
        entry,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments: vec![args[0], args[1], initial_place],
        }),
    );
    let backedge = Edge {
        target: header,
        arguments: vec![params[1], params[0], params[2]],
    };
    terminate(
        &mut function,
        header,
        TerminatorKind::Branch(backedge.clone()),
    );
    let (left, right) = (loan(args[0]), loan(args[1]));
    let (first, second, carried_place) = (loan(params[0]), loan(params[1]), place(params[2]));
    let mut predecessor = State::default();
    predecessor.loans.insert(left, reference(&[left], false));
    predecessor.loans.insert(right, reference(&[right], true));
    predecessor.loans.insert(first, reference(&[left], false));
    predecessor.loans.insert(second, reference(&[right], true));
    predecessor
        .places
        .insert(carried_place, reference(&[right], true));
    // Projection consumes an already simultaneous edge snapshot; this is not a
    // test of the provider verifier's separate edge-copy implementation.
    let mut state = predecessor.clone();
    for (from, to) in backedge.arguments.iter().zip(&params) {
        if let (EntityId::Loan(from), EntityId::Loan(to)) = (from, to) {
            state.loans.insert(*to, predecessor.loans[from].clone());
        }
    }
    EntryProjection::new(&function).retain_entry(&function, header, &mut state);
    let mut expected = predecessor;
    expected.loans.insert(first, reference(&[right], true));
    expected.loans.insert(second, reference(&[left], false));
    assert_eq!(state, expected);
    state.end(first);
    expected.loans.insert(right, reference(&[right], false));
    expected.loans.insert(first, reference(&[right], false));
    expected
        .places
        .insert(carried_place, reference(&[right], false));
    assert_eq!(
        state, expected,
        "ending the swapped alias still invalidates its place"
    );
}

#[test]
fn current_block_old_definition_survives_only_when_lineage_references_it() {
    let (mut function, ty, int, _, origin) = fixture();
    let entry = function
        .add_block(vec![ty, EntityType::Value(int)], origin.clone())
        .unwrap();
    let args = parameters(&function, entry);
    let initial_alias = append(
        &mut function,
        entry,
        Operation::SharedReborrow {
            source: loan(args[0]),
        },
        ty,
    );
    let initial_place = append(
        &mut function,
        entry,
        Operation::ContainerElementPlace {
            owner: initial_alias,
            index: value(args[1]),
        },
        EntityType::Place(int),
    );
    let header = function
        .add_block(vec![ty, ty, EntityType::Place(int)], origin)
        .unwrap();
    let params = parameters(&function, header);
    terminate(
        &mut function,
        entry,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments: vec![args[0], initial_alias, initial_place],
        }),
    );
    let (root, source, alias, carried_place) = (
        loan(args[0]),
        loan(params[0]),
        loan(params[1]),
        place(params[2]),
    );
    end_instruction(&mut function, header, alias);
    let old = loan(append(
        &mut function,
        header,
        Operation::SharedReborrow { source },
        ty,
    ));
    let unrelated = loan(append(
        &mut function,
        header,
        Operation::SharedReborrow { source },
        ty,
    ));
    end_instruction(&mut function, header, unrelated);
    terminate(
        &mut function,
        header,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments: vec![params[0], EntityId::Loan(old), params[2]],
        }),
    );
    let projection = EntryProjection::new(&function);
    assert!(!projection.available(&function, header, EntityId::Loan(old)));
    let mut state = State::default();
    for id in [root, source] {
        state.loans.insert(id, reference(&[root], true));
    }
    for id in [old, alias] {
        state.loans.insert(id, reference(&[root, old], true));
    }
    state
        .loans
        .insert(unrelated, reference(&[root, unrelated], false));
    state
        .places
        .insert(carried_place, reference(&[root, old], true));
    let mut expected = state.clone();
    expected.loans.remove(&unrelated);
    projection.retain_entry(&function, header, &mut state);
    assert_eq!(state, expected);
    // With both observable references removed, the previous instruction result
    // is discarded even though its block dominates itself.
    let mut unreferenced = state.clone();
    unreferenced.loans.remove(&alias);
    unreferenced.places.clear();
    projection.retain_entry(&function, header, &mut unreferenced);
    assert!(!unreferenced.loans.contains_key(&old));
    state.end(alias);
    for id in [old, alias] {
        expected.loans.insert(id, reference(&[root, old], false));
    }
    expected
        .places
        .insert(carried_place, reference(&[root, old], false));
    assert_eq!(
        state, expected,
        "original token identity still drives end(alias)"
    );
}

#[test]
fn nullable_non_null_proof_birth_is_retained_at_its_entry() {
    let (_, _, _, _, origin) = fixture();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-projection");
    let module = program.module_mut(module_id).unwrap();
    let payload = module.add_aggregate_type("Node.payload", vec![]).unwrap();
    let inner = module.declare_heap_owner("Node").unwrap();
    module.define_heap_owner(inner, payload).unwrap();
    let nullable = module.add_nullable_handle_type(inner).unwrap();
    let id = module
        .add_function("project", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let entry = function
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .unwrap();
    let owner = parameters(function, entry)[0];
    let null = function
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .unwrap();
    let non_null = function
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin,
        )
        .unwrap();
    let view = loan(parameters(function, non_null)[1]);
    terminate(
        function,
        entry,
        TerminatorKind::NullableBranch {
            owner: value(owner),
            view,
            when_null: Edge {
                target: null,
                arguments: vec![owner],
            },
            when_non_null: Edge {
                target: non_null,
                arguments: vec![owner],
            },
        },
    );
    terminate(function, null, TerminatorKind::Abort);
    terminate(function, non_null, TerminatorKind::Abort);
    // This fixture also has a complete, valid verifier baseline; the synthetic
    // state below specifically isolates preservation after the proof's birth.
    crate::ssa::verify::verify_program(&program).expect("nullable baseline");
    let function = program.module(module_id).unwrap().function(id).unwrap();
    let projection = EntryProjection::new(function);
    assert!(projection.available(function, non_null, EntityId::Loan(view)));
    assert!(!projection.available(function, null, EntityId::Loan(view)));
    let mut state = State {
        loans: [(view, reference(&[view], true))].into(),
        places: Default::default(),
    };
    let born = state.clone();
    projection.retain_entry(function, non_null, &mut state);
    assert_eq!(state, born);
    projection.retain_entry(function, null, &mut state);
    assert_eq!(
        state,
        State::default(),
        "proof does not survive on null entry"
    );
}
