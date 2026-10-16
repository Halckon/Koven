//! Query-only identities for a nullable while inside an ordinary borrowed for.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::source::Span;

use super::OuterSpans;
use crate::ssa::model::{
    BinaryOperator, Block, BlockId, CheckedArithmeticOperator, ComparisonOperator, Definition,
    Edge, EntityId, EntityType, Function, Instruction, InstructionId, LoanKind, Operation, Origin,
    PlaceAccess, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
};

const CURRENT: usize = 0;
const ITEMS: usize = 1;
const SOURCE: usize = 2;
const ELEMENT: usize = 3;
const VIEW: usize = 4;

struct Shape {
    header: BlockId,
    body: BlockId,
    exhaustion: BlockId,
    inner: BlockId,
    join: BlockId,
    construct: InstructionId,
    root: InstructionId,
    source_begin: InstructionId,
    element_place: InstructionId,
    element_begin: InstructionId,
    reads: [InstructionId; 2],
    member: InstructionId,
    increment: InstructionId,
    length: ValueId,
    zero: ValueId,
    step: ValueId,
    null_path: Vec<BlockId>,
    non_null_path: Vec<BlockId>,
    post_path: Vec<BlockId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Route {
    Untested,
    Null,
    NonNull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    live: [Option<EntityId>; 5],
    route: Route,
    inner_read: bool,
    member_read: bool,
    post_read: bool,
    advanced: bool,
    source_ended: bool,
}

#[derive(Default)]
struct Trace {
    entries: BTreeMap<BlockId, BTreeSet<State>>,
    before: BTreeMap<InstructionId, BTreeSet<State>>,
    exits: BTreeMap<BlockId, BTreeSet<State>>,
    aliases: [BTreeSet<EntityId>; 5],
    ends: [BTreeSet<InstructionId>; 5],
    drops: [BTreeSet<InstructionId>; 5],
}

pub(super) fn assert_outer_borrowed(program: &Program, spans: &OuterSpans) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(term(block), TerminatorKind::NullableBranch { .. })
                && term_at(block, spans.inner)
        })
    }));
    let shape = shape(function, spans);
    let trace = trace(function, &shape, spans);
    assert_scalars(function, &shape);
    let source = function
        .block(shape.inner)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .origin
        .span()
        .source_id();
    for block in &function.blocks {
        assert_eq!(block.origin.span().source_id(), source);
        assert_eq!(
            block.terminator.as_ref().unwrap().origin.span().source_id(),
            source
        );
    }
    for instruction in &function.instructions {
        assert_eq!(instruction.origin.span().source_id(), source);
    }
    // Same-type entities only close the alias universe after exact-edge tracing.
    let entry = only(
        trace.entries[&function.entry_block().unwrap()]
            .iter()
            .copied(),
    );
    let current = function
        .entity(entry.live[CURRENT].unwrap())
        .unwrap()
        .ty
        .semantic_type();
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(current) else {
        panic!("the actual guarded root is nullable")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("the proof views the actual Holder owner")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("registered Holder layout")
    };
    let integer = *only(fields.iter());
    let items = function
        .entity(result(function.instruction(shape.construct).unwrap()))
        .unwrap()
        .ty
        .semantic_type();
    let shared = |target| EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    };
    let types = [
        EntityType::Value(current),
        EntityType::Value(items),
        shared(items),
        shared(integer),
        shared(*inner),
    ];
    for (role, ty) in types.into_iter().enumerate() {
        let actual = function
            .blocks
            .iter()
            .flat_map(|block| &block.parameters)
            .chain(
                function
                    .instructions
                    .iter()
                    .flat_map(|instruction| &instruction.results),
            )
            .copied()
            .filter(|entity| function.entity(*entity).unwrap().ty == ty)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual, trace.aliases[role],
            "the role {role} has no substitute identity"
        );
    }
    for (id, states) in &trace.entries {
        for state in states {
            assert_live(&trace, *state, &function.block(*id).unwrap().parameters);
        }
    }
    for (id, states) in &trace.before {
        for state in states {
            assert_live(
                &trace,
                *state,
                &function.instruction(*id).unwrap().operation.entities(),
            );
        }
    }
    for (id, states) in &trace.exits {
        for state in states {
            assert_live(
                &trace,
                *state,
                &term(function.block(*id).unwrap()).entities(),
            );
        }
    }
    assert_eq!(trace.entries.len(), function.blocks.len());
    for role in [SOURCE, ELEMENT, VIEW] {
        assert_eq!(trace.ends[role].len(), 1);
    }
    for role in [CURRENT, ITEMS] {
        assert_eq!(trace.drops[role].len(), 1);
    }
    let post_states = &trace.before[&shape.reads[1]];
    assert_eq!(
        post_states.len(),
        2,
        "null and nonnull independently reach the post-item read"
    );
    assert_eq!(
        post_states
            .iter()
            .map(|state| state.route)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([Route::Null, Route::NonNull])
    );
    assert_eq!(
        post_states
            .iter()
            .map(|state| state.live)
            .collect::<BTreeSet<_>>()
            .len(),
        1,
        "both real paths merge the current owner/source/element columns identically"
    );
    let view_end = only(trace.ends[VIEW].iter().copied());
    assert!(
        shape
            .non_null_path
            .contains(&function.instruction(view_end).unwrap().block)
    );
    assert!(
        !shape
            .null_path
            .contains(&function.instruction(view_end).unwrap().block)
    );
    let element_end = only(trace.ends[ELEMENT].iter().copied());
    assert!(
        shape
            .post_path
            .contains(&function.instruction(element_end).unwrap().block)
    );
    let back = *shape.post_path.last().unwrap();
    assert_eq!(
        trace.exits[&back]
            .iter()
            .map(|state| state.route)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([Route::Null, Route::NonNull])
    );
}

