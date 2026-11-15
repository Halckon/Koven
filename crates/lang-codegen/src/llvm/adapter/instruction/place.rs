//! Place, loan, nullable, copy and drop instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction_place(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::FieldPlace { base, field } => {
                let result = place_result(instruction)?;
                let aggregate = place_type(self.function, *base)?;
                let pointer = self.builder.build_struct_gep(
                    self.dependencies.type_map.aggregate_type(aggregate)?,
                    self.place(*base)?,
                    *field as u32,
                    &format!("p{}", result.index()),
                )?;
                self.places.insert(result, pointer);
            }
            Operation::SharedFieldLoan { base, field } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    return Err(invalid_result_count(
                        "shared field loan",
                        1,
                        instruction.results.len(),
                    ));
                };
                let EntityType::Loan { target, .. } = self
                    .function
                    .entity(EntityId::Loan(*base))
                    .ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("shared field base is missing".to_owned())
                    })?
                    .ty
                else {
                    return Err(LlvmAdapterError::InvalidSsa(
                        "shared field base is not a loan".to_owned(),
                    ));
                };
                let pointer = self.builder.build_struct_gep(
                    self.dependencies.type_map.aggregate_type(target)?,
                    self.access(crate::ssa::model::PlaceAccess::Loan(*base))?,
                    *field as u32,
                    &format!("l{}", result.index()),
                )?;
                self.loans.insert(*result, pointer);
            }
            Operation::SharedHeapFieldLoan { base, field } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    return Err(invalid_result_count(
                        "shared heap field loan",
                        1,
                        instruction.results.len(),
                    ));
                };
                let pointer =
                    self.heap_field_pointer(*base, *field, &format!("l{}", result.index()))?;
                self.loans.insert(*result, pointer);
            }
            Operation::SharedReborrow { source } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    return Err(invalid_result_count(
                        "shared reborrow",
                        1,
                        instruction.results.len(),
                    ));
                };
                let pointer = self.access(crate::ssa::model::PlaceAccess::Loan(*source))?;
                self.loans.insert(*result, pointer);
            }
            Operation::NullableWrap { owner, .. } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("nullable wrap", 1, results.len()));
                };
                let value = if let Some(value) = self.inline_nullable_wrap(*result, *owner)? {
                    value
                } else if self
                    .module
                    .presence_payload_type(value_type(self.function, *result)?)
                    .is_some()
                {
                    self.build_map_result(
                        *result,
                        self.context.bool_type().const_int(1, false),
                        self.value(*owner)?,
                    )?
                } else {
                    self.value(*owner)?
                };
                self.values.insert(*result, value);
            }
            Operation::NullableNull { .. } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("nullable null", 1, results.len()));
                };
                let ty = value_type(self.function, *result)?;
                let null = self.dependencies.type_map.basic_type(ty)?.const_zero();
                self.values.insert(*result, null);
            }
            Operation::NullableIsNull { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("nullable is-null", 1, results.len()));
                };
                let is_null = self.nullable_is_null(*owner, &value_name(*result))?;
                self.values.insert(*result, is_null.into());
            }
            Operation::NullableLoanIsNull { source } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count(
                        "nullable loan is-null",
                        1,
                        results.len(),
                    ));
                };
                let is_null = self.nullable_loan_is_null(*source, &value_name(*result))?;
                self.values.insert(*result, is_null.into());
            }
            Operation::NullableTake { owner, proof } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("nullable take", 1, results.len()));
                };
                self.loans.remove(proof).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa(
                        "nullable proof 的 LLVM loan 映射不存在".to_owned(),
                    )
                })?;
                let value = if let Some((_, payload)) = self.inline_nullable_payload(*owner)? {
                    payload
                } else if self
                    .module
                    .presence_payload_type(value_type(self.function, *owner)?)
                    .is_some()
                {
                    self.builder.build_extract_value(
                        self.struct_value(*owner)?,
                        1,
                        &value_name(*result),
                    )?
                } else {
                    self.value(*owner)?
                };
                self.values.insert(*result, value);
            }
            Operation::Copy { source } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("copy", 1, results.len()));
                };
                self.values.insert(*result, self.value(*source)?);
            }
            Operation::Consume { .. } => {
                if !results.is_empty() {
                    return Err(invalid_result_count("ownership effect", 0, results.len()));
                }
            }
            Operation::Drop { owner } => {
                self.dependencies.runtime.emit_drop(
                    &self.builder,
                    value_type(self.function, *owner)?,
                    self.value(*owner)?,
                )?;
            }
            Operation::RootPlace { owner } => {
                let result = place_result(instruction)?;
                let pointer = self.builder.build_alloca(
                    self.local_storage_type(value_type(self.function, *owner)?)?,
                    &format!("p{}", result.index()),
                )?;
                self.builder.build_store(pointer, self.value(*owner)?)?;
                self.places.insert(result, pointer);
            }
            Operation::RootPlaceTake { place, .. } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("root place take", 1, results.len()));
                };
                let result_type = value_type(self.function, *result)?;
                let value = self.builder.build_load(
                    self.dependencies.type_map.basic_type(result_type)?,
                    self.access(PlaceAccess::Place(*place))?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value);
            }
            Operation::RootReplace {
                owner,
                loan,
                replacement,
            } => self.lower_root_replace(*owner, *loan, *replacement, &results)?,
            Operation::RootSwap { owners, loans } => {
                self.lower_root_swap(*owners, *loans, &results)?;
            }
            Operation::BorrowBegin { place, .. } => {
                let result = loan_result(instruction)?;
                self.loans.insert(result, self.place(*place)?);
                if let Some(value) = self.zero_sized_places.get(place).copied() {
                    self.zero_sized_loans.insert(result, value);
                }
            }
            Operation::BorrowEnd { loan } => {
                self.loans.remove(loan).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("结束的 LLVM loan 映射不存在".to_owned())
                })?;
                self.zero_sized_loans.remove(loan);
            }
            Operation::Read { source } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("place read", 1, results.len()));
                };
                let zero_sized = match source {
                    PlaceAccess::Place(place) => self.zero_sized_places.get(place).copied(),
                    PlaceAccess::Loan(loan) => self.zero_sized_loans.get(loan).copied(),
                };
                let value = if let Some(value) = zero_sized {
                    value
                } else {
                    let source_type = access_type(self.function, *source)?;
                    self.builder.build_load(
                        self.local_storage_type(source_type)?,
                        self.access(*source)?,
                        &value_name(*result),
                    )?
                };
                self.values.insert(*result, value);
            }
            Operation::Mutate { place, value } => {
                if !self.zero_sized_places.contains_key(place) {
                    self.builder
                        .build_store(self.place(*place)?, self.value(*value)?)?;
                }
            }
            Operation::LoanMutate { loan, value } => {
                if !self.zero_sized_loans.contains_key(loan) {
                    self.builder
                        .build_store(self.access(PlaceAccess::Loan(*loan))?, self.value(*value)?)?;
                }
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
