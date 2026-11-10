//! 收集可能影响流事实的写入；循环预扫描只用语法，已检查条件可排除仅退出路径。

use std::{cell::OnceCell, collections::BTreeMap};

use crate::{
    ast::{AstError, ExpressionId, StatementId},
    parser::{
        BinaryOperator, Expression, Item, LiteralKind, ParameterModeMarker, PrefixOperator,
        Statement, StringPart, SyntaxAst, WhenCondition,
    },
};

enum Pending {
    Expression(ExpressionId),
    Statement(StatementId),
    ConditionalRhs(ExpressionId),
    KeepWritesOnLoopJump {
        first_write: usize,
        previous_jumps: usize,
    },
    EnterLoop(StatementId),
    LeaveLoop,
}

/// Recognize an unconditional RHS without running constant evaluation in flow checking.
pub(super) fn logical_rhs_is_required(
    ast: &SyntaxAst,
    mut left: ExpressionId,
    operator: BinaryOperator,
) -> Result<bool, AstError> {
    let mut required_value = operator == BinaryOperator::LogicalAnd;
    loop {
        match ast.expressions().get(left)?.payload() {
            Expression::Group { expression } => left = *expression,
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                required_value = !required_value;
                left = *operand;
            }
            Expression::Literal(LiteralKind::Boolean(value)) => {
                return Ok(*value == required_value);
            }
            _ => return Ok(false),
        }
    }
}

/// Syntactic outcomes prove which logical operands can execute without constant evaluation.
pub(super) struct SyntacticBooleanOutcomes<'a> {
    ast: &'a SyntaxAst,
    outcomes: OnceCell<Vec<(bool, bool)>>,
}

impl<'a> SyntacticBooleanOutcomes<'a> {
    pub(super) fn new(ast: &'a SyntaxAst) -> Self {
        Self {
            ast,
            outcomes: OnceCell::new(),
        }
    }

    pub(super) fn rhs_is_bypassed(&self, left: ExpressionId, operator: BinaryOperator) -> bool {
        let values = self.outcomes.get_or_init(|| {
            // Parser expression IDs are in child-before-parent order.
            let mut values = Vec::with_capacity(self.ast.expressions().len());
            for (_, node) in self.ast.expressions().iter() {
                let value = match node.payload() {
                    Expression::Literal(LiteralKind::Boolean(value)) => (*value, !*value),
                    expression => structural_outcomes(expression, |child| {
                        values.get(child.index()).copied().unwrap_or((true, true))
                    }),
                };
                values.push(value);
            }
            values
        });
        let (yes, no) = values.get(left.index()).copied().unwrap_or((true, true));
        match operator {
            BinaryOperator::LogicalAnd => !yes,
            BinaryOperator::LogicalOr => !no,
            _ => false,
        }
    }
}

pub(super) fn structural_outcomes(
    expression: &Expression,
    outcomes: impl Fn(ExpressionId) -> (bool, bool),
) -> (bool, bool) {
    match expression {
        Expression::Group { expression } => outcomes(*expression),
        Expression::Prefix {
            operator: PrefixOperator::Not,
            operand,
            ..
        } => {
            let (yes, no) = outcomes(*operand);
            (no, yes)
        }
        Expression::Binary {
            left,
            operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
            right,
            ..
        } => {
            let (yes, no) = outcomes(*left);
            let (rhs_yes, rhs_no) = outcomes(*right);
            if *operator == BinaryOperator::LogicalAnd {
                (yes && rhs_yes, no || (yes && rhs_no))
            } else {
                (yes || (no && rhs_yes), no && rhs_no)
            }
        }
        _ => (true, true),
    }
}

pub(super) fn collect_expression_mutation_targets(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    expression: ExpressionId,
) -> Result<Vec<ExpressionId>, AstError> {
    collect_mutation_targets(
        outcomes,
        Pending::Expression(expression),
        |_| true,
        |_| Ok(true),
        |_| true,
    )
}

