//! N1a range metadata has inline storage; neither construct nor end allocates elements.
use super::*;
use crate::llvm::runtime::AbortMessage;
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
                let index_value = self.int_value(*index)?;
                let length = self.builder.build_int_sub(end, begin, "range.size")?;
                let out_of_bounds = AbortMessage::IndexOutOfBounds {
                    index: index_value,
                    size: length,
                };
                // Report a negative index with the bounds text before size conversion.
                let negative = self.builder.build_int_compare(
                    IntPredicate::SLT,
                    index_value,
                    index_value.get_type().const_zero(),
                    "range.index.negative",
                )?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    negative,
                    "range.element.negative",
                    out_of_bounds,
                )?;
                let relative = self.dependencies.runtime.container_int_to_size(
                    &self.builder,
                    self.llvm_function,
                    index_value,
                    "range.index",
                )?;
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
                    out_of_bounds,
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
                let begin = self.int_value(*begin)?;
                let end = self.int_value(*end)?;
                // SPEC-0355: negative endpoints report the range text, not a size conversion.
                let begin_negative = self.builder.build_int_compare(
                    IntPredicate::SLT,
                    begin,
                    begin.get_type().const_zero(),
                    "range.begin.negative",
                )?;
                let end_negative = self.builder.build_int_compare(
                    IntPredicate::SLT,
                    end,
                    end.get_type().const_zero(),
                    "range.end.negative",
                )?;
                let negative =
                    self.builder
                        .build_or(begin_negative, end_negative, "range.negative")?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    negative,
                    "range.bounds.negative",
                    AbortMessage::Text("range out of bounds"),
                )?;
                let begin = self.dependencies.runtime.container_int_to_size(
                    &self.builder,
                    self.llvm_function,
                    begin,
                    "range.begin",
                )?;
                let end = self.dependencies.runtime.container_int_to_size(
                    &self.builder,
                    self.llvm_function,
                    end,
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
                    AbortMessage::Text("range out of bounds"),
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

pub(crate) mod callable {
use crate::ssa::model::{
    ClosureCaptureOperand, EntityId, FunctionId, PlaceAccess, SsaTypeId, ValueId,
};

use super::super::{
    FunctionLowerer, LlvmAdapterError, closure, invalid_result_count, value_name, value_type,
};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(crate) fn lower_function_address(
        &mut self,
        target: FunctionId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("function address", 1, results.len()));
        };
        let target = *self.dependencies.functions.get(&target).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("function address target was not declared".to_owned())
        })?;
        self.values
            .insert(*result, closure::function_address(target).into());
        Ok(())
    }

    pub(crate) fn lower_closure_construct(
        &mut self,
        closure_type: SsaTypeId,
        thunk: FunctionId,
        captures: &[ClosureCaptureOperand],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("closure construct", 1, results.len()));
        };
        let thunk = *self.dependencies.functions.get(&thunk).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("closure thunk was not declared".to_owned())
        })?;
        let captures = captures
            .iter()
            .map(|capture| match capture {
                ClosureCaptureOperand::Owned(value) => self.value(*value),
                ClosureCaptureOperand::Shared(loan) => self
                    .loans
                    .get(loan)
                    .copied()
                    .map(Into::into)
                    .ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa(
                            "shared closure capture loan has no LLVM pointer".to_owned(),
                        )
                    }),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = closure::construct(
            &self.builder,
            self.dependencies.type_map,
            closure_type,
            thunk,
            &captures,
            &value_name(*result),
        )?;
        self.values.insert(*result, value.into());
        Ok(())
    }

    fn lower_callable_invoke(
        &mut self,
        callable: ValueId,
        arguments: &[EntityId],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let arguments = arguments
            .iter()
            .map(|argument| match argument {
                EntityId::Value(value) => self.value(*value),
                EntityId::Loan(loan) => self.access(PlaceAccess::Loan(*loan)).map(Into::into),
                EntityId::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                    "callable invoke does not accept a bare place".to_owned(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = closure::invoke(
            &self.builder,
            self.module,
            self.dependencies.type_map,
            value_type(self.function, callable)?,
            self.value(callable)?,
            &arguments,
            if results.is_empty() { "" } else { "invoke" },
        )?;
        match (results, value) {
            ([], None) => Ok(()),
            ([result], Some(value)) => {
                value.set_name(&value_name(*result));
                self.values.insert(*result, value);
                Ok(())
            }
            _ => Err(LlvmAdapterError::InvalidSsa(
                "indirect call LLVM result did not match SSA results".to_owned(),
            )),
        }
    }
}

/// ADR-0034 callable views: a view loan addresses a `{code, environment}` descriptor in the
/// caller's frame. Concrete closures contribute their thunk and the borrowed environment;
/// function pointers contribute a per-signature trampoline that receives the pointer as its
/// environment, so every view is called the same way.
impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    /// Indirect calls through a callable value, and the ADR-0034 view operations.
    pub(crate) fn lower_callable(
        &mut self,
        instruction: &crate::ssa::model::Instruction,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            crate::ssa::model::Operation::CallableInvoke {
                callable,
                arguments,
            } => self.lower_callable_invoke(*callable, arguments, results),
            crate::ssa::model::Operation::CallableViewOf { source } => {
                self.lower_callable_view_of(instruction, *source)
            }
            crate::ssa::model::Operation::CallableInvokeView { view, arguments } => {
                self.lower_callable_invoke_view(*view, arguments, results)
            }
            _ => Err(LlvmAdapterError::InvalidSsa(
                "callable lowering received a non-callable operation".to_owned(),
            )),
        }
    }

    fn lower_callable_view_of(
        &mut self,
        instruction: &crate::ssa::model::Instruction,
        source: crate::ssa::model::LoanId,
    ) -> Result<(), LlvmAdapterError> {
        let [EntityId::Loan(result)] = instruction.results.as_slice() else {
            return Err(invalid_result_count(
                "callable view",
                1,
                instruction.results.len(),
            ));
        };
        let loan_target = |loan| match self.function.entity(EntityId::Loan(loan)).map(|e| e.ty) {
            Some(crate::ssa::model::EntityType::Loan { target, .. }) => Ok(target),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "callable view operand is not a loan".to_owned(),
            )),
        };
        let callable = loan_target(source)?;
        let view = loan_target(*result)?;
        let storage = self.access(PlaceAccess::Loan(source))?;
        let pointer = self.context.ptr_type(inkwell::AddressSpace::default());
        let name = format!("l{}", result.index());
        let (code, environment) = match self.module.type_kind(callable) {
            Some(crate::ssa::model::SsaTypeKind::ConcreteClosure { .. }) => {
                let layout = self.dependencies.type_map.closure_layout(callable)?;
                let code_slot = self.builder.build_struct_gep(
                    layout.value,
                    storage,
                    0,
                    &format!("{name}.code"),
                )?;
                let code = self
                    .builder
                    .build_load(pointer, code_slot, &format!("{name}.thunk"))?;
                let environment = self.builder.build_struct_gep(
                    layout.value,
                    storage,
                    1,
                    &format!("{name}.env"),
                )?;
                (code, environment.into())
            }
            Some(crate::ssa::model::SsaTypeKind::FunctionPointer { signature }) => {
                let target =
                    self.builder
                        .build_load(pointer, storage, &format!("{name}.target"))?;
                let trampoline = self.view_trampoline(view, signature)?;
                (closure::function_address(trampoline).into(), target)
            }
            _ => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "callable view source is not a function pointer or closure".to_owned(),
                ));
            }
        };
        let descriptor_type = self.dependencies.type_map.basic_type(view)?;
        let descriptor = self.entry_alloca(descriptor_type, &format!("{name}.view"))?;
        let descriptor_struct = descriptor_type.into_struct_type();
        let code_field = self.builder.build_struct_gep(
            descriptor_struct,
            descriptor,
            0,
            &format!("{name}.view.code"),
        )?;
        self.builder.build_store(code_field, code)?;
        let environment_field = self.builder.build_struct_gep(
            descriptor_struct,
            descriptor,
            1,
            &format!("{name}.view.env"),
        )?;
        self.builder.build_store(environment_field, environment)?;
        self.loans.insert(*result, descriptor);
        Ok(())
    }

    fn lower_callable_invoke_view(
        &mut self,
        view: crate::ssa::model::LoanId,
        arguments: &[EntityId],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let Some(crate::ssa::model::EntityType::Loan { target, .. }) =
            self.function.entity(EntityId::Loan(view)).map(|e| e.ty)
        else {
            return Err(LlvmAdapterError::InvalidSsa(
                "callable view invoke operand is not a loan".to_owned(),
            ));
        };
        let signature = self.module.callable_view_signature(target).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("callable view invoke target is not a view".to_owned())
        })?;
        let descriptor_struct = self
            .dependencies
            .type_map
            .basic_type(target)?
            .into_struct_type();
        let descriptor = self.access(PlaceAccess::Loan(view))?;
        let pointer = self.context.ptr_type(inkwell::AddressSpace::default());
        let code_field =
            self.builder
                .build_struct_gep(descriptor_struct, descriptor, 0, "view.code")?;
        let code = self
            .builder
            .build_load(pointer, code_field, "view.code")?
            .into_pointer_value();
        let environment_field =
            self.builder
                .build_struct_gep(descriptor_struct, descriptor, 1, "view.env")?;
        let environment = self
            .builder
            .build_load(pointer, environment_field, "view.env")?;
        let mut operands = vec![inkwell::values::BasicMetadataValueEnum::from(environment)];
        for argument in arguments {
            operands.push(match argument {
                EntityId::Value(value) => self.value(*value)?.into(),
                EntityId::Loan(loan) => self.access(PlaceAccess::Loan(*loan))?.into(),
                EntityId::Place(_) => {
                    return Err(LlvmAdapterError::InvalidSsa(
                        "callable view invoke does not accept a bare place".to_owned(),
                    ));
                }
            });
        }
        let function_type = self
            .dependencies
            .type_map
            .callable_function_type(signature, true)?;
        let name = match results {
            [result] => value_name(*result),
            _ => String::new(),
        };
        let call = self
            .builder
            .build_indirect_call(function_type, code, &operands, &name)?;
        if signature.never_returns {
            closure::mark_call_noreturn(call, function_type);
        }
        match (results, call.try_as_basic_value()) {
            ([], inkwell::values::ValueKind::Instruction(_)) => Ok(()),
            ([result], inkwell::values::ValueKind::Basic(value)) => {
                self.values.insert(*result, value);
                Ok(())
            }
            _ => Err(LlvmAdapterError::InvalidSsa(
                "callable view call result did not match SSA results".to_owned(),
            )),
        }
    }

    /// The per-signature trampoline `(target, arguments...) -> target(arguments...)`.
    fn view_trampoline(
        &self,
        view: SsaTypeId,
        signature: &crate::ssa::model::CallableSignature,
    ) -> Result<inkwell::values::FunctionValue<'ctx>, LlvmAdapterError> {
        let name = format!("koven.callable_view.trampoline.t{}", view.index());
        if let Some(existing) = self.llvm.get_function(&name) {
            return Ok(existing);
        }
        let types = self.dependencies.type_map;
        let outer = types.callable_function_type(signature, true)?;
        let inner = types.callable_function_type(signature, false)?;
        let trampoline =
            self.llvm
                .add_function(&name, outer, Some(inkwell::module::Linkage::Internal));
        let builder = self.context.create_builder();
        let entry = self.context.append_basic_block(trampoline, "entry");
        builder.position_at_end(entry);
        let mut parameters = trampoline.get_param_iter();
        let target = parameters
            .next()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("trampoline has no target".to_owned()))?
            .into_pointer_value();
        let operands = parameters
            .map(inkwell::values::BasicMetadataValueEnum::from)
            .collect::<Vec<_>>();
        let call = builder.build_indirect_call(inner, target, &operands, "")?;
        if signature.never_returns {
            crate::llvm::runtime::add_noreturn_attribute(self.context, trampoline);
            closure::mark_call_noreturn(call, inner);
            builder.build_unreachable()?;
        } else {
            match call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(value) => builder.build_return(Some(&value))?,
                inkwell::values::ValueKind::Instruction(_) => builder.build_return(None)?,
            };
        }
        Ok(trampoline)
    }

    /// A stack slot in the entry block, reused by every execution of the requesting operation.
    fn entry_alloca(
        &self,
        ty: inkwell::types::BasicTypeEnum<'ctx>,
        name: &str,
    ) -> Result<inkwell::values::PointerValue<'ctx>, LlvmAdapterError> {
        let entry = self.llvm_function.get_first_basic_block().ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("callable view requires function entry".to_owned())
        })?;
        let allocator = self.context.create_builder();
        if let Some(first) = entry.get_first_instruction() {
            allocator.position_before(&first);
        } else {
            allocator.position_at_end(entry);
        }
        Ok(allocator.build_alloca(ty, name)?)
    }
}
}
