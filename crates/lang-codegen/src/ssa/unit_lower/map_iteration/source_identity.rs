//! Source identity selects the current slot; an entry owner never stands in for a carried owner.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum SourceIdentity {
    Parameter { root: UnitSymbolId, slot: usize },
    Temporary(UnitExpressionId),
}

impl SourceIdentity {
    pub(in super::super) fn parameter_root(
        self,
        span: Span,
    ) -> Result<UnitSymbolId, LoweringError> {
        match self {
            Self::Parameter { root, .. } => Ok(root),
            Self::Temporary(_) => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(in super::super) fn current(
        self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
        match self {
            Self::Parameter { root, .. } => match (
                lowerer.bindings.get(&root),
                lowerer.borrow_bindings.get(&root),
            ) {
                (Some(LoweredValue::Value(owner)), None) => Ok(EntityId::Value(*owner)),
                (None, Some(loan)) => Ok(EntityId::Loan(*loan)),
                _ => Err(missing()),
            },
            Self::Temporary(expression) => lowerer
                .temporaries
                .get(&expression)
                .copied()
                .map(EntityId::Value)
                .ok_or_else(missing),
        }
    }

    pub(in super::super) fn drop_target(self) -> UnitDropTarget {
        match self {
            Self::Parameter { root, .. } => UnitDropTarget::Named(root),
            Self::Temporary(expression) => UnitDropTarget::Temporary(expression),
        }
    }

    pub(in super::super) fn is_registered(self, lowerer: &UnitExpressionLowerer<'_>) -> bool {
        match self {
            Self::Parameter { root, .. } => {
                lowerer.bindings.contains_key(&root) || lowerer.borrow_bindings.contains_key(&root)
            }
            Self::Temporary(expression) => lowerer.temporaries.contains_key(&expression),
        }
    }
}
