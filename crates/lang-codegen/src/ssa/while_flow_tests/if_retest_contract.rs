//! Query-only owner/view identities across conditional replacement and nullable re-test.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    lexer::lex,
    parser::{Expression, LiteralKind, Statement, parse_file},
    source::{SourceMap, Span},
};

use crate::ssa::model::{
    Block, BlockId, Definition, Edge, EntityId, EntityType, Function, InstructionId, LoanId,
    LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    owner: Option<ValueId>,
    view: Option<LoanId>,
    replacement: Option<ValueId>,
    first: Option<bool>,
    second: Option<bool>,
    null_seen: bool,
    old_ended: bool,
    old_dropped: bool,
    read: bool,
    if_ended: bool,
    if_dropped: bool,
}

#[derive(Default)]
struct Trace {
    entries: BTreeMap<BlockId, BTreeSet<State>>,
    before: BTreeMap<InstructionId, BTreeSet<State>>,
    exits: BTreeMap<BlockId, BTreeSet<State>>,
    owners: BTreeSet<ValueId>,
    old_views: BTreeSet<LoanId>,
    if_views: BTreeSet<LoanId>,
    old_ends: BTreeSet<InstructionId>,
    old_drops: BTreeSet<InstructionId>,
    if_ends: BTreeSet<InstructionId>,
    if_drops: BTreeSet<InstructionId>,
}

impl Trace {
    fn record(&mut self, state: State) {
        self.owners.extend(state.owner);
        self.owners.extend(state.replacement);
        if state.second.is_none() {
            self.old_views.extend(state.view);
        } else {
            self.if_views.extend(state.view);
        }
    }

    fn assert_operands(&self, state: State, operands: &[EntityId]) {
        for entity in operands {
            match *entity {
                EntityId::Value(owner) if self.owners.contains(&owner) => {
                    assert_eq!(state.owner, Some(owner), "use only the current live owner");
                }
                EntityId::Loan(view)
                    if self.old_views.contains(&view) || self.if_views.contains(&view) =>
                {
                    assert_eq!(state.view, Some(view), "ended or stale proof cannot revive");
                }
                _ => {}
            }
        }
    }
}

struct Spans {
    outer: Span,
    first: Span,
    second: Span,
    replacement: Span,
    read: Span,
    broken: Span,
}

