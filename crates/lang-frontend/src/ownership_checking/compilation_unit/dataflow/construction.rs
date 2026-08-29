//! Source-qualified construction ordered-delivery dataflow。

use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{
        ConstructionDeliveryKind, ConstructionRootKind, UnitConstructionDeliveryEffect,
        UnitConstructionOwnershipPlan, UnitConstructionRootDropObligation,
    },
    parser::{Expression, ParsedFile},
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, NominalKind, UnitConstructionDescriptor,
        UnitConstructionTarget, UnitTypeKind,
    },
};

use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};

pub(super) fn validate_constructions<'descriptor>(
    parsed: &ParsedFile,
    source_unit: SourceUnitId,
    typed: &crate::type_checking::CompilationUnitTypes,
    descriptors: impl IntoIterator<Item = &'descriptor UnitConstructionDescriptor>,
) -> Result<(), OwnershipCheckingError> {
    for descriptor in descriptors {
        validate_construction(parsed, source_unit, typed, descriptor)?;
    }
    Ok(())
}

fn validate_construction(
    parsed: &ParsedFile,
    source_unit: SourceUnitId,
    typed: &crate::type_checking::CompilationUnitTypes,
    descriptor: &UnitConstructionDescriptor,
) -> Result<(), OwnershipCheckingError> {
    let expression = descriptor.expression();
    if expression.source_unit() != source_unit
        || typed.expression_type(expression) != Some(descriptor.result_type())
        || typed.types().get(descriptor.result_type()).is_none()
        || descriptor
            .instance()
            .type_arguments()
            .iter()
            .any(|&ty| typed.types().get(ty).is_none())
    {
        return Err(invalid_construction(source_unit, expression.expression()));
    }

    let node = parsed.ast().expressions().get(expression.expression())?;
    let source_arguments = match node.payload() {
        Expression::Call { arguments, .. } => arguments
            .iter()
            .map(|argument| argument.value)
            .collect::<Vec<_>>(),
        Expression::Name | Expression::Member { .. } if descriptor.arguments().is_empty() => {
            Vec::new()
        }
        _ => return Err(invalid_construction(source_unit, expression.expression())),
    };
    let expected_symbols = construction_parameter_symbols(
        typed,
        descriptor.target(),
        source_unit,
        descriptor.expression().expression(),
    )?;
    if source_arguments.len() != descriptor.arguments().len()
        || expected_symbols.len() != descriptor.arguments().len()
    {
        return Err(invalid_construction(source_unit, expression.expression()));
    }

    let mut seen_evaluations = vec![false; source_arguments.len()];
    for (parameter_index, argument) in descriptor.arguments().iter().enumerate() {
        let evaluation_index = argument.evaluation_index();
        if argument.parameter_index() != parameter_index
            || argument.parameter_symbol() != expected_symbols[parameter_index]
            || argument.argument().source_unit() != source_unit
            || evaluation_index >= source_arguments.len()
            || seen_evaluations[evaluation_index]
            || source_arguments[evaluation_index] != argument.argument().expression()
            || typed.expression_category(argument.argument()) != Some(argument.category())
            || typed.types().get(argument.parameter_type()).is_none()
        {
            return Err(invalid_construction(source_unit, expression.expression()));
        }
        seen_evaluations[evaluation_index] = true;
    }
    if descriptor.target() == UnitConstructionTarget::IntrinsicBox
        && descriptor.arguments().first().is_none_or(|argument| {
            argument.parameter_name() != "element" || argument.parameter_symbol().is_some()
        })
    {
        return Err(invalid_construction(source_unit, expression.expression()));
    }
    if descriptor.target() == UnitConstructionTarget::IntrinsicRc
        && descriptor.arguments().first().is_none_or(|argument| {
            argument.parameter_name() != "value" || argument.parameter_symbol().is_some()
        })
    {
        return Err(invalid_construction(source_unit, expression.expression()));
    }
    Ok(())
}

fn construction_parameter_symbols(
    typed: &crate::type_checking::CompilationUnitTypes,
    target: UnitConstructionTarget,
    source_unit: SourceUnitId,
    expression: crate::ast::ExpressionId,
) -> Result<Vec<Option<crate::name_resolution::UnitSymbolId>>, OwnershipCheckingError> {
    let symbols = match target {
        UnitConstructionTarget::Nominal(declaration) => typed
            .signatures()
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .filter(|nominal| {
                matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass)
            })
            .map(|nominal| {
                nominal
                    .fields()
                    .iter()
                    .map(|field| Some(field.symbol()))
                    .collect()
            }),
        UnitConstructionTarget::EnumCase(target) => typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.enum_cases())
            .find(|case| case.value_symbol() == target)
            .map(|case| {
                case.payloads()
                    .iter()
                    .map(|payload| Some(payload.symbol()))
                    .collect()
            }),
        UnitConstructionTarget::IntrinsicBox | UnitConstructionTarget::IntrinsicRc => {
            Some(vec![None])
        }
    };
    symbols.ok_or_else(|| invalid_construction(source_unit, expression))
}

