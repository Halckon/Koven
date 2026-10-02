//! source-qualified root commit 从已检查的普通 call 契约产生。
use super::{Checker, OwnershipCheckingError, UnitOwnershipDeferredFact};
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
            let delivery = self
                .value_deliveries
                .iter()
                .rev()
                .find(|delivery| {
                    delivery.call() == call && delivery.argument() == descriptor.operands()[1]
                })
                .ok_or(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: self.source_unit.index(),
                    expression: arguments[1].value.index(),
                })?;
            Some(match delivery.kind() {
                UnitValueDeliveryKind::Copy => OwnershipPrimitiveValueTransfer::Copy,
                UnitValueDeliveryKind::Move => OwnershipPrimitiveValueTransfer::Move,
                UnitValueDeliveryKind::Temporary => OwnershipPrimitiveValueTransfer::Temporary,
            })
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