pub(super) fn assert_retest(program: &Program, source: &str, unit: bool) {
    let spans = source_spans(source, unit);
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            source_origin(&block.terminator.as_ref().unwrap().origin, spans.outer)
                && matches!(term(block), TerminatorKind::NullableBranch { .. })
        })
    }));
    let at = |span| {
        only(function.blocks.iter().filter(|block| {
            source_origin(&block.terminator.as_ref().unwrap().origin, span)
                && matches!(
                    term(block),
                    TerminatorKind::NullableBranch { .. } | TerminatorKind::Conditional { .. }
                )
        }))
    };
    let outer = at(spans.outer);
    let first = at(spans.first);
    let second = at(spans.second);
    let source_id = outer.terminator.as_ref().unwrap().origin.span().source_id();
    for block in [first, second] {
        assert_eq!(
            block.terminator.as_ref().unwrap().origin.span().source_id(),
            source_id
        );
    }
    assert_eq!(
        function
            .blocks
            .iter()
            .filter(|block| { matches!(term(block), TerminatorKind::NullableBranch { .. }) })
            .count(),
        2,
        "the while and re-test have independent real proof births"
    );
    let (outer_owner, body_owner, outer_view) = birth(function, outer);
    let (tested_owner, if_owner, if_view) = birth(function, second);
    assert_ne!(outer_view, if_view);
    let TerminatorKind::NullableBranch {
        when_non_null: outer_true,
        when_null: outer_false,
        ..
    } = term(outer)
    else {
        unreachable!()
    };
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = term(first)
    else {
        panic!("the first source if is the actual flag split")
    };
    let first_join = common_join(function, when_true.target, when_false.target);
    let TerminatorKind::NullableBranch {
        when_non_null: if_true,
        when_null: if_false,
        ..
    } = term(second)
    else {
        unreachable!()
    };
    let second_join = common_join(function, if_true.target, if_false.target);
    let loop_exit = common_join(function, second_join, outer_false.target);
    assert_ne!(first_join, second_join);
    assert_ne!(second_join, loop_exit);
    for join in [first_join, second_join, loop_exit] {
        assert_eq!(
            incoming(function, join).len(),
            2,
            "two actual incoming edges"
        );
    }

    let EntityType::Value(nullable) = function.entity(EntityId::Value(outer_owner)).unwrap().ty
    else {
        panic!("the outer test reads owned nullable storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("registered nullable owner type")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("registered class owner")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("registered class payload")
    };
    let (field, integer) = only(fields.iter().enumerate().filter(|(_, ty)| {
        module.type_kind(**ty)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
    }));
    let read =
        only(function.instructions.iter().filter(|instruction| {
            matches!(instruction.operation, Operation::HeapFieldRead { .. })
        }));
    assert!(source_origin(&read.origin, spans.read));
    assert_eq!(read.origin.span().source_id(), source_id);
    assert!(matches!(read.operation,
        Operation::HeapFieldRead { field: actual, .. } if actual == field));
    let read_value = only(read.results.iter().copied());
    assert_eq!(
        function.entity(read_value).unwrap().ty,
        EntityType::Value(*integer)
    );
    let replacement = only(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableNull { .. })),
    );
    assert!(source_origin(&replacement.origin, spans.replacement));
    assert_eq!(replacement.origin.span().source_id(), source_id);
    assert!(matches!(replacement.operation,
        Operation::NullableNull { nullable: actual } if actual == nullable));

    let trace = trace(
        function,
        outer_true.target,
        State {
            owner: Some(body_owner),
            view: Some(outer_view),
            replacement: None,
            first: None,
            second: None,
            null_seen: false,
            old_ended: false,
            old_dropped: false,
            read: false,
            if_ended: false,
            if_dropped: false,
        },
        first.id,
        second.id,
        replacement.id,
        read.id,
    );
    assert_eq!(
        trace.old_ends.len(),
        2,
        "both flag arms retire their inherited view"
    );
    assert_eq!(
        trace.old_drops.len(),
        1,
        "only replacement drops the old owner"
    );
    assert_eq!(
        trace.if_ends.len(),
        1,
        "only the non-null re-test arm owns a view"
    );
    assert_eq!(
        trace.if_drops.len(),
        2,
        "both re-test arms drop their branch owner"
    );
    assert!(trace.old_views.is_disjoint(&trace.if_views));
    assert!(trace.if_views.contains(&if_view));
    assert!(trace.owners.contains(&if_owner));
    for owner in &trace.owners {
        assert_eq!(
            function.entity(EntityId::Value(*owner)).unwrap().ty,
            EntityType::Value(nullable)
        );
    }
    for view in trace.old_views.iter().chain(&trace.if_views) {
        assert_eq!(
            function.entity(EntityId::Loan(*view)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
    }

    let joined_owner = only(
        trace.entries[&first_join]
            .iter()
            .map(|state| {
                assert!(state.first.is_some());
                assert!(state.old_ended);
                assert_eq!(state.old_dropped, state.first.unwrap());
                assert_eq!(state.null_seen, state.first.unwrap());
                assert_eq!(state.view, None);
                assert_eq!(state.replacement, None);
                state
                    .owner
                    .expect("both flag arms retain one live nullable owner")
            })
            .collect::<BTreeSet<_>>()
            .into_iter(),
    );
    let Definition::BlockParameter {
        block,
        index: owner_column,
    } = function
        .entity(EntityId::Value(joined_owner))
        .unwrap()
        .definition
    else {
        panic!("the first join has an actual owner parameter")
    };
    assert_eq!(block, first_join);
    let mut incoming_arms = BTreeSet::new();
    for (predecessor, edge) in incoming(function, first_join) {
        for state in &trace.exits[&predecessor] {
            incoming_arms.insert(state.first.unwrap());
            assert_eq!(
                edge.arguments[owner_column],
                EntityId::Value(state.owner.unwrap())
            );
            assert!(state.old_ended);
            assert_eq!(state.old_dropped, state.first.unwrap());
        }
    }
    assert_eq!(incoming_arms, BTreeSet::from([false, true]));
    for state in &trace.entries[&second.id] {
        assert_eq!(
            state.owner,
            Some(tested_owner),
            "re-test reads the joined owner"
        );
        assert_eq!(state.view, None, "no while proof survives the first join");
    }
    for join in [second_join, loop_exit] {
        let states = &trace.entries[&join];
        assert_eq!(
            states
                .iter()
                .map(|state| state.second.unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([false, true])
        );
        for state in states {
            assert_finished(*state);
        }
    }
    // Locate break by its actual edge into the while exit. Its lowering origin is
    // the containing While span, rather than the Break expression span.
    assert!(spans.second.end() <= spans.broken.start());
    assert!(spans.broken.end() <= spans.outer.end());
    let break_predecessor = only(
        incoming(function, loop_exit)
            .into_iter()
            .filter_map(|(id, _)| trace.exits.contains_key(&id).then_some(id)),
    );
    let break_block = function.block(break_predecessor).unwrap();
    assert!(matches!(term(break_block), TerminatorKind::Branch(edge) if edge.target == loop_exit));
    assert!(source_origin(
        &break_block.terminator.as_ref().unwrap().origin,
        spans.outer
    ));
    for state in &trace.exits[&break_predecessor] {
        assert_finished(*state);
    }

    // Recheck with every discovered identity, including aliases on the other arm.
    // Stale operands, duplicate drops and ended proof transport are all forbidden.
    for (id, states) in &trace.entries {
        for state in states {
            trace.assert_operands(*state, &function.block(*id).unwrap().parameters);
        }
    }
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
            Operation::NullableTake { .. } | Operation::NullableWrap { .. }
        ));
    }
    assert_no_proof_path(function, outer_false.target, &trace, read.id);
}

