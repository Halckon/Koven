use std::path::Path;

use inkwell::{
    context::Context,
    module::Module as LlvmModule,
    targets::{FileType, TargetMachine, TargetTriple},
};
use lang_frontend::source::SourceMap;

use crate::ssa::{model::Program, verify::verify_program};

use super::super::entry::NativeEntryPlan;
use super::super::{LlvmAdapterError, configure_module, debug, entry, native_target_machine};
use super::ModuleLowerer;

#[cfg(test)]
pub(crate) fn render_verified_program(
    program: &Program,
    native_entry: Option<NativeEntryPlan>,
) -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = native_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        native_entry,
        None,
        &triple,
        &target_machine,
    )?;
    Ok(llvm_module.print_to_string().to_string())
}

#[cfg(test)]
pub(crate) fn render_verified_program_with_debug(
    program: &Program,
    sources: &SourceMap,
    native_entry: NativeEntryPlan,
) -> Result<String, LlvmAdapterError> {
    let (triple, target_machine) = native_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        Some(native_entry),
        Some(sources),
        &triple,
        &target_machine,
    )?;
    Ok(llvm_module.print_to_string().to_string())
}

pub(crate) fn emit_verified_object(
    program: &Program,
    sources: &SourceMap,
    native_entry: NativeEntryPlan,
    path: &Path,
) -> Result<(), LlvmAdapterError> {
    let (triple, target_machine) = native_target_machine()?;
    let context = Context::create();
    let llvm_module = lower_verified_module(
        &context,
        program,
        Some(native_entry),
        Some(sources),
        &triple,
        &target_machine,
    )?;
    #[cfg(test)]
    let object_path = crate::llvm::emission_failure::object_path(path);
    #[cfg(test)]
    let path = object_path.as_path();
    // Keep the original OS path and return write errors to the sibling
    // transaction. LLVM's direct file writer can terminate on a write error.
    let object = target_machine
        .write_to_memory_buffer(&llvm_module, FileType::Object)
        .map_err(|error| LlvmAdapterError::Object(error.to_string()))?;
    std::fs::write(path, object.as_slice())
        .map_err(|error| LlvmAdapterError::Object(format!("write object: {error}")))
}

fn lower_verified_module<'ctx>(
    context: &'ctx Context,
    program: &Program,
    native_entry: Option<NativeEntryPlan>,
    debug_sources: Option<&SourceMap>,
    triple: &TargetTriple,
    target_machine: &TargetMachine,
) -> Result<LlvmModule<'ctx>, LlvmAdapterError> {
    verify_program(program).map_err(|error| LlvmAdapterError::InvalidSsa(error.to_string()))?;
    let [ssa_module] = program.modules.as_slice() else {
        return Err(LlvmAdapterError::Unsupported(
            "当前 LLVM adapter 只接受一个 SSA module".to_owned(),
        ));
    };
    if let Some(native_entry) = native_entry {
        entry::validate(ssa_module, native_entry)?;
    }
    let debug_plan = match debug_sources {
        Some(sources) => {
            let native_entry = native_entry.ok_or_else(|| {
                LlvmAdapterError::Debug("debug lowering 缺少 native entry".to_owned())
            })?;
            Some(debug::DebugPlan::build(
                sources,
                ssa_module,
                native_entry.function(),
            )?)
        }
        None => None,
    };
    let llvm_module = context.create_module(&ssa_module.name);
    configure_module(&llvm_module, triple, target_machine);
    let debug = debug_plan.map(|plan| debug::DebugEmitter::new(context, &llvm_module, plan));
    ModuleLowerer::new(
        context,
        &llvm_module,
        ssa_module,
        &target_machine.get_target_data(),
        debug,
        native_entry,
    )?
    .lower(native_entry)?;
    llvm_module
        .verify()
        .map_err(|error| LlvmAdapterError::Verify(error.to_string()))?;
    Ok(llvm_module)
}

