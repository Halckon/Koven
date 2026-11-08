//! Subset rules for `RangeView` (`View<T>`), `borrow val` bindings, `Map.requireValue`,
//! and `borrow_return` / `carrier_return` contracts. They mirror `build/range_borrow.rs`.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingStorage, BorrowSourceLoan, RangeUseSite, UnitIterationCleanupAction as Action,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CallableResultSource, IntrinsicTypeConstructor,
        IterationProvider, ParameterMode, UnitCallTarget, UnitConstructionTarget, UnitExpressionId,
        UnitSequentialIterationBinding, UnitStatementId, UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    /// Returns true if the current function scope uses a construct that requires `ssa::refine`
    /// (`Inout`, ownership operations, or integer/range families retired from legacy lowering).
    pub(super) fn scope_requires_refine(&self) -> bool {
        if self.inputs.modes.contains(&ParameterMode::Inout)
            || self
                .inputs
                .receiver
                .is_some_and(|r| r.mode == ParameterMode::Inout)
        {
            return true;
        }
        let Some(scope) = self.inputs.scope_span() else {
            return !self.inputs.owned.field_replacements().is_empty()
                || !self.inputs.typed.ownership_primitives().is_empty();
        };
        self.span_requires_refine(scope)
    }

    /// Returns true if any declaration or expression in the compilation unit uses a construct
    /// that requires `ssa::refine`, enabling helper `borrow_return` and `carrier_return`
    /// functions in refined units to lower through `ssa::refine`.
    pub(super) fn unit_requires_refine(&self) -> bool {
        if self.scope_requires_refine()
            || !self.inputs.owned.field_replacements().is_empty()
            || !self.inputs.typed.ownership_primitives().is_empty()
        {
            return true;
        }
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expr_id, node)| self.expr_requires_refine(expr_id, node.payload()))
    }

    fn span_requires_refine(&self, scope: Span) -> bool {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expr_id, node)| {
                let span = node.span();
                scope.source_id() == span.source_id()
                    && scope.start() <= span.start()
                    && span.end() <= scope.end()
                    && self.expr_requires_refine(expr_id, node.payload())
            })
    }

    fn expr_requires_refine(&self, expr_id: ExpressionId, payload: &Expression) -> bool {
        let id = self.id(expr_id);
        let typed = self.inputs.typed;
        if typed.ownership_primitive(id).is_some()
            || self.inputs.owned.field_replacement(id).is_some()
            || self
                .inputs
                .callables
                .get(&id)
                .is_some_and(|callable| !callable.captures.is_empty())
            || typed
                .non_null_assertion(id)
                .is_some_and(|d| self.map_kv(self.c(d.inner_type())).is_some())
            || typed.rc_operation(id).is_some()
            // `RangeView.size` (SPEC-0393) and `String.clone()` (SPEC-0394) lower only here.
            || typed.range_size(id).is_some()
            || typed.string_operation(id).is_some()
            // Borrow-result and `RangeView` calls lower only here (SPEC-0393).
            || typed.call(id).is_some_and(|call| {
                call.borrow_return().is_some()
                    || matches!(call.result_source(), CallableResultSource::Carrier(_))
            })
            || typed.integer_operation(id).is_some()
            || typed.integer_range_operation(id).is_some()
            || typed.map_with_value(id).is_some()
            || typed.map_require_value(id).is_some()
            || typed.map_get(id).is_some()
            || typed.container_append(id).is_some()
            || typed.container_clear(id).is_some()
            || typed.container_remove_at(id).is_some()
            || typed.container_remove_first(id).is_some()
            || typed.container_remove_last(id).is_some()
            || typed.container_insert_at(id).is_some()
            || typed.element_place(id).is_some()
            || typed.container_construction(id).is_some_and(|d| {
                d.kind() == lang_frontend::type_checking::ContainerConstructionKind::RuntimeLength
            })
            || typed
                .aggregate_projection(id)
                .is_some_and(|p| match p.receiver() {
                    lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(
                        recv_expr,
                    ) => self.expression_type_of(recv_expr).is_some_and(|ty| {
                        (self.value_class(ty) || self.move_value_class(ty))
                            && self.copyability_of(self.c(p.ty()))
                                == lang_frontend::type_checking::Copyability::MoveOnly
                    }),
                    _ => false,
                })
            || typed.construction(id).is_some_and(|d| {
                matches!(
                    d.target(),
                    UnitConstructionTarget::IntrinsicBox | UnitConstructionTarget::IntrinsicRc
                )
            })
        {
            return true;
        }
        matches!(
            payload,
            Expression::Assignment { target, .. }
                if self.is_field_target(*target)
                    || self.is_element_target(*target)
                    || self.is_map_put_target(*target)
        )
    }

    /// Strips outer proof views for any binding reassigned inside `loop_span`.
    /// Returns whether this invalidated a view using the local-destructuring route.
    pub(super) fn strip_reassigned_loop_views(
        &mut self,
        loop_span: Span,
        local_route: bool,
    ) -> bool {
        if self.view_symbols.is_empty() {
            return false;
        }
        let uses_local_route = local_route && self.loop_route != super::LoopRoute::Existing;
        if !uses_local_route && !self.scope_requires_refine() {
            self.loop_route_retry |= local_route;
            return false;
        }
        let mut reassigned = Vec::new();
        for (_, node) in self.inputs.parsed.ast().expressions().iter() {
            let span = node.span();
            if loop_span.source_id() == span.source_id()
                && loop_span.start() <= span.start()
                && span.end() <= loop_span.end()
                && let Expression::Assignment { target, .. } = node.payload()
                && !self.is_field_target(*target)
                && !self.is_element_target(*target)
                && !self.is_map_put_target(*target)
                && let Some(symbol) = self.reference(*target)
            {
                reassigned.push(symbol);
            }
        }
        let mut invalidated = false;
        for symbol in reassigned {
            invalidated |= self.view_symbols.remove(&symbol);
        }
        invalidated && uses_local_route
    }

    /// A concrete `View<T>` whose element type `T` is a supported storage type.
    pub(super) fn range_view_element(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        self.range_view_element_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn range_view_element_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> Option<UnitTypeId> {
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::View,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let [element] = arguments.as_slice() else {
            return None;
        };
        self.storage_type_seen(*element, active).then_some(*element)
    }

    pub(super) fn local_borrow_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) -> bool {
        let id = self.id(initializer);
        let Some(fact) = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .cloned()
        else {
            return false;
        };
        let UnitLoanTarget::Place(place) = fact.origin() else {
            return false;
        };
        if place.element().is_some() {
            return false;
        }
        // A direct alias of a Borrow parameter stays on the per-family path that the legacy
        // `Map` borrowed iteration shares, unless the scope already needs this layer.
        if fact.storage() == BorrowBindingStorage::BorrowedStorage
            && fact.source_loan().is_none()
            && place.is_root()
            && self.parameter_mode(place.root()) == Some(ParameterMode::Borrow)
            && !self.scope_requires_refine()
        {
            return false;
        }
        if fact.storage() == BorrowBindingStorage::NewRangeDescriptor {
            if fact.source_loan().is_none() || !place.is_root() || !self.range_call(initializer) {
                return false;
            }
            self.known.insert(symbol);
            self.for_borrowed.insert(symbol);
            self.range_view_bindings.insert(symbol);
            self.migrated = true;
            return true;
        }
        if fact.storage() == BorrowBindingStorage::BorrowedCarrierMetadata
            && fact.source_loan().is_none()
            && let Some(parent) = self.reference(self.ungrouped(initializer))
            && self.range_view_bindings.contains(&parent)
        {
            if fact.parent() != Some(parent) {
                return false;
            }
            self.known.insert(symbol);
            self.for_borrowed.insert(symbol);
            self.migrated = true;
            return true;
        }
        let Some(declared) = self.symbol_type(symbol) else {
            return false;
        };
        if !self.storage_type(declared) && !self.nullable_storage(declared) {
            return false;
        }
        let ok = match fact.source_loan() {
            Some(source) => self.result_call(initializer, source),
            None => self.result_place(initializer),
        };
        if ok {
            self.known.insert(symbol);
            self.for_borrowed.insert(symbol);
            self.migrated = true;
        }
        ok
    }

    fn result_place(&self, initializer: ExpressionId) -> bool {
        let root = self.ungrouped(initializer);
        let id = self.id(root);
        let Some(ty) = self.expression_type(root) else {
            return false;
        };
        if !self.storage_type(ty) {
            return false;
        }
        if let Some(projection) = self.inputs.typed.aggregate_projection(id) {
            let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
                projection.receiver()
            else {
                return false;
            };
            if receiver.source_unit() != self.inputs.source_unit
                || projection.kind()
                    != lang_frontend::type_checking::UnitAggregateProjectionKind::Field
                || self.c(projection.ty()) != ty
                || !self.unspecialized(root)
            {
                return false;
            }
            let Some(owner_ty) = self.expression_type(receiver.expression()) else {
                return false;
            };
            let has_field = self.class_field(owner_ty, projection.field())
                || ((self.value_class(owner_ty) || self.move_value_class(owner_ty))
                    && self.value_signature(owner_ty).is_some_and(|(nominal, _)| {
                        nominal
                            .fields()
                            .iter()
                            .any(|candidate| candidate.symbol() == projection.field())
                    }));
            return has_field && self.result_place(receiver.expression());
        }
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        matches!(node.payload(), Expression::Name)
            && self
                .reference(root)
                .is_some_and(|symbol| self.known.contains(&symbol))
    }

    pub(super) fn result_call(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
    ) -> bool {
        let actual = self.ungrouped(expression);
        let id = self.id(actual);
        if source.call() != id {
            return false;
        }
        if self.inputs.typed.map_require_value(id).is_some() {
            return self.map_require_value(actual, source);
        }
        let Ok(node) = self.inputs.parsed.ast().expressions().get(actual) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        let Some(descriptor) = self.inputs.typed.call(id).cloned() else {
            return false;
        };
        let Some(contract) = descriptor.borrow_return() else {
            return false;
        };
        let BorrowReturnOrigin::Parameter(index) = contract.origin() else {
            return false;
        };
        if descriptor.receiver().is_some()
            || descriptor
                .arguments()
                .iter()
                .any(|m| m.mode() == ParameterMode::Inout)
            || !matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            )
            || !self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, id)
                .is_some_and(|site| self.inputs.function_ids.contains_key(site.key()))
        {
            return false;
        }
        let Some(selected) = descriptor
            .arguments()
            .iter()
            .find(|m| m.parameter_index() == index)
            .and_then(|m| arguments.get(m.argument_index()))
        else {
            return false;
        };
        if self.id(selected.value) != source.argument() {
            return false;
        }
        let values: Vec<_> = arguments.iter().map(|a| a.value).collect();
        self.arguments(actual, &values)
    }

    fn map_require_value(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
    ) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_require_value(id).copied() else {
            return false;
        };
        if source.call() != id
            || source.argument() != descriptor.receiver()
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.key().source_unit() != self.inputs.source_unit
        {
            return false;
        }
        let Some(map_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let Some((key_ty, value_ty)) = self.map_kv(map_ty) else {
            return false;
        };
        let key = descriptor.key().expression();
        self.c(descriptor.key_type()) == key_ty
            && self.c(descriptor.value_type()) == value_ty
            && self.expression_type(expression) == Some(value_ty)
            && self
                .named_map_owner(descriptor.receiver().expression(), map_ty, true)
                .is_some()
            && self.expression_type(key) == Some(key_ty)
            && (self.borrowed_string_name(key) || self.expression(key))
    }

    pub(super) fn range_call(&mut self, expression: ExpressionId) -> bool {
        let actual = self.ungrouped(expression);
        let id = self.id(actual);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(actual) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        let Some(descriptor) = self.inputs.typed.call(id).cloned() else {
            return false;
        };
        let CallableResultSource::Carrier(_) = descriptor.result_source() else {
            return false;
        };
        let Some(view_ty) = self.expression_type(actual) else {
            return false;
        };
        let Some(elem_ty) = self.range_view_element(view_ty) else {
            return false;
        };
        if let Some(construction) = descriptor.range_construction() {
            if construction.expression() != id
                || arguments.len() != 3
                || arguments[0].value != construction.source().expression()
                || arguments[1].value != construction.begin().expression()
                || arguments[2].value != construction.end().expression()
            {
                return false;
            }
            let src = arguments[0].value;
            let Some(src_ty) = self.expression_type(src) else {
                return false;
            };
            // The source is the root container or a view of it (`take(view, n)`).
            return (self.container_element(src_ty) == Some(elem_ty)
                || self.range_view_element(src_ty) == Some(elem_ty))
                && (self.borrowed_move_only_name(src, src_ty) || self.expression(src))
                && self.expression(arguments[1].value)
                && self.expression(arguments[2].value);
        } else if !self
            .inputs
            .source_plan
            .call_site(self.inputs.source_token, id)
            .is_some_and(|site| self.inputs.function_ids.contains_key(site.key()))
        {
            return false;
        }
        let values: Vec<_> = arguments.iter().map(|a| a.value).collect();
        self.arguments(actual, &values)
    }

    pub(super) fn range_size(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.range_size(id) else {
            return false;
        };
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || !self.expression_type(expression).is_some_and(|ty| {
                matches!(self.kind(ty), Some(UnitTypeKind::Builtin(BuiltinType::Int)))
            })
        {
            return false;
        }
        let root = self.ungrouped(descriptor.receiver().expression());
        // `this.size` in a `View<T>` receiver function reads the receiver loan.
        if self.reference(root).is_none()
            && matches!(
                self.inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(root)
                    .map(|node| node.payload()),
                Ok(Expression::This)
            )
        {
            let ok = self.inputs.receiver.is_some_and(|receiver| {
                receiver.mode == ParameterMode::Borrow
                    && self.range_view_element(receiver.ty).is_some()
            });
            self.migrated |= ok;
            return ok;
        }
        let Some(symbol) = self.reference(root) else {
            return false;
        };
        let ok = self.known.contains(&symbol)
            && (self.range_view_bindings.contains(&symbol)
                || self
                    .symbol_type(symbol)
                    .is_some_and(|ty| self.range_view_element(ty).is_some()));
        self.migrated |= ok;
        ok
    }

    pub(super) fn range_view_for(
        &mut self,
        statement: StatementId,
        source: ExpressionId,
        body: StatementId,
    ) -> bool {
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        let Some(plan) = self.inputs.owned.iteration(statement_id) else {
            return false;
        };
        let descriptor = plan.descriptor();
        let source_id = self.id(source);
        if self.inputs.typed.sequential_iteration(statement_id) != Some(descriptor)
            || descriptor.provider() != IterationProvider::RangeView
            || descriptor.statement() != statement_id
            || descriptor.source() != source_id
            || !matches!(
                plan.source_access(),
                UnitIterationSourceAccess::Owned
                    | UnitIterationSourceAccess::Shared
                    | UnitIterationSourceAccess::Temporary
            )
        {
            return false;
        }
        let view_ty = self.c(descriptor.source_type());
        let element_ty = self.c(descriptor.element_type());
        if self.range_view_element(view_ty) != Some(element_ty)
            || self.expression_type(source) != Some(view_ty)
        {
            return false;
        }
        let actual_source = self.ungrouped(source);
        let actual_id = self.id(actual_source);
        let is_short_range = self
            .inputs
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .any(|fact| fact.expression() == actual_id && fact.site() == RangeUseSite::Iteration);
        if is_short_range {
            if !self.range_call(actual_source) {
                return false;
            }
        } else {
            let Some(symbol) = self.reference(actual_source) else {
                return false;
            };
            if !self.known.contains(&symbol)
                || !(self.range_view_bindings.contains(&symbol)
                    || self.parameter_mode(symbol) == Some(ParameterMode::Borrow))
            {
                return false;
            }
        }
        if !plan.exits().iter().all(|exit| {
            exit.actions().iter().all(|action| {
                matches!(
                    action,
                    Action::Drop(_)
                        | Action::EndBinding { .. }
                        | Action::EndElement(_)
                        | Action::FinishProvider(_)
                        | Action::EndSource(_)
                        | Action::EndCallLoan(_)
                        | Action::EndReceiverLoan(_)
                )
            })
        }) {
            return false;
        }
        match descriptor.binding() {
            UnitSequentialIterationBinding::Discard => {}
            UnitSequentialIterationBinding::Name(symbol) => {
                if self.symbol_type(*symbol) != Some(element_ty) || !self.storage_type(element_ty) {
                    return false;
                }
                self.note_nullable(element_ty);
                self.known.insert(*symbol);
                self.for_borrowed.insert(*symbol);
            }
            UnitSequentialIterationBinding::Destructure(parts) => {
                if !(self.value_class(element_ty) || self.move_value_class(element_ty)) {
                    return false;
                }
                for part in parts {
                    let part_ty = self.c(part.ty());
                    if !self.storage_type(part_ty) {
                        return false;
                    }
                    self.note_nullable(part_ty);
                    if let Some(symbol) = part.symbol() {
                        if self.symbol_type(symbol) != Some(part_ty) {
                            return false;
                        }
                        self.known.insert(symbol);
                        self.for_borrowed.insert(symbol);
                    }
                }
            }
        }
        let ok = self.loop_body(body);
        self.migrated |= ok;
        ok
    }

    pub(super) fn nested_borrow_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
    ) -> bool {
        let call_id = self.id(call);
        let argument_id = self.id(argument);
        let Some(fact) = self
            .inputs
            .owned
            .borrow_results()
            .call_uses()
            .iter()
            .find(|fact| fact.call() == call_id && fact.argument() == argument_id)
            .cloned()
        else {
            return false;
        };
        let actual = self.ungrouped(argument);
        let ok = self.result_call(actual, fact.source_loan());
        self.migrated |= ok;
        ok
    }

    pub(super) fn short_range_argument(
        &mut self,
        argument: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        if self.range_view_element(parameter).is_none() {
            return false;
        }
        let actual = self.ungrouped(argument);
        let id = self.id(actual);
        if !self
            .inputs
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .any(|fact| fact.expression() == id && matches!(fact.site(), RangeUseSite::Call(_)))
        {
            return false;
        }
        let ok = self.range_call(actual);
        self.migrated |= ok;
        ok
    }

    pub(super) fn borrow_return_body(
        &mut self,
        control_expr: ExpressionId,
        value_expr: ExpressionId,
    ) -> bool {
        if self.inputs.borrow_return.is_none() {
            return false;
        }
        let return_id = self.id(control_expr);
        let value_id = self.id(value_expr);
        let Some(fact) = self
            .inputs
            .owned
            .borrow_return_origins()
            .iter()
            .find(|fact| fact.expression() == return_id || fact.expression() == value_id)
        else {
            return false;
        };
        let UnitLoanTarget::Place(place) = fact.origin() else {
            return false;
        };
        if place.element().is_some() {
            return false;
        }
        let actual = self.ungrouped(value_expr);
        let id = self.id(actual);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(actual) else {
            return false;
        };
        let ok = match node.payload() {
            Expression::Name | Expression::Member { .. } => self.result_place(actual),
            Expression::Call { .. } => self
                .inputs
                .owned
                .borrow_results()
                .forwarded_source_loans()
                .iter()
                .find(|s| s.call() == id)
                .copied()
                .is_some_and(|source| self.result_call(actual, source)),
            _ => false,
        };
        self.migrated |= ok;
        ok
    }

    pub(super) fn carrier_return_body(
        &mut self,
        control_expr: ExpressionId,
        value_expr: ExpressionId,
    ) -> bool {
        if self.inputs.carrier_return.is_none() {
            return false;
        }
        let return_id = self.id(control_expr);
        let value_id = self.id(value_expr);
        if !self
            .inputs
            .owned
            .borrow_results()
            .range_return_origins()
            .iter()
            .any(|f| f.expression() == return_id || f.expression() == value_id)
        {
            return false;
        }
        let ok = self.range_call(value_expr);
        self.migrated |= ok;
        ok
    }
}
