//! Statement-only controls for ordinary String result bindings.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SourceUnitId,
    parser::{Expression, ParsedFile, Statement, WhenCondition},
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, Copyability,
        IntegerRangeOperationKind, NominalKind, ParameterMode, UnitCallTarget,
        UnitConstructionTarget, UnitExpressionId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::{contains, ungroup};
use crate::ssa::refine::is_integer_builtin;

pub(in crate::ssa) struct StatementControl {
    pub(in crate::ssa) arms: Vec<StatementId>,
    operands: Vec<ExpressionId>,
    integer_selection: bool,
    unreachable: Vec<Span>,
}

impl StatementControl {
    pub(in crate::ssa) fn allow(
        &self,
        id: ExpressionId,
        file: &ParsedFile,
        typed: &CompilationUnitTypes,
        unit: SourceUnitId,
        allowed: &mut BTreeSet<usize>,
    ) {
        allowed.insert(id.index());
        for &operand in &self.operands {
            collect_condition_operators(file, operand, allowed, |expression| {
                self.integer_selection
                    && typed
                        .call(UnitExpressionId::new(unit, expression))
                        .is_some_and(|call| {
                            call.result_source() == CallableResultSource::Owned
                                && call.receiver().is_none()
                                && matches!(
                                    call.target(),
                                    UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                                )
                                && call.arguments().iter().all(|arg| {
                                    arg.mode() != ParameterMode::Inout && !arg.crosses_thread()
                                })
                        })
            });
        }
        // This syntax is beyond the typed finite-domain cutoff. It cannot
        // supply an exit fact or cause the lexical gate to reject live code.
        if !self.unreachable.is_empty() {
            allowed.extend(file.ast().expressions().iter().filter_map(|(id, node)| {
                self.unreachable
                    .iter()
                    .any(|&span| contains(span, node.span()))
                    .then_some(id.index())
            }));
        }
    }
}

pub(in crate::ssa) fn statement_control(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    id: ExpressionId,
) -> Option<StatementControl> {
    let type_of = |expression| typed.expression_type(UnitExpressionId::new(unit, expression));
    let boolean = typed.types().builtin(BuiltinType::Boolean)?;
    let node = file.ast().expressions().get(id).ok()?;
    match node.payload() {
        Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } if type_of(*condition) == Some(boolean) => Some(StatementControl {
            arms: std::iter::once(*then_branch).chain(*else_branch).collect(),
            operands: vec![*condition],
            integer_selection: false,
            unreachable: Vec::new(),
        }),
        Expression::When {
            subject, entries, ..
        } => {
            let selector_type = match subject {
                Some(subject) => type_of(*subject)?,
                None => boolean,
            };
            let integer = matches!(typed.types().get(selector_type),
                Some(UnitTypeKind::Builtin(builtin)) if is_integer_builtin(*builtin));
            let string = typed.types().get(selector_type)
                == Some(&UnitTypeKind::Builtin(BuiltinType::String));
            let tagged = match typed.types().get(selector_type) {
                Some(UnitTypeKind::Nominal {
                    declaration,
                    arguments,
                }) => typed
                    .signatures()
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .filter(|nominal| {
                        nominal.kind() == NominalKind::EnumClass
                            && nominal.type_parameters().len() == arguments.len()
                    }),
                _ => None,
            };
            if selector_type != boolean && !integer && !string && tagged.is_none() {
                return None;
            }
            let control = UnitExpressionId::new(unit, id);
            let reached = typed.when_closed_after(control).unwrap_or(entries.len());
            let mut arms = Vec::new();
            let mut operands: Vec<_> = subject.iter().copied().collect();
            let mut unreachable = Vec::new();
            for (index, entry) in entries.iter().enumerate() {
                if index >= reached {
                    unreachable.push(entry.span);
                    continue;
                }
                arms.push(entry.body);
                let selectors = typed
                    .when_final_selector_count(control, index)
                    .unwrap_or(entry.conditions.len());
                for (index, condition) in entry.conditions.iter().enumerate() {
                    if index < selectors {
                        let operand = match condition {
                            WhenCondition::Expression(expression) if string => {
                                let leaf = ungroup(file, *expression, entry.span).ok()?;
                                if type_of(*expression) != Some(selector_type)
                                    || !matches!(
                                        file.ast().expressions().get(leaf).ok()?.payload(),
                                        Expression::String { .. }
                                    )
                                {
                                    return None;
                                }
                                *expression
                            }
                            WhenCondition::Expression(expression) if tagged.is_some() => {
                                let leaf = ungroup(file, *expression, entry.span).ok()?;
                                let id = UnitExpressionId::new(unit, leaf);
                                let case = typed.construction(id)?;
                                let UnitConstructionTarget::EnumCase(symbol) = case.target() else {
                                    return None;
                                };
                                if typed.copyability(selector_type) != Copyability::Copyable
                                    || case.expression() != id
                                    || type_of(*expression) != Some(selector_type)
                                    || case.result_type() != selector_type
                                    || !case.arguments().is_empty()
                                    || !tagged.is_some_and(|nominal| {
                                        nominal.enum_cases().iter().any(|candidate| {
                                            candidate.value_symbol() == symbol
                                                && candidate.payloads().is_empty()
                                        })
                                    })
                                {
                                    return None;
                                }
                                *expression
                            }
                            WhenCondition::TypeTest { type_ref, .. } if tagged.is_some() => {
                                let target =
                                    typed.type_ref_type(UnitTypeRefId::new(unit, *type_ref))?;
                                let Some(UnitTypeKind::EnumCase { case, root }) =
                                    typed.types().get(target)
                                else {
                                    return None;
                                };
                                if *root != selector_type
                                    || !tagged.is_some_and(|nominal| {
                                        nominal
                                            .enum_cases()
                                            .iter()
                                            .any(|candidate| candidate.type_symbol() == *case)
                                    })
                                {
                                    return None;
                                }
                                continue;
                            }
                            WhenCondition::Expression(expression)
                                if type_of(*expression) == Some(selector_type) =>
                            {
                                *expression
                            }
                            WhenCondition::Contains { expression, .. } if integer => {
                                let range = typed.integer_range_operation(
                                    UnitExpressionId::new(unit, *expression),
                                )?;
                                if !matches!(
                                    range.kind(),
                                    IntegerRangeOperationKind::Inclusive
                                        | IntegerRangeOperationKind::Exclusive
                                ) || range.left_type() != selector_type
                                    || range.right_type() != selector_type
                                    || range.left().source_unit() != unit
                                    || range.right().source_unit() != unit
                                {
                                    return None;
                                }
                                *expression
                            }
                            _ => return None,
                        };
                        operands.push(operand);
                    } else if let WhenCondition::Expression(expression)
                    | WhenCondition::Contains { expression, .. } = condition
                    {
                        unreachable.push(file.ast().expressions().get(*expression).ok()?.span());
                    }
                }
            }
            Some(StatementControl {
                arms,
                operands,
                integer_selection: integer,
                unreachable,
            })
        }
        _ => None,
    }
}

