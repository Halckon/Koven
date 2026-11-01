//! Subset rules for `when` expressions (SPEC-0356, SPEC-0360). They mirror `build/when.rs`.
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, WhenCondition, WhenEntry},
    type_checking::{IntegerRangeOperationKind, ParameterMode, UnitTypeId},
};

use super::Checker;

impl Checker<'_, '_> {
    /// A scalar subject compared by value, an integer subject tested for Range membership,
    /// or a Copyable enum subject tested for its case. A value-context `when` needs `else`
    /// unless the subject is an enum, whose closed domain the frontend proved covered.
    pub(super) fn subject_when(
        &mut self,
        ty: UnitTypeId,
        subject: ExpressionId,
        entries: &[WhenEntry],
    ) -> bool {
        self.migrated = true;
        let Some(subject_type) = self.expression_type(subject) else {
            return false;
        };
        let tagged = self.enum_type(subject_type);
        // SPEC-0357 already validated lifetime and candidate inventory. Keep the backend
        // subset bounded to that literal kernel and its existing expression support.
        let string = self.string(subject_type);
        let subject_ok = if string {
            self.string_when_subject(subject)
        } else {
            (self.scalar(subject_type) || tagged) && self.expression(subject)
        };
        let has_else = entries.iter().any(|entry| entry.else_span.is_some());
        let boolean_exhaustive = matches!(
            self.kind(subject_type),
            Some(lang_frontend::type_checking::UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::Boolean
            ))
        ) && self.boolean_two_arms(entries);
        subject_ok
            && (self.unit(ty) || has_else || tagged || boolean_exhaustive)
            && entries.iter().all(|entry| {
                let saved = self.view_symbols.clone();
                let ok = entry.else_span.is_some() == entry.conditions.is_empty()
                    && entry.conditions.iter().all(|condition| match condition {
                        WhenCondition::Expression(condition) if string => {
                            self.string_when_literal(*condition)
                        }
                        WhenCondition::Expression(condition) if tagged => {
                            self.case_value_condition(*condition, subject_type)
                        }
                        WhenCondition::Expression(condition) => {
                            self.expression_type(*condition) == Some(subject_type)
                                && self.expression(*condition)
                        }
                        WhenCondition::Contains { expression, .. } => {
                            !tagged && !string && self.range_condition(*expression, subject_type)
                        }
                        WhenCondition::TypeTest { type_ref, .. } => {
                            tagged && self.type_test_target(*type_ref, subject_type)
                        }
                    })
                    && self.statement(entry.body);
                self.view_symbols = saved;
                ok
            })
    }

    fn boolean_two_arms(&self, entries: &[WhenEntry]) -> bool {
        let [first, second] = entries else {
            return false;
        };
        if first.else_span.is_some() || second.else_span.is_some() {
            return false;
        }
        let ([WhenCondition::Expression(left)], [WhenCondition::Expression(right)]) =
            (first.conditions.as_slice(), second.conditions.as_slice())
        else {
            return false;
        };
        let ast = self.inputs.parsed.ast().expressions();
        matches!(
            (
                ast.get(*left).map(|n| n.payload()),
                ast.get(*right).map(|n| n.payload())
            ),
            (
                Ok(Expression::Literal(
                    lang_frontend::parser::LiteralKind::Boolean(a)
                )),
                Ok(Expression::Literal(
                    lang_frontend::parser::LiteralKind::Boolean(b)
                ))
            ) if a != b
        )
    }

    fn string_when_subject(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Group { expression } => self.string_when_subject(*expression),
            Expression::Name
                if self.reference(expression).is_some_and(|symbol| {
                    self.known.contains(&symbol)
                        && self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
                }) =>
            {
                true
            }
            Expression::Member { .. } | Expression::Index { .. } => false,
            _ => self.expression(expression),
        }
    }

    fn string_when_literal(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Group { expression } => self.string_when_literal(*expression),
            Expression::String { .. } => self.expression(expression),
            _ => false,
        }
    }

    /// `in lo..hi` / `lo..<hi` over the subject's integer type.
    fn range_condition(&mut self, range: ExpressionId, subject_type: UnitTypeId) -> bool {
        let Some(operation) = self
            .inputs
            .typed
            .integer_range_operation(self.id(range))
            .copied()
        else {
            return false;
        };
        matches!(
            operation.kind(),
            IntegerRangeOperationKind::Inclusive | IntegerRangeOperationKind::Exclusive
        ) && self.integer_type(subject_type)
            && self.c(operation.left_type()) == subject_type
            && self.c(operation.right_type()) == subject_type
            && operation.left().source_unit() == self.inputs.source_unit
            && operation.right().source_unit() == self.inputs.source_unit
            && self.expression(operation.left().expression())
            && self.expression(operation.right().expression())
    }

    /// Boolean alternatives tested in order; a value-context `when` needs `else`.
    pub(super) fn subjectless_when(&mut self, ty: UnitTypeId, entries: &[WhenEntry]) -> bool {
        self.migrated = true;
        let has_else = entries.iter().any(|entry| entry.else_span.is_some());
        (self.unit(ty) || has_else)
            && entries.iter().all(|entry| {
                entry.else_span.is_some() == entry.conditions.is_empty()
                    && entry.conditions.iter().all(|condition| {
                        matches!(condition, WhenCondition::Expression(condition)
                            if self.condition(*condition, false))
                    })
                    && self.statement(entry.body)
            })
    }
}
