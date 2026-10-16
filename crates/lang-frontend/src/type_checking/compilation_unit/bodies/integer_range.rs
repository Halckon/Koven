//! Environment-bound ordinary numeric Range facts; independent of borrowed View carriers.
use crate::type_checking::{IntegerRangeOperationKind, UnitExpressionId, UnitTypeId};

/// Source identities and concrete operand/result types selected by Phase 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitIntegerRangeOperationDescriptor {
    expression: UnitExpressionId,
    left: UnitExpressionId,
    right: UnitExpressionId,
    left_type: UnitTypeId,
    right_type: UnitTypeId,
    result_type: UnitTypeId,
    kind: IntegerRangeOperationKind,
}
impl UnitIntegerRangeOperationDescriptor {
    pub(crate) const fn new(
        expression: UnitExpressionId,
        left: UnitExpressionId,
        right: UnitExpressionId,
        left_type: UnitTypeId,
        right_type: UnitTypeId,
        result_type: UnitTypeId,
        kind: IntegerRangeOperationKind,
    ) -> Self {
        Self {
            expression,
            left,
            right,
            left_type,
            right_type,
            result_type,
            kind,
        }
    }
    /// Operation expression identity.
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }
    /// Left operand, evaluated first.
    #[must_use]
    pub const fn left(self) -> UnitExpressionId {
        self.left
    }
    /// Right operand, evaluated second.
    #[must_use]
    pub const fn right(self) -> UnitExpressionId {
        self.right
    }
    /// Concrete left type.
    #[must_use]
    pub const fn left_type(self) -> UnitTypeId {
        self.left_type
    }
    /// Concrete right type.
    #[must_use]
    pub const fn right_type(self) -> UnitTypeId {
        self.right_type
    }
    /// Concrete result type.
    #[must_use]
    pub const fn result_type(self) -> UnitTypeId {
        self.result_type
    }
    /// Selected operation, never inferred from member spelling.
    #[must_use]
    pub const fn kind(self) -> IntegerRangeOperationKind {
        self.kind
    }
}

impl super::CompilationUnitTypes {
    pub(super) fn integer_range_operations_are_valid(&self) -> bool {
        use crate::type_checking::{
            ExpressionCategory, UnitTypeKind, integer_range::select_range_operation,
        };
        let builtin = |ty| match self.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        };
        let mut previous = None;
        self.integer_range_operations.iter().all(|fact| {
            let ordered = previous.is_none_or(|id| id < fact.expression);
            previous = Some(fact.expression);
            ordered
                && fact.expression != fact.left
                && fact.expression != fact.right
                && fact.expression.source_unit() == fact.left.source_unit()
                && fact.expression.source_unit() == fact.right.source_unit()
                && self.expression_type(fact.expression) == Some(fact.result_type)
                && self.expression_type(fact.left) == Some(fact.left_type)
                && self.expression_type(fact.right) == Some(fact.right_type)
                && self.expression_category(fact.expression) == Some(ExpressionCategory::Temporary)
                && builtin(fact.result_type).is_some()
                && select_range_operation(
                    fact.kind.operator(),
                    builtin(fact.left_type),
                    builtin(fact.right_type),
                ) == builtin(fact.result_type).map(|result| Some((fact.kind, result)))
        })
    }
}
