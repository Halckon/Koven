use std::fmt::{self, Write};

use super::model::{
    BinaryOperator, CallableSignature, CheckedArithmeticOperator, ClosureCaptureMode,
    ClosureCaptureOperand, ComparisonOperator, Edge, EntityId, EntityType, Function, LoanKind,
    Module, Operation, Origin, PlaceAccess, Program, ScalarConstant, SsaTypeId, SsaTypeKind,
    TerminatorKind,
};

/// 生成只用于调试和测试的确定性 SSA 文本。
pub(super) fn render_program(program: &Program) -> String {
    let mut output = String::new();
    // Writing into a String is infallible; fmt::Result only keeps the helpers composable.
    write_program(&mut output, program).expect("writing SSA debug text into a String cannot fail");
    output
}

fn write_program(output: &mut String, program: &Program) -> fmt::Result {
    for (index, module) in program.modules.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        write_module(output, module)?;
    }
    Ok(())
}

fn write_module(output: &mut String, module: &Module) -> fmt::Result {
    writeln!(output, "module {:?} {{", module.name)?;
    for (index, kind) in module.types.iter().enumerate() {
        write!(output, "  !t{index} = ")?;
        write_type_kind(output, kind)?;
        writeln!(output)?;
    }
    if !module.types.is_empty() && !module.functions.is_empty() {
        writeln!(output)?;
    }
    for (index, function) in module.functions.iter().enumerate() {
        if index != 0 {
            writeln!(output)?;
        }
        write_function(output, function)?;
    }
    writeln!(output, "}}")
}