pub(crate) mod constant {
    //! Scalar values and test-only synthetic ZST materialization.
    use super::super::*;
    impl FunctionLowerer<'_, '_, '_, '_, '_> {
        pub(crate) fn lower_constant(
            &mut self,
            constant: &ScalarConstant,
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [result] = results else {
                return Err(invalid_result_count("constant", 1, results.len()));
            };
            let integer = match constant {
                ScalarConstant::Boolean(value) => Some((u64::from(*value), false)),
                ScalarConstant::Char(value) => Some((u64::from(*value), false)),
                ScalarConstant::Integer(value) => Some((*value as u64, *value < 0)),
                #[cfg(test)]
                ScalarConstant::SyntheticZero => None,
                ScalarConstant::Unit => None,
            };
            let value = match integer {
                Some((value, signed)) => self
                    .dependencies
                    .type_map
                    .int_type(value_type(self.function, *result)?)?
                    .const_int(value, signed)
                    .into(),
                None => {
                    #[cfg(test)]
                    if matches!(constant, ScalarConstant::SyntheticZero) {
                        let zero = self
                            .dependencies
                            .type_map
                            .basic_type(value_type(self.function, *result)?)?
                            .into_struct_type()
                            .const_zero();
                        self.values.insert(*result, zero.into());
                        return Ok(());
                    }
                    self.context.struct_type(&[], false).const_zero().into()
                }
            };
            self.values.insert(*result, value);
            Ok(())
        }
    }
}

pub(crate) mod borrowed_generate {
    //! Borrowed callable storage bridge for container generation.

    use super::super::{
        FunctionLowerer, LlvmAdapterError, access_type, container, invalid_result_count, value_name,
    };
    use crate::ssa::model::{LoanId, PlaceAccess, SsaTypeId, ValueId};

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        pub(crate) fn lower_container_generate_borrowed(
            &mut self,
            container: SsaTypeId,
            length: ValueId,
            initializer: LoanId,
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [result] = results else {
                return Err(invalid_result_count(
                    "borrowed container generate",
                    1,
                    results.len(),
                ));
            };
            let ty = access_type(self.function, PlaceAccess::Loan(initializer))?;
            let callable = self.builder.build_load(
                self.dependencies.type_map.basic_type(ty)?,
                self.access(PlaceAccess::Loan(initializer))?,
                &format!("v{}.initializer", result.index()),
            )?;
            let value = container::generate_borrowed(
                self.context,
                self.llvm,
                &self.builder,
                self.llvm_function,
                container::Initializer::Borrowed {
                    module: self.module,
                    ty,
                    callable,
                },
                self.dependencies.type_map,
                self.dependencies.runtime,
                container,
                self.int_value(length)?,
                &value_name(*result),
            )?;
            self.values.insert(*result, value.into());
            Ok(())
        }
    }
}

pub(crate) mod integer {
    //! Exact-width integer bit operations, with defined LLVM shifts for every source count.
    use super::super::{
        FunctionLowerer, LlvmAdapterError, invalid_result_count, scalar, value_name,
    };
    use crate::ssa::model::{IntegerBitwiseOperator, ValueId};

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        pub(crate) fn lower_integer_bitwise(
            &mut self,
            operator: IntegerBitwiseOperator,
            left: ValueId,
            right: ValueId,
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [result] = results else {
                return Err(invalid_result_count("integer bitwise", 1, results.len()));
            };
            let lhs = self.int_value(left)?;
            let mut rhs = self.int_value(right)?;
            let name = value_name(*result);
            if matches!(
                operator,
                IntegerBitwiseOperator::Shl
                    | IntegerBitwiseOperator::Shr
                    | IntegerBitwiseOperator::Ushr
            ) {
                let mask = lhs
                    .get_type()
                    .const_int(u64::from(lhs.get_type().get_bit_width() - 1), false);
                rhs = self
                    .builder
                    .build_and(rhs, mask, &format!("{name}.count"))?;
            }
            let value = match operator {
                IntegerBitwiseOperator::And => self.builder.build_and(lhs, rhs, &name)?,
                IntegerBitwiseOperator::Or => self.builder.build_or(lhs, rhs, &name)?,
                IntegerBitwiseOperator::Xor => self.builder.build_xor(lhs, rhs, &name)?,
                // No nsw/nuw/exact flags: high bits are discarded, never checked arithmetic.
                IntegerBitwiseOperator::Shl => self.builder.build_left_shift(lhs, rhs, &name)?,
                IntegerBitwiseOperator::Shr => self.builder.build_right_shift(
                    lhs,
                    rhs,
                    scalar::integer_signed(self.module, self.function, left)?,
                    &name,
                )?,
                IntegerBitwiseOperator::Ushr => {
                    self.builder.build_right_shift(lhs, rhs, false, &name)?
                }
            };
            self.values.insert(*result, value.into());
            Ok(())
        }
    }
}