// Only statement control belongs to this lexical walk. Initializers and loops
// have independent gates. An active binding supplies its exact exit facts.
pub(in crate::ssa) fn string_statement_controls(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    statements: &[StatementId],
    allows_return: impl Fn(ExpressionId) -> bool,
    allowed: &mut BTreeSet<usize>,
) {
    let mut pending = statements.to_vec();
    while let Some(statement) = pending.pop() {
        let Ok(node) = file.ast().statements().get(statement) else {
            continue;
        };
        match node.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                pending.extend_from_slice(elements);
            }
            Statement::Expression { expression } => {
                let Ok(id) = ungroup(file, *expression, node.span()) else {
                    continue;
                };
                if let Some(control) = statement_control(file, typed, unit, id) {
                    control.allow(id, file, typed, unit, allowed);
                    pending.extend(control.arms);
                } else if allows_return(id)
                    && file
                        .ast()
                        .expressions()
                        .get(id)
                        .is_ok_and(|node| matches!(node.payload(), Expression::Return { .. }))
                {
                    allowed.insert(id.index());
                }
            }
            _ => {}
        }
    }
}

pub(in crate::ssa) fn collect_condition_operators(
    file: &ParsedFile,
    condition: ExpressionId,
    allowed: &mut BTreeSet<usize>,
    ordinary_call: impl Fn(ExpressionId) -> bool,
) {
    // Ordinary integer selector calls can contain scalar arithmetic. Stop at
    // nested controls and initializers so their separate gates still apply.
    let mut pending = vec![condition];
    while let Some(id) = pending.pop() {
        let Ok(node) = file.ast().expressions().get(id) else {
            continue;
        };
        match node.payload() {
            Expression::Binary { left, right, .. } => {
                allowed.insert(id.index());
                pending.extend([*left, *right]);
            }
            Expression::Prefix { operand, .. }
            | Expression::Group {
                expression: operand,
            } => {
                pending.push(*operand);
            }
            Expression::Call { arguments, .. } if ordinary_call(id) => {
                pending.extend(arguments.iter().map(|argument| argument.value));
            }
            _ => {}
        }
    }
}
