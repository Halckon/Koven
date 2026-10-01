//! Callable 级 owner 可用性与 may-origin 数据流；有限域只含 symbol/lambda。
use std::collections::{BTreeMap, BTreeSet};

use super::super::{Checker, ExpressionUse, OwnershipCheckingError};
use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, IterationClosureBinding,
        IterationClosureFlow,
    },
    parser::{
        AssignmentOperator, BinaryOperator, Expression, FunctionBody, FunctionForm, Item,
        Statement, StringPart, WhenCondition,
    },
    type_checking::{CallReceiverOrigin, ParameterMode},
};

type Origins = BTreeMap<usize, ExpressionId>;

#[derive(Default)]
struct CaptureSources {
    known: Origins,
    may_be_opaque: bool,
}

pub(super) struct CapturedOrigin {
    pub(super) known: Vec<ExpressionId>,
    pub(super) may_be_opaque: bool,
}

pub(super) type CapturedOrigins = BTreeMap<(usize, ClosureCaptureSource), CapturedOrigin>;

#[derive(Clone, Default, PartialEq, Eq)]
struct State {
    available: BTreeSet<SymbolId>,
    bindings: BTreeMap<SymbolId, Origins>,
    opaque: BTreeSet<SymbolId>,
    result: Origins,
    result_opaque: bool,
}

impl State {
    fn join(&mut self, other: Self) {
        self.available.extend(other.available);
        self.opaque.extend(other.opaque);
        for (symbol, origins) in other.bindings {
            self.bindings.entry(symbol).or_default().extend(origins);
        }
        self.result.extend(other.result);
        self.result_opaque |= other.result_opaque;
    }

    fn clear_result(&mut self) {
        self.result.clear();
        self.result_opaque = false;
    }

    fn bind(&mut self, symbol: SymbolId) {
        self.available.insert(symbol);
        let origins = std::mem::take(&mut self.result);
        if std::mem::take(&mut self.result_opaque) || origins.is_empty() {
            self.opaque.insert(symbol);
        } else {
            self.opaque.remove(&symbol);
        }
        if origins.is_empty() {
            self.bindings.remove(&symbol);
        } else {
            self.bindings.insert(symbol, origins);
        }
    }

    fn rows(&self) -> Vec<IterationClosureBinding> {
        self.available
            .iter()
            .map(|&symbol| IterationClosureBinding {
                symbol,
                origins: self
                    .bindings
                    .get(&symbol)
                    .map(|origins| origins.values().copied().collect())
                    .unwrap_or_default(),
            })
            .collect()
    }
}

#[derive(Default)]
struct Flow {
    next: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
}

fn join(into: &mut Option<State>, other: Option<State>) {
    if let Some(other) = other {
        if let Some(into) = into {
            into.join(other);
        } else {
            *into = Some(other);
        }
    }
}

impl Flow {
    fn next(state: State) -> Self {
        Self {
            next: Some(state),
            ..Self::default()
        }
    }

    fn join(&mut self, other: Self) {
        join(&mut self.next, other.next);
        join(&mut self.breaks, other.breaks);
        join(&mut self.continues, other.continues);
    }
}

pub(super) fn analyze(
    checker: &Checker<'_>,
) -> Result<(BTreeMap<usize, IterationClosureFlow>, CapturedOrigins), OwnershipCheckingError> {
    let mut analysis = Analysis {
        checker,
        loops: BTreeMap::new(),
        captures: BTreeMap::new(),
        recording: true,
        #[cfg(test)]
        passes: 0,
    };
    for &item in checker.parsed.roots() {
        analysis.item(item)?;
    }
    Ok((
        analysis.loops,
        analysis
            .captures
            .into_iter()
            .map(|(key, sources)| {
                (
                    key,
                    CapturedOrigin {
                        known: sources.known.into_values().collect(),
                        may_be_opaque: sources.may_be_opaque,
                    },
                )
            })
            .collect(),
    ))
}

