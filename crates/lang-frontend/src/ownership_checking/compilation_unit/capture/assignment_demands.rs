//! Identify typed whole-root writes that require a borrowed capture to be exclusive.

use super::{Lambda, ancestors};
use crate::{
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, ScopeId, ScopeKind, SymbolId, SymbolKind,
    },
    parser::{Expression, Item, NameMarker, ParsedFile, VariableKind},
    source::Span,
    type_checking::UnitAssignmentDescriptor,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn collect(
    parsed: &ParsedFile,
    names: &NameResolution,
    lambdas: &[Lambda],
    assignments: &[UnitAssignmentDescriptor],
) -> BTreeMap<(ScopeId, SymbolId), Span> {
    if assignments.is_empty() || lambdas.is_empty() {
        return BTreeMap::new();
    }
    let writable_names: BTreeSet<_> = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                kind: VariableKind::Var,
                name: NameMarker::Present(span),
                ..
            } => Some(span_key(*span)),
            _ => None,
        })
        .collect();
    let references: BTreeMap<_, _> = names
        .references()
        .iter()
        .filter(|reference| reference.namespace() == Namespace::Value)
        .map(|reference| (span_key(reference.span()), reference))
        .collect();
    let borrowed_lambdas: BTreeSet<_> = lambdas
        .iter()
        .filter(|lambda| !lambda.move_owned)
        .map(|lambda| lambda.scope)
        .collect();
    let mut demands = BTreeMap::new();
    for assignment in assignments {
        let mut target = assignment.target().expression();
        let span = loop {
            let Ok(node) = parsed.ast().expressions().get(target) else {
                break None;
            };
            match node.payload() {
                Expression::Group { expression } => target = *expression,
                Expression::Name => break Some(node.span()),
                _ => break None,
            }
        };
        let Some(span) = span else {
            continue;
        };
        let Some(reference) = references.get(&span_key(span)) else {
            continue;
        };
        let ReferenceTarget::Symbol(symbol) = reference.target() else {
            continue;
        };
        let Some(declaration) = names.symbols().get(symbol.index()) else {
            continue;
        };
        if declaration.kind() != SymbolKind::Variable
            || !writable_names.contains(&span_key(declaration.span()))
        {
            continue;
        }
        let Some(lambda) = enclosing_callable(names, reference.scope()) else {
            continue;
        };
        if !borrowed_lambdas.contains(&lambda) {
            continue;
        }
        let parent = names.scopes()[lambda.index()]
            .parent()
            .and_then(|scope| enclosing_callable(names, scope));
        if parent.is_some() && parent == enclosing_callable(names, declaration.scope()) {
            demands
                .entry((lambda, *symbol))
                .and_modify(|previous: &mut Span| {
                    if span.start() < previous.start() {
                        *previous = span;
                    }
                })
                .or_insert(span);
        }
    }
    demands
}

fn enclosing_callable(names: &NameResolution, scope: ScopeId) -> Option<ScopeId> {
    ancestors(scope, names).find(|scope| {
        matches!(
            names.scopes().get(scope.index()).map(|scope| scope.kind()),
            Some(ScopeKind::Function | ScopeKind::Lambda)
        )
    })
}

fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}
