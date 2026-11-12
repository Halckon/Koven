//! Actual paired owner/view CFG columns, with one active outer-while birth.
use super::*;
use crate::ssa::model::Definition;
use std::collections::BTreeSet;

impl ExpressionLowerer<'_> {
    /// Locate the active outer branch and its real root slot, not a similarly typed loan.
    pub(super) fn numeric_while_birth(&self, root: SymbolId, proof: NonNullProof) -> Option<Pair> {
        let (statement, birth) = proof.while_origin()?;
        let [outer, rest @ ..] = self.loops.as_slice() else {
            return None;
        };
        if rest.len() > 1
            || rest.iter().any(|context| context.numeric_loop.is_none())
            || outer.for_loop.is_some()
            || outer.numeric_loop.is_some()
            || !outer.entry_views.is_empty()
            || !outer.breaks.is_empty()
            || !outer.continues.is_empty()
        {
            return None;
        }
        let term = self.function.block(outer.header)?.terminator.as_ref()?;
        let TerminatorKind::NullableBranch {
            owner,
            view,
            when_non_null,
            when_null,
        } = &term.kind
        else {
            return None;
        };
        let span = self.parsed.ast().statements().get(statement).ok()?.span();
        if *view != birth
            || term.origin != Origin::Source(span)
            || when_non_null.target == when_null.target
            || incoming(self.function, when_non_null.target).as_slice()
                != [(outer.header, when_non_null)]
        {
            return None;
        }
        let (header, column) = parameter(self.function, EntityId::Value(*owner))?;
        let slot = outer.carried.slots.get(column)?;
        if header != outer.header
            || slot.symbol != Some(root)
            || outer
                .carried
                .slots
                .iter()
                .filter(|slot| slot.symbol == Some(root))
                .count()
                != 1
            || when_non_null.arguments.get(column) != Some(&EntityId::Value(*owner))
        {
            return None;
        }
        let EntityId::Value(owner) = *self
            .function
            .block(when_non_null.target)?
            .parameters
            .get(column)?
        else {
            return None;
        };
        let pair = Pair {
            block: when_non_null.target,
            owner,
            loan: birth,
        };
        pair_columns(self.function, pair, proof.inner, self.ssa_types)?;
        Some(pair)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Pair {
    pub(super) block: BlockId,
    pub(super) owner: ValueId,
    pub(super) loan: LoanId,
}

fn parameter(function: &Function, entity: EntityId) -> Option<(BlockId, usize)> {
    let Definition::BlockParameter { block, index } = function.entity(entity)?.definition else {
        return None;
    };
    (function.block(block)?.parameters.get(index) == Some(&entity)).then_some((block, index))
}
fn pair_columns(
    function: &Function,
    pair: Pair,
    inner: SsaTypeId,
    types: &[SsaTypeKind],
) -> Option<(usize, usize)> {
    let (owner_block, owner) = parameter(function, EntityId::Value(pair.owner))?;
    let (loan_block, loan) = parameter(function, EntityId::Loan(pair.loan))?;
    let EntityType::Value(nullable) = function.entity(EntityId::Value(pair.owner))?.ty else {
        return None;
    };
    (owner_block == pair.block
        && loan_block == pair.block
        && owner != loan
        && types.get(nullable.index()) == Some(&SsaTypeKind::NullableHandle { inner })
        && function.entity(EntityId::Loan(pair.loan))?.ty
            == (EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            }))
    .then_some((owner, loan))
}
fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match block.terminator.as_ref().map(|term| &term.kind) {
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
            edges
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

/// Follow paired columns together. Every predecessor is checked, and every
/// visited node must have a path to the unique actual birth. A closed SCC with
/// no entry cannot become evidence merely because its pair was visited before.
pub(super) fn transport(
    function: &Function,
    current: Pair,
    anchor: Pair,
    inner: SsaTypeId,
    types: &[SsaTypeKind],
) -> bool {
    let mut graph = BTreeMap::<Pair, Vec<Pair>>::new();
    let mut work = vec![current];
    while let Some(pair) = work.pop() {
        if graph.contains_key(&pair) {
            continue;
        }
        let Some((owner, loan)) = pair_columns(function, pair, inner, types) else {
            return false;
        };
        if pair == anchor {
            graph.insert(pair, Vec::new());
            continue;
        }
        let predecessors = incoming(function, pair.block);
        if predecessors.is_empty() {
            return false;
        }
        let mut parents = Vec::new();
        for (block, edge) in predecessors {
            if !matches!(
                function
                    .block(block)
                    .and_then(|block| block.terminator.as_ref())
                    .map(|term| &term.kind),
                Some(TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. })
            ) {
                return false;
            }
            let (Some(EntityId::Value(value)), Some(EntityId::Loan(view))) =
                (edge.arguments.get(owner), edge.arguments.get(loan))
            else {
                return false;
            };
            let Some((source_block, _)) = parameter(function, EntityId::Value(*value)) else {
                return false;
            };
            let source = Pair {
                block: source_block,
                owner: *value,
                loan: *view,
            };
            if source_block != block
                || pair_columns(function, source, inner, types).is_none()
                || edge
                    .arguments
                    .iter()
                    .filter(|entity| **entity == EntityId::Value(*value))
                    .count()
                    != 1
                || edge
                    .arguments
                    .iter()
                    .filter(|entity| **entity == EntityId::Loan(*view))
                    .count()
                    != 1
            {
                return false;
            }
            parents.push(source);
            work.push(source);
        }
        graph.insert(pair, parents);
    }
    if !graph.contains_key(&anchor) {
        return false;
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&pair, parents) in &graph {
            if parents.iter().any(|parent| anchored.contains(parent)) {
                anchored.insert(pair);
            }
        }
        if before == anchored.len() {
            return anchored.len() == graph.len();
        }
    }
}

