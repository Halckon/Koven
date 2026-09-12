//! verified typed SSA 到 LLVM IR 的 first-class value 适配器。

mod callable;
mod module_lowering;
mod storage;

use std::collections::BTreeMap;

use inkwell::{IntPredicate, basic_block::BasicBlock, builder::Builder, context::Context};
use inkwell::{
    intrinsics::Intrinsic,
    module::{Linkage, Module as LlvmModule},
    types::{BasicMetadataTypeEnum, BasicType},
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PhiValue, PointerValue,
        StructValue, ValueKind,
    },
};

use crate::ssa::model::{
    BinaryOperator, BlockId, CheckedArithmeticOperator, ComparisonOperator, Edge, EntityId,
    EntityType, Function, FunctionId, Instruction, LoanId, Module, Operation, Ownership,
    PlaceAccess, PlaceId, ScalarConstant, SsaTypeId, TerminatorKind, ValueId,
};

use super::{
    LlvmAdapterError, aggregate, closure, container,
    debug::DebugEmitter,
    entities::{
        access_type, loan_result, place_result, place_type, value_name, value_results, value_type,
    },
    entry,
    runtime::RuntimeAbi,
    scalar, string, tagged,
    type_map::TypeMap,
};

#[cfg(test)]
pub(super) use module_lowering::render_verified_program_with_debug;
pub(super) use module_lowering::{emit_verified_object, render_verified_program};

struct ModuleLowerer<'ctx, 'llvm, 'ssa, 'sources> {
    context: &'ctx Context,
    llvm: &'llvm LlvmModule<'ctx>,
    ssa: &'ssa Module,
    type_map: TypeMap<'ctx>,
    runtime: RuntimeAbi<'ctx>,
    functions: BTreeMap<FunctionId, FunctionValue<'ctx>>,
    debug: Option<DebugEmitter<'ctx, 'sources>>,
}

impl<'ctx, 'llvm, 'ssa, 'sources> ModuleLowerer<'ctx, 'llvm, 'ssa, 'sources> {
    fn new(
        context: &'ctx Context,
        llvm: &'llvm LlvmModule<'ctx>,
        ssa: &'ssa Module,
        target: &inkwell::targets::TargetData,
        debug: Option<DebugEmitter<'ctx, 'sources>>,
        native_entry: Option<entry::NativeEntryPlan>,
    ) -> Result<Self, LlvmAdapterError> {
        let type_map = TypeMap::lower(context, ssa, target)?;
        let runtime = RuntimeAbi::lower(context, llvm, ssa, &type_map, target, native_entry)?;
        Ok(Self {
            context,
            llvm,
            ssa,
            type_map,
            runtime,
            functions: BTreeMap::new(),
            debug,
        })
    }

    fn lower(
        mut self,
        native_entry: Option<entry::NativeEntryPlan>,
    ) -> Result<(), LlvmAdapterError> {
        self.declare_functions()?;
        for function in &self.ssa.functions {
            let llvm_function = *self
                .functions
                .get(&function.id)
                .ok_or_else(|| LlvmAdapterError::Build("缺少已声明 LLVM function".to_owned()))?;
            FunctionLowerer::new(
                self.context,
                self.llvm,
                self.ssa,
                function,
                llvm_function,
                FunctionDependencies {
                    type_map: &self.type_map,
                    runtime: &self.runtime,
                    functions: &self.functions,
                    debug: self.debug.as_ref(),
                },
            )
            .lower()?;
        }
        if let Some(native_entry) = native_entry {
            let target = *self
                .functions
                .get(&native_entry.function())
                .ok_or_else(|| {
                    LlvmAdapterError::InvalidEntry(
                        "validated entry declaration is missing".to_owned(),
                    )
                })?;
            entry::define_wrapper(
                self.context,
                self.llvm,
                target,
                native_entry,
                &self.type_map,
                &self.runtime,
            )?;
        }
        if let Some(debug) = &self.debug {
            debug.finalize();
        }
        Ok(())
    }

