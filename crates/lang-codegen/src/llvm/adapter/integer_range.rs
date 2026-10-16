//! Pure numeric Range values. Half-open normalization subtracts only on a nonempty edge.
use super::{
    FunctionLowerer, IntPredicate, LlvmAdapterError, aggregate, invalid_result_count, value_name,
    value_type,
};
use crate::ssa::model::ValueId;
use inkwell::values::IntValue;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_integer_range_construct(
        &mut self,
        first: ValueId,
        bound: ValueId,
        inclusive: bool,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count(
                "integer Range construct",
                1,
                results.len(),
            ));
        };
        let first = self.int_value(first)?;
        let bound = self.int_value(bound)?;
        let name = value_name(*result);
        let nonempty = self.builder.build_int_compare(
            if inclusive {
                IntPredicate::SLE
            } else {
                IntPredicate::SLT
            },
            first,
            bound,
            &format!("{name}.nonempty"),
        )?;
        let empty = self.builder.build_not(nonempty, &format!("{name}.empty"))?;
        let last = if inclusive {
            bound
        } else {
            let present = self
                .context
                .append_basic_block(self.llvm_function, &format!("{name}.nonempty"));
            let absent = self
                .context
                .append_basic_block(self.llvm_function, &format!("{name}.empty"));
            let ready = self
                .context
                .append_basic_block(self.llvm_function, &format!("{name}.ready"));
            self.builder
                .build_conditional_branch(nonempty, present, absent)?;
            self.builder.position_at_end(present);
            // first < bound proves bound is greater than the endpoint type's MIN.
            let last = self.builder.build_int_sub(
                bound,
                bound.get_type().const_int(1, false),
                &format!("{name}.last.nonempty"),
            )?;
            self.builder.build_unconditional_branch(ready)?;
            self.builder.position_at_end(absent);
            self.builder.build_unconditional_branch(ready)?;
            self.builder.position_at_end(ready);
            let phi = self
                .builder
                .build_phi(bound.get_type(), &format!("{name}.last"))?;
            phi.add_incoming(&[(&last, present), (&bound.get_type().const_zero(), absent)]);
            phi.as_basic_value().into_int_value()
        };
        let range = aggregate::construct(
            &self.builder,
            self.dependencies.type_map,
            value_type(self.function, *result)?,
            &[first.into(), last.into(), empty.into()],
            &name,
        )?;
        self.values.insert(*result, range.into());
        Ok(())
    }

    fn integer_range_fields(
        &self,
        value: ValueId,
    ) -> Result<[IntValue<'ctx>; 3], LlvmAdapterError> {
        let range = self.struct_value(value)?;
        let name = value_name(value);
        Ok([
            self.builder
                .build_extract_value(range, 0, &format!("{name}.first"))?
                .into_int_value(),
            self.builder
                .build_extract_value(range, 1, &format!("{name}.last"))?
                .into_int_value(),
            self.builder
                .build_extract_value(range, 2, &format!("{name}.empty"))?
                .into_int_value(),
        ])
    }

    pub(super) fn lower_integer_range_contains(
        &mut self,
        value: ValueId,
        range: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count(
                "integer Range contains",
                1,
                results.len(),
            ));
        };
        let value = self.int_value(value)?;
        let [first, last, empty] = self.integer_range_fields(range)?;
        let name = value_name(*result);
        let lower = self.builder.build_int_compare(
            IntPredicate::SLE,
            first,
            value,
            &format!("{name}.lower"),
        )?;
        let upper = self.builder.build_int_compare(
            IntPredicate::SLE,
            value,
            last,
            &format!("{name}.upper"),
        )?;
        let within = self
            .builder
            .build_and(lower, upper, &format!("{name}.within"))?;
        let nonempty = self.builder.build_not(empty, &format!("{name}.nonempty"))?;
        let result_value = self.builder.build_and(nonempty, within, &name)?;
        self.values.insert(*result, result_value.into());
        Ok(())
    }

    pub(super) fn lower_integer_range_equal(
        &mut self,
        left: ValueId,
        right: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count(
                "integer Range equality",
                1,
                results.len(),
            ));
        };
        let [lf, ll, le] = self.integer_range_fields(left)?;
        let [rf, rl, re] = self.integer_range_fields(right)?;
        let name = value_name(*result);
        let both_empty = self
            .builder
            .build_and(le, re, &format!("{name}.both.empty"))?;
        let either_empty = self
            .builder
            .build_or(le, re, &format!("{name}.either.empty"))?;
        let both_nonempty = self
            .builder
            .build_not(either_empty, &format!("{name}.both.nonempty"))?;
        let first_equal = self.builder.build_int_compare(
            IntPredicate::EQ,
            lf,
            rf,
            &format!("{name}.first.equal"),
        )?;
        let last_equal = self.builder.build_int_compare(
            IntPredicate::EQ,
            ll,
            rl,
            &format!("{name}.last.equal"),
        )?;
        let same_bounds =
            self.builder
                .build_and(first_equal, last_equal, &format!("{name}.same.bounds"))?;
        let same_nonempty =
            self.builder
                .build_and(both_nonempty, same_bounds, &format!("{name}.same.nonempty"))?;
        let equal = self.builder.build_or(both_empty, same_nonempty, &name)?;
        self.values.insert(*result, equal.into());
        Ok(())
    }
}
