//! verified typed SSA 到 LLVM IR 的标量适配器。

use std::collections::BTreeMap;

use inkwell::{IntPredicate, basic_block::BasicBlock, builder::Builder, context::Context};
use inkwell::{
    intrinsics::Intrinsic,
    module::Module as LlvmModule,
    types::{BasicMetadataTypeEnum, IntType},
    values::{BasicMetadataValueEnum, FunctionValue, IntValue, PhiValue, ValueKind},
};

use crate::ssa::{
    model::{
        BinaryOperator, BlockId, CheckedArithmeticOperator, ComparisonOperator, Edge, EntityId,
        EntityType, Function, FunctionId, Instruction, Module, Operation, Program, ScalarConstant,
        SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    verify::verify_program,
};

use super::{LlvmAdapterError, configure_module, first_target_machine};

pub(super) fn render_verified_program(program: &Program) -> Result<String, LlvmAdapterError> {
    verify_program(program).map_err(|error| LlvmAdapterError::InvalidSsa(error.to_string()))?;
    let [ssa_module] = program.modules.as_slice() else {
        return Err(LlvmAdapterError::Unsupported(
            "SPEC-0034 只接受一个 SSA module".to_owned(),
        ));
    };
    let (triple, target_machine) = first_target_machine()?;
    let context = Context::create();
    let llvm_module = context.create_module(&ssa_module.name);
    configure_module(&llvm_module, &triple, &target_machine);

    ModuleLowerer::new(&context, &llvm_module, ssa_module).lower()?;
    llvm_module
        .verify()
        .map_err(|error| LlvmAdapterError::Verify(error.to_string()))?;
    Ok(llvm_module.print_to_string().to_string())
}

struct ModuleLowerer<'ctx, 'llvm, 'ssa> {
    context: &'ctx Context,
    llvm: &'llvm LlvmModule<'ctx>,
    ssa: &'ssa Module,
    functions: BTreeMap<FunctionId, FunctionValue<'ctx>>,
}

impl<'ctx, 'llvm, 'ssa> ModuleLowerer<'ctx, 'llvm, 'ssa> {
    fn new(context: &'ctx Context, llvm: &'llvm LlvmModule<'ctx>, ssa: &'ssa Module) -> Self {
        Self {
            context,
            llvm,
            ssa,
            functions: BTreeMap::new(),
        }
    }

    fn lower(mut self) -> Result<(), LlvmAdapterError> {
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
                &self.functions,
            )
            .lower()?;
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
                .map(|entity| {
                    let EntityId::Value(value) = entity else {
                        return Err(unsupported("LLVM 标量参数不能是 place 或 loan"));
                    };
                    let ty = value_type(function, *value)?;
                    Ok(BasicMetadataTypeEnum::from(self.llvm_int_type(ty)?))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let function_type = match function.return_types.as_slice() {
                [] => self.context.void_type().fn_type(&parameters, false),
                [result] => self.llvm_int_type(*result)?.fn_type(&parameters, false),
                _ => return Err(unsupported("SPEC-0034 不支持多返回值 LLVM ABI")),
            };
            let name = format!("f{}.{}", function.id.index(), function.name);
            let llvm_function = self.llvm.add_function(&name, function_type, None);
            self.functions.insert(function.id, llvm_function);
        }
        Ok(())
    }

    fn llvm_int_type(&self, ty: SsaTypeId) -> Result<IntType<'ctx>, LlvmAdapterError> {
        llvm_int_type(self.context, self.ssa, ty)
    }
}

struct FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions> {
    context: &'ctx Context,
    llvm: &'llvm LlvmModule<'ctx>,
    module: &'ssa Module,
    function: &'ssa Function,
    llvm_function: FunctionValue<'ctx>,
    functions: &'functions BTreeMap<FunctionId, FunctionValue<'ctx>>,
    builder: Builder<'ctx>,
    blocks: BTreeMap<BlockId, BasicBlock<'ctx>>,
    values: BTreeMap<ValueId, IntValue<'ctx>>,
    phis: BTreeMap<ValueId, PhiValue<'ctx>>,
}