fn trace(
    function: &Function,
    entry: BlockId,
    initial: State,
    first: BlockId,
    second: BlockId,
    replacement: InstructionId,
    read: InstructionId,
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
            if *id == replacement {
                assert_eq!(state.first, Some(true));
                assert_eq!(state.second, None);
                assert!(!state.null_seen, "one replacement on the true flag arm");
                let EntityId::Value(value) = only(instruction.results.iter().copied()) else {
                    panic!("the replacement is the actual NullableNull result")
                };
                state.null_seen = true;
                if state.old_dropped {
                    assert_eq!(state.owner, None);
                    state.owner = Some(value);
                } else {
                    state.replacement = Some(value);
                }
            } else if *id == read {
                assert_eq!(state.second, Some(true));
                assert!(!state.read);
                assert!(matches!(instruction.operation,
                    Operation::HeapFieldRead { receiver, .. } if state.view == Some(receiver)));
                state.read = true;
            } else {
                match instruction.operation {
                    Operation::BorrowEnd { loan } if state.view == Some(loan) => {
                        if state.second.is_none() {
                            assert!(state.first.is_some());
                            assert!(!state.old_ended);
                            state.old_ended = true;
                            trace.old_ends.insert(*id);
                        } else {
                            assert_eq!(state.second, Some(true));
                            assert!(state.read, "field read precedes If-view retirement");
                            assert!(!state.if_ended);
                            state.if_ended = true;
                            trace.if_ends.insert(*id);
                        }
                        state.view = None;
                    }
                    Operation::Drop { owner } if state.owner == Some(owner) => {
                        assert_eq!(state.view, None, "end the paired proof before owner drop");
                        if let Some(second) = state.second {
                            assert_eq!(state.read, second);
                            assert_eq!(state.if_ended, second);
                            assert!(!state.if_dropped);
                            state.if_dropped = true;
                            state.owner = None;
                            trace.if_drops.insert(*id);
                        } else {
                            assert_eq!(state.first, Some(true));
                            assert!(state.old_ended);
                            assert!(!state.old_dropped);
                            state.old_dropped = true;
                            state.owner = state.replacement.take();
                            trace.old_drops.insert(*id);
                        }
                    }
                    _ => {}
                }
            }
            trace.record(state);
        }
        trace.exits.entry(block.id).or_default().insert(state);
        match term(block) {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => {
                assert_eq!(
                    block.id, first,
                    "the only scalar split is the source flag if"
                );
                assert_eq!(state.first, None);
                assert!(!state.old_ended && !state.old_dropped);
                for (edge, selected) in [(when_true, true), (when_false, false)] {
                    let mut next = carry_state(function, &dominators, edge, state);
                    next.first = Some(selected);
                    pending.push((edge.target, next));
                }
            }
            TerminatorKind::NullableBranch {
                owner,
                when_non_null,
                when_null,
                view,
            } => {
                assert_eq!(block.id, second, "no untracked proof split");
                assert_eq!(state.owner, Some(*owner));
                assert_eq!(state.view, None);
                assert_eq!(state.second, None);
                assert!(state.old_ended);
                assert_eq!(state.old_dropped, state.first.unwrap());
                assert_eq!(state.null_seen, state.first.unwrap());
                for (edge, selected) in [(when_non_null, true), (when_null, false)] {
                    let mut next = carry_state(function, &dominators, edge, state);
                    next.second = Some(selected);
                    next.view = selected.then_some(*view);
                    pending.push((edge.target, next));
                }
            }
            TerminatorKind::Branch(edge) => {
                pending.push((edge.target, carry_state(function, &dominators, edge, state)));
            }
            TerminatorKind::Return { .. } => assert_finished(state),
            _ => panic!("this ordinary break fixture has no other exit"),
        }
    }
    trace
}

fn assert_finished(state: State) {
    assert!(state.second.is_some());
    assert!(state.if_dropped);
    assert_eq!(state.read, state.second.unwrap());
    assert_eq!(state.if_ended, state.second.unwrap());
    assert_eq!(state.owner, None);
    assert_eq!(state.view, None);
    assert_eq!(state.replacement, None);
}