fn shape(function: &Function, spans: &OuterSpans) -> Shape {
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let instructions = &function.instructions;
    let select = |predicate: fn(&Operation) -> bool| {
        only(
            instructions
                .iter()
                .filter(|instruction| predicate(&instruction.operation)),
        )
    };
    let construct = select(|op| matches!(op, Operation::ContainerConstruct { .. }));
    assert!(source_origin(&construct.origin, spans.construct));
    let Operation::ContainerConstruct { elements, .. } = &construct.operation else {
        unreachable!()
    };
    assert_eq!(elements.len(), 2);
    for (value, expected) in elements.iter().zip([1, 2]) {
        assert_integer(function, *value, expected);
    }
    let root = select(|op| matches!(op, Operation::RootPlace { .. }));
    assert!(
        matches!(root.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == result(construct))
    );
    let begins = instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
        .collect::<Vec<_>>();
    assert_eq!(begins.len(), 2);
    let source_begin = only(begins.iter().copied().filter(|instruction| matches!(instruction.operation,
        Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == result(root))));
    let length = select(|op| matches!(op, Operation::ContainerLength { .. }));
    assert!(
        matches!(length.operation, Operation::ContainerLength { owner } if owner == result(source_begin))
    );
    let constant = |number| {
        only(instructions.iter().filter(|instruction| {
            source_origin(&instruction.origin, spans.outer)
                && instruction.operation == Operation::Constant(ScalarConstant::Integer(number))
        }))
    };
    let zero = constant(0);
    let step = constant(1);
    assert_eq!(construct.block, entry.id);
    for instruction in [root, source_begin, length, zero, step] {
        assert_eq!(instruction.block, entry.id);
        assert!(source_origin(&instruction.origin, spans.outer));
    }
    let TerminatorKind::Branch(start) = term(entry) else {
        panic!("actual provider entry")
    };
    let header = function.block(start.target).unwrap();
    let (condition, entered, exhausted) = conditional(header);
    let guard = select(|op| {
        matches!(
            op,
            Operation::Compare {
                operator: ComparisonOperator::LessThan,
                ..
            }
        )
    });
    assert_eq!(definition(function, condition).id, guard.id);
    assert_eq!(guard.block, header.id);
    assert!(source_origin(&guard.origin, spans.outer) && term_at(header, spans.outer));
    let element_place = select(|op| matches!(op, Operation::ContainerElementPlace { .. }));
    let element_begin = only(begins.into_iter().filter(|instruction| matches!(instruction.operation,
        Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == result(element_place))));
    for instruction in [element_place, element_begin] {
        assert_eq!(instruction.block, entered.target);
        assert!(source_origin(&instruction.origin, spans.outer));
    }
    let inner = only(
        function
            .blocks
            .iter()
            .filter(|block| matches!(term(block), TerminatorKind::NullableBranch { .. })),
    );
    assert!(term_at(inner, spans.inner));
    let TerminatorKind::Branch(to_inner) = term(function.block(entered.target).unwrap()) else {
        panic!("element reaches inner test")
    };
    assert_eq!(
        incoming(function, inner.id),
        vec![(entered.target, to_inner)]
    );
    let TerminatorKind::NullableBranch {
        when_null,
        when_non_null,
        view,
        ..
    } = term(inner)
    else {
        unreachable!()
    };
    let birth = function.block(when_non_null.target).unwrap();
    assert_eq!(
        incoming(function, birth.id),
        vec![(inner.id, when_non_null)]
    );
    assert_eq!(birth.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(birth.parameters.last(), Some(&EntityId::Loan(*view)));
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().definition,
        Definition::BlockParameter {
            block: birth.id,
            index: birth.parameters.len() - 1
        }
    );
    assert!(!when_non_null.arguments.contains(&EntityId::Loan(*view)));
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    let reads = spans.items.map(|span| {
        only(instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::Read { .. })
                && source_origin(&instruction.origin, span)
        }))
        .id
    });
    let member = select(|op| matches!(op, Operation::HeapFieldRead { .. }));
    assert!(source_origin(&member.origin, spans.member));
    assert!(matches!(
        member.operation,
        Operation::HeapFieldRead { field: 0, .. }
    ));
    for (read_span, read) in [
        (spans.items[0], reads[0]),
        (spans.member, member.id),
        (spans.items[1], reads[1]),
    ] {
        let span = only(
            spans
                .additions
                .iter()
                .filter(|span| span.end() == read_span.end()),
        );
        let addition = only(instructions.iter().filter(|instruction| {
            source_origin(&instruction.origin, *span)
                && matches!(
                    instruction.operation,
                    Operation::CheckedArithmetic {
                        operator: CheckedArithmeticOperator::Add,
                        ..
                    }
                )
        }));
        assert!(
            matches!(addition.operation, Operation::CheckedArithmetic { right, .. }
            if right == value(function.instruction(read).unwrap()))
        );
    }
    let join = function.instruction(reads[1]).unwrap().block;
    let null_path = normal_path(function, when_null.target, join);
    let non_null_path = normal_path(function, when_non_null.target, join);
    assert!(non_null_path.contains(&function.instruction(reads[0]).unwrap().block));
    assert!(non_null_path.contains(&member.block));
    let expected_join =
        BTreeSet::from([*null_path.last().unwrap(), *non_null_path.last().unwrap()]);
    assert_eq!(
        incoming(function, join)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>(),
        expected_join
    );
    let post_path = normal_path(function, join, header.id);
    let increment = select(|op| {
        matches!(
            op,
            Operation::Binary {
                operator: BinaryOperator::Add,
                ..
            }
        )
    });
    assert_eq!(increment.block, *post_path.last().unwrap());
    assert!(source_origin(&increment.origin, spans.outer));
    assert_eq!(
        incoming(function, header.id)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([entry.id, increment.block])
    );
    assert_eq!(
        incoming(function, exhausted.target),
        vec![(header.id, exhausted)]
    );
    Shape {
        header: header.id,
        body: entered.target,
        exhaustion: exhausted.target,
        inner: inner.id,
        join,
        construct: construct.id,
        root: root.id,
        source_begin: source_begin.id,
        element_place: element_place.id,
        element_begin: element_begin.id,
        reads,
        member: member.id,
        increment: increment.id,
        length: value(length),
        zero: value(zero),
        step: value(step),
        null_path,
        non_null_path,
        post_path,
    }
}

