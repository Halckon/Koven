//! LLVM direct-call 与普通 shared 返回的内部 ABI。
use super::*;

impl<'ctx, 'llvm, 'ssa, 'sources> ModuleLowerer<'ctx, 'llvm, 'ssa, 'sources> {
    pub(super) fn declare_functions(&mut self) -> Result<(), LlvmAdapterError> {
        for function in &self.ssa.functions {
            let entry = function.blocks.first().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("function 缺少 entry block".to_owned())
            })?;
            let parameters = entry
                .parameters
                .iter()
                .map(|entity| match entity {
                    EntityId::Value(value) => {
                        let ty = value_type(function, *value)?;
                        Ok(BasicMetadataTypeEnum::from(self.type_map.basic_type(ty)?))
                    }
                    EntityId::Loan(_) => Ok(BasicMetadataTypeEnum::from(
                        self.context.ptr_type(inkwell::AddressSpace::default()),
                    )),
                    EntityId::Place(_) => Err(unsupported("LLVM function 参数不能是 place")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let function_type = if function.borrow_return.is_some() {
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .fn_type(&parameters, false)
            } else {
                match function.return_types.as_slice() {
                    [] => self.context.void_type().fn_type(&parameters, false),
                    [result] => self
                        .type_map
                        .basic_type(*result)?
                        .fn_type(&parameters, false),
                    _ => return Err(unsupported("当前 LLVM adapter 不支持多返回值 ABI")),
                }
            };
            let name = format!("f{}.{}", function.id.index(), function.name);
            let llvm_function =
                self.llvm
                    .add_function(&name, function_type, Some(Linkage::Internal));
            if function.never_returns {
                crate::llvm::runtime::add_noreturn_attribute(self.context, llvm_function);
            }
            if let Some(debug) = &mut self.debug {
                debug.attach_function(function, llvm_function)?;
            }
            self.functions.insert(function.id, llvm_function);
        }
        Ok(())
    }
}

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_call(
        &mut self,
        callee: FunctionId,
        arguments: &[EntityId],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let llvm_callee =
            *self.dependencies.functions.get(&callee).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("direct call target 不存在".to_owned())
            })?;
        let arguments = self.lower_call_arguments(arguments)?;
        let call_name = results.first().map_or("", |result| {
            // LLVM 的 void call 不能有名称；非空 result 已由 SSA verifier 对齐 callee 签名。
            // 临时 String 只存活到本次 build_call。
            let _ = result;
            "call"
        });
        let call = self
            .builder
            .build_call(llvm_callee, &arguments, call_name)?;
        match results {
            [] => {
                if matches!(call.try_as_basic_value(), ValueKind::Basic(_)) {
                    return Err(LlvmAdapterError::InvalidSsa(
                        "void SSA call 得到了 LLVM value".to_owned(),
                    ));
                }
            }
            [result] => {
                let value = match call.try_as_basic_value() {
                    ValueKind::Basic(value) => value,
                    ValueKind::Instruction(_) => {
                        return Err(LlvmAdapterError::InvalidSsa(
                            "value SSA call 对应 LLVM void call".to_owned(),
                        ));
                    }
                };
                value.set_name(&value_name(*result));
                self.values.insert(*result, value);
            }
            _ => return Err(unsupported("当前 LLVM adapter 不支持多返回值 direct call")),
        }
        Ok(())
    }
    pub(super) fn lower_borrow_call(
        &mut self,
        instruction: &Instruction,
        callee: FunctionId,
        arguments: &[EntityId],
    ) -> Result<(), LlvmAdapterError> {
        let [EntityId::Loan(result)] = instruction.results.as_slice() else {
            return Err(unsupported("borrow call requires one loan result"));
        };
        let target = *self
            .dependencies
            .functions
            .get(&callee)
            .ok_or_else(|| unsupported("borrow call target missing"))?;
        let arguments = self.lower_call_arguments(arguments)?;
        let call = self.builder.build_call(target, &arguments, "borrow.call")?;
        let ValueKind::Basic(value) = call.try_as_basic_value() else {
            return Err(unsupported("borrow call returned void"));
        };
        self.loans.insert(*result, value.into_pointer_value());

        Ok(())
    }

    fn lower_call_arguments(
        &self,
        arguments: &[EntityId],
    ) -> Result<Vec<BasicMetadataValueEnum<'ctx>>, LlvmAdapterError> {
        arguments
            .iter()
            .map(|argument| match argument {
                EntityId::Value(value) => self.value(*value).map(BasicMetadataValueEnum::from),
                EntityId::Loan(loan) => self
                    .access(PlaceAccess::Loan(*loan))
                    .map(BasicMetadataValueEnum::from),
                EntityId::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                    "direct call 不接受裸 place argument".to_owned(),
                )),
            })
            .collect()
    }
}