    fn declare_functions(&mut self) -> Result<(), LlvmAdapterError> {
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
            let function_type = match function.return_types.as_slice() {
                [] => self.context.void_type().fn_type(&parameters, false),
                [result] => self
                    .type_map
                    .basic_type(*result)?
                    .fn_type(&parameters, false),
                _ => return Err(unsupported("当前 LLVM adapter 不支持多返回值 ABI")),
            };
            let name = format!("f{}.{}", function.id.index(), function.name);
            let llvm_function =
                self.llvm
                    .add_function(&name, function_type, Some(Linkage::Internal));
            if let Some(debug) = &mut self.debug {
                debug.attach_function(function, llvm_function)?;
            }
            self.functions.insert(function.id, llvm_function);
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct FunctionDependencies<'ctx, 'functions, 'sources> {
    type_map: &'functions TypeMap<'ctx>,
    runtime: &'functions RuntimeAbi<'ctx>,
    functions: &'functions BTreeMap<FunctionId, FunctionValue<'ctx>>,
    debug: Option<&'functions DebugEmitter<'ctx, 'sources>>,
}

struct FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources> {
    context: &'ctx Context,
    llvm: &'llvm LlvmModule<'ctx>,
    module: &'ssa Module,
    function: &'ssa Function,
    llvm_function: FunctionValue<'ctx>,
    dependencies: FunctionDependencies<'ctx, 'functions, 'sources>,
    builder: Builder<'ctx>,
    blocks: BTreeMap<BlockId, BasicBlock<'ctx>>,
    values: BTreeMap<ValueId, BasicValueEnum<'ctx>>,
    places: BTreeMap<PlaceId, PointerValue<'ctx>>,
    loans: BTreeMap<LoanId, PointerValue<'ctx>>,
    zero_sized_places: BTreeMap<PlaceId, BasicValueEnum<'ctx>>,
    zero_sized_loans: BTreeMap<LoanId, BasicValueEnum<'ctx>>,
    phis: BTreeMap<ValueId, PhiValue<'ctx>>,
    loan_phis: BTreeMap<LoanId, PhiValue<'ctx>>,
    place_phis: BTreeMap<PlaceId, PhiValue<'ctx>>,
}

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    fn new(
        context: &'ctx Context,
        llvm: &'llvm LlvmModule<'ctx>,
        module: &'ssa Module,
        function: &'ssa Function,
        llvm_function: FunctionValue<'ctx>,
        dependencies: FunctionDependencies<'ctx, 'functions, 'sources>,
    ) -> Self {
        Self {
            context,
            llvm,
            module,
            function,
            llvm_function,
            dependencies,
            builder: context.create_builder(),
            blocks: BTreeMap::new(),
            values: BTreeMap::new(),
            places: BTreeMap::new(),
            loans: BTreeMap::new(),
            zero_sized_places: BTreeMap::new(),
            zero_sized_loans: BTreeMap::new(),
            phis: BTreeMap::new(),
            loan_phis: BTreeMap::new(),
            place_phis: BTreeMap::new(),
        }
    }

