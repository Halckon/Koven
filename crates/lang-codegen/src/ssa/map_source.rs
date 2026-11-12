//! Transparent source syntax does not change the validated type, category or owner.
pub(in crate::ssa) use super::{LoweringError, LoweringErrorKind};
pub(in crate::ssa) use super::lower_frontend::nullable_numeric_cleanup::formation;
pub(super) use formation::{
    BindingDeclaration, Birth, Formation, OwnerUpdate, continuation, named_formation,
};
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile},
};

pub(super) mod transport {
    //! Model-only owner transport through witnessed mutations and every actual CFG incoming.
    use super::{LoweringError, LoweringErrorKind};
    use crate::ssa::model::{Definition, EntityId, Function, TerminatorKind};
    use lang_frontend::source::Span;
    use std::collections::{BTreeMap, BTreeSet};

    pub(in super::super) fn check(
        function: &Function,
        current: EntityId,
        anchor: EntityId,
        updates: &BTreeMap<EntityId, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: Some(span),
        };
        let ty = function.entity(anchor).ok_or_else(bad)?.ty;
        let mut pending = vec![current];
        let mut dependencies = BTreeMap::new();
        while let Some(entity) = pending.pop() {
            if entity == anchor || dependencies.contains_key(&entity) {
                continue;
            }
            let data = function.entity(entity).ok_or_else(bad)?;
            if data.ty != ty {
                return Err(bad());
            }
            let parents = if let Some(&input) = updates.get(&entity) {
                vec![input]
            } else {
                let Definition::BlockParameter { block, index } = data.definition else {
                    return Err(bad());
                };
                let parameters = &function.block(block).ok_or_else(bad)?.parameters;
                if parameters.get(index) != Some(&entity) {
                    return Err(bad());
                }
                let mut parents = Vec::new();
                for predecessor in &function.blocks {
                    let edges = match predecessor.terminator.as_ref().map(|t| &t.kind) {
                        Some(TerminatorKind::Branch(edge)) => vec![edge],
                        Some(TerminatorKind::Conditional {
                            when_true,
                            when_false,
                            ..
                        }) => vec![when_true, when_false],
                        Some(TerminatorKind::NullableBranch {
                            when_null,
                            when_non_null,
                            ..
                        }) => vec![when_null, when_non_null],
                        _ => Vec::new(),
                    };
                    for edge in edges.into_iter().filter(|edge| edge.target == block) {
                        if edge.arguments.len() != parameters.len() {
                            return Err(bad());
                        }
                        parents.push(*edge.arguments.get(index).ok_or_else(bad)?);
                    }
                }
                if parents.is_empty() {
                    return Err(bad());
                }
                parents
            };
            if parents
                .iter()
                .any(|parent| function.entity(*parent).map(|e| e.ty) != Some(ty))
            {
                return Err(bad());
            }
            pending.extend(parents.iter().copied());
            dependencies.insert(entity, parents);
        }
        if updates
            .keys()
            .any(|value| !dependencies.contains_key(value))
        {
            return Err(bad());
        }
        let mut anchored = BTreeSet::from([anchor]);
        loop {
            let before = anchored.len();
            for (&entity, parents) in &dependencies {
                if parents.iter().any(|parent| anchored.contains(parent)) {
                    anchored.insert(entity);
                }
            }
            if before == anchored.len() {
                break;
            }
        }
        if dependencies.keys().any(|entity| !anchored.contains(entity)) {
            return Err(bad());
        }
        Ok(())
    }
}

