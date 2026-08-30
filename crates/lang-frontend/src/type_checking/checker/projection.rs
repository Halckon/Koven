use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, TypeRefId},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        AggregateProjectionDescriptor, AggregateProjectionKind, CallDescriptor, CallableTarget,
    },
};

use super::*;

impl Checker<'_> {
    pub(super) fn aggregate_projection_for(
        &self,
        expression: ExpressionId,
    ) -> Option<AggregateProjectionDescriptor> {
        self.aggregate_projections
            .iter()
            .copied()
            .find(|projection| projection.expression() == expression)
    }

    pub(super) fn check_field_projection(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: TypeId,
        name_span: Span,
        safe: bool,
    ) -> Result<Option<TypeId>, TypeCheckingError> {
        if safe {
            return Ok(None);
        }
        if let TypeKind::EnumCase { case, root } = self.kind(receiver_type).clone() {
            let Some(case) = self.enum_case(case).cloned() else {
                return Err(TypeCheckingError::InvalidExternalBinding);
            };
            let name = self.sources.slice(name_span)?;
            let Some((field, declared_type)) =
                case.payloads().iter().copied().find(|(field, _)| {
                    self.sources.slice(self.symbol_spans[field.index()]) == Ok(name)
                })
            else {
                return Ok(None);
            };
            let Some((owner, arguments)) = self.nominal_instance(root) else {
                return Err(TypeCheckingError::InvalidExternalBinding);
            };
            if owner.id() != case.root() {
                return Err(TypeCheckingError::InvalidExternalBinding);
            }
            let substitutions = owner
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            let ty = self.substitute_type(declared_type, &substitutions)?;
            self.aggregate_projections
                .push(AggregateProjectionDescriptor::new(
                    expression,
                    AggregateProjectionReceiver::Expression(receiver),
                    field,
                    ty,
                    AggregateProjectionKind::Field,
                ));
            return Ok(Some(ty));
        }
        let Some((owner, arguments)) = self.nominal_instance(receiver_type) else {
            return Ok(None);
        };
        let name = self.sources.slice(name_span)?;
        let Some(field) = owner
            .fields()
            .iter()
            .copied()
            .find(|field| self.sources.slice(self.symbol_spans[field.index()]) == Ok(name))
        else {
            return Ok(None);
        };
        let ty = self.substituted_field_type(&owner, &arguments, field)?;
        self.aggregate_projections
            .push(AggregateProjectionDescriptor::new(
                expression,
                AggregateProjectionReceiver::Expression(receiver),
                field,
                ty,
                AggregateProjectionKind::Field,
            ));
        Ok(Some(ty))
    }

    pub(super) fn check_structural_component_call(
        &mut self,
        expression: ExpressionId,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        if !type_arguments.is_empty() || !arguments.is_empty() {
            return Ok(None);
        }
        let callee_node = self.ast().expressions().get(callee)?;
        let Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = callee_node.payload()
        else {
            return Ok(None);
        };
        let receiver = *receiver;
        let name_span = *name_span;
        let Some(component) = parse_component_name(self.sources.slice(name_span)?) else {
            return Ok(None);
        };
        let receiver_result = self.check_expression(receiver, None, None)?;
        let Some((owner, arguments)) = self.nominal_instance(receiver_result.ty) else {
            return Ok(None);
        };
        if owner.kind() != NominalKind::ValueClass {
            return Ok(None);
        }
        let Some(field) = owner.fields().get(component - 1).copied() else {
            return Ok(None);
        };
        let ty = self.substituted_field_type(&owner, &arguments, field)?;
        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: Vec::new(),
            return_type: ty,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.calls.push(CallDescriptor::new(
            expression,
            CallableTarget::StructuralComponent(field),
            Vec::new(),
            ty,
            Some(CallReceiverDescriptor {
                origin: CallReceiverOrigin::Expression(receiver),
                mode: ParameterMode::Borrow,
                category: self.expression_categories[receiver.index()],
                ty: receiver_result.ty,
            }),
            Vec::new(),
            false,
            false,
        ));
        self.aggregate_projections
            .push(AggregateProjectionDescriptor::new(
                expression,
                AggregateProjectionReceiver::Expression(receiver),
                field,
                ty,
                AggregateProjectionKind::StructuralComponent,
            ));
        Ok(Some(ExprCheck {
            ty,
            falls_through: receiver_result.falls_through,
        }))
    }

    fn nominal_instance(&self, ty: TypeId) -> Option<(NominalDescriptor, Vec<TypeId>)> {
        let TypeKind::Nominal { nominal, arguments } = self.kind(ty) else {
            return None;
        };
        let owner = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == *nominal)?
            .clone();
        Some((owner, arguments.clone()))
    }

    fn substituted_field_type(
        &mut self,
        owner: &NominalDescriptor,
        arguments: &[TypeId],
        field: SymbolId,
    ) -> Result<TypeId, TypeCheckingError> {
        let substitutions = owner
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect::<BTreeMap<_, _>>();
        let ty = self
            .symbol_type(field)
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        self.substitute_type(ty, &substitutions)
    }
}

fn parse_component_name(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("component")?;
    if digits.is_empty() || digits.starts_with('0') {
        return None;
    }
    let component = digits.parse::<usize>().ok()?;
    (component > 0 && component.to_string() == digits).then_some(component)
}

#[cfg(test)]
mod tests {
    use super::parse_component_name;

    #[test]
    fn component_names_are_positive_canonical_decimal_indices() {
        assert_eq!(parse_component_name("component1"), Some(1));
        assert_eq!(parse_component_name("component42"), Some(42));
        for invalid in ["component", "component0", "component01", "componentA", "x1"] {
            assert_eq!(parse_component_name(invalid), None, "{invalid}");
        }
    }
}