fn trace(function: &Function, shape: &Shape, spans: &OuterSpans) -> Trace {
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let current = only(
        entry
            .parameters
            .iter()
            .copied()
            .filter(|entity| matches!(entity, EntityId::Value(_))),
    );
    let initial = State {
        live: [Some(current), None, None, None, None],
        route: Route::Untested,
        inner_read: false,
        member_read: false,
        post_read: false,
        advanced: false,
        source_ended: false,
    };
    let mut trace = Trace::default();
    let mut pending = vec![(entry.id, initial)];
    let mut returns = BTreeSet::new();
    while let Some((id, mut state)) = pending.pop() {
        if !trace.entries.entry(id).or_default().insert(state) {
            continue;
        }
        for (role, entity) in state.live.iter().enumerate() {
            trace.aliases[role].extend(entity);
        }
        let block = function.block(id).unwrap();
        if id == shape.header {
            assert_eq!(state.route, Route::Untested);
            assert!(state.live[..ELEMENT].iter().all(Option::is_some));
            assert!(state.live[ELEMENT..].iter().all(Option::is_none));
            assert!(!state.inner_read && !state.member_read && !state.post_read && !state.advanced);
        }
        if id == shape.join {
            assert_ne!(state.route, Route::Untested);
            assert_eq!(state.live[VIEW], None);
            assert!(state.live[..VIEW].iter().all(Option::is_some));
        }
        for id in &block.instructions {
            trace.before.entry(*id).or_default().insert(state);
            let instruction = function.instruction(*id).unwrap();
            match instruction.operation {
                Operation::ContainerConstruct { .. } => {
                    assert_eq!(*id, shape.construct);
                    assert_eq!(state.live[ITEMS], None);
                    state.live[ITEMS] = Some(result(instruction));
                    trace.aliases[ITEMS].extend(state.live[ITEMS]);
                }
                Operation::RootPlace { owner } => {
                    assert_eq!(*id, shape.root);
                    assert_eq!(state.live[ITEMS], Some(EntityId::Value(owner)));
                }
                Operation::BorrowBegin {
                    kind: LoanKind::Shared,
                    ..
                } => {
                    let role = if *id == shape.source_begin {
                        SOURCE
                    } else {
                        assert_eq!(*id, shape.element_begin);
                        assert_eq!(state.route, Route::Untested);
                        assert!(state.live[..ELEMENT].iter().all(Option::is_some));
                        ELEMENT
                    };
                    assert_eq!(state.live[role], None);
                    assert!(!state.source_ended);
                    state.live[role] = Some(result(instruction));
                    trace.aliases[role].extend(state.live[role]);
                }
                Operation::ContainerLength { owner } => {
                    assert_eq!(value(instruction), shape.length);
                    assert_eq!(state.live[SOURCE], Some(owner));
                }
                Operation::ContainerElementPlace { owner, .. } => {
                    assert_eq!(*id, shape.element_place);
                    assert_eq!(state.live[SOURCE], Some(owner));
                    assert_eq!(state.live[ELEMENT], None);
                }
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                } => {
                    assert_eq!(state.live[ELEMENT], Some(EntityId::Loan(loan)));
                    if *id == shape.reads[0] {
                        assert_eq!(state.route, Route::NonNull);
                        assert!(state.live[VIEW].is_some() && !state.inner_read);
                        state.inner_read = true;
                    } else {
                        assert_eq!(*id, shape.reads[1]);
                        assert!(!state.post_read && !state.advanced);
                        assert_eq!(state.live[VIEW], None);
                        match state.route {
                            Route::Null => assert!(!state.inner_read && !state.member_read),
                            Route::NonNull => assert!(state.inner_read && state.member_read),
                            Route::Untested => {
                                panic!("post read must follow a real null/non-null route")
                            }
                        }
                        state.post_read = true;
                    }
                }
                Operation::HeapFieldRead { receiver, .. } => {
                    assert_eq!(*id, shape.member);
                    assert_eq!(state.route, Route::NonNull);
                    assert_eq!(state.live[VIEW], Some(EntityId::Loan(receiver)));
                    assert!(state.inner_read && !state.member_read && !state.post_read);
                    state.member_read = true;
                }
                Operation::BorrowEnd { loan } => {
                    let role = only(state.live.iter().enumerate().filter_map(|(role, entity)| {
                        (*entity == Some(EntityId::Loan(loan))).then_some(role)
                    }));
                    match role {
                        VIEW => {
                            assert_eq!(state.route, Route::NonNull);
                            assert!(state.inner_read && state.member_read && !state.post_read);
                            assert!(state.live[..VIEW].iter().all(Option::is_some));
                            assert!(source_origin(&instruction.origin, spans.broken));
                        }
                        ELEMENT => {
                            assert!(state.post_read && !state.advanced);
                            assert_eq!(state.live[VIEW], None);
                            assert!(state.live[..ELEMENT].iter().all(Option::is_some));
                            assert!(source_origin(&instruction.origin, spans.outer));
                        }
                        SOURCE => {
                            assert_eq!(block.id, shape.exhaustion);
                            assert_eq!(state.route, Route::Untested);
                            assert!(state.live[ELEMENT..].iter().all(Option::is_none));
                            assert!(!state.source_ended);
                            assert!(source_origin(&instruction.origin, spans.outer));
                            state.source_ended = true;
                        }
                        _ => panic!(
                            "only this iteration's proof/element or the exhausted source ends"
                        ),
                    }
                    state.live[role] = None;
                    trace.ends[role].insert(*id);
                }
                Operation::Drop { owner } => {
                    assert_eq!(block.id, shape.exhaustion);
                    assert!(state.source_ended);
                    assert!(state.live[SOURCE..].iter().all(Option::is_none));
                    let role = if state.live[ITEMS].is_some() {
                        ITEMS
                    } else {
                        CURRENT
                    };
                    assert_eq!(state.live[role].take(), Some(EntityId::Value(owner)));
                    assert!(source_origin(&instruction.origin, spans.bindings[role]));
                    trace.drops[role].insert(*id);
                }
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    ..
                } => {
                    assert_eq!(*id, shape.increment);
                    assert!(state.post_read && !state.advanced);
                    assert!(state.live[ELEMENT..].iter().all(Option::is_none));
                    state.advanced = true;
                }
                Operation::CheckedArithmetic {
                    operator: CheckedArithmeticOperator::Add,
                    ..
                } => {
                    assert!(
                        spans
                            .additions
                            .iter()
                            .any(|span| source_origin(&instruction.origin, *span))
                    );
                }
                Operation::Constant(_) | Operation::Compare { .. } => {}
                _ => panic!("no alternate loan, owner replacement or consuming operation"),
            }
        }
        trace.exits.entry(id).or_default().insert(state);
        match term(block) {
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => {
                assert_eq!(id, shape.inner);
                assert_eq!(state.live[CURRENT], Some(EntityId::Value(*owner)));
                assert!(state.live[..VIEW].iter().all(Option::is_some));
                assert_eq!(state.live[VIEW], None);
                assert_eq!(state.route, Route::Untested);
                let mut null = transport(function, when_null, state, false);
                null.route = Route::Null;
                pending.push((when_null.target, null));
                let mut nonnull = transport(function, when_non_null, state, true);
                nonnull.route = Route::NonNull;
                nonnull.live[VIEW] = Some(EntityId::Loan(*view));
                pending.push((when_non_null.target, nonnull));
            }
            TerminatorKind::Return { .. } => {
                assert_eq!(id, shape.exhaustion);
                assert!(term_at(block, spans.returned));
                assert!(state.source_ended && state.live.iter().all(Option::is_none));
                returns.insert(id);
            }
            TerminatorKind::Abort => {
                assert!(!state.source_ended && !state.advanced);
                assert!(state.live[..VIEW].iter().all(Option::is_some));
                assert_eq!(
                    state.live[VIEW].is_some(),
                    state.route == Route::NonNull && !state.post_read
                );
            }
            _ => {
                for edge in edges(block) {
                    let mut next = transport(function, edge, state, false);
                    if edge.target == shape.header && id != entry.id {
                        assert!(state.post_read && state.advanced);
                        assert!(state.live[ELEMENT..].iter().all(Option::is_none));
                        next.route = Route::Untested;
                        next.inner_read = false;
                        next.member_read = false;
                        next.post_read = false;
                        next.advanced = false;
                    }
                    pending.push((edge.target, next));
                }
            }
        }
    }
    assert_eq!(returns.len(), 1);
    trace
}

