//! Environment-bound ordinary numeric Range facts; independent of borrowed View carriers.
use super::{BuiltinType, TypeId};
use crate::{ast::ExpressionId, parser::BinaryOperator};

/// A fully typed numeric Range operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerRangeOperationKind {
    /// Inclusive endpoint construction.
    Inclusive,
    /// Exclusive upper endpoint construction.
    Exclusive,
    /// Element membership.
    Contains,
    /// Negated element membership.
    NotContains,
    /// Ascending sequence equality.
    Equal,
    /// Negated ascending sequence equality.
    NotEqual,
}

/// Source identities and concrete operand/result types selected by Phase 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerRangeOperationDescriptor {
    expression: ExpressionId,
    left: ExpressionId,
    right: ExpressionId,
    left_type: TypeId,
    right_type: TypeId,
    result_type: TypeId,
    kind: IntegerRangeOperationKind,
}
impl IntegerRangeOperationDescriptor {
    pub(crate) const fn new(
        expression: ExpressionId,
        left: ExpressionId,
        right: ExpressionId,
        left_type: TypeId,
        right_type: TypeId,
        result_type: TypeId,
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
    pub const fn expression(self) -> ExpressionId {
        self.expression
    }
    /// Left operand, evaluated first.
    #[must_use]
    pub const fn left(self) -> ExpressionId {
        self.left
    }
    /// Right operand, evaluated second.
    #[must_use]
    pub const fn right(self) -> ExpressionId {
        self.right
    }
    /// Concrete left type.
    #[must_use]
    pub const fn left_type(self) -> TypeId {
        self.left_type
    }
    /// Concrete right type.
    #[must_use]
    pub const fn right_type(self) -> TypeId {
        self.right_type
    }
    /// Concrete result type.
    #[must_use]
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }
    /// Selected operation, never inferred from member spelling.
    #[must_use]
    pub const fn kind(self) -> IntegerRangeOperationKind {
        self.kind
    }
}

pub(crate) const fn range_endpoint(builtin: BuiltinType) -> Option<BuiltinType> {
    match builtin {
        BuiltinType::IntRange => Some(BuiltinType::Int),
        BuiltinType::LongRange => Some(BuiltinType::Long),
        _ => None,
    }
}

/// None means an unrelated binary operation; Some(None) is an invalid Range pair.
pub(crate) fn select_range_operation(
    operator: BinaryOperator,
    left: Option<BuiltinType>,
    right: Option<BuiltinType>,
) -> Option<Option<(IntegerRangeOperationKind, BuiltinType)>> {
    use IntegerRangeOperationKind as Kind;
    let result = match operator {
        BinaryOperator::InclusiveRange | BinaryOperator::ExclusiveRange => {
            let result = match (left, right) {
                (Some(BuiltinType::Int), Some(BuiltinType::Int)) => Some(BuiltinType::IntRange),
                (Some(BuiltinType::Long), Some(BuiltinType::Long)) => Some(BuiltinType::LongRange),
                _ => None,
            };
            result.map(|result| {
                (
                    if operator == BinaryOperator::InclusiveRange {
                        Kind::Inclusive
                    } else {
                        Kind::Exclusive
                    },
                    result,
                )
            })
        }
        BinaryOperator::In | BinaryOperator::NotIn if right.and_then(range_endpoint).is_some() => {
            (left == right.and_then(range_endpoint)).then_some((
                if operator == BinaryOperator::In {
                    Kind::Contains
                } else {
                    Kind::NotContains
                },
                BuiltinType::Boolean,
            ))
        }
        BinaryOperator::Equal | BinaryOperator::NotEqual
            if left.and_then(range_endpoint).is_some()
                || right.and_then(range_endpoint).is_some() =>
        {
            (left == right).then_some((
                if operator == BinaryOperator::Equal {
                    Kind::Equal
                } else {
                    Kind::NotEqual
                },
                BuiltinType::Boolean,
            ))
        }
        _ => return None,
    };
    Some(result)
}

impl IntegerRangeOperationKind {
    pub(crate) const fn operator(self) -> BinaryOperator {
        match self {
            Self::Inclusive => BinaryOperator::InclusiveRange,
            Self::Exclusive => BinaryOperator::ExclusiveRange,
            Self::Contains => BinaryOperator::In,
            Self::NotContains => BinaryOperator::NotIn,
            Self::Equal => BinaryOperator::Equal,
            Self::NotEqual => BinaryOperator::NotEqual,
        }
    }
}