/// The consumer's pair must be the actual numerical exhaustion column. Check
/// the zero-iteration edge and every emitted terminal/increment pair against the
/// complete frontend exit list, including Continue and normal fallthrough.
pub(super) fn numeric_exit(
    lowerer: &ExpressionLowerer<'_>,
    plan: &Plan,
    current: Pair,
    anchor: Pair,
) -> bool {
    let Some(iteration) = lowerer.owned.iteration(plan.statement) else {
        return false;
    };
    let Some(proof) = lowerer.non_null_bindings.get(&plan.root) else {
        return false;
    };
    let Some((owner, loan)) =
        pair_columns(lowerer.function, current, proof.inner, lowerer.ssa_types)
    else {
        return false;
    };
    let Ok(node) = lowerer.parsed.ast().statements().get(plan.statement) else {
        return false;
    };
    let span = node.span();
    if lowerer
        .function
        .block(current.block)
        .map(|block| &block.origin)
        != Some(&Origin::Source(span))
    {
        return false;
    }
    let Some(term) = lowerer
        .function
        .block(anchor.block)
        .and_then(|block| block.terminator.as_ref())
    else {
        return false;
    };
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &term.kind
    else {
        return false;
    };
    if term.origin != Origin::Source(span)
        || when_true.target != current.block
        || when_false.target == current.block
        || when_true.arguments.get(owner) != Some(&EntityId::Value(anchor.owner))
        || when_true.arguments.get(loan) != Some(&EntityId::Loan(anchor.loan))
        || when_false.arguments.get(owner) != Some(&EntityId::Value(anchor.owner))
        || when_false.arguments.get(loan) != Some(&EntityId::Loan(anchor.loan))
    {
        return false;
    }
    let header = when_false.target;
    let edges = incoming(lowerer.function, current.block);
    let mut expected = Vec::new();
    for exit in iteration.exits() {
        match exit.kind() {
            IterationExitKind::Continue(expression) => {
                let Ok(node) = lowerer.parsed.ast().expressions().get(expression) else {
                    return false;
                };
                expected.push(node.span());
            }
            IterationExitKind::Fallthrough => expected.push(span),
            IterationExitKind::Exhaustion => {}
            _ => return false,
        }
    }
    if edges.len() != expected.len() + 1 || !edges.contains(&(anchor.block, when_true)) {
        return false;
    }
    let mut header_sources = vec![(anchor.block, when_false)];
    for (block, edge) in edges {
        if block == anchor.block {
            continue;
        }
        let Some(term) = lowerer
            .function
            .block(block)
            .and_then(|block| block.terminator.as_ref())
        else {
            return false;
        };
        let Origin::Source(origin) = &term.origin else {
            return false;
        };
        let Some(index) = expected.iter().position(|span| *span == *origin) else {
            return false;
        };
        expected.remove(index);
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &term.kind
        else {
            return false;
        };
        if edge != when_true || when_false.target == current.block {
            return false;
        }
        let Some(increment) = lowerer.function.block(when_false.target) else {
            return false;
        };
        let Some(term) = increment.terminator.as_ref() else {
            return false;
        };
        let TerminatorKind::Branch(backedge) = &term.kind else {
            return false;
        };
        if backedge.target != header
            || term.origin != Origin::Source(*origin)
            || incoming(lowerer.function, increment.id).as_slice() != [(block, when_false)]
        {
            return false;
        }
        header_sources.push((increment.id, backedge));
    }
    let incoming_header = incoming(lowerer.function, header);
    expected.is_empty()
        && incoming_header.len() == header_sources.len()
        && incoming_header
            .iter()
            .all(|edge| header_sources.contains(edge))
}

