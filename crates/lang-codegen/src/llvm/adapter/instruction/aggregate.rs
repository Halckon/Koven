//! Aggregate and tagged instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction_aggregate(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
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
            _ => {
                return Err(unsupported(
                    "operation must be dispatched to its lowering family",
                ));
            }
        }
        Ok(())
    }
}
