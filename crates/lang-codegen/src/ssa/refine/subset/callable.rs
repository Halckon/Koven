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
        UnitCallDescriptor, UnitCallTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
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
            // A loop jump out of an argument stays on the per-family path unless the scope
            // forms a capturing closure, which lowers only here.
            && (self.scope_requires_refine()
                || !values
                    .iter()
                    .any(|value| self.argument_has_bare_loop_jump(*value)))
            && self.reference(callee).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && (self.view_parameter(symbol) || self.callables.contains(&symbol))
            });
        self.migrated |= supported;
        supported
    }

    /// Whether the argument *is* a bare `break`/`continue`; an `if`/`when` wrapper
    /// lowers as a diverging branch value in the refinement layer.
    fn argument_has_bare_loop_jump(&self, argument: ExpressionId) -> bool {
        let Ok(argument) = self.inputs.parsed.ast().expressions().get(argument) else {
            return true;
        };
        matches!(
            argument.payload(),
            Expression::Break { .. } | Expression::Continue { .. }
        )
    }

    /// A `break` or `continue` out of a function-value call argument (not inside a lambda in
    /// it); a `return` or abort out of one lowers here.
    fn argument_has_loop_jump(&self, argument: ExpressionId) -> bool {
        let Ok(argument) = self.inputs.parsed.ast().expressions().get(argument) else {
            return true;
        };
        let span = argument.span();
        let expressions = self.inputs.parsed.ast().expressions();
        expressions
            .iter()
            .filter(|(_, candidate)| {
                span_contains(span, candidate.span())
                    && matches!(
                        candidate.payload(),
                        Expression::Break { .. } | Expression::Continue { .. }
                    )
            })
            .any(|(_, candidate)| {
                !expressions.iter().any(|(_, boundary)| {
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
            Expression::Name => {
                self.reference(root).is_some_and(|symbol| {
                    self.known.contains(&symbol)
                        && (self.view_parameter(symbol) || self.callables.contains(&symbol))
                }) || self.known_function(root)
            }
            Expression::Lambda { .. } => self.lambda_formation(root),
            _ => false,
        };
        self.migrated |= supported;
        supported
    }

    pub(super) fn known_function(&self, expression: ExpressionId) -> bool {
        use lang_frontend::{
            name_resolution::{Namespace, UnitReferenceTarget},
            ownership_checking::UnitCallableOrigin,
            type_checking::UnitCallableTarget,
        };
        let root = self.ungrouped(expression);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        if !matches!(node.payload(), Expression::Name) {
            return false;
        }
        let id = self.id(root);
        let Some(fact) = self.inputs.owned.callable_origin(id) else {
            return false;
        };
        let UnitCallableOrigin::KnownFunction(target) = fact.origin() else {
            return false;
        };
        self.inputs.known_functions.contains_key(&target)
            && self
                .expression_type(root)
                .is_some_and(|ty| matches!(self.kind(ty), Some(UnitTypeKind::Function { .. })))
            && self
                .inputs
                .names
                .names()
                .references()
                .iter()
                .any(|reference| {
                    reference.source_unit() == self.inputs.source_unit
                        && reference.span() == node.span()
                        && reference.namespace() == Some(Namespace::Value)
                        && match (target, reference.target()) {
                            (
                                UnitCallableTarget::Declaration(expected),
                                UnitReferenceTarget::Declaration(actual),
                            ) => expected == *actual,
                            (
                                UnitCallableTarget::Declaration(expected),
                                UnitReferenceTarget::OverloadSet(actual),
                            ) => actual.contains(&expected),
                            (
                                UnitCallableTarget::Symbol(expected),
                                UnitReferenceTarget::Symbol(actual),
                            ) => expected == *actual,
                            _ => false,
                        }
                })
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
            Expression::Name => {
                self.reference(root)
                    .is_some_and(|symbol| self.callables.contains(&symbol))
                    || self.known_function(root)
            }
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
            .is_ok_and(|node| matches!(node.payload(), Expression::Name | Expression::This))
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
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        if self.inputs.typed.non_null_use(self.id(root)).is_some()
            || self.copyability_of(parameter) != Copyability::MoveOnly
        {
            return false;
        }
        match node.payload() {
            Expression::Name => self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
                    && self.symbol_type(symbol) == Some(parameter)
            }),
            Expression::This => {
                self.inputs.receiver.is_some_and(|receiver| {
                    matches!(receiver.mode, ParameterMode::Borrow | ParameterMode::Inout)
                        && receiver.ty == parameter
                }) || self.reference(root).is_some_and(|symbol| {
                    self.known.contains(&symbol)
                        && matches!(
                            self.parameter_mode(symbol),
                            Some(ParameterMode::Borrow | ParameterMode::Inout)
                        )
                        && self.symbol_type(symbol) == Some(parameter)
                })
            }
            _ => false,
        }
    }

    /// `owner.field` (or nested `this.a.b` / `owner.a.b`) of a MoveOnly type lent to a Borrow
    /// call parameter via `SharedHeapFieldLoan` / `SharedFieldLoan`.
    pub(super) fn borrowed_string_field(
        &mut self,
        call: ExpressionId,
        value: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        let Ok(call_node) = self.inputs.parsed.ast().expressions().get(call) else {
            return false;
        };
        let is_extension = self
            .inputs
            .typed
            .call(self.id(call))
            .is_some_and(|d| self.is_extension_call(d));
        let is_deinit = self.inputs.function_item.is_some_and(|item| {
            matches!(
                self.inputs
                    .parsed
                    .ast()
                    .items()
                    .get(item)
                    .map(|node| node.payload()),
                Ok(lang_frontend::parser::Item::Deinit { .. })
            )
        });
        if (self.span_has_control_transfer(call_node.span())
            && !self.call_has_field_replace(call)
            && !is_extension
            && !is_deinit
            && !self.scope_requires_refine())
            || (self.copyability_of(parameter) != Copyability::MoveOnly && !is_deinit)
            || !self.storage_type(parameter)
        {
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
        let mut is_migrated_field = false;
        let field_ok = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver)
                if receiver.source_unit() == self.inputs.source_unit
                    && matches!(
                        typed.expression_type(receiver).and_then(|ty| self.kind(ty)),
                        Some(lang_frontend::type_checking::UnitTypeKind::EnumCase { .. })
                    ) =>
            {
                let Some(lang_frontend::type_checking::UnitTypeKind::EnumCase { root, .. }) =
                    typed.expression_type(receiver).and_then(|ty| self.kind(ty))
                else {
                    return false;
                };
                let receiver_expr = receiver.expression();
                let root_ty = self.c(*root);
                let is_name = self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(receiver_expr)
                    .is_ok_and(|node| matches!(node.payload(), Expression::Name));
                let ok = self.plain(receiver_expr)
                    && is_name
                    && self.box_enum_type(root_ty)
                    && self.payload_case(root_ty, projection.field()).is_some()
                    && self.reference(receiver_expr).is_some_and(|symbol| {
                        self.known.contains(&symbol)
                            && matches!(
                                self.parameter_mode(symbol),
                                None | Some(ParameterMode::Value)
                            )
                            && self.symbol_type(symbol) == Some(root_ty)
                    });
                if ok {
                    is_migrated_field = true;
                }
                ok
            }
            _ => {
                let multi_hop = matches!(
                    projection.receiver(),
                    UnitAggregateProjectionReceiver::Expression(receiver)
                        if receiver.source_unit() == self.inputs.source_unit
                            && typed
                                .aggregate_projection(
                                    self.id(self.ungrouped(receiver.expression())),
                                )
                                .is_some()
                );
                let value_param =
                    !multi_hop && self.borrowed_value_parameter_field(call, root, parameter);
                if value_param {
                    is_migrated_field = true;
                }
                self.field_loan_receiver(root, !multi_hop) == Some(parameter) || value_param
            }
        };
        let loans_match = self.inputs.owned.loans().iter().any(|loan| {
            loan.call() == self.id(call)
                && (loan.argument() == self.id(value) || loan.argument() == id)
                && loan.kind() == LoanKind::Shared
        }) || self.inputs.owned.receiver_facts().iter().any(|fact| {
            fact.call() == self.id(call)
                && matches!(
                    fact.source(),
                    lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(expr)
                        if expr == self.id(value) || expr == id
                )
                && fact.kind()
                    == lang_frontend::ownership_checking::UnitReceiverOwnershipKind::SharedLoan
        });
        let ok = field_ok && loans_match;
        if ok && is_migrated_field {
            self.migrated = true;
        }
        ok
    }

    fn borrowed_value_parameter_field(
        &self,
        call: ExpressionId,
        field_expr: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        if matches!(self.inputs.body, RefineBody::Lambda(_))
            || self.field_is_result_source(call, field_expr)
        {
            return false;
        }
        let id = self.id(field_expr);
        let Some(projection) = self.inputs.typed.aggregate_projection(id) else {
            return false;
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return false;
        };
        if receiver.source_unit() != self.inputs.source_unit || !self.plain(receiver.expression()) {
            return false;
        }
        let ungrouped = self.ungrouped(receiver.expression());
        let Ok(node) = self.inputs.parsed.ast().expressions().get(ungrouped) else {
            return false;
        };
        if !matches!(node.payload(), Expression::Name)
            || self.inputs.typed.non_null_use(self.id(ungrouped)).is_some()
        {
            return false;
        }
        let Some(symbol) = self.reference(ungrouped) else {
            return false;
        };
        if !self.known.contains(&symbol)
            || self.for_borrowed.contains(&symbol)
            || self.parameter_mode(symbol) != Some(ParameterMode::Borrow)
        {
            return false;
        }
        let Some(owner_ty) = self.symbol_type(symbol) else {
            return false;
        };
        (self.value_class(owner_ty) || self.move_value_class(owner_ty))
            && self
                .value_signature(owner_ty)
                .is_some_and(|(nominal, args)| {
                    args.is_empty()
                        && nominal.type_parameters().is_empty()
                        && nominal.fields().iter().any(|f| {
                            f.symbol() == projection.field() && self.c(f.ty()) == parameter
                        })
                })
    }

    fn field_is_result_source(&self, call: ExpressionId, field_expression: ExpressionId) -> bool {
        use lang_frontend::type_checking::{
            BorrowReturnOrigin, CallableResultSource, UnitCallReceiverOrigin,
        };
        let call_id = self.id(call);
        let Some(descriptor) = self.inputs.typed.call(call_id) else {
            return false;
        };
        let origin = match descriptor.result_source() {
            CallableResultSource::Owned => return false,
            CallableResultSource::Borrow(contract) => contract.origin(),
            CallableResultSource::Carrier(contract) => contract.origin(),
        };
        let source = match origin {
            BorrowReturnOrigin::Parameter(index) => {
                let Ok(node) = self.inputs.parsed.ast().expressions().get(call) else {
                    return true;
                };
                let Expression::Call { arguments, .. } = node.payload() else {
                    return true;
                };
                let Some(arg) = descriptor
                    .arguments()
                    .iter()
                    .find(|mapping| mapping.parameter_index() == index)
                    .and_then(|mapping| arguments.get(mapping.argument_index()))
                else {
                    return true;
                };
                arg.value
            }
            BorrowReturnOrigin::Receiver => {
                let Some(receiver) = descriptor.receiver() else {
                    return true;
                };
                let UnitCallReceiverOrigin::Expression(source) = receiver.origin() else {
                    return false;
                };
                if source.source_unit() != self.inputs.source_unit {
                    return true;
                }
                source.expression()
            }
        };
        self.ungrouped(source) == field_expression
    }

    /// Resolves the projected field type of a single- or multi-hop class/value-class field
    /// projection `r.f1.f2...` that can be borrowed in place.
    pub(super) fn field_loan_receiver(
        &self,
        expression: ExpressionId,
        allow_local_root: bool,
    ) -> Option<UnitTypeId> {
        let root = self.ungrouped(expression);
        let id = self.id(root);
        let typed = self.inputs.typed;
        let projection = typed.aggregate_projection(id)?;
        let field_ty = self.c(projection.ty());
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.expression_type_of(id) != Some(field_ty)
            || !self.storage_type(field_ty)
            || !self.unspecialized(root)
        {
            return None;
        }
        let parent_ty = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(declaration) => {
                self.owner_type(Owner::This(declaration), ParameterMode::Borrow)?
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return None;
                }
                let receiver_expr = receiver.expression();
                if typed
                    .aggregate_projection(self.id(self.ungrouped(receiver_expr)))
                    .is_some()
                {
                    let parent = self.field_loan_receiver(receiver_expr, allow_local_root)?;
                    if !self.class(parent) {
                        return None;
                    }
                    parent
                } else {
                    if !self.plain(receiver_expr) {
                        return None;
                    }
                    let ungrouped = self.ungrouped(receiver_expr);
                    let owner_ty =
                        self.owner_type(Owner::Expression(ungrouped), ParameterMode::Borrow)?;
                    let allowed_source = match self
                        .inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(ungrouped)
                        .map(|node| node.payload())
                    {
                        Ok(Expression::This) => true,
                        Ok(Expression::Name) => self.reference(ungrouped).is_some_and(|symbol| {
                            matches!(
                                self.parameter_mode(symbol),
                                Some(ParameterMode::Borrow | ParameterMode::Inout)
                            ) || typed.non_null_use(self.id(ungrouped)).is_some()
                                || (allow_local_root
                                    && self.parameter_mode(symbol).is_none()
                                    && self
                                        .class_signature(owner_ty)
                                        .is_some_and(|(_, args)| args.is_empty()))
                        }),
                        _ => false,
                    };
                    if !allowed_source {
                        return None;
                    }
                    owner_ty
                }
            }
        };
        self.class_field(parent_ty, projection.field())
            .then_some(field_ty)
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

    /// Returns true if the call expression tree contains a field replacement (`replace(&h.field, ...)`).
    ///
    /// Capability contract:
    /// Direct calls whose arguments involve field replacements require fine-grained loan management and
    /// automatic `BorrowEnd` insertion on early-exit paths in `ssa::refine`.
    /// Ordinary direct calls with control transfers but without field replacements remain on `unit_lower`
    /// to preserve legacy prefix scope rejection contracts.
    fn call_has_field_replace(&self, call: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(call) else {
            return false;
        };
        let cspan = node.span();
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expr_id, expr_node)| {
                let span = expr_node.span();
                cspan.source_id() == span.source_id()
                    && cspan.start() <= span.start()
                    && span.end() <= cspan.end()
                    && self
                        .inputs
                        .owned
                        .field_replacement(self.id(expr_id))
                        .is_some()
            })
    }

    pub(super) fn is_extension_call(
        &self,
        descriptor: &lang_frontend::type_checking::UnitCallDescriptor,
    ) -> bool {
        match descriptor.target() {
            UnitCallTarget::Declaration(id) => self
                .inputs
                .typed
                .signatures()
                .declaration(id)
                .and_then(|signature| signature.callable())
                .is_some_and(|callable| {
                    callable.range_extension().is_some() || callable.text_extension().is_some()
                }),
            _ => false,
        }
    }

    pub(super) fn named_nothing_call(&self, call: &UnitCallDescriptor) -> bool {
        matches!(
            call.target(),
            UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
        ) && !call.aborts()
            && self.nothing(self.c(call.return_type()))
    }

    pub(super) fn abandoned_borrow_result_call(&self, id: UnitExpressionId) -> bool {
        // Ordinary exits already unwind pending call loans. The dedicated
        // borrow/carrier return routes keep their existing admission boundary.
        self.inputs.borrow_return.is_none()
            && self.inputs.carrier_return.is_none()
            && self.inputs.typed.expression_falls_through(id) == Some(false)
            && self.inputs.owned.call_may_dispatch(id) == Some(false)
            && self.inputs.typed.call(id).is_some_and(|descriptor| {
                descriptor.borrow_return().is_some()
                    && descriptor.receiver().is_none()
                    && matches!(
                        descriptor.target(),
                        UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                    )
            })
            && self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, id)
                .is_some_and(|site| site.dispatch_key().is_none())
    }

    pub(super) fn call(&mut self, expression: ExpressionId, values: &[ExpressionId]) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.call(id) else {
            return false;
        };
        let site = self
            .inputs
            .source_plan
            .call_site(self.inputs.source_token, id);
        let prefix_only = site.is_some_and(|site| site.dispatch_key().is_none());
        let borrow_prefix =
            descriptor.borrow_return().is_some() && self.abandoned_borrow_result_call(id);
        let is_extension = self.is_extension_call(descriptor);
        let simple = (descriptor.receiver().is_none()
            || is_extension
            || self.receiver_call(expression))
            && descriptor.range_construction().is_none()
            && (borrow_prefix || descriptor.borrow_return().is_none())
            && descriptor
                .arguments()
                .iter()
                .filter(|argument| {
                    !prefix_only
                        || values.get(argument.argument_index()).is_some_and(|&value| {
                            self.inputs.owned.call_argument_completed(self.id(value))
                        })
                })
                .all(|argument| match argument.mode() {
                    ParameterMode::Value => self.storage_type(self.c(argument.parameter_type())),
                    ParameterMode::Borrow => {
                        self.storage_type(self.c(argument.parameter_type()))
                            || self.view_function(self.c(argument.parameter_type()))
                    }
                    ParameterMode::Inout => {
                        let param = self.c(argument.parameter_type());
                        (self.storage_type(param)
                            && self.copyability_of(param) == Copyability::Copyable)
                            || self.class(param)
                    }
                });
        // A call through a function value is further restricted to view parameters.
        let target = descriptor.prints_line()
            || descriptor.aborts()
            || descriptor.text_primitive().is_some()
            || descriptor.target() == UnitCallTarget::FunctionValue
            || (matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            ) && site.is_some_and(|site| {
                let delegation_ok = site.delegation().is_empty()
                    || (descriptor
                        .receiver()
                        .is_some_and(|r| r.mode() == ParameterMode::Borrow)
                        && site.delegation().first().is_some_and(|first| {
                            Some(first.outer_receiver())
                                == descriptor.receiver().map(|r| self.c(r.ty()))
                        })
                        && site.delegation().iter().all(|route| {
                            self.class(route.outer_receiver())
                                && self.class(route.delegate_receiver())
                                && self.class_field(route.outer_receiver(), route.field())
                        }));
                if !site.delegation().is_empty() && delegation_ok {
                    self.migrated = true;
                }
                delegation_ok
                    && site
                        .dispatch_key()
                        .is_none_or(|key| self.inputs.function_ids.contains_key(key))
            }));
        let ok = simple && target;
        self.migrated |= ok
            && (borrow_prefix
                || is_extension
                || descriptor.text_primitive().is_some()
                || self.named_nothing_call(descriptor));
        ok
    }

    /// Completed operands can require storage even if there is no callee instance.
    pub(super) fn demand_prefix_nullable_arguments(
        &self,
        id: UnitExpressionId,
        expression: &Expression,
        demands: &mut Vec<UnitTypeId>,
    ) {
        if self.inputs.owned.call_may_dispatch(id) != Some(false) {
            return;
        }
        let (Some(call), Expression::Call { arguments, .. }) =
            (self.inputs.typed.call(id), expression)
        else {
            return;
        };
        for argument in call.arguments() {
            if arguments
                .get(argument.argument_index())
                .is_some_and(|operand| {
                    self.inputs
                        .owned
                        .call_argument_completed(self.id(operand.value))
                })
            {
                let parameter = self.c(argument.parameter_type());
                if matches!(self.kind(parameter), Some(UnitTypeKind::Nullable(_))) {
                    demands.push(parameter);
                }
            }
        }
    }

    /// Arguments are plain expressions, except that a Borrow argument may be a MoveOnly
    /// non-null name read through its proof view.
    pub(super) fn arguments(&mut self, call: ExpressionId, values: &[ExpressionId]) -> bool {
        let Some(descriptor) = self.inputs.typed.call(self.id(call)) else {
            return false;
        };
        let is_extension = self.is_extension_call(descriptor);
        let prefix_only = self.inputs.owned.call_may_dispatch(self.id(call)) == Some(false);
        let mut modes: Vec<_> = descriptor
            .arguments()
            .iter()
            .map(|argument| {
                (
                    Some(argument.argument_index()),
                    argument.mode(),
                    self.c(argument.parameter_type()),
                )
            })
            .collect();
        let mut extension_rec = None;
        if is_extension
            && let Some(receiver) = descriptor.receiver()
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(expression) =
                receiver.origin()
            && expression.source_unit() == self.inputs.source_unit
        {
            extension_rec = Some(expression.expression());
            modes.insert(0, (None, receiver.mode(), self.c(receiver.ty())));
        }
        let mut reaches_operand = true;
        modes.into_iter().all(|(index, mode, parameter)| {
            if !reaches_operand {
                return true;
            }
            let Some(value) = index.map_or(extension_rec, |i| values.get(i).copied()) else {
                return false;
            };
            if mode == ParameterMode::Inout {
                return self.inout_argument(call, value);
            }
            if self.inputs.typed.expression_falls_through(self.id(value)) == Some(false) {
                reaches_operand = false;
                return self.expression(value);
            }
            if prefix_only && self.inputs.owned.call_argument_completed(self.id(value)) {
                self.note_nullable(parameter);
            }
            if mode == ParameterMode::Borrow && self.view_function(parameter) {
                return self.callable_argument(value);
            }
            if mode == ParameterMode::Borrow
                && let Some((parent, _)) =
                    self.inputs
                        .temporary_field_argument(self.id(call), value, parameter)
            {
                let ok = self
                    .expression_type(parent)
                    .is_some_and(|ty| self.class(ty))
                    && self.expression(parent);
                if ok {
                    self.temporary_field_parents.insert(self.id(parent));
                }
                self.migrated |= ok;
                return ok;
            }
            if mode == ParameterMode::Borrow
                && (self.nested_borrow_argument(call, value, parameter)
                    || self.short_range_argument(value, parameter)
                    || self.borrowed_move_only_name(value, parameter)
                    || self.borrowed_string_field(call, value, parameter)
                    || self.borrowed_rc_payload(call, value, parameter)
                    || self.borrowed_container_element(call, value, parameter))
            {
                return true;
            }
            // A borrowed MoveOnly value is wrapped for a nullable parameter only when it is
            // a temporary whose drop obligation can move to the wrapper.
            if mode == ParameterMode::Borrow
                && self.expression_type(value).is_some_and(|actual| {
                    actual != parameter && self.copyability_of(actual) != Copyability::Copyable
                })
                && self.inputs.typed.expression_category(self.id(value))
                    != Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
                && self
                    .inputs
                    .constant_owned
                    .is_none_or(|owned| owned.materialization_at(self.id(value)).is_none())
            {
                return false;
            }
            if mode == ParameterMode::Borrow
                && let Some(fact) = self.inputs.typed.non_null_use(self.id(value))
                && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::MoveOnly
            {
                let root = self.ungrouped(value);
                return self.rc_payload(self.c(fact.narrowed_type())).is_none()
                    && self.reference(root) == Some(fact.symbol())
                    && self.known.contains(&fact.symbol())
                    && self.view_symbols.contains(&fact.symbol())
                    && (!self.is_map_remove_symbol(fact.symbol())
                        || self.is_direct_statement_call(call));
            }
            self.expression(value)
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
