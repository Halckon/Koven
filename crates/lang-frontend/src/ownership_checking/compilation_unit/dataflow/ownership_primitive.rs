//! source-qualified root 与独立一级字段 commit 从已检查的普通 call 契约产生。
use super::{Checker, OwnershipCheckingError, State, UnitOwnershipDeferredFact};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        OwnershipDeferredReason, OwnershipPrimitiveValueTransfer,
        UnitOwnershipPrimitiveOwnershipPlan, UnitValueDeliveryKind,
    },
    parser::{CallArgument, VariableKind},
    type_checking::{Copyability, OwnershipPrimitiveKind, UnitTypeId, UnitTypeKind},
};

impl Checker<'_> {
    pub(super) fn record_ownership_primitive(
        &mut self,
        id: ExpressionId,
        arguments: &[CallArgument],
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let call = self.unit_expression(id);
        let Some(descriptor) = self.typed.ownership_primitive(call) else {
            return Ok(());
        };
        if arguments.len() != 2
            || descriptor.operands()
                != arguments
                    .iter()
                    .map(|argument| self.unit_expression(argument.value))
                    .collect::<Vec<_>>()
                    .as_slice()
        {
            return Err(OwnershipCheckingError::InvalidUnitCall {
                source_unit: self.source_unit.index(),
                expression: id.index(),
            });
        }
        if self.primitive_type_has_closure(descriptor.value_type()) {
            self.primitive_deferred.push(UnitOwnershipDeferredFact::new(
                call,
                OwnershipDeferredReason::OwnershipPrimitiveClosureTransport,
            ));
            return Ok(());
        }
        if !matches!(
            self.typed.copyability(descriptor.value_type()),
            Copyability::Copyable | Copyability::MoveOnly
        ) || !self.primitive_type_is_concrete(descriptor.value_type())
        {
            return Ok(());
        }
        let count = if descriptor.kind() == OwnershipPrimitiveKind::Replace {
            1
        } else {
            2
        };
        if descriptor.kind() == OwnershipPrimitiveKind::Replace
            && let Some((place, owner_type)) =
                self.direct_field_replace_target(arguments[0].value, descriptor.value_type())?
        {
            if state.immutable_captures.contains_key(&place.root()) {
                self.primitive_deferred.push(UnitOwnershipDeferredFact::new(
                    call,
                    OwnershipDeferredReason::OwnershipPrimitiveClosureTransport,
                ));
                return Ok(());
            }
            let transfer = self.primitive_new_value_transfer(call, descriptor.operands()[1])?;
            self.field_replacements.push(
                crate::ownership_checking::UnitFieldReplaceOwnershipPlan {
                    descriptor,
                    place,
                    owner_type,
                    new_value_transfer: transfer,
                },
            );
            return Ok(());
        }
        let mut places = Vec::new();
        for argument in &arguments[..count] {
            let Some(place) = self.place(argument.value)? else {
                return Ok(());
            };
            if !place.is_root()
                || self.variable_kinds.get(&place.root()) != Some(&VariableKind::Var)
                || self.bindings.contains_key(&place.root())
            {
                return Ok(());
            }
            places.push(place);
        }
        let new_value_transfer = if descriptor.kind() == OwnershipPrimitiveKind::Replace {
            Some(self.primitive_new_value_transfer(call, descriptor.operands()[1])?)
        } else {
            None
        };
        self.ownership_primitives
            .push(UnitOwnershipPrimitiveOwnershipPlan {
                descriptor,
                places,
                new_value_transfer,
            });
        Ok(())
    }

    fn primitive_new_value_transfer(
        &self,
        call: crate::type_checking::UnitExpressionId,
        argument: crate::type_checking::UnitExpressionId,
    ) -> Result<OwnershipPrimitiveValueTransfer, OwnershipCheckingError> {
        let delivery = self
            .value_deliveries
            .iter()
            .rev()
            .find(|delivery| delivery.call() == call && delivery.argument() == argument)
            .ok_or(OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: argument.expression().index(),
            })?;
        Ok(match delivery.kind() {
            UnitValueDeliveryKind::Copy => OwnershipPrimitiveValueTransfer::Copy,
            UnitValueDeliveryKind::Move => OwnershipPrimitiveValueTransfer::Move,
            UnitValueDeliveryKind::Temporary => OwnershipPrimitiveValueTransfer::Temporary,
        })
    }

    fn direct_field_replace_target(
        &self,
        mut argument: ExpressionId,
        value_type: UnitTypeId,
    ) -> Result<
        Option<(crate::ownership_checking::UnitOwnershipPlace, UnitTypeId)>,
        OwnershipCheckingError,
    > {
        use crate::{
            parser::Expression,
            type_checking::{
                NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
            },
        };
        let Some(place) = self.place(argument)? else {
            return Ok(None);
        };
        if place.fields().len() != 1
            || place.element().is_some()
            || !self.variable_kinds.contains_key(&place.root())
            || !self
                .names
                .names()
                .source_units()
                .get(place.root().source_unit().index())
                .is_some_and(|source| {
                    crate::ownership_checking::ownership_primitive::is_local_variable(
                        source.resolution(),
                        place.root().symbol(),
                    )
                })
            || self.bindings.contains_key(&place.root())
            || self.field_kinds.get(&place.fields()[0]) != Some(&VariableKind::Var)
        {
            return Ok(None);
        }
        while let Expression::Group { expression } =
            self.parsed.ast().expressions().get(argument)?.payload()
        {
            argument = *expression;
        }
        let Some(projection) = self
            .typed
            .aggregate_projection(self.unit_expression(argument))
        else {
            return Ok(None);
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        let Some(owner_type) = self.typed.symbol_type(place.root()) else {
            return Ok(None);
        };
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(owner_type)
        else {
            return Ok(None);
        };
        let Some(owner) = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|owner| owner.nominal())
        else {
            return Ok(None);
        };
        if !arguments.is_empty()
            || owner.kind() != NominalKind::Class
            || !owner
                .fields()
                .iter()
                .any(|field| field.symbol() == projection.field() && field.ty() == value_type)
            || projection.field() != place.fields()[0]
            || projection.ty() != value_type
            || projection.kind() != UnitAggregateProjectionKind::Field
            || self.typed.expression_type(receiver) != Some(owner_type)
            || receiver.source_unit() != self.source_unit
            || self.primitive_type_has_closure(owner_type)
            || !self
                .place(receiver.expression())?
                .is_some_and(|receiver| receiver.is_root() && receiver.root() == place.root())
        {
            return Ok(None);
        }
        Ok(Some((place, owner_type)))
    }

    // Only actual type arguments decide whether a generic instance is concrete.
    fn primitive_type_is_concrete(&self, root: UnitTypeId) -> bool {
        let mut pending = vec![root];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match self.typed.types().get(ty) {
                Some(UnitTypeKind::Builtin(_)) => {}
                Some(UnitTypeKind::Nullable(inner)) => pending.push(*inner),
                Some(UnitTypeKind::EnumCase { root, .. }) => pending.push(*root),
                Some(
                    UnitTypeKind::Nominal { arguments, .. }
                    | UnitTypeKind::Intrinsic { arguments, .. },
                ) => pending.extend(arguments),
                _ => return false,
            }
        }
        true
    }

    fn primitive_type_has_closure(&self, root: UnitTypeId) -> bool {
        let mut pending = vec![root];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match self.typed.types().get(ty) {
                Some(UnitTypeKind::Function { .. }) => return true,
                Some(UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner)) => {
                    pending.push(*inner)
                }
                Some(UnitTypeKind::EnumCase { root, .. }) => pending.push(*root),
                Some(UnitTypeKind::Intrinsic { arguments, .. }) => pending.extend(arguments),
                Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) => {
                    pending.extend(arguments);
                    if let Some(nominal) = self
                        .typed
                        .signatures()
                        .declaration(*declaration)
                        .and_then(|declaration| declaration.nominal())
                    {
                        pending.extend(nominal.fields().iter().map(|field| field.ty()));
                        pending.extend(
                            nominal
                                .enum_cases()
                                .iter()
                                .flat_map(|case| case.payloads().iter().map(|field| field.ty())),
                        );
                    }
                }
                _ => {}
            }
        }
        false
    }
}
