//! N1a range metadata has inline storage; neither construct nor end allocates elements.
use super::*;
use crate::ssa::model::SsaTypeKind;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_range(
        &mut self,
        instruction: &Instruction,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::RangeElementPlace { view, index } => {
                let result = place_result(instruction)?;
                let view_type = self
                    .function
                    .entity(EntityId::Loan(*view))
                    .ok_or_else(|| unsupported("range metadata loan missing"))?
                    .ty
                    .semantic_type();
                let Some(SsaTypeKind::RangeView { source }) = self.module.type_kind(view_type)
                else {
                    return Err(unsupported("range metadata type missing"));
                };
                let descriptor = self
                    .builder
                    .build_load(
                        self.dependencies.type_map.basic_type(view_type)?,
                        self.access(PlaceAccess::Loan(*view))?,
                        "range.metadata",
                    )?
                    .into_struct_value();
                let root = self
                    .builder
                    .build_extract_value(descriptor, 0, "range.source")?
                    .into_pointer_value();
                let begin = self
                    .builder
                    .build_extract_value(descriptor, 1, "range.begin")?
                    .into_int_value();
                let end = self
                    .builder
                    .build_extract_value(descriptor, 2, "range.end")?
                    .into_int_value();
                let relative = self.dependencies.runtime.container_int_to_size(
                    &self.builder,
                    self.llvm_function,
                    self.int_value(*index)?,
                    "range.index",
                )?;
                let length = self.builder.build_int_sub(end, begin, "range.size")?;
                let beyond = self.builder.build_int_compare(
                    IntPredicate::UGE,
                    relative,
                    length,
                    "range.index.beyond",
                )?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    beyond,
                    "range.element.bounds",
                )?;
                // Range construction validated begin <= end <= List length <= Int::MAX.
                // The relative bounds guard ensures this sum remains in that same range.
                let absolute =
                    self.builder
                        .build_int_add(begin, relative, "range.absolute.index")?;
                let absolute = self.builder.build_int_cast(
                    absolute,
                    self.context.i32_type(),
                    "range.absolute.int",
                )?;
                let header = self
                    .builder
                    .build_load(
                        self.dependencies.type_map.basic_type(*source)?,
                        root,
                        "range.root",
                    )?
                    .into_struct_value();
                let pointer = container::element_place(
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    *source,
                    header,
                    absolute,
                    "range.element",
                )?;
                self.places.insert(result, pointer);
                if self.dependencies.type_map.container_layout(*source)?.stride == 0 {
                    self.zero_sized_places.insert(
                        result,
                        self.dependencies
                            .type_map
                            .container_layout(*source)?
                            .element
                            .const_zero(),
                    );
                }
            }
            Operation::RangeConstruct {
                view,
                source,
                begin,
                end,
            } => {
                let [EntityId::Value(value), EntityId::Loan(loan)] = instruction.results.as_slice()
                else {
                    return Err(unsupported("range requires value and protecting loan"));
                };
                let Some(SsaTypeKind::RangeView { .. }) = self.module.type_kind(*view) else {
                    return Err(unsupported("range storage type missing"));
                };
                let (root, offset, length) = self.range_source(*source)?;
                let begin = self.dependencies.runtime.container_int_to_size(
                    &self.builder,
                    self.llvm_function,
                    self.int_value(*begin)?,
                    "range.begin",
                )?;
                let end = self.dependencies.runtime.container_int_to_size(
                    &self.builder,
                    self.llvm_function,
                    self.int_value(*end)?,
                    "range.end",
                )?;
                let reversed = self.builder.build_int_compare(
                    IntPredicate::UGT,
                    begin,
                    end,
                    "range.reversed",
                )?;
                let beyond = self.builder.build_int_compare(
                    IntPredicate::UGT,
                    end,
                    length,
                    "range.beyond",
                )?;
                let invalid = self.builder.build_or(reversed, beyond, "range.invalid")?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    invalid,
                    "range.bounds",
                )?;
                // Relative bounds lie within the parent; root coordinates remain <= List length.
                let begin = self
                    .builder
                    .build_int_add(offset, begin, "range.root.begin")?;
                let end = self.builder.build_int_add(offset, end, "range.root.end")?;
                let ty = self
                    .dependencies
                    .type_map
                    .basic_type(*view)?
                    .into_struct_type();
                let descriptor = self
                    .builder
                    .build_insert_value(ty.get_undef(), root, 0, "range.source")?
                    .into_struct_value();
                let descriptor = self
                    .builder
                    .build_insert_value(descriptor, begin, 1, "range.begin")?
                    .into_struct_value();
                let descriptor = self
                    .builder
                    .build_insert_value(descriptor, end, 2, "range.end")?
                    .into_struct_value();
                self.values.insert(*value, descriptor.into());
                self.loans.insert(*loan, root);
            }
            Operation::RangeCall {
                callee,
                arguments,
                source,
            } => {
                let [EntityId::Value(view), EntityId::Loan(loan)] = instruction.results.as_slice()
                else {
                    return Err(unsupported("range call requires value and protecting loan"));
                };
                let (root, _, _) = self.range_source(*source)?;
                self.lower_call(*callee, arguments, &[*view])?;
                self.loans.insert(*loan, root);
            }
            Operation::RangeLength { view } => {
                let [EntityId::Value(result)] = instruction.results.as_slice() else {
                    return Err(unsupported("range length requires one value"));
                };
                let descriptor = match view {
                    EntityId::Value(value) => self.struct_value(*value)?,
                    EntityId::Loan(loan) => {
                        let EntityType::Loan { target, .. } = self
                            .function
                            .entity(*view)
                            .ok_or_else(|| unsupported("range loan missing"))?
                            .ty
                        else {
                            return Err(unsupported("range loan type missing"));
                        };
                        self.builder
                            .build_load(
                                self.dependencies.type_map.basic_type(target)?,
                                self.access(PlaceAccess::Loan(*loan))?,
                                "range.metadata",
                            )?
                            .into_struct_value()
                    }
                    EntityId::Place(_) => {
                        return Err(unsupported("range length requires active metadata"));
                    }
                };
                let begin = self
                    .builder
                    .build_extract_value(descriptor, 1, "range.begin")?
                    .into_int_value();
                let end = self
                    .builder
                    .build_extract_value(descriptor, 2, "range.end")?
                    .into_int_value();
                let size = self.builder.build_int_sub(end, begin, "range.size")?;
                let size =
                    self.builder
                        .build_int_cast(size, self.context.i32_type(), "range.size.int")?;
                self.values.insert(*result, size.into());
            }
            Operation::RangeEnd { view, source } => {
                self.values.remove(view);
                self.loans.remove(source);
            }
            _ => return Err(unsupported("unexpected range operation")),
        }
        Ok(())
    }

    fn range_source(
        &self,
        loan: crate::ssa::model::LoanId,
    ) -> Result<(PointerValue<'ctx>, IntValue<'ctx>, IntValue<'ctx>), LlvmAdapterError> {
        let target = self
            .function
            .entity(EntityId::Loan(loan))
            .ok_or_else(|| unsupported("range source loan missing"))?
            .ty
            .semantic_type();
        let pointer = self.access(PlaceAccess::Loan(loan))?;
        let source = self
            .builder
            .build_load(
                self.dependencies.type_map.basic_type(target)?,
                pointer,
                "range.source.storage",
            )?
            .into_struct_value();
        let begin_or_length = self
            .builder
            .build_extract_value(source, 1, "range.source.bound")?
            .into_int_value();
        if matches!(
            self.module.type_kind(target),
            Some(SsaTypeKind::RangeView { .. })
        ) {
            let root = self
                .builder
                .build_extract_value(source, 0, "range.source.root")?
                .into_pointer_value();
            let end = self
                .builder
                .build_extract_value(source, 2, "range.source.end")?
                .into_int_value();
            let length = self
                .builder
                .build_int_sub(end, begin_or_length, "range.source.size")?;
            Ok((root, begin_or_length, length))
        } else {
            Ok((
                pointer,
                begin_or_length.get_type().const_zero(),
                begin_or_length,
            ))
        }
    }
}