pub(in crate::ssa) mod handoff {
    //! Original Return/local-Move syntax selects a terminal; consumers prove ownership and SSA.
    use super::*;
    use lang_frontend::{
        parser::{Item, NameMarker, VariableKind},
        source::Span,
    };

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) enum Handoff<R> {
        Return {
            expression: ExpressionId,
            operand: ExpressionId,
        },
        Binding {
            root: R,
            initializer: ExpressionId,
            marker: Span,
        },
    }

    pub(in crate::ssa) fn same_callable(parsed: &ParsedFile, left: Span, right: Span) -> bool {
        let enclosing = |at| {
            parsed
                .ast()
                .items()
                .iter()
                .filter(|(_, n)| {
                    matches!(n.payload(), Item::Function { .. } | Item::Deinit { .. })
                        && contains(n.span(), at)
                })
                .min_by_key(|(_, n)| n.span().end() - n.span().start())
                .map(|(id, _)| id)
        };
        enclosing(left).is_some()
            && enclosing(left) == enclosing(right)
            && !parsed.ast().expressions().iter().any(|(_, n)| {
                matches!(n.payload(), Expression::Lambda { .. }) && contains(n.span(), right)
            })
    }

    pub(in crate::ssa) fn find<R: Copy + Eq, T: Copy + Eq>(
        parsed: &ParsedFile,
        span: Span,
        source: R,
        storage: T,
        mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
        mut declaration: impl FnMut(Span) -> Result<R, LoweringError>,
        mut typed: impl FnMut(
            ExpressionId,
        ) -> Option<(T, lang_frontend::type_checking::ExpressionCategory)>,
    ) -> Result<Option<Handoff<R>>, LoweringError> {
        use lang_frontend::type_checking::ExpressionCategory;
        let bad = || LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: Some(span),
        };
        let mut candidates = Vec::new();
        let mut operand = |id| -> Result<bool, LoweringError> {
            let id = transparent_expression(parsed, id, &mut typed)?;
            let node = parsed.ast().expressions().get(id).map_err(|_| bad())?;
            Ok(node.payload() == &Expression::Name
                && reference(node.span())? == source
                && typed(id) == Some((storage, ExpressionCategory::Place)))
        };
        for (expression, node) in parsed.ast().expressions().iter() {
            if node.span().start() <= span.end() || !same_callable(parsed, span, node.span()) {
                continue;
            }
            if let Expression::Return {
                value: Some(value), ..
            } = node.payload()
                && operand(*value)?
            {
                candidates.push(Handoff::Return {
                    expression,
                    operand: *value,
                });
            }
        }
        for (_, node) in parsed.ast().items().iter() {
            if node.span().start() <= span.end() || !same_callable(parsed, span, node.span()) {
                continue;
            }
            if let Item::Variable {
                kind: VariableKind::Val | VariableKind::Var,
                name: NameMarker::Present(marker),
                initializer,
                ..
            } = node.payload()
                && operand(*initializer)?
            {
                candidates.push(Handoff::Binding {
                    root: declaration(*marker)?,
                    initializer: *initializer,
                    marker: *marker,
                });
            }
        }
        match candidates.as_slice() {
            [] => Ok(None),
            [candidate] => Ok(Some(*candidate)),
            _ => Err(LoweringError {
                kind: LoweringErrorKind::UnsupportedNode,
                span: Some(span),
            }),
        }
    }

    fn contains(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }
}

/// Return the canonical expression of a pure Group chain. Consumers keep the
/// original descriptor/loan source ID and independently verify its published root.
pub(in crate::ssa) fn transparent_expression<T: Copy + Eq>(
    parsed: &ParsedFile,
    outer: ExpressionId,
    mut typed: impl FnMut(ExpressionId) -> Option<T>,
) -> Result<ExpressionId, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(outer)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let missing = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(node.span()),
    };
    let expected = typed(outer).ok_or_else(missing)?;
    let mut current = outer;
    let mut seen = Vec::new();
    loop {
        if seen.contains(&current) || typed(current) != Some(expected) {
            return Err(missing());
        }
        seen.push(current);
        let node = parsed
            .ast()
            .expressions()
            .get(current)
            .map_err(|_| missing())?;
        match node.payload() {
            Expression::Group { expression } => current = *expression,
            _ => return Ok(current),
        }
    }
}

/// A published later Drop must contain a real future reference in the same callable.
/// This checks scope/identity, never adjacency or a whitelist of continuation shapes.
pub(in crate::ssa) fn future_reference(
    parsed: &ParsedFile,
    current: lang_frontend::source::Span,
    point: lang_frontend::source::Span,
    references: impl Iterator<Item = lang_frontend::source::Span>,
) -> bool {
    use lang_frontend::parser::{FunctionBody, FunctionForm, Item};
    let contains = |outer: lang_frontend::source::Span, inner: lang_frontend::source::Span| {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && outer.end() >= inner.end()
    };
    let scope = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Function {
                form:
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    },
                ..
            } if contains(node.span(), current) => {
                parsed.ast().statements().get(*body).ok().map(|b| b.span())
            }
            _ => None,
        })
        .min_by_key(|s| s.end() - s.start());
    scope.is_some_and(|scope| contains(scope, point))
        && point.start() >= current.end()
        && references
            .into_iter()
            .any(|reference| contains(point, reference))
        && !parsed.ast().expressions().iter().any(|(_, node)| {
            matches!(node.payload(), Expression::Lambda { .. }) && contains(node.span(), point)
        })
}

/// Preserve the actual FE event: finishing an RHS and finishing its enclosing
/// assignment have the same end offset, but install different binding epochs.
#[derive(Clone, Copy)]
pub(in crate::ssa) struct DropEvent {
    pub span: lang_frontend::source::Span,
    after_expression: Option<ExpressionId>,
}
impl DropEvent {
    fn installs(self, parsed: &ParsedFile, epoch: &formation::BindingEpoch) -> bool {
        self.after_expression.is_some_and(|id| {
            parsed.ast().expressions().get(id).is_ok_and(|node| {
                node.span() == epoch.origin
                    && matches!(node.payload(), Expression::Assignment { value, .. } if *value == epoch.rhs)
            })
        })
    }
}

