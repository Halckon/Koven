//! Heap, shared-owner and field-exchange instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction_heap(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::HeapAllocate { owner, payload } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("heap allocate", 1, results.len()));
                };
                let allocation = self.dependencies.runtime.allocate(
                    &self.builder,
                    self.llvm_function,
                    *owner,
                    self.value(*payload)?,
                    &value_name(*result),
                )?;
                allocation.set_name(&value_name(*result));
                self.values.insert(*result, allocation.into());
            }
            Operation::HeapPayloadPlace { owner } => {
                let result = place_result(instruction)?;
                self.places.insert(result, self.pointer_value(*owner)?);
            }
            Operation::HeapFieldRead { receiver, field } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("heap field read", 1, results.len()));
                };
                let result_type = value_type(self.function, *result)?;
                let value = self.builder.build_load(
                    self.dependencies.type_map.basic_type(result_type)?,
                    self.heap_field_pointer(
                        *receiver,
                        *field,
                        &format!("v{}.field", result.index()),
                    )?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value);
            }
            Operation::HeapFieldReplace {
                receiver,
                field,
                value,
            } => {
                let field_type = value_type(self.function, *value)?;
                let pointer = self.heap_field_pointer(
                    *receiver,
                    *field,
                    &format!("replace.i{}", instruction.id.index()),
                )?;
                if self.module.type_ownership(field_type) == Some(Ownership::MoveOnly) {
                    let old = self.builder.build_load(
                        self.dependencies.type_map.basic_type(field_type)?,
                        pointer,
                        &format!("replace.i{}.old", instruction.id.index()),
                    )?;
                    self.dependencies
                        .runtime
                        .emit_drop(&self.builder, field_type, old)?;
                }
                self.builder.build_store(pointer, self.value(*value)?)?;
            }
            Operation::HeapFieldExchange {
                loan, replacement, ..
            } => {
                self.lower_heap_field_exchange(*loan, *replacement, &results)?;
            }
            Operation::InlineFieldReplace {
                receiver,
                field,
                value,
            } => {
                let EntityType::Loan { target, .. } = self
                    .function
                    .entity(EntityId::Loan(*receiver))
                    .ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa(
                            "inline field replace receiver is missing".to_owned(),
                        )
                    })?
                    .ty
                else {
                    return Err(LlvmAdapterError::InvalidSsa(
                        "inline field replace receiver is not a loan".to_owned(),
                    ));
                };
                let pointer = self.builder.build_struct_gep(
                    self.dependencies.type_map.aggregate_type(target)?,
                    self.access(PlaceAccess::Loan(*receiver))?,
                    *field as u32,
                    &format!("inline.replace.i{}", instruction.id.index()),
                )?;
                let field_type = value_type(self.function, *value)?;
                if self.module.type_ownership(field_type) == Some(Ownership::MoveOnly) {
                    let old = self.builder.build_load(
                        self.dependencies.type_map.basic_type(field_type)?,
                        pointer,
                        &format!("inline.replace.i{}.old", instruction.id.index()),
                    )?;
                    self.dependencies
                        .runtime
                        .emit_drop(&self.builder, field_type, old)?;
                }
                self.builder.build_store(pointer, self.value(*value)?)?;
            }
            Operation::SharedAllocate { owner, payload } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("shared allocate", 1, results.len()));
                };
                let allocation = self.dependencies.runtime.allocate_shared(
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    *owner,
                    self.value(*payload)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, allocation.into());
            }
            Operation::SharedRetain { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("shared retain", 1, results.len()));
                };
                let owner_type = self.shared_owner_type(*owner)?;
                let retained = self.dependencies.runtime.retain_shared(
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    owner_type,
                    self.shared_owner_pointer(*owner)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, retained.into());
            }
            Operation::SharedPayloadPlace { owner } => {
                let result = place_result(instruction)?;
                let owner_type = self.shared_owner_type(*owner)?;
                let place = self.dependencies.runtime.shared_payload_place(
                    &self.builder,
                    self.dependencies.type_map,
                    owner_type,
                    self.shared_owner_pointer(*owner)?,
                    &format!("p{}", result.index()),
                )?;
                self.places.insert(result, place);
            }
            Operation::SharedPayloadLoan { source } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    return Err(invalid_result_count(
                        "shared payload loan",
                        1,
                        instruction.results.len(),
                    ));
                };
                let owner = EntityId::Loan(*source);
                let place = self.dependencies.runtime.shared_payload_place(
                    &self.builder,
                    self.dependencies.type_map,
                    self.shared_owner_type(owner)?,
                    self.shared_owner_pointer(owner)?,
                    &format!("l{}", result.index()),
                )?;
                self.loans.insert(*result, place);
            }
            Operation::SharedReferenceFollow { source } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    return Err(invalid_result_count(
                        "shared reference follow",
                        1,
                        instruction.results.len(),
                    ));
                };
                let pointer = self.builder.build_load(
                    self.context.ptr_type(inkwell::AddressSpace::default()),
                    self.access(crate::ssa::model::PlaceAccess::Loan(*source))?,
                    &format!("l{}", result.index()),
                )?;
                self.loans.insert(*result, pointer.into_pointer_value());
            }
            _ => {
                return Err(unsupported(
                    "internal dispatch invariant violated: operation belongs to another lowering family",
                ));
            }
        }
        Ok(())
    }
}
