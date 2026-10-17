//! Five explicit SSA Map operations share the real insertion-chain primitives.
use super::*;
use crate::llvm::map::iteration::{self, Component};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_map_iteration_operation(
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
                let pointer =
                    iteration::component(&slot_source, self.int_value(*cursor)?, component, &name)?;
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