pub(crate) mod integer_conversion {
    //! Checked conversions compare the original mathematical integer before truncation.
    use super::super::{
        FunctionLowerer, IntPredicate, LlvmAdapterError, invalid_result_count, scalar, value_name,
        value_type,
    };
    use crate::ssa::model::ValueId;

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        pub(crate) fn lower_checked_integer_conversion(
            &mut self,
            operand: ValueId,
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [result, failed] = results else {
                return Err(invalid_result_count(
                    "checked integer conversion",
                    2,
                    results.len(),
                ));
            };
            let source = self.int_value(operand)?;
            let source_signed = scalar::integer_signed(self.module, self.function, operand)?;
            let target_signed = scalar::integer_signed(self.module, self.function, *result)?;
            let target = self
                .dependencies
                .type_map
                .int_type(value_type(self.function, *result)?)?;
            let bits = target.get_bit_width();
            let wide = self.context.i128_type();
            let name = value_name(*result);
            let mathematical = if source_signed {
                self.builder
                    .build_int_s_extend(source, wide, &format!("{name}.mathematical"))?
            } else {
                self.builder
                    .build_int_z_extend(source, wide, &format!("{name}.mathematical"))?
            };
            // Every source integer fits i128. Sign-extend a signed i64 minimum, but never
            // sign-extend the unsigned maximum (notably ULong::MAX).
            let minimum = if target_signed {
                -(1_i128 << (bits - 1))
            } else {
                0
            };
            let maximum = (1_u128 << (bits - u32::from(target_signed))) - 1;
            let below = self.builder.build_int_compare(
                IntPredicate::SLT,
                mathematical,
                wide.const_int(minimum as u64, true),
                &format!("{name}.below"),
            )?;
            let above = self.builder.build_int_compare(
                IntPredicate::SGT,
                mathematical,
                wide.const_int(maximum as u64, false),
                &format!("{name}.above"),
            )?;
            let failure = self.builder.build_or(below, above, &value_name(*failed))?;
            // A plain trunc has no poison/side effect. The checked SSA CFG exposes this
            // temporary only on success; failure branches to Abort without delivery.
            let converted = self
                .builder
                .build_int_truncate(mathematical, target, &name)?;
            self.values.insert(*result, converted.into());
            self.values.insert(*failed, failure.into());
            Ok(())
        }
    }
}

pub(crate) mod storage {
    use inkwell::values::{BasicValueEnum, PointerValue};

    use crate::ssa::model::{EntityId, EntityType, LoanId, PlaceAccess, PlaceId, ValueId};

    use super::super::{FunctionLowerer, LlvmAdapterError};

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        pub(crate) fn pointer_value(
            &self,
            id: ValueId,
        ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
            match self.value(id)? {
                BasicValueEnum::PointerValue(value) => Ok(value),
                _ => Err(LlvmAdapterError::InvalidSsa(
                    "heap owner operand 不是 LLVM pointer".to_owned(),
                )),
            }
        }

