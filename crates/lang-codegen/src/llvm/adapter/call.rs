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
        let arguments = arguments
            .iter()
            .map(|argument| {
                let EntityId::Loan(loan) = argument else {
                    return Err(unsupported("borrow result slice requires loan argument"));
                };
                self.access(PlaceAccess::Loan(*loan))
                    .map(BasicMetadataValueEnum::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let call = self.builder.build_call(target, &arguments, "borrow.call")?;
        let ValueKind::Basic(value) = call.try_as_basic_value() else {
            return Err(unsupported("borrow call returned void"));
        };
        self.loans.insert(*result, value.into_pointer_value());

        Ok(())
    }
}
