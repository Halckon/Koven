//! Source identity, real CFG transport, and bounded exact-path assertions.
use super::*;
use crate::ssa::model::Definition;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Prepared {
    pub(super) fixture: Fixture,
    pub(super) source: UnitSource,
    pub(super) program: Program,
    pub(super) entry: FunctionId,
}

pub(super) fn prepared() -> Vec<Prepared> {
    let mut ready = Vec::new();
    let mut unsupported = Vec::new();
    for fixture in fixtures() {
        let source = UnitSource::analyze(fixture.name, fixture.text).unwrap_or_else(|error| {
            panic!(
                "{}: unexpected frontend rejection: {error:?}; later rows not reached",
                fixture.name
            )
        });
        source.assert_frontend_clean();
        assert!(matches!(&source.products, OwnedProducts::Basic(..)));
        assert_eq!(
            source.sources.source_text(source.parsed.source_id()),
            Ok(fixture.text)
        );
        assert_eq!(
            source.sources.source_name(source.parsed.source_id()),
            Ok(fixture.name)
        );
        let result = source.lower_entry(source.declaration("entry", SymbolKind::Function));
        match result {
            Ok((program, entry)) => {
                verify_program(&program).expect("real Unit ownership and SSA verify");
                ready.push(Prepared {
                    fixture,
                    source,
                    program,
                    entry,
                });
            }
            Err(error) => {
                // Only the known old-production When capability boundary is collectable.
                // Unexpected kinds, spans, frontend failures and panics stop immediately.
                assert_eq!(
                    error.kind,
                    LoweringErrorKind::UnsupportedNode,
                    "{}: {error:?}",
                    fixture.name
                );
                let span = error.span.expect("the old boundary has a real source span");
                assert_eq!(span.source_id(), source.parsed.source_id());
                assert_eq!(
                    (span.start(), span.end()),
                    fixture.first_when,
                    "{}: not the fixed old first rejection",
                    fixture.name
                );
                let expected = "when (subject) {\n        null -> { println(\"null\") };\n        else -> { observe(subject); observe((subject)) }\n    }";
                assert_eq!(source.sources.slice(span), Ok(expected));
                let first = controls(&source, "repeated");
                assert_eq!(first.len(), 3);
                assert_eq!(
                    first[0].span, span,
                    "source-order repeated entry, not another same-kind When"
                );
                unsupported.push((
                    fixture.name,
                    span.start(),
                    span.end(),
                    source.sources.slice(span).unwrap().to_owned(),
                ));
            }
        }
    }
    assert!(
        unsupported.is_empty(),
        "complete-source lower preflight: {} ready; {} expected old When rejections; SSA/LLVM/native rows for rejected documents NOT reached: {unsupported:#?}",
        ready.len(),
        unsupported.len()
    );
    assert_eq!(ready.len(), 3);
    ready
}

pub(super) fn item_span(source: &UnitSource, name: &str) -> Span {
    let declaration = source.declaration(name, SymbolKind::Function);
    let entry = &source.names.names().index().declarations()[declaration.index()];
    source
        .parsed
        .ast()
        .items()
        .get(entry.root())
        .unwrap()
        .span()
}

pub(super) fn expression(
    source: &UnitSource,
    within: Span,
    text: &str,
) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one original document")
    };
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| {
            contains(within, node.span()) && source.sources.slice(node.span()) == Ok(text)
        })
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one real expression {text:?} in {within:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

pub(super) fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

pub(super) fn span(source: &UnitSource, expression: UnitExpressionId) -> Span {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one original document")
    };
    assert_eq!(expression.source_unit(), unit.id());
    source
        .parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .unwrap()
        .span()
}

pub(super) struct Control {
    pub(super) id: UnitExpressionId,
    pub(super) span: Span,
    pub(super) subject: UnitExpressionId,
    pub(super) subject_span: Span,
    pub(super) entries: Vec<(UnitStatementId, Span)>,
}