pub(crate) mod text {
//! Byte reads preserve the String owner and check indices before constructing an address.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(crate) fn lower_text_read(
        &mut self,
        instruction: &Instruction,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("text read", 1, results.len()));
        };
        let name = value_name(*result);
        let value = match instruction.operation {
            Operation::Utf8ErrorOffset { source } => {
                return self.lower_utf8_error_offset(source, results);
            }
            Operation::StringByteLength { source } => {
                let view = self.string_view(EntityId::Loan(source))?;
                // Every live String was checked against Int.MAX_VALUE at creation.
                self.builder
                    .build_int_cast(view.length(), self.context.i32_type(), &name)?
            }
            Operation::StringByteAt { source, index } => {
                let view = self.string_view(EntityId::Loan(source))?;
                let index = self.int_value(index)?;
                let length = self.builder.build_int_cast(
                    view.length(),
                    self.context.i32_type(),
                    &format!("{name}.length"),
                )?;
                // Unsigned comparison also rejects negative signed Int indices.
                let invalid = self.builder.build_int_compare(
                    IntPredicate::UGE,
                    index,
                    length,
                    &format!("{name}.invalid"),
                )?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    invalid,
                    &name,
                    crate::llvm::runtime::AbortMessage::Text("index out of bounds"),
                )?;
                let offset = self.builder.build_int_z_extend_or_bit_cast(
                    index,
                    self.dependencies.runtime.size_type(),
                    &format!("{name}.offset"),
                )?;
                // SAFETY: the checked success edge proves 0 <= index < byte length.
                let pointer = unsafe {
                    self.builder.build_in_bounds_gep(
                        self.context.i8_type(),
                        view.bytes(),
                        &[offset],
                        &format!("{name}.address"),
                    )?
                };
                self.builder
                    .build_load(self.context.i8_type(), pointer, &name)?
                    .into_int_value()
            }
            _ => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "expected text read".to_owned(),
                ));
            }
        };
        self.values.insert(*result, value.into());
        Ok(())
    }
}

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(crate) fn lower_utf8_error_offset(
        &mut self,
        source: LoanId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("utf8 error offset", 1, results.len()));
        };
        let name = value_name(*result);
        let (bytes, length) = self.byte_array_parts(source, &name)?;
        let offset = self.utf8_offset(bytes, length, &name)?;
        // Array length is bounded by Int.MAX_VALUE; all-ones success becomes Int -1.
        let value = self
            .builder
            .build_int_cast(offset, self.context.i32_type(), &name)?;
        self.values.insert(*result, value.into());
        Ok(())
    }
}

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(crate) fn byte_array_parts(
        &self,
        source: LoanId,
        name: &str,
    ) -> Result<(PointerValue<'ctx>, IntValue<'ctx>), LlvmAdapterError> {
        let ty = access_type(self.function, PlaceAccess::Loan(source))?;
        let header = self
            .builder
            .build_load(
                self.dependencies.type_map.basic_type(ty)?,
                self.access(PlaceAccess::Loan(source))?,
                &format!("{name}.array"),
            )?
            .into_struct_value();
        let bytes = self
            .builder
            .build_extract_value(header, 0, &format!("{name}.bytes"))?
            .into_pointer_value();
        let length = self
            .builder
            .build_extract_value(header, 1, &format!("{name}.length"))?
            .into_int_value();
        Ok((bytes, length))
    }

    pub(crate) fn utf8_offset(
        &self,
        bytes: PointerValue<'ctx>,
        length: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        let scanner = crate::llvm::utf8::define_error_offset(
            self.context,
            self.llvm,
            self.dependencies.runtime.size_type(),
        )?;
        let offset = match self
            .builder
            .build_call(
                scanner,
                &[bytes.into(), length.into()],
                &format!("{name}.offset"),
            )?
            .try_as_basic_value()
        {
            ValueKind::Basic(value) => value.into_int_value(),
            ValueKind::Instruction(_) => {
                return Err(LlvmAdapterError::Build(
                    "UTF-8 scanner returned void".to_owned(),
                ));
            }
        };
        Ok(offset)
    }
}
}