pub(super) fn collect_normal_expression_mutation_targets(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    expression: ExpressionId,
    includes_inout: impl Fn(ExpressionId) -> bool,
    expression_completes: impl Fn(ExpressionId) -> Result<bool, AstError>,
    includes_loop_jump: impl Fn(ExpressionId) -> bool,
) -> Result<Vec<ExpressionId>, AstError> {
    if !expression_completes(expression)? {
        return Ok(Vec::new());
    }
    collect_mutation_targets(
        outcomes,
        Pending::Expression(expression),
        includes_inout,
        expression_completes,
        includes_loop_jump,
    )
}

pub(super) fn collect_statement_mutation_targets(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    statement: StatementId,
) -> Result<Vec<ExpressionId>, AstError> {
    collect_mutation_targets(
        outcomes,
        Pending::Statement(statement),
        |_| true,
        |_| Ok(true),
        |_| true,
    )
}

fn collect_mutation_targets(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    root: Pending,
    includes_inout: impl Fn(ExpressionId) -> bool,
    expression_completes: impl Fn(ExpressionId) -> Result<bool, AstError>,
    includes_loop_jump: impl Fn(ExpressionId) -> bool,
) -> Result<Vec<ExpressionId>, AstError> {
    let ast = outcomes.ast;
    let mut pending = vec![root];
    let mut targets = Vec::new();
    let mut loop_jumps = Vec::new();
    while let Some(node) = pending.pop() {
        match node {
            Pending::ConditionalRhs(rhs) => {
                if expression_completes(rhs)? {
                    pending.push(Pending::Expression(rhs));
                } else if let Some(&previous_jumps) = loop_jumps.last() {
                    // Only a jump to a loop already in this scan can make
                    // writes on this otherwise abrupt path observable.
                    pending.push(Pending::KeepWritesOnLoopJump {
                        first_write: targets.len(),
                        previous_jumps,
                    });
                    pending.push(Pending::Expression(rhs));
                }
            }
            Pending::KeepWritesOnLoopJump {
                first_write,
                previous_jumps,
            } => {
                if loop_jumps.last() == Some(&previous_jumps) {
                    targets.truncate(first_write);
                }
            }
            Pending::EnterLoop(body) => {
                loop_jumps.push(0);
                pending.push(Pending::LeaveLoop);
                pending.push(Pending::Statement(body));
            }
            Pending::LeaveLoop => {
                loop_jumps.pop();
            }
            Pending::Expression(expression) => match ast.expressions().get(expression)?.payload() {
                Expression::Error
                | Expression::Name
                | Expression::This
                | Expression::Literal(_)
                | Expression::SuperMember { .. } => {}
                Expression::Break { .. } | Expression::Continue { .. } => {
                    if let Some(jumps) = loop_jumps.last_mut()
                        && includes_loop_jump(expression)
                    {
                        *jumps += 1;
                    }
                }
                // 创建 callable 不执行其声明体；不能把捕获体中的写入算作当前求值效果。
                Expression::Lambda { .. } => {}
                Expression::Group { expression }
                | Expression::Cast { expression, .. }
                | Expression::TypeTest { expression, .. } => {
                    pending.push(Pending::Expression(*expression));
                }
                Expression::Prefix { operand, .. } | Expression::NonNullAssert { operand, .. } => {
                    pending.push(Pending::Expression(*operand));
                }
                Expression::Propagate { value, .. } => {
                    pending.push(Pending::Expression(*value));
                }
                Expression::String { parts } => {
                    for part in parts.iter().rev() {
                        match part {
                            StringPart::Interpolation { expression, .. } => {
                                pending.push(Pending::Expression(*expression));
                            }
                            StringPart::Text(_) | StringPart::Error(_) => {}
                        }
                    }
                }
                Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    if let Some(branch) = else_branch {
                        pending.push(Pending::Statement(*branch));
                    }
                    pending.push(Pending::Statement(*then_branch));
                    pending.push(Pending::Expression(*condition));
                }
                Expression::When {
                    subject, entries, ..
                } => {
                    for entry in entries.iter().rev() {
                        pending.push(Pending::Statement(entry.body));
                        for condition in entry.conditions.iter().rev() {
                            match condition {
                                WhenCondition::Expression(expression)
                                | WhenCondition::Contains { expression, .. } => {
                                    pending.push(Pending::Expression(*expression));
                                }
                                WhenCondition::TypeTest { .. } => {}
                            }
                        }
                    }
                    if let Some(subject) = subject {
                        pending.push(Pending::Expression(*subject));
                    }
                }
                Expression::Return { value, .. } => {
                    if let Some(value) = value {
                        pending.push(Pending::Expression(*value));
                    }
                }
                Expression::Binary {
                    left,
                    operator,
                    right,
                    ..
                } => {
                    match operator {
                        BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
                            if outcomes.rhs_is_bypassed(*left, *operator) => {}
                        BinaryOperator::LogicalAnd
                        | BinaryOperator::LogicalOr
                        | BinaryOperator::Elvis => {
                            pending.push(Pending::ConditionalRhs(*right));
                        }
                        _ => pending.push(Pending::Expression(*right)),
                    }
                    pending.push(Pending::Expression(*left));
                }
                Expression::Assignment { target, value, .. } => {
                    targets.push(*target);
                    pending.push(Pending::Expression(*value));
                    pending.push(Pending::Expression(*target));
                }
                Expression::Member { receiver, .. } => {
                    pending.push(Pending::Expression(*receiver));
                }
                Expression::Call {
                    callee, arguments, ..
                } => {
                    if includes_inout(expression) {
                        targets.extend(arguments.iter().filter_map(|argument| {
                            matches!(argument.mode_marker, Some(ParameterModeMarker::Inout(_)))
                                .then_some(argument.value)
                        }));
                    }
                    let first_operand = pending.len();
                    for operand in std::iter::once(*callee)
                        .chain(arguments.iter().map(|argument| argument.value))
                    {
                        pending.push(Pending::Expression(operand));
                        if !expression_completes(operand)? {
                            break;
                        }
                    }
                    pending[first_operand..].reverse();
                }
                Expression::Index { receiver, index } => {
                    pending.push(Pending::Expression(*index));
                    pending.push(Pending::Expression(*receiver));
                }
                Expression::CallableReference { receiver, .. } => {
                    if let Some(receiver) = receiver {
                        pending.push(Pending::Expression(*receiver));
                    }
                }
            },
            Pending::Statement(statement) => match ast.statements().get(statement)?.payload() {
                Statement::Error => {}
                Statement::Block { elements }
                | Statement::LambdaBody { elements }
                | Statement::ControlBody { elements } => {
                    for element in elements.iter().rev() {
                        pending.push(Pending::Statement(*element));
                    }
                }
                Statement::LocalVariable { declaration } => {
                    match ast.items().get(*declaration)?.payload() {
                        Item::Variable { initializer, .. } => {
                            pending.push(Pending::Expression(*initializer));
                        }
                        Item::Error
                        | Item::Modified { .. }
                        | Item::Constant { .. }
                        | Item::Function { .. }
                        | Item::Classifier(_)
                        | Item::Companion(_)
                        | Item::Deinit { .. } => {}
                    }
                }
                Statement::LocalDestructuring { initializer, .. } => {
                    pending.push(Pending::Expression(*initializer));
                }
                Statement::While {
                    condition, body, ..
                } => {
                    pending.push(Pending::EnterLoop(*body));
                    pending.push(Pending::Expression(*condition));
                }
                Statement::For { source, body, .. } => {
                    pending.push(Pending::EnterLoop(*body));
                    pending.push(Pending::Expression(*source));
                }
                Statement::Loop { body, .. } => {
                    pending.push(Pending::EnterLoop(*body));
                }
                Statement::Expression { expression } => {
                    pending.push(Pending::Expression(*expression));
                }
            },
        }
    }
    Ok(targets)
}

/// Preserve invalidation on any incoming normal control edge.
pub(super) fn merge_flow_versions<K: Copy + Ord>(
    left: &BTreeMap<K, u64>,
    right: &BTreeMap<K, u64>,
) -> BTreeMap<K, u64> {
    let mut merged = left.clone();
    for (&key, &version) in right {
        let previous = merged.entry(key).or_default();
        *previous = (*previous).max(version);
    }
    merged
}
