//! Independent queries over the four ordinary nullable-while/numeric-for programs.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    lexer::lex,
    parser::{Expression, Statement, parse_file},
    source::{SourceMap, Span},
};

use crate::ssa::model::{
    BinaryOperator, Block, BlockId, CheckedArithmeticOperator, ComparisonOperator, Definition,
    Edge, EntityId, EntityType, Function, Instruction, InstructionId, LoanId, LoanKind, Operation,
    Origin, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
};

struct Spans {
    outer: Span,
    numeric: Span,
    range: Span,
    branch: Span,
    continued: Span,
    reads: [Span; 2],
}

struct Numeric {
    empty: BlockId,
    body: BlockId,
    exhaustion: BlockId,
    first: ValueId,
    last: ValueId,
    terminals: BTreeSet<BlockId>,
    increments: BTreeSet<InstructionId>,
    inner_read: InstructionId,
    final_read: InstructionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    owner: Option<ValueId>,
    view: Option<LoanId>,
    read: bool,
    ended: bool,
    dropped: bool,
}

#[derive(Default)]
struct Trace {
    entries: BTreeMap<BlockId, BTreeSet<State>>,
    before: BTreeMap<InstructionId, BTreeSet<State>>,
    exits: BTreeMap<BlockId, BTreeSet<State>>,
    owners: BTreeSet<ValueId>,
    views: BTreeSet<LoanId>,
    ends: BTreeSet<InstructionId>,
    drops: BTreeSet<InstructionId>,
}

