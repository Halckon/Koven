use crate::ssa::model::{ClosureCaptureOperand, FunctionId, SsaTypeId, ValueId};

use super::{FunctionLowerer, LlvmAdapterError, closure, unsupported, value_name, value_type};

impl<'ctx, 'llvm, 'ssa, 'functions> FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions> {
    pub(super) fn lower_function_address(
        &mut self,
        target: FunctionId,
        result: ValueId,
    ) -> Result<(), LlvmAdapterError> {
        let target = *self.dependencies.functions.get(&target).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("function address target was not declared".to_owned())
        })?;
        self.values
            .insert(result, closure::function_address(target).into());
        Ok(())
    }

    pub(super) fn lower_closure_construct(
        &mut self,
        closure_type: SsaTypeId,
        thunk: FunctionId,
        captures: &[ClosureCaptureOperand],
        result: ValueId,
    ) -> Result<(), LlvmAdapterError> {
        let thunk = *self.dependencies.functions.get(&thunk).ok_or_else(|| {
            LlvmAdapterError::InvalidSsa("closure thunk was not declared".to_owned())
        })?;
        let captures = captures
            .iter()
            .map(|capture| match capture {
                ClosureCaptureOperand::Owned(value) => self.value(*value),
                ClosureCaptureOperand::Shared(_) => Err(unsupported(
                    "shared closure formation waits for the final SPEC-0038 slice",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let value = closure::construct(
            &self.builder,
            self.dependencies.type_map,
            closure_type,
            thunk,
            &captures,
            &value_name(result),
        )?;
        self.values.insert(result, value.into());
        Ok(())
    }

    pub(super) fn lower_callable_invoke(
        &mut self,
        callable: ValueId,
        arguments: &[ValueId],
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let arguments = arguments
            .iter()
            .map(|argument| self.value(*argument))
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
