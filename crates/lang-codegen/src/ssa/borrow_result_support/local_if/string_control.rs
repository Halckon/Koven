//! Statement-only controls for ordinary String result bindings.
use std::{cmp::Reverse, collections::BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SourceUnitId,
    parser::{
        BinaryOperator, Expression, Item, ParsedFile, Statement, VariableKind, WhenCondition,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, Copyability,
        IntegerRangeOperationKind, NominalKind, ParameterMode, UnitCallTarget,
        UnitConstructionTarget, UnitExpressionId, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::ordinary_local::control_tail;
use super::ungroup;
use crate::ssa::refine::is_integer_builtin;

pub(in crate::ssa) struct StatementControl {
    pub(in crate::ssa) arms: Vec<StatementId>,
    operands: Vec<ExpressionId>,
    integer_selection: bool,
    value_subject: Option<(ExpressionId, UnitTypeId)>,
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
        self.allow_reachable(id, file, typed, unit, allowed);
        allow_cutoff(file, self.unreachable.clone(), allowed);
    }

    fn allow_reachable(
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
                self.integer_selection && ordinary_selector_call(typed, unit, expression)
            });
        }
    }
}

fn ordinary_selector_call(
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    expression: ExpressionId,
) -> bool {
    typed
        .call(UnitExpressionId::new(unit, expression))
        .is_some_and(|call| {
            call.result_source() == CallableResultSource::Owned
                && call.receiver().is_none()
                && matches!(
                    call.target(),
                    UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                )
                && call
                    .arguments()
                    .iter()
                    .all(|arg| arg.mode() != ParameterMode::Inout && !arg.crosses_thread())
        })
}

/// Allow only expressions fully contained in a typed finite-domain cutoff.
fn allow_cutoff(file: &ParsedFile, mut spans: Vec<Span>, allowed: &mut BTreeSet<usize>) {
    if spans.is_empty() {
        return;
    }
    spans.retain(|span| span.source_id() == file.source_id());
    spans.sort_unstable_by_key(|span| (span.start(), Reverse(span.end())));
    let mut farthest = None;
    spans.retain(|span| {
        let extends = farthest.is_none_or(|end| span.end() > end);
        if extends {
            farthest = Some(span.end());
        }
        extends
    });
    // Among spans starting before an expression, the last retained span has
    // the largest end. Do not merge overlapping spans into a wider allowance.
    allowed.extend(file.ast().expressions().iter().filter_map(|(id, node)| {
        let span = node.span();
        let end = spans.partition_point(|cutoff| cutoff.start() <= span.start());
        (end > 0 && span.end() <= spans[end - 1].end()).then_some(id.index())
    }));
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
            value_subject: None,
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
                value_subject: subject
                    .filter(|_| integer || string || selector_type == boolean || tagged.is_some())
                    .map(|subject| (subject, selector_type)),
                unreachable,
            })
        }
        _ => None,
    }
}

// An active binding supplies its exact exit facts. String concatenation
// admits only the typed operation; other controls keep their own gates.
struct StringControlWork {
    statements: Vec<StatementId>,
    unreachable: Vec<Span>,
    value_subjects: Vec<(ExpressionId, UnitTypeId)>,
}

