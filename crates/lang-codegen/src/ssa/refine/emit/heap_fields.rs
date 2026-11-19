//! Map refined field loans and exchanges to typed SSA operands.
use super::{Emitter, LoweringError, Operation, ROp, Span, loan};

impl Emitter<'_> {
    pub(super) fn heap_field_operation(
        &self,
        op: &ROp,
        span: Span,
    ) -> Result<Operation, LoweringError> {
        Ok(match op {
            ROp::SharedHeapFieldLoan { base, field } => Operation::SharedHeapFieldLoan {
                base: loan(self.get(*base, span)?, span)?,
                field: *field,
            },
            ROp::ExclusiveHeapFieldLoan { base, field } => Operation::ExclusiveHeapFieldLoan {
                base: loan(self.get(*base, span)?, span)?,
                field: *field,
            },
            ROp::HeapFieldExchange {
                owner,
                field,
                loan: slot,
                replacement,
            } => Operation::HeapFieldExchange {
                owner: self.value(*owner, span)?,
                field: *field,
                loan: loan(self.get(*slot, span)?, span)?,
                replacement: self.value(*replacement, span)?,
            },
            ROp::HeapReceiverFieldExchange {
                loan: slot,
                replacement,
            } => Operation::HeapReceiverFieldExchange {
                loan: loan(self.get(*slot, span)?, span)?,
                replacement: self.value(*replacement, span)?,
            },
            ROp::ExclusiveRootReborrow { source } => Operation::ExclusiveRootReborrow {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::InoutExchange {
                loan: slot,
                replacement,
            } => Operation::InoutExchange {
                loan: loan(self.get(*slot, span)?, span)?,
                replacement: self.value(*replacement, span)?,
            },
            _ => unreachable!("heap field emitter dispatch"),
        })
    }
}
