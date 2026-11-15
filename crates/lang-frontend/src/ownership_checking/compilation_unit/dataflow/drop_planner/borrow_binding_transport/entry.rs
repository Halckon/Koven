//! Feature-only linear entrance evidence. Existing observable/transport semantics are untouched.
use super::*;
use crate::{
    ownership_checking::{LoanKind, UnitLoanTarget},
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement},
    type_checking::{BuiltinType, ParameterMode, UnitExpressionId, UnitItemId, UnitTypeKind},
};
#[derive(Clone, Debug)]
pub(super) struct Observation {
    callable: UnitItemId,
    consumers: Vec<UnitExpressionId>,
    returned: Vec<(UnitLoanFact, Option<ActionHandle>)>,
    ambiguous: bool,
}
impl DropPlanner<'_, '_> {
    pub(super) fn entry_binding_candidate(
        &self,
        source: &UnitLoanFact,
        binding: UnitSymbolId,
    ) -> Option<Observation> {
        if !self.loop_boundaries.is_empty() || source.kind() != LoanKind::Shared {
            return None;
        }
        let UnitLoanTarget::Place(root) = source.target() else {
            return None;
        };
        if !root.is_root() {
            return None;
        }
        let call = self.checker.typed.call(source.call())?;
        if call.receiver().is_some()
            || call.aborts()
            || !call.instance().type_arguments().is_empty()
            || call.borrow_return()?.origin()
                != crate::type_checking::BorrowReturnOrigin::Parameter(0)
            || call.arguments().len() != 2
            || self.checker.typed.types().get(call.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            return None;
        }
        for (index, argument) in call.arguments().iter().enumerate() {
            let kind = if index == 0 {
                BuiltinType::String
            } else {
                BuiltinType::Boolean
            };
            if argument.argument_index() != index
                || argument.parameter_index() != index
                || argument.mode() != ParameterMode::Borrow
                || argument.crosses_thread()
                || argument.category() != crate::type_checking::ExpressionCategory::Place
                || self.checker.typed.types().get(argument.parameter_type())
                    != Some(&UnitTypeKind::Builtin(kind))
            {
                return None;
            }
        }
        let ast = self.checker.parsed.ast();
        self.checker.parsed.roots().iter().find_map(|&item| {
            let Item::Function {
                name,
                parameters,
                type_parameters,
                extension_receiver: None,
                form,
                ..
            } = ast.items().get(item).ok()?.payload()
            else {
                return None;
            };
            if parameters.len() != 1 || !type_parameters.is_empty() {
                return None;
            }
            let symbol = *self.checker.marker_symbol(*name)?;
            let signature = self
                .checker
                .typed
                .signatures()
                .declarations()
                .iter()
                .find(|d| d.symbol() == symbol)?
                .callable()?;
            let [parameter] = signature.parameters() else {
                return None;
            };
            let flag = parameter.symbol()?;
            if parameter.mode() != ParameterMode::Borrow
                || signature.receiver().is_some()
                || self.checker.typed.types().get(parameter.ty())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                || self.checker.typed.types().get(signature.return_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            {
                return None;
            }
            let body = match form {
                FunctionForm::ImplicitUnitBlock(b)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(b),
                    ..
                } => *b,
                _ => return None,
            };
            let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
                return None;
            };
            let [owner, bind, consumers @ ..] = elements.as_slice() else {
                return None;
            };
            if !matches!(consumers.len(), 1 | 2) {
                return None;
            }
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*owner).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: crate::parser::VariableKind::Val,
                name,
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if self.checker.marker_symbol(*name).copied() != Some(root.root()) {
                return None;
            }
            let Expression::String { parts } = ast.expressions().get(*initializer).ok()?.payload()
            else {
                return None;
            };
            if !parts
                .iter()
                .all(|p| matches!(p, crate::parser::StringPart::Text(_)))
            {
                return None;
            }
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*bind).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: crate::parser::VariableKind::BorrowVal(_),
                name,
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if self.checker.marker_symbol(*name).copied() != Some(binding)
                || self.checker.unit_expression(*initializer) != source.call()
            {
                return None;
            }
            let Expression::Call { arguments, .. } =
                ast.expressions().get(*initializer).ok()?.payload()
            else {
                return None;
            };
            let [first, second] = arguments.as_slice() else {
                return None;
            };
            if self.checker.unit_expression(first.value) != source.argument()
                || !matches!(
                    ast.expressions().get(first.value).ok()?.payload(),
                    Expression::Name
                )
            {
                return None;
            }
            let place = self.checker.place(second.value).ok()??;
            if !place.is_root()
                || place.root() != flag
                || !matches!(
                    ast.expressions().get(second.value).ok()?.payload(),
                    Expression::Name
                )
            {
                return None;
            }
            let consumers = consumers
                .iter()
                .map(|consumer| {
                    let Statement::Expression { expression } =
                        ast.statements().get(*consumer).ok()?.payload()
                    else {
                        return None;
                    };
                    let Expression::Call { arguments, .. } =
                        ast.expressions().get(*expression).ok()?.payload()
                    else {
                        return None;
                    };
                    let [argument] = arguments.as_slice() else {
                        return None;
                    };
                    if !matches!(
                        ast.expressions().get(argument.value).ok()?.payload(),
                        Expression::Name
                    ) {
                        return None;
                    }
                    let place = self.checker.place(argument.value).ok()??;
                    if !place.is_root() || place.root() != binding {
                        return None;
                    }
                    Some(self.checker.unit_expression(*expression))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Observation {
                callable: UnitItemId::new(self.checker.source_unit, item),
                consumers,
                returned: Vec::new(),
                ambiguous: false,
            })
        })
    }
    pub(super) fn observe_entry_binding_use(
        &self,
        loan: &UnitLoanFact,
        acquisition: Option<&ActionHandle>,
        pending: &mut [PendingBindingTransport],
    ) {
        let Ok(Some(place)) = self.checker.place(loan.argument().expression()) else {
            return;
        };
        let single = pending.len() == 1;
        for p in pending.iter_mut().filter(|p| p.binding == place.root()) {
            let Some(entry) = &mut p.entry else {
                continue;
            };
            let valid = single
                && place.is_root()
                && entry.consumers.get(entry.returned.len()) == Some(&loan.call())
                && loan.target() == p.source.target()
                && self.checker.typed.call(loan.call()).is_some_and(|call| {
                    call.result_source() == CallableResultSource::Owned
                        && self.checker.typed.types().get(call.return_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                })
                && self
                    .consumer_region_loan(
                        loan.call().expression(),
                        loan.argument().expression(),
                        crate::type_checking::ExpressionCategory::Place,
                    )
                    .is_some_and(|(checked, _)| checked == *loan);
            if !valid || entry.ambiguous {
                entry.ambiguous = true;
            } else {
                entry.returned.push((loan.clone(), acquisition.cloned()));
            }
        }
    }
}
impl PendingBindingTransport {
    pub(super) fn publish_entry_retirement(
        &self,
        actions: &mut super::super::action_source::ActionSource,
        transaction: &UnitBorrowBindingTransportFact,
        end: &ActionHandle,
    ) {
        if let Some(entry) = &self.entry {
            actions.entry_binding_retired(super::super::action_source::entry_binding::Retiring {
                callable: entry.callable,
                consumers: &entry.consumers,
                transaction,
                source: self.source_begin.as_ref(),
                handoff: self.handoff.as_ref(),
                returned: &entry.returned,
                ambiguous_use: entry.ambiguous,
                end,
            });
        }
    }
}
