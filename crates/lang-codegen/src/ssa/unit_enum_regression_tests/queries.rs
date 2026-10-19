//! 只读真实 frontend descriptor/Source origin/SSA def-use；不新建图或拼造 IR。
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    parser::{Expression, Item, ParsedFile, WhenCondition},
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, UnitCallTarget, UnitConstructionTarget, UnitExpressionId,
        UnitTypeKind,
    },
};

use super::{Case, require};
use crate::ssa::model::{
    ComparisonOperator, Definition, EntityId, EntityType, Function, FunctionId, Instruction,
    Operation, Origin, Program, ScalarConstant, SsaTypeKind, TerminatorKind, ValueId,
};

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn definition(function: &Function, value: ValueId) -> Result<&Instruction, String> {
    let entity = function
        .entity(EntityId::Value(value))
        .ok_or_else(|| format!("missing real value definition: {value:?}"))?;
    let Definition::InstructionResult { instruction, .. } = entity.definition else {
        return Err(format!("expected real instruction result: {value:?}"));
    };
    function
        .instruction(instruction)
        .ok_or_else(|| format!("missing defining instruction: {instruction:?}"))
}

pub(super) fn check(
    case: Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    program: &Program,
    driver: FunctionId,
) -> Result<(), String> {
    let driver_function = program
        .module(driver.module())
        .and_then(|module| module.function(driver))
        .ok_or_else(|| "missing actual lowered driver entry".to_owned())?;
    let [source_unit] = names.names().index().source_units() else {
        return Err("query needs the same unique original source unit".to_owned());
    };
    let source_unit = source_unit.id();
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.source_unit() == source_unit
                && declaration.name() == "entry"
                && (
                    declaration.name_span().start(),
                    declaration.name_span().end(),
                ) == case.entry_span
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err("query lost the unique original driver declaration".to_owned());
    };
    let entry_item = parsed
        .ast()
        .items()
        .get(entry.root())
        .map_err(|error| format!("original driver root Item: {error:?}"))?;
    require(
        matches!(entry_item.payload(), Item::Function { .. })
            && driver_function.origin == Origin::Source(entry_item.span()),
        "returned driver is the exact original entry Function",
        entry,
    )?;
    let driver_span = entry_item.span();
    let mut all_conditions = Vec::new();
    for (_, node) in parsed.ast().expressions().iter() {
        let Expression::When { entries, .. } = node.payload() else {
            continue;
        };
        for entry in entries {
            for condition in &entry.conditions {
                let (span, expression) = match condition {
                    WhenCondition::Expression(expression) => {
                        let node = parsed
                            .ast()
                            .expressions()
                            .get(*expression)
                            .map_err(|error| format!("original condition AST: {error:?}"))?;
                        (
                            node.span(),
                            Some(UnitExpressionId::new(source_unit, *expression)),
                        )
                    }
                    WhenCondition::TypeTest {
                        operator_span,
                        type_ref,
                        ..
                    } => {
                        let target =
                            parsed.ast().type_refs().get(*type_ref).map_err(|error| {
                                format!("original type condition AST: {error:?}")
                            })?;
                        (
                            sources
                                .span(
                                    parsed.source_id(),
                                    operator_span.start(),
                                    target.span().end(),
                                )
                                .map_err(|error| {
                                    format!("original type condition span: {error:?}")
                                })?,
                            None,
                        )
                    }
                    _ => return Err("unexpected condition in frozen enum sources".to_owned()),
                };
                all_conditions.push((entry.span, span, expression));
            }
        }
    }
    require(
        all_conditions.len() == case.conditions
            && all_conditions.iter().filter(|row| row.2.is_some()).count()
                == case.expression_conditions,
        "all original when conditions enumerated",
        &all_conditions,
    )?;

    // All original condition spans are inspected, including the Guide's two `is` arms.
    // The full original driver remains present and performs its ordinary constructions.
    for &(entry_span, condition_span, expression) in &all_conditions {
        let mut compares = Vec::new();
        for module in &program.modules {
            for function in &module.functions {
                for instruction in &function.instructions {
                    let origin = instruction.origin.span();
                    if matches!(instruction.operation, Operation::TaggedConstruct { .. })
                        && (contains(condition_span, origin) || origin == entry_span)
                    {
                        return Err(format!(
                            "condition constructed an enum value: {:?}; {instruction:?}",
                            sources.slice(condition_span)
                        ));
                    }
                    if matches!(instruction.operation, Operation::Compare { .. })
                        && matches!(instruction.origin, Origin::Source(span)
                            if span == condition_span || span == entry_span)
                    {
                        compares.push((module, function, instruction));
                    }
                }
            }
        }
        let [(module, function, compare)] = compares.as_slice() else {
            return Err(format!(
                "one source-anchored tag compare per original condition: {:?}; count={}",
                sources.slice(condition_span),
                compares.len()
            ));
        };
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right,
        } = &compare.operation
        else {
            return Err(format!("expected tag equality: {compare:?}"));
        };
        let [EntityId::Value(result)] = compare.results.as_slice() else {
            return Err(format!("one Boolean comparison result: {compare:?}"));
        };
        let result_type = function
            .entity(EntityId::Value(*result))
            .map(|entity| entity.ty);
        require(
            matches!(result_type, Some(EntityType::Value(ty))
            if module.types.get(ty.index()) == Some(&SsaTypeKind::Boolean)),
            "tag comparison Boolean result",
            result_type,
        )?;
        let terminator = function
            .block(compare.block)
            .and_then(|block| block.terminator.as_ref())
            .ok_or_else(|| format!("missing actual when terminator: {:?}", compare.block))?;
        require(
            terminator.origin == Origin::Source(entry_span)
                && matches!(terminator.kind, TerminatorKind::Conditional { condition, .. } if condition == *result),
            "actual when block consumes this exact Boolean tag comparison",
            terminator,
        )?;

        // The queried function must be the actual callee of an original driver Call.
        // Planner templates take the complete root Item span (unit_plan.rs), not its name alone.
        let mut actual_calls = 0;
        for instruction in &driver_function.instructions {
            let Operation::DirectCall { callee, .. } = &instruction.operation else {
                continue;
            };
            if *callee != function.id {
                continue;
            }
            let Origin::Source(call_span) = instruction.origin else {
                return Err("original driver Call has no source identity".to_owned());
            };
            require(
                contains(driver_span, call_span),
                "Call belongs to the original driver body",
                call_span,
            )?;
            let descriptors = typed
                .calls()
                .iter()
                .filter(|call| {
                    call.expression().source_unit() == source_unit
                        && parsed
                            .ast()
                            .expressions()
                            .get(call.expression().expression())
                            .is_ok_and(|node| {
                                node.span() == call_span
                                    && matches!(node.payload(), Expression::Call { .. })
                            })
                })
                .collect::<Vec<_>>();
            let [call] = descriptors.as_slice() else {
                return Err(format!(
                    "unique original driver Call descriptor at {call_span:?}"
                ));
            };
            let UnitCallTarget::Declaration(target) = call.target() else {
                return Err(format!(
                    "original enum helper Call is not declaration-directed: {call:?}"
                ));
            };
            let declarations = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| declaration.id() == target)
                .collect::<Vec<_>>();
            let [declaration] = declarations.as_slice() else {
                return Err(format!("missing original helper declaration: {target:?}"));
            };
            require(
                declaration.source_unit() == source_unit,
                "callee original source unit",
                declaration,
            )?;
            let item = parsed
                .ast()
                .items()
                .get(declaration.root())
                .map_err(|error| format!("callee original root Item: {error:?}"))?;
            require(
                parsed.roots().contains(&declaration.root())
                    && matches!(item.payload(), Item::Function { .. })
                    && function.origin == Origin::Source(item.span())
                    && contains(item.span(), entry_span),
                "driver Call target is the exact original when-containing Function",
                declaration,
            )?;
            actual_calls += 1;
        }
        require(
            actual_calls != 0,
            "original driver reaches the queried enum helper",
            function.id,
        )?;
        let Operation::TaggedDiscriminant { owner } = &definition(function, *left)?.operation
        else {
            return Err(format!(
                "comparison must read the actual subject tag: {compare:?}"
            ));
        };
        let Some(expression) = expression else {
            continue;
        };

        // The expected variant originates in the same analysis's actual descriptor and
        // nominal declaration, not an SSA integer/type/arena-order guess.
        let descriptor = typed
            .construction(expression)
            .ok_or_else(|| format!("missing original condition construction: {expression:?}"))?;
        let UnitConstructionTarget::EnumCase(symbol) = descriptor.target() else {
            return Err(format!("condition is not an enum case: {descriptor:?}"));
        };
        let Some(UnitTypeKind::Nominal { declaration, .. }) =
            typed.types().get(descriptor.result_type())
        else {
            return Err(format!("condition has no nominal root: {descriptor:?}"));
        };
        let nominal = typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| format!("missing actual enum declaration: {declaration:?}"))?;
        let selected = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .filter(|(_, case)| case.value_symbol() == symbol)
            .collect::<Vec<_>>();
        let [(variant, selected)] = selected.as_slice() else {
            return Err(format!(
                "unique actual declaration/case identity: {symbol:?}"
            ));
        };
        require(
            descriptor.expression() == expression
                && descriptor.arguments().is_empty()
                && selected.payloads().is_empty(),
            "original payloadless case descriptor",
            descriptor,
        )?;
        require(
            matches!(&definition(function, *right)?.operation,
            Operation::Constant(ScalarConstant::Integer(actual)) if *actual == *variant as i128),
            "tag constant equals descriptor-selected declaration order",
            (*declaration, symbol, variant),
        )?;

        // Independently anchor the tagged root to the same case's original driver
        // construction. Both enums reuse On/Off spelling with reversed declaration order.
        let mut births = Vec::new();
        for birth in typed.constructions().iter().filter(|birth| {
            birth.expression().source_unit() == source_unit
                && birth.expression() != expression
                && birth.target() == descriptor.target()
                && birth.result_type() == descriptor.result_type()
        }) {
            let span = parsed
                .ast()
                .expressions()
                .get(birth.expression().expression())
                .map_err(|error| format!("original driver construction AST: {error:?}"))?
                .span();
            for instruction in &driver_function.instructions {
                if instruction.origin == Origin::Source(span)
                    && let Operation::TaggedConstruct {
                        tagged, variant, ..
                    } = &instruction.operation
                {
                    births.push((*tagged, *variant, birth.expression()));
                }
            }
        }
        let [(tagged, actual_variant, birth)] = births.as_slice() else {
            return Err(format!(
                "one real same-case construction in original driver: {births:?}"
            ));
        };
        require(
            actual_variant == variant
                && function
                    .entity(EntityId::Value(*owner))
                    .map(|entity| entity.ty)
                    == Some(EntityType::Value(*tagged)),
            "subject tagged root equals real descriptor-matched driver construction",
            (*declaration, symbol, variant, birth, tagged),
        )?;
    }
    Ok(())
}
