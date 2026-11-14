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
    Write(ExpressionId),
    Conditional(ConditionalSite),
    DiscardNormalWrites(usize),
    SaveBranchWrites(usize),
    JoinBranchWrites(usize),
    LogicalRight {
        left: ExpressionId,
        right: ExpressionId,
        left_result: bool,
        first_write: usize,
    },
    FinishLogicalRight {
        right_start: usize,
        first_write: usize,
        left_writes: Vec<ExpressionId>,
    },
    BeginWhen,
    WhenSelector(ExpressionId, StatementId),
    FinishWhenSelector {
        operand: ExpressionId,
        branch: StatementId,
        first_write: usize,
    },
    ResumeWhenMiss {
        first_write: usize,
        writes: Vec<ExpressionId>,
    },
    WhenBranch(StatementId),
    FinishWhen,
    EnterLoop(StatementId),
    LeaveLoop,
}

enum ConditionalSite {
    RightOperand(ExpressionId),
    Branch(StatementId),
}

struct LoopWrites {
    first_write: usize,
    captured_through: usize,
    on_jump: BTreeMap<usize, ExpressionId>,
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
    loop_jump_offsets: OnceCell<Vec<usize>>,
}

impl<'a> SyntacticBooleanOutcomes<'a> {
    pub(super) fn new(ast: &'a SyntaxAst) -> Self {
        Self {
            ast,
            outcomes: OnceCell::new(),
            loop_jump_offsets: OnceCell::new(),
        }
    }

    fn possible_results(&self, expression: ExpressionId) -> (bool, bool) {
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
        values
            .get(expression.index())
            .copied()
            .unwrap_or((true, true))
    }

    fn may_contain_loop_jump(&self, expression: ExpressionId) -> Result<bool, AstError> {
        let span = self.ast.expressions().get(expression)?.span();
        let offsets = self.loop_jump_offsets.get_or_init(|| {
            let mut offsets: Vec<_> = self
                .ast
                .expressions()
                .iter()
                .filter_map(|(_, node)| {
                    matches!(
                        node.payload(),
                        Expression::Break { .. } | Expression::Continue { .. }
                    )
                    .then_some(node.span().start())
                })
                .collect();
            offsets.sort_unstable();
            offsets
        });
        let index = offsets.partition_point(|&offset| offset < span.start());
        Ok(offsets
            .get(index)
            .is_some_and(|&offset| offset < span.end()))
    }

    pub(super) fn rhs_is_bypassed(&self, left: ExpressionId, operator: BinaryOperator) -> bool {
        let (yes, no) = self.possible_results(left);
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
        |_| Ok(true),
        |_| false,
        false,
    )
}

pub(super) fn collect_normal_expression_mutation_targets(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    expression: ExpressionId,
    includes_inout: impl Fn(ExpressionId) -> bool,
    expression_completes: impl Fn(ExpressionId) -> Result<bool, AstError>,
    branch_completes: impl Fn(StatementId) -> Result<bool, AstError>,
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
        branch_completes,
        includes_loop_jump,
        true,
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
        |_| Ok(true),
        |_| false,
        false,
    )
}

