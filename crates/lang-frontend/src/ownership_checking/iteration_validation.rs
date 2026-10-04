//! Finite read-only schema validation against the actual typed provider descriptors.
use super::{
    IterationCleanupAction as Action, IterationExitKind, LoanTarget, OwnershipBindingKind,
    OwnershipCheckedFile,
};
use crate::{
    name_resolution::{NameResolution, ReferenceTarget, ScopeKind},
    parser::{Expression, ParsedFile, Statement},
    source::Span,
    type_checking::{ParameterMode, SequentialIterationBinding, TypedFile},
};
use std::{collections::BTreeSet, error::Error, fmt};

/// A published iteration contract is inconsistent with its typed/source authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationFactError {
    span: Option<Span>,
    reason: &'static str,
}
impl IterationFactError {
    /// Source span at the rejected published provider.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
    /// Stable internal schema reason, not a source language diagnostic.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}
impl fmt::Display for IterationFactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason)
    }
}
impl Error for IterationFactError {}

/// Validate a finite published iteration schema without mutating or minting capabilities.
/// Callers perform identity, diagnostic, deferred and constant gates before this check.
pub fn validate_iteration_facts(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), IterationFactError> {
    let mut seen = BTreeSet::new();
    for plan in owned.iterations() {
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| IterationFactError {
                span: None,
                reason: "unknown iteration statement",
            })?;
        let span = node.span();
        let reject = |reason| IterationFactError {
            span: Some(span),
            reason,
        };
        if !seen.insert(statement.index())
            || typed.sequential_iteration(statement) != Some(descriptor)
            || descriptor.delivery() != ParameterMode::Borrow
        {
            return Err(reject("typed provider descriptor mismatch or duplicate"));
        }
        let Statement::For { source, body, .. } = node.payload() else {
            return Err(reject("provider is not a for statement"));
        };
        if *source != descriptor.source() {
            return Err(reject("source evaluation identity mismatch"));
        }
        let mut root = *source;
        loop {
            let node = parsed
                .ast()
                .expressions()
                .get(root)
                .map_err(|_| reject("unknown source expression"))?;
            match node.payload() {
                Expression::Group { expression } => root = *expression,
                _ => break,
            }
        }
        let source_node = parsed
            .ast()
            .expressions()
            .get(root)
            .map_err(|_| reject("unknown source expression"))?;
        match plan.source() {
            LoanTarget::Place(path) if path.is_root() => {
                if !matches!(source_node.payload(), Expression::Name) || !names.references().iter().any(|reference| reference.span() == source_node.span() && matches!(reference.target(), ReferenceTarget::Symbol(symbol) if *symbol == path.root())) {
                    return Err(reject("source owner does not match evaluated source"));
                }
            }
            LoanTarget::Temporary(temporary) => {
                if *temporary != root && *temporary != *source && !typed.element_place(root).is_some_and(|place| place.receiver() == *temporary) {
                    return Err(reject("temporary root bypasses source evaluation"));
                }
            }
            LoanTarget::Place(_) | LoanTarget::This(_) => {}
        }
        let expected = match descriptor.binding() {
            SequentialIterationBinding::Discard => Vec::new(),
            SequentialIterationBinding::Name(symbol) => vec![*symbol],
            SequentialIterationBinding::Destructure(parts) => {
                parts.iter().filter_map(|part| part.symbol()).collect()
            }
        };
        if plan
            .bindings()
            .iter()
            .map(|binding| binding.symbol())
            .collect::<Vec<_>>()
            != expected
            || plan
                .bindings()
                .iter()
                .any(|binding| binding.kind() != OwnershipBindingKind::Shared)
        {
            return Err(reject(
                "element bindings must preserve typed shared delivery",
            ));
        }
        if !plan
            .exits()
            .iter()
            .any(|exit| exit.kind() == IterationExitKind::Exhaustion)
        {
            return Err(reject("missing exhaustion exit"));
        }
        for exit in plan.exits() {
            if owned.cleanup_conditions().get(exit.condition()).is_none() {
                return Err(reject("unknown exit path condition"));
            }
            let element = exit
                .actions()
                .iter()
                .position(|action| *action == Action::EndElement(statement));
            if exit.kind() != IterationExitKind::Exhaustion && element.is_none() {
                return Err(reject("missing element cleanup"));
            }
            if matches!(
                exit.kind(),
                IterationExitKind::Break(_)
                    | IterationExitKind::Return(_)
                    | IterationExitKind::Exhaustion
            ) {
                let provider = exit
                    .actions()
                    .iter()
                    .position(|action| *action == Action::FinishProvider(statement))
                    .ok_or_else(|| reject("missing provider finish"))?;
                let source = exit
                    .actions()
                    .iter()
                    .position(|action| *action == Action::EndSource(statement))
                    .ok_or_else(|| reject("missing source loan end"))?;
                if !(element.is_none_or(|element| element < provider) && provider < source) {
                    return Err(reject("element/provider/source cleanup order"));
                }
            }
        }
        let body_span = parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|_| reject("unknown provider body"))?
            .span();
        for (expression, node) in parsed.ast().expressions().iter() {
            if matches!(node.payload(), Expression::Return { .. })
                && node.span().start() >= body_span.start()
                && node.span().end() <= body_span.end()
                && !names.scopes().iter().any(|scope| {
                    matches!(scope.kind(), ScopeKind::Function | ScopeKind::Lambda)
                        && scope.span().is_some_and(|boundary| {
                            contains(boundary, node.span()) && !contains(boundary, span)
                        })
                })
                && !plan
                    .exits()
                    .iter()
                    .any(|exit| exit.kind() == IterationExitKind::Return(expression))
            {
                return Err(reject("missing Return descriptor"));
            }
        }
    }
    Ok(())
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