pub(super) fn assert_numeric_proof(program: &Program, source: &str, unit: bool) {
    let spans = source_spans(source, unit);
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(term(block), TerminatorKind::NullableBranch { .. })
                && source_origin(&block.terminator.as_ref().unwrap().origin, spans.outer)
        })
    }));
    let outer = only(
        function
            .blocks
            .iter()
            .filter(|block| matches!(term(block), TerminatorKind::NullableBranch { .. })),
    );
    assert!(source_origin(
        &outer.terminator.as_ref().unwrap().origin,
        spans.outer
    ));
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
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
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    let body_owner = carry_value(function, when_non_null, *owner);
    let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("the guard reads actual owned nullable storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("the original owner has a registered nullable type")
    };
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(*inner)
    else {
        panic!("the view targets the registered class owner")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*payload) else {
        panic!("the class has a registered field layout")
    };
    let (field, integer) = only(fields.iter().enumerate());
    assert_eq!(
        module.type_kind(*integer),
        Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    );
    let proof_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: *inner,
    };
    let numeric = numeric_shape(function, &spans, unit);
    for id in [numeric.inner_read, numeric.final_read] {
        let read = function.instruction(id).unwrap();
        assert!(
            matches!(read.operation, Operation::HeapFieldRead { field: actual, .. } if actual == field)
        );
        assert_eq!(
            function
                .entity(only(read.results.iter().copied()))
                .unwrap()
                .ty,
            EntityType::Value(*integer)
        );
    }
    let trace = trace_pairs(function, birth.id, body_owner, *view, &numeric);
    for owner in &trace.owners {
        assert_eq!(
            function.entity(EntityId::Value(*owner)).unwrap().ty,
            EntityType::Value(nullable)
        );
    }
    for view in &trace.views {
        assert_eq!(
            function.entity(EntityId::Loan(*view)).unwrap().ty,
            proof_type
        );
    }
    let all_proofs = function
        .blocks
        .iter()
        .flat_map(|block| &block.parameters)
        .chain(
            function
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.results),
        )
        .filter_map(|entity| match entity {
            EntityId::Loan(loan)
                if matches!(function.entity(*entity).unwrap().ty,
                EntityType::Loan { target, .. } if target == *inner) =>
            {
                Some(*loan)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        all_proofs, trace.views,
        "every payload view is this birth or an actual paired edge column"
    );
    for instruction in &function.instructions {
        assert!(
            !matches!(
                instruction.operation,
                Operation::NullableWrap { .. }
                    | Operation::NullableTake { .. }
                    | Operation::NullableNull { .. }
            ),
            "no replacement owner, extraction or second proof source"
        );
        assert_eq!(
            instruction.origin.span().source_id(),
            outer.terminator.as_ref().unwrap().origin.span().source_id()
        );
    }
    for (id, states) in &trace.before {
        let instruction = function.instruction(*id).unwrap();
        for state in states {
            assert_live_operands(&trace, *state, &instruction.operation.entities());
            if instruction
                .operation
                .entities()
                .iter()
                .any(|entity| tracked(&trace, *entity))
            {
                assert!(
                    matches!(
                        instruction.operation,
                        Operation::HeapFieldRead { .. }
                            | Operation::BorrowEnd { .. }
                            | Operation::Drop { .. }
                    ),
                    "the proof/root cannot be copied, consumed, aliased or used as a fake read loan"
                );
            }
        }
    }
    for (block, states) in &trace.exits {
        for state in states {
            assert_live_operands(
                &trace,
                *state,
                &term(function.block(*block).unwrap()).entities(),
            );
        }
    }
    let end = only(trace.ends.iter().copied());
    let drop = only(trace.drops.iter().copied());
    let final_state = only(trace.before[&numeric.final_read].iter().copied());
    let exhausted = only(trace.entries[&numeric.exhaustion].iter().copied());
    assert_eq!(
        final_state, exhausted,
        "the final Member uses the real exhaustion pair"
    );
    assert_ne!(
        final_state.view,
        Some(*view),
        "exhaustion rebinds the inherited proof"
    );
    assert_eq!(
        only(trace.before[&end].iter().copied()).view,
        final_state.view
    );
    assert_eq!(
        only(trace.before[&drop].iter().copied()).owner,
        final_state.owner
    );
    assert_numeric_scalars(function, &numeric);
    assert_null_path(function, when_null, *owner, &trace);
}

fn numeric_shape(function: &Function, spans: &Spans, unit: bool) -> Numeric {
    let range = only(function.instructions.iter().filter(|instruction| {
        matches!(
            instruction.operation,
            Operation::IntegerRangeConstruct { .. }
        )
    }));
    assert!(source_origin(&range.origin, spans.range));
    let Operation::IntegerRangeConstruct {
        first,
        bound,
        inclusive,
    } = range.operation
    else {
        unreachable!()
    };
    assert!(!inclusive);
    assert_integer(function, first, 0);
    assert_integer(function, bound, 2);
    let snapshot = result_value(range);
    let projections = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::AggregateProject { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        projections.len(),
        3,
        "one evaluated range snapshot supplies first/last/empty"
    );
    let project = |field| {
        only(projections.iter().copied().filter(|instruction| {
            matches!(instruction.operation, Operation::AggregateProject { aggregate, field: actual }
            if aggregate == snapshot && actual == field)
        }))
    };
    let [first, last, empty] = [project(0), project(1), project(2)];
    for projection in [first, last, empty] {
        assert!(source_origin(&projection.origin, spans.numeric));
        assert_eq!(projection.block, empty.block);
    }
    let first = result_value(first);
    let last = result_value(last);
    let (condition, exhausted, entered) = conditional(function.block(empty.block).unwrap());
    assert_eq!(condition, result_value(empty));
    let body = entered.target;
    let exhaustion = exhausted.target;
    assert_ne!(body, exhaustion);
    let reads = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. }))
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 2);
    let at = |span| {
        only(
            reads
                .iter()
                .copied()
                .filter(|instruction| source_origin(&instruction.origin, span)),
        )
    };
    let inner_read = at(spans.reads[0]);
    let final_read = at(spans.reads[1]);
    assert_eq!(
        final_read.block, exhaustion,
        "empty and terminal edges reach the final Member"
    );
    let terminals = function
        .blocks
        .iter()
        .filter_map(|block| {
            let TerminatorKind::Conditional {
                condition,
                when_true,
                ..
            } = term(block)
            else {
                return None;
            };
            (when_true.target == exhaustion
                && matches!(
                    definition(function, *condition).operation,
                    Operation::Compare {
                        operator: ComparisonOperator::Equal,
                        ..
                    }
                ))
            .then_some(block.id)
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        terminals.len(),
        if unit { 1 } else { 2 },
        "unit shares its terminal; single has normal and continue terminals"
    );
    let mut increments = BTreeSet::new();
    let mut backedges = BTreeSet::new();
    for terminal in &terminals {
        let block = function.block(*terminal).unwrap();
        let (_, terminal_true, terminal_false) = conditional(block);
        assert_eq!(terminal_true.target, exhaustion);
        let mut path = BTreeSet::new();
        let mut id = terminal_false.target;
        let increment = loop {
            assert!(
                path.insert(id),
                "the nonterminal arm reaches its increment without looping"
            );
            let block = function.block(id).unwrap();
            let additions = block
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .filter(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::Binary {
                            operator: BinaryOperator::Add,
                            ..
                        }
                    )
                })
                .collect::<Vec<_>>();
            if !additions.is_empty() {
                break only(additions.into_iter());
            }
            let TerminatorKind::Branch(edge) = term(block) else {
                panic!("terminal false reaches unchecked increment")
            };
            id = edge.target;
        };
        let Operation::Binary { right, .. } = increment.operation else {
            unreachable!()
        };
        assert_integer(function, right, 1);
        assert!(
            increments.insert(increment.id),
            "one increment for each real terminal false arm"
        );
        let TerminatorKind::Branch(backedge) = term(function.block(increment.block).unwrap())
        else {
            panic!("the increment returns to the body through its real backedge")
        };
        assert_eq!(backedge.target, body);
        assert!(
            backedge
                .arguments
                .contains(&EntityId::Value(result_value(increment)))
        );
        backedges.insert(increment.block);
    }
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::Binary {
                    operator: BinaryOperator::Add,
                    ..
                }
            ))
            .count(),
        increments.len()
    );
    let body_predecessors = incoming(function, body)
        .into_iter()
        .map(|(block, _)| block)
        .collect::<BTreeSet<_>>();
    let mut expected_body = backedges;
    expected_body.insert(empty.block);
    assert_eq!(
        body_predecessors, expected_body,
        "first entry and actual increment edges close the loop"
    );
    let mut expected_exhaustion = terminals.clone();
    expected_exhaustion.insert(empty.block);
    assert_eq!(
        incoming(function, exhaustion)
            .into_iter()
            .map(|(block, _)| block)
            .collect::<BTreeSet<_>>(),
        expected_exhaustion
    );
    let branch = only(function.blocks.iter().filter(|block| {
        source_origin(&block.terminator.as_ref().unwrap().origin, spans.branch)
            && matches!(term(block), TerminatorKind::Conditional { .. })
    }));
    assert_eq!(branch.id, body);
    let (_, continued, normal) = conditional(branch);
    let continue_path = terminal_path(function, continued.target, &terminals);
    let normal_path = terminal_path(function, normal.target, &terminals);
    assert!(continue_path.iter().any(|id| {
        source_origin(
            &function
                .block(*id)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .origin,
            spans.continued,
        )
    }));
    assert!(!continue_path.contains(&inner_read.block));
    assert!(normal_path.contains(&inner_read.block));
    assert_eq!(
        BTreeSet::from([*continue_path.last().unwrap(), *normal_path.last().unwrap()]),
        terminals
    );
    Numeric {
        empty: empty.block,
        body,
        exhaustion,
        first,
        last,
        terminals,
        increments,
        inner_read: inner_read.id,
        final_read: final_read.id,
    }
}