fn write_type_kind(output: &mut String, kind: &SsaTypeKind) -> fmt::Result {
    match kind {
        SsaTypeKind::Unit => output.write_str("unit"),
        SsaTypeKind::Boolean => output.write_str("bool"),
        SsaTypeKind::Integer { bits, signed } => {
            write!(output, "{}{bits}", if *signed { 'i' } else { 'u' })
        }
        SsaTypeKind::Opaque { name, ownership } => {
            write!(output, "opaque {name:?} {:?}", ownership)
        }
        SsaTypeKind::ZeroSized { name, ownership } => {
            write!(output, "zst {name:?} {:?}", ownership)
        }
        SsaTypeKind::Aggregate {
            name,
            fields,
            ownership,
        } => {
            write!(output, "aggregate {name:?} {:?} (", ownership)?;
            write_type_ids(output, fields)?;
            output.push(')');
            Ok(())
        }
        SsaTypeKind::TaggedUnion {
            name,
            variants,
            ownership,
        } => {
            write!(output, "tagged_union {name:?} {:?} (", ownership)?;
            write_type_ids(output, variants)?;
            output.push(')');
            Ok(())
        }
        SsaTypeKind::HeapOwner { name, payload } => {
            write!(output, "heap_owner {name:?}")?;
            if let Some(payload) = payload {
                output.write_str(" payload ")?;
                write_type_id(output, *payload)?;
            } else {
                output.write_str(" <declared>")?;
            }
            Ok(())
        }
        SsaTypeKind::SharedOwner { name, payload } => {
            write!(output, "shared_owner {name:?}")?;
            if let Some(payload) = payload {
                output.write_str(" payload ")?;
                write_type_id(output, *payload)?;
            } else {
                output.write_str(" <declared>")?;
            }
            Ok(())
        }
        SsaTypeKind::StringOwner => output.write_str("string_owner"),
        SsaTypeKind::NullableHandle { inner } => {
            output.write_str("nullable_handle<")?;
            write_type_id(output, *inner)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::SequentialContainer { kind, element } => {
            write!(output, "container {kind:?}<")?;
            write_type_id(output, *element)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::SharedReference { target } => {
            output.write_str("shared_ref<")?;
            write_type_id(output, *target)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::FunctionPointer { signature } => {
            output.write_str("function_pointer ")?;
            write_callable_signature(output, signature)
        }
        SsaTypeKind::ConcreteClosure {
            name,
            signature,
            environment,
            captures,
        } => {
            write!(output, "closure {name:?} env ")?;
            write_type_id(output, *environment)?;
            output.write_str(" captures (")?;
            for (index, capture) in captures.iter().enumerate() {
                if index != 0 {
                    output.write_str(", ")?;
                }
                output.write_str(match capture.mode {
                    ClosureCaptureMode::Shared => "shared ",
                    ClosureCaptureMode::Owned => "owned ",
                })?;
                write_type_id(output, capture.ty)?;
            }
            output.write_str(") ")?;
            write_callable_signature(output, signature)
        }
    }
}

fn write_callable_signature(output: &mut String, signature: &CallableSignature) -> fmt::Result {
    output.push('(');
    for (index, parameter) in signature.parameters.iter().enumerate() {
        if index != 0 {
            output.write_str(", ")?;
        }
        write_entity_type(output, *parameter)?;
    }
    output.write_str(") -> (")?;
    write_type_ids(output, &signature.returns)?;
    output.push(')');
    Ok(())
}

fn write_function(output: &mut String, function: &Function) -> fmt::Result {
    write!(output, "  func {:?}(", function.name)?;
    if let Some(entry) = function.blocks.first() {
        if function.receiver.is_some() {
            if let Some((receiver, parameters)) = entry.parameters.split_first() {
                output.write_str("receiver ")?;
                write_entities_with_types(output, function, std::slice::from_ref(receiver))?;
                if !parameters.is_empty() {
                    output.write_str("; ")?;
                    write_entities_with_types(output, function, parameters)?;
                }
            } else {
                output.write_str("receiver <missing>")?;
            }
        } else {
            write_entities_with_types(output, function, &entry.parameters)?;
        }
    }
    output.write_str(") -> (")?;
    write_type_ids(output, &function.return_types)?;
    output.write_str(") ")?;
    write_origin(output, &function.origin)?;
    writeln!(output, " {{")?;

    for block in &function.blocks {
        write!(output, "    bb{}(", block.id.index())?;
        write_entities_with_types(output, function, &block.parameters)?;
        output.write_str(") ")?;
        write_origin(output, &block.origin)?;
        writeln!(output, ":")?;

        for instruction_id in &block.instructions {
            let Some(instruction) = function.instruction(*instruction_id) else {
                writeln!(
                    output,
                    "      i{}: <unknown instruction>",
                    instruction_id.index()
                )?;
                continue;
            };
            write!(output, "      i{}: ", instruction.id.index())?;
            if !instruction.results.is_empty() {
                write_entity_ids(output, &instruction.results)?;
                output.write_str(" = ")?;
            }
            write_operation(output, &instruction.operation)?;
            if !instruction.results.is_empty() {
                output.write_str(" : ")?;
                write_entity_types(output, function, &instruction.results)?;
            }
            output.push(' ');
            write_origin(output, &instruction.origin)?;
            writeln!(output)?;
        }

        write!(output, "      ")?;
        match &block.terminator {
            Some(terminator) => {
                write_terminator(output, &terminator.kind)?;
                output.push(' ');
                write_origin(output, &terminator.origin)?;
            }
            None => output.write_str("<missing terminator>")?,
        }
        writeln!(output)?;
    }
    writeln!(output, "  }}")
}

fn write_entities_with_types(
    output: &mut String,
    function: &Function,
    entities: &[EntityId],
) -> fmt::Result {
    for (index, entity) in entities.iter().enumerate() {
        if index != 0 {
            output.write_str(", ")?;
        }
        write_entity_id(output, *entity)?;
        output.write_str(": ")?;
        match function.entity(*entity) {
            Some(data) => write_entity_type(output, data.ty)?,
            None => output.write_str("<unknown>")?,
        }
    }
    Ok(())
}

fn write_entity_types(
    output: &mut String,
    function: &Function,
    entities: &[EntityId],
) -> fmt::Result {
    output.push('(');
    for (index, entity) in entities.iter().enumerate() {
        if index != 0 {
            output.write_str(", ")?;
        }
        match function.entity(*entity) {
            Some(data) => write_entity_type(output, data.ty)?,
            None => output.write_str("<unknown>")?,
        }
    }
    output.push(')');
    Ok(())
}

fn write_entity_ids(output: &mut String, entities: &[EntityId]) -> fmt::Result {
    for (index, entity) in entities.iter().enumerate() {
        if index != 0 {
            output.write_str(", ")?;
        }
        write_entity_id(output, *entity)?;
    }
    Ok(())
}

fn write_entity_id(output: &mut String, entity: EntityId) -> fmt::Result {
    match entity {
        EntityId::Value(id) => write!(output, "%v{}", id.index()),
        EntityId::Place(id) => write!(output, "%p{}", id.index()),
        EntityId::Loan(id) => write!(output, "%l{}", id.index()),
    }
}

fn write_entity_type(output: &mut String, ty: EntityType) -> fmt::Result {
    match ty {
        EntityType::Value(ty) => {
            output.write_str("value ")?;
            write_type_id(output, ty)
        }
        EntityType::Place(ty) => {
            output.write_str("place ")?;
            write_type_id(output, ty)
        }
        EntityType::Loan { kind, target } => {
            write!(
                output,
                "loan.{} ",
                match kind {
                    LoanKind::Shared => "shared",
                    LoanKind::Exclusive => "exclusive",
                }
            )?;
            write_type_id(output, target)
        }
    }
}

fn write_type_ids(output: &mut String, types: &[SsaTypeId]) -> fmt::Result {
    for (index, ty) in types.iter().enumerate() {
        if index != 0 {
            output.write_str(", ")?;
        }
        write_type_id(output, *ty)?;
    }
    Ok(())
}

fn write_type_id(output: &mut String, ty: SsaTypeId) -> fmt::Result {
    write!(output, "!t{}", ty.index())
}

fn write_operation(output: &mut String, operation: &Operation) -> fmt::Result {
    match operation {
        Operation::Constant(constant) => {
            output.write_str("const ")?;
            write_constant(output, constant)
        }
        Operation::PrintLiteral { bytes } => write!(output, "print.literal {bytes:?}"),
        Operation::StringLiteral { string, bytes } => {
            output.write_str("string.literal ")?;
            write_type_id(output, *string)?;
            write!(output, ", {bytes:?}")
        }
        Operation::StringConcat { left, right } => {
            output.write_str("string.concat ")?;
            write_entity_id(output, *left)?;
            output.write_str(", ")?;
            write_entity_id(output, *right)
        }
        Operation::StringEqual { left, right } => {
            output.write_str("string.equal ")?;
            write_entity_id(output, *left)?;
            output.write_str(", ")?;
            write_entity_id(output, *right)
        }
        Operation::PrintString { value } => {
            output.write_str("print.string ")?;
            write_entity_id(output, EntityId::Loan(*value))
        }
        Operation::Binary {
            operator,
            left,
            right,
        } => {
            write!(output, "{} ", binary_name(*operator))?;
            write_entity_id(output, EntityId::Value(*left))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*right))
        }
        Operation::CheckedArithmetic {
            operator,
            left,
            right,
        } => {
            write!(output, "checked.{} ", checked_arithmetic_name(*operator))?;
            write_entity_id(output, EntityId::Value(*left))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*right))
        }
        Operation::Compare {
            operator,
            left,
            right,
        } => {
            write!(output, "cmp.{} ", comparison_name(*operator))?;
            write_entity_id(output, EntityId::Value(*left))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*right))
        }
        Operation::BooleanNot { operand } => {
            output.write_str("not ")?;
            write_entity_id(output, EntityId::Value(*operand))
        }
        Operation::DirectCall {
            callee,
            receiver,
            arguments,
        } => {
            write!(output, "call @f{}(", callee.index())?;
            if let Some(receiver) = receiver {
                output.write_str("receiver ")?;
                write_entity_id(output, *receiver)?;
                if !arguments.is_empty() {
                    output.write_str("; ")?;
                }
            }
            write_entity_ids(output, arguments)?;
            output.push(')');
            Ok(())
        }
        Operation::FunctionAddress { target } => {
            write!(output, "function_address @f{}", target.index())
        }
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures,
        } => {
            output.write_str("closure.construct ")?;
            write_type_id(output, *closure)?;
            write!(output, ", @f{}(", thunk.index())?;
            for (index, capture) in captures.iter().enumerate() {
                if index != 0 {
                    output.write_str(", ")?;
                }
                match capture {
                    ClosureCaptureOperand::Shared(loan) => {
                        output.write_str("shared ")?;
                        write_entity_id(output, EntityId::Loan(*loan))?;
                    }
                    ClosureCaptureOperand::Owned(value) => {
                        output.write_str("owned ")?;
                        write_entity_id(output, EntityId::Value(*value))?;
                    }
                }
            }
            output.push(')');
            Ok(())
        }
        Operation::CallableInvoke {
            callable,
            arguments,
        } => {
            output.write_str("invoke ")?;
            write_entity_id(output, EntityId::Value(*callable))?;
            output.push('(');
            write_entity_ids(output, arguments)?;
            output.push(')');
            Ok(())
        }
        Operation::AggregateConstruct { aggregate, fields } => {
            output.write_str("aggregate.construct ")?;
            write_type_id(output, *aggregate)?;
            output.push('(');
            let fields = fields
                .iter()
                .copied()
                .map(EntityId::Value)
                .collect::<Vec<_>>();
            write_entity_ids(output, &fields)?;
            output.push(')');
            Ok(())
        }
        Operation::AggregateProject { aggregate, field } => {
            output.write_str("aggregate.project ")?;
            write_entity_id(output, EntityId::Value(*aggregate))?;
            write!(output, ", {field}")
        }
        Operation::AggregateExplode { aggregate } => {
            output.write_str("aggregate.explode ")?;
            write_entity_id(output, EntityId::Value(*aggregate))
        }
        Operation::AggregateCopyExplode { aggregate } => {
            output.write_str("aggregate.copy_explode ")?;
            write_entity_id(output, EntityId::Value(*aggregate))
        }
        Operation::TaggedConstruct {
            tagged,
            variant,
            payload,
        } => {
            output.write_str("tagged.construct ")?;
            write_type_id(output, *tagged)?;
            write!(output, ", {variant}, ")?;
            write_entity_id(output, EntityId::Value(*payload))
        }
        Operation::TaggedPayloadPlace { owner, variant } => {
            output.write_str("tagged.payload_place ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            write!(output, ", {variant}")
        }
        Operation::TaggedDiscriminant { owner } => {
            output.write_str("tagged.discriminant ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::HeapAllocate { owner, payload } => {
            output.write_str("heap.allocate ")?;
            write_type_id(output, *owner)?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*payload))
        }
        Operation::HeapPayloadPlace { owner } => {
            output.write_str("heap.payload_place ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::SharedAllocate { owner, payload } => {
            output.write_str("shared.allocate ")?;
            write_type_id(output, *owner)?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*payload))
        }
        Operation::SharedRetain { owner } => {
            output.write_str("shared.retain ")?;
            write_entity_id(output, *owner)
        }
        Operation::SharedPayloadPlace { owner } => {
            output.write_str("shared.payload_place ")?;
            write_entity_id(output, *owner)
        }
        Operation::NullableWrap { nullable, owner } => {
            output.write_str("nullable.wrap ")?;
            write_type_id(output, *nullable)?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::NullableNull { nullable } => {
            output.write_str("nullable.null ")?;
            write_type_id(output, *nullable)
        }
        Operation::NullableIsNull { owner } => {
            output.write_str("nullable.is_null ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::NullableTake { owner, proof } => {
            output.write_str("nullable.take ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Loan(*proof))
        }
        Operation::ContainerConstruct {
            container,
            elements,
        } => {
            output.write_str("container.construct ")?;
            write_type_id(output, *container)?;
            output.push('(');
            let elements = elements
                .iter()
                .copied()
                .map(EntityId::Value)
                .collect::<Vec<_>>();
            write_entity_ids(output, &elements)?;
            output.push(')');
            Ok(())
        }
        Operation::ContainerGenerate {
            container,
            length,
            initializer,
        } => {
            output.write_str("container.generate ")?;
            write_type_id(output, *container)?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*length))?;
            write!(output, ", @f{}", initializer.index())
        }
        Operation::ContainerLength { owner } => {
            output.write_str("container.length ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::ContainerElementPlace { owner, index } => {
            output.write_str("container.element_place ")?;
            write_entity_id(output, *owner)?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*index))
        }
        Operation::ContainerReplace {
            owner,
            index,
            value,
        } => {
            output.write_str("container.replace ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*index))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*value))
        }
        Operation::FieldPlace { base, field } => {
            output.write_str("field_place ")?;
            write_entity_id(output, EntityId::Place(*base))?;
            write!(output, ", {field}")
        }
        Operation::SharedFieldLoan { base, field } => {
            output.write_str("shared_field_loan ")?;
            write_entity_id(output, EntityId::Loan(*base))?;
            write!(output, ", {field}")
        }
        Operation::SharedReborrow { source } => {
            output.write_str("shared_reborrow ")?;
            write_entity_id(output, EntityId::Loan(*source))
        }
        Operation::Copy { source } => {
            output.write_str("copy ")?;
            write_entity_id(output, EntityId::Value(*source))
        }
        Operation::Consume { owner } => {
            output.write_str("consume ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::RootPlace { owner } => {
            output.write_str("root_place ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
        Operation::BorrowBegin { place, kind } => {
            write!(
                output,
                "borrow.{} ",
                match kind {
                    LoanKind::Shared => "shared",
                    LoanKind::Exclusive => "exclusive",
                }
            )?;
            write_entity_id(output, EntityId::Place(*place))
        }
        Operation::BorrowEnd { loan } => {
            output.write_str("end_borrow ")?;
            write_entity_id(output, EntityId::Loan(*loan))
        }
        Operation::Read { source } => {
            output.write_str("read ")?;
            match source {
                PlaceAccess::Place(place) => write_entity_id(output, EntityId::Place(*place)),
                PlaceAccess::Loan(loan) => write_entity_id(output, EntityId::Loan(*loan)),
            }
        }
        Operation::Mutate { place, value } => {
            output.write_str("mutate ")?;
            write_entity_id(output, EntityId::Place(*place))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*value))
        }
        Operation::Drop { owner } => {
            output.write_str("drop ")?;
            write_entity_id(output, EntityId::Value(*owner))
        }
    }
}

