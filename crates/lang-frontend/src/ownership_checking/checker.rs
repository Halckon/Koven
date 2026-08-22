use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes, ordered_diagnostics},
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind},
    parser::{
        AssignmentOperator, Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, StringPart, WhenCondition,
    },
    source::{SourceMap, Span},
    type_checking::{Copyability, ParameterMode, TypedFile},
};

use super::{OwnershipCheckedFile, OwnershipCheckingError};

type State = BTreeMap<SymbolId, Span>;

#[derive(Clone, Copy)]
enum ExpressionUse {
    Read,
    Consume,
}

#[derive(Default)]
struct Flows {
    next: Option<State>,
    breaks: Option<State>,
    continues: Option<State>,
}

impl Flows {
    fn next(state: State) -> Self {
        Self {
            next: Some(state),
            ..Self::default()
        }
    }

    fn merge(&mut self, other: Self) {
        merge_optional_state(&mut self.next, other.next);
        merge_optional_state(&mut self.breaks, other.breaks);
        merge_optional_state(&mut self.continues, other.continues);
    }
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
    sources.source_text(parsed.source_id())?;
    Checker::new(sources, parsed, names, typed)?.run()
}

struct Checker<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    names: &'a NameResolution,
    typed: &'a TypedFile,
    symbols_by_span: BTreeMap<(usize, usize), SymbolId>,
    references_by_span: BTreeMap<(usize, usize), SymbolId>,
    calls_by_expression: BTreeMap<usize, Vec<ParameterMode>>,
    diagnostics: Vec<Diagnostic>,
    use_after_move_code: DiagnosticCode,
}

