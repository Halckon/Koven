use std::fmt::{self, Write};

use super::model::{
    BinaryOperator, Edge, EntityId, EntityType, Function, LoanKind, Module, Operation, Origin,
    PlaceAccess, Program, ScalarConstant, SsaTypeId, SsaTypeKind, TerminatorKind,
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
    }
}

fn write_function(output: &mut String, function: &Function) -> fmt::Result {
    write!(output, "  func {:?}(", function.name)?;
    if let Some(entry) = function.blocks.first() {
        write_entities_with_types(output, function, &entry.parameters)?;
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
