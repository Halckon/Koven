//! Local type contracts for Map container operations.

use super::{is_koven_int, single_value_result, value_type};
use crate::ssa::model::{
    EntityId, EntityType, Function, LoanKind, MapContainerKind, Module, Operation, Ownership,
    SsaTypeId, SsaTypeKind,
};

#[cfg(test)]
mod require_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod with_tests;

fn read_type(function: &Function, entity: EntityId) -> Option<SsaTypeId> {
    match function.entity(entity)?.ty {
        EntityType::Value(ty)
        | EntityType::Loan {
            kind: LoanKind::Shared,
            target: ty,
        } => Some(ty),
        _ => None,
    }
}

pub(super) fn verify_map_operation(
    module: &Module,
    function: &Function,
    operation: &Operation,
    results: &[EntityType],
) -> bool {
    match operation {
        Operation::MapConstruct { map_type } => {
            module.map_container(*map_type).is_some()
                && single_value_result(results) == Some(*map_type)
        }
        Operation::MapSize { owner } => {
            let owner_type = match function.entity(*owner).map(|e| e.ty) {
                Some(EntityType::Value(ty))
                | Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }) => Some(ty),
                _ => None,
            };
            owner_type.and_then(|ty| module.map_container(ty)).is_some()
                && single_value_result(results).is_some_and(|ty| is_koven_int(module, ty))
        }
        Operation::MapContains { owner, key } => {
            let owner_type = match function.entity(*owner).map(|e| e.ty) {
                Some(EntityType::Value(ty))
                | Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }) => Some(ty),
                _ => None,
            };
            let Some((_, key_ty, _)) = owner_type.and_then(|ty| module.map_container(ty)) else {
                return false;
            };
            read_type(function, *key) == Some(key_ty)
                && single_value_result(results)
                    .is_some_and(|ty| matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean)))
        }
        Operation::MapRequireValue { source, key } => {
            let Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }) = function.entity(EntityId::Loan(*source)).map(|data| data.ty)
            else {
                return false;
            };
            let Some((_, key_type, value_type)) = module.map_container(target) else {
                return false;
            };
            read_type(function, *key) == Some(key_type)
                && results
                    == [EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: value_type,
                    }]
        }
        Operation::MapWithValue {
            source,
            key,
            action,
        } => {
            let Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            }) = function.entity(EntityId::Loan(*source)).map(|data| data.ty)
            else {
                return false;
            };
            let Some((_, key_type, value_type)) = module.map_container(target) else {
                return false;
            };
            let Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target: action_type,
            }) = function.entity(EntityId::Loan(*action)).map(|data| data.ty)
            else {
                return false;
            };
            let Some(signature) = module.callable_signature(action_type) else {
                return false;
            };
            read_type(function, *key) == Some(key_type)
                && signature.parameters
                    == [EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: value_type,
                    }]
                && signature.returns.is_empty()
                && single_value_result(results)
                    .is_some_and(|ty| matches!(module.type_kind(ty), Some(SsaTypeKind::Boolean)))
        }
        Operation::MapGet { owner, key } => {
            let owner_type = match function.entity(*owner).map(|e| e.ty) {
                Some(EntityType::Value(ty))
                | Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }) => Some(ty),
                _ => None,
            };
            let Some((_, key_ty, val_ty)) = owner_type.and_then(|ty| module.map_container(ty))
            else {
                return false;
            };
            let Some(res_ty) = single_value_result(results) else {
                return false;
            };
            read_type(function, *key) == Some(key_ty)
                && module.type_ownership(val_ty) == Some(Ownership::Copyable)
                && module.map_result_value(res_ty) == Some(val_ty)
        }
        Operation::MapResultUnwrap { result } => {
            value_type(function, *result).and_then(|ty| module.map_result_value(ty))
                == single_value_result(results)
                && single_value_result(results)
                    .is_some_and(|ty| module.type_ownership(ty) == Some(Ownership::Copyable))
        }
        Operation::MapPut { owner, key, value } => {
            let Some((kind, key_ty, val_ty)) =
                value_type(function, *owner).and_then(|ty| module.map_container(ty))
            else {
                return false;
            };
            kind == MapContainerKind::MutableMap
                && value_type(function, *key) == Some(key_ty)
                && value_type(function, *value) == Some(val_ty)
                && single_value_result(results) == value_type(function, *owner)
        }
        Operation::MapRemove { owner, key } => {
            let Some((kind, key_ty, val_ty)) =
                value_type(function, *owner).and_then(|ty| module.map_container(ty))
            else {
                return false;
            };
            if kind != MapContainerKind::MutableMap || read_type(function, *key) != Some(key_ty) {
                return false;
            }
            let owner_ty = value_type(function, *owner).unwrap();
            if results.len() == 2 {
                results[0] == EntityType::Value(owner_ty)
                    && matches!(results[1], EntityType::Value(res_ty) if module.map_result_value(res_ty) == Some(val_ty) || matches!(module.type_kind(res_ty), Some(SsaTypeKind::NullableHandle { inner }) if *inner == val_ty))
            } else {
                false
            }
        }
        _ => false,
    }
}