fn terminal_path(
    function: &Function,
    mut id: BlockId,
    terminals: &BTreeSet<BlockId>,
) -> Vec<BlockId> {
    let mut path = Vec::new();
    loop {
        assert!(
            !path.contains(&id),
            "the selected source arm reaches its terminal"
        );
        path.push(id);
        if terminals.contains(&id) {
            return path;
        }
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
            _ => panic!("a normal/continue arm cannot exit before the numeric terminal"),
        };
    }
}

fn trace_pairs(
    function: &Function,
    entry: BlockId,
    owner: ValueId,
    view: LoanId,
    numeric: &Numeric,
) -> Trace {
    let mut trace = Trace::default();
    let mut pending = vec![(
        entry,
        State {
            owner: Some(owner),
            view: Some(view),
            read: false,
            ended: false,
            dropped: false,
        },
    )];
    let mut returns = BTreeSet::new();
    while let Some((id, mut state)) = pending.pop() {
        if !trace.entries.entry(id).or_default().insert(state) {
            continue;
        }
        trace.owners.extend(state.owner);
        trace.views.extend(state.view);
        let block = function.block(id).unwrap();
        for id in &block.instructions {
            trace.before.entry(*id).or_default().insert(state);
            let instruction = function.instruction(*id).unwrap();
            match instruction.operation {
                Operation::HeapFieldRead { receiver, .. } => {
                    assert_eq!(state.view, Some(receiver), "read the currently paired view");
                    assert!(!state.read && !state.ended && !state.dropped);
                    if *id == numeric.final_read {
                        state.read = true;
                    } else {
                        assert_eq!(*id, numeric.inner_read);
                    }
                }
                Operation::BorrowEnd { loan } if state.view == Some(loan) => {
                    assert!(
                        state.read,
                        "numeric entry, continue, terminal and increment never retire the outer proof"
                    );
                    assert!(!state.ended && !state.dropped);
                    state.ended = true;
                    state.view = None;
                    trace.ends.insert(*id);
                }
                Operation::Drop { owner } if state.owner == Some(owner) => {
                    assert!(
                        state.read && state.ended && !state.dropped,
                        "final Member precedes exactly one parent end then same-owner drop"
                    );
                    assert_eq!(state.view, None);
                    state.dropped = true;
                    state.owner = None;
                    trace.drops.insert(*id);
                }
                _ => {}
            }
        }
        trace.exits.entry(block.id).or_default().insert(state);
        match term(block) {
            TerminatorKind::Return { .. } => {
                assert_finished(state);
                returns.insert(block.id);
            }
            TerminatorKind::Abort => {
                if state.read {
                    assert_finished(state);
                } else {
                    assert!(state.owner.is_some() && state.view.is_some());
                    assert!(
                        !state.ended && !state.dropped,
                        "checked body failure has no invented cleanup"
                    );
                }
            }
            TerminatorKind::NullableBranch { .. } => {
                panic!("the inner numeric loop cannot birth another nullable proof")
            }
            _ => {
                for edge in edges(block) {
                    let mut next = state;
                    next.owner = state.owner.map(|owner| carry_value(function, edge, owner));
                    next.view = state.view.map(|view| {
                        assert!(state.owner.is_some(), "view and owner travel together");
                        let EntityId::Loan(next) =
                            carry_column(function, edge, EntityId::Loan(view))
                        else {
                            panic!("actual view column")
                        };
                        next
                    });
                    pending.push((edge.target, next));
                }
            }
        }
    }
    assert_eq!(returns.len(), 1);
    for id in [numeric.empty, numeric.body, numeric.exhaustion]
        .into_iter()
        .chain(numeric.terminals.iter().copied())
    {
        let state = only(trace.entries[&id].iter().copied());
        assert!(state.owner.is_some() && state.view.is_some());
        assert!(!state.read && !state.ended && !state.dropped);
    }
    trace
}

