//! `RangeView` (`View<T>`), `borrow val` bindings, `Map.requireValue`, and
//! `borrow_return` / `carrier_return` contracts in the refined CFG.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingStorage, BorrowSourceLoan, RangeUseSite, UnitDropPoint, UnitLoanTarget,
    },
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BorrowReturnOrigin, CallableResultSource, ParameterMode, UnitAggregateProjectionReceiver,
        UnitCallDescriptor, UnitCallReceiverOrigin, UnitExpressionId, UnitTypeId,
    },
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind, SsaTypeId, SsaTypeKind},
    refine::body::{ROp, RTerm, Slot},
};

type BorrowCallOperand = (Slot, Vec<Slot>, Option<ExpressionId>);

impl Builder<'_, '_> {
    pub(super) fn ungrouped_expr(
        &self,
        mut expression: ExpressionId,
    ) -> Result<ExpressionId, LoweringError> {
        while let (Expression::Group { expression: inner }, _) = self.node(expression)? {
            expression = inner;
        }
        Ok(expression)
    }

    pub(super) fn is_borrow_result_binding(
        &self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) -> bool {
        let id = self.unit(initializer);
        self.inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .any(|fact| fact.binding() == symbol && fact.initializer() == id)
    }

    pub(super) fn local_borrow_binding(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(initializer);
        let fact = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.binding() == symbol && fact.initializer() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if fact.storage() == BorrowBindingStorage::NewRangeDescriptor {
            let Some((view, loan, created)) = self.lower_range_call(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.range_views.insert(symbol, view);
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        }
        if fact.storage() == BorrowBindingStorage::BorrowedCarrierMetadata
            && fact.source_loan().is_none()
            && let Ok(root) = self.ungrouped_expr(initializer)
            && let Ok(parent_sym) = self.reference(self.span(root))
            && let Some(&view) = self.range_views.get(&parent_sym)
        {
            if fact.parent() != Some(parent_sym) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let target = self.ssa_type(self.expression_type(initializer)?, span)?;
            let place = self.value(
                ROp::RootPlace { owner: view },
                EntityType::Place(target),
                span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            self.borrowed.insert(symbol, loan);
            return Ok(Lowered::Unit);
        }
        let Some(source) = fact.source_loan() else {
            let Some((loan, created)) = self.lower_result_place(initializer, span)? else {
                return Ok(Lowered::Diverged);
            };
            self.borrowed.insert(symbol, loan);
            self.result_source_loans.insert(symbol, created);
            return Ok(Lowered::Unit);
        };
        let Some((loan, created)) = self.lower_result_call(initializer, source, span)? else {
            return Ok(Lowered::Diverged);
        };
        self.borrowed.insert(symbol, loan);
        self.result_source_loans.insert(symbol, created);
        Ok(Lowered::Unit)
    }

    pub(super) fn end_result_bindings(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let ends: Vec<_> = self
            .inputs
            .owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|fact| fact.point() == point)
            .copied()
            .collect();
        for fact in ends {
            let Some(loan) = self.borrowed.remove(&fact.binding()) else {
                continue;
            };
            let span = self
                .inputs
                .owned
                .borrow_results()
                .bindings()
                .iter()
                .find(|b| b.binding() == fact.binding())
                .map_or(self.inputs.span, |b| b.marker_span());
            if let Some(view) = self.range_views.remove(&fact.binding()) {
                self.push(ROp::RangeEnd { view, source: loan }, Vec::new(), span)?;
            } else {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            }
            if let Some(sources) = self.result_source_loans.remove(&fact.binding()) {
                for source in sources.into_iter().rev() {
                    self.push(ROp::BorrowEnd { loan: source }, Vec::new(), span)?;
                }
            }
        }
        Ok(())
    }

    fn lower_result_place(
        &mut self,
        initializer: ExpressionId,
        span: Span,
    ) -> Result<Option<(Slot, Vec<Slot>)>, LoweringError> {
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target: self.ssa_type(self.expression_type(initializer)?, span)?,
        };
        let mut created = Vec::new();
        let Some(source) = self.clone_field_loan(initializer, &mut created, span)? else {
            return Ok(None);
        };
        let loan = if created.last() == Some(&source) {
            created.pop();
            source
        } else {
            self.value(ROp::SharedReborrow { source }, ty, span)?
        };
        Ok(Some((loan, created)))
    }