/// Select the actual epoch first; span equality alone cannot distinguish the
/// replaced owner's RHS cleanup from the new owner's immediate ASAP cleanup.
pub(in crate::ssa) fn in_formation<R>(
    parsed: &ParsedFile,
    formation: &Formation<R>,
    event: DropEvent,
) -> Result<bool, LoweringError> {
    let epoch = epoch_at(parsed, formation, event)?;
    let rhs = parsed
        .ast()
        .expressions()
        .get(epoch.rhs)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: Some(event.span),
        })?;
    Ok(rhs.span() == formation.binding
        && (event.span.end() > formation.binding.end() || event.installs(parsed, epoch)))
}

#[cfg(test)]
pub(super) fn drop_event(
    parsed: &ParsedFile,
    point: lang_frontend::ownership_checking::DropPoint,
) -> Option<DropEvent> {
    use lang_frontend::ownership_checking::DropPoint as P;
    let (span, after_expression) = match point {
        P::AfterExpression(e) => (parsed.ast().expressions().get(e).ok()?.span(), Some(e)),
        P::CallReturn(e) | P::ControlTransfer(e) => {
            (parsed.ast().expressions().get(e).ok()?.span(), None)
        }
        P::AfterStatement(s) | P::LoopExit(s) => {
            (parsed.ast().statements().get(s).ok()?.span(), None)
        }
        _ => return None,
    };
    Some(DropEvent {
        span,
        after_expression,
    })
}

/// Every root Drop belongs to exactly one actual initializer/replacement event.
/// Strict completion ordering keeps cleanup of the replaced value before installing the RHS.
pub(super) fn epoch_at<'a, R>(
    parsed: &ParsedFile,
    formation: &'a Formation<R>,
    event: DropEvent,
) -> Result<&'a formation::BindingEpoch, LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(event.span),
    };
    let mut selected = None;
    let mut end = None;
    for (index, epoch) in formation.history.iter().enumerate() {
        let at = parsed
            .ast()
            .expressions()
            .get(epoch.rhs)
            .map_err(|_| bad())?
            .span();
        if at.source_id() != event.span.source_id() {
            return Err(bad());
        }
        // AfterExpression(assignment) proves installation. AfterExpression(rhs)
        // still owns the old value even though both events have the same end.
        if (at.end() < event.span.end()
            || (at.end() == event.span.end() && (index == 0 || event.installs(parsed, epoch))))
            && end.is_none_or(|end| at.end() > end)
        {
            selected = Some(epoch);
            end = Some(at.end());
        }
    }
    selected.ok_or_else(bad)
}

pub(super) mod handoff_runtime {
    //! Actual provider lease retirement precedes its proved ordinary Return or final owner Drop.
    use super::{LoweringError, LoweringErrorKind, transport};
    use crate::ssa::model::*;
    use lang_frontend::source::Span;
    use std::collections::BTreeMap;

    pub(in super::super) enum Terminal {
        Return(Span),
        Drop {
            marker: Span,
            call: Span,
            callee: FunctionId,
        },
    }