        pub(crate) fn place(&self, id: PlaceId) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
            self.places
                .get(&id)
                .copied()
                .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM place 映射缺失".to_owned()))
        }

        pub(crate) fn access(
            &self,
            access: PlaceAccess,
        ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
            match access {
                PlaceAccess::Place(place) => self.place(place),
                PlaceAccess::Loan(loan) => {
                    self.loans.get(&loan).copied().ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("LLVM loan 映射缺失".to_owned())
                    })
                }
            }
        }

        /// Resolve a receiver loan to the same heap handle stored by the caller, then project a
        /// payload field. The returned pointer never aliases the receiver binding storage itself.
        pub(crate) fn heap_field_pointer(
            &self,
            receiver: LoanId,
            field: usize,
            name: &str,
        ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
            let EntityType::Loan { target: owner, .. } = self
                .function
                .entity(EntityId::Loan(receiver))
                .ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("heap receiver loan 不存在".to_owned())
                })?
                .ty
            else {
                return Err(LlvmAdapterError::InvalidSsa(
                    "heap receiver operand 不是 loan".to_owned(),
                ));
            };
            let payload = self.module.heap_payload(owner).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("heap receiver loan target 不是 heap owner".to_owned())
            })?;
            let handle = self
                .builder
                .build_load(
                    self.dependencies.type_map.basic_type(owner)?,
                    self.access(PlaceAccess::Loan(receiver))?,
                    &format!("{name}.handle"),
                )?
                .into_pointer_value();
            self.builder
                .build_struct_gep(
                    self.dependencies.type_map.aggregate_type(payload)?,
                    handle,
                    field as u32,
                    name,
                )
                .map_err(Into::into)
        }
    }
}

pub(crate) mod abort {
    //! SPEC-0355: abort reasons and source positions lowered to the runtime abort report.
    use super::super::{FunctionLowerer, IntPredicate, LlvmAdapterError, unsupported};
    use crate::{
        llvm::runtime::AbortMessage,
        ssa::model::{AbortReason, CheckedArithmeticOperator, EntityId, Module, Origin, SsaTypeId},
    };

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        /// SPEC-0355: the source position printed by aborts emitted for this origin.
        pub(crate) fn set_abort_location(&self, origin: &Origin) -> Result<(), LlvmAdapterError> {
            let location = match self.dependencies.debug {
                Some(debug) => Some(debug.abort_location(origin)?),
                None => None,
            };
            self.dependencies.runtime.set_abort_location(location);
            Ok(())
        }

        pub(crate) fn lower_abort(
            &mut self,
            reason: Option<&AbortReason>,
        ) -> Result<(), LlvmAdapterError> {
            let runtime = self.dependencies.runtime;
            match reason {
                Some(AbortReason::Error { message }) => {
                    let view = self.string_view(EntityId::Loan(*message))?;
                    runtime.emit_abort(
                        &self.builder,
                        AbortMessage::Bytes {
                            bytes: view.bytes(),
                            length: view.length(),
                        },
                    )
                }
                Some(AbortReason::CheckedArithmetic { operator, divisor }) => {
                    let overflow = match operator {
                        CheckedArithmeticOperator::Add => "integer overflow in +",
                        CheckedArithmeticOperator::Subtract => "integer overflow in -",
                        CheckedArithmeticOperator::Multiply => "integer overflow in *",
                        CheckedArithmeticOperator::Divide => "integer overflow in /",
                        CheckedArithmeticOperator::Remainder => "integer overflow in %",
                    };
                    match operator {
                        CheckedArithmeticOperator::Divide
                        | CheckedArithmeticOperator::Remainder => {
                            let divisor = self.int_value(*divisor)?;
                            let zero = self.builder.build_int_compare(
                                IntPredicate::EQ,
                                divisor,
                                divisor.get_type().const_zero(),
                                "abort.division_by_zero",
                            )?;
                            runtime.emit_abort(
                                &self.builder,
                                AbortMessage::Select {
                                    condition: zero,
                                    when_true: "division by zero",
                                    when_false: overflow,
                                },
                            )
                        }
                        _ => runtime.emit_abort(&self.builder, AbortMessage::Text(overflow)),
                    }
                }
                Some(AbortReason::CheckedConversion { target }) => {
                    let text = format!(
                        "value out of range for {}",
                        integer_type_name(self.module, *target)?
                    );
                    runtime.emit_abort(&self.builder, AbortMessage::Text(&text))
                }
                Some(AbortReason::Text(text)) => {
                    runtime.emit_abort(&self.builder, AbortMessage::Text(text))
                }
                None => runtime.emit_abort(&self.builder, AbortMessage::Text("abort")),
            }
        }
    }

    /// Koven name of a checked-conversion target for the SPEC-0355 abort message.
    fn integer_type_name(module: &Module, ty: SsaTypeId) -> Result<&'static str, LlvmAdapterError> {
        match module.type_kind(ty) {
            Some(crate::ssa::model::SsaTypeKind::Integer { bits, signed }) => {
                Ok(match (*bits, *signed) {
                    (8, true) => "Byte",
                    (16, true) => "Short",
                    (32, true) => "Int",
                    (64, true) => "Long",
                    (8, false) => "UByte",
                    (16, false) => "UShort",
                    (32, false) => "UInt",
                    (64, false) => "ULong",
                    _ => return Err(unsupported("checked conversion target width")),
                })
            }
            _ => Err(unsupported("checked conversion target is not an integer")),
        }
    }
}

