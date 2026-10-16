//! Query the real call-prefix child and its paired nullable proof across argument CFG.
use std::collections::{BTreeMap, BTreeSet};

use crate::ssa::model::{
    Block, BlockId, Definition, Edge, EntityId, EntityType, Function, Instruction, InstructionId,
    LoanId, LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Parent,
    Pending,
    Called,
    ChildEnded,
    ParentEnded,
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    owner: ValueId,
    parent: LoanId,
    child: Option<LoanId>,
    marker: Option<ValueId>,
    stage: Stage,
    final_read: bool,
}

impl State {
    fn live_owner(self) -> bool {
        self.stage != Stage::Dropped
    }

    fn live_parent(self) -> bool {
        self.stage < Stage::ParentEnded
    }

    fn live_child(self) -> bool {
        matches!(self.stage, Stage::Pending | Stage::Called)
    }
}

#[derive(Default)]
struct Trace {
    entries: BTreeMap<BlockId, BTreeSet<State>>,
    before: BTreeMap<InstructionId, BTreeSet<State>>,
    exits: BTreeMap<BlockId, BTreeSet<State>>,
    owners: BTreeSet<ValueId>,
    parents: BTreeSet<LoanId>,
    children: BTreeSet<LoanId>,
}

impl Trace {
    fn record(&mut self, state: State) {
        self.owners.insert(state.owner);
        self.parents.insert(state.parent);
        self.children.extend(state.child);
    }

    fn assert_operands(&self, state: State, operands: &[EntityId]) {
        for entity in operands {
            match *entity {
                EntityId::Value(owner) if self.owners.contains(&owner) => {
                    assert!(state.live_owner(), "an ended owner cannot revive");
                    assert_eq!(owner, state.owner, "use the current paired owner");
                }
                EntityId::Loan(parent) if self.parents.contains(&parent) => {
                    assert!(state.live_parent(), "an ended parent cannot revive");
                    assert_eq!(parent, state.parent, "use the current paired proof");
                }
                EntityId::Loan(child) if self.children.contains(&child) => {
                    assert!(state.live_child(), "an ended child cannot revive");
                    assert_eq!(Some(child), state.child, "use the current pending child");
                }
                _ => {}
            }
        }
    }
}