pub(in crate::ssa) fn string_statement_controls(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    statements: &[StatementId],
    allows_return: impl Fn(ExpressionId) -> bool,
    allow_concat: bool,
    allowed: &mut BTreeSet<usize>,
) {
    let mut work = StringControlWork {
        statements: statements.to_vec(),
        unreachable: Vec::new(),
        value_subjects: Vec::new(),
    };
    loop {
        if let Some((subject, ty)) = work.value_subjects.pop() {
            allow_value_controls(
                file,
                typed,
                unit,
                (subject, Some(ty)),
                &allows_return,
                allowed,
                &mut work,
            );
            continue;
        }
        let Some(statement) = work.statements.pop() else {
            break;
        };
        let Ok(node) = file.ast().statements().get(statement) else {
            continue;
        };
        match node.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                work.statements.extend_from_slice(elements);
            }
            Statement::Expression { expression } => {
                let Ok(id) = ungroup(file, *expression, node.span()) else {
                    continue;
                };
                if let Some(control) = statement_control(file, typed, unit, id) {
                    control.allow_reachable(id, file, typed, unit, allowed);
                    work.unreachable.extend(control.unreachable);
                    work.statements.extend(control.arms);
                    if allow_concat {
                        work.value_subjects.extend(control.value_subject);
                    }
                } else if allows_return(id)
                    && let Ok(node) = file.ast().expressions().get(id)
                    && let Expression::Return { value, .. } = node.payload()
                {
                    allowed.insert(id.index());
                    if allow_concat && let Some(value) = value {
                        allow_value_controls(
                            file,
                            typed,
                            unit,
                            (*value, None),
                            &allows_return,
                            allowed,
                            &mut work,
                        );
                    }
                } else if allow_concat {
                    allow_value_controls(
                        file,
                        typed,
                        unit,
                        (id, None),
                        &allows_return,
                        allowed,
                        &mut work,
                    );
                }
            }
            Statement::LocalVariable { declaration } if allow_concat => {
                if let Ok(node) = file.ast().items().get(*declaration)
                    && let Item::Variable {
                        kind: VariableKind::Val | VariableKind::Var,
                        initializer,
                        ..
                    } = node.payload()
                {
                    allow_value_controls(
                        file,
                        typed,
                        unit,
                        (*initializer, None),
                        &allows_return,
                        allowed,
                        &mut work,
                    );
                }
            }
            _ => {}
        }
    }
    allow_cutoff(file, work.unreachable, allowed);
}

