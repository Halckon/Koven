//! Map container SSA operation lowering to LLVM IR.

use inkwell::{
    IntPredicate,
    values::{BasicValue, BasicValueEnum, IntValue, StructValue},
};

use super::super::map;
use super::{
    FunctionLowerer, LlvmAdapterError, invalid_result_count, value_name, value_results, value_type,
};
use crate::llvm::entities::access_type;
use crate::ssa::model::{
    ComparisonOperator, EntityId, Instruction, Operation, PlaceAccess, SsaTypeId, ValueId,
};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn nullable_is_null(
        &self,
        owner: ValueId,
        name: &str,
    ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        if self
            .module
            .map_result_value(value_type(self.function, owner)?)
            .is_some()
        {
            let present = self
                .builder
                .build_extract_value(self.struct_value(owner)?, 0, "map.present")?
                .into_int_value();
            Ok(self.builder.build_not(present, name)?)
        } else {
            Ok(self
                .builder
                .build_is_null(self.pointer_value(owner)?, name)?)
        }
    }

    pub(super) fn nullable_view_pointer(
        &self,
        owner: ValueId,
    ) -> Result<inkwell::values::PointerValue<'ctx>, LlvmAdapterError> {
        let ty = value_type(self.function, owner)?;
        if let Some(inner) = self.module.map_result_value(ty) {
            // Stack storage backs the non-owning payload view for the existing branch proof.
            // The proof aliases the SSA owner; only NullableTake can transfer that payload.
            let payload =
                self.builder
                    .build_extract_value(self.struct_value(owner)?, 1, "map.payload")?;
            // One stack slot per SSA branch, including when that branch is inside a loop.
            let entry = self.llvm_function.get_first_basic_block().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("nullable view requires function entry".into())
            })?;
            let allocator = self.context.create_builder();
            if let Some(instruction) = entry.get_first_instruction() {
                allocator.position_before(&instruction);
            } else {
                allocator.position_at_end(entry);
            }
            let pointer = allocator.build_alloca(
                self.dependencies.type_map.basic_type(inner)?,
                "map.payload.view",
            )?;
            self.builder.build_store(pointer, payload)?;
            Ok(pointer)
        } else {
            self.pointer_value(owner)
        }
    }
    pub(super) fn lower_map_with_operation(
        &mut self,
        instruction: &Instruction,
    ) -> Result<(), LlvmAdapterError> {
        let Operation::MapWithValue {
            source,
            key,
            action,
        } = instruction.operation
        else {
            return Err(LlvmAdapterError::InvalidSsa("expected scoped query".into()));
        };
        let [EntityId::Value(result)] = instruction.results.as_slice() else {
            return Err(LlvmAdapterError::InvalidSsa(
                "scoped query must return Boolean".into(),
            ));
        };
        let map_type = self.resolve_map_type(EntityId::Loan(source))?;
        let header = self
            .builder
            .build_load(
                self.dependencies.type_map.basic_type(map_type)?,
                self.access(PlaceAccess::Loan(source))?,
                "with.map",
            )?
            .into_struct_value();
        let action_type = access_type(self.function, PlaceAccess::Loan(action))?;
        let action = self.builder.build_load(
            self.dependencies.type_map.basic_type(action_type)?,
            self.access(PlaceAccess::Loan(action))?,
            "with.action",
        )?;
        let key = self.resolve_map_key(key)?;
        let value = map::with_value(
            self.context,
            self.llvm,
            &self.builder,
            self.llvm_function,
            self.dependencies.type_map,
            self.dependencies.runtime,
            self.module,
            map_type,
            header,
            key,
            action_type,
            action,
            &value_name(*result),
        )?;
        self.values.insert(*result, value.into());
        Ok(())
    }
    pub(super) fn lower_map_require_operation(
        &mut self,
        instruction: &Instruction,
    ) -> Result<(), LlvmAdapterError> {
        let Operation::MapRequireValue { source, key } = instruction.operation else {
            return Err(LlvmAdapterError::InvalidSsa(
                "expected required Map loan".into(),
            ));
        };
        let [EntityId::Loan(result)] = instruction.results.as_slice() else {
            return Err(LlvmAdapterError::InvalidSsa(
                "required Map query must return one loan".into(),
            ));
        };
        let map_type = self.resolve_map_type(EntityId::Loan(source))?;
        let pointer = self.access(PlaceAccess::Loan(source))?;
        let header = self
            .builder
            .build_load(
                self.dependencies.type_map.basic_type(map_type)?,
                pointer,
                "required.map",
            )?
            .into_struct_value();
        let key = self.resolve_map_key(key)?;
        let pointer = map::require_value(
            self.context,
            self.llvm,
            &self.builder,
            self.llvm_function,
            self.dependencies.type_map,
            self.dependencies.runtime,
            self.module,
            map_type,
            header,
            key,
            &format!("l{}.required", result.index()),
        )?;
        self.loans.insert(*result, pointer);
        Ok(())
    }

    pub(super) fn lower_map_operation(
        &mut self,
        instruction: &Instruction,
    ) -> Result<(), LlvmAdapterError> {
        let results = value_results(instruction)?;
        match &instruction.operation {
            Operation::MapConstruct { map_type } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("map construct", 1, results.len()));
                };
                let value = map::construct(
                    self.context,
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    self.module,
                    *map_type,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::MapSize { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("map size", 1, results.len()));
                };
                let owner_header = self.resolve_map_owner(*owner, *result)?;
                let value = map::size(&self.builder, owner_header, &value_name(*result))?;
                self.values.insert(*result, value.into());
            }
            Operation::MapContains { owner, key } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("map contains", 1, results.len()));
                };
                let map_ty = self.resolve_map_type(*owner)?;
                let owner_header = self.resolve_map_owner(*owner, *result)?;
                let key_val = self.resolve_map_key(*key)?;
                let value = map::contains(
                    self.context,
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    self.module,
                    map_ty,
                    owner_header,
                    key_val,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::MapGet { owner, key } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("map get", 1, results.len()));
                };
                let map_ty = self.resolve_map_type(*owner)?;
                let owner_header = self.resolve_map_owner(*owner, *result)?;
                let key_val = self.resolve_map_key(*key)?;
                let (present, value) = map::get(
                    self.context,
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    self.module,
                    map_ty,
                    owner_header,
                    key_val,
                    &value_name(*result),
                )?;
                let value = self.build_map_result(*result, present, value)?;
                self.values.insert(*result, value);
            }
            Operation::MapResultUnwrap { result: operand } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("map result unwrap", 1, results.len()));
                };
                let header = self.struct_value(*operand)?;
                let present = self
                    .builder
                    .build_extract_value(header, 0, "map.result.present")?
                    .into_int_value();
                let found = self
                    .context
                    .append_basic_block(self.llvm_function, "map.result.found");
                let absent = self
                    .context
                    .append_basic_block(self.llvm_function, "map.result.absent");
                self.builder
                    .build_conditional_branch(present, found, absent)?;
                self.builder.position_at_end(absent);
                self.dependencies.runtime.emit_abort(&self.builder)?;
                self.builder.position_at_end(found);
                let value = self
                    .builder
                    .build_extract_value(header, 1, &value_name(*result))?;
                self.values.insert(*result, value);
            }
            Operation::MapPut { owner, key, value } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("map put", 1, results.len()));
                };
                let map_ty = value_type(self.function, *owner)?;
                let owner_header = self.struct_value(*owner)?;
                let key_val = self.value(*key)?;
                let val_val = self.value(*value)?;
                let new_owner = map::put(
                    self.context,
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    self.module,
                    map_ty,
                    owner_header,
                    key_val,
                    val_val,
                    &value_name(*result),
                )?;
                self.values.insert(*result, new_owner.into());
            }
            Operation::MapRemove { owner, key } => {
                let [result, removed] = results.as_slice() else {
                    return Err(invalid_result_count("map remove", 2, results.len()));
                };
                let map_ty = value_type(self.function, *owner)?;
                let owner_header = self.struct_value(*owner)?;
                let key_val = self.resolve_map_key(*key)?;
                let (new_owner, present, removed_value) = map::remove(
                    self.context,
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    self.module,
                    map_ty,
                    owner_header,
                    key_val,
                    &value_name(*result),
                )?;
                self.values.insert(*result, new_owner.into());
                let removed_value = if self
                    .module
                    .map_result_value(value_type(self.function, *removed)?)
                    .is_some()
                {
                    self.build_map_result(*removed, present, removed_value)?
                } else {
                    removed_value
                };
                self.values.insert(*removed, removed_value);
            }
            _ => unreachable!("not a map operation"),
        }
        Ok(())
    }

    pub(super) fn build_map_result(
        &self,
        result: ValueId,
        present: IntValue<'ctx>,
        payload: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, LlvmAdapterError> {
        let ty = value_type(self.function, result)?;
        let header = self.dependencies.type_map.aggregate_type(ty)?.const_zero();
        let header = self
            .builder
            .build_insert_value(header, present, 0, "map.result.tag")?;
        Ok(self
            .builder
            .build_insert_value(header, payload, 1, &value_name(result))?
            .as_basic_value_enum())
    }

    pub(super) fn lower_map_result_comparison(
        &mut self,
        operator: ComparisonOperator,
        left: ValueId,
        right: ValueId,
        result: ValueId,
    ) -> Result<(), LlvmAdapterError> {
        let left = self.struct_value(left)?;
        let right = self.struct_value(right)?;
        let lp = self
            .builder
            .build_extract_value(left, 0, "left.present")?
            .into_int_value();
        let rp = self
            .builder
            .build_extract_value(right, 0, "right.present")?
            .into_int_value();
        let lv = self
            .builder
            .build_extract_value(left, 1, "left.payload")?
            .into_int_value();
        let rv = self
            .builder
            .build_extract_value(right, 1, "right.payload")?
            .into_int_value();
        let payload_equal =
            self.builder
                .build_int_compare(IntPredicate::EQ, lv, rv, "payload.equal")?;
        let both_present = self.builder.build_and(lp, rp, "both.present")?;
        let present_equal = self
            .builder
            .build_and(both_present, payload_equal, "present.equal")?;
        let either_present = self.builder.build_or(lp, rp, "either.present")?;
        let both_absent = self.builder.build_not(either_present, "both.absent")?;
        let equal = self
            .builder
            .build_or(present_equal, both_absent, "lookup.equal")?;
        let compared = if operator == ComparisonOperator::NotEqual {
            self.builder.build_not(equal, &value_name(result))?
        } else {
            equal
        };
        self.values.insert(result, compared.into());
        Ok(())
    }

    fn resolve_map_type(&self, owner: EntityId) -> Result<SsaTypeId, LlvmAdapterError> {
        let ty = self
            .function
            .entity(owner)
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("map owner entity is missing".to_owned()))?
            .ty
            .semantic_type();
        Ok(ty)
    }

    fn resolve_map_key(&mut self, key: EntityId) -> Result<BasicValueEnum<'ctx>, LlvmAdapterError> {
        match key {
            EntityId::Value(value) => self.value(value),
            EntityId::Loan(loan) => {
                let ty = self.resolve_map_type(key)?;
                let pointer = self.access(PlaceAccess::Loan(loan))?;
                Ok(self.builder.build_load(
                    self.dependencies.type_map.basic_type(ty)?,
                    pointer,
                    "map.key",
                )?)
            }
            EntityId::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                "map key requires a value or shared loan".to_owned(),
            )),
        }
    }

    fn resolve_map_owner(
        &mut self,
        owner: EntityId,
        result: ValueId,
    ) -> Result<StructValue<'ctx>, LlvmAdapterError> {
        match owner {
            EntityId::Value(owner) => self.struct_value(owner),
            EntityId::Loan(loan) => {
                let map_type = self.resolve_map_type(owner)?;
                let llvm_ty = self.dependencies.type_map.basic_type(map_type)?;
                let ptr = self.access(PlaceAccess::Loan(loan))?;
                let loaded =
                    self.builder
                        .build_load(llvm_ty, ptr, &format!("v{}.map", result.index()))?;
                Ok(loaded.into_struct_value())
            }
            EntityId::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                "map owner cannot be a place".to_owned(),
            )),
        }
    }
}