pub(super) fn assert_pending_child(program: &Program, source: &str, unit: bool) {
    let offset = if unit { "package p\n".len() } else { 0 };
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function
            .blocks
            .iter()
            .any(|block| matches!(term(block), TerminatorKind::NullableBranch { .. }))
    }));
    let outer = only(
        function
            .blocks
            .iter()
            .filter(|block| matches!(term(block), TerminatorKind::NullableBranch { .. })),
    );
    assert_span(
        &outer.terminator.as_ref().unwrap().origin,
        source.find("while (").unwrap() + offset,
        source.rfind("\n    return 0").unwrap() + offset,
    );
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = term(outer)
    else {
        unreachable!()
    };
    let body = function.block(when_non_null.target).unwrap();
    let owner_column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(column, entity)| (*entity == EntityId::Value(*owner)).then_some(column)),
    );
    let EntityId::Value(body_owner) = body.parameters[owner_column] else {
        panic!("the nullable true edge carries its actual owner")
    };
    assert_eq!(body.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(body.parameters.last(), Some(&EntityId::Loan(*view)));
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().definition,
        Definition::BlockParameter {
            block: body.id,
            index: body.parameters.len() - 1,
        }
    );
    assert!(!when_non_null.arguments.contains(&EntityId::Loan(*view)));
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    assert_eq!(
        function.block(when_null.target).unwrap().parameters.len(),
        when_null.arguments.len()
    );
    let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("the guard reads nullable owned storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("registered nullable type")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("registered class owner")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("registered class fields")
    };
    let (text_field, string) = only(
        fields
            .iter()
            .enumerate()
            .filter(|(_, ty)| module.type_kind(**ty) == Some(&SsaTypeKind::StringOwner)),
    );
    let (value_field, integer) = only(fields.iter().enumerate().filter(|(_, ty)| {
        module.type_kind(**ty)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
    }));
    let child_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: *string,
    };
    let projection = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. })
    }));
    let borrowed_start = source.find("current.text").unwrap() + offset;
    assert_span(
        &projection.origin,
        borrowed_start,
        borrowed_start + "current.text".len(),
    );
    let EntityId::Loan(born_child) = only(projection.results.iter().copied()) else {
        panic!("the actual String field projection creates a child loan")
    };
    assert!(matches!(projection.operation,
        Operation::SharedHeapFieldLoan { base, field } if base == *view && field == text_field));
    assert_eq!(projection.block, body.id);
    assert_ne!(born_child, *view);

    let reads = source
        .match_indices("current.value")
        .map(|(start, text)| {
            only(function.instructions.iter().filter(|instruction| {
                matches!(instruction.origin, Origin::Source(span)
                if span.start() == start + offset && span.end() == start + offset + text.len())
                    && matches!(instruction.operation, Operation::HeapFieldRead { field, .. }
                    if field == value_field)
            }))
        })
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 3, "both marker arms and the later parent read");
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::HeapFieldRead { .. })
            })
            .count(),
        reads.len()
    );
    let call_start = source.find("inspect(current.text").unwrap();
    let call_end = source[..source.find("\n        return result").unwrap()]
        .trim_end()
        .len();
    let call = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.origin, Origin::Source(span)
            if span.start() == call_start + offset && span.end() == call_end + offset)
            && matches!(instruction.operation, Operation::DirectCall { .. })
    }));
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &call.operation
    else {
        unreachable!()
    };
    assert_eq!(*receiver, None);
    let [EntityId::Loan(call_child), EntityId::Value(marker)] = arguments.as_slice() else {
        panic!("the real call consumes Borrow String and the owned Int marker")
    };
    assert_ne!(
        *call_child, born_child,
        "call uses the join-rebound child, not its birth ID"
    );
    let callee = module.function(*callee).unwrap();
    assert_eq!(callee.borrow_return, None);
    assert_eq!(callee.carrier_return, None);
    let callee_entry = callee.block(callee.entry_block().unwrap()).unwrap();
    let [
        EntityId::Loan(text_parameter),
        EntityId::Value(marker_parameter),
    ] = callee_entry.parameters.as_slice()
    else {
        panic!("the actual callee ABI has Borrow String and Value Int parameters")
    };
    assert_eq!(
        callee.entity(EntityId::Loan(*text_parameter)).unwrap().ty,
        child_type
    );
    assert_eq!(
        callee
            .entity(EntityId::Value(*marker_parameter))
            .unwrap()
            .ty,
        EntityType::Value(*integer)
    );
    assert_eq!(
        function.entity(EntityId::Value(*marker)).unwrap().ty,
        EntityType::Value(*integer)
    );

    let trace = trace(
        function,
        body.id,
        State {
            owner: body_owner,
            parent: *view,
            child: None,
            marker: None,
            stage: Stage::Parent,
            final_read: false,
        },
        projection,
        call,
        &reads,
    );
    for owner in &trace.owners {
        assert_eq!(
            function.entity(EntityId::Value(*owner)).unwrap().ty,
            EntityType::Value(nullable)
        );
    }
    for parent in &trace.parents {
        assert_eq!(
            function.entity(EntityId::Loan(*parent)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
    }
    for child in &trace.children {
        assert_eq!(
            function.entity(EntityId::Loan(*child)).unwrap().ty,
            child_type
        );
        assert!(!trace.parents.contains(child));
    }
    assert_structure(function, source, offset, &trace, projection, call, &reads);

    let child_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan }
            if trace.children.contains(&loan))
    }));
    let parent_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan }
            if trace.parents.contains(&loan))
    }));
    let owner_drop = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if trace.owners.contains(&owner))
    }));
    for pair in [
        [call, child_end],
        [child_end, reads[2]],
        [reads[2], parent_end],
        [parent_end, owner_drop],
    ] {
        assert_before(function, pair[0], pair[1]);
    }
    assert!(matches!(child_end.operation, Operation::BorrowEnd { loan } if loan == *call_child));
    let Operation::HeapFieldRead {
        receiver: final_parent,
        ..
    } = reads[2].operation
    else {
        unreachable!()
    };
    assert_ne!(
        final_parent, *view,
        "later field read uses the rebound parent"
    );
    assert!(matches!(parent_end.operation, Operation::BorrowEnd { loan } if loan == final_parent));
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| { matches!(instruction.operation, Operation::BorrowEnd { .. }) })
            .count(),
        2,
        "no inner parent retirement or replacement child lifetime"
    );

    // Validate with the complete traced identity sets, including IDs discovered on
    // other paths: stale aliases and ended entities cannot be transported or revived.
    for (id, states) in &trace.before {
        let instruction = function.instruction(*id).unwrap();
        for state in states {
            trace.assert_operands(*state, &instruction.operation.entities());
        }
    }
    for (id, states) in &trace.exits {
        for state in states {
            trace.assert_operands(*state, &term(function.block(*id).unwrap()).entities());
        }
    }
    for instruction in &function.instructions {
        assert!(!matches!(
            instruction.operation,
            Operation::StringClone { .. }
                | Operation::Read { .. }
                | Operation::NullableTake { .. }
                | Operation::NullableWrap { .. }
        ));
        assert!(
            instruction.results.iter().all(|entity| {
                function.entity(*entity).unwrap().ty != EntityType::Value(*string)
            }),
            "the String field never becomes an owned copied-out value"
        );
    }
    assert_null_path(function, when_null.target, &trace, projection.id);
}