impl<'a> Checker<'a> {
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        names: &'a NameResolution,
        typed: &'a TypedFile,
    ) -> Result<Self, OwnershipCheckingError> {
        let symbols_by_span = names
            .symbols()
            .iter()
            .map(|symbol| (span_key(symbol.span()), symbol.id()))
            .collect();
        let references_by_span = names
            .references()
            .iter()
            .filter_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                _ => None,
            })
            .collect();
        let calls_by_expression = typed
            .calls()
            .iter()
            .map(|call| {
                let mut modes = vec![ParameterMode::Borrow; call.arguments().len()];
                for argument in call.arguments() {
                    modes[argument.argument_index()] = argument.mode();
                }
                (call.expression().index(), modes)
            })
            .collect();
        Ok(Self {
            sources,
            parsed,
            names,
            typed,
            symbols_by_span,
            references_by_span,
            calls_by_expression,
            diagnostics: Vec::new(),
            use_after_move_code: codes::catalog()?.resolve(codes::USE_AFTER_MOVE)?,
        })
    }

    fn run(mut self) -> Result<OwnershipCheckedFile, OwnershipCheckingError> {
        let mut state = State::new();
        for &root in self.parsed.roots() {
            self.check_item(root, &mut state)?;
        }
        let diagnostics = ordered_diagnostics(self.sources, &self.diagnostics)?
            .into_iter()
            .cloned()
            .collect();
        Ok(OwnershipCheckedFile::new(
            self.parsed.source_id(),
            diagnostics,
        ))
    }

    fn check_item(&mut self, id: ItemId, state: &mut State) -> Result<(), OwnershipCheckingError> {
        match self.parsed.ast().items().get(id)?.payload().clone() {
            Item::Error => {}
            Item::Modified { declaration, .. } => self.check_item(declaration, state)?,
            Item::Variable {
                name, initializer, ..
            }
            | Item::Constant {
                name, initializer, ..
            } => {
                let flows =
                    self.check_expression(initializer, state.clone(), ExpressionUse::Consume)?;
                if let Some(mut next) = flows.next {
                    self.mark_available(name, &mut next);
                    *state = next;
                }
            }
            Item::Function {
                parameters, form, ..
            } => {
                let mut function_state = State::new();
                for parameter in parameters {
                    self.mark_available(parameter.name, &mut function_state);
                }
                self.check_function(form, function_state)?;
            }
            Item::Classifier(classifier) => {
                if let Some(body) = classifier.body {
                    for member in body.members {
                        self.check_item(member, &mut State::new())?;
                    }
                }
            }
            Item::Companion(companion) => {
                for member in companion.body.members {
                    self.check_item(member, &mut State::new())?;
                }
            }
        }
        Ok(())
    }

    fn check_function(
        &mut self,
        form: FunctionForm,
        state: State,
    ) -> Result<(), OwnershipCheckingError> {
        match form {
            FunctionForm::ImplicitUnitAbsent => {}
            FunctionForm::ImplicitUnitBlock(body) => {
                self.check_statement(body, state)?;
            }
            FunctionForm::Explicit { body, .. } => match body {
                FunctionBody::Absent => {}
                FunctionBody::Expression { expression, .. } => {
                    self.check_expression(expression, state, ExpressionUse::Consume)?;
                }
                FunctionBody::Block(body) => {
                    self.check_statement(body, state)?;
                }
            },
        }
        Ok(())
    }

    fn check_statement(
        &mut self,
        id: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().statements().get(id)?.payload().clone() {
            Statement::Error => Ok(Flows::next(state)),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => self.check_elements(&elements, state),
            Statement::LocalVariable { declaration } => {
                let mut state = state;
                self.check_item(declaration, &mut state)?;
                Ok(Flows::next(state))
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.check_expression(initializer, state, ExpressionUse::Read)
            }
            Statement::While {
                condition, body, ..
            } => {
                let condition = self.check_expression(condition, state, ExpressionUse::Read)?;
                self.check_maybe_loop(condition, body)
            }
            Statement::For { source, body, .. } => {
                let source = self.check_expression(source, state, ExpressionUse::Read)?;
                self.check_maybe_loop(source, body)
            }
            Statement::Loop { body, .. } => {
                let body = self.check_statement(body, state)?;
                Ok(Flows {
                    next: body.breaks,
                    breaks: None,
                    continues: None,
                })
            }
            Statement::Expression { expression } => {
                self.check_expression(expression, state, ExpressionUse::Read)
            }
        }
    }

    fn check_elements(
        &mut self,
        elements: &[StatementId],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut flows = Flows::next(state);
        for &element in elements {
            let Some(next) = flows.next.take() else {
                break;
            };
            flows.merge(self.check_statement(element, next)?);
        }
        Ok(flows)
    }

    fn check_maybe_loop(
        &mut self,
        mut prefix: Flows,
        body: StatementId,
    ) -> Result<Flows, OwnershipCheckingError> {
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let body = self.check_statement(body, base.clone())?;
        let mut next = base;
        for state in [body.next, body.breaks, body.continues]
            .into_iter()
            .flatten()
        {
            merge_state(&mut next, state);
        }
        prefix.next = Some(next);
        Ok(prefix)
    }

    fn check_expression(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(id)?;
        let span = node.span();
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(Flows::next(state)),
            Expression::Name => {
                let mut state = state;
                self.use_name(span, usage, &mut state)?;
                Ok(Flows::next(state))
            }
            Expression::Group { expression } => self.check_expression(expression, state, usage),
            Expression::String { parts } => {
                let mut flows = Flows::next(state);
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        flows = self.chain_expression(flows, expression, ExpressionUse::Read)?;
                    }
                }
                Ok(flows)
            }
            Expression::Lambda { .. } => Ok(Flows::next(state)),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.check_if(condition, then_branch, else_branch, state),
            Expression::When {
                subject, entries, ..
            } => self.check_when(subject, &entries, state),
            Expression::Return { value, .. } => {
                let mut flows = Flows::next(state);
                if let Some(value) = value {
                    flows = self.chain_expression(flows, value, ExpressionUse::Consume)?;
                }
                flows.next = None;
                Ok(flows)
            }
            Expression::Break { .. } => Ok(Flows {
                breaks: Some(state),
                ..Flows::default()
            }),
            Expression::Continue { .. } => Ok(Flows {
                continues: Some(state),
                ..Flows::default()
            }),
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            }
            | Expression::NonNullAssert { operand, .. }
            | Expression::Propagate { value: operand, .. } => {
                self.check_expression(operand, state, ExpressionUse::Read)
            }
            Expression::Binary { left, right, .. } => {
                let flows = self.check_expression(left, state, ExpressionUse::Read)?;
                self.chain_expression(flows, right, ExpressionUse::Read)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => {
                let diagnostic_count = self.diagnostics.len();
                let flows = self.check_expression(value, state, ExpressionUse::Consume)?;
                self.finish_assignment(
                    flows,
                    target,
                    operator,
                    self.diagnostics.len() == diagnostic_count,
                )
            }
            Expression::Member { receiver, .. } => {
                self.check_expression(receiver, state, ExpressionUse::Read)
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let mut flows = self.check_expression(callee, state, ExpressionUse::Read)?;
                let modes = self.calls_by_expression.get(&id.index()).cloned();
                for (index, argument) in arguments.into_iter().enumerate() {
                    let usage = if modes
                        .as_ref()
                        .and_then(|modes| modes.get(index))
                        .is_some_and(|mode| *mode == ParameterMode::Value)
                    {
                        ExpressionUse::Consume
                    } else {
                        ExpressionUse::Read
                    };
                    flows = self.chain_expression(flows, argument.value, usage)?;
                }
                Ok(flows)
            }
            Expression::Index { receiver, index } => {
                let flows = self.check_expression(receiver, state, ExpressionUse::Read)?;
                self.chain_expression(flows, index, ExpressionUse::Read)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.check_expression(receiver, state, ExpressionUse::Read)
                } else {
                    Ok(Flows::next(state))
                }
            }
        }
    }

    fn check_if(
        &mut self,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let mut branches = self.check_statement(then_branch, base.clone())?;
        branches.merge(if let Some(else_branch) = else_branch {
            self.check_statement(else_branch, base)?
        } else {
            Flows::next(base)
        });
        prefix.merge(branches);
        Ok(prefix)
    }

    fn check_when(
        &mut self,
        subject: Option<ExpressionId>,
        entries: &[crate::parser::WhenEntry],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = Flows::next(state);
        if let Some(subject) = subject {
            prefix = self.chain_expression(prefix, subject, ExpressionUse::Read)?;
        }
        let Some(mut base) = prefix.next.take() else {
            return Ok(prefix);
        };
        for entry in entries {
            for condition in &entry.conditions {
                let expression = match condition {
                    WhenCondition::Expression(expression)
                    | WhenCondition::Contains { expression, .. } => Some(*expression),
                    WhenCondition::TypeTest { .. } => None,
                };
                if let Some(expression) = expression {
                    let flows = self.check_expression(expression, base, ExpressionUse::Read)?;
                    base = flows.next.unwrap_or_default();
                }
            }
        }
        let mut branches = Flows::default();
        for entry in entries {
            branches.merge(self.check_statement(entry.body, base.clone())?);
        }
        if entries.iter().all(|entry| entry.else_span.is_none()) {
            branches.merge(Flows::next(base));
        }
        prefix.merge(branches);
        Ok(prefix)
    }

    fn chain_expression(
        &mut self,
        mut flows: Flows,
        id: ExpressionId,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(next) = flows.next.take() {
            flows.merge(self.check_expression(id, next, usage)?);
        }
        Ok(flows)
    }

    fn finish_assignment(
        &mut self,
        flows: Flows,
        target: ExpressionId,
        operator: AssignmentOperator,
        value_is_valid: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let target_node = self.parsed.ast().expressions().get(target)?;
        if matches!(target_node.payload(), Expression::Name) {
            let span = target_node.span();
            let mut flows = flows;
            if let Some(state) = flows.next.as_mut() {
                if operator == AssignmentOperator::Assign {
                    if value_is_valid && let Some(symbol) = self.reference_symbol(span) {
                        state.remove(&symbol);
                    }
                } else {
                    self.use_name(span, ExpressionUse::Read, state)?;
                }
            }
            Ok(flows)
        } else {
            self.chain_expression(flows, target, ExpressionUse::Read)
        }
    }

    fn use_name(
        &mut self,
        span: Span,
        usage: ExpressionUse,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(symbol) = self.reference_symbol(span) else {
            return Ok(());
        };
        if let Some(origin) = state.get(&symbol).copied() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.use_after_move_code,
                "use of moved value",
                span,
            )?;
            diagnostic.add_label(self.sources, origin, "value was moved here")?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        if matches!(usage, ExpressionUse::Consume) && self.is_move_only_variable(symbol) {
            state.insert(symbol, span);
        }
        Ok(())
    }

    fn is_move_only_variable(&self, symbol: SymbolId) -> bool {
        let Some(symbol_data) = self.names.symbols().get(symbol.index()) else {
            return false;
        };
        if !matches!(
            symbol_data.kind(),
            SymbolKind::Variable
                | SymbolKind::ValueParameter
                | SymbolKind::LambdaParameter
                | SymbolKind::ForBinding
                | SymbolKind::DestructuringBinding
        ) {
            return false;
        }
        self.typed
            .symbol_type(symbol)
            .and_then(|ty| self.typed.copyability(ty))
            == Some(Copyability::MoveOnly)
    }

    fn mark_available(&self, marker: NameMarker, state: &mut State) {
        if let NameMarker::Present(span) = marker
            && let Some(symbol) = self.symbols_by_span.get(&span_key(span))
        {
            state.remove(symbol);
        }
    }

    fn reference_symbol(&self, span: Span) -> Option<SymbolId> {
        self.references_by_span.get(&span_key(span)).copied()
    }
}

fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

fn merge_optional_state(target: &mut Option<State>, source: Option<State>) {
    let Some(source) = source else {
        return;
    };
    if let Some(target) = target {
        merge_state(target, source);
    } else {
        *target = Some(source);
    }
}

fn merge_state(target: &mut State, source: State) {
    for (symbol, origin) in source {
        target
            .entry(symbol)
            .and_modify(|current| {
                if origin.start() < current.start() {
                    *current = origin;
                }
            })
            .or_insert(origin);
    }
}
