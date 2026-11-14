//! 显式 shared 结果绑定：来源解析、调用 loan 交接、词法 scope 权限恢复。
use super::loan::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget};
use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::borrow_result::origin_expression;
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity, codes},
    ownership_checking::{
        BorrowBindingFact, BorrowSourceLoan, LoanKind, LoanTarget, OwnershipPlace,
    },
    parser::{Expression, NameMarker, Statement},
    source::Span,
};

impl Checker<'_> {
    pub(super) fn borrow_result_call(&self, expression: ExpressionId) -> Option<ExpressionId> {
        crate::ownership_checking::borrow_result::borrow_result_call(
            self.parsed,
            expression,
            |call| self.borrowed_call_source_operand(call).is_some(),
        )
    }

    pub(super) fn canonical_borrow_place(
        &self,
        place: &OwnershipPlace,
        state: &State,
    ) -> OwnershipPlace {
        let Some(origin) = state.borrow_bindings.get(&place.root()) else {
            return place.clone();
        };
        // 父结果的来源 lease 可能覆盖 callee 内部投影；保持该 lease，不伪造 payload 地址。
        origin.clone()
    }

    pub(super) fn check_borrow_binding(
        &mut self,
        name: NameMarker,
        initializer: ExpressionId,
        marker: Span,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        // 循环动态实例运输尚未有普通结果证明；不把静态绑定 identity 当作循环实例。
        let no_carry_allowed = self
            .marker_symbol(name)
            .is_some_and(|binding| self.no_carry_allows(binding, initializer, marker));
        if !no_carry_allowed
            && self.parsed.ast().statements().iter().any(|(_, node)| {
                matches!(
                    node.payload(),
                    Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
                ) && node.span().start() <= marker.start()
                    && marker.end() <= node.span().end()
            })
        {
            self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow result loop continuation is not yet proven",
                marker,
            )?;
            return Ok(Flows::next(state));
        }
        if matches!(
            self.parsed.ast().expressions().get(initializer)?.payload(),
            Expression::If { .. } | Expression::When { .. }
        ) {
            self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow binding control-flow origin is not yet proven",
                marker,
            )?;
            return Ok(Flows::next(state));
        }
        let call = self.borrow_result_call(initializer);
        let before = self.diagnostics.len();
        let previous = std::mem::replace(&mut self.allowed_borrow_call, call);
        let checked = self.check_expression(initializer, state, ExpressionUse::Read);
        self.allowed_borrow_call = previous;
        let mut flows = checked?;
        if self.diagnostics.len() != before {
            return Ok(flows);
        }
        let Some(next) = flows.next.as_mut() else {
            return Ok(flows);
        };
        let Some(binding) = self.marker_symbol(name) else {
            return Ok(flows);
        };
        let actual = origin_expression(self.parsed, initializer, |call| {
            self.borrowed_call_source(call)
        })?;
        let needs_conversion = self
            .typed
            .symbol_type(binding)
            .is_some_and(|expected| self.borrow_needs_nullable_conversion(initializer, expected));
        if needs_conversion && let Some(call) = call {
            // A rejected binding does not take over the initializer's source loan.
            next.loans
                .retain(|loan| loan.owner != ActiveLoanOwner::Call(call));
        }
        let Some(place) = actual
            .map(|origin| self.place(origin))
            .transpose()?
            .flatten()
            .filter(|_| !needs_conversion)
        else {
            self.emit_borrow_binding_diagnostic(
                codes::INVALID_BORROW_CONTRACT,
                if needs_conversion {
                    "borrow binding requires existing nullable storage"
                } else {
                    "borrow binding requires a stable source place"
                },
                marker,
            )?;
            return Ok(flows);
        };
        let storage = self.range_binding_storage(initializer, call);
        let parent = (storage
            != crate::ownership_checking::BorrowBindingStorage::NewRangeDescriptor)
            .then(|| {
                next.borrow_bindings
                    .contains_key(&place.root())
                    .then_some(place.root())
            })
            .flatten();
        let origin = self.canonical_borrow_place(&place, next);
        let source_loan = call.and_then(|call| {
            let argument = self.borrowed_call_source(call)?;
            self.loans
                .iter()
                .find(|loan| {
                    loan.call() == call
                        && loan.argument() == argument
                        && loan.kind() == LoanKind::Shared
                })
                .map(|_| BorrowSourceLoan { call, argument })
        });
        if call.is_some() && source_loan.is_none() {
            self.emit_borrow_binding_diagnostic(
                codes::UNSUPPORTED_BORROW_FLOW,
                "borrow binding has no proven source loan",
                marker,
            )?;
            return Ok(flows);
        }
        // 原来源调用 loan 在同一交接边改由结果持有；无关参数已经在 CallReturn 结束。
        if let Some(source) = source_loan {
            let source_span = self
                .loans
                .iter()
                .find(|loan| loan.call() == source.call && loan.argument() == source.argument)
                .map(|loan| loan.begin_span());
            let mut transferred = false;
            for loan in &mut next.loans {
                if loan.owner == ActiveLoanOwner::Call(source.call)
                    && Some(loan.origin) == source_span
                {
                    loan.owner = ActiveLoanOwner::BorrowBinding(binding);
                    transferred = true;
                }
            }
            if !transferred {
                self.emit_borrow_binding_diagnostic(
                    codes::UNSUPPORTED_BORROW_FLOW,
                    "borrow source loan handoff is not proven",
                    marker,
                )?;
                return Ok(flows);
            }
        } else if self.access_place(&origin, super::AccessKind::SharedLoan, false, marker, next)? {
            next.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::BorrowBinding(binding),
                target: ActiveLoanTarget::Place(origin.clone()),
                kind: LoanKind::Shared,
                origin: marker,
            });
        }
        next.borrow_bindings.insert(binding, origin.clone());
        next.non_owning.insert(binding, marker);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        if crate::ownership_checking::research_borrow_frame::is_capturing() {
            crate::ownership_checking::research_borrow_frame::acquired(
                binding,
                next.borrow_bindings.get(&binding),
                next.non_owning.get(&binding),
                next.loans
                    .iter()
                    .filter(|loan| loan.owner == ActiveLoanOwner::BorrowBinding(binding))
                    .cloned()
                    .collect(),
            );
        }
        self.mark_available(name, next);
        let fact = BorrowBindingFact {
            binding,
            initializer,
            storage,
            origin: LoanTarget::Place(origin),
            parent,
            source_loan,
            marker,
        };
        if !self
            .borrow_results
            .bindings
            .iter()
            .any(|prior| prior.binding == binding)
        {
            self.borrow_results.bindings.push(fact);
        }
        self.no_carry_acquired(binding, next);
        Ok(flows)
    }

    pub(super) fn end_last_borrow_uses(
        &mut self,
        statement: StatementId,
        uses: Option<
            &crate::ownership_checking::borrow_last_use::StraightLineBorrowUses<
                crate::name_resolution::SymbolId,
            >,
        >,
        inherited: &std::collections::BTreeSet<crate::name_resolution::SymbolId>,
        flows: &mut Flows,
    ) {
        let Some(future) = uses.and_then(|uses| uses.after(statement)) else {
            return;
        };
        let Some(state) = flows.next.as_mut() else {
            return;
        };
        let active = state.borrow_bindings.keys().copied().collect();
        let dead = crate::ownership_checking::borrow_last_use::dead_bindings(
            &active,
            inherited,
            future,
            |symbol| {
                self.borrow_results
                    .bindings
                    .iter()
                    .find(|fact| fact.binding == symbol)
                    .and_then(|fact| fact.parent)
            },
        );
        // Creation order is a topological order: every parent preceded its child.
        let ended = self
            .borrow_results
            .bindings
            .iter()
            .rev()
            .filter(|fact| dead.contains(&fact.binding))
            .map(|fact| fact.binding)
            .collect::<Vec<_>>();
        for binding in ended {
            let no_carry_removal = self.no_carry_removing(binding, statement, state);
            #[cfg(any(test, feature = "loan-observation-test-support"))]
            let removal = crate::ownership_checking::research_borrow_frame::removing(
                binding,
                &state.borrow_bindings,
                &state.non_owning,
                &state.loans,
                |loan| loan.owner == ActiveLoanOwner::BorrowBinding(binding),
            );
            state.borrow_bindings.remove(&binding);
            state.non_owning.remove(&binding);
            state
                .loans
                .retain(|loan| loan.owner != ActiveLoanOwner::BorrowBinding(binding));
            #[cfg(any(test, feature = "loan-observation-test-support"))]
            crate::ownership_checking::research_borrow_frame::removed(
                removal,
                statement,
                &state.borrow_bindings,
                &state.non_owning,
                &state.loans,
            );
            self.borrow_results
                .ends
                .push(crate::ownership_checking::BorrowBindingEndFact {
                    binding,
                    point: crate::ownership_checking::DropPoint::AfterStatement(statement),
                });
            self.no_carry_removed(no_carry_removal, state);
        }
    }

    pub(super) fn check_borrow_scope(
        &mut self,
        scope: StatementId,
        elements: &[StatementId],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let no_carry_scope = self.no_carry_claim(scope, &state);
        let parents = state
            .borrow_bindings
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let frame = if crate::ownership_checking::research_borrow_frame::is_capturing() {
            let snapshot = self.resource_snapshot(&state);
            let simple = self.simple_resources(&snapshot);
            crate::ownership_checking::research_borrow_frame::begin(
                (
                    crate::ownership_checking::research_borrow_frame::BorrowFrameChecker::Single,
                    None,
                ),
                self.parsed,
                scope,
                &snapshot,
                simple,
                |call, initializer| self.research_direct_string_call(call, initializer),
                |name| {
                    use crate::ownership_checking::research_borrow_frame::Name;
                    match name {
                        Name::Declaration(marker) => self.marker_symbol(marker),
                        Name::Reference(span) => self.reference_symbol(span),
                    }
                    .map(Into::into)
                },
            )
        } else {
            None
        };
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let checkpoint = (self.borrow_results.ends.len(), self.diagnostics.len());
        #[cfg(not(any(test, feature = "loan-observation-test-support")))]
        let _ = scope;
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let body_ticket = self.research_zero_body_enter(scope, &state);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        let consumer_body = self.research_body_enter(scope, &state);
        let mut flows = self.check_elements(elements, state)?;
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        self.research_zero_body_close(body_ticket, &flows);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        self.research_body_close(consumer_body, &flows);
        #[cfg(any(test, feature = "loan-observation-test-support"))]
        if frame.is_some() {
            let ends = self.borrow_results.ends[checkpoint.0..]
                .iter()
                .map(|end| {
                    (
                        end.binding,
                        match end.point {
                            crate::ownership_checking::DropPoint::AfterStatement(id) => Some(id),
                            _ => None,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let exit = flows
                .next
                .as_ref()
                .map(|state| self.resource_snapshot(state));
            crate::ownership_checking::research_borrow_frame::finish(
                frame,
                exit.as_ref(),
                flows.breaks.is_none()
                    && flows.continues.is_none()
                    && self.diagnostics.len() == checkpoint.1,
                &ends,
            );
        }
        self.no_carry_close(scope, no_carry_scope, &flows);
        self.restore_borrow_scope(&parents, &mut flows);
        Ok(flows)
    }

    pub(super) fn restore_borrow_scope(
        &self,
        parents: &std::collections::BTreeSet<crate::name_resolution::SymbolId>,
        flows: &mut Flows,
    ) {
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            let ended = state
                .borrow_bindings
                .keys()
                .copied()
                .filter(|symbol| !parents.contains(symbol))
                .collect::<Vec<_>>();
            for symbol in ended {
                state.borrow_bindings.remove(&symbol);
                state.non_owning.remove(&symbol);
                state
                    .loans
                    .retain(|loan| loan.owner != ActiveLoanOwner::BorrowBinding(symbol));
            }
        }
    }

    pub(super) fn emit_borrow_binding_diagnostic(
        &mut self,
        code: &'static str,
        message: &'static str,
        span: Span,
    ) -> Result<(), OwnershipCheckingError> {
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            codes::catalog()?.resolve(code)?,
            message,
            span,
        )?);
        Ok(())
    }

    pub(super) fn continues_source_loan(&self, call: ExpressionId, argument: ExpressionId) -> bool {
        let source = BorrowSourceLoan { call, argument };
        self.borrow_results
            .bindings
            .iter()
            .any(|fact| fact.source_loan == Some(source))
            || self.borrow_results.forwarded.contains(&source)
            || self
                .borrow_results
                .call_uses
                .iter()
                .any(|fact| fact.source_loan() == source)
            || self
                .borrow_results
                .range_uses
                .iter()
                .any(|fact| fact.source_loan() == source)
    }
}