fn collect_mutation_targets(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    root: Pending,
    includes_inout: impl Fn(ExpressionId) -> bool,
    expression_completes: impl Fn(ExpressionId) -> Result<bool, AstError>,
    branch_completes: impl Fn(StatementId) -> Result<bool, AstError>,
    includes_loop_jump: impl Fn(ExpressionId) -> bool,
    track_loop_writes: bool,
) -> Result<Vec<ExpressionId>, AstError> {
    let ast = outcomes.ast;
    let mut pending = vec![root];
    let mut targets: Vec<ExpressionId> = Vec::new();
    let mut loops: Vec<LoopWrites> = Vec::new();
    let mut branch_writes = Vec::new();
    let mut when_normal_prefixes = Vec::new();
    while let Some(node) = pending.pop() {
        match node {
            Pending::Write(target) => targets.push(target),
            Pending::Conditional(site) => {
                let (child, completes) = match site {
                    ConditionalSite::RightOperand(rhs) => {
                        (Pending::Expression(rhs), expression_completes(rhs)?)
                    }
                    ConditionalSite::Branch(branch) => {
                        (Pending::Statement(branch), branch_completes(branch)?)
                    }
                };
                if !completes {
                    if loops.is_empty() {
                        continue;
                    }
                    pending.push(Pending::DiscardNormalWrites(targets.len()));
                } else if !loops.is_empty() && matches!(site, ConditionalSite::Branch(_)) {
                    pending.push(Pending::SaveBranchWrites(targets.len()));
                }
                pending.push(child);
            }
            Pending::DiscardNormalWrites(first_write) | Pending::SaveBranchWrites(first_write) => {
                if matches!(node, Pending::SaveBranchWrites(_)) {
                    branch_writes.extend(targets.drain(first_write..));
                } else {
                    targets.truncate(first_write);
                }
                if let Some(frame) = loops.last_mut() {
                    frame.captured_through = frame.captured_through.min(first_write);
                }
            }
            Pending::JoinBranchWrites(first_branch) => {
                targets.extend(branch_writes.drain(first_branch..));
            }
            Pending::LogicalRight {
                left,
                right,
                left_result,
                first_write,
            } => {
                let left_writes = targets.split_off(first_write);
                targets.extend(writes_on_boolean_edge(
                    outcomes,
                    left,
                    left_result,
                    &left_writes,
                )?);
                if let Some(frame) = loops.last_mut() {
                    frame.captured_through = frame.captured_through.min(first_write);
                }
                pending.push(Pending::FinishLogicalRight {
                    right_start: targets.len(),
                    first_write,
                    left_writes,
                });
                pending.push(Pending::Conditional(ConditionalSite::RightOperand(right)));
            }
            Pending::FinishLogicalRight {
                right_start,
                first_write,
                left_writes,
            } => {
                targets.splice(first_write..right_start, left_writes);
                if let Some(frame) = loops.last_mut() {
                    frame.captured_through = frame.captured_through.min(first_write);
                }
            }
            Pending::BeginWhen => when_normal_prefixes.push(targets.len()),
            Pending::WhenSelector(operand, branch) => {
                pending.push(Pending::FinishWhenSelector {
                    operand,
                    branch,
                    first_write: targets.len(),
                });
                pending.push(Pending::Expression(operand));
            }
            Pending::FinishWhenSelector {
                operand,
                branch,
                first_write,
            } => {
                let writes = &targets[first_write..];
                let matched = writes_on_boolean_edge(outcomes, operand, true, writes)?;
                let missed = writes_on_boolean_edge(outcomes, operand, false, writes)?;
                if branch_completes(branch)? {
                    *when_normal_prefixes
                        .last_mut()
                        .expect("matching when frame") = first_write;
                    branch_writes.extend(&matched);
                }
                // Selector jump effects are recorded separately.
                // The branch sees matched effects; the next selector sees missed effects.
                targets.truncate(first_write);
                targets.extend(matched);
                if let Some(frame) = loops.last_mut() {
                    frame.captured_through = frame.captured_through.min(first_write);
                }
                pending.push(Pending::ResumeWhenMiss {
                    first_write,
                    writes: missed,
                });
                pending.push(Pending::Conditional(ConditionalSite::Branch(branch)));
            }
            Pending::ResumeWhenMiss {
                first_write,
                writes,
            } => {
                targets.truncate(first_write);
                targets.extend(writes);
                if let Some(frame) = loops.last_mut() {
                    frame.captured_through = frame.captured_through.min(first_write);
                }
            }
            Pending::WhenBranch(branch) => {
                if branch_completes(branch)? {
                    *when_normal_prefixes
                        .last_mut()
                        .expect("matching when frame") = targets.len();
                }
                pending.push(Pending::Conditional(ConditionalSite::Branch(branch)));
            }
            Pending::FinishWhen => {
                let prefix = when_normal_prefixes.pop().expect("matching when frame");
                targets.truncate(prefix);
                if let Some(frame) = loops.last_mut() {
                    frame.captured_through = frame.captured_through.min(prefix);
                }
            }
            Pending::EnterLoop(body) => {
                if track_loop_writes {
                    loops.push(LoopWrites {
                        first_write: targets.len(),
                        captured_through: targets.len(),
                        on_jump: BTreeMap::new(),
                    });
                    pending.push(Pending::LeaveLoop);
                    if !branch_completes(body)? {
                        // Keep only effects captured by this loop's break or continue.
                        pending.push(Pending::DiscardNormalWrites(targets.len()));
                    }
                }
                pending.push(Pending::Statement(body));
            }
            Pending::LeaveLoop => {
                let mut frame = loops.pop().expect("matching loop frame");
                for target in &targets[frame.first_write..] {
                    frame.on_jump.remove(&target.index());
                }
                targets.extend(frame.on_jump.into_values());
            }
            Pending::Expression(expression) => match ast.expressions().get(expression)?.payload() {
                Expression::Error
                | Expression::Name
                | Expression::This
                | Expression::Literal(_)
                | Expression::SuperMember { .. } => {}
                Expression::Break { .. } | Expression::Continue { .. } => {
                    if let Some(frame) = loops.last_mut()
                        && includes_loop_jump(expression)
                    {
                        // Keep the prefix reaching this jump before visiting sibling paths.
                        frame.on_jump.extend(
                            targets[frame.captured_through..]
                                .iter()
                                .map(|&target| (target.index(), target)),
                        );
                        frame.captured_through = targets.len();
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
                    pending.push(Pending::JoinBranchWrites(branch_writes.len()));
                    if let Some(branch) = else_branch {
                        pending.push(Pending::Conditional(ConditionalSite::Branch(*branch)));
                    }
                    pending.push(Pending::Conditional(ConditionalSite::Branch(*then_branch)));
                    pending.push(Pending::Expression(*condition));
                }
                Expression::When {
                    subject, entries, ..
                } => {
                    pending.push(Pending::JoinBranchWrites(branch_writes.len()));
                    let mut track_prefix = !loops.is_empty()
                        && entries
                            .last()
                            .is_some_and(|entry| entry.else_span.is_some());
                    if track_prefix {
                        // An unmatched path may reach any later selector. Only trim
                        // selector effects when each selector can complete.
                        for condition in entries.iter().flat_map(|entry| &entry.conditions) {
                            if let Some(operand) = when_operand(condition)
                                && !expression_completes(operand)?
                            {
                                track_prefix = false;
                                break;
                            }
                        }
                    }
                    if track_prefix {
                        pending.push(Pending::FinishWhen);
                    }
                    for entry in entries.iter().rev() {
                        if track_prefix
                            && subject.is_none()
                            && let [WhenCondition::Expression(operand)] =
                                entry.conditions.as_slice()
                        {
                            pending.push(Pending::WhenSelector(*operand, entry.body));
                            continue;
                        }
                        pending.push(if track_prefix {
                            Pending::WhenBranch(entry.body)
                        } else {
                            Pending::Conditional(ConditionalSite::Branch(entry.body))
                        });
                        for operand in entry.conditions.iter().rev().filter_map(when_operand) {
                            pending.push(Pending::Expression(operand));
                        }
                    }
                    if track_prefix {
                        pending.push(Pending::BeginWhen);
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
                        BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
                            if !loops.is_empty() && outcomes.may_contain_loop_jump(*right)? =>
                        {
                            pending.push(Pending::LogicalRight {
                                left: *left,
                                right: *right,
                                left_result: *operator == BinaryOperator::LogicalAnd,
                                first_write: targets.len(),
                            });
                        }
                        BinaryOperator::LogicalAnd
                        | BinaryOperator::LogicalOr
                        | BinaryOperator::Elvis => {
                            pending
                                .push(Pending::Conditional(ConditionalSite::RightOperand(*right)));
                        }
                        _ => pending.push(Pending::Expression(*right)),
                    }
                    pending.push(Pending::Expression(*left));
                }
                Expression::Assignment { target, value, .. } => {
                    let target_completes = expression_completes(*target)?
                        && match ast.expressions().get(*target)?.payload() {
                            // Container writes check the receiver and index without a read.
                            Expression::Index { receiver, index } => {
                                expression_completes(*receiver)? && expression_completes(*index)?
                            }
                            _ => true,
                        };
                    if target_completes {
                        if expression_completes(*value)? {
                            // A jump during operand evaluation precedes the store.
                            pending.push(Pending::Write(*target));
                        }
                        pending.push(Pending::Expression(*value));
                    }
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
                    if expression_completes(*receiver)? {
                        pending.push(Pending::Expression(*index));
                    }
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
                    if !track_loop_writes || outcomes.possible_results(*condition).0 {
                        pending.push(Pending::EnterLoop(*body));
                    }
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

/// Exclude writes in short-circuit operands that cannot execute on this result edge.
fn writes_on_boolean_edge(
    outcomes: &SyntacticBooleanOutcomes<'_>,
    operand: ExpressionId,
    matched: bool,
    writes: &[ExpressionId],
) -> Result<Vec<ExpressionId>, AstError> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }
    let mut excluded = Vec::new();
    let mut pending = vec![(operand, matched, !matched)];
    while let Some((operand, want_true, want_false)) = pending.pop() {
        let node = outcomes.ast.expressions().get(operand)?;
        if !want_true && !want_false {
            excluded.push(node.span());
            continue;
        }
        match node.payload() {
            Expression::Group { expression } => pending.push((*expression, want_true, want_false)),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                pending.push((*operand, want_false, want_true));
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalAnd,
                right,
                ..
            } => {
                let (left_true, _) = outcomes.possible_results(*left);
                let (right_true, right_false) = outcomes.possible_results(*right);
                pending.push((
                    *right,
                    left_true && want_true && right_true,
                    left_true && want_false && right_false,
                ));
                pending.push((
                    *left,
                    (want_true && right_true) || (want_false && right_false),
                    want_false,
                ));
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalOr,
                right,
                ..
            } => {
                let (_, left_false) = outcomes.possible_results(*left);
                let (right_true, right_false) = outcomes.possible_results(*right);
                pending.push((
                    *right,
                    left_false && want_true && right_true,
                    left_false && want_false && right_false,
                ));
                pending.push((
                    *left,
                    want_true,
                    (want_true && right_true) || (want_false && right_false),
                ));
            }
            _ => {}
        }
    }
    excluded.sort_unstable_by_key(|span| span.start());
    let mut retained = Vec::new();
    for &write in writes {
        let span = outcomes.ast.expressions().get(write)?.span();
        let index = excluded.partition_point(|part| part.start() <= span.start());
        if index == 0 || span.end() > excluded[index - 1].end() {
            retained.push(write);
        }
    }
    Ok(retained)
}

fn when_operand(condition: &WhenCondition) -> Option<ExpressionId> {
    match condition {
        WhenCondition::Expression(expression) | WhenCondition::Contains { expression, .. } => {
            Some(*expression)
        }
        WhenCondition::TypeTest { .. } => None,
    }
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
