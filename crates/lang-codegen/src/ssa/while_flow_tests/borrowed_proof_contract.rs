//! Independent, query-only identities for the ordinary inner borrowed-for fixture.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::source::Span;

use super::Spans;

use crate::ssa::model::{
    BinaryOperator, Block, BlockId, CheckedArithmeticOperator, ComparisonOperator, Definition,
    Edge, EntityId, EntityType, Function, Instruction, InstructionId, LoanKind, Operation, Origin,
    PlaceAccess, Program, ScalarConstant, SequentialContainerKind, SsaTypeKind, TerminatorKind,
    ValueId,
};

const OWNER: usize = 0;
const VIEW: usize = 1;
const ITEMS: usize = 2;
const SOURCE: usize = 3;
const ELEMENT: usize = 4;

struct Shape {
    outer: BlockId,
    birth: BlockId,
    header: BlockId,
    body: BlockId,
    exhaustion: BlockId,
    construct: InstructionId,
    root: InstructionId,
    source_begin: InstructionId,
    element_place: InstructionId,
    element_begin: InstructionId,
    length: ValueId,
    zero: ValueId,
    step: ValueId,
    members: [InstructionId; 2],
    continued: Vec<BlockId>,
    normal: Vec<BlockId>,
    increments: BTreeSet<InstructionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    live: [Option<EntityId>; 5],
    exhausted: bool,
    final_read: bool,
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

pub(super) fn assert_borrowed_proof(program: &Program, spans: &Spans, unit: bool) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(term(block), TerminatorKind::NullableBranch { .. })
                && term_at(block, spans.outer)
        })
    }));
    let shape = shape(function, spans);
    let mut trace = Trace::default();
    let birth = prefix_and_null(function, &shape, spans, &mut trace);
    trace_borrowed(function, &shape, spans, birth, &mut trace);
    assert_scalar_transport(function, &shape);

    // Types close the set of aliases only after exact operand-column tracing; they
    // never select a replacement owner, source, proof or element of the same type.
    let types = birth
        .live
        .map(|entity| entity.map(|id| function.entity(id).unwrap().ty));
    let owner_type = types[OWNER].unwrap().semantic_type();
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(owner_type) else {
        panic!("the real guard owns nullable storage")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("the nullable view targets the actual Holder owner")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("registered Holder payload")
    };
    let integer = *only(fields.iter());
    assert_eq!(
        module.type_kind(integer),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    let items_type = types[ITEMS].unwrap().semantic_type();
    assert_eq!(
        module.type_kind(items_type),
        Some(&SsaTypeKind::SequentialContainer {
            kind: SequentialContainerKind::List,
            element: integer,
        })
    );
    let shared = |target| EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    };
    let expected = [
        EntityType::Value(owner_type),
        shared(*inner),
        EntityType::Value(items_type),
        shared(items_type),
        shared(integer),
    ];
    for (role, ty) in expected.into_iter().enumerate() {
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
            .filter(|id| function.entity(*id).unwrap().ty == ty)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual, trace.aliases[role],
            "all aliases have the traced role {role}"
        );
    }
    for id in shape.members {
        let read = function.instruction(id).unwrap();
        assert!(matches!(
            read.operation,
            Operation::HeapFieldRead { field: 0, .. }
        ));
        assert_eq!(
            function
                .entity(only(read.results.iter().copied()))
                .unwrap()
                .ty,
            EntityType::Value(integer)
        );
    }
    for (id, states) in &trace.entries {
        for state in states {
            assert_live(&trace, *state, &function.block(*id).unwrap().parameters);
        }
    }
    for (id, states) in &trace.before {
        let instruction = function.instruction(*id).unwrap();
        for state in states {
            assert_live(&trace, *state, &instruction.operation.entities());
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
    assert_eq!(
        trace.entries.len(),
        function.blocks.len(),
        "every actual CFG block was queried"
    );
    for instruction in &function.instructions {
        assert_eq!(
            instruction.origin.span().source_id(),
            function
                .block(shape.outer)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .origin
                .span()
                .source_id()
        );
    }
    assert_eq!(trace.ends[VIEW].len(), 1);
    assert_eq!(trace.ends[SOURCE].len(), 1);
    assert_eq!(trace.ends[ELEMENT].len(), 2);
    assert_eq!(
        trace.drops[OWNER].len(),
        2,
        "nonnull and null each drop their actual owner"
    );
    assert_eq!(trace.drops[ITEMS].len(), 2);
    for (path, continued) in [(&shape.continued, true), (&shape.normal, false)] {
        let end = only(
            path.iter()
                .flat_map(|id| &function.block(*id).unwrap().instructions)
                .filter(|id| trace.ends[ELEMENT].contains(id))
                .copied(),
        );
        let expected = if continued && !unit {
            spans.continued
        } else {
            spans.borrowed
        };
        assert!(source_origin(
            &function.instruction(end).unwrap().origin,
            expected
        ));
        let back = function.block(*path.last().unwrap()).unwrap();
        let state = only(trace.exits[&back.id].iter().copied());
        assert_eq!(
            state.live[ELEMENT], None,
            "an ended element never enters a backedge"
        );
        assert!(state.live[..ELEMENT].iter().all(Option::is_some));
    }
}

fn shape(function: &Function, spans: &Spans) -> Shape {
    let instructions = &function.instructions;
    let outer = only(
        function
            .blocks
            .iter()
            .filter(|block| matches!(term(block), TerminatorKind::NullableBranch { .. })),
    );
    assert!(term_at(outer, spans.outer));
    let TerminatorKind::NullableBranch {
        when_non_null,
        view,
        ..
    } = term(outer)
    else {
        unreachable!()
    };
    let birth = function.block(when_non_null.target).unwrap();
    assert_eq!(
        incoming(function, birth.id),
        vec![(outer.id, when_non_null)]
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
    let construct = only(instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::ContainerConstruct { .. })
    }));
    assert!(source_origin(&construct.origin, spans.construct));
    let Operation::ContainerConstruct { elements, .. } = &construct.operation else {
        unreachable!()
    };
    assert_eq!(elements.len(), 2);
    assert_integer(function, elements[0], 1);
    assert_integer(function, elements[1], 2);
    let root = only(
        instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::RootPlace { .. })),
    );
    assert_eq!(root.block, birth.id);
    let begins = instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        begins.len(),
        2,
        "only the real source and per-iteration element births"
    );
    let source_begin = only(begins.iter().copied().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
            if EntityId::Place(place) == only(root.results.iter().copied()))
    }));
    let length =
        only(instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::ContainerLength { .. })
        }));
    let zero = only(instructions.iter().filter(|instruction| {
        instruction.block == birth.id
            && instruction.operation == Operation::Constant(ScalarConstant::Integer(0))
    }));
    let step = only(instructions.iter().filter(|instruction| {
        instruction.block == birth.id
            && instruction.operation == Operation::Constant(ScalarConstant::Integer(1))
    }));
    for instruction in [root, source_begin, length, zero, step] {
        assert_eq!(instruction.block, birth.id);
        assert!(source_origin(&instruction.origin, spans.borrowed));
    }
    let TerminatorKind::Branch(entry) = term(birth) else {
        panic!("provider preheader")
    };
    let header = function.block(entry.target).unwrap();
    let (condition, entered, exhausted) = conditional(header);
    let guard = definition(function, condition);
    let provider_guard = only(instructions.iter().filter(|instruction| {
        matches!(
            instruction.operation,
            Operation::Compare {
                operator: ComparisonOperator::LessThan,
                ..
            }
        )
    }));
    assert_eq!(provider_guard.id, guard.id);
    assert_eq!(guard.block, header.id);
    assert!(source_origin(&guard.origin, spans.borrowed));
    assert!(term_at(header, spans.borrowed));
    let element_place = only(instructions.iter().filter(|instruction| {
        matches!(
            instruction.operation,
            Operation::ContainerElementPlace { .. }
        )
    }));
    assert_eq!(element_place.block, entered.target);
    let element_begin = only(begins.into_iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
            if EntityId::Place(place) == only(element_place.results.iter().copied()))
    }));
    assert_eq!(element_begin.block, entered.target);
    for instruction in [element_place, element_begin] {
        assert!(source_origin(&instruction.origin, spans.borrowed));
    }
    let branch = only(function.blocks.iter().filter(|block| {
        term_at(block, spans.branch) && matches!(term(block), TerminatorKind::Conditional { .. })
    }));
    assert_eq!(branch.id, entered.target);
    let (condition, continued, normal) = conditional(branch);
    let Operation::Compare {
        operator: ComparisonOperator::Equal,
        left,
        right,
    } = definition(function, condition).operation
    else {
        panic!("the source if compares this iteration's element")
    };
    assert_integer(function, right, 1);
    let EntityId::Loan(element) = only(element_begin.results.iter().copied()) else {
        panic!("the real shared element birth")
    };
    assert_eq!(
        definition(function, left).operation,
        Operation::Read {
            source: PlaceAccess::Loan(element)
        }
    );
    let continued = path_to_header(function, continued.target, header.id);
    let normal = path_to_header(function, normal.target, header.id);
    assert!(
        continued
            .iter()
            .any(|id| term_at(function.block(*id).unwrap(), spans.continued))
    );
    let members = spans.members.map(|span| {
        only(instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::HeapFieldRead { .. })
                && source_origin(&instruction.origin, span)
        }))
        .id
    });
    assert!(normal.contains(&function.instruction(members[0]).unwrap().block));
    assert!(!continued.contains(&function.instruction(members[0]).unwrap().block));
    assert_eq!(
        function.instruction(members[1]).unwrap().block,
        exhausted.target
    );
    let increments = instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    ..
                }
            )
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(increments.len(), 2);
    let mut predecessors = BTreeSet::from([birth.id]);
    for path in [&continued, &normal] {
        let back = function.block(*path.last().unwrap()).unwrap();
        assert!(predecessors.insert(back.id));
    }
    assert_eq!(
        incoming(function, header.id)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>(),
        predecessors
    );
    assert_eq!(
        incoming(function, exhausted.target),
        vec![(header.id, exhausted)]
    );
    Shape {
        outer: outer.id,
        birth: birth.id,
        header: header.id,
        body: entered.target,
        exhaustion: exhausted.target,
        construct: construct.id,
        root: root.id,
        source_begin: source_begin.id,
        element_place: element_place.id,
        element_begin: element_begin.id,
        length: result_value(length),
        zero: result_value(zero),
        step: result_value(step),
        members,
        continued,
        normal,
        increments,
    }
}