fn assert_scalars(function: &Function, shape: &Shape) {
    let mut pending = vec![(function.entry_block().unwrap(), shape.zero, shape.length)];
    let mut seen = BTreeSet::new();
    let mut backedges = BTreeSet::new();
    while let Some((id, mut cursor, length)) = pending.pop() {
        if !seen.insert((id, cursor, length)) {
            continue;
        }
        let block = function.block(id).unwrap();
        for id in &block.instructions {
            let instruction = function.instruction(*id).unwrap();
            if *id == shape.element_place {
                assert!(
                    matches!(instruction.operation, Operation::ContainerElementPlace { index, .. } if index == cursor)
                );
            }
            if *id == shape.increment {
                assert!(
                    matches!(instruction.operation, Operation::Binary { operator: BinaryOperator::Add, left, right }
                    if left == cursor && right == shape.step)
                );
                cursor = value(instruction);
            }
        }
        if id == shape.header {
            let (condition, _, _) = conditional(block);
            assert!(matches!(definition(function, condition).operation,
                Operation::Compare { operator: ComparisonOperator::LessThan, left, right }
                if left == cursor && right == length));
        }
        for edge in edges(block) {
            // Exhaustion and Abort do not need provider scalars. Other edges either
            // rebind the exact column or retain the same dominating SSA definition.
            if edge.target == shape.exhaustion
                || matches!(
                    term(function.block(edge.target).unwrap()),
                    TerminatorKind::Abort
                )
            {
                continue;
            }
            if edge.target == shape.header && id != function.entry_block().unwrap() {
                assert_eq!(
                    cursor,
                    value(function.instruction(shape.increment).unwrap())
                );
                assert_eq!(id, function.instruction(shape.increment).unwrap().block);
                backedges.insert(id);
            }
            pending.push((
                edge.target,
                carry_scalar(function, edge, cursor),
                carry_scalar(function, edge, length),
            ));
        }
    }
    assert!(seen.iter().any(|(id, _, _)| *id == shape.body));
    assert_eq!(backedges.len(), 1);
}

