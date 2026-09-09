//! SPEC-0197 compilation-unit null comparison 与非空 flow facts。

use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    name_resolution::SourceUnitId,
    parser::{BinaryOperator, Expression},
    type_checking::{
        CompilationUnitTypeError, UnitNonNullUseDescriptor, UnitNullComparisonDescriptor,
        UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, UnitExpressionId, flow::ConditionFacts, flow::FlowKey};

impl BodyChecker<'_> {
    pub(super) fn record_nullable_facts(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        narrowed_type: UnitTypeId,
    ) {
        self.record_non_null_use(source, expression, narrowed_type);
        self.record_null_comparison(source, expression);
    }

    fn record_non_null_use(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        narrowed_type: UnitTypeId,
    ) {
        let Some(FlowKey::Symbol(symbol)) = self.stable_flow_key(source, expression) else {
            return;
        };
        let Some(active_type) = self.flow_facts.get(&FlowKey::Symbol(symbol)).copied() else {
            return;
        };
        let Some(declared_type) = self
            .parts
            .symbol_types
            .get(&symbol)
            .copied()
            .or_else(|| self.signatures.symbol_type(symbol))
        else {
            return;
        };
        let Some(UnitTypeKind::Nullable(inner)) = self.signatures.types().get(declared_type) else {
            return;
        };
        if *inner != narrowed_type || active_type != narrowed_type {
            return;
        }
        self.parts
            .nullable
            .non_null_uses
            .push(UnitNonNullUseDescriptor::new(
                UnitExpressionId::new(source, expression),
                symbol,
                declared_type,
                narrowed_type,
            ));
    }

    fn record_null_comparison(&mut self, source: SourceUnitId, expression: ExpressionId) {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return;
        };
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = node.payload()
        else {
            return;
        };
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
            return;
        }
        let candidate = match (
            self.is_null_literal(source, *left),
            self.is_null_literal(source, *right),
        ) {
            (Ok(true), Ok(false)) => *right,
            (Ok(false), Ok(true)) => *left,
            _ => return,
        };
        let Some(FlowKey::Symbol(symbol)) = self.stable_flow_key(source, candidate) else {
            return;
        };
        let Some(nullable_type) = self
            .parts
            .symbol_types
            .get(&symbol)
            .copied()
            .or_else(|| self.signatures.symbol_type(symbol))
        else {
            return;
        };
        if !matches!(
            self.signatures.types().get(nullable_type),
            Some(UnitTypeKind::Nullable(_))
        ) {
            return;
        }
        self.parts
            .nullable
            .null_comparisons
            .push(UnitNullComparisonDescriptor::new(
                UnitExpressionId::new(source, expression),
                symbol,
                nullable_type,
                *operator == BinaryOperator::NotEqual,
            ));
    }

    pub(super) fn null_comparison_facts(
        &self,
        source: SourceUnitId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> Result<ConditionFacts, CompilationUnitTypeError> {
        let candidate = match (
            self.is_null_literal(source, left)?,
            self.is_null_literal(source, right)?,
        ) {
            (true, false) => right,
            (false, true) => left,
            _ => return Ok((BTreeMap::new(), BTreeMap::new())),
        };
        let Some(key) = self.stable_flow_key(source, candidate) else {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        };
        let Some(ty) = self
            .parts
            .expression_types
            .get(&UnitExpressionId::new(source, candidate))
            .copied()
        else {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        };
        let Some(UnitTypeKind::Nullable(inner)) = self.signatures.types().get(ty) else {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        };
        let positive = BTreeMap::from([(key, *inner)]);
        if operator == BinaryOperator::NotEqual {
            Ok((positive, BTreeMap::new()))
        } else {
            Ok((BTreeMap::new(), positive))
        }
    }
}

impl BodyChecker<'_> {
    /// 来源能力只描述 typed 候选，合法整体移动仍由 ownership 判断。
    pub(super) fn non_null_assertion_source_category(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<crate::type_checking::NullableWhenSubjectCategory, CompilationUnitTypeError> {
        use crate::{
            name_resolution::{Namespace, UnitReferenceTarget},
            type_checking::{
                NullableWhenSubjectCategory as Category, ParameterMode, UnitAggregateProjectionKind,
            },
        };
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(crate::type_checking::TypeCheckingError::from)?;
        if let Expression::Group { expression } = node.payload() {
            return self.non_null_assertion_source_category(source, *expression);
        }
        let key = UnitExpressionId::new(source, expression);
        if self
            .parts
            .element_places
            .iter()
            .any(|place| place.expression() == key)
        {
            return Ok(Category::ContainerElement);
        }
        if self.parts.aggregate_projections.iter().any(|place| {
            place.expression() == key && place.kind() == UnitAggregateProjectionKind::Field
        }) {
            return Ok(Category::OrdinaryField);
        }
        let mode = match node.payload() {
            Expression::Name => match self.reference(source, node.span(), Namespace::Value) {
                Some(UnitReferenceTarget::Symbol(symbol)) => {
                    self.parts.parameter_modes.get(symbol).copied().or_else(|| {
                        self.signatures
                            .declarations()
                            .iter()
                            .flat_map(|declaration| {
                                declaration.callable().into_iter().chain(
                                    declaration.nominal().into_iter().flat_map(|nominal| {
                                        nominal.members().iter().chain(nominal.companion_members())
                                    }),
                                )
                            })
                            .flat_map(|callable| callable.parameters())
                            .find(|parameter| parameter.symbol() == Some(*symbol))
                            .map(|parameter| parameter.mode())
                    })
                }
                _ => return Ok(Category::Temporary),
            },
            Expression::This => self.current_receiver_mode,
            _ => return Ok(Category::Temporary),
        };
        Ok(match mode {
            Some(ParameterMode::Borrow) => Category::BorrowRoot,
            Some(ParameterMode::Inout) => Category::InoutRoot,
            _ => Category::OwnedRoot,
        })
    }
}