fn assert_finished(state: State) {
    assert!(state.read && state.ended && state.dropped);
    assert_eq!(state.owner, None);
    assert_eq!(state.view, None);
}

fn tracked(trace: &Trace, entity: EntityId) -> bool {
    match entity {
        EntityId::Value(owner) => trace.owners.contains(&owner),
        EntityId::Loan(view) => trace.views.contains(&view),
        EntityId::Place(_) => false,
    }
}

fn assert_live_operands(trace: &Trace, state: State, entities: &[EntityId]) {
    for entity in entities {
        match *entity {
            EntityId::Value(owner) if trace.owners.contains(&owner) => {
                assert_eq!(state.owner, Some(owner))
            }
            EntityId::Loan(view) if trace.views.contains(&view) => {
                assert_eq!(state.view, Some(view))
            }
            _ => {}
        }
    }
}

// A separate scalar walk identifies current/last through their actual edge columns.
// It never invokes numeric lowering or nullable-proof qualification helpers.
fn assert_numeric_scalars(function: &Function, numeric: &Numeric) {
    let mut pending = vec![(numeric.empty, numeric.first, numeric.last)];
    let mut seen = BTreeSet::new();
    let mut terminals = BTreeSet::new();
    let mut increments = BTreeSet::new();
    let mut closed_backedges = BTreeSet::new();
    while let Some((id, mut current, last)) = pending.pop() {
        if !seen.insert((id, current, last)) {
            continue;
        }
        if id == numeric.exhaustion {
            continue;
        }
        let block = function.block(id).unwrap();
        for id in &block.instructions {
            if numeric.increments.contains(id) {
                let instruction = function.instruction(*id).unwrap();
                assert!(
                    matches!(instruction.operation, Operation::Binary { operator: BinaryOperator::Add, left, .. } if left == current)
                );
                current = result_value(instruction);
                increments.insert(*id);
            }
        }
        if numeric.terminals.contains(&block.id) {
            let (condition, _, _) = conditional(block);
            assert!(
                matches!(definition(function, condition).operation,
                Operation::Compare { operator: ComparisonOperator::Equal, left, right }
                if left == current && right == last),
                "terminal compares this iteration's current against the actual snapshot last"
            );
            terminals.insert(block.id);
        }
        for edge in edges(block) {
            if edge.target == numeric.body && block.id != numeric.empty {
                let increment = only(
                    block
                        .instructions
                        .iter()
                        .copied()
                        .filter(|id| numeric.increments.contains(id)),
                );
                assert_eq!(
                    current,
                    result_value(function.instruction(increment).unwrap())
                );
                closed_backedges.insert(increment);
            }
            let next_current = carry_scalar(function, edge, current);
            let next_last = carry_scalar(function, edge, last);
            pending.push((edge.target, next_current, next_last));
        }
    }
    assert_eq!(terminals, numeric.terminals);
    assert_eq!(increments, numeric.increments);
    assert_eq!(closed_backedges, numeric.increments);
}