fn normal_path(function: &Function, mut id: BlockId, target: BlockId) -> Vec<BlockId> {
    let mut path = Vec::new();
    while id != target {
        assert!(
            !path.contains(&id),
            "the selected arm reaches the actual join/backedge"
        );
        path.push(id);
        id = match term(function.block(id).unwrap()) {
            TerminatorKind::Branch(edge) => edge.target,
            TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                let checked = definition(function, *condition);
                assert!(matches!(
                    checked.operation,
                    Operation::CheckedArithmetic {
                        operator: CheckedArithmeticOperator::Add,
                        ..
                    }
                ));
                assert_eq!(checked.results.get(1), Some(&EntityId::Value(*condition)));
                assert!(matches!(
                    term(function.block(when_true.target).unwrap()),
                    TerminatorKind::Abort
                ));
                when_false.target
            }
            _ => panic!("inner null/break and post-item paths have no other exit or retest"),
        };
    }
    assert!(!path.is_empty());
    path
}

fn assert_live(trace: &Trace, state: State, entities: &[EntityId]) {
    for entity in entities {
        for (role, aliases) in trace.aliases.iter().enumerate() {
            if aliases.contains(entity) {
                assert_eq!(
                    state.live[role],
                    Some(*entity),
                    "stale role {role} operand or edge column"
                );
            }
        }
    }
}

