//! File-local adapters consume existing typed/name/capture facts; no second language analysis.

use super::{Checker, ExpressionUse, Flows, State};
use crate::ownership_checking::{
    CallableOrigin, CallableOriginFact, OwnershipCheckingError, PointerCallableReturnOrigin,
    PointerCallableReturnSummary,
    callable_provenance::{
        FileCallableFacts,
        graph::{Collector, ReturnFrame},
    },
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::{ScopeKind, SymbolId, SymbolKind},
    parser::Expression,
    type_checking::{CallableTarget, TypeId, TypeKind},
};

pub(super) type Collection = Collector<usize, ExpressionId, CallableOrigin, SymbolId, TypeId>;

impl Checker<'_> {
    pub(super) fn check_maybe_loop(
        &mut self,
        mut prefix: Flows,
        body: StatementId,
        errors: usize,
    ) -> Result<Flows, OwnershipCheckingError> {
        let headers = if let Some(state) = &mut prefix.next {
            state.origins.attach(&self.callable_sources.arena);
            state.origins.begin_loop()
        } else {
            Vec::new()
        };
        self.check_maybe_loop_with_headers(prefix, body, errors, &headers)
    }
    fn callable_type(&self, expression: ExpressionId) -> Option<TypeId> {
        self.typed
            .expression_type(expression)
            .filter(|ty| matches!(self.typed.types().get(*ty), Some(TypeKind::Function { .. })))
    }

    /// A signature never selects an overload, external target or function-value environment.
    fn known_callable(&self, symbol: SymbolId) -> bool {
        self.names
            .symbols()
            .get(symbol.index())
            .is_some_and(|source| {
                source.kind() == SymbolKind::Function
                    && self
                        .names
                        .scopes()
                        .get(source.scope().index())
                        .is_some_and(|scope| scope.kind() == ScopeKind::File)
            })
            && self.typed.callables().iter().any(|callable| {
                callable.symbol() == symbol
                    && callable.owner().is_none()
                    && callable.receiver().is_none()
                    && callable.type_parameters().is_empty()
            })
    }

    pub(super) fn seed_callable_parameter(&self, symbol: SymbolId, state: &mut State) {
        state.origins.attach(&self.callable_sources.arena);
        if self.typed.parameter_mode(symbol).is_some()
            && self.typed.symbol_type(symbol).is_some_and(|ty| {
                matches!(self.typed.types().get(ty), Some(TypeKind::Function { .. }))
            })
        {
            state.origins.bind(
                symbol,
                self.callable_sources
                    .leaf(CallableOrigin::Parameter(symbol)),
            );
        }
    }

    pub(super) fn bind_callable_symbol(&self, symbol: SymbolId, state: &mut State) {
        if self
            .typed
            .symbol_type(symbol)
            .is_some_and(|ty| matches!(self.typed.types().get(ty), Some(TypeKind::Function { .. })))
        {
            state.origins.bind(symbol, state.origins.result);
        }
    }

    pub(super) fn enter_callable(
        &mut self,
        symbol: Option<SymbolId>,
    ) -> Option<ReturnFrame<SymbolId, TypeId, ExpressionId>> {
        let previous = self.callable_sources.active_return.take();
        if let Some(symbol) = symbol
            && let Some(callable) = self.typed.callables().iter().find(|callable| {
                callable.symbol() == symbol
                    && callable.owner().is_none()
                    && callable.receiver().is_none()
            })
            && matches!(
                self.typed.types().get(callable.return_type()),
                Some(TypeKind::Function { .. })
            )
        {
            self.callable_sources.active_return = Some(ReturnFrame {
                target: symbol,
                function_type: callable.return_type(),
                deliveries: Vec::new(),
            });
        }
        previous
    }

    pub(super) fn leave_callable(
        &mut self,
        previous: Option<ReturnFrame<SymbolId, TypeId, ExpressionId>>,
    ) {
        if let Some(frame) = self.callable_sources.active_return.take() {
            self.callable_sources.returns.push(frame);
        }
        self.callable_sources.active_return = previous;
    }

    fn file_factory_target(&self, expression: ExpressionId) -> Option<SymbolId> {
        let call = self
            .typed
            .calls()
            .iter()
            .find(|call| call.expression() == expression)?;
        let CallableTarget::Source(symbol) = call.target() else {
            return None;
        };
        let source = self.names.symbols().get(symbol.index())?;
        let ordinary = source.kind() == SymbolKind::Function
            && self.names.scopes().get(source.scope().index())?.kind() == ScopeKind::File;
        (ordinary
            && call.receiver().is_none()
            && matches!(
                self.typed.types().get(call.return_type()),
                Some(TypeKind::Function { .. })
            ))
        .then_some(symbol)
    }

    pub(super) fn check_expression(
        &mut self,
        id: ExpressionId,
        mut state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        state.origins.attach(&self.callable_sources.arena);
        let ast = self.parsed.ast().expressions().get(id)?;
        let span = ast.span();
        let expression = ast.payload().clone();
        let name = if matches!(expression, Expression::Name) {
            self.reference_symbol(span).map(|symbol| {
                if self.known_callable(symbol) {
                    self.callable_sources
                        .leaf(CallableOrigin::KnownFunction(symbol))
                } else {
                    state.origins.binding(symbol)
                }
            })
        } else {
            None
        };
        state.origins.result = Default::default();
        // Lambda returns belong to its own callable, never to the enclosing factory.
        let is_lambda = matches!(expression, Expression::Lambda { .. });
        let previous = if is_lambda {
            self.callable_sources.active_return.take()
        } else {
            None
        };
        let checked = self.check_expression_inner(id, state, usage);
        if is_lambda {
            self.callable_sources.active_return = previous;
        }
        let mut flows = checked?;
        if let Some(next) = &mut flows.next {
            let value = if self.callable_type(id).is_some() {
                match expression {
                    Expression::Name => name.unwrap_or_default(),
                    Expression::Lambda { .. } => {
                        self.callable_sources.leaf(CallableOrigin::Lambda(id))
                    }
                    Expression::Group { .. } | Expression::If { .. } | Expression::When { .. } => {
                        next.origins.result
                    }
                    Expression::Call { .. } if self.file_factory_target(id).is_some() => self
                        .callable_sources
                        .leaf(CallableOrigin::FactoryResult(id)),
                    _ => Default::default(),
                }
            } else {
                Default::default()
            };
            next.origins.result = value;
            if self.callable_type(id).is_some() {
                self.callable_sources.record(id.index(), id, value, span);
            }
        }
        Ok(flows)
    }

    /// Only a function expression body or an explicit AST Return.value is a return delivery.
    pub(super) fn check_return_expression(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let result = self.check_escaping_expression(id, state, usage);
        let flows = result?;
        if let Some(next) = &flows.next {
            let mut value = id;
            while let Expression::Group { expression } =
                self.parsed.ast().expressions().get(value)?.payload()
            {
                value = *expression;
            }
            let direct = matches!(
                self.parsed.ast().expressions().get(value)?.payload(),
                Expression::Name | Expression::Lambda { .. }
            );
            let ty = self.callable_type(id);
            let span = self.parsed.ast().expressions().get(id)?.span();
            if let Some(frame) = &mut self.callable_sources.active_return {
                frame.deliveries.push((
                    id,
                    next.origins.result,
                    span,
                    direct && ty == Some(frame.function_type),
                ));
            }
        }
        Ok(flows)
    }

    pub(super) fn finish_callable_sources(&self, successful: bool) -> FileCallableFacts {
        if !successful {
            return FileCallableFacts::default();
        }
        let graph = self.callable_sources.arena.borrow();
        let initial = graph.solve(|origin| !matches!(origin, CallableOrigin::FactoryResult(_)));
        let mut returns = Vec::new();
        for frame in &self.callable_sources.returns {
            let [(expression, node, span, true)] = frame.deliveries.as_slice() else {
                continue;
            };
            let origin = match initial.get(*node) {
                Some(CallableOrigin::Lambda(lambda))
                    if !self
                        .captures
                        .iter()
                        .any(|capture| capture.lambda() == lambda) =>
                {
                    PointerCallableReturnOrigin::Lambda(lambda)
                }
                Some(CallableOrigin::KnownFunction(symbol)) => {
                    PointerCallableReturnOrigin::KnownFunction(symbol)
                }
                _ => continue,
            };
            returns.push(PointerCallableReturnSummary::new(
                frame.target,
                *expression,
                origin,
                frame.function_type,
                *span,
            ));
        }
        returns.sort_by_key(|summary| summary.target().index());
        let solved = graph.solve(|origin| match origin {
            CallableOrigin::FactoryResult(call) => self
                .file_factory_target(call)
                .is_some_and(|target| returns.iter().any(|summary| summary.target() == target)),
            _ => true,
        });
        let origins = self
            .callable_sources
            .uses
            .values()
            .filter_map(|(expression, node, span)| {
                solved
                    .get(*node)
                    .map(|origin| CallableOriginFact::new(*expression, origin, *span))
            })
            .collect();
        FileCallableFacts { origins, returns }
    }
}
