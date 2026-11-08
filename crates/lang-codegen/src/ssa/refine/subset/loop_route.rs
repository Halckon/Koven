//! A retry can qualify sequential iteration with while, or a while header
//! that invalidates a proof, only with a validated local destructuring.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::ScopeKind,
    parser::Statement,
    source::Span,
};

use super::{Checker, RefineInputs, accepts_with_loop_route};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopRoute {
    Existing,
    Candidate,
    Verified,
}

pub(in crate::ssa::refine) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    let (accepted, retry) = accepts_with_loop_route(inputs, LoopRoute::Existing);
    accepted
        || retry
            && has_destructuring_candidate(inputs)
            && accepts_with_loop_route(inputs, LoopRoute::Candidate).0
}

// Span membership only avoids a retry on unrelated files. The retry itself must
// see the descriptor through this callable's normal reachable-body traversal.
fn has_destructuring_candidate(inputs: &RefineInputs<'_>) -> bool {
    let Some(scope) = inputs.scope_span() else {
        return false;
    };
    inputs.typed.destructurings().iter().any(|descriptor| {
        let statement = descriptor.statement();
        descriptor.is_reachable()
            && statement.source_unit() == inputs.source_unit
            && inputs
                .parsed
                .ast()
                .statements()
                .get(statement.statement())
                .is_ok_and(|node| {
                    let span = node.span();
                    span.source_id() == scope.source_id()
                        && scope.start() <= span.start()
                        && span.end() <= scope.end()
                })
    })
}

impl Checker<'_, '_> {
    pub(super) fn check_while(
        &mut self,
        condition: ExpressionId,
        body: StatementId,
        span: Span,
    ) -> bool {
        let invalidated = self.strip_reassigned_loop_views(span, true);
        let saved = self.view_symbols.clone();
        let ok = self.condition(condition, true)
            && (!self.condition_outcomes(condition).0 || self.loop_body(body));
        self.view_symbols = saved;
        if ok && invalidated {
            self.loop_route = LoopRoute::Verified;
        }
        ok
    }

    pub(super) fn scope_has_while_loop(&self) -> bool {
        let Some(owner) = self.inputs.scope_span() else {
            return true;
        };
        self.inputs
            .while_scopes
            .0
            .get_or_init(|| index_while_scopes(self.inputs))
            .as_ref()
            .and_then(|scopes| scopes.get(&(owner.start(), owner.end())))
            .copied()
            .unwrap_or(true)
    }
}

fn index_while_scopes(inputs: &RefineInputs<'_>) -> Option<BTreeMap<(usize, usize), bool>> {
    let names = inputs
        .names
        .names()
        .source_units()
        .get(inputs.source_unit.index())?;
    let mut while_spans: BTreeSet<_> = inputs
        .parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, node)| {
            let span = node.span();
            matches!(node.payload(), Statement::While { .. }).then_some((span.start(), span.end()))
        })
        .collect();
    let scopes = names.resolution().scopes();
    let mut callable_by_scope = Vec::with_capacity(scopes.len());
    let mut has_while = BTreeMap::new();
    for scope in scopes {
        let key = scope.span().map(|span| (span.start(), span.end()));
        let callable = if matches!(scope.kind(), ScopeKind::Function | ScopeKind::Lambda) {
            let key = key?;
            has_while.insert(key, false);
            Some(key)
        } else if let Some(parent) = scope.parent() {
            // Name resolution assigns the parent before entering a child.
            *callable_by_scope.get(parent.index())?
        } else {
            None
        };
        callable_by_scope.push(callable);
        if scope.kind() == ScopeKind::Loop && key.is_some_and(|key| while_spans.remove(&key)) {
            *has_while.get_mut(&callable?)? = true;
        }
    }
    // Preserve the conservative route if the AST and lexical scope facts disagree.
    while_spans.is_empty().then_some(has_while)
}