pub(super) fn controls(source: &UnitSource, name: &str) -> Vec<Control> {
    let scope = item_span(source, name);
    let [unit] = source.names.names().index().source_units() else {
        panic!("one document")
    };
    let OwnedProducts::Basic(typed, _) = &source.products else {
        panic!("Basic source")
    };
    let mut found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::When {
                subject: Some(subject),
                entries,
                ..
            } = node.payload()
            else {
                return None;
            };
            let subject = UnitExpressionId::new(unit.id(), *subject);
            let ty = typed.types().expression_type(subject).unwrap();
            if !contains(scope, node.span())
                || !matches!(
                    typed.types().types().get(ty),
                    Some(UnitTypeKind::Nullable(_))
                )
            {
                return None;
            }
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].conditions.len(), 1);
            assert!(entries[0].else_span.is_none() && entries[1].else_span.is_some());
            assert!(entries[1].conditions.is_empty());
            Some(Control {
                id: UnitExpressionId::new(unit.id(), id),
                span: node.span(),
                subject,
                subject_span: span(source, subject),
                entries: entries
                    .iter()
                    .map(|entry| (UnitStatementId::new(unit.id(), entry.body), entry.span))
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|control| control.span.start());
    found
}

pub(super) fn root(source: &UnitSource, reference: UnitExpressionId) -> UnitSymbolId {
    let node = source
        .parsed
        .ast()
        .expressions()
        .get(reference.expression())
        .unwrap();
    if let Expression::Group { expression } = node.payload() {
        return root(
            source,
            UnitExpressionId::new(reference.source_unit(), *expression),
        );
    }
    assert!(matches!(node.payload(), Expression::Name));
    let refs = source
        .names
        .names()
        .references()
        .iter()
        .filter(|fact| fact.source_unit() == reference.source_unit() && fact.span() == node.span())
        .collect::<Vec<_>>();
    let [fact] = refs.as_slice() else {
        panic!("one source-qualified real reference")
    };
    let UnitReferenceTarget::Symbol(symbol) = fact.target() else {
        panic!("actual local/parameter root")
    };
    *symbol
}

pub(super) fn function<'a>(source: &UnitSource, program: &'a Program, name: &str) -> &'a Function {
    let origin = Origin::Source(item_span(source, name));
    let found = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == origin)
        .collect::<Vec<_>>();
    let [function] = found.as_slice() else {
        panic!("one source-qualified SSA function {name}")
    };
    function
}

pub(super) fn instruction(
    function: &Function,
    origin: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> &Instruction {
    let found = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(origin) && predicate(&instruction.operation)
        })
        .collect::<Vec<_>>();
    let [instruction] = found.as_slice() else {
        panic!("one real SSA operation at {origin:?}")
    };
    instruction
}

pub(super) fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
    match kind {
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
        TerminatorKind::Return { .. } => vec![],
        _ => panic!("these ordinary fixtures do not contain Abort or borrowed/carrier returns"),
    }
}

