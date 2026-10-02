//! 普通 call 成功后分别登记 owned mutable root 与 owned class 一级字段 commit。
use super::{Checker, OwnershipCheckingError, State};
use crate::{
    ast::ExpressionId,
    ownership_checking::{
        OwnershipDeferredReason, OwnershipPrimitiveOwnershipPlan, OwnershipPrimitiveValueTransfer,
    },
    parser::VariableKind,
    type_checking::{Copyability, ExpressionCategory, OwnershipPrimitiveKind, TypeId, TypeKind},
};

impl Checker<'_> {
    pub(super) fn record_ownership_primitive(
        &mut self,
        id: ExpressionId,
        arguments: &[ExpressionId],
        state: &State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(descriptor) = self.typed.ownership_primitive(id) else {
            return Ok(());
        };
        if descriptor.operands() != arguments {
            return Err(OwnershipCheckingError::InvalidOwnershipPrimitive {
                expression: id.index(),
            });
        }
        if self.primitive_type_has_closure(descriptor.value_type()) {
            self.defer(
                id,
                OwnershipDeferredReason::OwnershipPrimitiveClosureTransport,
            );
            return Ok(());
        }
        if !matches!(
            self.typed.copyability(descriptor.value_type()),
            Some(Copyability::Copyable | Copyability::MoveOnly)
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
                self.direct_field_replace_target(arguments[0], descriptor.value_type())?
        {
            if state.immutable_captures.contains_key(&place.root()) {
                self.defer(
                    id,
                    OwnershipDeferredReason::OwnershipPrimitiveClosureTransport,
                );
                return Ok(());
            }
            let transfer =
                self.primitive_new_value_transfer(arguments[1], descriptor.value_type())?;
            self.field_replacements.insert(
                id.index(),
                crate::ownership_checking::FieldReplaceOwnershipPlan {
                    descriptor,
                    place,
                    owner_type,
                    new_value_transfer: transfer,
                },
            );
            return Ok(());
        }
        let mut places = Vec::new();
        for &argument in &arguments[..count] {
            let Some(place) = self.place(argument)? else {
                return Ok(());
            };
            if !place.is_root()
                || self.variable_kinds.get(&place.root()) != Some(&VariableKind::Var)
                || self.typed.parameter_mode(place.root()).is_some()
            {
                return Ok(());
            }
            places.push(place);
        }
        let new_value_transfer = if descriptor.kind() == OwnershipPrimitiveKind::Replace {
            Some(self.primitive_new_value_transfer(arguments[1], descriptor.value_type())?)
        } else {
            None
        };
        self.ownership_primitives.insert(
            id.index(),
            OwnershipPrimitiveOwnershipPlan {
                descriptor,
                places,
                new_value_transfer,
            },
        );
        Ok(())
    }

    fn primitive_new_value_transfer(
        &self,
        argument: ExpressionId,
        ty: TypeId,
    ) -> Result<OwnershipPrimitiveValueTransfer, OwnershipCheckingError> {
        Ok(
            if self.typed.expression_category(argument) == Some(ExpressionCategory::Temporary) {
                OwnershipPrimitiveValueTransfer::Temporary
            } else if self.typed.copyability(ty) == Some(Copyability::Copyable) {
                OwnershipPrimitiveValueTransfer::Copy
            } else if self.place(argument)?.is_some() {
                OwnershipPrimitiveValueTransfer::Move
            } else {
                OwnershipPrimitiveValueTransfer::Temporary
            },
        )
    }

    fn direct_field_replace_target(
        &self,
        mut argument: ExpressionId,
        value_type: TypeId,
    ) -> Result<Option<(crate::ownership_checking::OwnershipPlace, TypeId)>, OwnershipCheckingError>
    {
        use crate::{
            parser::Expression,
            type_checking::{AggregateProjectionKind, AggregateProjectionReceiver, NominalKind},
        };
        let Some(place) = self.place(argument)? else {
            return Ok(None);
        };
        if place.fields().len() != 1
            || !place.elements().is_empty()
            || !self.variable_kinds.contains_key(&place.root())
            || !crate::ownership_checking::ownership_primitive::is_local_variable(
                self.names,
                place.root(),
            )
            || self.typed.parameter_mode(place.root()).is_some()
            || self.field_kinds.get(&place.fields()[0]) != Some(&VariableKind::Var)
        {
            return Ok(None);
        }
        while let Expression::Group { expression } =
            self.parsed.ast().expressions().get(argument)?.payload()
        {
            argument = *expression;
        }
        let Some(projection) = self.typed.aggregate_projection(argument) else {
            return Ok(None);
        };
        let AggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        let Some(owner_type) = self.typed.symbol_type(place.root()) else {
            return Ok(None);
        };
        let Some(TypeKind::Nominal { nominal, arguments }) = self.typed.types().get(owner_type)
        else {
            return Ok(None);
        };
        let Some(owner) = self
            .typed
            .nominals()
            .iter()
            .find(|owner| owner.id() == *nominal)
        else {
            return Ok(None);
        };
        if !arguments.is_empty()
            || owner.kind() != NominalKind::Class
            || !owner.fields().contains(&projection.field())
            || projection.field() != place.fields()[0]
            || projection.ty() != value_type
            || projection.kind() != AggregateProjectionKind::Field
            || self.typed.expression_type(receiver) != Some(owner_type)
            || self.primitive_type_has_closure(owner_type)
            || !self
                .place(receiver)?
                .is_some_and(|receiver| receiver.is_root() && receiver.root() == place.root())
        {
            return Ok(None);
        }
        Ok(Some((place, owner_type)))
    }

    // Inspect actual instantiation arguments, not unsubstituted declaration fields.
    // A concrete Foo<Int> is supported even when its field signature names T.
    fn primitive_type_is_concrete(&self, root: TypeId) -> bool {
        let mut pending = vec![root];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match self.typed.types().get(ty) {
                Some(TypeKind::Builtin(_)) => {}
                Some(TypeKind::Nullable(inner)) => pending.push(*inner),
                Some(TypeKind::EnumCase { root, .. }) => pending.push(*root),
                Some(
                    TypeKind::Nominal { arguments, .. } | TypeKind::Intrinsic { arguments, .. },
                ) => pending.extend(arguments),
                _ => return false,
            }
        }
        true
    }

    fn primitive_type_has_closure(&self, root: TypeId) -> bool {
        let mut pending = vec![root];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match self.typed.types().get(ty) {
                Some(TypeKind::Function { .. }) => return true,
                Some(TypeKind::Nullable(inner) | TypeKind::StaticSelf(inner)) => {
                    pending.push(*inner)
                }
                Some(TypeKind::EnumCase { root, .. }) => pending.push(*root),
                Some(TypeKind::Intrinsic { arguments, .. }) => pending.extend(arguments),
                Some(TypeKind::Nominal { nominal, arguments }) => {
                    pending.extend(arguments);
                    if let Some(declaration) = self
                        .typed
                        .nominals()
                        .iter()
                        .find(|declaration| declaration.id() == *nominal)
                    {
                        pending.extend(
                            declaration
                                .fields()
                                .iter()
                                .filter_map(|&field| self.typed.symbol_type(field)),
                        );
                    }
                    for case in self
                        .typed
                        .enum_cases()
                        .iter()
                        .filter(|case| case.root() == *nominal)
                    {
                        pending.extend(case.payloads().iter().map(|(_, ty)| *ty));
                    }
                }
                _ => {}
            }
        }
        false
    }
}