fn assert_null_path(function: &Function, edge: &Edge, original: ValueId, trace: &Trace) {
    assert_eq!(
        function.block(edge.target).unwrap().parameters.len(),
        edge.arguments.len()
    );
    let mut pending = vec![(edge.target, Some(carry_value(function, edge, original)))];
    let mut seen = BTreeSet::new();
    let mut drops = BTreeSet::new();
    let mut returns = BTreeSet::new();
    while let Some((id, mut owner)) = pending.pop() {
        if !seen.insert((id, owner)) {
            continue;
        }
        assert!(
            !trace.entries.contains_key(&id),
            "outer-null path never enters numeric proof flow"
        );
        let block = function.block(id).unwrap();
        let no_view = |entities: &[EntityId]| {
            assert!(entities.iter().all(
                |entity| !matches!(entity, EntityId::Loan(view) if trace.views.contains(view))
            ))
        };
        no_view(&block.parameters);
        no_view(&term(block).entities());
        for id in &block.instructions {
            let instruction = function.instruction(*id).unwrap();
            no_view(&instruction.operation.entities());
            no_view(&instruction.results);
            assert!(!matches!(
                instruction.operation,
                Operation::HeapFieldRead { .. }
            ));
            if let Operation::Drop { owner: dropped } = instruction.operation {
                assert_eq!(owner.take(), Some(dropped));
                drops.insert(*id);
            }
        }
        if matches!(term(block), TerminatorKind::Return { .. }) {
            assert_eq!(owner, None);
            returns.insert(block.id);
        } else {
            assert!(!matches!(
                term(block),
                TerminatorKind::NullableBranch { .. } | TerminatorKind::Abort
            ));
            for edge in edges(block) {
                pending.push((
                    edge.target,
                    owner.map(|owner| carry_value(function, edge, owner)),
                ));
            }
        }
    }
    assert_eq!(drops.len(), 1);
    assert_eq!(returns.len(), 1);
}