pub(crate) mod integer_range {
//! Pure numeric Range values. Half-open normalization subtracts only on a nonempty edge.
use super::super::{
    FunctionLowerer, IntPredicate, LlvmAdapterError, aggregate, invalid_result_count, value_name,
    value_type,
};
use crate::ssa::model::ValueId;
use inkwell::values::IntValue;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(crate) fn lower_integer_range_construct(
        &mut self,
        first: ValueId,
        bound: ValueId,
        inclusive: bool,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count(
                "integer Range construct",
                1,
                results.len(),
            ));
        };
        let first = self.int_value(first)?;
        let bound = self.int_value(bound)?;
        let name = value_name(*result);
        let nonempty = self.builder.build_int_compare(
            if inclusive {
                IntPredicate::SLE
            } else {
                IntPredicate::SLT
            },
            first,
            bound,
            &format!("{name}.nonempty"),
        )?;
        let empty = self.builder.build_not(nonempty, &format!("{name}.empty"))?;
        let last = if inclusive {
            bound
        } else {
            let present = self
                .context
                .append_basic_block(self.llvm_function, &format!("{name}.nonempty"));
            let absent = self
                .context
                .append_basic_block(self.llvm_function, &format!("{name}.empty"));
            let ready = self
                .context
                .append_basic_block(self.llvm_function, &format!("{name}.ready"));
            self.builder
                .build_conditional_branch(nonempty, present, absent)?;
            self.builder.position_at_end(present);
            // first < bound proves bound is greater than the endpoint type's MIN.
            let last = self.builder.build_int_sub(
                bound,
                bound.get_type().const_int(1, false),
                &format!("{name}.last.nonempty"),
            )?;
            self.builder.build_unconditional_branch(ready)?;
            self.builder.position_at_end(absent);
            self.builder.build_unconditional_branch(ready)?;
            self.builder.position_at_end(ready);
            let phi = self
                .builder
                .build_phi(bound.get_type(), &format!("{name}.last"))?;
            phi.add_incoming(&[(&last, present), (&bound.get_type().const_zero(), absent)]);
            phi.as_basic_value().into_int_value()
        };
        let range = aggregate::construct(
            &self.builder,
            self.dependencies.type_map,
            value_type(self.function, *result)?,
            &[first.into(), last.into(), empty.into()],
            &name,
        )?;
        self.values.insert(*result, range.into());
        Ok(())
    }

    fn integer_range_fields(
        &self,
        value: ValueId,
    ) -> Result<[IntValue<'ctx>; 3], LlvmAdapterError> {
        let range = self.struct_value(value)?;
        let name = value_name(value);
        Ok([
            self.builder
                .build_extract_value(range, 0, &format!("{name}.first"))?
                .into_int_value(),
            self.builder
                .build_extract_value(range, 1, &format!("{name}.last"))?
                .into_int_value(),
            self.builder
                .build_extract_value(range, 2, &format!("{name}.empty"))?
                .into_int_value(),
        ])
    }

    pub(crate) fn lower_integer_range_contains(
        &mut self,
        value: ValueId,
        range: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count(
                "integer Range contains",
                1,
                results.len(),
            ));
        };
        let value = self.int_value(value)?;
        let [first, last, empty] = self.integer_range_fields(range)?;
        let name = value_name(*result);
        let lower = self.builder.build_int_compare(
            IntPredicate::SLE,
            first,
            value,
            &format!("{name}.lower"),
        )?;
        let upper = self.builder.build_int_compare(
            IntPredicate::SLE,
            value,
            last,
            &format!("{name}.upper"),
        )?;
        let within = self
            .builder
            .build_and(lower, upper, &format!("{name}.within"))?;
        let nonempty = self.builder.build_not(empty, &format!("{name}.nonempty"))?;
        let result_value = self.builder.build_and(nonempty, within, &name)?;
        self.values.insert(*result, result_value.into());
        Ok(())
    }

    pub(crate) fn lower_integer_range_equal(
        &mut self,
        left: ValueId,
        right: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count(
                "integer Range equality",
                1,
                results.len(),
            ));
        };
        let [lf, ll, le] = self.integer_range_fields(left)?;
        let [rf, rl, re] = self.integer_range_fields(right)?;
        let name = value_name(*result);
        let both_empty = self
            .builder
            .build_and(le, re, &format!("{name}.both.empty"))?;
        let either_empty = self
            .builder
            .build_or(le, re, &format!("{name}.either.empty"))?;
        let both_nonempty = self
            .builder
            .build_not(either_empty, &format!("{name}.both.nonempty"))?;
        let first_equal = self.builder.build_int_compare(
            IntPredicate::EQ,
            lf,
            rf,
            &format!("{name}.first.equal"),
        )?;
        let last_equal = self.builder.build_int_compare(
            IntPredicate::EQ,
            ll,
            rl,
            &format!("{name}.last.equal"),
        )?;
        let same_bounds =
            self.builder
                .build_and(first_equal, last_equal, &format!("{name}.same.bounds"))?;
        let same_nonempty =
            self.builder
                .build_and(both_nonempty, same_bounds, &format!("{name}.same.nonempty"))?;
        let equal = self.builder.build_or(both_empty, same_nonempty, &name)?;
        self.values.insert(*result, equal.into());
        Ok(())
    }
}
}