fn trace(
    function: &Function,
    entry: BlockId,
    initial: State,
    projection: &Instruction,
    call: &Instruction,
    reads: &[&Instruction],
) -> Trace {
    let dominators = dominators(function);
    let mut trace = Trace::default();
    let mut pending = vec![(entry, initial)];
    while let Some((id, mut state)) = pending.pop() {
        if !trace.entries.entry(id).or_default().insert(state) {
            continue;
        }
        trace.record(state);
        let block = function.block(id).unwrap();
        for id in &block.instructions {
            trace.before.entry(*id).or_default().insert(state);
            let instruction = function.instruction(*id).unwrap();
            if *id == projection.id {
                assert_eq!(state.stage, Stage::Parent);
                assert!(matches!(instruction.operation,
                    Operation::SharedHeapFieldLoan { base, .. } if base == state.parent));
                let EntityId::Loan(child) = only(instruction.results.iter().copied()) else {
                    unreachable!()
                };
                state.child = Some(child);
                state.stage = Stage::Pending;
            } else if *id == call.id {
                assert_eq!(state.stage, Stage::Pending);
                let Operation::DirectCall { arguments, .. } = &instruction.operation else {
                    unreachable!()
                };
                assert_eq!(
                    arguments.as_slice(),
                    [
                        EntityId::Loan(state.child.unwrap()),
                        EntityId::Value(
                            state
                                .marker
                                .expect("actual result of the selected marker arm")
                        )
                    ]
                );
                state.stage = Stage::Called;
            } else if let Some(read) = reads.iter().position(|read| read.id == *id) {
                assert!(matches!(instruction.operation,
                    Operation::HeapFieldRead { receiver, .. } if receiver == state.parent));
                if read == 2 {
                    assert_eq!(state.stage, Stage::ChildEnded);
                    assert!(!state.final_read);
                    state.final_read = true;
                } else {
                    assert_eq!(state.stage, Stage::Pending);
                    assert!(state.marker.is_none(), "exactly one selected marker arm");
                    let EntityId::Value(marker) = only(instruction.results.iter().copied()) else {
                        panic!("the marker comes from the actual Int field read")
                    };
                    state.marker = Some(marker);
                }
            } else {
                match instruction.operation {
                    Operation::BorrowEnd { loan } if Some(loan) == state.child => {
                        assert_eq!(state.stage, Stage::Called);
                        state.stage = Stage::ChildEnded;
                    }
                    Operation::BorrowEnd { loan } if loan == state.parent => {
                        assert_eq!(state.stage, Stage::ChildEnded);
                        assert!(
                            state.final_read,
                            "parent remains active through the last read"
                        );
                        state.stage = Stage::ParentEnded;
                    }
                    Operation::BorrowEnd { .. } => panic!("no unpaired or stale proof retirement"),
                    Operation::Drop { owner } if owner == state.owner => {
                        assert_eq!(state.stage, Stage::ParentEnded);
                        state.stage = Stage::Dropped;
                    }
                    _ => {}
                }
            }
            trace.record(state);
        }
        trace.exits.entry(block.id).or_default().insert(state);
        if matches!(term(block), TerminatorKind::Return { .. }) {
            assert_eq!(
                state.stage,
                Stage::Dropped,
                "normal return closes the paired owner"
            );
            assert!(state.final_read);
        }
        for edge in edges(block) {
            let mut next = state;
            if state.live_owner() {
                let EntityId::Value(owner) =
                    carry(function, &dominators, edge, EntityId::Value(state.owner))
                else {
                    panic!("owner edge column")
                };
                next.owner = owner;
            }
            if state.live_parent() {
                let EntityId::Loan(parent) =
                    carry(function, &dominators, edge, EntityId::Loan(state.parent))
                else {
                    panic!("parent edge column")
                };
                next.parent = parent;
            }
            if state.live_child() {
                let EntityId::Loan(child) = carry(
                    function,
                    &dominators,
                    edge,
                    EntityId::Loan(state.child.unwrap()),
                ) else {
                    panic!("child edge column")
                };
                next.child = Some(child);
            }
            if state.stage == Stage::Pending
                && let Some(marker) = state.marker
            {
                let EntityId::Value(marker) =
                    carry(function, &dominators, edge, EntityId::Value(marker))
                else {
                    panic!("selected marker result column")
                };
                next.marker = Some(marker);
            }
            pending.push((edge.target, next));
        }
    }
    trace
}

