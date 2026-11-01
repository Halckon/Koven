//! Subset rules for the closure family (SPEC-0364, SPEC-0365, SPEC-0370, ADR-0034):
//! Borrow function parameters are callable views, local callable bindings hold concrete
//! function pointers or closures, and lambda bodies lower as thunks.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode, LoanKind},
    parser::Expression,
    source::Span,
    type_checking::{
        Copyability, ParameterMode, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitCallTarget, UnitTypeId, UnitTypeKind,
    },
};

use super::{Checker, receiver::Owner};
use crate::ssa::refine::{RefineBody, RefineCapture, RefineThunk};

impl Checker<'_, '_> {
    /// A lambda thunk body and its captured environment bindings.
    pub(super) fn thunk_body(&mut self, thunk: &RefineThunk) -> bool {
        self.migrated = true;
        thunk
            .captures
            .iter()
            .all(|capture| self.storage_type(capture.ty))
            && self.statement(thunk.body)
    }

    /// A concrete function type whose callable view target is interned.
    pub(super) fn view_function(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Function { .. }))
            && self.inputs.views.contains_key(&ty)
    }

    /// A Borrow parameter of function type: a callable view.
    pub(super) fn view_parameter(&self, symbol: UnitSymbolId) -> bool {
        self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
            && self
                .symbol_type(symbol)
                .is_some_and(|ty| self.view_function(ty))
    }

    /// `f(...)` where `f` names a view parameter or a local callable binding.
    pub(super) fn function_callee(&mut self, call: ExpressionId, values: &[ExpressionId]) -> bool {
        let id = self.id(call);
        let Some(descriptor) = self.inputs.typed.call(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(call) else {
            return false;
        };
        let Expression::Call { callee, .. } = node.payload() else {
            return false;
        };
        let callee = self.ungrouped(*callee);
        let named = matches!(
            self.inputs
                .parsed
                .ast()
                .expressions()
                .get(callee)
                .map(|node| node.payload()),
            Ok(Expression::Name)
        );
        let supported = descriptor.target() == UnitCallTarget::FunctionValue
            && descriptor.receiver().is_none()
            && named
            && !values
                .iter()
                .any(|value| self.argument_has_control_transfer(*value))
            && self.reference(callee).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && (self.view_parameter(symbol) || self.callables.contains(&symbol))
            });
        self.migrated |= supported;
        supported
    }

    fn argument_has_control_transfer(&self, argument: ExpressionId) -> bool {
        let Ok(argument) = self.inputs.parsed.ast().expressions().get(argument) else {
            return true;
        };
        let span = argument.span();
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, candidate)| {
                span_contains(span, candidate.span())
                    && matches!(
                        candidate.payload(),
                        Expression::Return { .. }
                            | Expression::Break { .. }
                            | Expression::Continue { .. }
                    )
            })
            .any(|(_, candidate)| {
                !self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .any(|(_, boundary)| {
                        matches!(boundary.payload(), Expression::Lambda { .. })
                            && span_contains(span, boundary.span())
                            && strictly_contains(boundary.span(), candidate.span())
                    })
            })
    }

    /// The operand of a Borrow function parameter: a view parameter passed on, a local
    /// callable binding lent as a view, or a lambda literal formed and lent at the call.
    pub(super) fn callable_argument(&mut self, value: ExpressionId) -> bool {
        let root = self.ungrouped(value);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        let supported = match node.payload() {
            Expression::Name => self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && (self.view_parameter(symbol) || self.callables.contains(&symbol))
            }),
            Expression::Lambda { .. } => self.lambda_formation(root),
            _ => false,
        };
        self.migrated |= supported;
        supported
    }

    pub(super) fn lambda_formation(&self, lambda: ExpressionId) -> bool {
        self.inputs
            .callables
            .get(&self.id(lambda))
            .is_some_and(|callable| {
                callable
                    .captures
                    .iter()
                    .all(|capture| self.capture_supported(capture))
            })
    }

    /// A lambda returned by this function (its body or a `return` operand), whose closure
    /// return provenance the planner froze (SPEC-0365).
    pub(super) fn returned_lambda(&mut self, lambda: ExpressionId) -> bool {
        let supported = self.callable_return() && self.lambda_formation(lambda);
        self.migrated |= supported;
        supported
    }

    /// The function returns a callable whose provenance the planner froze.
    pub(super) fn callable_return(&self) -> bool {
        self.view_or_function(self.inputs.return_type)
            && self
                .inputs
                .source_plan
                .callable_return(self.inputs.source_token)
                .is_some()
    }

    pub(super) fn view_or_function(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Function { .. }))
    }

    /// A non-lambda expression whose result is a concrete callable value (a moved local
    /// callable binding or a direct call to a function returning a concrete callable).
    pub(super) fn callable_value(&self, expression: ExpressionId) -> bool {
        let root = self.ungrouped(expression);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        match node.payload() {
            Expression::Name => self
                .reference(root)
                .is_some_and(|symbol| self.callables.contains(&symbol)),
            Expression::Call { .. } => self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, self.id(root))
                .and_then(|site| self.inputs.function_ids.get(site.key()))
                .is_some_and(|callee| self.inputs.callable_returns.contains_key(callee)),
            _ => false,
        }
    }

    /// A local variable binding initialized with a lambda literal, a moved local callable,
    /// or a direct call returning a concrete callable.
    pub(super) fn callable_local(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) -> bool {
        let root = self.ungrouped(initializer);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        let supported = match node.payload() {
            Expression::Lambda { .. } => self.lambda_formation(root),
            _ => self.callable_value(root) && self.expression(initializer),
        };
        self.migrated |= supported;
        self.callables.insert(symbol);
        self.known.insert(symbol);
        supported
    }

    /// A Borrow String parameter or capture read through its shared loan in `==`, `!=`, `+`.
    pub(super) fn borrowed_string_name(&self, expression: ExpressionId) -> bool {
        let root = self.ungrouped(expression);
        self.inputs
            .parsed
            .ast()
            .expressions()
            .get(root)
            .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            && self.inputs.typed.non_null_use(self.id(root)).is_none()
            && self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
                    && self.symbol_type(symbol).is_some_and(|ty| self.string(ty))
            })
    }

    /// A MoveOnly Borrow parameter or capture lent directly to a Borrow call parameter.
    pub(super) fn borrowed_move_only_name(
        &self,
        expression: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        let root = self.ungrouped(expression);
        self.inputs
            .parsed
            .ast()
            .expressions()
            .get(root)
            .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            && self.inputs.typed.non_null_use(self.id(root)).is_none()
            && self.copyability_of(parameter) == Copyability::MoveOnly
            && self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
                    && self.symbol_type(symbol) == Some(parameter)
            })
    }

    /// `owner.field` of type `String` lent to a Borrow call parameter via `SharedHeapFieldLoan`.
    pub(super) fn borrowed_string_field(
        &self,
        call: ExpressionId,
        value: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        if !self.string(parameter) {
            return false;
        }
        let root = self.ungrouped(value);
        let id = self.id(root);
        let typed = self.inputs.typed;
        let Some(projection) = typed.aggregate_projection(id) else {
            return false;
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.c(projection.ty()) != parameter
            || self.expression_type_of(id) != Some(parameter)
            || !self.unspecialized(root)
        {
            return false;
        }
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit
                    || !self.plain(receiver.expression())
                    || typed.non_null_use(receiver).is_some()
                {
                    return false;
                }
                self.owner_type(
                    Owner::Expression(receiver.expression()),
                    ParameterMode::Borrow,
                )
            }
            UnitAggregateProjectionReceiver::This(declaration) => {
                self.owner_type(Owner::This(declaration), ParameterMode::Borrow)
            }
        };
        owner.is_some_and(|owner| self.class_field(owner, projection.field()))
            && self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == self.id(call)
                    && (loan.argument() == self.id(value) || loan.argument() == id)
                    && loan.kind() == LoanKind::Shared
            })
    }

    /// Shared captures borrow a known binding; owned captures copy a Copyable one or move
    /// an owned one.
    fn capture_supported(&self, capture: &RefineCapture) -> bool {
        let symbol = capture.symbol;
        let Some(ty) = self.symbol_type(symbol) else {
            return false;
        };
        if !self.known.contains(&symbol) || !self.storage_type(ty) {
            return false;
        }
        let mode = self.parameter_mode(symbol);
        match (capture.mode, capture.effect) {
            (ClosureCaptureMode::Shared, _) => mode != Some(ParameterMode::Inout),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Copy) => {
                self.copyability_of(ty) == Copyability::Copyable
                    && mode != Some(ParameterMode::Inout)
            }
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                matches!(mode, None | Some(ParameterMode::Value))
            }
            _ => false,
        }
    }

    /// Facts inside a nested lambda body belong to its thunk, not to this body.
    pub(super) fn inside_lambda(&self, origin: Span) -> bool {
        let current = match &self.inputs.body {
            RefineBody::Lambda(thunk) => Some(thunk.expression),
            _ => None,
        };
        let scope = self.inputs.scope_span();
        self.inputs.callables.keys().any(|lambda| {
            Some(*lambda) != current
                && self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(lambda.expression())
                    .is_ok_and(|node| {
                        let span = node.span();
                        scope.is_none_or(|outer| strictly_contains(outer, span))
                            && strictly_contains(span, origin)
                    })
        })
    }
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn strictly_contains(outer: Span, inner: Span) -> bool {
    outer != inner && span_contains(outer, inner)
}
