//! verified typed SSA 到 LLVM IR 的 first-class value 适配器。

#[allow(unused_imports)]
pub(crate) use module_lowering::abort;
#[allow(unused_imports)]
pub(crate) use module_lowering::borrowed_generate;
pub(crate) mod call;
#[allow(unused_imports)]
pub(crate) use call::integer_range;
#[allow(unused_imports)]
pub(crate) use module_lowering::constant;
#[allow(unused_imports)]
pub(crate) use module_lowering::integer;
#[allow(unused_imports)]
pub(crate) use module_lowering::integer_conversion;
#[allow(unused_imports)]
pub(crate) use range::callable;
mod instruction;
mod map;
#[allow(unused_imports)]
pub(crate) use module_lowering::map_iteration;
pub(crate) mod module_lowering;
#[allow(unused_imports)]
pub(crate) use module_lowering::nullable;
pub(crate) mod range;
#[allow(unused_imports)]
pub(crate) use module_lowering::root_exchange;
#[cfg(test)]
mod root_exchange_tests;
#[allow(unused_imports)]
pub(crate) use call::text;
#[allow(unused_imports)]
pub(crate) use call::text_copy;
#[allow(unused_imports)]
pub(crate) use module_lowering::storage;

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
        self.runtime
            .define_drop_functions(self.ssa, &self.type_map, &self.functions)?;
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
                self.set_abort_location(&instruction.origin)?;
                self.lower_instruction(instruction)?;
            }
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("basic block 缺少 terminator".to_owned())
            })?;
            self.set_debug_location(&terminator.origin)?;
            self.set_abort_location(&terminator.origin)?;
            if matches!(terminator.kind, TerminatorKind::Abort) {
                self.lower_abort(terminator.abort.as_ref())?;
            } else {
                self.lower_terminator(&terminator.kind)?;
            }
        }
        self.dependencies.runtime.set_abort_location(None);
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
                            self.local_storage_type(value_type(self.function, *value)?)?,
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
        if self
            .module
            .map_result_value(value_type(self.function, left)?)
            .is_some()
        {
            return self.lower_map_result_comparison(operator, left, right, *result);
        }
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
                let pointer = self.nullable_view_pointer(*owner)?;
                let phi = self.loan_phis.get(view).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("non-null view 缺少 LLVM PHI".to_owned())
                })?;
                phi.add_incoming(&[(&pointer, llvm_source)]);
                let is_null = self.nullable_is_null(*owner, "nullable.is_null")?;
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
                self.lower_abort(None)?;
            }
            TerminatorKind::Unreachable => {
                self.builder.build_unreachable()?;
            }
            TerminatorKind::RangeReturn { view, .. } => {
                self.builder.build_return(Some(&self.value(*view)?))?;
            }
            TerminatorKind::BorrowReturn { loan } => {
                let pointer = self.access(PlaceAccess::Loan(*loan))?;
                self.builder.build_return(Some(&pointer))?;
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
                "operand 不是 LLVM integer".to_owned(),
            )),
        }
    }

    fn struct_value(&self, id: ValueId) -> Result<StructValue<'ctx>, LlvmAdapterError> {
        match self.value(id)? {
            BasicValueEnum::StructValue(value) => Ok(value),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "operand 不是 LLVM struct".to_owned(),
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
