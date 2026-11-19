//! Render direct heap field access, projection and exchange.
use super::{EntityId, Operation, Write, fmt, write_entity_id};

pub(super) fn write(output: &mut String, operation: &Operation) -> fmt::Result {
    match operation {
        Operation::HeapFieldRead { receiver, field } => {
            output.write_str("heap.field_read ")?;
            write_entity_id(output, EntityId::Loan(*receiver))?;
            write!(output, ", {field}")
        }
        Operation::HeapFieldReplace {
            receiver,
            field,
            value,
        } => {
            output.write_str("heap.field_replace ")?;
            write_entity_id(output, EntityId::Loan(*receiver))?;
            write!(output, ", {field}, ")?;
            write_entity_id(output, EntityId::Value(*value))
        }
        Operation::HeapFieldExchange {
            owner,
            field,
            loan,
            replacement,
        } => {
            output.write_str("heap.field_exchange ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            write!(output, ", {field}, ")?;
            write_entity_id(output, EntityId::Loan(*loan))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*replacement))
        }
        Operation::HeapReceiverFieldExchange { loan, replacement } => {
            output.write_str("heap.receiver_field_exchange ")?;
            write_entity_id(output, EntityId::Loan(*loan))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*replacement))
        }
        Operation::SharedHeapFieldLoan { base, field } => {
            output.write_str("shared_heap_field_loan ")?;
            write_entity_id(output, EntityId::Loan(*base))?;
            write!(output, ", {field}")
        }
        Operation::ExclusiveHeapFieldLoan { base, field } => {
            output.write_str("exclusive_heap_field_loan ")?;
            write_entity_id(output, EntityId::Loan(*base))?;
            write!(output, ", {field}")
        }
        Operation::InoutExchange { loan, replacement } => {
            output.write_str("inout.exchange ")?;
            write_entity_id(output, EntityId::Loan(*loan))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*replacement))
        }
        Operation::ExclusiveRootReborrow { source } => {
            output.write_str("inout.root_reborrow ")?;
            write_entity_id(output, EntityId::Loan(*source))
        }
        _ => unreachable!("heap field renderer dispatch"),
    }
}