pub(super) fn aliases(function: &Function, seed: EntityId) -> BTreeSet<EntityId> {
    let entry = function.entry_block().unwrap();
    let mut roots: BTreeMap<EntityId, BTreeSet<EntityId>> = BTreeMap::new();
    let mut incoming: BTreeMap<EntityId, BTreeSet<EntityId>> = BTreeMap::new();
    let mut predecessor_count: BTreeMap<BlockId, usize> = BTreeMap::new();
    let mut births = BTreeMap::new();
    for block in &function.blocks {
        for (column, parameter) in block.parameters.iter().enumerate() {
            assert_eq!(
                function.entity(*parameter).unwrap().definition,
                Definition::BlockParameter {
                    block: block.id,
                    index: column
                }
            );
            assert!(roots.insert(*parameter, BTreeSet::new()).is_none());
            assert!(incoming.insert(*parameter, BTreeSet::new()).is_none());
            if block.id == entry {
                roots.get_mut(parameter).unwrap().insert(*parameter);
            }
        }
    }
    for instruction in &function.instructions {
        for (index, result) in instruction.results.iter().enumerate() {
            assert_eq!(
                function.entity(*result).unwrap().definition,
                Definition::InstructionResult {
                    instruction: instruction.id,
                    index
                }
            );
            assert!(roots.insert(*result, BTreeSet::from([*result])).is_none());
        }
    }
    for block in &function.blocks {
        let kind = &block.terminator.as_ref().unwrap().kind;
        for edge in edges(kind) {
            let target = function.block(edge.target).unwrap();
            *predecessor_count.entry(target.id).or_default() += 1;
            let birth = match kind {
                TerminatorKind::NullableBranch {
                    when_non_null,
                    view,
                    ..
                } if std::ptr::eq(edge, when_non_null) => Some(EntityId::Loan(*view)),
                _ => None,
            };
            assert_eq!(
                edge.arguments.len() + usize::from(birth.is_some()),
                target.parameters.len()
            );
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                assert!(
                    roots.contains_key(argument),
                    "same-function real entity definition"
                );
                incoming.get_mut(parameter).unwrap().insert(*argument);
            }
            if let Some(view) = birth {
                assert_eq!(target.parameters.last(), Some(&view));
                assert!(
                    births.insert(view, target.id).is_none(),
                    "one actual proof birth"
                );
                roots.get_mut(&view).unwrap().insert(view);
            }
        }
    }
    assert!(!predecessor_count.contains_key(&entry));
    for (view, target) in births {
        assert_eq!(
            predecessor_count[&target], 1,
            "dedicated non-null proof target"
        );
        assert!(
            incoming[&view].is_empty(),
            "a proof birth is not an ordinary incoming argument"
        );
    }
    // Propagate origin sets, not entity unions. A loop is grounded only by its
    // real entry/instruction/proof birth; a rootless cycle remains empty.
    loop {
        let mut changed = false;
        for (parameter, predecessors) in &incoming {
            let origins = predecessors
                .iter()
                .flat_map(|value| roots[value].iter().copied())
                .collect::<BTreeSet<_>>();
            let known = roots.get_mut(parameter).unwrap();
            let before = known.len();
            known.extend(origins);
            changed |= known.len() != before;
        }
        if !changed {
            break;
        }
    }
    let seed_roots = &roots[&seed];
    assert_eq!(
        seed_roots.len(), 1,
        "one grounded real origin for seed {seed:?}: {seed_roots:?}"
    );
    let root = *seed_roots.first().unwrap();
    let found = roots
        .iter()
        .filter_map(|(value, origins)| origins.contains(&root).then_some(*value))
        .collect::<BTreeSet<_>>();
    for value in &found {
        assert_eq!(
            &roots[value], seed_roots,
            "mixed real origins cannot become an alias: {value:?}"
        );
        if *value == root {
            continue;
        }
        let predecessors = incoming
            .get(value)
            .expect("a non-root alias is real block transport");
        assert!(
            !predecessors.is_empty(),
            "transport has actual incoming edges"
        );
        assert!(
            predecessors.iter().all(|argument| found.contains(argument) && &roots[argument] == seed_roots),
            "every real incoming edge must have the same grounded origin; function={:?}; seed={seed:?}; root={root:?}; value={value:?}; predecessors={predecessors:?}; roots={roots:#?}",
            function.id
        );
    }
    assert!(found.contains(&seed));
    found
}

pub(super) struct Birth<'a> {
    pub(super) header: BlockId,
    pub(super) owner: ValueId,
    pub(super) null: &'a Edge,
    pub(super) present: &'a Edge,
    pub(super) view: LoanId,
    pub(super) owners: BTreeSet<EntityId>,
    pub(super) views: BTreeSet<EntityId>,
}

pub(super) fn birth<'a>(function: &'a Function, control: &Control) -> Birth<'a> {
    let blocks = function
        .blocks
        .iter()
        .filter(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                terminator.origin == Origin::Source(control.span)
                    && matches!(terminator.kind, TerminatorKind::NullableBranch { .. })
            })
        })
        .collect::<Vec<_>>();
    let [block] = blocks.as_slice() else {
        panic!("one actual NullableBranch for this source When")
    };
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &block.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    assert_ne!(when_null.target, when_non_null.target);
    let columns = when_null
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| (*argument == EntityId::Value(*owner)).then_some(index))
        .collect::<Vec<_>>();
    let [column] = columns.as_slice() else {
        panic!("one actual owner column")
    };
    assert_eq!(when_non_null.arguments[*column], EntityId::Value(*owner));
    assert_eq!(when_null.arguments, when_non_null.arguments);
    assert_eq!(
        function
            .block(when_non_null.target)
            .unwrap()
            .parameters
            .last(),
        Some(&EntityId::Loan(*view))
    );
    let incoming = function
        .blocks
        .iter()
        .filter(|candidate| {
            candidate
                .terminator
                .as_ref()
                .unwrap()
                .kind
                .targets()
                .contains(&when_non_null.target)
        })
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    assert_eq!(
        incoming,
        [block.id],
        "only this non-null edge births the proof"
    );
    Birth {
        header: block.id,
        owner: *owner,
        null: when_null,
        present: when_non_null,
        view: *view,
        owners: aliases(function, EntityId::Value(*owner)),
        views: aliases(function, EntityId::Loan(*view)),
    }
}