fn transport(function: &Function, edge: &Edge, mut state: State, birth: bool) -> State {
    let target = function.block(edge.target).unwrap();
    assert_eq!(
        target.parameters.len(),
        edge.arguments.len() + usize::from(birth)
    );
    for entity in state.live.iter_mut().flatten() {
        let column = only(
            edge.arguments
                .iter()
                .enumerate()
                .filter_map(|(column, argument)| (*argument == *entity).then_some(column)),
        );
        let next = target.parameters[column];
        assert_eq!(
            function.entity(*entity).unwrap().ty,
            function.entity(next).unwrap().ty
        );
        *entity = next;
    }
    state
}

fn carry_scalar(function: &Function, edge: &Edge, value: ValueId) -> ValueId {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(column, argument)| (*argument == EntityId::Value(value)).then_some(column))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        let definition = match function.entity(EntityId::Value(value)).unwrap().definition {
            Definition::BlockParameter { block, .. } => block,
            Definition::InstructionResult { instruction, .. } => {
                function.instruction(instruction).unwrap().block
            }
        };
        let mut pending = vec![function.entry_block().unwrap()];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if id == definition || !seen.insert(id) {
                continue;
            }
            assert_ne!(
                id, edge.target,
                "unchanged scalar identity must dominate its target"
            );
            pending.extend(term(function.block(id).unwrap()).targets());
        }
        value
    } else {
        let EntityId::Value(next) =
            function.block(edge.target).unwrap().parameters[only(columns.into_iter())]
        else {
            panic!("actual scalar operand column")
        };
        next
    }
}