pub(in crate::ssa::lower_frontend::nullable_numeric_cleanup) mod source {
    //! Bounded AST and typed-use qualification, independent of runtime SSA transport.
    use super::super::*;
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        parser::{FunctionBody, FunctionForm, VariableKind},
        type_checking::IntegerRangeOperationKind,
    };

    pub(in crate::ssa::lower_frontend::nullable_numeric_cleanup) fn qualify(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        statement: StatementId,
        member: ExpressionId,
        root: SymbolId,
        seed: SymbolId,
    ) -> Option<StatementId> {
        let storage = typed.symbol_type(root)?;
        let Statement::For { source, body, .. } =
            parsed.ast().statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let leaf = ungroup(parsed, *source)?;
        let range = typed.integer_range_operation(leaf)?;
        let Expression::Binary {
            left,
            right,
            operator,
            ..
        } = expr(parsed, leaf)?
        else {
            return None;
        };
        if range.expression() != leaf
            || range.left() != *left
            || range.right() != *right
            || typed.expression_type(*source) != Some(range.result_type())
            || typed.expression_type(leaf) != Some(range.result_type())
            || typed.expression_type(*left) != Some(range.left_type())
            || typed.expression_type(*right) != Some(range.right_type())
            || !matches!(
                (operator, range.kind()),
                (
                    AstBinaryOperator::InclusiveRange,
                    IntegerRangeOperationKind::Inclusive
                ) | (
                    AstBinaryOperator::ExclusiveRange,
                    IntegerRangeOperationKind::Exclusive
                )
            )
            || !matches!(
                expr(parsed, range.left())?,
                Expression::Literal(LiteralKind::Integer(_))
            )
            || !matches!(
                expr(parsed, range.right())?,
                Expression::Literal(LiteralKind::Integer(_))
            )
        {
            return None;
        }
        let body_span = parsed.ast().statements().get(*body).ok()?.span();
        for (_, node) in parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| contains(body_span, node.span()))
        {
            if !matches!(
                node.payload(),
                Statement::Block { .. }
                    | Statement::ControlBody { .. }
                    | Statement::Expression { .. }
            ) {
                return None;
            }
        }
        for (id, node) in parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| contains(body_span, node.span()))
        {
            match node.payload() {
                Expression::If { .. } | Expression::Continue { .. } => {}
                Expression::Assignment {
                    target,
                    operator: AssignmentOperator::Assign,
                    ..
                } => {
                    let symbol = name(parsed, names, *target)?;
                    if symbol == root || !scalar(typed, typed.symbol_type(symbol)?) {
                        return None;
                    }
                }
                _ => {
                    readonly_expression(parsed, names, typed, id, root, storage)?;
                }
            }
        }
        let Expression::Member {
            receiver,
            safe: false,
            ..
        } = expr(parsed, member)?
        else {
            return None;
        };
        let use_fact = typed.non_null_use(*receiver)?;
        if name(parsed, names, *receiver)? != root
            || use_fact.expression() != *receiver
            || use_fact.symbol() != root
            || use_fact.declared_type() != storage
            || typed.types().get(storage) != Some(&TypeKind::Nullable(use_fact.narrowed_type()))
        {
            return None;
        }
        let mut loops = parsed.ast().statements().iter().filter_map(|(id, node)| {
            let Statement::While {
                condition, body, ..
            } = node.payload()
            else {
                return None;
            };
            let [numeric, tail] = elements(parsed, *body)? else {
                return None;
            };
            let Statement::Expression { expression } =
                parsed.ast().statements().get(*tail).ok()?.payload()
            else {
                return None;
            };
            let Expression::Return {
                value: Some(value), ..
            } = expr(parsed, *expression)?
            else {
                return None;
            };
            let guard = typed.null_comparison(ungroup(parsed, *condition)?)?;
            if *numeric != statement
                || guard.symbol() != root
                || guard.nullable_type() != storage
                || !guard.non_null_when_true()
                || !contains(
                    parsed.ast().expressions().get(*value).ok()?.span(),
                    parsed.ast().expressions().get(member).ok()?.span(),
                )
            {
                return None;
            }
            let tail_span = parsed.ast().expressions().get(*value).ok()?.span();
            for (id, _) in parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| contains(tail_span, node.span()))
            {
                readonly_expression(parsed, names, typed, id, root, storage)?;
            }
            Some(id)
        });
        let outer = loops.next()?;
        if loops.next().is_some() {
            return None;
        }
        // Tie the frontend parameter seed to the real source binding before this outer loop.
        let mut functions = parsed.ast().items().iter().filter_map(|(_, node)| {
            let Item::Function {
                name: NameMarker::Present(function_name),
                form,
                ..
            } = node.payload()
            else {
                return None;
            };
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => return None,
            };
            let statements = elements(parsed, body)?;
            let index = statements.iter().position(|id| *id == outer)?;
            let mut initialized = false;
            for id in &statements[..index] {
                let Statement::LocalVariable { declaration } =
                    parsed.ast().statements().get(*id).ok()?.payload()
                else {
                    return None;
                };
                let Item::Variable {
                    name: NameMarker::Present(span),
                    initializer,
                    kind,
                    ..
                } = parsed.ast().items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                if *span == names.symbols().get(root.index())?.span() {
                    if initialized
                        || *kind != VariableKind::Var
                        || name(parsed, names, *initializer)? != seed
                    {
                        return None;
                    }
                    initialized = true;
                } else if !matches!(expr(parsed, *initializer)?, Expression::Literal(_)) {
                    return None;
                }
            }
            let callable = typed.callables().iter().find(|callable| {
                names
                    .symbols()
                    .get(callable.symbol().index())
                    .is_some_and(|symbol| symbol.span() == *function_name)
            })?;
            let index = callable
                .parameter_symbols()
                .iter()
                .position(|symbol| *symbol == Some(seed))?;
            let parameter = callable.parameters().get(index)?;
            (initialized
                && callable.owner().is_none()
                && callable.receiver().is_none()
                && callable.extension_receiver_symbol().is_none()
                && callable.type_parameters().is_empty()
                && parameter.mode == ParameterMode::Value
                && parameter.ty == storage)
                .then_some(())
        });
        functions.next()?;
        functions.next().is_none().then_some(outer)
    }

    fn readonly_expression(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        id: ExpressionId,
        root: SymbolId,
        storage: TypeId,
    ) -> Option<()> {
        match expr(parsed, id)? {
            Expression::Name => {
                let symbol = name(parsed, names, id)?;
                if symbol == root {
                    let fact = typed.non_null_use(id)?;
                    if fact.expression() != id
                        || fact.symbol() != root
                        || fact.declared_type() != storage
                        || typed.types().get(storage)
                            != Some(&TypeKind::Nullable(fact.narrowed_type()))
                        || typed.expression_type(id) != Some(fact.narrowed_type())
                    {
                        return None;
                    }
                } else if !scalar(typed, typed.expression_type(id)?) {
                    return None;
                }
            }
            Expression::Member {
                receiver,
                safe: false,
                ..
            } => {
                if name(parsed, names, *receiver)? != root
                    || builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int)
                {
                    return None;
                }
            }
            Expression::Literal(_) | Expression::Group { .. } | Expression::Binary { .. } => {
                if !scalar(typed, typed.expression_type(id)?) {
                    return None;
                }
            }
            _ => return None,
        }
        Some(())
    }
    fn scalar(typed: &TypedFile, ty: TypeId) -> bool {
        matches!(
            builtin_type(typed, ty),
            Some(BuiltinType::Int | BuiltinType::Long | BuiltinType::Boolean)
        )
    }
    pub(in crate::ssa::lower_frontend::nullable_numeric_cleanup) fn expr(
        parsed: &ParsedFile,
        id: ExpressionId,
    ) -> Option<&Expression> {
        Some(parsed.ast().expressions().get(id).ok()?.payload())
    }
    fn ungroup(parsed: &ParsedFile, mut id: ExpressionId) -> Option<ExpressionId> {
        for _ in 0..=parsed.ast().expressions().len() {
            match expr(parsed, id)? {
                Expression::Group { expression } => id = *expression,
                _ => return Some(id),
            }
        }
        None
    }
    fn name(parsed: &ParsedFile, names: &NameResolution, id: ExpressionId) -> Option<SymbolId> {
        let node = parsed.ast().expressions().get(id).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        names
            .references()
            .iter()
            .find_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) if reference.span() == node.span() => Some(*symbol),
                _ => None,
            })
    }
    fn elements(parsed: &ParsedFile, id: StatementId) -> Option<&[StatementId]> {
        match parsed.ast().statements().get(id).ok()?.payload() {
            Statement::Block { elements, .. } | Statement::ControlBody { elements, .. } => {
                Some(elements)
            }
            _ => None,
        }
    }
    fn contains(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }
}

#[cfg(test)]
#[path = "../../nullable_numeric_cleanup/tests.rs"]
mod tests;