fn write_constant(output: &mut String, constant: &ScalarConstant) -> fmt::Result {
    match constant {
        ScalarConstant::Unit => output.write_str("unit"),
        ScalarConstant::Boolean(value) => write!(output, "{value}"),
        ScalarConstant::Integer(value) => write!(output, "{value}"),
    }
}

const fn binary_name(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "add",
        BinaryOperator::Subtract => "sub",
        BinaryOperator::Multiply => "mul",
        BinaryOperator::Equal => "eq",
        BinaryOperator::LessThan => "lt",
    }
}

const fn checked_arithmetic_name(operator: CheckedArithmeticOperator) -> &'static str {
    match operator {
        CheckedArithmeticOperator::Add => "add",
        CheckedArithmeticOperator::Subtract => "sub",
        CheckedArithmeticOperator::Multiply => "mul",
        CheckedArithmeticOperator::Divide => "div",
        CheckedArithmeticOperator::Remainder => "rem",
    }
}

const fn comparison_name(operator: ComparisonOperator) -> &'static str {
    match operator {
        ComparisonOperator::Equal => "eq",
        ComparisonOperator::NotEqual => "ne",
        ComparisonOperator::LessThan => "lt",
        ComparisonOperator::LessThanOrEqual => "le",
        ComparisonOperator::GreaterThan => "gt",
        ComparisonOperator::GreaterThanOrEqual => "ge",
    }
}