    pub(super) fn clone_field_loan(
        &mut self,
        expression: ExpressionId,
        created: &mut Vec<Slot>,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let root = self.ungrouped_expr(expression)?;
        let id = self.unit(root);
        let target = self.ssa_type(self.expression_type(root)?, span)?;
        if let Some(projection) = self.inputs.typed.aggregate_projection(id) {
            let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
            else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let Some(base) = self.clone_field_loan(receiver.expression(), created, span)? else {
                return Ok(None);
            };
            let owner_ty = match self.inputs.typed.non_null_use(receiver) {
                Some(fact) => self.c(fact.narrowed_type()),
                None => self.expression_type(receiver.expression())?,
            };
            let field = *self
                .types
                .field_indices
                .get(&(owner_ty, projection.field()))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let op = if self.is_value_class(owner_ty) {
                ROp::SharedFieldLoan { base, field }
            } else {
                ROp::SharedHeapFieldLoan { base, field }
            };
            let loan = self.value(
                op,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            created.push(loan);
            return Ok(Some(loan));
        }
        let (payload, root_span) = self.node(root)?;
        if matches!(payload, Expression::Name) {
            if let Some(view) = self.narrowed_view(root)? {
                return Ok(Some(view));
            }
            if let Ok(symbol) = self.reference(root_span)
                && let Some(&loan) = self.borrowed.get(&symbol)
            {
                return Ok(Some(loan));
            }
        }
        let Some(owner) = self.required(root)? else {
            return Ok(None);
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        created.push(loan);
        Ok(Some(loan))
    }

    pub(super) fn lower_result_call(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<Option<(Slot, Vec<Slot>)>, LoweringError> {
        let actual = self.ungrouped_expr(expression)?;
        let id = self.unit(actual);
        if source.call() != id {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if self.inputs.typed.map_require_value(id).is_some() {
            return self.lower_map_require_result(actual, source, span);
        }
        let (Expression::Call { arguments, .. }, call_span) = self.node(actual)? else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let descriptor = self
            .inputs
            .typed
            .call(id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let index = match descriptor
            .borrow_return()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .origin()
        {
            BorrowReturnOrigin::Parameter(index) => index,
            BorrowReturnOrigin::Receiver => 0,
        };
        let site = self
            .inputs
            .source_plan
            .call_site(self.inputs.source_token, id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let callee = *self
            .inputs
            .function_ids
            .get(site.key())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.ssa_type(self.expression_type(actual)?, span)?;
        let Some((lowered_args, mut arg_loans, created_loans, view_reads)) =
            self.lower_source_call_arguments(id, &arguments, &descriptor, call_span)?
        else {
            return Ok(None);
        };
        let source_loan = *lowered_args
            .get(index)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let continued = arg_loans
            .remove(&source.argument())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result = self.value(
            ROp::BorrowCall {
                callee,
                arguments: lowered_args,
                source: source_loan,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            call_span,
        )?;
        for (loan, end_span) in created_loans.into_iter().rev() {
            self.pending_call_loans.retain(|&active| active != loan);
            if !continued.contains(&loan) {
                self.push(ROp::BorrowEnd { loan }, Vec::new(), end_span)?;
            }
        }
        for read_expr in view_reads {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read_expr)))?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Some((result, continued)))
    }

    fn lower_map_require_result(
        &mut self,
        expression: ExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<Option<(Slot, Vec<Slot>)>, LoweringError> {
        let id = self.unit(expression);
        let descriptor = *self
            .inputs
            .typed
            .map_require_value(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if source.call() != id || source.argument() != descriptor.receiver() {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let receiver_span = self.span(receiver);
        let Some((source_loan, created, receiver_read)) =
            self.borrow_call_operand_loan(receiver, receiver_span)?
        else {
            return Ok(None);
        };
        for &loan in &created {
            self.pending_call_loans.push(loan);
        }
        let key = descriptor.key().expression();
        let key_span = self.span(key);
        let Some((key_loan, key_created, key_read)) =
            self.borrow_call_operand_loan(key, key_span)?
        else {
            return Ok(None);
        };
        let target = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::MapRequireValue {
                source: source_loan,
                key: key_loan,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        for &loan in &created {
            self.pending_call_loans.retain(|&active| active != loan);
        }
        for loan in key_created.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        for arg in [receiver_read, key_read].into_iter().flatten() {
            self.drops(UnitDropPoint::AfterExpression(self.unit(arg)))?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Some((result, created)))
    }

    fn borrow_call_operand_loan(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<BorrowCallOperand>, LoweringError> {
        let root = self.ungrouped_expr(expression)?;
        let id = self.unit(root);
        if self.inputs.typed.aggregate_projection(id).is_some() {
            let mut created = Vec::new();
            let Some(loan) = self.clone_field_loan(root, &mut created, span)? else {
                return Ok(None);
            };
            return Ok(Some((loan, created, Some(expression))));
        }
        let (payload, root_span) = self.node(root)?;
        let target = self.ssa_type(self.expression_type(expression)?, span)?;
        if matches!(payload, Expression::Name) {
            if let Some(view) = self.narrowed_view(root)? {
                return Ok(Some((view, Vec::new(), Some(expression))));
            }
            if let Ok(symbol) = self.reference(root_span) {
                if let Some(&view) = self.range_views.get(&symbol) {
                    let place = self.value(
                        ROp::RootPlace { owner: view },
                        EntityType::Place(target),
                        span,
                    )?;
                    let loan = self.value(
                        ROp::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        },
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        },
                        span,
                    )?;
                    return Ok(Some((loan, vec![loan], Some(expression))));
                }
                if let Some(&source_loan) = self.borrowed.get(&symbol)
                    && self.inputs.typed.non_null_use(id).is_none()
                    && self
                        .inputs
                        .constant_owned
                        .is_none_or(|owned| owned.materialization_at(id).is_none())
                {
                    return Ok(Some((source_loan, Vec::new(), Some(expression))));
                }
            }
        }
        let Some(owner) = self.required(expression)? else {
            return Ok(None);
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(target), span)?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        Ok(Some((loan, vec![loan], None)))
    }

    #[allow(clippy::type_complexity)]
    fn lower_source_call_arguments(
        &mut self,
        call: UnitExpressionId,
        arguments: &[CallArgument],
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<
        Option<(
            Vec<Slot>,
            std::collections::BTreeMap<UnitExpressionId, Vec<Slot>>,
            Vec<(Slot, Span)>,
            Vec<ExpressionId>,
        )>,
        LoweringError,
    > {
        let is_extension = match descriptor.target() {
            lang_frontend::type_checking::UnitCallTarget::Declaration(id) => self
                .inputs
                .typed
                .signatures()
                .declaration(id)
                .and_then(|sig| sig.callable())
                .is_some_and(|c| c.range_extension().is_some() || c.text_extension().is_some()),
            _ => false,
        };
        let ext_offset = usize::from(is_extension);
        let mut call_args = Vec::with_capacity(arguments.len() + ext_offset);
        if is_extension
            && let Some(rec) = descriptor.receiver()
            && let UnitCallReceiverOrigin::Expression(expr) = rec.origin()
            && expr.source_unit() == self.inputs.source_unit
        {
            call_args.push((
                0,
                expr.expression(),
                self.span(expr.expression()),
                rec.mode(),
                rec.ty(),
            ));
        }
        let mut sorted_args: Vec<_> = descriptor.arguments().iter().collect();
        sorted_args.sort_by_key(|a| a.argument_index());
        for arg in sorted_args {
            let call_arg = arguments
                .get(arg.argument_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            call_args.push((
                arg.parameter_index() + ext_offset,
                call_arg.value,
                call_arg.span,
                arg.mode(),
                arg.parameter_type(),
            ));
        }
        let mut lowered_slots: Vec<Option<Slot>> = vec![None; call_args.len()];
        let mut arg_loans = std::collections::BTreeMap::new();
        let mut created_loans = Vec::new();
        let mut view_reads = Vec::new();
        let mut delivered = Vec::new();
        for (param_slot, value_expr, call_arg_span, mode, param_ty) in call_args {
            let arg_id = self.unit(value_expr);
            if mode == ParameterMode::Borrow {
                if let Some((loan, created)) =
                    self.lower_nested_borrow_argument(call, value_expr, call_arg_span)?
                {
                    for &c in &created {
                        created_loans.push((c, call_arg_span));
                        self.pending_call_loans.push(c);
                    }
                    created_loans.push((loan, call_arg_span));
                    self.pending_call_loans.push(loan);
                    let mut all = created;
                    all.push(loan);
                    arg_loans.insert(arg_id, all);
                    lowered_slots[param_slot] = Some(loan);
                    continue;
                }
                if let Some((loan, created, read_expr)) =
                    self.lower_short_range_argument(value_expr, self.c(param_ty), call_arg_span)?
                {
                    for &c in &created {
                        created_loans.push((c, call_arg_span));
                        self.pending_call_loans.push(c);
                    }
                    arg_loans.insert(arg_id, created);
                    view_reads.extend(read_expr);
                    lowered_slots[param_slot] = Some(loan);
                    continue;
                }
                let Some((loan, created, read_expr)) =
                    self.borrow_call_operand_loan(value_expr, call_arg_span)?
                else {
                    return Ok(None);
                };
                for &c in &created {
                    created_loans.push((c, call_arg_span));
                    self.pending_call_loans.push(c);
                }
                arg_loans.insert(arg_id, created);
                view_reads.extend(read_expr);
                lowered_slots[param_slot] = Some(loan);
                continue;
            }
            let Some(value) = self.required(value_expr)? else {
                return Ok(None);
            };
            let mut value = self.adapt(value, value_expr, self.c(param_ty))?;
            value = self.snapshot_binding_if_name(value_expr, value)?;
            self.repoint_temporary(value_expr, value);
            delivered.push(value_expr);
            lowered_slots[param_slot] = Some(value);
        }
        let lowered = lowered_slots
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        for expr in delivered {
            self.deliver(expr);
        }
        Ok(Some((lowered, arg_loans, created_loans, view_reads)))
    }

    pub(super) fn lower_nested_borrow_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        span: Span,
    ) -> Result<Option<(Slot, Vec<Slot>)>, LoweringError> {
        let argument_id = self.unit(argument);
        let Some(fact) = self
            .inputs
            .owned
            .borrow_results()
            .call_uses()
            .iter()
            .find(|fact| fact.call() == call && fact.argument() == argument_id)
            .cloned()
        else {
            return Ok(None);
        };
        let actual = self.ungrouped_expr(argument)?;
        self.lower_result_call(actual, fact.source_loan(), span)
    }

    pub(super) fn lower_range_call(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<(Slot, Slot, Vec<Slot>)>, LoweringError> {
        let actual = self.ungrouped_expr(expression)?;
        let (Expression::Call { arguments, .. }, call_span) = self.node(actual)? else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let call = self.unit(actual);
        let descriptor = self
            .inputs
            .typed
            .call(call)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let CallableResultSource::Carrier(contract) = descriptor.result_source() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let index = match contract.origin() {
            BorrowReturnOrigin::Parameter(index) => index,
            BorrowReturnOrigin::Receiver => 0,
        };
        let view_ty = self.ssa_type(self.expression_type(actual)?, span)?;
        let Some(&SsaTypeKind::RangeView { source: source_ty }) =
            self.types.ssa_types.get(view_ty.index())
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let Some((lowered_args, _, created_loans, view_reads)) =
            self.lower_source_call_arguments(call, &arguments, &descriptor, call_span)?
        else {
            return Ok(None);
        };
        let source = *lowered_args
            .get(index)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let op = if descriptor.range_construction().is_some() {
            let begin = self.range_index_operand(lowered_args[1], span)?;
            let end = self.range_index_operand(lowered_args[2], span)?;
            ROp::RangeConstruct {
                view: view_ty,
                source,
                begin,
                end,
            }
        } else {
            let site = self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, call)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let callee = *self
                .inputs
                .function_ids
                .get(site.key())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            ROp::RangeCall {
                callee,
                arguments: lowered_args,
                source,
            }
        };
        let view = self.body.slot(EntityType::Value(view_ty));
        let protecting = self.body.slot(EntityType::Loan {
            kind: LoanKind::Shared,
            target: source_ty,
        });
        self.push(op, vec![view, protecting], call_span)?;
        let mut continued = Vec::new();
        for (created, end_span) in created_loans.into_iter().rev() {
            self.pending_call_loans.retain(|&active| active != created);
            if created == source && self.body.slots[source.0].semantic_type() == source_ty {
                continued.push(created);
            } else {
                self.end_short_call_loan(created, end_span)?;
            }
        }
        for read_expr in view_reads {
            self.drops(UnitDropPoint::AfterExpression(self.unit(read_expr)))?;
        }
        self.drops(UnitDropPoint::CallReturn(call))?;
        Ok(Some((view, protecting, continued)))
    }

    fn range_index_operand(&mut self, slot: Slot, span: Span) -> Result<Slot, LoweringError> {
        match self.body.slots[slot.0] {
            EntityType::Value(_) => Ok(slot),
            EntityType::Loan { target, .. } => self.value(
                ROp::ReadLoan { loan: slot },
                EntityType::Value(target),
                span,
            ),
            EntityType::Place(_) => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    pub(super) fn lower_short_range_argument(
        &mut self,
        expression: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<BorrowCallOperand>, LoweringError> {
        let actual = self.ungrouped_expr(expression)?;
        if let (Expression::Name, root_span) = self.node(actual)?
            && let Ok(symbol) = self.reference(root_span)
            && let Some(&view) = self.range_views.get(&symbol)
        {
            let target = self.ssa_type(parameter, span)?;
            let place = self.value(
                ROp::RootPlace { owner: view },
                EntityType::Place(target),
                span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                span,
            )?;
            return Ok(Some((loan, vec![loan], Some(expression))));
        }
        let id = self.unit(actual);
        if !self
            .inputs
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .any(|fact| fact.expression() == id && matches!(fact.site(), RangeUseSite::Call(_)))
        {
            return Ok(None);
        }
        let ty = self.ssa_type(parameter, span)?;
        let Some((view, protecting, continued)) = self.lower_range_call(actual, span)? else {
            return Ok(None);
        };
        self.short_ranges
            .insert(id, (view, protecting, continued.clone()));
        let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
        let metadata = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            },
            span,
        )?;
        let mut loans = continued;
        loans.push(protecting);
        loans.push(metadata);
        Ok(Some((metadata, loans, None)))
    }

    pub(super) fn end_short_call_loan(
        &mut self,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let short_id = self
            .short_ranges
            .iter()
            .find_map(|(&id, &(_, protecting, _))| (protecting == loan).then_some(id));
        if let Some(id) = short_id
            && let Some((view, _, _)) = self.short_ranges.remove(&id)
        {
            self.push(ROp::RangeEnd { view, source: loan }, Vec::new(), span)
        } else {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)
        }
    }

    pub(super) fn finish_short_range(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let actual = self.ungrouped_expr(expression)?;
        let id = self.unit(actual);
        let Some((view, protecting, continued)) = self.short_ranges.remove(&id) else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        self.push(
            ROp::RangeEnd {
                view,
                source: protecting,
            },
            Vec::new(),
            span,
        )?;
        for loan in continued.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        Ok(())
    }

    pub(super) fn range_iteration_loan(
        &mut self,
        source: ExpressionId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let actual = self.ungrouped_expr(source)?;
        let id = self.unit(actual);
        if self
            .inputs
            .owned
            .borrow_results()
            .range_uses()
            .iter()
            .any(|fact| fact.expression() == id && fact.site() == RangeUseSite::Iteration)
        {
            let Some((view, protecting, continued)) = self.lower_range_call(actual, span)? else {
                return Ok(None);
            };
            self.short_ranges.insert(id, (view, protecting, continued));
            let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                span,
            )?;
            return Ok(Some(loan));
        }
        let symbol = self.named_container_symbol(source, span)?;
        if let Some(&view) = self.range_views.get(&symbol) {
            let place = self.value(ROp::RootPlace { owner: view }, EntityType::Place(ty), span)?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
                span,
            )?;
            return Ok(Some(loan));
        }
        let caller_loan = *self
            .borrowed
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let loan = self.value(
            ROp::SharedReborrow {
                source: caller_loan,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            },
            span,
        )?;
        Ok(Some(loan))
    }

    pub(super) fn lower_range_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .range_size(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let view = self
            .range_views
            .get(&symbol)
            .or_else(|| self.borrowed.get(&symbol))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let result_ty = self.ssa_type(self.expression_type(expression)?, span)?;
        let result = self.value(
            ROp::RangeLength { view },
            EntityType::Value(result_ty),
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    pub(super) fn lower_borrow_return(
        &mut self,
        control_expr: ExpressionId,
        value_expr: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let return_id = self.unit(control_expr);
        let value_id = self.unit(value_expr);
        let fact = self
            .inputs
            .owned
            .borrow_return_origins()
            .iter()
            .find(|fact| fact.expression() == return_id || fact.expression() == value_id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let UnitLoanTarget::Place(_) = fact.origin() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let actual = self.ungrouped_expr(value_expr)?;
        let id = self.unit(actual);
        let (payload, node_span) = self.node(actual)?;
        let loan = match payload {
            Expression::Name | Expression::Member { .. } => {
                let mut created = Vec::new();
                let Some(loan) = self.clone_field_loan(actual, &mut created, span)? else {
                    return Ok(Lowered::Diverged);
                };
                loan
            }
            Expression::Call { .. } => {
                let source = self
                    .inputs
                    .owned
                    .borrow_results()
                    .forwarded_source_loans()
                    .iter()
                    .find(|source| source.call() == id)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, node_span))?;
                let Some((loan, _)) = self.lower_result_call(actual, source, node_span)? else {
                    return Ok(Lowered::Diverged);
                };
                loan
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, node_span)),
        };
        self.drops(UnitDropPoint::ControlTransfer(return_id))?;
        self.terminate(RTerm::BorrowReturn { loan }, span)?;
        Ok(Lowered::Diverged)
    }

    pub(super) fn lower_carrier_return(
        &mut self,
        control_expr: ExpressionId,
        value_expr: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let return_id = self.unit(control_expr);
        let actual = self.ungrouped_expr(value_expr)?;
        let Some((view, source, created)) = self.lower_range_call(actual, span)? else {
            return Ok(Lowered::Diverged);
        };
        if !created.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.drops(UnitDropPoint::ControlTransfer(return_id))?;
        self.terminate(RTerm::RangeReturn { view, source }, span)?;
        Ok(Lowered::Diverged)
    }
}