fn prefix_and_null(function: &Function, shape: &Shape, spans: &Spans, trace: &mut Trace) -> State {
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let original = only(
        entry
            .parameters
            .iter()
            .copied()
            .filter(|id| matches!(id, EntityId::Value(_))),
    );
    let mut state = State {
        live: [Some(original), None, None, None, None],
        exhausted: false,
        final_read: false,
    };
    record_entry(trace, entry.id, state);
    for id in &entry.instructions {
        trace.before.entry(*id).or_default().insert(state);
        let instruction = function.instruction(*id).unwrap();
        if *id == shape.construct {
            assert_eq!(state.live[ITEMS], None);
            state.live[ITEMS] = Some(only(instruction.results.iter().copied()));
            trace.aliases[ITEMS].extend(state.live[ITEMS]);
        } else {
            assert!(matches!(instruction.operation, Operation::Constant(_)));
        }
    }
    trace.exits.entry(entry.id).or_default().insert(state);
    let outer = function.block(shape.outer).unwrap();
    let (predecessor, edge) = only(incoming(function, outer.id).into_iter());
    assert_eq!(predecessor, entry.id);
    assert!(matches!(term(entry), TerminatorKind::Branch(_)));
    assert!(outer.instructions.is_empty());
    state = transport(function, edge, state);
    record_entry(trace, outer.id, state);
    trace.exits.entry(outer.id).or_default().insert(state);
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = term(outer)
    else {
        unreachable!()
    };
    assert_eq!(state.live[OWNER], Some(EntityId::Value(*owner)));
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    let mut pending = vec![(when_null.target, transport(function, when_null, state))];
    let mut returned = BTreeSet::new();
    while let Some((id, mut null)) = pending.pop() {
        assert!(
            !trace.entries.contains_key(&id),
            "null path cannot enter provider or proof flow"
        );
        record_entry(trace, id, null);
        let block = function.block(id).unwrap();
        for instruction_id in &block.instructions {
            trace
                .before
                .entry(*instruction_id)
                .or_default()
                .insert(null);
            let instruction = function.instruction(*instruction_id).unwrap();
            let Operation::Drop { owner } = instruction.operation else {
                panic!("null path only cleans original owners")
            };
            let role = if null.live[ITEMS].is_some() {
                ITEMS
            } else {
                OWNER
            };
            assert_eq!(null.live[role].take(), Some(EntityId::Value(owner)));
            assert!(source_origin(
                &instruction.origin,
                spans.bindings[usize::from(role == ITEMS)]
            ));
            trace.drops[role].insert(*instruction_id);
        }
        trace.exits.entry(id).or_default().insert(null);
        match term(block) {
            TerminatorKind::Return { .. } => {
                assert!(null.live.iter().all(Option::is_none));
                returned.insert(id);
            }
            TerminatorKind::Branch(edge) => {
                pending.push((edge.target, transport(function, edge, null)))
            }
            _ => panic!("outer null never creates a view or enters a provider"),
        }
    }
    assert_eq!(returned.len(), 1);
    let mut birth = transport(function, when_non_null, state);
    birth.live[VIEW] = Some(EntityId::Loan(*view));
    birth
}