    fn lower(mut self) -> Result<(), LlvmAdapterError> {
        self.create_blocks_and_parameters()?;
        for block in &self.function.blocks {
            let llvm_block = self.block(block.id)?;
            self.builder.position_at_end(llvm_block);
            self.builder.unset_current_debug_location();
            for instruction in &block.instructions {
                let instruction = self.function.instruction(*instruction).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("block 引用未知 instruction".to_owned())
                })?;
                self.set_debug_location(&instruction.origin)?;
                self.lower_instruction(instruction)?;
            }
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("basic block 缺少 terminator".to_owned())
            })?;
            self.set_debug_location(&terminator.origin)?;
            self.lower_terminator(&terminator.kind)?;
        }
        self.builder.unset_current_debug_location();
        Ok(())
    }

    fn create_blocks_and_parameters(&mut self) -> Result<(), LlvmAdapterError> {
        for block in &self.function.blocks {
            let llvm_block = self
                .context
                .append_basic_block(self.llvm_function, &format!("bb{}", block.id.index()));
            self.blocks.insert(block.id, llvm_block);
        }

        let entry =
            self.function.blocks.first().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("function 缺少 entry block".to_owned())
            })?;
        if entry.parameters.len() != self.llvm_function.count_params() as usize {
            return Err(LlvmAdapterError::InvalidSsa(
                "SSA entry 参数与 LLVM 签名数量不一致".to_owned(),
            ));
        }
        for (index, entity) in entry.parameters.iter().enumerate() {
            let llvm_value = self
                .llvm_function
                .get_nth_param(index as u32)
                .ok_or_else(|| LlvmAdapterError::Build("缺少 LLVM function 参数".to_owned()))?;
            match entity {
                EntityId::Value(value) => {
                    llvm_value.set_name(&value_name(*value));
                    self.values.insert(*value, llvm_value);
                }
                EntityId::Loan(loan) => {
                    let BasicValueEnum::PointerValue(pointer) = llvm_value else {
                        return Err(LlvmAdapterError::InvalidSsa(
                            "LLVM Borrow 参数不是 pointer".to_owned(),
                        ));
                    };
                    pointer.set_name(&format!("l{}", loan.index()));
                    self.loans.insert(*loan, pointer);
                }
                EntityId::Place(_) => {
                    return Err(unsupported("LLVM entry 参数不能是 place"));
                }
            }
        }

        for block in self.function.blocks.iter().skip(1) {
            self.builder.position_at_end(self.block(block.id)?);
            self.set_debug_location(&block.origin)?;
            for entity in &block.parameters {
                match entity {
                    EntityId::Value(value) => {
                        let phi = self.builder.build_phi(
                            self.dependencies
                                .type_map
                                .basic_type(value_type(self.function, *value)?)?,
                            &value_name(*value),
                        )?;
                        self.values.insert(*value, phi.as_basic_value());
                        self.phis.insert(*value, phi);
                    }
                    EntityId::Loan(loan) => {
                        let phi = self.builder.build_phi(
                            self.context.ptr_type(inkwell::AddressSpace::default()),
                            &format!("l{}", loan.index()),
                        )?;
                        self.loans
                            .insert(*loan, phi.as_basic_value().into_pointer_value());
                        self.loan_phis.insert(*loan, phi);
                    }
                    EntityId::Place(place) => {
                        // 跨块传递同一存储地址，不能重新建立 RootPlace 并写入旧值。
                        let phi = self.builder.build_phi(
                            self.context.ptr_type(inkwell::AddressSpace::default()),
                            &format!("p{}", place.index()),
                        )?;
                        self.places
                            .insert(*place, phi.as_basic_value().into_pointer_value());
                        self.place_phis.insert(*place, phi);
                    }
                }
            }
        }
        Ok(())
    }

    fn set_debug_location(
        &self,
        origin: &crate::ssa::model::Origin,
    ) -> Result<(), LlvmAdapterError> {
        if let Some(debug) = self.dependencies.debug {
            debug.set_location(self.context, &self.builder, self.function.id, origin)?;
        }
        Ok(())
    }

    fn lower_instruction(&mut self, instruction: &Instruction) -> Result<(), LlvmAdapterError> {
        let results = if matches!(
            instruction.operation,
            Operation::TaggedPayloadPlace { .. }
                | Operation::HeapPayloadPlace { .. }
                | Operation::SharedPayloadPlace { .. }
                | Operation::FieldPlace { .. }
                | Operation::SharedFieldLoan { .. }
                | Operation::SharedHeapFieldLoan { .. }
                | Operation::SharedReborrow { .. }
                | Operation::ContainerElementPlace { .. }
                | Operation::RootPlace { .. }
                | Operation::BorrowBegin { .. }
        ) {
            Vec::new()
        } else {
            value_results(instruction)?
        };
        match &instruction.operation {
            Operation::Constant(constant) => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("constant", 1, results.len()));
                };
                let ty = self
                    .dependencies
                    .type_map
                    .int_type(value_type(self.function, *result)?)?;
                let value = match constant {
                    ScalarConstant::Boolean(value) => ty.const_int(u64::from(*value), false),
                    ScalarConstant::Char(value) => ty.const_int(u64::from(*value), false),
                    ScalarConstant::Integer(value) => ty.const_int(*value as u64, *value < 0),
                    ScalarConstant::Unit => {
                        return Err(unsupported("Unit constant 不产生 LLVM payload"));
                    }
                };
                self.values.insert(*result, value.into());
            }
            Operation::PrintLiteral { bytes } => {
                if !results.is_empty() {
                    return Err(invalid_result_count("print literal", 0, results.len()));
                }
                self.dependencies.runtime.emit_print_literal(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    bytes,
                    &format!(
                        "print.f{}.i{}",
                        self.function.id.index(),
                        instruction.id.index()
                    ),
                )?;
            }
            Operation::StringLiteral { string, bytes } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string literal", 1, results.len()));
                };
                let value = string::literal(
                    self.llvm,
                    &self.builder,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    *string,
                    bytes,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::StringConcat { left, right } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string concat", 1, results.len()));
                };
                let value = string::concat(
                    self.context,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    self.dependencies
                        .type_map
                        .basic_type(value_type(self.function, *result)?)?
                        .into_struct_type(),
                    self.string_view(*left)?,
                    self.string_view(*right)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::StringEqual { left, right } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string equal", 1, results.len()));
                };
                let value = string::equal(
                    self.context,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    self.string_view(*left)?,
                    self.string_view(*right)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::PrintString { value } => {
                if !results.is_empty() {
                    return Err(invalid_result_count("print string", 0, results.len()));
                }
                string::print(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    self.string_view(EntityId::Loan(*value))?,
                    &format!(
                        "print.f{}.i{}",
                        self.function.id.index(),
                        instruction.id.index()
                    ),
                )?;
            }
            Operation::Binary {
                operator,
                left,
                right,
            } => self.lower_binary(*operator, *left, *right, &results)?,
            Operation::CheckedArithmetic {
                operator,
                left,
                right,
            } => self.lower_checked(*operator, *left, *right, &results)?,
            Operation::Compare {
                operator,
                left,
                right,
            } => self.lower_comparison(*operator, *left, *right, &results)?,
            Operation::BooleanNot { operand } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("boolean not", 1, results.len()));
                };
                let value = self
                    .builder
                    .build_not(self.int_value(*operand)?, &value_name(*result))?;
                self.values.insert(*result, value.into());
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                let operands = receiver
                    .iter()
                    .copied()
                    .chain(arguments.iter().copied())
                    .collect::<Vec<_>>();
                self.lower_call(*callee, &operands, &results)?;
            }
            Operation::FunctionAddress { target } => {
                self.lower_function_address(*target, &results)?;
            }
            Operation::ClosureConstruct {
                closure: closure_type,
                thunk,
                captures,
            } => {
                self.lower_closure_construct(*closure_type, *thunk, captures, &results)?;
            }
            Operation::CallableInvoke {
                callable,
                arguments,
            } => {
                self.lower_callable_invoke(*callable, arguments, &results)?;
            }
            Operation::AggregateConstruct { aggregate, fields } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count(
                        "aggregate construct",
                        1,
                        results.len(),
                    ));
                };
                let fields = fields
                    .iter()
                    .map(|field| self.value(*field))
                    .collect::<Result<Vec<_>, _>>()?;
                let value = aggregate::construct(
                    &self.builder,
                    self.dependencies.type_map,
                    *aggregate,
                    &fields,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::AggregateProject { aggregate, field } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("aggregate project", 1, results.len()));
                };
                let value = aggregate::project(
                    &self.builder,
                    self.struct_value(*aggregate)?,
                    *field,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value);
            }
            Operation::AggregateExplode { aggregate } => {
                let names = results.iter().copied().map(value_name).collect::<Vec<_>>();
                let values =
                    aggregate::explode(&self.builder, self.struct_value(*aggregate)?, &names)?;
                self.values.extend(results.iter().copied().zip(values));
            }
            Operation::AggregateCopyExplode { aggregate } => {
                let names = results.iter().copied().map(value_name).collect::<Vec<_>>();
                let values =
                    aggregate::explode(&self.builder, self.struct_value(*aggregate)?, &names)?;
                self.values.extend(results.iter().copied().zip(values));
            }
            Operation::TaggedConstruct {
                tagged: tagged_type,
                variant,
                payload,
            } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("tagged construct", 1, results.len()));
                };
                let value = tagged::construct(
                    &self.builder,
                    self.dependencies.type_map,
                    *tagged_type,
                    *variant,
                    self.struct_value(*payload)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::TaggedPayloadPlace { owner, variant } => {
                let result = place_result(instruction)?;
                let tagged_type = value_type(self.function, *owner)?;
                let pointer = tagged::payload_place(
                    &self.builder,
                    self.dependencies.type_map,
                    tagged_type,
                    *variant,
                    self.struct_value(*owner)?,
                    &format!("p{}", result.index()),
                )?;
                self.places.insert(result, pointer);
            }
            Operation::TaggedDiscriminant { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count(
                        "tagged discriminant",
                        1,
                        results.len(),
                    ));
                };
                let tagged_type = value_type(self.function, *owner)?;
                let tag = tagged::discriminant(
                    &self.builder,
                    self.dependencies.type_map,
                    tagged_type,
                    self.struct_value(*owner)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, tag.into());
            }
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
            Operation::ContainerLength { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("container length", 1, results.len()));
                };
                let value = container::length(
                    &self.builder,
                    self.struct_value(*owner)?,
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
                self.values.insert(*result, self.value(*owner)?);
            }
            Operation::NullableNull { .. } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("nullable null", 1, results.len()));
                };
                let null = self
                    .context
                    .ptr_type(inkwell::AddressSpace::default())
                    .const_null();
                self.values.insert(*result, null.into());
            }
            Operation::NullableIsNull { owner } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("nullable is-null", 1, results.len()));
                };
                let is_null = self
                    .builder
                    .build_is_null(self.pointer_value(*owner)?, &value_name(*result))?;
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
                self.values.insert(*result, self.value(*owner)?);
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
                    self.dependencies
                        .type_map
                        .basic_type(value_type(self.function, *owner)?)?,
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
                        self.dependencies.type_map.basic_type(source_type)?,
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
        }
        Ok(())
    }

    fn lower_binary(
        &mut self,
        operator: BinaryOperator,
        left: ValueId,
        right: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("binary", 1, results.len()));
        };
        let left_value = self.int_value(left)?;
        let right_value = self.int_value(right)?;
        let name = value_name(*result);
        let value = match operator {
            BinaryOperator::Add => self.builder.build_int_add(left_value, right_value, &name)?,
            BinaryOperator::Subtract => {
                self.builder.build_int_sub(left_value, right_value, &name)?
            }
            BinaryOperator::Multiply => {
                self.builder.build_int_mul(left_value, right_value, &name)?
            }
            BinaryOperator::Equal => {
                self.builder
                    .build_int_compare(IntPredicate::EQ, left_value, right_value, &name)?
            }
            BinaryOperator::LessThan => self.builder.build_int_compare(
                scalar::ordering_predicate(
                    self.module,
                    self.function,
                    left,
                    IntPredicate::SLT,
                    IntPredicate::ULT,
                )?,
                left_value,
                right_value,
                &name,
            )?,
        };
        self.values.insert(*result, value.into());
        Ok(())
    }

    fn lower_checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result, failed] = results else {
            return Err(invalid_result_count("checked arithmetic", 2, results.len()));
        };
        match operator {
            CheckedArithmeticOperator::Add
            | CheckedArithmeticOperator::Subtract
            | CheckedArithmeticOperator::Multiply => {
                self.lower_overflow_intrinsic(operator, left, right, *result, *failed)
            }
            CheckedArithmeticOperator::Divide | CheckedArithmeticOperator::Remainder => {
                self.lower_checked_division(operator, left, right, *result, *failed)
            }
        }
    }

    fn lower_overflow_intrinsic(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        result: ValueId,
        failed: ValueId,
    ) -> Result<(), LlvmAdapterError> {
        let signed = scalar::integer_signed(self.module, self.function, left)?;
        let intrinsic_name = match (operator, signed) {
            (CheckedArithmeticOperator::Add, true) => "llvm.sadd.with.overflow",
            (CheckedArithmeticOperator::Add, false) => "llvm.uadd.with.overflow",
            (CheckedArithmeticOperator::Subtract, true) => "llvm.ssub.with.overflow",
            (CheckedArithmeticOperator::Subtract, false) => "llvm.usub.with.overflow",
            (CheckedArithmeticOperator::Multiply, true) => "llvm.smul.with.overflow",
            (CheckedArithmeticOperator::Multiply, false) => "llvm.umul.with.overflow",
            _ => return Err(unsupported("非法 overflow intrinsic operator")),
        };
        let int_type = self
            .dependencies
            .type_map
            .int_type(value_type(self.function, left)?)?;
        let intrinsic = Intrinsic::find(intrinsic_name)
            .and_then(|intrinsic| intrinsic.get_declaration(self.llvm, &[int_type.into()]))
            .ok_or_else(|| {
                LlvmAdapterError::Build(format!("无法声明 LLVM intrinsic {intrinsic_name}"))
            })?;
        let arguments = [
            BasicMetadataValueEnum::from(self.int_value(left)?),
            BasicMetadataValueEnum::from(self.int_value(right)?),
        ];
        let aggregate = match self
            .builder
            .build_call(
                intrinsic,
                &arguments,
                &format!("checked.{}", result.index()),
            )?
            .try_as_basic_value()
        {
            ValueKind::Basic(value) => value.into_struct_value(),
            ValueKind::Instruction(_) => {
                return Err(LlvmAdapterError::Build(
                    "overflow intrinsic 未返回 aggregate".to_owned(),
                ));
            }
        };
        let value = self
            .builder
            .build_extract_value(aggregate, 0, &value_name(result))?
            .into_int_value();
        let overflow = self
            .builder
            .build_extract_value(aggregate, 1, &value_name(failed))?
            .into_int_value();
        self.values.insert(result, value.into());
        self.values.insert(failed, overflow.into());
        Ok(())
    }

    fn lower_checked_division(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        result: ValueId,
        failed: ValueId,
    ) -> Result<(), LlvmAdapterError> {
        let left_value = self.int_value(left)?;
        let right_value = self.int_value(right)?;
        let ty = left_value.get_type();
        let zero = ty.const_zero();
        let one = ty.const_int(1, false);
        let is_zero = self.builder.build_int_compare(
            IntPredicate::EQ,
            right_value,
            zero,
            &format!("v{}.zero", failed.index()),
        )?;
        let signed = scalar::integer_signed(self.module, self.function, left)?;
        let failure = if signed {
            let bits = ty.get_bit_width();
            let minimum = ty.const_int(1_u64 << (bits - 1), false);
            let minus_one = ty.const_all_ones();
            let is_min = self.builder.build_int_compare(
                IntPredicate::EQ,
                left_value,
                minimum,
                &format!("v{}.min", failed.index()),
            )?;
            let is_minus_one = self.builder.build_int_compare(
                IntPredicate::EQ,
                right_value,
                minus_one,
                &format!("v{}.minus_one", failed.index()),
            )?;
            let signed_overflow = self.builder.build_and(
                is_min,
                is_minus_one,
                &format!("v{}.signed_overflow", failed.index()),
            )?;
            self.builder
                .build_or(is_zero, signed_overflow, &value_name(failed))?
        } else {
            is_zero.set_name(&value_name(failed));
            is_zero
        };
        let safe_right = self
            .builder
            .build_select(
                failure,
                one,
                right_value,
                &format!("v{}.safe_rhs", result.index()),
            )?
            .into_int_value();
        let value = match (operator, signed) {
            (CheckedArithmeticOperator::Divide, true) => {
                self.builder
                    .build_int_signed_div(left_value, safe_right, &value_name(result))?
            }
            (CheckedArithmeticOperator::Divide, false) => {
                self.builder
                    .build_int_unsigned_div(left_value, safe_right, &value_name(result))?
            }
            (CheckedArithmeticOperator::Remainder, true) => {
                self.builder
                    .build_int_signed_rem(left_value, safe_right, &value_name(result))?
            }
            (CheckedArithmeticOperator::Remainder, false) => {
                self.builder
                    .build_int_unsigned_rem(left_value, safe_right, &value_name(result))?
            }
            _ => return Err(unsupported("非法 checked division operator")),
        };
        self.values.insert(result, value.into());
        self.values.insert(failed, failure.into());
        Ok(())
    }

    fn lower_comparison(
        &mut self,
        operator: ComparisonOperator,
        left: ValueId,
        right: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("comparison", 1, results.len()));
        };
        let predicate = scalar::comparison_predicate(self.module, self.function, left, operator)?;
        let value = self.builder.build_int_compare(
            predicate,
            self.int_value(left)?,
            self.int_value(right)?,
            &value_name(*result),
        )?;
        self.values.insert(*result, value.into());
        Ok(())
    }

    fn lower_call(
        &mut self,
        callee: FunctionId,
        arguments: &[EntityId],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let llvm_callee =
            *self.dependencies.functions.get(&callee).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("direct call target 不存在".to_owned())
            })?;
        let arguments = arguments
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
            .collect::<Result<Vec<_>, _>>()?;
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

    fn lower_terminator(&mut self, terminator: &TerminatorKind) -> Result<(), LlvmAdapterError> {
        let llvm_source = self.builder.get_insert_block().ok_or_else(|| {
            LlvmAdapterError::Build("terminator lowering 缺少当前 LLVM block".to_owned())
        })?;
        match terminator {
            TerminatorKind::Branch(edge) => {
                self.add_edge_incoming(llvm_source, edge)?;
                self.builder
                    .build_unconditional_branch(self.block(edge.target)?)?;
            }
            TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                if when_true.target == when_false.target
                    && when_true.arguments != when_false.arguments
                {
                    return Err(unsupported(
                        "同一 conditional predecessor 到同一 PHI target 的实参必须相同",
                    ));
                }
                self.add_edge_incoming(llvm_source, when_true)?;
                if when_true.target != when_false.target
                    || when_true.arguments != when_false.arguments
                {
                    self.add_edge_incoming(llvm_source, when_false)?;
                }
                self.builder.build_conditional_branch(
                    self.int_value(*condition)?,
                    self.block(when_true.target)?,
                    self.block(when_false.target)?,
                )?;
            }
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => {
                self.add_edge_incoming(llvm_source, when_null)?;
                self.add_edge_incoming(llvm_source, when_non_null)?;
                let pointer = self.pointer_value(*owner)?;
                let phi = self.loan_phis.get(view).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("non-null view 缺少 LLVM PHI".to_owned())
                })?;
                phi.add_incoming(&[(&pointer, llvm_source)]);
                let is_null = self.builder.build_is_null(pointer, "nullable.is_null")?;
                self.builder.build_conditional_branch(
                    is_null,
                    self.block(when_null.target)?,
                    self.block(when_non_null.target)?,
                )?;
            }
            TerminatorKind::Return { values } => match values.as_slice() {
                [] => {
                    self.builder.build_return(None)?;
                }
                [value] => {
                    let value = self.value(*value)?;
                    self.builder.build_return(Some(&value))?;
                }
                _ => return Err(unsupported("SPEC-0034 不支持多返回值 return")),
            },
            TerminatorKind::Abort => {
                self.dependencies.runtime.emit_abort(&self.builder)?;
            }
        }
        Ok(())
    }

    fn add_edge_incoming(
        &self,
        source: BasicBlock<'ctx>,
        edge: &Edge,
    ) -> Result<(), LlvmAdapterError> {
        let target = self
            .function
            .block(edge.target)
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("edge target block 不存在".to_owned()))?;
        for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
            match (argument, parameter) {
                (EntityId::Value(argument), EntityId::Value(parameter)) => {
                    let value = self.value(*argument)?;
                    let phi = self.phis.get(parameter).ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("非 entry value 参数缺少 LLVM PHI".to_owned())
                    })?;
                    phi.add_incoming(&[(&value, source)]);
                }
                (EntityId::Loan(argument), EntityId::Loan(parameter)) => {
                    let value = self.loans.get(argument).copied().ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("LLVM edge loan 映射缺失".to_owned())
                    })?;
                    let phi = self.loan_phis.get(parameter).ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("非 entry loan 参数缺少 LLVM PHI".to_owned())
                    })?;
                    phi.add_incoming(&[(&value, source)]);
                }
                (EntityId::Place(argument), EntityId::Place(parameter)) => {
                    let value = self.access(PlaceAccess::Place(*argument))?;
                    let phi = self.place_phis.get(parameter).ok_or_else(|| {
                        LlvmAdapterError::InvalidSsa("非 entry place 参数缺少 LLVM PHI".to_owned())
                    })?;
                    phi.add_incoming(&[(&value, source)]);
                }
                _ => return Err(unsupported("LLVM edge 不支持不同 entity kind")),
            }
        }
        Ok(())
    }

    fn block(&self, id: BlockId) -> Result<BasicBlock<'ctx>, LlvmAdapterError> {
        self.blocks
            .get(&id)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM basic block 映射缺失".to_owned()))
    }

    fn value(&self, id: ValueId) -> Result<BasicValueEnum<'ctx>, LlvmAdapterError> {
        self.values
            .get(&id)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM value 映射缺失".to_owned()))
    }

    fn string_view(&self, entity: EntityId) -> Result<string::StringView<'ctx>, LlvmAdapterError> {
        let value = match entity {
            EntityId::Value(value) => self.value(value)?.into_struct_value(),
            EntityId::Loan(loan) => {
                let ty = access_type(self.function, PlaceAccess::Loan(loan))?;
                self.builder
                    .build_load(
                        self.dependencies.type_map.basic_type(ty)?,
                        self.access(PlaceAccess::Loan(loan))?,
                        &format!("l{}.string", loan.index()),
                    )?
                    .into_struct_value()
            }
            EntityId::Place(_) => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "string view 不能是裸 place".to_owned(),
                ));
            }
        };
        string::view(&self.builder, value, "string.view")
    }

    fn shared_owner_type(&self, owner: EntityId) -> Result<SsaTypeId, LlvmAdapterError> {
        match self
            .function
            .entity(owner)
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("shared owner 不存在".to_owned()))?
            .ty
        {
            EntityType::Value(ty) | EntityType::Loan { target: ty, .. } => Ok(ty),
            EntityType::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                "shared owner 不能是 place".to_owned(),
            )),
        }
    }

    fn shared_owner_pointer(
        &self,
        owner: EntityId,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        match owner {
            EntityId::Value(owner) => self.pointer_value(owner),
            EntityId::Loan(owner) => self.loans.get(&owner).copied().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("shared owner loan 映射缺失".to_owned())
            }),
            EntityId::Place(_) => Err(LlvmAdapterError::InvalidSsa(
                "shared owner 不能是 place".to_owned(),
            )),
        }
    }

    fn int_value(&self, id: ValueId) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        match self.value(id)? {
            BasicValueEnum::IntValue(value) => Ok(value),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "integer operation 的 operand 不是 LLVM integer".to_owned(),
            )),
        }
    }

    fn struct_value(&self, id: ValueId) -> Result<StructValue<'ctx>, LlvmAdapterError> {
        match self.value(id)? {
            BasicValueEnum::StructValue(value) => Ok(value),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "aggregate operation 的 operand 不是 LLVM struct".to_owned(),
            )),
        }
    }
}

fn invalid_result_count(operation: &str, expected: usize, actual: usize) -> LlvmAdapterError {
    LlvmAdapterError::InvalidSsa(format!(
        "{operation} 结果数量错误：期望 {expected}，实际 {actual}"
    ))
}

fn unsupported(message: &str) -> LlvmAdapterError {
    LlvmAdapterError::Unsupported(message.to_owned())
}
