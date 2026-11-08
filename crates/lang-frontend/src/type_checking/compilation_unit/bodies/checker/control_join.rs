//! Closed value-branch joins; no implicit Any fallback.
use super::{BodyChecker, BuiltinType, CompilationUnitTypeError, UnitTypeId, UnitTypeKind};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SourceUnitId,
    parser::Expression,
    type_checking::{DeferredReason, TypeCheckingError, UnitExpressionId},
};

const INTEGER_WIDTHS: [BuiltinType; 8] = [
    BuiltinType::Byte,
    BuiltinType::Short,
    BuiltinType::Int,
    BuiltinType::Long,
    BuiltinType::UByte,
    BuiltinType::UShort,
    BuiltinType::UInt,
    BuiltinType::ULong,
];

#[derive(Clone, Copy)]
struct IntegerBranch {
    widths: u8,
    default: Option<BuiltinType>,
    nullable: bool,
}

impl BodyChecker<'_> {
    pub(super) fn join_value_branches(
        &mut self,
        expression: UnitExpressionId,
        left: (StatementId, UnitTypeId),
        right: (StatementId, UnitTypeId),
        expected: Option<UnitTypeId>,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let constrained =
            self.has_contextual_integer(left.1) || self.has_contextual_integer(right.1);
        let expected_pending = matches!(
            expected.and_then(|ty| self.signatures.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::ContextualArgument))
        );
        let ordinary = self.join_control_types(left.1, right.1);
        if constrained || expected_pending {
            match (
                self.branch_integer(source, left)?,
                self.branch_integer(source, right)?,
            ) {
                (Some(a), Some(b)) => {
                    let widths = a.widths & b.widths;
                    let default = (a.default == b.default).then_some(a.default).flatten();
                    let nullable = a.nullable || b.nullable;
                    // A nullable expectation cannot retune an integer literal.
                    if widths == 0 || (nullable && default.is_none()) {
                        return Ok(None);
                    }
                    // A later branch must intersect this result rather than
                    // infer possibilities again from the first branch's syntax.
                    if constrained
                        || widths.count_ones() > 1
                        || widths != a.widths
                        || widths != b.widths
                        || ordinary.is_none()
                    {
                        return Ok(Some(self.deferred_type(
                            DeferredReason::ContextualInteger {
                                widths,
                                default,
                                nullable,
                            },
                        )));
                    }
                }
                (Some(branch), None) | (None, Some(branch)) if constrained => {
                    let (constrained_type, other) = if self.has_contextual_integer(left.1) {
                        (left.1, right.1)
                    } else {
                        (right.1, left.1)
                    };
                    match self.signatures.types().get(other) {
                        Some(UnitTypeKind::Nullable(inner))
                            if self.is_builtin(*inner, BuiltinType::Nothing) =>
                        {
                            if branch.default.is_none() {
                                return Ok(None);
                            }
                            return Ok(Some(self.deferred_type(
                                DeferredReason::ContextualInteger {
                                    widths: branch.widths,
                                    default: branch.default,
                                    nullable: true,
                                },
                            )));
                        }
                        _ if self.is_error(other)
                            || self.is_deferred(other)
                            || self.is_builtin(other, BuiltinType::Nothing) =>
                        {
                            return Ok(Some(constrained_type));
                        }
                        _ => return Ok(None),
                    }
                }
                _ => {}
            }
        }
        // Keep the deferred element in the returned value type. The known
        // branch requirement is for candidate comparison only.
        if let Some(list) = [left.1, right.1]
            .into_iter()
            .find(|&ty| self.is_contextual_list(ty))
        {
            let requirement = |(body, ty)| {
                self.contextual_branch_requirement(source, body)
                    .map(|required| required.unwrap_or(ty))
            };
            let left_type = match self.parts.contextual_argument_requirements.get(&expression) {
                Some(&required) => required,
                None => requirement(left)?,
            };
            let right_type = requirement(right)?;
            if self.types_could_be_equal(left_type, right_type) {
                let required = if self.is_unresolved_call_type(left_type)
                    || (self.is_contextual_list(left_type)
                        && !self.is_unresolved_call_type(right_type))
                {
                    right_type
                } else {
                    left_type
                };
                self.parts
                    .contextual_argument_requirements
                    .insert(expression, required);
                return Ok(Some(list));
            }
            for (other, required) in [(left_type, right_type), (right_type, left_type)] {
                if self.is_builtin(other, BuiltinType::Nothing) || self.is_error(other) {
                    self.parts
                        .contextual_argument_requirements
                        .insert(expression, required);
                    return Ok(ordinary);
                }
            }
            return Ok(None);
        }
        Ok(ordinary)
    }

    pub(super) fn contextual_when_branch_type(
        &mut self,
        source: SourceUnitId,
        body: StatementId,
        ty: UnitTypeId,
        expected: Option<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if matches!(
            expected.and_then(|ty| self.signatures.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::ContextualArgument))
        ) && let Some(branch) = self.branch_integer(source, (body, ty))?
        {
            return Ok(self.deferred_type(DeferredReason::ContextualInteger {
                widths: branch.widths,
                default: branch.default,
                nullable: branch.nullable,
            }));
        }
        Ok(ty)
    }

    pub(super) fn contextual_branch_requirement(
        &self,
        source: SourceUnitId,
        body: StatementId,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let Some(tail) =
            crate::type_checking::control_body::tail_expression(self.file(source), body)?
        else {
            return Ok(None);
        };
        self.contextual_argument_requirement(source, tail)
    }

    fn branch_integer(
        &self,
        source: SourceUnitId,
        (body, ty): (StatementId, UnitTypeId),
    ) -> Result<Option<IntegerBranch>, CompilationUnitTypeError> {
        if let Some(branch) = self.contextual_integer(ty) {
            return Ok(Some(branch));
        }
        let (ty, nullable) = match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Nullable(inner)) => (*inner, true),
            _ => (ty, false),
        };
        let Some(default) = self
            .builtin_kind(ty)
            .filter(|&ty| integer_width_bit(ty) != 0)
        else {
            return Ok(None);
        };
        let mut widths = integer_width_bit(default);
        if let Some(expression) =
            crate::type_checking::control_body::tail_expression(self.file(source), body)?
        {
            for ty in self.contextual_integer_types(source, expression)? {
                if let Some(builtin) = self.builtin_kind(ty) {
                    widths |= integer_width_bit(builtin);
                }
            }
        }
        Ok(Some(IntegerBranch {
            widths,
            default: Some(default),
            nullable,
        }))
    }

    fn contextual_integer(&self, ty: UnitTypeId) -> Option<IntegerBranch> {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Deferred(DeferredReason::ContextualInteger {
                widths,
                default,
                nullable,
            })) => Some(IntegerBranch {
                widths: *widths,
                default: *default,
                nullable: *nullable,
            }),
            _ => None,
        }
    }

    pub(super) fn has_contextual_integer(&self, ty: UnitTypeId) -> bool {
        self.contextual_integer(ty).is_some()
    }

    pub(super) fn contextual_integer_could_match(
        &self,
        actual: UnitTypeId,
        expected: UnitTypeId,
        pending: bool,
    ) -> Option<bool> {
        let branch = self.contextual_integer(actual)?;
        Some(match self.signatures.types().get(expected) {
            Some(UnitTypeKind::Builtin(ty)) => {
                !branch.nullable && branch.widths & integer_width_bit(*ty) != 0
            }
            Some(UnitTypeKind::Nullable(inner)) => branch.default.is_some_and(|ty| {
                self.call_argument_could_match(self.builtin(ty), *inner, pending)
            }),
            _ => pending && self.is_unresolved_call_type(expected),
        })
    }

    fn is_contextual_list(&self, ty: UnitTypeId) -> bool {
        self.container_parts(ty).is_some_and(|(_, element)| {
            matches!(
                self.signatures.types().get(element),
                Some(UnitTypeKind::Deferred(DeferredReason::ContextualArgument))
            )
        })
    }

    pub(super) fn contextual_argument_requirement(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let requirements = &self.parts.contextual_argument_requirements;
        if requirements.is_empty() {
            return Ok(None);
        }
        let expressions = self.file(source).ast().expressions();
        loop {
            if let Some(&required) = requirements.get(&UnitExpressionId::new(source, expression)) {
                return Ok(Some(required));
            }
            match expressions
                .get(expression)
                .map_err(TypeCheckingError::from)?
                .payload()
            {
                Expression::Group { expression: inner } => expression = *inner,
                _ => return Ok(None),
            }
        }
    }

    pub(super) fn control_branch_expected(
        &mut self,
        source: crate::name_resolution::SourceUnitId,
        body: crate::ast::StatementId,
        expected: Option<UnitTypeId>,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        if expected.is_some()
            || !crate::type_checking::control_body::ends_in_null(self.file(source), body)?
        {
            return Ok(expected);
        }
        let nothing = self.builtin(BuiltinType::Nothing);
        Ok(Some(
            self.signatures
                .types_mut()
                .intern(UnitTypeKind::Nullable(nothing)),
        ))
    }

    pub(super) fn join_control_types(
        &mut self,
        left: UnitTypeId,
        right: UnitTypeId,
    ) -> Option<UnitTypeId> {
        if self.is_deferred(left) {
            return Some(left);
        }
        if self.is_deferred(right) {
            return Some(right);
        }
        if left == right {
            return Some(left);
        }
        if self.is_error(left) {
            return Some(right);
        }
        if self.is_error(right) {
            return Some(left);
        }
        if self.is_builtin(left, BuiltinType::Nothing) {
            return Some(right);
        }
        if self.is_builtin(right, BuiltinType::Nothing) {
            return Some(left);
        }
        let left_kind = self.signatures.types().get(left)?.clone();
        let right_kind = self.signatures.types().get(right)?.clone();
        let left_inner = match left_kind {
            UnitTypeKind::Nullable(inner) => inner,
            _ => left,
        };
        let right_inner = match right_kind {
            UnitTypeKind::Nullable(inner) => inner,
            _ => right,
        };
        if left_inner != left || right_inner != right {
            let inner = self.join_control_types(left_inner, right_inner)?;
            // Case refinements are internal flow types; nullable values use their runtime root.
            let inner = match self.signatures.types().get(inner)? {
                UnitTypeKind::EnumCase { root, .. } => *root,
                _ => inner,
            };
            return Some(
                self.signatures
                    .types_mut()
                    .intern(UnitTypeKind::Nullable(inner)),
            );
        }
        match (left_kind, right_kind) {
            (UnitTypeKind::EnumCase { root, .. }, _) if root == right => Some(right),
            (_, UnitTypeKind::EnumCase { root, .. }) if left == root => Some(left),
            (
                UnitTypeKind::EnumCase {
                    root: left_root, ..
                },
                UnitTypeKind::EnumCase {
                    root: right_root, ..
                },
            ) if left_root == right_root => Some(left_root),
            _ => None,
        }
    }
}

fn integer_width_bit(ty: BuiltinType) -> u8 {
    INTEGER_WIDTHS
        .iter()
        .position(|&width| width == ty)
        .map_or(0, |index| 1 << index)
}
