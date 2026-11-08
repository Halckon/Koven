use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    source::Span,
    type_checking::{AggregateProjectionDescriptor, AggregateProjectionKind},
};

use super::*;

impl Checker<'_> {
    pub(super) fn check_field_projection(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_type: TypeId,
        name_span: Span,
        safe: bool,
    ) -> Result<Option<TypeId>, TypeCheckingError> {
        if !safe && let TypeKind::EnumCase { case, root } = self.kind(receiver_type).clone() {
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
        let owner_type = match self.kind(receiver_type) {
            TypeKind::Nullable(inner) => *inner,
            _ => receiver_type,
        };
        let Some((owner, arguments)) = self.nominal_instance(owner_type) else {
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
        if self.private_field_inaccessible(field, name_span) {
            self.emit_private_field_access(field, name_span)?;
            return Ok(Some(self.error_type()));
        }
        if safe || owner_type != receiver_type {
            return Ok(None);
        }
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

    pub(super) fn private_field_inaccessible(&self, field: SymbolId, access: Span) -> bool {
        // Extension receivers do not grant the lexical classifier's private access.
        self.private_field_owners.get(&field).is_some_and(|owner| {
            owner.source_id() != access.source_id()
                || access.start() < owner.start()
                || access.end() > owner.end()
        })
    }

    pub(super) fn emit_private_field_access(
        &mut self,
        field: SymbolId,
        access: Span,
    ) -> Result<(), TypeCheckingError> {
        self.emit_with_label(
            self.unresolved_name_code,
            "member is private to its declaring classifier",
            access,
            self.symbol_spans[field.index()],
            "private member declared here",
        )
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
