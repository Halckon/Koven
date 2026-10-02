//! 普通 call 检查成功后，仅登记已闭合的 owned mutable root commit。
use super::{Checker, OwnershipCheckingError};
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
            Some(
                if self.typed.expression_category(arguments[1])
                    == Some(ExpressionCategory::Temporary)
                {
                    OwnershipPrimitiveValueTransfer::Temporary
                } else if self.typed.copyability(descriptor.value_type())
                    == Some(Copyability::Copyable)
                {
                    OwnershipPrimitiveValueTransfer::Copy
                } else if self.place(arguments[1])?.is_some() {
                    OwnershipPrimitiveValueTransfer::Move
                } else {
                    OwnershipPrimitiveValueTransfer::Temporary
                },
            )
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