fn carry_column(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    let column = only(
        edge.arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (*argument == entity).then_some(index)),
    );
    let target = function.block(edge.target).unwrap();
    assert_eq!(target.parameters.len(), edge.arguments.len());
    target.parameters[column]
}

fn carry_value(function: &Function, edge: &Edge, owner: ValueId) -> ValueId {
    // The nullable birth alone appends a view beyond its ordinary arguments.
    let column = only(
        edge.arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (*argument == EntityId::Value(owner)).then_some(index)),
    );
    let EntityId::Value(next) = function.block(edge.target).unwrap().parameters[column] else {
        panic!("actual owner operand column")
    };
    next
}

fn carry_scalar(function: &Function, edge: &Edge, value: ValueId) -> ValueId {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| (*argument == EntityId::Value(value)).then_some(index))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        let block = match function.entity(EntityId::Value(value)).unwrap().definition {
            Definition::BlockParameter { block, .. } => block,
            Definition::InstructionResult { instruction, .. } => {
                function.instruction(instruction).unwrap().block
            }
        };
        assert_dominates(function, block, edge.target);
        value
    } else {
        let EntityId::Value(next) =
            function.block(edge.target).unwrap().parameters[only(columns.into_iter())]
        else {
            panic!("numeric scalar edge column")
        };
        next
    }
}

// Removing the definition must disconnect entry from the prospective use.
fn assert_dominates(function: &Function, definition: BlockId, target: BlockId) {
    let mut seen = BTreeSet::new();
    let mut pending = vec![function.entry_block().unwrap()];
    while let Some(id) = pending.pop() {
        if id == definition || !seen.insert(id) {
            continue;
        }
        assert_ne!(
            id, target,
            "unchanged scalar identity must dominate its successor"
        );
        pending.extend(term(function.block(id).unwrap()).targets());
    }
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
    let ast = file.ast();
    let outer = only(ast.statements().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Statement::While { .. }).then_some(node.span())
    }));
    let (numeric, mut range) = only(ast.statements().iter().filter_map(|(_, node)| {
        let Statement::For { source, .. } = node.payload() else {
            return None;
        };
        Some((node.span(), *source))
    }));
    while let Expression::Group { expression } = ast.expressions().get(range).unwrap().payload() {
        range = *expression;
    }
    let range = ast.expressions().get(range).unwrap().span();
    let branch = only(ast.expressions().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Expression::If { .. }).then_some(node.span())
    }));
    let continued = only(ast.expressions().iter().filter_map(|(_, node)| {
        matches!(node.payload(), Expression::Continue { .. }).then_some(node.span())
    }));
    let mut reads = ast
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::Member { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    reads.sort_by_key(|span| span.start());
    let [inner, final_read] = reads.as_slice() else {
        panic!("one inner and one post-loop Member")
    };
    Spans {
        outer,
        numeric,
        range,
        branch,
        continued,
        reads: [*inner, *final_read],
    }
}

fn source_origin(origin: &Origin, expected: Span) -> bool {
    // The independent parse has its own SourceMap; it only supplies node coordinates.
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
        panic!("actual numeric conditional")
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
        _ => panic!("this ordinary numeric fixture has no borrowed return"),
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity is unique");
    value
}
