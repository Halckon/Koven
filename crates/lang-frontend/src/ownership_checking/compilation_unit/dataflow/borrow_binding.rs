//! 显式 shared 结果绑定：来源解析、调用 loan 交接、词法 scope 权限恢复。
use super::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget};
use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::ownership_checking::borrow_result::origin_expression;
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity, codes},
    ownership_checking::{
        BorrowBindingFact, BorrowSourceLoan, LoanKind, UnitLoanTarget as LoanTarget,
        UnitOwnershipPlace as OwnershipPlace,
    },
    parser::{Expression, NameMarker, Statement},
    source::Span,
};

impl Checker<'_> {
    pub(super) fn borrow_result_call(&self, mut expression: ExpressionId) -> Option<ExpressionId> {
        loop {
            match self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .ok()?
                .payload()
            {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Call { .. } if self.borrowed_call_source(expression).is_some() => {
                    return Some(expression);
                }
                _ => return None,
            }
        }
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
        if self.parsed.ast().statements().iter().any(|(_, node)| {
            matches!(
                node.payload(),
                Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
            ) && node.span().start() <= marker.start()
                && marker.end() <= node.span().end()
        }) {
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
        let Some(binding) = self.marker_symbol(name).copied() else {
            return Ok(flows);
        };
        let actual = origin_expression(self.parsed, initializer, |call| {
            self.borrowed_call_source(call)
        })?;
        let Some(place) = actual
            .map(|origin| self.place(origin))
            .transpose()?
            .flatten()
        else {
            self.emit_borrow_binding_diagnostic(
                codes::INVALID_BORROW_CONTRACT,
                "borrow binding requires a stable source place",
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
                    loan.call() == self.unit_expression(call)
                        && loan.argument() == self.unit_expression(argument)
                        && loan.kind() == LoanKind::Shared
                })
                .map(|_| BorrowSourceLoan {
                    call: self.unit_expression(call),
                    argument: self.unit_expression(argument),
                })
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
        } else if self.access_place(&origin, super::AccessKind::SharedLoan, marker, None, next)? {
            next.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::BorrowBinding(binding),
                target: ActiveLoanTarget::Place(origin.clone()),
                kind: LoanKind::Shared,
                reserved: false,
                origin: marker,
            });
        }
        next.borrow_bindings.insert(binding, origin.clone());
        next.non_owning.insert(binding, marker);
        self.mark_available(name, next);
        let fact = BorrowBindingFact {
            binding,
            initializer: self.unit_expression(initializer),
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
        Ok(flows)
    }

    pub(super) fn end_last_borrow_uses(
        &mut self,
        statement: StatementId,
        uses: Option<
            &crate::ownership_checking::borrow_last_use::StraightLineBorrowUses<
                crate::name_resolution::UnitSymbolId,
            >,
        >,
        inherited: &std::collections::BTreeSet<crate::name_resolution::UnitSymbolId>,
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
            state.borrow_bindings.remove(&binding);
            state.non_owning.remove(&binding);
            state
                .loans
                .retain(|loan| loan.owner != ActiveLoanOwner::BorrowBinding(binding));
            self.borrow_results
                .ends
                .push(crate::ownership_checking::BorrowBindingEndFact {
                    binding,
                    point: crate::ownership_checking::UnitDropPoint::AfterStatement(
                        crate::type_checking::UnitStatementId::new(self.source_unit, statement),
                    ),
                });
        }
    }

    pub(super) fn check_borrow_scope(
        &mut self,
        elements: &[StatementId],
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let parents = state
            .borrow_bindings
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut flows = self.check_elements(elements, state)?;
        self.restore_borrow_scope(&parents, &mut flows);
        Ok(flows)
    }

    pub(super) fn restore_borrow_scope(
        &self,
        parents: &std::collections::BTreeSet<crate::name_resolution::UnitSymbolId>,
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
}