pub(crate) mod map_iteration {
    //! Five explicit SSA Map operations share the real insertion-chain primitives.
    use super::super::*;
    use crate::llvm::map::iteration::{self, Component};

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        pub(crate) fn lower_map_iteration_operation(
            &mut self,
            instruction: &Instruction,
        ) -> Result<(), LlvmAdapterError> {
            let source = match instruction.operation {
                Operation::MapIterationFirst { source }
                | Operation::MapIterationHasNext { source, .. }
                | Operation::MapIterationNext { source, .. }
                | Operation::MapIterationKeyLoan { source, .. }
                | Operation::MapIterationValueLoan { source, .. } => source,
                _ => {
                    return Err(LlvmAdapterError::InvalidSsa(
                        "expected Map iteration operation".into(),
                    ));
                }
            };
            let map_type = access_type(self.function, PlaceAccess::Loan(source))?;
            if self.module.map_container(map_type).is_none() {
                return Err(LlvmAdapterError::InvalidSsa(
                    "Map iteration source is not Map storage".into(),
                ));
            }
            let [result] = instruction.results.as_slice() else {
                return Err(invalid_result_count(
                    "Map iteration",
                    1,
                    instruction.results.len(),
                ));
            };
            let header = self
                .builder
                .build_load(
                    self.dependencies.type_map.basic_type(map_type)?,
                    self.access(PlaceAccess::Loan(source))?,
                    "map.iteration.header",
                )?
                .into_struct_value();
            let slot_source = iteration::SlotSource {
                context: self.context,
                builder: &self.builder,
                function: self.llvm_function,
                types: self.dependencies.type_map,
                runtime: self.dependencies.runtime,
                module: self.module,
                map_type,
                header,
            };
            let name = format!("map.iteration.i{}", instruction.id.index());
            let value = match (&instruction.operation, *result) {
                (Operation::MapIterationFirst { .. }, EntityId::Value(result)) => {
                    let value = iteration::first(
                        self.context,
                        &self.builder,
                        self.dependencies.runtime,
                        header,
                        &name,
                    )?;
                    (result, value)
                }
                (Operation::MapIterationHasNext { cursor, .. }, EntityId::Value(result)) => {
                    let value = iteration::has_next(
                        self.context,
                        &self.builder,
                        self.dependencies.runtime,
                        self.int_value(*cursor)?,
                        &name,
                    )?;
                    (result, value)
                }
                (Operation::MapIterationNext { cursor, .. }, EntityId::Value(result)) => {
                    let value = iteration::next(&slot_source, self.int_value(*cursor)?, &name)?;
                    (result, value)
                }
                (Operation::MapIterationKeyLoan { cursor, .. }, EntityId::Loan(result))
                | (Operation::MapIterationValueLoan { cursor, .. }, EntityId::Loan(result)) => {
                    let component =
                        if matches!(instruction.operation, Operation::MapIterationKeyLoan { .. }) {
                            Component::Key
                        } else {
                            Component::Value
                        };
                    let pointer = iteration::component(
                        &slot_source,
                        self.int_value(*cursor)?,
                        component,
                        &name,
                    )?;
                    self.loans.insert(result, pointer);
                    return Ok(());
                }
                _ => {
                    return Err(LlvmAdapterError::InvalidSsa(
                        "Map iteration result kind mismatch".into(),
                    ));
                }
            };
            self.values.insert(value.0, value.1.into());
            Ok(())
        }
    }
}

pub(crate) mod root_exchange {
    //! Verified exclusive root exchange: move storage without invoking resource glue.

    use inkwell::types::BasicTypeEnum;

    use crate::ssa::model::{SsaTypeId, SsaTypeKind};