impl<'ctx, 'llvm, 'ssa, 'functions> FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions> {
    fn new(
        context: &'ctx Context,
        llvm: &'llvm LlvmModule<'ctx>,
        module: &'ssa Module,
        function: &'ssa Function,
        llvm_function: FunctionValue<'ctx>,
        functions: &'functions BTreeMap<FunctionId, FunctionValue<'ctx>>,
    ) -> Self {
        Self {
            context,
            llvm,
            module,
            function,
            llvm_function,
            functions,
            builder: context.create_builder(),
            blocks: BTreeMap::new(),
            values: BTreeMap::new(),
            phis: BTreeMap::new(),
        }
    }

    fn lower(mut self) -> Result<(), LlvmAdapterError> {
        self.create_blocks_and_parameters()?;
        for block in &self.function.blocks {
            let llvm_block = self.block(block.id)?;
            self.builder.position_at_end(llvm_block);
            for instruction in &block.instructions {
                let instruction = self.function.instruction(*instruction).ok_or_else(|| {
                    LlvmAdapterError::InvalidSsa("block 引用未知 instruction".to_owned())
                })?;
                self.lower_instruction(instruction)?;
            }
            let terminator = block.terminator.as_ref().ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("basic block 缺少 terminator".to_owned())
            })?;
            self.lower_terminator(block.id, &terminator.kind)?;
        }
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
            let EntityId::Value(value) = entity else {
                return Err(unsupported("LLVM 标量 entry 参数不能是 place 或 loan"));
            };
            let llvm_value = self
                .llvm_function
                .get_nth_param(index as u32)
                .ok_or_else(|| LlvmAdapterError::Build("缺少 LLVM function 参数".to_owned()))?
                .into_int_value();
            llvm_value.set_name(&value_name(*value));
            self.values.insert(*value, llvm_value);
        }

        for block in self.function.blocks.iter().skip(1) {
            self.builder.position_at_end(self.block(block.id)?);
            for entity in &block.parameters {
                let EntityId::Value(value) = entity else {
                    return Err(unsupported("LLVM 标量 block 参数不能是 place 或 loan"));
                };
                let phi = self.builder.build_phi(
                    self.llvm_int_type(value_type(self.function, *value)?)?,
                    &value_name(*value),
                )?;
                self.values
                    .insert(*value, phi.as_basic_value().into_int_value());
                self.phis.insert(*value, phi);
            }
        }
        Ok(())
    }

    fn lower_instruction(&mut self, instruction: &Instruction) -> Result<(), LlvmAdapterError> {
        let results = value_results(instruction)?;
        match &instruction.operation {
            Operation::Constant(constant) => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("constant", 1, results.len()));
                };
                let ty = self.llvm_int_type(value_type(self.function, *result)?)?;
                let value = match constant {
                    ScalarConstant::Boolean(value) => ty.const_int(u64::from(*value), false),
                    ScalarConstant::Integer(value) => ty.const_int(*value as u64, *value < 0),
                    ScalarConstant::Unit => {
                        return Err(unsupported("Unit constant 不产生 LLVM payload"));
                    }
                };
                self.values.insert(*result, value);
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
                    .build_not(self.value(*operand)?, &value_name(*result))?;
                self.values.insert(*result, value);
            }
            Operation::DirectCall { callee, arguments } => {
                self.lower_call(*callee, arguments, &results)?;
            }
            Operation::Copy { source } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("copy", 1, results.len()));
                };
                self.values.insert(*result, self.value(*source)?);
            }
            Operation::Consume { .. } | Operation::Drop { .. } => {
                if !results.is_empty() {
                    return Err(invalid_result_count("ownership effect", 0, results.len()));
                }
            }
            Operation::RootPlace { .. }
            | Operation::BorrowBegin { .. }
            | Operation::BorrowEnd { .. }
            | Operation::Read { .. }
            | Operation::Mutate { .. } => {
                return Err(unsupported(
                    "SPEC-0034 LLVM adapter 不接受 place/loan operation",
                ));
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
        let left_value = self.value(left)?;
        let right_value = self.value(right)?;
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
                self.ordering_predicate(left, IntPredicate::SLT, IntPredicate::ULT)?,
                left_value,
                right_value,
                &name,
            )?,
        };
        self.values.insert(*result, value);
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
        let signed = self.integer_signed(left)?;
        let intrinsic_name = match (operator, signed) {
            (CheckedArithmeticOperator::Add, true) => "llvm.sadd.with.overflow",
            (CheckedArithmeticOperator::Add, false) => "llvm.uadd.with.overflow",
            (CheckedArithmeticOperator::Subtract, true) => "llvm.ssub.with.overflow",
            (CheckedArithmeticOperator::Subtract, false) => "llvm.usub.with.overflow",
            (CheckedArithmeticOperator::Multiply, true) => "llvm.smul.with.overflow",
            (CheckedArithmeticOperator::Multiply, false) => "llvm.umul.with.overflow",
            _ => return Err(unsupported("非法 overflow intrinsic operator")),
        };
        let int_type = self.llvm_int_type(value_type(self.function, left)?)?;
        let intrinsic = Intrinsic::find(intrinsic_name)
            .and_then(|intrinsic| intrinsic.get_declaration(self.llvm, &[int_type.into()]))
            .ok_or_else(|| {
                LlvmAdapterError::Build(format!("无法声明 LLVM intrinsic {intrinsic_name}"))
            })?;
        let arguments = [
            BasicMetadataValueEnum::from(self.value(left)?),
            BasicMetadataValueEnum::from(self.value(right)?),
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
        self.values.insert(result, value);
        self.values.insert(failed, overflow);
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
        let left_value = self.value(left)?;
        let right_value = self.value(right)?;
        let ty = left_value.get_type();
        let zero = ty.const_zero();
        let one = ty.const_int(1, false);
        let is_zero = self.builder.build_int_compare(
            IntPredicate::EQ,
            right_value,
            zero,
            &format!("v{}.zero", failed.index()),
        )?;
        let signed = self.integer_signed(left)?;
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
        self.values.insert(result, value);
        self.values.insert(failed, failure);
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
        let predicate = match operator {
            ComparisonOperator::Equal => IntPredicate::EQ,
            ComparisonOperator::NotEqual => IntPredicate::NE,
            ComparisonOperator::LessThan => {
                self.ordering_predicate(left, IntPredicate::SLT, IntPredicate::ULT)?
            }
            ComparisonOperator::LessThanOrEqual => {
                self.ordering_predicate(left, IntPredicate::SLE, IntPredicate::ULE)?
            }
            ComparisonOperator::GreaterThan => {
                self.ordering_predicate(left, IntPredicate::SGT, IntPredicate::UGT)?
            }
            ComparisonOperator::GreaterThanOrEqual => {
                self.ordering_predicate(left, IntPredicate::SGE, IntPredicate::UGE)?
            }
        };
        let value = self.builder.build_int_compare(
            predicate,
            self.value(left)?,
            self.value(right)?,
            &value_name(*result),
        )?;
        self.values.insert(*result, value);
        Ok(())
    }

    fn lower_call(
        &mut self,
        callee: FunctionId,
        arguments: &[ValueId],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let llvm_callee = *self
            .functions
            .get(&callee)
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("direct call target 不存在".to_owned()))?;
        let arguments = arguments
            .iter()
            .map(|argument| self.value(*argument).map(BasicMetadataValueEnum::from))
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
                    ValueKind::Basic(value) => value.into_int_value(),
                    ValueKind::Instruction(_) => {
                        return Err(LlvmAdapterError::InvalidSsa(
                            "value SSA call 对应 LLVM void call".to_owned(),
                        ));
                    }
                };
                value.set_name(&value_name(*result));
                self.values.insert(*result, value);
            }
            _ => return Err(unsupported("SPEC-0034 不支持多返回值 direct call")),
        }
        Ok(())
    }

    fn lower_terminator(
        &mut self,
        source: BlockId,
        terminator: &TerminatorKind,
    ) -> Result<(), LlvmAdapterError> {
        let llvm_source = self.block(source)?;
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
                    self.value(*condition)?,
                    self.block(when_true.target)?,
                    self.block(when_false.target)?,
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
                let trap = Intrinsic::find("llvm.trap")
                    .and_then(|intrinsic| intrinsic.get_declaration(self.llvm, &[]))
                    .ok_or_else(|| LlvmAdapterError::Build("无法声明 llvm.trap".to_owned()))?;
                self.builder.build_call(trap, &[], "")?;
                self.builder.build_unreachable()?;
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
            let EntityId::Value(argument) = argument else {
                return Err(unsupported("LLVM 标量 edge argument 不能是 place 或 loan"));
            };
            let EntityId::Value(parameter) = parameter else {
                return Err(unsupported(
                    "LLVM 标量 block parameter 不能是 place 或 loan",
                ));
            };
            let value = self.value(*argument)?;
            let phi = self.phis.get(parameter).ok_or_else(|| {
                LlvmAdapterError::InvalidSsa("非 entry block 参数缺少 LLVM PHI".to_owned())
            })?;
            phi.add_incoming(&[(&value, source)]);
        }
        Ok(())
    }

    fn ordering_predicate(
        &self,
        operand: ValueId,
        signed: IntPredicate,
        unsigned: IntPredicate,
    ) -> Result<IntPredicate, LlvmAdapterError> {
        Ok(if self.integer_signed(operand)? {
            signed
        } else {
            unsigned
        })
    }

    fn integer_signed(&self, value: ValueId) -> Result<bool, LlvmAdapterError> {
        match self.module.type_kind(value_type(self.function, value)?) {
            Some(SsaTypeKind::Integer { signed, .. }) => Ok(*signed),
            _ => Err(LlvmAdapterError::InvalidSsa(
                "integer operation 的 operand 不是整数".to_owned(),
            )),
        }
    }

    fn llvm_int_type(&self, ty: SsaTypeId) -> Result<IntType<'ctx>, LlvmAdapterError> {
        llvm_int_type(self.context, self.module, ty)
    }

    fn block(&self, id: BlockId) -> Result<BasicBlock<'ctx>, LlvmAdapterError> {
        self.blocks
            .get(&id)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM basic block 映射缺失".to_owned()))
    }

    fn value(&self, id: ValueId) -> Result<IntValue<'ctx>, LlvmAdapterError> {
        self.values
            .get(&id)
            .copied()
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("LLVM value 映射缺失".to_owned()))
    }
}