    fn instruction(
        function: &Function,
        entity: EntityId,
        span: Span,
    ) -> Result<&Instruction, LoweringError> {
        let bad = || LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: Some(span),
        };
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function.entity(entity).ok_or_else(bad)?.definition
        else {
            return Err(bad());
        };
        let instruction = function.instruction(instruction).ok_or_else(bad)?;
        if instruction.results.as_slice() != [entity] {
            return Err(bad());
        }
        Ok(instruction)
    }

    pub(in super::super) fn complete(
        function: &Function,
        provider: Span,
        terminal: Terminal,
    ) -> Result<(EntityId, EntityId), LoweringError> {
        let bad = || LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: Some(provider),
        };
        let births = function
            .instructions
            .iter()
            .filter(|i| {
                i.origin == Origin::Source(provider)
                    && matches!(i.operation, Operation::MapIterationFirst { .. })
            })
            .collect::<Vec<_>>();
        let [first] = births.as_slice() else {
            return Err(bad());
        };
        let Operation::MapIterationFirst { source } = first.operation else {
            return Err(bad());
        };
        let lease = instruction(function, EntityId::Loan(source), provider)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = lease.operation
        else {
            return Err(bad());
        };
        let place = instruction(function, EntityId::Place(place), provider)?;
        let Operation::RootPlace { owner: seed } = place.operation else {
            return Err(bad());
        };
        let seed = EntityId::Value(seed);
        let none = BTreeMap::new();
        let ends = function
            .instructions
            .iter()
            .filter(|i| match i.operation {
                Operation::BorrowEnd { loan } => transport::check(
                    function,
                    EntityId::Loan(loan),
                    EntityId::Loan(source),
                    &none,
                    provider,
                )
                .is_ok(),
                _ => false,
            })
            .collect::<Vec<_>>();
        let [lease_end] = ends.as_slice() else {
            return Err(bad());
        };
        let (block, owner, final_instruction) = match terminal {
            Terminal::Return(at) => {
                let blocks = function
                    .blocks
                    .iter()
                    .filter(|b| {
                        b.terminator.as_ref().is_some_and(|t| {
                            t.origin == Origin::Source(at)
                                && matches!(t.kind, TerminatorKind::Return { .. })
                        })
                    })
                    .collect::<Vec<_>>();
                let [block] = blocks.as_slice() else {
                    return Err(bad());
                };
                let Some(TerminatorKind::Return { values }) =
                    block.terminator.as_ref().map(|t| &t.kind)
                else {
                    return Err(bad());
                };
                let [owner] = values.as_slice() else {
                    return Err(bad());
                };
                let ty = function.entity(EntityId::Value(*owner)).ok_or_else(bad)?.ty;
                if !matches!(ty, EntityType::Value(_))
                    || function.return_types.as_slice() != [ty.semantic_type()]
                    || function.borrow_return.is_some()
                    || function.carrier_return.is_some()
                {
                    return Err(bad());
                }
                if function
                    .instructions
                    .iter()
                    .any(|i| i.operation == (Operation::Drop { owner: *owner }))
                {
                    return Err(bad());
                }
                (block.id, *owner, None)
            }
            Terminal::Drop {
                marker,
                call,
                callee,
            } => {
                let drops = function
                    .instructions
                    .iter()
                    .filter(|i| {
                        i.origin == Origin::Source(marker)
                            && matches!(i.operation, Operation::Drop { .. })
                    })
                    .collect::<Vec<_>>();
                let [drop] = drops.as_slice() else {
                    return Err(bad());
                };
                let Operation::Drop { owner } = drop.operation else {
                    return Err(bad());
                };
                let calls = function
                    .instructions
                    .iter()
                    .filter(|i| {
                        i.origin == Origin::Source(call)
                            && matches!(i.operation, Operation::DirectCall { .. })
                    })
                    .collect::<Vec<_>>();
                let [observe] = calls.as_slice() else {
                    return Err(bad());
                };
                let Operation::DirectCall {
                    callee: actual,
                    receiver: None,
                    arguments,
                } = &observe.operation
                else {
                    return Err(bad());
                };
                if *actual != callee {
                    return Err(bad());
                }
                let [EntityId::Loan(loan)] = arguments.as_slice() else {
                    return Err(bad());
                };
                let begin = instruction(function, EntityId::Loan(*loan), provider)?;
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = begin.operation
                else {
                    return Err(bad());
                };
                let root = instruction(function, EntityId::Place(place), provider)?;
                if root.operation != (Operation::RootPlace { owner }) {
                    return Err(bad());
                }
                let ends = function
                    .instructions
                    .iter()
                    .filter(|i| i.operation == (Operation::BorrowEnd { loan: *loan }))
                    .collect::<Vec<_>>();
                let [end] = ends.as_slice() else {
                    return Err(bad());
                };
                let block = function.block(drop.block).ok_or_else(bad)?;
                let position = |id| {
                    block
                        .instructions
                        .iter()
                        .position(|i| *i == id)
                        .ok_or_else(bad)
                };
                if [root, begin, observe, end]
                    .iter()
                    .any(|i| i.block != drop.block)
                    || end.origin != Origin::Source(call)
                    || !(position(root.id)? < position(begin.id)?
                        && position(begin.id)? < position(observe.id)?
                        && position(observe.id)? < position(end.id)?
                        && position(end.id)? < position(drop.id)?)
                {
                    return Err(bad());
                }
                (drop.block, owner, Some(root.id))
            }
        };
        let owner = EntityId::Value(owner);
        transport::check(function, owner, seed, &none, provider)?;
        let block = function.block(block).ok_or_else(bad)?;
        let end = block
            .instructions
            .iter()
            .position(|id| *id == lease_end.id)
            .ok_or_else(bad)?;
        if lease_end.block != block.id
            || final_instruction.is_some_and(|id| {
                block
                    .instructions
                    .iter()
                    .position(|i| *i == id)
                    .is_none_or(|position| end >= position)
            })
        {
            return Err(bad());
        }
        Ok((seed, owner))
    }
}