pub(crate) mod text_copy {
//! Checked text copies share the established clone allocation and empty-owner paths.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(crate) fn lower_text_copy(
        &mut self,
        instruction: &Instruction,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("text copy", 1, results.len()));
        };
        let name = value_name(*result);
        let view = match instruction.operation {
            Operation::StringClone { source } => self.string_view(EntityId::Loan(source))?,
            Operation::StringCopyBytes { source } => {
                let (bytes, length) = self.byte_array_parts(source, &name)?;
                let offset = self.utf8_offset(bytes, length, &name)?;
                let invalid = self.builder.build_int_compare(
                    IntPredicate::NE,
                    offset,
                    self.dependencies.runtime.size_type().const_all_ones(),
                    &format!("{name}.invalid_utf8"),
                )?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    invalid,
                    &name,
                    crate::llvm::runtime::AbortMessage::Text("invalid UTF-8"),
                )?;
                string::StringView::from_valid_utf8(bytes, length)
            }
            Operation::StringCopySlice { source, start, end } => {
                let source = self.string_view(EntityId::Loan(source))?;
                self.checked_text_slice(
                    source,
                    self.int_value(start)?,
                    self.int_value(end)?,
                    &name,
                )?
            }
            _ => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "expected text copy".to_owned(),
                ));
            }
        };
        let value = string::clone_owner(
            self.context,
            &self.builder,
            self.llvm_function,
            self.dependencies.runtime,
            self.dependencies
                .type_map
                .basic_type(value_type(self.function, *result)?)?
                .into_struct_type(),
            view,
            &name,
        )?;
        self.values.insert(*result, value.into());
        Ok(())
    }

    fn checked_text_slice(
        &self,
        source: string::StringView<'ctx>,
        start: IntValue<'ctx>,
        end: IntValue<'ctx>,
        name: &str,
    ) -> Result<string::StringView<'ctx>, LlvmAdapterError> {
        let length = self.builder.build_int_cast(
            source.length(),
            self.context.i32_type(),
            &format!("{name}.source_length"),
        )?;
        // UGT rejects negative endpoints, and reversed unsigned endpoints are rejected too.
        let start_out = self.builder.build_int_compare(
            IntPredicate::UGT,
            start,
            length,
            &format!("{name}.start_out"),
        )?;
        let end_out = self.builder.build_int_compare(
            IntPredicate::UGT,
            end,
            length,
            &format!("{name}.end_out"),
        )?;
        let reversed = self.builder.build_int_compare(
            IntPredicate::UGT,
            start,
            end,
            &format!("{name}.reversed"),
        )?;
        let outside = self
            .builder
            .build_or(start_out, end_out, &format!("{name}.outside"))?;
        let invalid = self
            .builder
            .build_or(outside, reversed, &format!("{name}.invalid_range"))?;
        self.dependencies.runtime.abort_if(
            &self.builder,
            self.llvm_function,
            invalid,
            name,
            crate::llvm::runtime::AbortMessage::Text("invalid byte range"),
        )?;
        let size = self.dependencies.runtime.size_type();
        let start =
            self.builder
                .build_int_z_extend_or_bit_cast(start, size, &format!("{name}.start"))?;
        let end = self
            .builder
            .build_int_z_extend_or_bit_cast(end, size, &format!("{name}.end"))?;
        self.require_scalar_boundary(source, start, &format!("{name}.start_boundary"))?;
        self.require_scalar_boundary(source, end, &format!("{name}.end_boundary"))?;
        let length = self
            .builder
            .build_int_sub(end, start, &format!("{name}.length"))?;
        // SAFETY: checked bounds prove start <= source length, including the one-past empty slice.
        let bytes = unsafe {
            self.builder.build_in_bounds_gep(
                self.context.i8_type(),
                source.bytes(),
                &[start],
                &format!("{name}.bytes"),
            )?
        };
        Ok(string::StringView::from_valid_utf8(bytes, length))
    }

    /// The caller has checked offset <= length; the end boundary must never load a byte.
    fn require_scalar_boundary(
        &self,
        source: string::StringView<'ctx>,
        offset: IntValue<'ctx>,
        name: &str,
    ) -> Result<(), LlvmAdapterError> {
        let inspect = self
            .context
            .append_basic_block(self.llvm_function, &format!("{name}.inspect"));
        let valid = self
            .context
            .append_basic_block(self.llvm_function, &format!("{name}.valid"));
        let at_end = self.builder.build_int_compare(
            IntPredicate::EQ,
            offset,
            source.length(),
            &format!("{name}.at_end"),
        )?;
        self.builder
            .build_conditional_branch(at_end, valid, inspect)?;
        self.builder.position_at_end(inspect);
        // SAFETY: offset <= length was checked, and this edge excludes offset == length.
        let pointer = unsafe {
            self.builder.build_in_bounds_gep(
                self.context.i8_type(),
                source.bytes(),
                &[offset],
                &format!("{name}.address"),
            )?
        };
        let byte = self
            .builder
            .build_load(self.context.i8_type(), pointer, &format!("{name}.byte"))?
            .into_int_value();
        let prefix = self.builder.build_and(
            byte,
            self.context.i8_type().const_int(0xc0, false),
            &format!("{name}.prefix"),
        )?;
        let continuation = self.builder.build_int_compare(
            IntPredicate::EQ,
            prefix,
            self.context.i8_type().const_int(0x80, false),
            &format!("{name}.continuation"),
        )?;
        self.dependencies.runtime.abort_if(
            &self.builder,
            self.llvm_function,
            continuation,
            name,
            crate::llvm::runtime::AbortMessage::Text("not a scalar boundary"),
        )?;
        self.builder.build_unconditional_branch(valid)?;
        self.builder.position_at_end(valid);
        Ok(())
    }
}
}