pub(super) fn branch_owner(function: &Function, edge: &Edge, owner: ValueId) -> ValueId {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(column, argument)| (*argument == EntityId::Value(owner)).then_some(column))
        .collect::<Vec<_>>();
    let [column] = columns.as_slice() else {
        panic!("one owner transport column")
    };
    let EntityId::Value(owner) = function.block(edge.target).unwrap().parameters[*column] else {
        panic!("owned parameter")
    };
    owner
}

pub(super) fn position(function: &Function, instruction: &Instruction) -> usize {
    function
        .block(instruction.block)
        .unwrap()
        .instructions
        .iter()
        .position(|id| *id == instruction.id)
        .unwrap()
}

pub(super) fn assert_make_once(
    source: &UnitSource,
    program: &Program,
    name: &str,
    origin: Span,
    owner: ValueId,
) {
    let function = function(source, program, name);
    let make = instruction(function, origin, |op| {
        matches!(op, Operation::DirectCall { .. })
    });
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &make.operation
    else {
        unreachable!()
    };
    assert!(receiver.is_none());
    assert_eq!(arguments.len(), 1);
    assert_eq!(*callee, self::function(source, program, "make").id);
    assert_eq!(make.results.len(), 1);
    assert!(aliases(function, make.results[0]).contains(&EntityId::Value(owner)));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    ViewEnd,
    SubjectDrop,
    Body,
    InnerCall,
    PrefixEnd,
    PrefixDrop,
    OuterCall,
    OuterEnd,
    OuterDrop,
    Observe,
    Take,
    InnerDrop,
    TakeCommit,
}

pub(super) struct Track {
    owners: BTreeSet<EntityId>,
    loans: BTreeSet<EntityId>,
}

pub(super) fn temporary_track(
    source: &UnitSource,
    function: &Function,
    scope: Span,
    constructor: &str,
) -> Track {
    let origin = expression(source, scope, constructor).1;
    let allocation = instruction(function, origin, |op| {
        matches!(op, Operation::HeapAllocate { .. })
    });
    let [owner @ EntityId::Value(_)] = allocation.results.as_slice() else {
        panic!("one actual temporary allocation")
    };
    let owners = aliases(function, *owner);
    let loans = function
        .instructions
        .iter()
        .filter(|instruction| {
            let Operation::BorrowBegin { place, .. } = instruction.operation else {
                return false;
            };
            function.instructions.iter().any(|root| {
                root.results.contains(&EntityId::Place(place))
                    && matches!(root.operation,
            Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)))
            })
        })
        .collect::<Vec<_>>();
    let [loan] = loans.as_slice() else {
        panic!("one actual pending loan of this exact temporary")
    };
    let [loan @ EntityId::Loan(_)] = loan.results.as_slice() else {
        panic!("one actual loan result")
    };
    Track {
        owners,
        loans: aliases(function, *loan),
    }
}

pub(super) enum Stop {
    Checkpoint(Span),
    Return(Span),
    Break {
        statement: Span,
        target: BlockId,
        expression: Span,
    },
    Continue {
        expression: Span,
        header: BlockId,
    },
}

