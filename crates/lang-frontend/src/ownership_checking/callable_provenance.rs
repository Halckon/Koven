//! Read-only callable source facts share the existing sealed ownership analysis identity.

pub(super) mod graph;

use super::{CompilationUnitOwnership, OwnershipCheckedFile};
use crate::{
    ast::ExpressionId,
    name_resolution::{SymbolId, UnitSymbolId},
    source::Span,
    type_checking::{TypeId, UnitCallableTarget, UnitExpressionId, UnitTypeId},
};

/// A proved source identity; this fact does not transfer ownership or prove a loan alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableOrigin {
    /// The concrete lambda AST origin, independent of its runtime capture values.
    Lambda(ExpressionId),
    /// A real Function parameter in this callable's entry state.
    Parameter(SymbolId),
    /// A uniquely selected, non-deferred ordinary source function.
    KnownFunction(SymbolId),
    /// A committed source call whose completed callee has a pointer return summary.
    FactoryResult(ExpressionId),
}

/// Source-qualified counterpart of a single-file callable source identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCallableOrigin {
    /// The concrete lambda AST origin qualified by source unit.
    Lambda(UnitExpressionId),
    /// A real Function parameter in this callable's entry state.
    Parameter(UnitSymbolId),
    /// A uniquely selected ordinary top-level source declaration.
    KnownFunction(UnitCallableTarget),
    /// A committed source call whose completed callee has a pointer return summary.
    FactoryResult(UnitExpressionId),
}

/// An empty-environment return with stable source identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerCallableReturnOrigin {
    /// The normally returned lambda has no captures.
    Lambda(ExpressionId),
    /// The normally returned callable is a selected ordinary source function.
    KnownFunction(SymbolId),
}

/// An empty-environment return with stable, source-qualified identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitPointerCallableReturnOrigin {
    /// The normally returned lambda has no captures.
    Lambda(UnitExpressionId),
    /// The normally returned callable is a selected ordinary source declaration.
    KnownFunction(UnitCallableTarget),
}

macro_rules! origin_fact {
    ($fact:ident, $expression:ty, $origin:ty) => {
        /// Callable provenance for one normally completed expression.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $fact {
            expression: $expression,
            origin: $origin,
            span: Span,
        }
        impl $fact {
            pub(super) const fn new(expression: $expression, origin: $origin, span: Span) -> Self {
                Self {
                    expression,
                    origin,
                    span,
                }
            }
            /// The expression whose runtime callable has this source.
            #[must_use]
            pub const fn expression(self) -> $expression {
                self.expression
            }
            /// The proved source; environment runtime values are not part of identity.
            #[must_use]
            pub const fn origin(self) -> $origin {
                self.origin
            }
            /// The actual use expression location.
            #[must_use]
            pub const fn span(self) -> Span {
                self.span
            }
        }
    };
}
origin_fact!(CallableOriginFact, ExpressionId, CallableOrigin);
origin_fact!(UnitCallableOriginFact, UnitExpressionId, UnitCallableOrigin);

macro_rules! return_summary {
    ($summary:ident, $target:ty, $expression:ty, $origin:ty, $ty:ty) => {
        /// One normal pointer return from a completed source function body.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $summary {
            target: $target,
            return_value: $expression,
            origin: $origin,
            function_type: $ty,
            span: Span,
        }
        impl $summary {
            pub(super) const fn new(
                target: $target,
                return_value: $expression,
                origin: $origin,
                function_type: $ty,
                span: Span,
            ) -> Self {
                Self {
                    target,
                    return_value,
                    origin,
                    function_type,
                    span,
                }
            }
            /// The factory source function, rather than its returned callable.
            #[must_use]
            pub const fn target(self) -> $target {
                self.target
            }
            /// The actual normally delivered return operand, preserving Group nodes.
            #[must_use]
            pub const fn return_value(self) -> $expression {
                self.return_value
            }
            /// The returned callable source after transparent grouping.
            #[must_use]
            pub const fn origin(self) -> $origin {
                self.origin
            }
            /// Declaration return identity; generic callsites use their instantiated call facts.
            #[must_use]
            pub const fn function_type(self) -> $ty {
                self.function_type
            }
            /// The normal return operand location.
            #[must_use]
            pub const fn span(self) -> Span {
                self.span
            }
        }
    };
}
return_summary!(
    PointerCallableReturnSummary,
    SymbolId,
    ExpressionId,
    PointerCallableReturnOrigin,
    TypeId
);
return_summary!(
    UnitPointerCallableReturnSummary,
    UnitCallableTarget,
    UnitExpressionId,
    UnitPointerCallableReturnOrigin,
    UnitTypeId
);

#[derive(Clone, Debug, Default)]
pub(super) struct FileCallableFacts {
    pub(super) origins: Vec<CallableOriginFact>,
    pub(super) returns: Vec<PointerCallableReturnSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct UnitCallableFacts {
    pub(super) origins: Vec<UnitCallableOriginFact>,
    pub(super) returns: Vec<UnitPointerCallableReturnSummary>,
}

impl OwnershipCheckedFile {
    /// Deterministic callable facts from this successful analysis; unavailable sources are absent.
    #[must_use]
    pub fn callable_origins(&self) -> &[CallableOriginFact] {
        &self.callable_provenance.origins
    }
    /// No source is guessed from a Function signature or a deferred name expression.
    #[must_use]
    pub fn callable_origin(&self, expression: ExpressionId) -> Option<CallableOriginFact> {
        self.callable_origins()
            .iter()
            .copied()
            .find(|fact| fact.expression == expression)
    }
    /// Empty-environment return summaries belonging to this same typed/ownership chain.
    #[must_use]
    pub fn pointer_callable_returns(&self) -> &[PointerCallableReturnSummary] {
        &self.callable_provenance.returns
    }
    /// Multiple normal returns, capturing environments and unknown sources have no summary.
    #[must_use]
    pub fn pointer_callable_return(
        &self,
        target: SymbolId,
    ) -> Option<&PointerCallableReturnSummary> {
        self.pointer_callable_returns()
            .iter()
            .find(|summary| summary.target == target)
    }
}

impl CompilationUnitOwnership {
    /// Deterministic, source-qualified callable facts from this successful analysis.
    #[must_use]
    pub fn callable_origins(&self) -> &[UnitCallableOriginFact] {
        &self.callable_provenance.origins
    }
    /// Returns the proved source at this use; type shape alone never defines an environment.
    #[must_use]
    pub fn callable_origin(&self, expression: UnitExpressionId) -> Option<UnitCallableOriginFact> {
        self.callable_origins()
            .iter()
            .copied()
            .find(|fact| fact.expression == expression)
    }
    /// Empty-environment source return summaries; callsite substitution remains a typed fact.
    #[must_use]
    pub fn pointer_callable_returns(&self) -> &[UnitPointerCallableReturnSummary] {
        &self.callable_provenance.returns
    }
    /// Returns a summary only for a completed source factory with one proved normal delivery.
    #[must_use]
    pub fn pointer_callable_return(
        &self,
        target: UnitCallableTarget,
    ) -> Option<&UnitPointerCallableReturnSummary> {
        self.pointer_callable_returns()
            .iter()
            .find(|summary| summary.target == target)
    }
}