    use super::super::{
        FunctionLowerer, LlvmAdapterError, LoanId, PlaceAccess, ValueId, invalid_result_count,
        value_name, value_type,
    };

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        /// Unit has local zero-sized storage for root exchange and CFG values. Function signatures
        /// still use TypeMap directly, so this does not introduce a non-void Unit ABI.
        pub(crate) fn local_storage_type(
            &self,
            ty: SsaTypeId,
        ) -> Result<BasicTypeEnum<'ctx>, LlvmAdapterError> {
            if matches!(self.module.type_kind(ty), Some(SsaTypeKind::Unit)) {
                Ok(self.context.struct_type(&[], false).into())
            } else {
                self.dependencies.type_map.basic_type(ty)
            }
        }

        pub(crate) fn lower_root_replace(
            &mut self,
            owner: ValueId,
            loan: LoanId,
            replacement: ValueId,
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [new_root, old] = results else {
                return Err(invalid_result_count("root replace", 2, results.len()));
            };
            let pointer = self.access(PlaceAccess::Loan(loan))?;
            let replacement = self.value(replacement)?;
            let storage = self.local_storage_type(value_type(self.function, owner)?)?;
            // Read the current root storage, not the owner's original SSA payload. The verifier
            // proves this loan is active, exclusive and exactly covers the owned root.
            let old_value = self
                .builder
                .build_load(storage, pointer, &value_name(*old))?;
            self.builder.build_store(pointer, replacement)?;
            self.loans.remove(&loan);
            self.zero_sized_loans.remove(&loan);
            self.values.insert(*new_root, replacement);
            self.values.insert(*old, old_value);
            Ok(())
        }

        pub(crate) fn lower_heap_field_exchange(
            &mut self,
            loan: LoanId,
            replacement: ValueId,
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [old] = results else {
                return Err(invalid_result_count(
                    "heap field exchange",
                    1,
                    results.len(),
                ));
            };
            let pointer = self.access(PlaceAccess::Loan(loan))?;
            let replacement = self.value(replacement)?;
            let storage = self.local_storage_type(value_type(self.function, *old)?)?;
            // The verified loan denotes this exact direct field of the still-live parent.
            // Load before store; never call drop glue or expose an empty field in between.
            let old_value = self
                .builder
                .build_load(storage, pointer, &value_name(*old))?;
            self.builder.build_store(pointer, replacement)?;
            self.loans.remove(&loan);
            self.zero_sized_loans.remove(&loan);
            self.values.insert(*old, old_value);
            Ok(())
        }

        pub(crate) fn lower_root_swap(
            &mut self,
            owners: [ValueId; 2],
            loans: [LoanId; 2],
            results: &[ValueId],
        ) -> Result<(), LlvmAdapterError> {
            let [new_a, new_b] = results else {
                return Err(invalid_result_count("root swap", 2, results.len()));
            };
            let pointer_a = self.access(PlaceAccess::Loan(loans[0]))?;
            let pointer_b = self.access(PlaceAccess::Loan(loans[1]))?;
            let storage = self.local_storage_type(value_type(self.function, owners[0])?)?;
            // Both old values are loaded before either store. No drop, clone, retain, allocation
            // or other observable call may interrupt the ownership exchange.
            let old_a = self
                .builder
                .build_load(storage, pointer_a, &value_name(*new_b))?;
            let old_b = self
                .builder
                .build_load(storage, pointer_b, &value_name(*new_a))?;
            self.builder.build_store(pointer_a, old_b)?;
            self.builder.build_store(pointer_b, old_a)?;
            for loan in loans {
                self.loans.remove(&loan);
                self.zero_sized_loans.remove(&loan);
            }
            self.values.insert(*new_a, old_b);
            self.values.insert(*new_b, old_a);
            Ok(())
        }
    }
}

pub(crate) mod nullable {
    //! ADR-0033 inline nullable values: `{present, payload}` or a niche-carrying owner.
    use inkwell::values::{BasicValue, BasicValueEnum, IntValue};

    use super::super::{FunctionLowerer, LlvmAdapterError, access_type, value_name, value_type};
    use crate::ssa::model::{LoanId, PlaceAccess, SsaTypeId, ValueId};

    impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
        FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    {
        /// `Some` for an inline nullable result: the niche form is the inner value itself.
        pub(crate) fn inline_nullable_wrap(
            &self,
            result: ValueId,
            owner: ValueId,
        ) -> Result<Option<BasicValueEnum<'ctx>>, LlvmAdapterError> {
            let ty = value_type(self.function, result)?;
            let Some((_, niche)) = self.module.inline_nullable(ty) else {
                return Ok(None);
            };
            let value = self.value(owner)?;
            if niche {
                return Ok(Some(value));
            }
            let header = self
                .dependencies
                .type_map
                .basic_type(ty)?
                .into_struct_type()
                .const_zero();
            let present = self.context.bool_type().const_int(1, false);
            let header = self
                .builder
                .build_insert_value(header, present, 0, "nullable.present")?;
            Ok(Some(
                self.builder
                    .build_insert_value(header, value, 1, &value_name(result))?
                    .as_basic_value_enum(),
            ))
        }

        /// `Some((inner, payload))` for an inline nullable operand on a proven non-null edge.
        pub(crate) fn inline_nullable_payload(
            &self,
            owner: ValueId,
        ) -> Result<Option<(SsaTypeId, BasicValueEnum<'ctx>)>, LlvmAdapterError> {
            let Some((inner, niche)) = self
                .module
                .inline_nullable(value_type(self.function, owner)?)
            else {
                return Ok(None);
            };
            let payload = if niche {
                self.value(owner)?
            } else {
                self.builder.build_extract_value(
                    self.struct_value(owner)?,
                    1,
                    "nullable.payload",
                )?
            };
            Ok(Some((inner, payload)))
        }

        /// Read only the presence field (or the handle itself) through the existing loan.
        pub(crate) fn nullable_loan_is_null(
            &self,
            source: LoanId,
            name: &str,
        ) -> Result<IntValue<'ctx>, LlvmAdapterError> {
            let source = PlaceAccess::Loan(source);
            let ty = access_type(self.function, source)?;
            let storage = self.access(source)?;
            let llvm_type = self.dependencies.type_map.basic_type(ty)?;
            let presence_niche = self
                .module
                .inline_nullable(ty)
                .map(|(_, niche)| niche)
                .or_else(|| self.module.map_result_value(ty).map(|_| false));
            let Some(niche) = presence_niche else {
                let handle =
                    self.builder
                        .build_load(llvm_type, storage, "nullable.borrowed.handle")?;
                return Ok(self
                    .builder
                    .build_is_null(handle.into_pointer_value(), name)?);
            };
            let wrapper = llvm_type.into_struct_type();
            let presence_type = wrapper.get_field_type_at_index(0).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("inline nullable requires a presence field".into())
            })?;
            let presence_slot = self.builder.build_struct_gep(
                wrapper,
                storage,
                0,
                "nullable.borrowed.presence.slot",
            )?;
            let presence = self.builder.build_load(
                presence_type,
                presence_slot,
                "nullable.borrowed.presence",
            )?;
            if niche {
                Ok(self
                    .builder
                    .build_is_null(presence.into_pointer_value(), name)?)
            } else {
                Ok(self.builder.build_not(presence.into_int_value(), name)?)
            }
        }

        pub(crate) fn inline_nullable_is_null(
            &self,
            owner: ValueId,
            name: &str,
        ) -> Result<Option<IntValue<'ctx>>, LlvmAdapterError> {
            let Some((_, niche)) = self
                .module
                .inline_nullable(value_type(self.function, owner)?)
            else {
                return Ok(None);
            };
            let wrapper = self.struct_value(owner)?;
            let is_null = if niche {
                // String and container owners keep a non-null data pointer in field 0.
                let data = self
                    .builder
                    .build_extract_value(wrapper, 0, "nullable.data")?
                    .into_pointer_value();
                self.builder.build_is_null(data, name)?
            } else {
                let present = self
                    .builder
                    .build_extract_value(wrapper, 0, "nullable.present")?
                    .into_int_value();
                self.builder.build_not(present, name)?
            };
            Ok(Some(is_null))
        }
    }
}