pub(super) fn loop_stop(
    source: &UnitSource,
    function: &Function,
    scope: Span,
    exit: Span,
    breaking: bool,
) -> Stop {
    let loops = source
        .parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| {
            contains(scope, node.span())
                && contains(node.span(), exit)
                && matches!(node.payload(), Statement::While { .. })
        })
        .collect::<Vec<_>>();
    let [(_, node)] = loops.as_slice() else {
        panic!("one actual enclosing bounded while")
    };
    let statement = node.span();
    let headers = function
        .blocks
        .iter()
        .filter(|block| {
            block.terminator.as_ref().is_some_and(|term| {
                term.origin == Origin::Source(statement)
                    && matches!(term.kind, TerminatorKind::Conditional { .. })
            })
        })
        .collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        panic!("one actual while header")
    };
    if !breaking {
        return Stop::Continue {
            expression: exit,
            header: header.id,
        };
    }
    let TerminatorKind::Conditional { when_false, .. } = &header.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    let block = function.block(when_false.target).unwrap();
    assert_eq!(block.origin, Origin::Source(statement));
    let term = block.terminator.as_ref().unwrap();
    assert_eq!(term.origin, Origin::Source(statement));
    let TerminatorKind::Branch(edge) = &term.kind else {
        panic!("while false joins the exact break destination")
    };
    assert_eq!(
        function.block(edge.target).unwrap().origin,
        Origin::Source(statement)
    );
    Stop::Break {
        statement,
        target: edge.target,
        expression: exit,
    }
}

pub(super) struct Path<'a> {
    pub(super) function: &'a Function,
    pub(super) birth: &'a Birth<'a>,
    pub(super) prefix: Option<Track>,
    pub(super) outer: Option<Track>,
    pub(super) body: Option<Span>,
    pub(super) inner_call: Option<Span>,
    pub(super) outer_call: Option<Span>,
    pub(super) observe: Option<Span>,
    pub(super) take: Option<Span>,
    pub(super) take_call: Option<Span>,
    pub(super) inner: BTreeSet<EntityId>,
}