fn trace_borrowed(
    function: &Function,
    shape: &Shape,
    spans: &Spans,
    birth: State,
    trace: &mut Trace,
) {
    let mut pending = vec![(shape.birth, birth)];
    let mut returned = BTreeSet::new();
    while let Some((id, mut state)) = pending.pop() {
        if !record_entry(trace, id, state) {
            continue;
        }
        let block = function.block(id).unwrap();
        if id == shape.header || id == shape.exhaustion {
            assert!(state.live[..ELEMENT].iter().all(Option::is_some));
            assert_eq!(state.live[ELEMENT], None);
            assert!(!state.exhausted && !state.final_read);
        }
        for id in &block.instructions {
            trace.before.entry(*id).or_default().insert(state);
            let instruction = function.instruction(*id).unwrap();
            if shape.increments.contains(id) {
                assert_eq!(
                    state.live[ELEMENT], None,
                    "end the current element before advancing"
                );
            }
            match instruction.operation {
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
                        assert!(state.live[SOURCE].is_some());
                        ELEMENT
                    };
                    assert!(!state.exhausted && !state.final_read);
                    assert_eq!(state.live[role], None);
                    state.live[role] = Some(only(instruction.results.iter().copied()));
                    trace.aliases[role].extend(state.live[role]);
                }
                Operation::ContainerLength { owner } => {
                    assert_eq!(result_value(instruction), shape.length);
                    assert_eq!(state.live[SOURCE], Some(owner));
                }
                Operation::ContainerElementPlace { owner, .. } => {
                    assert_eq!(*id, shape.element_place);
                    assert_eq!(state.live[SOURCE], Some(owner));
                    assert_eq!(state.live[ELEMENT], None);
                    assert!(!state.exhausted);
                }
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                } => {
                    assert_eq!(state.live[ELEMENT], Some(EntityId::Loan(loan)));
                }
                Operation::HeapFieldRead { receiver, .. } => {
                    assert_eq!(state.live[VIEW], Some(EntityId::Loan(receiver)));
                    assert!(state.live[OWNER].is_some() && !state.final_read);
                    if *id == shape.members[1] {
                        assert!(state.exhausted);
                        assert!(state.live[ITEMS..].iter().all(Option::is_none));
                        state.final_read = true;
                    } else {
                        assert_eq!(*id, shape.members[0]);
                        assert!(!state.exhausted);
                        assert!(state.live[ITEMS..].iter().all(Option::is_some));
                    }
                }
                Operation::BorrowEnd { loan } => {
                    let role = only(state.live.iter().enumerate().filter_map(|(role, live)| {
                        (*live == Some(EntityId::Loan(loan))).then_some(role)
                    }));
                    match role {
                        ELEMENT => {
                            assert!(!state.exhausted && !state.final_read);
                            assert!(state.live[..ELEMENT].iter().all(Option::is_some));
                        }
                        SOURCE => {
                            assert_eq!(block.id, shape.exhaustion);
                            assert_eq!(state.live[ELEMENT], None);
                            assert!(!state.exhausted && !state.final_read);
                            assert!(source_origin(&instruction.origin, spans.borrowed));
                            state.exhausted = true;
                        }
                        VIEW => {
                            assert!(state.exhausted && state.final_read);
                            assert!(state.live[ITEMS..].iter().all(Option::is_none));
                            assert!(source_origin(&instruction.origin, spans.bindings[0]));
                        }
                        _ => panic!("only the current element, source or outer view can end"),
                    }
                    state.live[role] = None;
                    trace.ends[role].insert(*id);
                }
                Operation::Drop { owner } => {
                    let role = if state.live[ITEMS] == Some(EntityId::Value(owner)) {
                        ITEMS
                    } else {
                        OWNER
                    };
                    assert!(state.exhausted);
                    assert!(state.live[SOURCE..].iter().all(Option::is_none));
                    if role == ITEMS {
                        assert!(!state.final_read && state.live[VIEW].is_some());
                    } else {
                        assert!(state.final_read && state.live[VIEW].is_none());
                        assert_eq!(state.live[ITEMS], None);
                    }
                    assert_eq!(state.live[role].take(), Some(EntityId::Value(owner)));
                    assert!(source_origin(
                        &instruction.origin,
                        spans.bindings[usize::from(role == ITEMS)]
                    ));
                    trace.drops[role].insert(*id);
                }
                Operation::Constant(_)
                | Operation::Compare { .. }
                | Operation::CheckedArithmetic { .. }
                | Operation::Binary { .. } => {}
                _ => panic!("no alternate copy, reborrow, extraction or consuming operation"),
            }
        }
        trace.exits.entry(block.id).or_default().insert(state);
        match term(block) {
            TerminatorKind::Return { .. } => {
                assert_finished(state);
                returned.insert(block.id);
            }
            TerminatorKind::Abort => {
                if state.final_read {
                    assert_finished(state);
                } else {
                    assert!(!state.exhausted);
                    assert!(
                        state.live.iter().all(Option::is_some),
                        "body overflow does not invent cleanup"
                    );
                }
            }
            TerminatorKind::NullableBranch { .. } => panic!("no second nullable proof birth"),
            _ => {
                for edge in edges(block) {
                    if edge.target == shape.header && block.id != shape.birth {
                        assert_eq!(state.live[ELEMENT], None);
                    }
                    pending.push((edge.target, transport(function, edge, state)));
                }
            }
        }
    }
    assert_eq!(returned.len(), 1);
}

