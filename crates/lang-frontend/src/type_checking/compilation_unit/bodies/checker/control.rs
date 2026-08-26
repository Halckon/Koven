use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::Statement,
    source::Span,
    type_checking::{BuiltinType, TypeCheckingError, UnitTypeId, UnitTypeKind},
};

use super::flow::{extend_facts, intersect_facts};
use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_if(
        &mut self,
        source: SourceUnitId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_span: Option<Span>,
        else_branch: Option<StatementId>,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let boolean = self.builtin(BuiltinType::Boolean);
        self.check_expression(source, condition, Some(boolean), None, return_type)?;
        let baseline = self.flow_facts.clone();
        let (true_facts, false_facts) = self.condition_facts(source, condition)?;
        self.flow_facts = extend_facts(&baseline, &true_facts);
        let then_result =
            self.check_value_body(source, then_branch, expected, expected_span, return_type)?;
        let then_facts = self.flow_facts.clone();
        let Some(else_branch) = else_branch else {
            let false_path = extend_facts(&baseline, &false_facts);
            self.flow_facts = if then_result.falls_through {
                intersect_facts(&then_facts, &false_path)
            } else {
                false_path
            };
            return Ok(ExpressionCheck {
                ty: self.builtin(BuiltinType::Unit),
                falls_through: true,
            });
        };
        self.flow_facts = extend_facts(&baseline, &false_facts);
        let else_result =
            self.check_value_body(source, else_branch, expected, expected_span, return_type)?;
        let else_facts = self.flow_facts.clone();
        self.flow_facts = match (then_result.falls_through, else_result.falls_through) {
            (true, true) => intersect_facts(&then_facts, &else_facts),
            (true, false) => then_facts,
            (false, true) => else_facts,
            (false, false) => baseline,
        };
        let ty = if let Some(join) = self.join_control_types(then_result.ty, else_result.ty) {
            join
        } else {
            let primary = else_span.unwrap_or(
                self.file(source)
                    .ast()
                    .statements()
                    .get(else_branch)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            );
            let first = self
                .file(source)
                .ast()
                .statements()
                .get(then_branch)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::NO_COMMON_BRANCH_TYPE,
                "control branches do not have a common type",
                primary,
                Some(first),
                format!("first branch has type {}", self.type_name(then_result.ty)),
            )?;
            self.error_type()
        };
        Ok(ExpressionCheck {
            ty,
            falls_through: then_result.falls_through || else_result.falls_through,
        })
    }

    fn check_value_body(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .statements()
            .get(statement)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let payload = node.payload().clone();
        match payload {
            Statement::ControlBody { elements } => self.check_value_elements(
                source,
                &elements,
                span,
                expected,
                expected_span,
                return_type,
            ),
            Statement::Expression { expression } => {
                self.check_expression(source, expression, expected, expected_span, return_type)
            }
            _ => self.check_statement(source, statement, return_type, expected_span),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_value_elements(
        &mut self,
        source: SourceUnitId,
        elements: &[StatementId],
        body_span: Span,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let unit = self.builtin(BuiltinType::Unit);
        let Some((&last, prefix)) = elements.split_last() else {
            let ty = self.check_empty_control_body(unit, body_span, expected, expected_span)?;
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        };
        let mut falls_through = true;
        for &element in prefix {
            let result = self.check_statement(source, element, return_type, expected_span)?;
            falls_through &= result.falls_through;
        }
        let payload = self
            .file(source)
            .ast()
            .statements()
            .get(last)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        let result = if let Statement::Expression { expression } = payload {
            self.check_expression(source, expression, expected, expected_span, return_type)?
        } else {
            self.check_statement(source, last, return_type, expected_span)?
        };
        falls_through &= result.falls_through;
        Ok(ExpressionCheck {
            ty: result.ty,
            falls_through,
        })
    }

    fn check_empty_control_body(
        &mut self,
        unit: UnitTypeId,
        body_span: Span,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(expected) = expected else {
            return Ok(unit);
        };
        if self.assignable(unit, expected) {
            return Ok(unit);
        }
        self.emit_maybe_label(
            codes::TYPE_MISMATCH,
            "expression type does not match the expected type",
            body_span,
            expected_span,
            format!(
                "expected {}, found {}",
                self.type_name(expected),
                self.type_name(unit)
            ),
        )?;
        Ok(self.error_type())
    }

    fn join_control_types(&self, left: UnitTypeId, right: UnitTypeId) -> Option<UnitTypeId> {
        if left == right {
            return Some(left);
        }
        if self.is_builtin(left, BuiltinType::Nothing) {
            return Some(right);
        }
        if self.is_builtin(right, BuiltinType::Nothing) {
            return Some(left);
        }
        match (
            self.signatures.types().get(left),
            self.signatures.types().get(right),
        ) {
            (Some(UnitTypeKind::Error), _) => Some(right),
            (_, Some(UnitTypeKind::Error)) => Some(left),
            (Some(UnitTypeKind::Nullable(_)), _) if self.assignable(right, left) => Some(left),
            (_, Some(UnitTypeKind::Nullable(_))) if self.assignable(left, right) => Some(right),
            (Some(UnitTypeKind::EnumCase { root, .. }), _) if *root == right => Some(right),
            (_, Some(UnitTypeKind::EnumCase { root, .. })) if left == *root => Some(left),
            (
                Some(UnitTypeKind::EnumCase {
                    root: left_root, ..
                }),
                Some(UnitTypeKind::EnumCase {
                    root: right_root, ..
                }),
            ) if left_root == right_root => Some(*left_root),
            _ => None,
        }
    }
}