fn assert_structure(
    function: &Function,
    source: &str,
    offset: usize,
    trace: &Trace,
    projection: &Instruction,
    call: &Instruction,
    reads: &[&Instruction],
) {
    let if_start = source.find("if (flag)").unwrap();
    let if_block = only(function.blocks.iter().filter(|block| {
        matches!(term(block), TerminatorKind::Conditional { .. })
            && block.terminator.as_ref().unwrap().origin.span().start() == if_start + offset
    }));
    assert_eq!(
        if_block.id, projection.block,
        "child birth precedes the later argument split"
    );
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = term(if_block)
    else {
        unreachable!()
    };
    let inner_start = source.rfind("while (").unwrap();
    let inner_end = inner_start + source[inner_start..].find('}').unwrap() + 1;
    let inner = only(function.blocks.iter().filter(|block| {
        matches!(term(block), TerminatorKind::Conditional { .. })
            && block.terminator.as_ref().unwrap().origin.span().start() == inner_start + offset
    }));
    assert_span(
        &inner.terminator.as_ref().unwrap().origin,
        inner_start + offset,
        inner_end + offset,
    );
    let preheader = function.block(when_true.target).unwrap();
    let TerminatorKind::Branch(inner_entry) = term(preheader) else {
        panic!("then arm enters the readonly loop")
    };
    assert_eq!(inner_entry.target, inner.id);
    let TerminatorKind::Conditional {
        condition,
        when_true: inner_true,
        when_false: inner_false,
    } = term(inner)
    else {
        unreachable!()
    };
    let negation = only(
        function
            .instructions
            .iter()
            .filter(|instruction| instruction.results == [EntityId::Value(*condition)]),
    );
    let Operation::BooleanNot { operand } = negation.operation else {
        panic!("the readonly != null test has an actual negated null check")
    };
    let null_check = only(
        function
            .instructions
            .iter()
            .filter(|instruction| instruction.results == [EntityId::Value(operand)]),
    );
    for state in trace.entries.get(&inner.id).unwrap() {
        assert_eq!(state.stage, Stage::Pending);
        assert!(
            matches!(null_check.operation, Operation::NullableIsNull { owner } if owner == state.owner)
        );
    }
    let broken = function.block(inner_true.target).unwrap();
    let absent = function.block(inner_false.target).unwrap();
    let TerminatorKind::Branch(break_edge) = term(broken) else {
        panic!("inner body breaks")
    };
    let TerminatorKind::Branch(false_edge) = term(absent) else {
        panic!("inner false edge joins its break")
    };
    assert_eq!(break_edge.target, false_edge.target);
    assert_eq!(break_edge.target, reads[0].block);
    assert_eq!(when_false.target, reads[1].block);
    assert_eq!(incoming(function, reads[0].block).len(), 2);
    let then_exit = function.block(reads[0].block).unwrap();
    let else_exit = function.block(reads[1].block).unwrap();
    for block in [then_exit, else_exit] {
        let TerminatorKind::Branch(edge) = term(block) else {
            panic!("both actual marker arms join before the call")
        };
        assert_eq!(edge.target, call.block);
    }
    assert_eq!(incoming(function, call.block).len(), 2);
    for id in [
        if_block.id,
        preheader.id,
        inner.id,
        broken.id,
        absent.id,
        then_exit.id,
        else_exit.id,
    ] {
        for state in trace.exits.get(&id).unwrap() {
            assert_eq!(
                state.stage,
                Stage::Pending,
                "pending child survives every argument/loop edge"
            );
            assert!(state.child.is_some());
        }
    }
    let call_block = function.block(call.block).unwrap();
    for state in trace.before.get(&call.id).unwrap() {
        assert!(
            call_block
                .parameters
                .contains(&EntityId::Loan(state.child.unwrap())),
            "the call consumes the actual join child parameter"
        );
        assert!(
            call_block
                .parameters
                .contains(&EntityId::Value(state.marker.unwrap())),
            "the owned marker is the actual if-result parameter"
        );
    }
}