fn assert_scalar_transport(function: &Function, shape: &Shape) {
    let mut pending = vec![(shape.birth, shape.zero, shape.length)];
    let mut seen = BTreeSet::new();
    let mut increments = BTreeSet::new();
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
            if shape.increments.contains(id) {
                assert!(matches!(instruction.operation,
                    Operation::Binary { operator: BinaryOperator::Add, left, right }
                    if left == cursor && right == shape.step));
                assert_integer(function, shape.step, 1);
                cursor = result_value(instruction);
                increments.insert(*id);
            }
        }
        if id == shape.header {
            let (condition, _, _) = conditional(block);
            assert!(matches!(definition(function, condition).operation,
                Operation::Compare { operator: ComparisonOperator::LessThan, left, right }
                if left == cursor && right == length));
        }
        for edge in edges(block) {
            // These scalars are needed by the next provider use, not by exhaustion
            // cleanup or Abort. Single uses dominating scalars; unit also carries them.
            if edge.target == shape.exhaustion
                || matches!(
                    term(function.block(edge.target).unwrap()),
                    TerminatorKind::Abort
                )
            {
                continue;
            }
            if edge.target == shape.header && id != shape.birth {
                let increment = only(
                    block
                        .instructions
                        .iter()
                        .filter(|id| shape.increments.contains(id))
                        .copied(),
                );
                assert_eq!(
                    cursor,
                    result_value(function.instruction(increment).unwrap())
                );
                backedges.insert(increment);
            }
            pending.push((
                edge.target,
                carry_scalar(function, edge, cursor),
                carry_scalar(function, edge, length),
            ));
        }
    }
    assert!(seen.iter().any(|(id, _, _)| *id == shape.body));
    assert_eq!(increments, shape.increments);
    assert_eq!(backedges, shape.increments);
}

