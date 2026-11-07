//! SPEC-0197 compilation-unit expression 类型与 value-category facts。

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{BinaryOperator, Expression, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{
        DeferredReason, ExpressionCategory, UnitAggregateProjectionKind, UnitConstructionTarget,
        UnitTypeId, UnitTypeKind,
    },
};

use super::{
    BodyChecker, CompilationUnitTypeError, UnitExpressionId, trial::DeferredFact,
    type_graph::CandidateTypes,
};

#[derive(Clone, Copy)]
pub(super) enum ExpressionExpected<'a> {
    Canonical(UnitTypeId),
    Candidate(&'a CandidateTypes, UnitTypeId),
}

impl ExpressionExpected<'_> {
    pub(super) fn resolve(
        self,
        checker: &mut BodyChecker<'_>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        match self {
            Self::Canonical(ty) => Ok(ty),
            Self::Candidate(types, ty) => {
                types.materialize_expected(checker.signatures.types_mut(), ty)
            }
        }
    }

    pub(super) fn kind<'a>(&'a self, checker: &'a BodyChecker<'_>) -> Option<&'a UnitTypeKind> {
        let types = checker.signatures.types();
        match self {
            Self::Canonical(ty) => types.get(*ty),
            Self::Candidate(candidate, ty) => candidate.kind(types, *ty),
        }
    }

    pub(super) fn child(self, ty: UnitTypeId) -> Self {
        match self {
            Self::Canonical(_) => Self::Canonical(ty),
            Self::Candidate(candidate, _) => Self::Candidate(candidate, ty),
        }
    }

    pub(super) fn context_for(
        self,
        checker: &mut BodyChecker<'_>,
        expression: &Expression,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        // Forms that dispatch to a separate checker can resolve their own
        // expectation. Context-free forms only compare the finished result.
        let contextual = match expression {
            Expression::Literal(LiteralKind::Integer(_))
            | Expression::Prefix {
                operator: PrefixOperator::Minus,
                ..
            } => matches!(self.kind(checker), Some(UnitTypeKind::Builtin(_))),
            Expression::Binary {
                operator: BinaryOperator::Elvis,
                ..
            }
            | Expression::If { .. }
            | Expression::When { .. } => true,
            _ => false,
        };
        if matches!(self, Self::Canonical(_)) || contextual {
            return self.resolve(checker).map(Some);
        }
        Ok(None)
    }

    pub(super) fn probe(
        self,
        checker: &mut BodyChecker<'_>,
        check: impl FnOnce(&mut BodyChecker<'_>, UnitTypeId) -> Result<bool, CompilationUnitTypeError>,
    ) -> Result<bool, CompilationUnitTypeError> {
        let checkpoint =
            matches!(self, Self::Candidate(..)).then(|| checker.signatures.types().len());
        let result = self.resolve(checker).and_then(|ty| check(checker, ty));
        if let Some(checkpoint) = checkpoint {
            checker.signatures.types_mut().discard_after(checkpoint);
        }
        result
    }
}

impl BodyChecker<'_> {
    pub(super) fn apply_expression_expectation(
        &mut self,
        key: UnitExpressionId,
        span: Span,
        actual: UnitTypeId,
        expected: ExpressionExpected<'_>,
        expected_span: Option<Span>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if self.is_error(actual) || self.is_deferred(actual) {
            return Ok(actual);
        }
        let mismatch = expected.probe(self, |checker, expected| {
            if checker.assignable(actual, expected)
                || checker.is_error(expected)
                || checker.is_deferred(expected)
                || (checker.contains_unresolved_call_type(actual)
                    || checker.contains_unresolved_call_type(expected))
                    && checker.call_argument_could_match(actual, expected, true)
            {
                return Ok(false);
            }
            checker.emit_maybe_label(
                codes::TYPE_MISMATCH,
                "expression type does not match the expected type",
                span,
                expected_span,
                format!(
                    "expected {}, found {}",
                    checker.type_name(expected),
                    checker.type_name(actual)
                ),
            )?;
            Ok(true)
        })?;
        if mismatch {
            self.parts
                .assignments
                .retain(|descriptor| descriptor.expression() != key);
            return Ok(self.error_type());
        }
        Ok(actual)
    }

    pub(super) fn expression_category(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> ExpressionCategory {
        let id = UnitExpressionId::new(source, expression);
        if self.parts.constructions.iter().any(|construction| {
            construction.expression() == id
                && matches!(construction.target(), UnitConstructionTarget::EnumCase(_))
        }) {
            return ExpressionCategory::Temporary;
        }
        if self
            .parts
            .constant_selections
            .contains_key(&UnitExpressionId::new(source, expression))
        {
            return ExpressionCategory::Temporary;
        }
        if let Ok(node) = self.file(source).ast().expressions().get(expression)
            && let Expression::Group { expression } = node.payload()
        {
            return self
                .parts
                .expression_categories
                .get(&UnitExpressionId::new(source, *expression))
                .copied()
                .unwrap_or(ExpressionCategory::Temporary);
        }
        let expression = UnitExpressionId::new(source, expression);
        if self.parts.aggregate_projections.iter().any(|projection| {
            projection.expression() == expression
                && projection.kind() == UnitAggregateProjectionKind::Field
        }) {
            return ExpressionCategory::Place;
        }
        if self
            .parts
            .element_places
            .iter()
            .any(|place| place.expression() == expression)
        {
            return ExpressionCategory::Place;
        }
        if self
            .is_read_only_container_size(source, expression.expression())
            .unwrap_or(false)
        {
            return ExpressionCategory::Temporary;
        }
        if self.is_syntactic_place(source, expression.expression()) {
            ExpressionCategory::Place
        } else {
            ExpressionCategory::Temporary
        }
    }

    pub(super) fn record_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        ty: UnitTypeId,
    ) {
        let key = UnitExpressionId::new(source, expression);
        let previous = self.store_expression(key, ty);
        self.note_deferred_fact(DeferredFact::Expression(key), previous, ty);
    }

    pub(super) fn record_container_callee(&mut self, source: SourceUnitId, callee: ExpressionId) {
        // The published constructor descriptor supplies the static callee
        // contract. This placeholder is not an unresolved argument fact.
        let ty = self.deferred_type(DeferredReason::Call);
        self.store_expression(UnitExpressionId::new(source, callee), ty);
    }

    fn store_expression(&mut self, key: UnitExpressionId, ty: UnitTypeId) -> Option<UnitTypeId> {
        let previous = self.parts.expression_types.insert(key, ty);
        self.parts.expression_categories.insert(
            key,
            self.expression_category(key.source_unit(), key.expression()),
        );
        previous
    }

    pub(super) fn is_syntactic_place(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> bool {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::This => self.current_receiver.is_some(),
            Expression::Member { name_span, .. } => !matches!(
                self.reference(source, *name_span, Namespace::Value),
                Some(UnitReferenceTarget::Declaration(_))
            ),
            Expression::Name => matches!(
                self.reference(source, node.span(), Namespace::Value),
                Some(UnitReferenceTarget::Symbol(_))
            ),
            Expression::Group { expression } => self.is_syntactic_place(source, *expression),
            _ => false,
        }
    }
}
