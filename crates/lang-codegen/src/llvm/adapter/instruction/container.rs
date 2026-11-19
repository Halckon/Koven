//! Sequential container and Map instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction_container(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::ContainerConstruct {
                container,
                elements,
            } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count(
                        "container construct",
                        1,
                        results.len(),
                    ));
                };
                let elements = elements
                    .iter()
                    .map(|element| self.value(*element))
                    .collect::<Result<Vec<_>, _>>()?;
                let value = container::construct(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    *container,
                    &elements,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::ContainerGenerate {
                container,
                length,
                initializer,
            } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("container generate", 1, results.len()));
                };
                let initializer =
                    *self
                        .dependencies
                        .functions
                        .get(initializer)
                        .ok_or_else(|| {
                            LlvmAdapterError::InvalidSsa(
                                "容器 initializer target 未声明".to_owned(),
                            )
                        })?;
                let value = container::generate(
                    self.context,
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    initializer,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    *container,
                    self.int_value(*length)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::ContainerGenerateBorrowed {
                container,
                length,
                initializer,
            } => {
                self.lower_container_generate_borrowed(
                    *container,
                    *length,
                    *initializer,
                    &results,
                )?;
            }
            Operation::ContainerLength { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("container length", 1, results.len()));
                };
                let owner = match owner {
                    EntityId::Value(owner) => self.struct_value(*owner)?,
                    EntityId::Loan(loan) => {
                        let container_type = self
                            .function
                            .entity(*owner)
                            .ok_or_else(|| {
                                LlvmAdapterError::InvalidSsa(
                                    "container length owner is missing".to_owned(),
                                )
                            })?
                            .ty
                            .semantic_type();
                        self.builder
                            .build_load(
                                self.dependencies.type_map.basic_type(container_type)?,
                                self.access(PlaceAccess::Loan(*loan))?,
                                &format!("v{}.container", result.index()),
                            )?
                            .into_struct_value()
                    }
                    EntityId::Place(_) => {
                        return Err(LlvmAdapterError::InvalidSsa(
                            "container length owner cannot be a place".to_owned(),
                        ));
                    }
                };
                let value = container::length(
                    &self.builder,
                    owner,
                    self.context.i32_type(),
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::ContainerElementPlace { owner, index } => {
                let result = place_result(instruction)?;
                let container_type = self
                    .function
                    .entity(*owner)
                    .ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa(
                            "container element owner entity is missing".to_owned(),
                        )
                    })?
                    .ty
                    .semantic_type();
                let layout = self
                    .dependencies
                    .type_map
                    .container_layout(container_type)?;
                let owner = match owner {
                    EntityId::Value(owner) => self.struct_value(*owner)?,
                    EntityId::Loan(loan) => self
                        .builder
                        .build_load(
                            self.dependencies.type_map.basic_type(container_type)?,
                            self.access(PlaceAccess::Loan(*loan))?,
                            &format!("p{}.container", result.index()),
                        )?
                        .into_struct_value(),
                    EntityId::Place(_) => {
                        return Err(LlvmAdapterError::InvalidSsa(
                            "container element owner cannot be a place".to_owned(),
                        ));
                    }
                };
                let pointer = container::element_place(
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    owner,
                    self.int_value(*index)?,
                    &format!("p{}", result.index()),
                )?;
                self.places.insert(result, pointer);
                if layout.stride == 0 {
                    self.zero_sized_places
                        .insert(result, layout.element.const_zero());
                }
            }
            Operation::ProjectedSwap {
                root,
                loans: [left, right],
            } => {
                let element = self
                    .function
                    .entity(EntityId::Loan(*left))
                    .ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("missing projected swap loan".to_owned())
                    })?
                    .ty
                    .semantic_type();
                let types = self.dependencies.type_map;
                let storage = self.local_storage_type(element)?;
                // A CFG parameter can carry a zero-stride element loan without
                // inheriting the lowering map's sentinel marker.
                let old_right = if types.is_zero_sized_container_element(element) {
                    storage.const_zero()
                } else {
                    let left = self.access(PlaceAccess::Loan(*left))?;
                    let right = self.access(PlaceAccess::Loan(*right))?;
                    let old_left = self.builder.build_load(storage, left, "swap.left")?;
                    let old_right = self.builder.build_load(storage, right, "swap.right")?;
                    self.builder.build_store(left, old_right)?;
                    self.builder.build_store(right, old_left)?;
                    old_right
                };
                if root.is_some() {
                    let [new_root] = results.as_slice() else {
                        return Err(invalid_result_count(
                            "mixed projected swap",
                            1,
                            results.len(),
                        ));
                    };
                    self.values.insert(*new_root, old_right);
                }
                for loan in [left, right] {
                    self.loans.remove(loan);
                    self.zero_sized_loans.remove(loan);
                }
            }
            Operation::ContainerAppend { owner, element } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("container append", 1, results.len()));
                };
                let container_type = value_type(self.function, *owner)?;
                let new_owner = container::append(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    self.value(*element)?,
                    &format!("append.i{}", instruction.id.index()),
                )?;
                self.values.insert(*result, new_owner.into());
            }
            Operation::ContainerAppendInout { loan, element } => {
                let container_type = access_type(self.function, PlaceAccess::Loan(*loan))?;
                let pointer = self.access(PlaceAccess::Loan(*loan))?;
                let name = format!("append.i{}", instruction.id.index());
                let header = self.builder.build_load(
                    self.dependencies.type_map.basic_type(container_type)?,
                    pointer,
                    &format!("{name}.header"),
                )?;
                let updated = container::append(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    header.into_struct_value(),
                    self.value(*element)?,
                    &name,
                )?;
                self.builder.build_store(pointer, updated)?;
            }
            Operation::ContainerClear { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("container clear", 1, results.len()));
                };
                let container_type = value_type(self.function, *owner)?;
                let new_owner = container::clear(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.module,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    &format!("clear.i{}", instruction.id.index()),
                )?;
                self.values.insert(*result, new_owner.into());
            }
            Operation::ContainerRemoveAt { owner, index } => {
                let [removed_result, new_owner_result] = results.as_slice() else {
                    return Err(invalid_result_count("container removeAt", 2, results.len()));
                };
                let container_type = value_type(self.function, *owner)?;
                let (removed_element, new_owner) = container::remove_at(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.module,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    self.int_value(*index)?,
                    &format!("remove_at.i{}", instruction.id.index()),
                )?;
                self.values.insert(*removed_result, removed_element);
                self.values.insert(*new_owner_result, new_owner.into());
            }
            Operation::ContainerRemoveFirst { owner } => {
                let [removed_result, new_owner_result] = results.as_slice() else {
                    return Err(invalid_result_count(
                        "container removeFirst",
                        2,
                        results.len(),
                    ));
                };
                let container_type = value_type(self.function, *owner)?;
                let (removed_element, new_owner) = container::remove_first(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.module,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    &format!("remove_first.i{}", instruction.id.index()),
                )?;
                self.values.insert(*removed_result, removed_element);
                self.values.insert(*new_owner_result, new_owner.into());
            }
            Operation::ContainerRemoveLast { owner } => {
                let [removed_result, new_owner_result] = results.as_slice() else {
                    return Err(invalid_result_count(
                        "container removeLast",
                        2,
                        results.len(),
                    ));
                };
                let container_type = value_type(self.function, *owner)?;
                let (removed_element, new_owner) = container::remove_last(
                    &self.builder,
                    self.llvm_function,
                    self.module,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    &format!("remove_last.i{}", instruction.id.index()),
                )?;
                self.values.insert(*removed_result, removed_element);
                self.values.insert(*new_owner_result, new_owner.into());
            }
            Operation::ContainerInsertAt {
                owner,
                index,
                element,
            } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("container insertAt", 1, results.len()));
                };
                let container_type = value_type(self.function, *owner)?;
                let new_owner = container::insert_at(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.module,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    self.int_value(*index)?,
                    self.value(*element)?,
                    &format!("insert_at.i{}", instruction.id.index()),
                )?;
                self.values.insert(*result, new_owner.into());
            }
            Operation::ContainerReplace {
                owner,
                index,
                value,
            } => {
                let container_type = value_type(self.function, *owner)?;
                container::replace(
                    &self.builder,
                    self.llvm_function,
                    self.module,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    container_type,
                    self.struct_value(*owner)?,
                    self.int_value(*index)?,
                    self.value(*value)?,
                    &format!("replace.i{}", instruction.id.index()),
                )?;
            }
            Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. } => {
                self.lower_map_iteration_operation(instruction)?;
            }
            Operation::MapRequireValue { .. } => self.lower_map_require_operation(instruction)?,
            Operation::MapWithValue { .. } => self.lower_map_with_operation(instruction)?,
            Operation::MapConstruct { .. }
            | Operation::MapConsume { .. }
            | Operation::MapSize { .. }
            | Operation::MapContains { .. }
            | Operation::MapGet { .. }
            | Operation::MapResultUnwrap { .. }
            | Operation::MapPut { .. }
            | Operation::MapRemove { .. } => self.lower_map_operation(instruction)?,
            _ => {
                return Err(unsupported(
                    "internal dispatch invariant violated: operation belongs to another lowering family",
                ));
            }
        }
        Ok(())
    }
}