fn llvm_int_type<'ctx>(
    context: &'ctx Context,
    module: &Module,
    ty: SsaTypeId,
) -> Result<IntType<'ctx>, LlvmAdapterError> {
    match module.type_kind(ty) {
        Some(SsaTypeKind::Boolean) => Ok(context.bool_type()),
        Some(SsaTypeKind::Integer { bits: 8, .. }) => Ok(context.i8_type()),
        Some(SsaTypeKind::Integer { bits: 16, .. }) => Ok(context.i16_type()),
        Some(SsaTypeKind::Integer { bits: 32, .. }) => Ok(context.i32_type()),
        Some(SsaTypeKind::Integer { bits: 64, .. }) => Ok(context.i64_type()),
        Some(SsaTypeKind::Unit) => Err(unsupported("Unit 不具有 LLVM first-class payload")),
        Some(SsaTypeKind::Integer { .. })
        | Some(SsaTypeKind::Opaque { .. })
        | Some(SsaTypeKind::Aggregate { .. })
        | Some(SsaTypeKind::HeapOwner { .. }) => {
            Err(unsupported("SSA 类型不属于 SPEC-0034 LLVM 标量子集"))
        }
        None => Err(LlvmAdapterError::InvalidSsa(
            "SSA type identity 不存在".to_owned(),
        )),
    }
}

fn value_type(function: &Function, value: ValueId) -> Result<SsaTypeId, LlvmAdapterError> {
    match function.entity(EntityId::Value(value)).map(|data| data.ty) {
        Some(EntityType::Value(ty)) => Ok(ty),
        _ => Err(LlvmAdapterError::InvalidSsa(
            "ValueId 缺少 value entity type".to_owned(),
        )),
    }
}

fn value_results(instruction: &Instruction) -> Result<Vec<ValueId>, LlvmAdapterError> {
    instruction
        .results
        .iter()
        .map(|entity| match entity {
            EntityId::Value(value) => Ok(*value),
            EntityId::Place(_) | EntityId::Loan(_) => Err(unsupported(
                "SPEC-0034 LLVM instruction result 必须是 value",
            )),
        })
        .collect()
}

fn value_name(value: ValueId) -> String {
    format!("v{}", value.index())
}

fn invalid_result_count(operation: &str, expected: usize, actual: usize) -> LlvmAdapterError {
    LlvmAdapterError::InvalidSsa(format!(
        "{operation} 结果数量错误：期望 {expected}，实际 {actual}"
    ))
}

fn unsupported(message: &str) -> LlvmAdapterError {
    LlvmAdapterError::Unsupported(message.to_owned())
}