fn allow_value_controls(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    root: (ExpressionId, Option<UnitTypeId>),
    allows_return: &impl Fn(ExpressionId) -> bool,
    allowed: &mut BTreeSet<usize>,
    work: &mut StringControlWork,
) {
    let Some(string) = typed.types().builtin(BuiltinType::String) else {
        return;
    };
    let type_of = |id| typed.expression_type(UnitExpressionId::new(unit, id));
    let is_string = |id| type_of(id) == Some(string);
    let boolean = typed.types().builtin(BuiltinType::Boolean);
    let is_integer = |ty| matches!(typed.types().get(ty), Some(UnitTypeKind::Builtin(builtin)) if is_integer_builtin(*builtin));
    let mut pending = vec![root];
    while let Some((id, expected)) = pending.pop() {
        let Ok(node) = file.ast().expressions().get(id) else {
            continue;
        };
        let integer_context = expected.is_some_and(is_integer);
        let boolean_context = expected.is_some() && expected == boolean;
        let scalar_value = |id| {
            type_of(id)
                .filter(|ty| {
                    integer_context && is_integer(*ty) || boolean_context && Some(*ty) == boolean
                })
                .map(|ty| (id, Some(ty)))
        };
        match node.payload() {
            Expression::Group { expression } => pending.push((*expression, expected)),
            Expression::Binary {
                left,
                operator: BinaryOperator::Add,
                right,
                ..
            } if is_string(id) && is_string(*left) && is_string(*right) => {
                allowed.insert(id.index());
                pending.extend([(*left, Some(string)), (*right, Some(string))]);
            }
            Expression::Binary { left, right, .. } if integer_context || boolean_context => {
                pending.extend([*left, *right].into_iter().filter_map(scalar_value));
            }
            Expression::Prefix { operand, .. } if integer_context || boolean_context => {
                pending.extend(scalar_value(*operand));
            }
            Expression::Call { arguments, .. }
                if typed
                    .construction(UnitExpressionId::new(unit, id))
                    .is_some_and(|construction| {
                        Some(construction.result_type()) == expected
                            && construction.arguments().len() == arguments.len()
                            && matches!(construction.target(), UnitConstructionTarget::EnumCase(_))
                    }) =>
            {
                pending.extend(arguments.iter().map(|argument| (argument.value, None)));
            }
            Expression::Call { arguments, .. }
                if typed
                    .call(UnitExpressionId::new(unit, id))
                    .is_some_and(|call| {
                        !call.aborts()
                            && typed.types().get(call.return_type())
                                != Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
                            && call.result_source() == CallableResultSource::Owned
                            && call.target() != UnitCallTarget::FunctionValue
                            && call
                                .receiver()
                                .is_none_or(|receiver| receiver.mode() != ParameterMode::Inout)
                            && call.arguments().len() == arguments.len()
                            && call.arguments().iter().all(|argument| {
                                argument.mode() != ParameterMode::Inout
                                    && !argument.crosses_thread()
                            })
                    }) =>
            {
                let scalar_call = integer_context && ordinary_selector_call(typed, unit, id);
                pending.extend(arguments.iter().map(|argument| {
                    if scalar_call {
                        scalar_value(argument.value).unwrap_or((argument.value, None))
                    } else {
                        (argument.value, None)
                    }
                }));
            }
            Expression::If {
                else_branch: Some(_),
                ..
            }
            | Expression::When { .. }
                if let Some(expected) = expected
                    && type_of(id) == Some(expected) =>
            {
                if let Expression::When { entries, .. } = node.payload()
                    && typed
                        .when_closed_after(UnitExpressionId::new(unit, id))
                        .is_none()
                    && entries.last().is_none_or(|entry| entry.else_span.is_none())
                {
                    continue;
                }
                if let Some(control) = statement_control(file, typed, unit, id)
                    && let Some(arms) = checked_value_arms(
                        file,
                        typed,
                        unit,
                        &control.arms,
                        expected,
                        allows_return,
                    )
                {
                    control.allow_reachable(id, file, typed, unit, allowed);
                    work.unreachable.extend(control.unreachable);
                    work.value_subjects.extend(control.value_subject);
                    // The tail proof covers the final expression of each arm.
                    // Prefix statements can have their own conditional exits.
                    for arm in control.arms {
                        if let Ok(node) = file.ast().statements().get(arm)
                            && let Statement::Block { elements }
                            | Statement::ControlBody { elements } = node.payload()
                            && let Some((_, prefix)) = elements.split_last()
                        {
                            work.statements.extend_from_slice(prefix);
                        }
                    }
                    for arm in arms {
                        match arm {
                            ValueArm::Value(value) => {
                                if is_integer(expected) || Some(expected) == boolean {
                                    collect_condition_operators(file, value, allowed, |call| {
                                        is_integer(expected)
                                            && ordinary_selector_call(typed, unit, call)
                                    });
                                }
                                pending.push((value, Some(expected)));
                            }
                            ValueArm::Return { expression, value } => {
                                allowed.insert(expression.index());
                                pending.extend(value.map(|value| (value, None)));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

enum ValueArm {
    Value(ExpressionId),
    Return {
        expression: ExpressionId,
        value: Option<ExpressionId>,
    },
}

fn checked_value_arms(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    unit: SourceUnitId,
    branches: &[StatementId],
    expected: UnitTypeId,
    allows_return: &impl Fn(ExpressionId) -> bool,
) -> Option<Vec<ValueArm>> {
    let classify = |branch| {
        let expression = control_tail(file, branch)?;
        let node = file.ast().expressions().get(expression).ok()?;
        if let Expression::Return { value, .. } = node.payload() {
            return allows_return(expression).then_some(ValueArm::Return {
                expression,
                value: *value,
            });
        }
        let id = UnitExpressionId::new(unit, expression);
        let actual = typed.expression_type(id)?;
        let matches_subject = actual == expected
            || matches!(typed.types().get(actual), Some(UnitTypeKind::EnumCase { root, .. })
                if *root == expected);
        (matches_subject && typed.expression_falls_through(id) == Some(true))
            .then_some(ValueArm::Value(expression))
    };
    let arms = branches
        .iter()
        .copied()
        .map(classify)
        .collect::<Option<Vec<_>>>()?;
    arms.iter()
        .any(|arm| matches!(arm, ValueArm::Value(_)))
        .then_some(arms)
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
