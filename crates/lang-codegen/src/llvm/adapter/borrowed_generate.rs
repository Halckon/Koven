//! Borrowed callable storage bridge for container generation.

use super::{
    FunctionLowerer, LlvmAdapterError, access_type, container, invalid_result_count, value_name,
};
use crate::ssa::model::{LoanId, PlaceAccess, SsaTypeId, ValueId};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_container_generate_borrowed(
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