fn path_to_header(function: &Function, mut id: BlockId, header: BlockId) -> Vec<BlockId> {
    let mut path = Vec::new();
    while id != header {
        assert!(
            !path.contains(&id),
            "each source arm reaches its real provider backedge"
        );
        path.push(id);
        let block = function.block(id).unwrap();
        id = match term(block) {
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
            _ => panic!("normal and continue arms cannot exit before their backedges"),
        };
    }
    assert!(!path.is_empty());
    path
}

fn record_entry(trace: &mut Trace, id: BlockId, state: State) -> bool {
    for (role, entity) in state.live.iter().enumerate() {
        trace.aliases[role].extend(entity);
    }
    trace.entries.entry(id).or_default().insert(state)
}

fn assert_finished(state: State) {
    assert!(state.exhausted && state.final_read);
    assert!(state.live.iter().all(Option::is_none));
}

fn assert_live(trace: &Trace, state: State, entities: &[EntityId]) {
    for entity in entities {
        for (role, aliases) in trace.aliases.iter().enumerate() {
            if aliases.contains(entity) {
                assert_eq!(
                    state.live[role],
                    Some(*entity),
                    "stale role {role} operand or residual edge column"
                );
            }
        }
    }
}

fn transport(function: &Function, edge: &Edge, mut state: State) -> State {
    let target = function.block(edge.target).unwrap();
    let birth = function.blocks.iter().any(|block| {
        matches!(term(block),
        TerminatorKind::NullableBranch { when_non_null, .. } if when_non_null == edge)
    });
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
                "an uncarried scalar must really dominate the target"
            );
            pending.extend(term(function.block(id).unwrap()).targets());
        }
        value
    } else {
        let EntityId::Value(next) =
            function.block(edge.target).unwrap().parameters[only(columns.into_iter())]
        else {
            panic!("actual provider scalar column")
        };
        next
    }
}

fn term_at(block: &Block, expected: Span) -> bool {
    source_origin(&block.terminator.as_ref().unwrap().origin, expected)
}

fn source_origin(origin: &Origin, expected: Span) -> bool {
    matches!(origin, Origin::Source(span)
        if span.start() == expected.start() && span.end() == expected.end())
}

fn definition(function: &Function, value: ValueId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(EntityId::Value(value)).unwrap().definition
    else {
        panic!("actual instruction result")
    };
    function.instruction(instruction).unwrap()
}

fn result_value(instruction: &Instruction) -> ValueId {
    let EntityId::Value(value) = only(instruction.results.iter().copied()) else {
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
        panic!("actual conditional")
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
        _ => panic!("this fixture does not return borrowed storage"),
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
