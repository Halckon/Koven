//! Source-qualified adapters reuse validated declarations, body types and capture descriptors.

use super::{Checker, ExpressionUse, Flows, State};
use crate::{
    ast::ExpressionId,
    name_resolution::{UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{
        OwnershipCheckingError, UnitCallableOrigin, UnitCallableOriginFact,
        UnitPointerCallableReturnOrigin, UnitPointerCallableReturnSummary,
        callable_provenance::{
            UnitCallableFacts,
            graph::{Collector, ReturnFrame},
        },
    },
    parser::{Expression, NameMarker},
    type_checking::{
        UnitCallTarget, UnitCallableTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

pub(super) type Collection = Collector<
    UnitExpressionId,
    UnitExpressionId,
    UnitCallableOrigin,
    UnitCallableTarget,
    UnitTypeId,
>;

impl Checker<'_> {
    fn callable_type(&self, expression: ExpressionId) -> Option<UnitTypeId> {
        self.typed
            .expression_type(self.unit_expression(expression))
            .filter(|ty| {
                matches!(
                    self.typed.signatures().types().get(*ty),
                    Some(UnitTypeKind::Function { .. })
                )
            })
    }
    fn named_callable(&self, expression: ExpressionId) -> Option<UnitCallableTarget> {
        let span = self.parsed.ast().expressions().get(expression).ok()?.span();
        let reference = self.names.names().references().iter().find(|reference| {
            reference.source_unit() == self.source_unit && reference.span() == span
        })?;
        let UnitReferenceTarget::Declaration(declaration) = reference.target() else {
            return None;
        };
        let signature = self.typed.signatures().declaration(*declaration)?;
        let callable = signature.callable()?;
        (callable.has_body()
            && callable.receiver().is_none()
            && callable.type_parameters().is_empty()
            && matches!(
                self.typed.signatures().types().get(signature.ty()),
                Some(UnitTypeKind::Function { .. })
            ))
        .then_some(UnitCallableTarget::Declaration(*declaration))
    }
    pub(super) fn seed_callable_parameter(&self, symbol: UnitSymbolId, state: &mut State) {
        state.origins.attach(&self.callable_sources.arena);
        if self.bindings.contains_key(&symbol)
            && self.typed.symbol_type(symbol).is_some_and(|ty| {
                matches!(
                    self.typed.signatures().types().get(ty),
                    Some(UnitTypeKind::Function { .. })
                )
            })
        {
            state.origins.bind(
                symbol,
                self.callable_sources
                    .leaf(UnitCallableOrigin::Parameter(symbol)),
            );
        }
    }
    pub(super) fn bind_callable_symbol(&self, symbol: UnitSymbolId, state: &mut State) {
        if self.typed.symbol_type(symbol).is_some_and(|ty| {
            matches!(
                self.typed.signatures().types().get(ty),
                Some(UnitTypeKind::Function { .. })
            )
        }) {
            state.origins.bind(symbol, state.origins.result);
        }
    }
    pub(super) fn seed_lambda_parameters(
        &self,
        lambda: ExpressionId,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        if let Expression::Lambda { parameters, .. } =
            self.parsed.ast().expressions().get(lambda)?.payload()
        {
            for span in parameters {
                if let Some(symbol) = self.symbols_by_span.get(&super::span_key(*span)).copied() {
                    self.seed_callable_parameter(symbol, state);
                }
            }
        }
        Ok(())
    }
    pub(super) fn enter_callable(
        &mut self,
        name: NameMarker,
    ) -> Option<ReturnFrame<UnitCallableTarget, UnitTypeId, UnitExpressionId>> {
        let previous = self.callable_sources.active_return.take();
        let symbol = match name {
            NameMarker::Present(span) => self.symbols_by_span.get(&super::span_key(span)).copied(),
            _ => None,
        };
        let callable = self
            .typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.callable())
            .find(|callable| match callable.target() {
                UnitCallableTarget::Declaration(declaration) => {
                    self.names.names().declaration_symbol(declaration) == symbol
                }
                UnitCallableTarget::Symbol(local) => Some(local) == symbol,
            });
        if let Some(callable) = callable
            && matches!(callable.target(), UnitCallableTarget::Declaration(_))
            && callable.receiver().is_none()
            && matches!(
                self.typed.signatures().types().get(callable.return_type()),
                Some(UnitTypeKind::Function { .. })
            )
        {
            self.callable_sources.active_return = Some(ReturnFrame {
                target: callable.target(),
                function_type: callable.return_type(),
                deliveries: Vec::new(),
            });
        }
        previous
    }
    pub(super) fn leave_callable(
        &mut self,
        previous: Option<ReturnFrame<UnitCallableTarget, UnitTypeId, UnitExpressionId>>,
    ) {
        if let Some(frame) = self.callable_sources.active_return.take() {
            self.callable_sources.returns.push(frame);
        }
        self.callable_sources.active_return = previous;
    }
    pub(super) fn check_expression(
        &mut self,
        id: ExpressionId,
        mut state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.check_borrow_call_use(id, matches!(usage, ExpressionUse::Consume { .. }))?;
        state.origins.attach(&self.callable_sources.arena);
        let ast = self.parsed.ast().expressions().get(id)?;
        let span = ast.span();
        let expression = ast.payload().clone();
        let name = if matches!(expression, Expression::Name) {
            if let Some(target) = self.named_callable(id) {
                Some(
                    self.callable_sources
                        .leaf(UnitCallableOrigin::KnownFunction(target)),
                )
            } else {
                self.references_by_span
                    .get(&super::span_key(span))
                    .map(|symbol| state.origins.binding(*symbol))
            }
        } else {
            None
        };
        state.origins.result = Default::default();
        let is_lambda = matches!(expression, Expression::Lambda { .. });
        let previous = if is_lambda {
            self.callable_sources.active_return.take()
        } else {
            None
        };
        let previous_borrow = if is_lambda {
            self.current_borrow_return.take()
        } else {
            None
        };
        let previous_borrow_delivery = if is_lambda {
            std::mem::replace(&mut self.checking_borrow_return, false)
        } else {
            self.checking_borrow_return
        };
        let checked = self.check_expression_inner(id, state, usage);
        self.checking_borrow_return = previous_borrow_delivery;
        if is_lambda {
            self.current_borrow_return = previous_borrow;
        }
        if is_lambda {
            self.callable_sources.active_return = previous;
        }
        let mut flows = checked?;
        if let Some(next) = &mut flows.next {
            let value = if self.callable_type(id).is_some() {
                match expression {
                    Expression::Name => name.unwrap_or_default(),
                    Expression::Lambda { .. } => self
                        .callable_sources
                        .leaf(UnitCallableOrigin::Lambda(self.unit_expression(id))),
                    Expression::Group { .. } | Expression::If { .. } | Expression::When { .. } => {
                        next.origins.result
                    }
                    Expression::Call { .. }
                        if factory_target(self.typed, self.unit_expression(id)).is_some() =>
                    {
                        self.callable_sources
                            .leaf(UnitCallableOrigin::FactoryResult(self.unit_expression(id)))
                    }
                    _ => Default::default(),
                }
            } else {
                Default::default()
            };
            next.origins.result = value;
            if self.callable_type(id).is_some() {
                self.callable_sources.record(
                    self.unit_expression(id),
                    self.unit_expression(id),
                    value,
                    span,
                );
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
        if self.current_borrow_return.is_some() {
            return self.check_borrow_return(id, state);
        }
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
            let unit_id = self.unit_expression(id);
            if let Some(frame) = &mut self.callable_sources.active_return {
                frame.deliveries.push((
                    unit_id,
                    next.origins.result,
                    span,
                    direct && ty == Some(frame.function_type),
                ));
            }
        }
        Ok(flows)
    }
}

fn factory_target(
    typed: &crate::type_checking::CompilationUnitTypes,
    expression: UnitExpressionId,
) -> Option<UnitCallableTarget> {
    let call = typed.call(expression)?;
    let UnitCallTarget::Declaration(declaration) = call.target() else {
        return None;
    };
    let callable = typed.signatures().declaration(declaration)?.callable()?;
    (call.receiver().is_none()
        && callable.receiver().is_none()
        && matches!(
            typed.signatures().types().get(call.return_type()),
            Some(UnitTypeKind::Function { .. })
        ))
    .then_some(UnitCallableTarget::Declaration(declaration))
}

pub(super) fn finish(
    collection: &Collection,
    typed: &crate::type_checking::CompilationUnitTypes,
    captures: &[crate::ownership_checking::UnitClosureCaptureDescriptor],
) -> UnitCallableFacts {
    let graph = collection.arena.borrow();
    let initial = graph.solve(|origin| !matches!(origin, UnitCallableOrigin::FactoryResult(_)));
    let mut returns = Vec::new();
    for frame in &collection.returns {
        let [(expression, node, span, true)] = frame.deliveries.as_slice() else {
            continue;
        };
        let origin = match initial.get(*node) {
            Some(UnitCallableOrigin::Lambda(lambda))
                if !captures.iter().any(|capture| capture.lambda() == lambda) =>
            {
                UnitPointerCallableReturnOrigin::Lambda(lambda)
            }
            Some(UnitCallableOrigin::KnownFunction(target)) => {
                UnitPointerCallableReturnOrigin::KnownFunction(target)
            }
            _ => continue,
        };
        returns.push(UnitPointerCallableReturnSummary::new(
            frame.target,
            *expression,
            origin,
            frame.function_type,
            *span,
        ));
    }
    returns.sort_by_key(|summary| summary.target());
    let solved = graph.solve(|origin| match origin {
        UnitCallableOrigin::FactoryResult(call) => factory_target(typed, call)
            .is_some_and(|target| returns.iter().any(|summary| summary.target() == target)),
        _ => true,
    });
    let origins = collection
        .uses
        .values()
        .filter_map(|(expression, node, span)| {
            solved
                .get(*node)
                .map(|origin| UnitCallableOriginFact::new(*expression, origin, *span))
        })
        .collect();
    UnitCallableFacts { origins, returns }
}