fn term_at(block: &Block, expected: Span) -> bool {
    source_origin(&block.terminator.as_ref().unwrap().origin, expected)
}

fn source_origin(origin: &Origin, expected: Span) -> bool {
    // The independent AST has another SourceMap; Program source identity is locked separately.
    matches!(origin, Origin::Source(span) if span.start() == expected.start() && span.end() == expected.end())
}

fn definition(function: &Function, value: ValueId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(EntityId::Value(value)).unwrap().definition
    else {
        panic!("actual operation result")
    };
    function.instruction(instruction).unwrap()
}

fn result(instruction: &Instruction) -> EntityId {
    only(instruction.results.iter().copied())
}

fn value(instruction: &Instruction) -> ValueId {
    let EntityId::Value(value) = result(instruction) else {
        panic!("one scalar result")
    };
    value
}

fn assert_integer(function: &Function, value: ValueId, expected: i128) {
    assert_eq!(
        definition(function, value).operation,
        Operation::Constant(ScalarConstant::Integer(expected))
    );
}

fn conditional(block: &Block) -> (ValueId, &Edge, &Edge) {
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = term(block)
    else {
        panic!("actual provider conditional")
    };
    (*condition, when_true, when_false)
}

fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            edges(block)
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

fn term(block: &Block) -> &TerminatorKind {
    &block.terminator.as_ref().unwrap().kind
}

fn edges(block: &Block) -> Vec<&Edge> {
    match term(block) {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        TerminatorKind::Return { .. } | TerminatorKind::Abort => Vec::new(),
        _ => panic!("no borrowed return in this ordinary fixture"),
    }
}

#[track_caller]
fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