fn write_terminator(output: &mut String, terminator: &TerminatorKind) -> fmt::Result {
    match terminator {
        TerminatorKind::Branch(edge) => {
            output.write_str("branch ")?;
            write_edge(output, edge)
        }
        TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            output.write_str("cond ")?;
            write_entity_id(output, EntityId::Value(*condition))?;
            output.write_str(", ")?;
            write_edge(output, when_true)?;
            output.write_str(", ")?;
            write_edge(output, when_false)
        }
        TerminatorKind::NullableBranch {
            owner,
            when_null,
            when_non_null,
            view,
        } => {
            output.write_str("nullable.branch ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            output.write_str(", null ")?;
            write_edge(output, when_null)?;
            output.write_str(", non_null ")?;
            write_edge(output, when_non_null)?;
            output.write_str(" view ")?;
            write_entity_id(output, EntityId::Loan(*view))
        }
        TerminatorKind::Return { values } => {
            output.write_str("return")?;
            if !values.is_empty() {
                output.push(' ');
                let entities = values
                    .iter()
                    .copied()
                    .map(EntityId::Value)
                    .collect::<Vec<_>>();
                write_entity_ids(output, &entities)?;
            }
            Ok(())
        }
        TerminatorKind::Abort => output.write_str("abort"),
    }
}

fn write_edge(output: &mut String, edge: &Edge) -> fmt::Result {
    write!(output, "bb{}(", edge.target.index())?;
    write_entity_ids(output, &edge.arguments)?;
    output.push(')');
    Ok(())
}

fn write_origin(output: &mut String, origin: &Origin) -> fmt::Result {
    let span = origin.span();
    match origin {
        Origin::Source(_) => write!(
            output,
            "@source({:?}:{}..{})",
            span.source_id(),
            span.start(),
            span.end()
        ),
        Origin::Synthetic { reason, .. } => write!(
            output,
            "@synthetic({reason:?}, {:?}:{}..{})",
            span.source_id(),
            span.start(),
            span.end()
        ),
    }
}
