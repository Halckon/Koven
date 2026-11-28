//! 普通直接调用的观测切片；从实际 pending loan 和同一 cleanup 序列发布。
use super::{DropPlanner, PlannerDropPoint, TemporaryArgumentUnwinds, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{
        LoanKind, UnitDirectCallLoanEndFact, UnitIterationCleanupAction as Action, UnitLoanFact,
        UnitLoanTarget,
    },
    parser::{Expression, LiteralKind},
    type_checking::{
        BuiltinType, CallableResultSource, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, NominalKind, ParameterMode, UnitCallDescriptor,
        UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeKind,
    },
};
use std::collections::BTreeMap;

impl DropPlanner<'_, '_> {
    fn direct_call_for_loan(
        &self,
        loan: &UnitLoanFact,
        kind: LoanKind,
    ) -> Option<&UnitCallDescriptor> {
        self.checker.typed.call(loan.call()).filter(|call| {
            loan.kind() == kind
                && (matches!(
                    call.target(),
                    UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                ) || kind == LoanKind::Exclusive
                    && matches!(call.target(), UnitCallTarget::External(_)))
                && call.result_source() == CallableResultSource::Owned
                && self
                    .checker
                    .borrow_result_call(loan.argument().expression())
                    .is_none()
        })
    }

    // Shared also admits proven temporary owners and field parents. Exclusive
    // remains root-only; both reuse registered tokens and the existing unwind traversal.
    fn observes_call_loan(
        &self,
        loan: &UnitLoanFact,
        kind: LoanKind,
        control_transfer: bool,
    ) -> bool {
        if self
            .direct_call_for_loan(loan, kind)
            .is_none_or(|call| call.receiver().is_some())
        {
            return false;
        }
        // 在 canonical_borrow_place/temporary_projection_owner 之前核对真实 operand。
        // 目标表示本身无法区分投影 backing、borrow binding 或 provider 的来源。
        let mut operand = loan.argument().expression();
        loop {
            let Ok(node) = self.checker.parsed.ast().expressions().get(operand) else {
                return false;
            };
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Name => {
                    if kind == LoanKind::Shared
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument())
                        && self.checker.typed.expression_category(loan.argument())
                            == Some(ExpressionCategory::Place)
                    {
                        // A Copyable nullable conversion of the name has its own temporary loan.
                        return true;
                    }
                    let Ok(Some(place)) = self.checker.place(operand) else {
                        return false;
                    };
                    return place.is_root()
                        && self.is_observable_owned_loan_root(place.root())
                        && matches!(loan.target(), UnitLoanTarget::Place(target) if target == &place);
                }
                Expression::Member { .. } => {
                    // The field parent's cleanup obligation is independent of the loan target.
                    // Only a loan of the actual parent is observed by this field path.
                    return kind == LoanKind::Shared
                        && self
                            .call_temporary_field_parent(
                                loan.call().expression(),
                                loan.argument().expression(),
                                super::UnitCallArgumentOwnershipKind::SharedLoan,
                            )
                            .is_ok_and(|owner| {
                                owner.is_some_and(|owner| {
                                    matches!(loan.target(), UnitLoanTarget::Temporary(target)
                                if *target == self.checker.unit_expression(owner))
                                })
                            });
                }
                Expression::Call { .. } => {
                    // The actual unwind can also expose a loan of an owned String
                    // result from a source call.
                    let expression = self.checker.unit_expression(operand);
                    let owned_string_result = control_transfer
                        && self.checker.typed.call(expression).is_some_and(|call| {
                            matches!(
                                call.target(),
                                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
                            ) && call.result_source() == CallableResultSource::Owned
                                && self.checker.typed.types().get(call.return_type())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                        });
                    return kind == LoanKind::Shared
                        && (self
                            .checker
                            .construction_descriptors
                            .contains_key(&expression)
                            || owned_string_result)
                        && self.is_move_only_temporary(loan.argument().expression())
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument());
                }
                Expression::Literal(LiteralKind::Integer(_) | LiteralKind::Boolean(_)) => {
                    // Observe only the original bare Copyable scalar operand. A grouped
                    // literal is deliberately not widened by the traversal above.
                    let argument = loan.argument();
                    let Some(ty) = self.checker.typed.expression_type(argument) else {
                        return false;
                    };
                    let Some(contracts) = self.checker.contracts_by_call.get(&loan.call()) else {
                        return false;
                    };
                    let mut matching = contracts
                        .iter()
                        .filter(|contract| contract.argument() == argument);
                    let Some(contract) = matching.next() else {
                        return false;
                    };
                    if matching.next().is_some() || contract.parameter_type() != ty {
                        return false;
                    }
                    return kind == LoanKind::Shared
                        && operand == argument.expression()
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == argument)
                        && self.checker.typed.expression_category(argument)
                            == Some(ExpressionCategory::Temporary)
                        && self.checker.typed.copyability(ty) == Copyability::Copyable
                        && matches!(
                            self.checker.typed.types().get(ty),
                            Some(UnitTypeKind::Builtin(
                                BuiltinType::Int | BuiltinType::Boolean
                            ))
                        );
                }
                Expression::String { .. } => {
                    // The argument token is registered only after every interpolation
                    // completed. It borrows the outer String, not any interpolation input.
                    return kind == LoanKind::Shared
                        && self.is_move_only_temporary(loan.argument().expression())
                        && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == loan.argument());
                }
                _ => return false,
            }
        }
    }

    /// 全部 operands 已完成且 call 正常返回后，退休当前 call 的实际 pending token。
    /// 提前退出仍由既有 depth-aware unwind 发布同一种 EndCallLoan action。
    pub(super) fn finish_direct_call_loans(&mut self, call: ExpressionId, state: &mut ValueState) {
        for index in (0..state.pending_loans.len()).rev() {
            let Action::EndCallLoan(loan) = &state.pending_loans[index].0 else {
                continue;
            };
            if loan.call() != self.checker.unit_expression(call)
                || !self.observes_call_loan(loan, LoanKind::Shared, false)
            {
                continue;
            }
            let super::model::PendingLoan(action, _, acquisition) =
                state.pending_loans.remove(index);
            if let Action::EndCallLoan(loan) = &action {
                #[cfg(any(test, feature = "d10-catalog-experiment"))]
                if let Some(probe) = &mut self.origin_probe {
                    probe.push(
                        super::owner_transport::origin_probe::Event::DirectBorrowEnded {
                            loan: loan.clone(),
                            point: PlannerDropPoint::CallReturn(call)
                                .into_unit(self.checker.source_unit),
                        },
                    );
                }
                self.iteration_actions.observe_multi_auxiliary_return(
                    loan,
                    acquisition.as_ref(),
                    &state.values,
                    &state.borrow_bindings,
                    &state.borrow_binding_transports,
                );
                self.iteration_actions.observe_later_auxiliary_return(
                    loan,
                    acquisition.as_ref(),
                    &state.values,
                    &state.borrow_bindings,
                    &state.borrow_binding_transports,
                );
            }
            self.iteration_actions
                .push((PlannerDropPoint::CallReturn(call), action));
        }
    }

    fn names_singleton(&self, source: UnitExpressionId) -> bool {
        if source.source_unit() != self.checker.source_unit {
            return false;
        }
        let mut expression = source.expression();
        while let Ok(node) = self.checker.parsed.ast().expressions().get(expression) {
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Name => {
                    return self
                        .checker
                        .reference_symbol(node.span())
                        .and_then(|symbol| self.checker.symbol_kind(symbol))
                        == Some(SymbolKind::ObjectValue);
                }
                _ => return false,
            }
        }
        false
    }

    fn is_named_singleton_prefix(
        &self,
        call: &UnitCallDescriptor,
        expression: UnitExpressionId,
    ) -> bool {
        call.receiver().is_some_and(|receiver| {
            matches!(call.target(), UnitCallTarget::Symbol(_))
                && receiver.mode() == ParameterMode::Borrow
                // The observed exit may be one arm of a call that dispatches on another.
                && self.checker.call_dispatch.contains_key(&expression)
                && self.is_stateless_object_type(receiver.ty())
                && matches!(receiver.origin(), UnitCallReceiverOrigin::Expression(source)
                    if self.names_singleton(source))
        })
    }

    fn singleton_temporary_argument(&self, loan: &UnitLoanFact) -> bool {
        let argument = loan.argument();
        if argument.source_unit() != self.checker.source_unit {
            return false;
        }
        let owner = self
            .checker
            .constant_temporary_origin(argument.expression())
            .map_or(argument, |(owner, _)| owner);
        let types = self.checker.typed.types();
        let kind = self
            .checker
            .typed
            .expression_type(argument)
            .and_then(|ty| types.get(ty));
        let handle = match kind {
            Some(UnitTypeKind::Nullable(inner)) => types.get(*inner),
            _ => kind,
        };
        let supported = matches!(
            kind,
            Some(
                UnitTypeKind::Builtin(BuiltinType::String)
                    | UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::Array
                            | IntrinsicTypeConstructor::List
                            | IntrinsicTypeConstructor::MutableList,
                        ..
                    }
            )
        ) || matches!(
            handle,
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Box
                    | IntrinsicTypeConstructor::Rc
                    | IntrinsicTypeConstructor::Map
                    | IntrinsicTypeConstructor::MutableMap,
                ..
            })
        );
        supported
            && loan.target() == &UnitLoanTarget::Temporary(owner)
            && self.is_move_only_temporary(argument.expression())
            && self
                .direct_call_for_loan(loan, LoanKind::Shared)
                .is_some_and(|call| self.is_named_singleton_prefix(call, loan.call()))
    }

    fn class_result_argument(&self, loan: &UnitLoanFact) -> bool {
        let argument = loan.argument();
        let Some(call) = self.direct_call_for_loan(loan, LoanKind::Shared) else {
            return false;
        };
        if !self.is_move_only_temporary(argument.expression())
            || loan.target() != &UnitLoanTarget::Temporary(argument)
        {
            return false;
        }
        let singleton_prefix = self.is_named_singleton_prefix(call, loan.call());
        if call.receiver().is_some() && !singleton_prefix {
            return false;
        }
        let mut operand = argument.expression();
        loop {
            let Ok(node) = self.checker.parsed.ast().expressions().get(operand) else {
                return false;
            };
            match node.payload() {
                Expression::Group { expression } => operand = *expression,
                Expression::Call { .. } => break,
                _ => return false,
            }
        }
        let typed = self.checker.typed;
        let operand = self.checker.unit_expression(operand);
        let result_type = if let Some(producer) = typed.call(operand).filter(|producer| {
            matches!(
                producer.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            ) && producer.result_source() == CallableResultSource::Owned
        }) {
            producer.return_type()
        } else if singleton_prefix
            && let Some(construction) = self.checker.construction_descriptors.get(&operand)
        {
            construction.result_type()
        } else {
            return false;
        };
        let Some(UnitTypeKind::Nominal { declaration, .. }) = typed.types().get(result_type) else {
            return false;
        };
        typed
            .signatures()
            .declaration(*declaration)
            .and_then(|sig| sig.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::Class)
    }

    /// Project the call contracts from the same ordered cleanup actions.
    pub(super) fn direct_call_unwinds(
        &self,
    ) -> (Vec<UnitDirectCallLoanEndFact>, TemporaryArgumentUnwinds) {
        self.collect_direct_call_ends(true)
    }

    #[cfg(any(test, feature = "d10-catalog-experiment"))]
    pub(super) fn direct_call_loan_ends(&self) -> Vec<UnitDirectCallLoanEndFact> {
        self.collect_direct_call_ends(false).0
    }

    fn collect_direct_call_ends(
        &self,
        include_supplemental_unwinds: bool,
    ) -> (Vec<UnitDirectCallLoanEndFact>, TemporaryArgumentUnwinds) {
        // 本 planner 只处理一个 source 的 AST；稳定下标与 point 种类组成唯一计数 key。
        let mut preceding_drops: BTreeMap<(bool, usize), usize> = BTreeMap::new();
        let mut preceding_call_loan_ends = BTreeMap::<_, usize>::new();
        let mut ends = Vec::new();
        let mut temporary_unwinds = TemporaryArgumentUnwinds::default();
        for (point, action) in self.iteration_actions.iter() {
            let key = match point {
                PlannerDropPoint::CallReturn(call) => (false, call.index()),
                PlannerDropPoint::ControlTransfer(transfer) => (true, transfer.index()),
                _ => continue,
            };
            let count = preceding_drops.entry(key).or_insert(0);
            match action {
                Action::Drop(_) => *count += 1,
                Action::EndCallLoan(loan) => {
                    let loan_count = preceding_call_loan_ends.entry(key).or_default();
                    let destination = if self.observes_call_loan(loan, LoanKind::Shared, key.0) {
                        Some(&mut ends)
                    } else if include_supplemental_unwinds && key.0 {
                        if self.singleton_temporary_argument(loan) {
                            Some(&mut temporary_unwinds.singleton_temporaries)
                        } else if self.class_result_argument(loan) {
                            Some(&mut temporary_unwinds.class_results)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(destination) = destination {
                        destination.push(UnitDirectCallLoanEndFact {
                            loan: loan.clone(),
                            point: point.into_unit(self.checker.source_unit),
                            preceding_drops: *count,
                            preceding_call_loan_ends: *loan_count,
                        });
                    }
                    *loan_count += 1;
                }
                _ => {}
            }
        }
        (ends, temporary_unwinds)
    }

    /// Only actual unwind actions qualify; never infer execution from descriptors.
    pub(super) fn exclusive_call_prefix_ends(
        &self,
    ) -> Vec<crate::ownership_checking::UnitExclusiveCallPrefixEndFact> {
        let mut preceding_drops = BTreeMap::new();
        let mut preceding_call_loan_ends = BTreeMap::<_, usize>::new();
        let mut ends = Vec::new();
        for (point, action) in self.iteration_actions.iter() {
            let PlannerDropPoint::ControlTransfer(transfer) = point else {
                continue;
            };
            if !self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(*transfer)
                .is_ok_and(|node| {
                    matches!(
                        node.payload(),
                        Expression::Return { .. }
                            | Expression::Break { .. }
                            | Expression::Continue { .. }
                    )
                })
            {
                continue;
            }
            let count = preceding_drops.entry(transfer.index()).or_insert(0);
            match action {
                Action::Drop(_) => *count += 1,
                Action::EndCallLoan(loan) => {
                    // Count every actual end before filtering to the Exclusive slice.
                    let loan_count = preceding_call_loan_ends
                        .entry(transfer.index())
                        .or_default();
                    if self.observes_call_loan(loan, LoanKind::Exclusive, false) {
                        ends.push(crate::ownership_checking::UnitExclusiveCallPrefixEndFact {
                            loan: loan.clone(),
                            point: point.into_unit(self.checker.source_unit),
                            preceding_drops: *count,
                            preceding_call_loan_ends: *loan_count,
                        });
                    }
                    *loan_count += 1;
                }
                _ => {}
            }
        }
        ends
    }
}