const fn invalid_construction(
    source_unit: SourceUnitId,
    expression: crate::ast::ExpressionId,
) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitConstruction {
        source_unit: source_unit.index(),
        expression: expression.index(),
    }
}

impl Checker<'_> {
    pub(super) fn check_construction(
        &mut self,
        descriptor: UnitConstructionDescriptor,
        state: State,
        _usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let mut flows = Flows::next(state);
        let mut arguments = descriptor.arguments().to_vec();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        let mut deliveries = Vec::with_capacity(arguments.len());

        for argument in arguments {
            let parameter_span = argument
                .parameter_symbol()
                .map(|symbol| self.symbol_span(symbol))
                .transpose()?;
            let argument_diagnostics = self.diagnostics.len();
            if let Some(next) = flows.next.as_ref() {
                self.reject_borrowed_closure_escape(argument.argument().expression(), next)?;
            }
            flows = self.chain_expression(
                flows,
                argument.argument().expression(),
                ExpressionUse::Consume { parameter_span },
            )?;

            if self.is_nothing_expression(argument.argument()) {
                flows.next = None;
                if self.diagnostics.len() == diagnostic_count {
                    self.construction_plans
                        .push(UnitConstructionOwnershipPlan::new(
                            descriptor.expression(),
                            descriptor.target(),
                            deliveries,
                            None,
                            Some(argument.argument()),
                        ));
                }
                return Ok(flows);
            }
            if self.diagnostics.len() != argument_diagnostics || flows.next.is_none() {
                continue;
            }
            deliveries.push(UnitConstructionDeliveryEffect::new(
                descriptor.expression(),
                argument.argument(),
                argument.parameter_index(),
                argument.parameter_symbol(),
                argument.evaluation_index(),
                self.construction_delivery_kind(argument.argument(), argument.category())?,
            ));
        }

        if flows.next.is_some() && self.diagnostics.len() == diagnostic_count {
            let root_obligation = match self.typed.copyability(descriptor.result_type()) {
                Copyability::Copyable => None,
                Copyability::MoveOnly => Some(UnitConstructionRootDropObligation::new(
                    descriptor.expression(),
                    descriptor.result_type(),
                    self.construction_root_kind(
                        descriptor.target(),
                        descriptor.expression().expression(),
                    )?,
                )),
                Copyability::Unknown | Copyability::Error => {
                    return Err(self.invalid_construction(descriptor.expression().expression()));
                }
            };
            self.construction_plans
                .push(UnitConstructionOwnershipPlan::new(
                    descriptor.expression(),
                    descriptor.target(),
                    deliveries,
                    root_obligation,
                    None,
                ));
        }
        Ok(flows)
    }

    fn construction_delivery_kind(
        &self,
        expression: crate::type_checking::UnitExpressionId,
        category: ExpressionCategory,
    ) -> Result<ConstructionDeliveryKind, OwnershipCheckingError> {
        if category == ExpressionCategory::Temporary {
            return Ok(ConstructionDeliveryKind::DeliverTemporary);
        }
        let Some(ty) = self.typed.expression_type(expression) else {
            return Err(self.invalid_construction(expression.expression()));
        };
        match self.typed.copyability(ty) {
            Copyability::Copyable => Ok(ConstructionDeliveryKind::Copy),
            Copyability::MoveOnly => Ok(ConstructionDeliveryKind::Move),
            Copyability::Unknown | Copyability::Error => {
                Err(self.invalid_construction(expression.expression()))
            }
        }
    }

    fn construction_root_kind(
        &self,
        target: UnitConstructionTarget,
        expression: crate::ast::ExpressionId,
    ) -> Result<ConstructionRootKind, OwnershipCheckingError> {
        match target {
            UnitConstructionTarget::IntrinsicBox => Ok(ConstructionRootKind::HeapOwner),
            UnitConstructionTarget::IntrinsicRc => Ok(ConstructionRootKind::SharedOwner),
            UnitConstructionTarget::EnumCase(_) => Ok(ConstructionRootKind::Inline),
            UnitConstructionTarget::Nominal(declaration) => self
                .typed
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .and_then(|nominal| match nominal.kind() {
                    NominalKind::Class => Some(ConstructionRootKind::HeapOwner),
                    NominalKind::ValueClass => Some(ConstructionRootKind::Inline),
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => None,
                })
                .ok_or_else(|| self.invalid_construction(expression)),
        }
    }

    pub(super) fn is_nothing_expression(
        &self,
        expression: crate::type_checking::UnitExpressionId,
    ) -> bool {
        self.typed
            .expression_type(expression)
            .and_then(|ty| self.typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
    }

    fn invalid_construction(&self, expression: crate::ast::ExpressionId) -> OwnershipCheckingError {
        OwnershipCheckingError::InvalidUnitConstruction {
            source_unit: self.source_unit.index(),
            expression: expression.index(),
        }
    }
}