fn carry_state(
    function: &Function,
    dominators: &BTreeMap<BlockId, BTreeSet<BlockId>>,
    edge: &Edge,
    mut state: State,
) -> State {
    assert_eq!(
        state.replacement, None,
        "install replacement before leaving its arm"
    );
    if let Some(owner) = state.owner {
        let EntityId::Value(next) = carry(function, dominators, edge, EntityId::Value(owner))
        else {
            panic!("the live owner follows its real operand column")
        };
        state.owner = Some(next);
    }
    if let Some(view) = state.view {
        let EntityId::Loan(next) = carry(function, dominators, edge, EntityId::Loan(view)) else {
            panic!("the live view follows its real operand column")
        };
        state.view = Some(next);
    }
    state
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
            "unchanged identity must dominate its next use"
        );
        entity
    } else {
        function.block(edge.target).unwrap().parameters[only(columns.into_iter())]
    }
}

fn birth(function: &Function, block: &Block) -> (ValueId, ValueId, LoanId) {
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = term(block)
    else {
        panic!("actual nullable proof terminator")
    };
    let target = function.block(when_non_null.target).unwrap();
    let (predecessor, edge) = only(incoming(function, target.id).into_iter());
    assert_eq!(
        predecessor, block.id,
        "the proof target belongs to this guard"
    );
    assert_eq!(
        edge, when_non_null,
        "only its actual non-null edge births the view"
    );
    let column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index)),
    );
    let EntityId::Value(paired_owner) = target.parameters[column] else {
        panic!("the proof is paired with the actual non-null owner column")
    };
    assert_eq!(target.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(target.parameters.last(), Some(&EntityId::Loan(*view)));
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().definition,
        Definition::BlockParameter {
            block: target.id,
            index: target.parameters.len() - 1
        }
    );
    assert!(!when_non_null.arguments.contains(&EntityId::Loan(*view)));
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    assert_eq!(
        function.block(when_null.target).unwrap().parameters.len(),
        when_null.arguments.len()
    );
    (*owner, paired_owner, *view)
}

fn source_spans(source: &str, unit: bool) -> Spans {
    let text = if unit {
        format!("package p\n{source}")
    } else {
        source.to_owned()
    };
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(if unit { "p/range.ko" } else { "range.ko" }, &text)
        .unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let outer = only(file.ast().statements().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Statement::While { .. }).then_some(node.span())
    }));
    let mut branches = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    branches.sort_by_key(|span| span.start());
    let [first, second] = branches.as_slice() else {
        panic!("exactly the flag if and nullable re-test")
    };
    let replacement = only(file.ast().expressions().iter().filter_map(|(_, node)| {
        let Expression::Assignment { value, .. } = node.payload() else {
            return None;
        };
        let value = file.ast().expressions().get(*value).unwrap();
        matches!(value.payload(), Expression::Literal(LiteralKind::Null)).then_some(value.span())
    }));
    let read = only(file.ast().expressions().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Expression::Member { .. }).then_some(node.span())
    }));
    let broken = only(file.ast().expressions().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Expression::Break { .. }).then_some(node.span())
    }));
    Spans {
        outer,
        first: *first,
        second: *second,
        replacement,
        read,
        broken,
    }
}

fn common_join(function: &Function, left: BlockId, right: BlockId) -> BlockId {
    let chain = |mut id| {
        let mut result = Vec::new();
        loop {
            assert!(!result.contains(&id), "each source arm is acyclic");
            result.push(id);
            let TerminatorKind::Branch(edge) = term(function.block(id).unwrap()) else {
                break;
            };
            id = edge.target;
        }
        result
    };
    let left = chain(left);
    let right = chain(right);
    *left
        .iter()
        .find(|id| right.contains(id))
        .expect("both actual arms reach one join")
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
            let predecessors = incoming(function, id)
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| reachable.contains(id))
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

fn assert_no_proof_path(function: &Function, entry: BlockId, trace: &Trace, read: InstructionId) {
    let no_view = |entities: &[EntityId]| {
        for entity in entities {
            if let EntityId::Loan(view) = entity {
                assert!(!trace.old_views.contains(view) && !trace.if_views.contains(view));
            }
        }
    };
    let mut seen = BTreeSet::new();
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let block = function.block(id).unwrap();
        no_view(&block.parameters);
        for id in &block.instructions {
            assert_ne!(*id, read, "the outer null edge cannot reach the field read");
            let instruction = function.instruction(*id).unwrap();
            no_view(&instruction.operation.entities());
            no_view(&instruction.results);
        }
        no_view(&term(block).entities());
        pending.extend(term(block).targets());
    }
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

fn source_origin(origin: &Origin, expected: Span) -> bool {
    // The independent parse has its own SourceMap owner. Coordinates only
    // qualify nodes; all owner, view, block and edge identities come from SSA.
    matches!(origin, Origin::Source(span)
        if span.start() == expected.start() && span.end() == expected.end())
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
        TerminatorKind::Return { .. } => Vec::new(),
        _ => panic!("ordinary if re-test fixture has no other terminator"),
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
