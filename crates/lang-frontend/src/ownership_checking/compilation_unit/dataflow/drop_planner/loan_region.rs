//! Conservative lexical/typed admission, separate from actual action closure validation.
use super::*;
use crate::ownership_checking::compilation_unit::loan_region_validation::Expected;
use crate::ownership_checking::{LoanKind, UnitLoanTarget};
use crate::type_checking::{CallableResultSource, UnitCallTarget, UnitItemId};

pub(super) struct AdmissionIndex {
    callables: BTreeMap<usize, UnitItemId>,
    pub(super) receivers: BTreeMap<UnitExpressionId, Vec<usize>>,
    loans: BTreeMap<UnitExpressionId, Vec<usize>>,
}

impl DropPlanner<'_, '_> {
    pub(super) fn enter_loan_region(&mut self, call: ExpressionId, state: &ValueState) -> usize {
        let callable = self
            .loan_region_index()
            .callables
            .get(&call.index())
            .copied();
        let expected = callable.and_then(|_| self.loan_region_expected(call, state));
        let receiver_protocol = self
            .checker
            .typed
            .call(self.checker.unit_expression(call))
            .is_some_and(|call| call.receiver().is_some());
        let receiver =
            callable.and_then(|item| self.loan_receiver_region_expected(call, item, state));
        self.iteration_actions.enter_region(
            self.checker.unit_expression(call),
            callable,
            expected,
            receiver_protocol,
            receiver,
        )
    }

    pub(super) fn loan_region_index(&self) -> &AdmissionIndex {
        self.loan_region_index.get_or_init(|| {
            let mut calls = BTreeMap::new();
            for &item in self.checker.parsed.roots() {
                let Ok(node) = self.checker.parsed.ast().items().get(item) else {
                    continue;
                };
                let Item::Function { name, form, .. } = node.payload() else {
                    continue;
                };
                if self.checker.receiver_context(*name).is_some() {
                    continue;
                }
                let expressions = match form {
                    FunctionForm::Explicit {
                        body: FunctionBody::Expression { expression, .. },
                        ..
                    } => Some(vec![*expression]),
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    } => self
                        .checker
                        .parsed
                        .ast()
                        .statements()
                        .get(*body)
                        .ok()
                        .and_then(|node| {
                            let Statement::Block { elements } = node.payload() else {
                                return None;
                            };
                            elements
                                .iter()
                                .map(|id| {
                                    let node =
                                        self.checker.parsed.ast().statements().get(*id).ok()?;
                                    if let Statement::Expression { expression } = node.payload() {
                                        Some(*expression)
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Option<Vec<_>>>()
                        }),
                    _ => None,
                };
                let Some(expressions) = expressions else {
                    continue;
                };
                if expressions.iter().all(|id| {
                    self.checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(*id)
                        .is_ok_and(|node| matches!(node.payload(), Expression::Call { .. }))
                }) {
                    for id in expressions {
                        calls.insert(id.index(), UnitItemId::new(self.checker.source_unit, item));
                    }
                }
            }
            let mut loans = BTreeMap::<_, Vec<_>>::new();
            for (index, loan) in self.checker.loans.iter().enumerate() {
                loans.entry(loan.call()).or_default().push(index);
            }
            let mut receivers = BTreeMap::<_, Vec<_>>::new();
            for (index, receiver) in self.checker.receiver_facts.iter().enumerate() {
                receivers.entry(receiver.call()).or_default().push(index);
            }
            AdmissionIndex {
                receivers,
                callables: calls,
                loans,
            }
        })
    }

    fn loan_region_expected(
        &self,
        expression: ExpressionId,
        state: &ValueState,
    ) -> Option<Vec<Expected>> {
        if !self.loop_boundaries.is_empty()
            || !state.iterations.is_empty()
            || !state.pending_loans.is_empty()
            || !state.pending_temporaries.is_empty()
            || !state.pending_borrows.is_empty()
            || !state.pending_capture_borrows.is_empty()
            || !state.pending_borrow_results.is_empty()
        {
            return None;
        }
        let id = self.checker.unit_expression(expression);
        let call = self.checker.typed.call(id)?;
        if !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.result_source() != CallableResultSource::Owned
            || call.aborts()
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.checker.typed.types().get(call.return_type()),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
        {
            return None;
        }
        let Expression::Call {
            callee, arguments, ..
        } = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .payload()
        else {
            return None;
        };
        if !matches!(
            self.checker
                .parsed
                .ast()
                .expressions()
                .get(*callee)
                .ok()?
                .payload(),
            Expression::Name
        ) || arguments.len() != call.arguments().len()
        {
            return None;
        }
        let contracts = self
            .checker
            .contracts_by_call
            .get(&id)
            .map_or(&[][..], Vec::as_slice);
        if contracts.len() != arguments.len() {
            return None;
        }
        let loans = self
            .loan_region_index()
            .loans
            .get(&id)
            .map_or(&[][..], Vec::as_slice);
        if loans.len() != arguments.len() {
            return None;
        }
        let mut expected = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let typed = call.arguments()[index];
            let contract = contracts[index];
            let argument_id = self.checker.unit_expression(argument.value);
            let node = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(argument.value)
                .ok()?;
            let Expression::String { parts, .. } = node.payload() else {
                return None;
            };
            if !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                || typed.argument_index() != index
                || typed.mode() != ParameterMode::Borrow
                || typed.crosses_thread()
                || contract.crosses_thread()
                || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                || contract.argument() != argument_id
                || contract.call() != id
                || contract.parameter_index() != typed.parameter_index()
                || contract.parameter_type() != typed.parameter_type()
                || self.checker.typed.expression_type(argument_id) != Some(typed.parameter_type())
                || !matches!(
                    self.checker.typed.types().get(typed.parameter_type()),
                    Some(UnitTypeKind::Builtin(BuiltinType::String))
                )
            {
                return None;
            }
            let matches: Vec<_> = loans
                .iter()
                .map(|&index| &self.checker.loans[index])
                .filter(|loan| loan.argument() == argument_id)
                .collect();
            let [loan] = matches.as_slice() else {
                return None;
            };
            if loan.begin_span() != node.span()
                || loan.parameter_span() != contract.parameter_span()
                || loan.kind() != LoanKind::Shared
                || loan.target() != &UnitLoanTarget::Temporary(argument_id)
            {
                return None;
            }
            expected.push(Expected {
                loan: (*loan).clone(),
                origin: node.span(),
            });
        }
        Some(expected)
    }
}
