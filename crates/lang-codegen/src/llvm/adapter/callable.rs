use crate::ssa::model::{
    ClosureCaptureOperand, EntityId, FunctionId, PlaceAccess, SsaTypeId, ValueId,
};

use super::{
    FunctionLowerer, LlvmAdapterError, closure, invalid_result_count, value_name, value_type,
};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_function_address(
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

    pub(super) fn lower_closure_construct(
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

    pub(super) fn lower_callable_invoke(
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