impl Path<'_> {
    pub(super) fn check(&self, start: BlockId, stop: &Stop, expected: &[Event]) {
        let count = self.visit(start, stop, expected, Vec::new(), Vec::new());
        assert_eq!(
            count, 1,
            "this source-selected edge has one deterministic bounded path to its exact exit"
        );
    }

    fn visit(
        &self,
        id: BlockId,
        stop: &Stop,
        expected: &[Event],
        mut events: Vec<Event>,
        mut seen: Vec<BlockId>,
    ) -> usize {
        assert!(
            !seen.contains(&id),
            "proof/transaction must retire before a loop backedge: {seen:?}"
        );
        seen.push(id);
        let block = self.function.block(id).unwrap();
        for id in &block.instructions {
            let instruction = self.function.instruction(*id).unwrap();
            let origin = instruction.origin.span();
            if matches!(stop, Stop::Checkpoint(span) if *span == origin)
                && matches!(
                    instruction.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
            {
                assert_eq!(events, expected, "exact source path {seen:?}");
                return 1;
            }
            if self.body == Some(origin)
                && matches!(
                    instruction.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
            {
                events.push(Event::Body);
            }
            match &instruction.operation {
                Operation::BorrowEnd { loan }
                    if self.birth.views.contains(&EntityId::Loan(*loan)) =>
                {
                    events.push(Event::ViewEnd)
                }
                Operation::Drop { owner }
                    if self.birth.owners.contains(&EntityId::Value(*owner)) =>
                {
                    events.push(Event::SubjectDrop)
                }
                Operation::BorrowEnd { loan }
                    if self
                        .prefix
                        .as_ref()
                        .is_some_and(|track| track.loans.contains(&EntityId::Loan(*loan))) =>
                {
                    events.push(Event::PrefixEnd)
                }
                Operation::Drop { owner }
                    if self
                        .prefix
                        .as_ref()
                        .is_some_and(|track| track.owners.contains(&EntityId::Value(*owner))) =>
                {
                    events.push(Event::PrefixDrop)
                }
                Operation::BorrowEnd { loan }
                    if self
                        .outer
                        .as_ref()
                        .is_some_and(|track| track.loans.contains(&EntityId::Loan(*loan))) =>
                {
                    events.push(Event::OuterEnd)
                }
                Operation::Drop { owner }
                    if self
                        .outer
                        .as_ref()
                        .is_some_and(|track| track.owners.contains(&EntityId::Value(*owner))) =>
                {
                    events.push(Event::OuterDrop)
                }
                Operation::Drop { owner } if self.inner.contains(&EntityId::Value(*owner)) => {
                    events.push(Event::InnerDrop)
                }
                Operation::NullableTake { owner, proof } if self.take == Some(origin) => {
                    assert!(
                        self.birth.owners.contains(&EntityId::Value(*owner))
                            && self.birth.views.contains(&EntityId::Loan(*proof))
                    );
                    events.push(Event::Take);
                }
                Operation::DirectCall {
                    receiver,
                    arguments,
                    ..
                } if self.inner_call == Some(origin) => {
                    assert!(receiver.is_none());
                    assert!(self.prefix.as_ref().unwrap().loans.contains(&arguments[0]));
                    events.push(Event::InnerCall);
                }
                Operation::DirectCall {
                    receiver,
                    arguments,
                    ..
                } if self.outer_call == Some(origin) => {
                    assert!(receiver.is_none());
                    assert!(self.outer.as_ref().unwrap().loans.contains(&arguments[0]));
                    events.push(Event::OuterCall);
                }
                Operation::DirectCall { arguments, .. } if self.observe == Some(origin) => {
                    assert!(self.birth.views.contains(&arguments[0]));
                    events.push(Event::Observe);
                }
                Operation::DirectCall { arguments, .. } if self.take_call == Some(origin) => {
                    assert!(self.inner.contains(&arguments[0]));
                    events.push(Event::TakeCommit);
                }
                Operation::NullableTake { owner, proof }
                    if self.birth.owners.contains(&EntityId::Value(*owner))
                        || self.birth.views.contains(&EntityId::Loan(*proof)) =>
                {
                    panic!("unexpected extraction on this exact source path")
                }
                Operation::SharedRetain { owner }
                    if self.birth.owners.contains(owner) || self.birth.views.contains(owner) =>
                {
                    panic!("When must not implicitly retain")
                }
                _ => {}
            }
        }
        let term = block.terminator.as_ref().unwrap();
        let matched = match stop {
            Stop::Return(span) => {
                term.origin == Origin::Source(*span)
                    && matches!(term.kind, TerminatorKind::Return { .. })
            }
            Stop::Continue { expression, header } => {
                term.origin == Origin::Source(*expression)
                    && matches!(&term.kind, TerminatorKind::Branch(edge) if edge.target == *header)
            }
            Stop::Break {
                statement,
                target,
                expression,
            } => {
                let boundary = term.origin == Origin::Source(*statement)
                    && matches!(&term.kind, TerminatorKind::Branch(edge) if edge.target == *target);
                if boundary && self.outer.is_some() {
                    assert!(
                        block
                            .instructions
                            .iter()
                            .map(|id| self.function.instruction(*id).unwrap())
                            .any(|instruction| {
                                instruction.origin == Origin::Source(*expression)
                                    && matches!(instruction.operation, Operation::BorrowEnd { loan }
                            if self.outer.as_ref().unwrap().loans.contains(&EntityId::Loan(loan)))
                            }),
                        "break is dual-locked to real AST while destination and exact transfer cleanup origin"
                    );
                }
                boundary
            }
            Stop::Checkpoint(_) => false,
        };
        if matched {
            if let TerminatorKind::Return { values } = &term.kind {
                assert!(
                    values.is_empty(),
                    "the exact source transfer is a Unit return"
                );
            }
            assert_eq!(events, expected, "exact transfer path {seen:?}");
            for edge in edges(&term.kind) {
                assert!(
                    edge.arguments
                        .iter()
                        .all(|entity| !self.birth.views.contains(entity)
                            && !self.birth.owners.contains(entity)
                            && !self.inner.contains(entity))
                );
                for track in [&self.prefix, &self.outer].into_iter().flatten() {
                    assert!(
                        edge.arguments
                            .iter()
                            .all(|entity| !track.owners.contains(entity)
                                && !track.loans.contains(entity))
                    );
                }
            }
            return 1;
        }
        let next = edges(&term.kind);
        assert!(
            !next.is_empty(),
            "unmatched terminator, never a generic Return fallback: {term:?}; expected {expected:?}; events {events:?}"
        );
        next.into_iter()
            .map(|edge| self.visit(edge.target, stop, expected, events.clone(), seen.clone()))
            .sum()
    }
}