struct Analysis<'a, 'checker> {
    checker: &'a Checker<'checker>,
    loops: BTreeMap<usize, IterationClosureFlow>,
    captures: BTreeMap<(usize, ClosureCaptureSource), CaptureSources>,
    recording: bool,
    #[cfg(test)]
    passes: usize,
}

impl Analysis<'_, '_> {
    fn item(&mut self, item: ItemId) -> Result<(), OwnershipCheckingError> {
        match self
            .checker
            .parsed
            .ast()
            .items()
            .get(item)?
            .payload()
            .clone()
        {
            Item::Modified { declaration, .. } => self.item(declaration)?,
            Item::Function {
                parameters, form, ..
            } => {
                let mut entry = State::default();
                for parameter in parameters {
                    if let Some(symbol) = self.checker.marker_symbol(parameter.name)
                        && self.checker.is_move_only_variable(symbol)
                    {
                        entry.available.insert(symbol);
                        entry.opaque.insert(symbol);
                    }
                }
                match form {
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => {
                        self.statement(body, entry)?;
                    }
                    FunctionForm::Explicit {
                        body: FunctionBody::Expression { expression, .. },
                        ..
                    } => {
                        self.expression(expression, ExpressionUse::Consume, entry)?;
                    }
                    _ => {}
                }
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.item(member)?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.item(member)?;
                }
            }
            Item::Deinit { body, .. } => {
                self.statement(body, State::default())?;
            }
            Item::Error | Item::Variable { .. } | Item::Constant { .. } => {}
        }
        Ok(())
    }

    fn statement(&mut self, id: StatementId, state: State) -> Result<Flow, OwnershipCheckingError> {
        self.body(id, ExpressionUse::Read, state)
    }

    fn body(
        &mut self,
        id: StatementId,
        usage: ExpressionUse,
        state: State,
    ) -> Result<Flow, OwnershipCheckingError> {
        match self
            .checker
            .parsed
            .ast()
            .statements()
            .get(id)?
            .payload()
            .clone()
        {
            Statement::Error => Ok(Flow::next(state)),
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => {
                let mut flow = Flow::next(state);
                let mut locals = Vec::new();
                for (index, &element) in elements.iter().enumerate() {
                    let Some(state) = flow.next.take() else {
                        break;
                    };
                    match self
                        .checker
                        .parsed
                        .ast()
                        .statements()
                        .get(element)?
                        .payload()
                    {
                        Statement::LocalVariable { declaration } => {
                            if let Item::Variable { name, .. } = self
                                .checker
                                .parsed
                                .ast()
                                .items()
                                .get(*declaration)?
                                .payload()
                                && let Some(symbol) = self.checker.marker_symbol(*name)
                            {
                                locals.push(symbol);
                            }
                        }
                        Statement::LocalDestructuring { .. } => {
                            if let Some(descriptor) = self.checker.typed.destructuring(element) {
                                locals.extend(
                                    descriptor
                                        .components()
                                        .iter()
                                        .map(|component| component.symbol()),
                                );
                            }
                        }
                        _ => {}
                    }
                    flow.join(self.body(
                        element,
                        if index + 1 == elements.len() {
                            usage
                        } else {
                            ExpressionUse::Read
                        },
                        state,
                    )?);
                }
                // 词法 alias 不进入外层 header；尾值已独立保存在 result 中。
                for state in [&mut flow.next, &mut flow.breaks, &mut flow.continues]
                    .into_iter()
                    .flatten()
                {
                    for symbol in &locals {
                        state.available.remove(symbol);
                        state.bindings.remove(symbol);
                        state.opaque.remove(symbol);
                    }
                }
                Ok(flow)
            }
            Statement::LocalVariable { declaration } => {
                let Item::Variable {
                    name, initializer, ..
                } = self
                    .checker
                    .parsed
                    .ast()
                    .items()
                    .get(declaration)?
                    .payload()
                    .clone()
                else {
                    return Ok(Flow::next(state));
                };
                let mut flow = self.expression(initializer, ExpressionUse::Consume, state)?;
                if let Some(state) = &mut flow.next {
                    if let Some(symbol) = self.checker.marker_symbol(name)
                        && self.checker.is_move_only_variable(symbol)
                    {
                        state.bind(symbol);
                    }
                    state.clear_result();
                }
                Ok(flow)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                let mut flow = self.expression(initializer, ExpressionUse::Consume, state)?;
                if let Some(state) = &mut flow.next {
                    if let Some(descriptor) = self.checker.typed.destructuring(id) {
                        for component in descriptor.components() {
                            let symbol = component.symbol();
                            if self.checker.is_move_only_variable(symbol) {
                                state.available.insert(symbol);
                            }
                        }
                    }
                    state.clear_result();
                }
                Ok(flow)
            }
            Statement::Expression { expression } => self.expression(expression, usage, state),
            Statement::For { source, body, .. } => {
                let mut flow = self.expression(source, ExpressionUse::Place, state)?;
                if let Some(entry) = flow.next.take() {
                    flow.join(self.loop_body(id, None, body, entry, true)?);
                }
                Ok(flow)
            }
            Statement::While {
                condition, body, ..
            } => self.loop_body(id, Some(condition), body, state, true),
            Statement::Loop { body, .. } => self.loop_body(id, None, body, state, false),
        }
    }

    /// Header 只增长有限 symbol×lambda 集；探测遍历不发布内层摘要。
    fn loop_body(
        &mut self,
        id: StatementId,
        condition: Option<ExpressionId>,
        body: StatementId,
        mut entry: State,
        exhausts: bool,
    ) -> Result<Flow, OwnershipCheckingError> {
        entry.clear_result();
        let mut header = entry;
        let recording = self.recording;
        self.recording = false;
        let (mut flow, mut exit) = loop {
            let (flow, exhaustion) = self.loop_pass(condition, body, &header, exhausts)?;
            let mut next = header.clone();
            for state in [&flow.next, &flow.continues].into_iter().flatten() {
                let mut state = state.clone();
                state.clear_result();
                next.join(state);
            }
            if next == header {
                break (flow, exhaustion);
            }
            header = next;
        };
        self.recording = recording;
        if recording {
            // 仅最终发布遍历需要重走稳定 header；探测复用收敛轮，避免嵌套指数重复。
            (flow, exit) = self.loop_pass(condition, body, &header, exhausts)?;
        }
        join(&mut exit, flow.breaks);
        if let Some(state) = &mut exit {
            state.clear_result();
        }
        if recording {
            let flow = IterationClosureFlow {
                header: header.rows(),
                exit: exit.as_ref().map(State::rows).unwrap_or_default(),
            };
            self.loops.insert(id.index(), flow);
        }
        Ok(Flow {
            next: exit,
            ..Flow::default()
        })
    }

    fn loop_pass(
        &mut self,
        condition: Option<ExpressionId>,
        body: StatementId,
        header: &State,
        exhausts: bool,
    ) -> Result<(Flow, Option<State>), OwnershipCheckingError> {
        #[cfg(test)]
        {
            self.passes += 1;
        }
        let mut flow = if let Some(condition) = condition {
            self.expression(condition, ExpressionUse::Read, header.clone())?
        } else {
            Flow::next(header.clone())
        };
        // while 耗尽保留 condition 的副作用；不能读取原 header 或 body 完成状态。
        let exhaustion = if exhausts { flow.next.clone() } else { None };
        if let Some(state) = flow.next.take() {
            flow.join(self.statement(body, state)?);
        }
        Ok((flow, exhaustion))
    }

    fn chain(
        &mut self,
        mut flow: Flow,
        id: ExpressionId,
        usage: ExpressionUse,
    ) -> Result<Flow, OwnershipCheckingError> {
        if let Some(state) = flow.next.take() {
            flow.join(self.expression(id, usage, state)?);
        }
        Ok(flow)
    }

    fn expression(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        mut state: State,
    ) -> Result<Flow, OwnershipCheckingError> {
        state.clear_result();
        let mut flow = self.expression_inner(id, usage, state)?;
        if let Some(state) = &mut flow.next {
            if usage == ExpressionUse::Read {
                state.clear_result();
            } else if state.result.is_empty() && self.checker.is_move_only_expression(id) {
                // 未知 MoveOnly 值可能是闭包；与已知 lambda 合流时不能丢失此来源。
                state.result_opaque = true;
            }
            if self.checker.is_nothing_expression(id) {
                flow.next = None;
            }
        }
        Ok(flow)
    }

    fn expression_inner(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        mut state: State,
    ) -> Result<Flow, OwnershipCheckingError> {
        if self.checker.is_constant_use(id) {
            return Ok(Flow::next(state));
        }
        if let Some(descriptor) = self.checker.construction.descriptor(id) {
            let mut arguments = descriptor.arguments().to_vec();
            arguments.sort_by_key(|argument| argument.evaluation_index());
            let mut flow = Flow::next(state);
            for argument in arguments {
                flow = self.chain(flow, argument.argument(), ExpressionUse::Consume)?;
            }
            if let Some(state) = &mut flow.next {
                state.clear_result();
            }
            return Ok(flow);
        }
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    state.result_opaque = state.opaque.contains(&symbol)
                        || (state.available.contains(&symbol)
                            && !state.bindings.contains_key(&symbol));
                    state.result = if usage == ExpressionUse::Consume {
                        state.available.remove(&symbol);
                        state.opaque.remove(&symbol);
                        state.bindings.remove(&symbol).unwrap_or_default()
                    } else {
                        state.bindings.get(&symbol).cloned().unwrap_or_default()
                    };
                }
                Ok(Flow::next(state))
            }
            Expression::Lambda {
                opener_span,
                mut parameters,
                arrow_span,
                body,
                ..
            } => {
                let mut captured = State::default();
                if arrow_span.is_none()
                    && let Some(symbol) = self
                        .checker
                        .symbols_by_span
                        .get(&super::super::span_key(opener_span))
                    && self.checker.typed.parameter_mode(*symbol).is_some()
                {
                    parameters.push(opener_span);
                }
                for parameter in parameters {
                    if let Some(&symbol) = self
                        .checker
                        .symbols_by_span
                        .get(&super::super::span_key(parameter))
                        && self.checker.is_move_only_variable(symbol)
                    {
                        captured.available.insert(symbol);
                        captured.opaque.insert(symbol);
                    }
                }
                for capture in self.checker.captures_of(id) {
                    if let ClosureCaptureSource::Symbol(symbol) = capture.source() {
                        if capture.mode() == ClosureCaptureMode::Owned
                            && capture.effect() == ClosureCaptureEffect::Move
                        {
                            let sources = self
                                .captures
                                .entry((id.index(), capture.source()))
                                .or_default();
                            if let Some(origins) = state.bindings.get(&symbol) {
                                sources.known.extend(origins);
                            }
                            sources.may_be_opaque |= state.opaque.contains(&symbol)
                                || (state.available.contains(&symbol)
                                    && !state.bindings.contains_key(&symbol));
                        }
                        if let Some(origins) = state.bindings.get(&symbol) {
                            captured.bindings.insert(symbol, origins.clone());
                        }
                        if state.opaque.contains(&symbol) {
                            captured.opaque.insert(symbol);
                        }
                        if capture.mode() == ClosureCaptureMode::Owned
                            && self.checker.is_move_only_variable(symbol)
                        {
                            captured.available.insert(symbol);
                        }
                        if capture.mode() == ClosureCaptureMode::Owned
                            && capture.effect() == ClosureCaptureEffect::Move
                        {
                            state.available.remove(&symbol);
                            state.bindings.remove(&symbol);
                            state.opaque.remove(&symbol);
                        }
                    }
                }
                // 跳转在 callable 内消费，不能成为创建点所在 loop 的边。
                self.body(body, ExpressionUse::Consume, captured)?;
                state.result.insert(id.index(), id);
                Ok(Flow::next(state))
            }
            Expression::Group { expression } => self.expression(expression, usage, state),
            Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, ExpressionUse::Consume, state)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let mut flow = self.expression(condition, ExpressionUse::Read, state)?;
                if let Some(base) = flow.next.take() {
                    let usage = self.checker.control_result_usage(id);
                    flow.join(self.body(then_branch, usage, base.clone())?);
                    flow.join(if let Some(body) = else_branch {
                        self.body(body, usage, base)?
                    } else {
                        Flow::next(base)
                    });
                }
                Ok(flow)
            }
            Expression::When {
                subject, entries, ..
            } => {
                let mut flow = if let Some(subject) = subject {
                    self.expression(subject, ExpressionUse::Place, state)?
                } else {
                    Flow::next(state)
                };
                let mut remaining = flow.next.take();
                let plan = self.checker.typed.nullable_when(id).cloned();
                for (entry_index, entry) in entries.iter().enumerate() {
                    let mut matched = None;
                    if entry.else_span.is_some() {
                        matched = remaining.take();
                    }
                    for (alternative_index, condition) in entry.conditions.iter().enumerate() {
                        let Some(input) = remaining.take() else {
                            break;
                        };
                        let mut alternative = if let WhenCondition::Expression(value)
                        | WhenCondition::Contains {
                            expression: value,
                            ..
                        } = condition
                        {
                            self.expression(*value, ExpressionUse::Read, input)?
                        } else {
                            Flow::next(input)
                        };
                        let input = alternative.next.take();
                        flow.join(alternative);
                        let descriptor = plan
                            .as_ref()
                            .and_then(|plan| plan.entries().get(entry_index))
                            .and_then(|entry| entry.alternatives().get(alternative_index));
                        if descriptor.is_none_or(|descriptor| !descriptor.match_domain().is_empty())
                        {
                            join(&mut matched, input.clone());
                        }
                        if descriptor
                            .is_none_or(|descriptor| !descriptor.fallthrough_domain().is_empty())
                        {
                            remaining = input;
                        }
                    }
                    if let Some(state) = matched {
                        flow.join(self.body(
                            entry.body,
                            self.checker.control_result_usage(id),
                            state,
                        )?);
                    }
                }
                join(&mut flow.next, remaining);
                Ok(flow)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                let mut flow = Flow::next(state);
                if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
                    flow = self.chain(flow, descriptor.receiver(), ExpressionUse::Place)?;
                    flow = self.chain(flow, descriptor.index(), ExpressionUse::Read)?;
                } else if operator != AssignmentOperator::Assign {
                    flow = self.chain(flow, target, ExpressionUse::Read)?;
                }
                flow = self.chain(flow, value, ExpressionUse::Consume)?;
                if let Some(state) = &mut flow.next {
                    if operator == AssignmentOperator::Assign
                        && let Some(place) = self.checker.place(target)?
                        && place.is_root()
                        && self.checker.is_move_only_variable(place.root())
                    {
                        state.bind(place.root());
                    }
                    state.clear_result();
                }
                Ok(flow)
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let receiver = self
                    .checker
                    .receivers_by_expression
                    .get(&id.index())
                    .copied();
                let mut flow = match receiver {
                    Some(receiver) => match receiver.origin() {
                        CallReceiverOrigin::Expression(expression) => {
                            self.expression(expression, mode_usage(receiver.mode()), state)?
                        }
                        CallReceiverOrigin::ImplicitThis(_) => Flow::next(state),
                    },
                    None => self.expression(callee, ExpressionUse::Place, state)?,
                };
                let modes = self.checker.calls_by_expression.get(&id.index());
                for (index, argument) in arguments.iter().enumerate() {
                    let mode = modes
                        .and_then(|modes| modes.get(index))
                        .copied()
                        .unwrap_or(ParameterMode::Borrow);
                    flow = self.chain(flow, argument.value, mode_usage(mode))?;
                }
                if let Some(state) = &mut flow.next {
                    state.clear_result();
                }
                Ok(flow)
            }
            Expression::Return { value, .. } => {
                let mut flow = if let Some(value) = value {
                    self.expression(value, ExpressionUse::Consume, state)?
                } else {
                    Flow::next(state)
                };
                flow.next = None;
                Ok(flow)
            }
            Expression::Break { .. } => Ok(Flow {
                breaks: Some(state),
                ..Flow::default()
            }),
            Expression::Continue { .. } => Ok(Flow {
                continues: Some(state),
                ..Flow::default()
            }),
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => {
                let usage = self.checker.control_result_usage(id);
                let mut flow = self.expression(left, ExpressionUse::Place, state)?;
                if let Some(input) = flow.next.take() {
                    let only_null = self.checker.is_only_null(left);
                    // Nothing? 必走 RHS；其余 nullable 保留非空值及绕过 RHS 的正常路径。
                    if !only_null {
                        let mut selected = input.clone();
                        if usage == ExpressionUse::Consume
                            && let Some(symbol) = self.checker.expression_root_symbol(left)?
                        {
                            selected.available.remove(&symbol);
                            selected.bindings.remove(&symbol);
                            selected.opaque.remove(&symbol);
                        }
                        flow.next = Some(selected);
                    }
                    flow.join(self.expression(right, usage, input)?);
                }
                Ok(flow)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let mut left = self.expression(left, ExpressionUse::Read, state)?;
                if matches!(
                    operator,
                    BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
                ) {
                    if let Some(state) = &left.next {
                        left.join(self.expression(right, ExpressionUse::Read, state.clone())?);
                    }
                    Ok(left)
                } else {
                    self.chain(left, right, ExpressionUse::Read)
                }
            }
            Expression::Index { receiver, index } => {
                let flow = self.expression(receiver, ExpressionUse::Place, state)?;
                self.chain(flow, index, ExpressionUse::Read)
            }
            Expression::Member { receiver, .. } => {
                self.expression(receiver, ExpressionUse::Read, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::Propagate { value: operand, .. } => {
                self.expression(operand, ExpressionUse::Read, state)
            }
            Expression::String { parts } => {
                let mut flow = Flow::next(state);
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        flow = self.chain(flow, expression, ExpressionUse::Read)?;
                    }
                }
                Ok(flow)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.expression(receiver, ExpressionUse::Read, state)
                } else {
                    Ok(Flow::next(state))
                }
            }
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(Flow::next(state)),
        }
    }
}

fn mode_usage(mode: ParameterMode) -> ExpressionUse {
    if mode == ParameterMode::Value {
        ExpressionUse::Consume
    } else {
        ExpressionUse::Place
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_origin_nested_fixed_point_does_not_repeat_converged_probes() {
        let depth = 12;
        let text = format!(
            "fun run(flags: List<Boolean>) {{ {} {} }}",
            "for (_ in flags) { ".repeat(depth),
            "} ".repeat(depth)
        );
        let mut sources = crate::source::SourceMap::new();
        let source = sources.add_source("nested-origin.ko", &text).unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (names, types) = crate::type_checking::standard_environments();
        let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
        let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        let checker = Checker::new(&sources, &parsed, &names, &typed).unwrap();
        let mut analysis = Analysis {
            checker: &checker,
            loops: BTreeMap::new(),
            captures: BTreeMap::new(),
            recording: true,
            passes: 0,
        };
        for &root in parsed.roots() {
            analysis.item(root).unwrap();
        }
        assert_eq!(analysis.loops.len(), depth);
        assert!(
            analysis.passes <= 2 * depth * depth,
            "unchanged nested headers must not double the traversal at every level: {} passes",
            analysis.passes
        );
    }
}
