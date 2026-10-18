//! Lexical Map ancestry and exact CFG transport; no closed pair topology.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Nesting {
    pub(super) parent: Option<StatementId>,
}

pub(super) fn nesting(
    parsed: &ParsedFile,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    statement: StatementId,
) -> Result<Nesting, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let inside = |outer: Span| {
        outer != span
            && outer.source_id() == span.source_id()
            && outer.start() <= span.start()
            && span.end() <= outer.end()
    };
    let mut ancestors = Vec::new();
    for (id, candidate) in parsed.ast().statements().iter() {
        match candidate.payload() {
            Statement::For { body, .. } => {
                let body = parsed
                    .ast()
                    .statements()
                    .get(*body)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                if id == statement || !inside(body.span()) {
                    continue;
                }
                if crosses_callable(parsed, candidate.span(), span) {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let descriptor = typed
                    .map_iteration(id)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                let plan = owned
                    .map_iteration(id)
                    .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                if plan.descriptor() != descriptor {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                ancestors.push((body.span().end() - body.span().start(), id));
            }
            Statement::While { .. } | Statement::Loop { .. } if inside(candidate.span()) => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            _ => {}
        }
    }
    ancestors.sort_by_key(|(length, _)| *length);
    Ok(Nesting {
        parent: ancestors.first().map(|(_, id)| *id),
    })
}

/// A lexical containment relation cannot cross a callable's execution boundary.
fn crosses_callable(parsed: &ParsedFile, parent: Span, child: Span) -> bool {
    let contains_child = |boundary: Span| {
        boundary.source_id() == child.source_id()
            && parent.start() <= boundary.start()
            && boundary.start() <= child.start()
            && child.end() <= boundary.end()
            && boundary.end() <= parent.end()
    };
    parsed.ast().items().iter().any(|(_, item)| {
        matches!(item.payload(), Item::Function { .. } | Item::Deinit { .. })
            && contains_child(item.span())
    }) || parsed.ast().expressions().iter().any(|(_, expression)| {
        matches!(expression.payload(), Expression::Lambda { .. })
            && contains_child(expression.span())
    })
}

impl ExpressionLowerer<'_> {
    pub(super) fn validate_map_parent(
        &self,
        nesting: Nesting,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut previous = None;
        for context in &self.loops {
            let state = context
                .map_loop
                .as_ref()
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            if state.nesting.parent != previous
                || state.cleaned
                || state.frame.finished
                || state.body != self.block
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            conditional::transport::roles(self, state, span)?;
            previous = Some(state.frame.statement);
        }
        if previous != nesting.parent {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    /// Only proven active hidden owners may coexist with another provider birth.
    pub(super) fn active_map_temporary(&self, key: usize, value: ValueId) -> bool {
        self.loops.iter().filter_map(|context| context.map_loop.as_ref()).any(|state| {
            !state.cleaned && !state.frame.finished && state.owner.is_some_and(|(_, source, owner)|
                matches!(source, SourceIdentity::Temporary(expression) if expression.index() == key) && owner == value)
        })
    }
}

fn edges(function: &Function, from: BlockId) -> Vec<&Edge> {
    match carrier::terminator(function, from) {
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
    }
}
pub(super) fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            edges(function, block.id)
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge))
        })
        .collect()
}

/// Only exact edge-column transport is accepted. Every predecessor is visited,
/// including backedges; every cycle must have a real path to this immutable anchor.
pub(super) fn transport(
    function: &Function,
    current: EntityId,
    anchor: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let ty = function.entity(anchor).ok_or_else(missing)?.ty;
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::<EntityId, Vec<EntityId>>::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor || dependencies.contains_key(&entity) {
            continue;
        }
        let data = function.entity(entity).ok_or_else(missing)?;
        if data.ty != ty {
            return Err(missing());
        }
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(missing());
        };
        let predecessors = incoming(function, block);
        if predecessors.is_empty() {
            return Err(missing());
        }
        let mut sources = Vec::new();
        for (_, edge) in predecessors {
            let parameters = &function.block(block).ok_or_else(missing)?.parameters;
            if edge.arguments.len() != parameters.len() || parameters.get(index) != Some(&entity) {
                return Err(missing());
            }
            let source = *edge.arguments.get(index).ok_or_else(missing)?;
            if function.entity(source).map(|data| data.ty) != Some(ty) {
                return Err(missing());
            }
            sources.push(source);
            pending.push(source);
        }
        dependencies.insert(entity, sources);
    }
    let mut anchored = std::collections::BTreeSet::from([anchor]);
    loop {
        let previous = anchored.len();
        for (&entity, sources) in &dependencies {
            if sources.iter().any(|source| anchored.contains(source)) {
                anchored.insert(entity);
            }
        }
        if previous == anchored.len() {
            break;
        }
    }
    if dependencies.keys().any(|entity| !anchored.contains(entity)) {
        return Err(missing());
    }
    Ok(())
}