fn carry(
    function: &Function,
    dominators: &BTreeMap<BlockId, BTreeSet<BlockId>>,
    edge: &Edge,
    entity: EntityId,
) -> EntityId {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| (*argument == entity).then_some(index))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        let definition = match function.entity(entity).unwrap().definition {
            Definition::BlockParameter { block, .. } => block,
            Definition::InstructionResult { instruction, .. } => {
                function.instruction(instruction).unwrap().block
            }
        };
        assert!(
            dominators[&edge.target].contains(&definition),
            "an unmodified identity may survive only when its definition dominates the target"
        );
        entity
    } else {
        let column = only(columns.into_iter());
        function.block(edge.target).unwrap().parameters[column]
    }
}

fn dominators(function: &Function) -> BTreeMap<BlockId, BTreeSet<BlockId>> {
    let entry = function.entry_block().unwrap();
    let mut reachable = BTreeSet::new();
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if reachable.insert(id) {
            pending.extend(term(function.block(id).unwrap()).targets());
        }
    }
    let mut result = reachable
        .iter()
        .map(|id| {
            (
                *id,
                if *id == entry {
                    BTreeSet::from([entry])
                } else {
                    reachable.clone()
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut changed = false;
        for id in reachable.iter().copied().filter(|id| *id != entry) {
            let predecessors = function
                .blocks
                .iter()
                .filter(|block| {
                    reachable.contains(&block.id) && term(block).targets().contains(&id)
                })
                .map(|block| block.id)
                .collect::<Vec<_>>();
            let mut next = result[predecessors.first().unwrap()].clone();
            for predecessor in predecessors.iter().skip(1) {
                next.retain(|dominator| result[predecessor].contains(dominator));
            }
            next.insert(id);
            if next != result[&id] {
                result.insert(id, next);
                changed = true;
            }
        }
        if !changed {
            return result;
        }
    }
}

fn assert_null_path(function: &Function, entry: BlockId, trace: &Trace, projection: InstructionId) {
    let mut pending = vec![entry];
    let mut seen = BTreeSet::new();
    let no_proof = |entities: &[EntityId]| {
        for entity in entities {
            if let EntityId::Loan(loan) = entity {
                assert!(!trace.parents.contains(loan));
                assert!(!trace.children.contains(loan));
            }
        }
    };
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        assert!(
            !trace.entries.contains_key(&id),
            "null path cannot enter the live child path"
        );
        let block = function.block(id).unwrap();
        no_proof(&block.parameters);
        for id in &block.instructions {
            assert_ne!(*id, projection);
            let instruction = function.instruction(*id).unwrap();
            no_proof(&instruction.operation.entities());
            no_proof(&instruction.results);
        }
        no_proof(&term(block).entities());
        pending.extend(term(block).targets());
    }
}

fn assert_before(function: &Function, first: &Instruction, second: &Instruction) {
    assert_eq!(first.block, second.block);
    let block = function.block(first.block).unwrap();
    let position = |id| {
        block
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(
        position(first.id) < position(second.id),
        "call, child end, final read, parent end, paired drop"
    );
}

fn incoming(function: &Function, target: BlockId) -> Vec<&Edge> {
    function
        .blocks
        .iter()
        .flat_map(edges)
        .filter(|edge| edge.target == target)
        .collect()
}

fn assert_span(origin: &Origin, start: usize, end: usize) {
    assert!(
        matches!(origin, Origin::Source(span) if span.start() == start && span.end() == end),
        "actual source span {start}..{end}: {origin:?}"
    );
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
        _ => panic!("ordinary pending child cannot escape through a borrowed return"),
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
