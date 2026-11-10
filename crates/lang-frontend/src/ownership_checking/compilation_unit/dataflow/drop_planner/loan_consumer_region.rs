//! Protocol 4 admission from syntax, typed contracts and current local-owner state.
use super::*;
use crate::name_resolution::SymbolKind;
use crate::ownership_checking::compilation_unit::loan_region_validation::ExpectedConsumer;
use crate::ownership_checking::{
    LoanKind, OwnershipBindingKind, UnitDropPoint, UnitLoanFact, UnitLoanTarget,
};
use crate::parser::{NameMarker, VariableKind};
use crate::source::Span;
use crate::type_checking::{BorrowReturnOrigin, CallableResultSource, UnitCallTarget, UnitItemId};

impl DropPlanner<'_, '_> {
    pub(super) fn is_consumer_loan_region(&self, call: ExpressionId) -> bool {
        self.consumer_region_operands(call).is_some()
    }

    fn consumer_region_operands(&self, call: ExpressionId) -> Option<(ExpressionId, ExpressionId)> {
        let source = self.consumer_region_operand(call)?;
        let argument = self.consumer_region_operand(source)?;
        matches!(
            self.checker
                .parsed
                .ast()
                .expressions()
                .get(argument)
                .ok()?
                .payload(),
            Expression::Name
        )
        .then_some((source, argument))
    }

    fn consumer_region_operand(&self, call: ExpressionId) -> Option<ExpressionId> {
        let ast = self.checker.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = ast.get(call).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        (argument.mode_marker.is_none()
            && matches!(ast.get(*callee).ok()?.payload(), Expression::Name))
        .then_some(argument.value)
    }

    pub(super) fn loan_consumer_region_expected(
        &self,
        call: ExpressionId,
        state: &ValueState,
    ) -> Option<(UnitItemId, ExpectedConsumer)> {
        if !self.consumer_region_state_is_flat(state) {
            return None;
        }
        let (source_call, argument) = self.consumer_region_operands(call)?;
        let place = self.checker.place(argument).ok()??;
        let root = place.root();
        let provenance = self.loan_provenance();
        if !place.is_root()
            || root.source_unit() != self.checker.source_unit
            || self.checker.symbol_kind(root) != Some(SymbolKind::Variable)
            || !provenance.formed_owned.contains(&root)
            || provenance.iteration_providers.contains(&root)
            || provenance.borrow_bindings.contains(&root)
            // Ordinary locals have no parameter binding descriptor. Their owned
            // capability requires the declaration, formation and current value below.
            || self.checker.bindings.get(&root).is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
        {
            return None;
        }
        let (callable, owner_origin, formation_span) = self.consumer_region_caller(call, root)?;
        let [owner] = state.values.as_slice() else {
            return None;
        };
        if owner.symbol != root
            || owner.origin != owner_origin
            || owner.declaration != owner_origin
            || owner.scope_depth != self.scope_depth
        {
            return None;
        }
        let (source, source_parameter) =
            self.consumer_region_loan(source_call, argument, ExpressionCategory::Place)?;
        let (consumer, _) =
            self.consumer_region_loan(call, source_call, ExpressionCategory::Temporary)?;
        let inner = self.checker.typed.call(source.call())?;
        let outer = self.checker.typed.call(consumer.call())?;
        if inner.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(source_parameter)
            || self.checker.typed.types().get(inner.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || outer.result_source() != CallableResultSource::Owned
            || !matches!(
                self.checker.typed.types().get(outer.return_type()),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
            || source.target() != &UnitLoanTarget::Place(place)
            || consumer.target() != source.target()
        {
            return None;
        }
        let mut uses = self
            .checker
            .borrow_results
            .call_uses()
            .iter()
            .filter(|fact| fact.expression() == source.call() || fact.call() == consumer.call());
        let use_fact = uses.next()?;
        if uses.next().is_some()
            || use_fact.expression() != source.call()
            || use_fact.call() != consumer.call()
            || use_fact.argument() != source.call()
            || use_fact.source_loan().call() != source.call()
            || use_fact.source_loan().argument() != source.argument()
            || use_fact.origin() != source.target()
            || use_fact.parent().is_some()
            || use_fact.end_point() != UnitDropPoint::CallReturn(consumer.call())
        {
            return None;
        }
        Some((
            callable,
            ExpectedConsumer {
                source_origin: source.begin_span(),
                source_call_origin: source.end_span(),
                consumer_call_origin: consumer.end_span(),
                source,
                consumer,
                root,
                owner_origin,
                formation_span,
            },
        ))
    }

    fn consumer_region_state_is_flat(&self, state: &ValueState) -> bool {
        // Explicitly reject every currently represented replacement/control frame.
        // A future owner replace-consumer frame must join this gate when integrated.
        self.scope_depth == 1
            && self.loop_boundaries.is_empty()
            && self.owner_choice.is_none()
            && self.owner_loop.is_none()
            && self.owner_branch_depth == 0
            && self.call_return_copy_source.is_none()
            && self.pending_borrow_handoffs.is_empty()
            && state.lambda.is_none()
            && state.this.is_none()
            && state.iterations.is_empty()
            && state.replacements.is_empty()
            && state.borrow_bindings.is_empty()
            && state.pending_borrow_results.is_empty()
            && state.borrow_initializer_sources.is_empty()
            && state.pending_loans.is_empty()
            && state.borrow_call_transports.is_empty()
            && state.borrow_binding_transports.is_empty()
            && state.borrow_forward_transports.is_empty()
            && state.pending_temporaries.is_empty()
            && state.pending_borrows.is_empty()
            && state.pending_capture_borrows.is_empty()
            && state.closures.is_empty()
            && state.closure_asap.is_empty()
    }

    fn consumer_region_loan(
        &self,
        expression: ExpressionId,
        argument: ExpressionId,
        category: ExpressionCategory,
    ) -> Option<(UnitLoanFact, usize)> {
        if self.consumer_region_operand(expression) != Some(argument) {
            return None;
        }
        let id = self.checker.unit_expression(expression);
        let argument_id = self.checker.unit_expression(argument);
        let call = self.checker.typed.call(id)?;
        let UnitCallTarget::Declaration(declaration) = call.target() else {
            return None;
        };
        let signature = self
            .checker
            .typed
            .signatures()
            .declaration(declaration)?
            .callable()?;
        let [parameter] = signature.parameters() else {
            return None;
        };
        let [typed] = call.arguments() else {
            return None;
        };
        let [contract] = self.checker.contracts_by_call.get(&id)?.as_slice() else {
            return None;
        };
        let [index] = self.loan_region_index().loans.get(&id)?.as_slice() else {
            return None;
        };
        let loan = &self.checker.loans[*index];
        let ast = self.checker.parsed.ast().expressions();
        let origin = ast.get(argument).ok()?.span();
        let call_origin = ast.get(expression).ok()?.span();
        if call.expression() != id
            || call.receiver().is_some()
            || call.aborts()
            || !call.instance().type_arguments().is_empty()
            || !signature.type_parameters().is_empty()
            || signature.receiver().is_some()
            || signature.return_type() != call.return_type()
            || signature.result_source() != call.result_source()
            || self.loan_region_index().receivers.contains_key(&id)
            || typed.argument_index() != 0
            || typed.parameter_index() != 0
            || typed.mode() != ParameterMode::Borrow
            || typed.crosses_thread()
            || parameter.mode() != typed.mode()
            || parameter.ty() != typed.parameter_type()
            || typed.category() != category
            || contract.category() != category
            || self.checker.typed.expression_category(argument_id) != Some(category)
            || self.checker.typed.expression_type(id) != Some(call.return_type())
            || self.checker.typed.expression_type(argument_id) != Some(typed.parameter_type())
            || self.checker.typed.types().get(typed.parameter_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || contract.call() != id
            || contract.argument() != argument_id
            || contract.parameter_index() != typed.parameter_index()
            || contract.parameter_type() != typed.parameter_type()
            || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
            || contract.crosses_thread()
            || contract.argument_span() != origin
            || contract.loan_begin_span() != origin
            || contract.call_span() != call_origin
            || contract.parameter_span() != Some(parameter.span())
            || loan.call() != id
            || loan.argument() != argument_id
            || loan.kind() != LoanKind::Shared
            || loan.begin_span() != origin
            || loan.end_span() != call_origin
            || loan.parameter_span() != contract.parameter_span()
        {
            return None;
        }
        Some((loan.clone(), typed.parameter_index()))
    }

    fn consumer_region_caller(
        &self,
        call: ExpressionId,
        root: UnitSymbolId,
    ) -> Option<(UnitItemId, Span, Span)> {
        let ast = self.checker.parsed.ast();
        self.checker.parsed.roots().iter().find_map(|&item| {
            let Item::Function {
                name,
                extension_receiver: None,
                type_parameters,
                form,
                ..
            } = ast.items().get(item).ok()?.payload()
            else {
                return None;
            };
            if !type_parameters.is_empty() || self.checker.receiver_context(*name).is_some() {
                return None;
            }
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => return None,
            };
            let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
                return None;
            };
            let (first, tail) = elements.split_first()?;
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*first).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: VariableKind::Val,
                name: NameMarker::Present(owner_origin),
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if self
                .checker
                .marker_symbol(NameMarker::Present(*owner_origin))
                .copied()
                != Some(root)
            {
                return None;
            }
            let formation = ast.expressions().get(*initializer).ok()?;
            let Expression::String { parts } = formation.payload() else {
                return None;
            };
            let literal = self.checker.unit_expression(*initializer);
            let ty = self.checker.typed.symbol_type(root)?;
            if !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                || self.checker.typed.types().get(ty)
                    != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                || self.checker.typed.expression_type(literal) != Some(ty)
                || self.checker.typed.expression_category(literal)
                    != Some(ExpressionCategory::Temporary)
                || !self.checker.is_move_only_variable(root)
            {
                return None;
            }
            let mut found = 0;
            for statement in tail {
                let Statement::Expression { expression } =
                    ast.statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                if *expression == call {
                    found += 1;
                } else if !self.consumer_region_plain_call(*expression) {
                    return None;
                }
            }
            (found == 1).then_some((
                UnitItemId::new(self.checker.source_unit, item),
                *owner_origin,
                formation.span(),
            ))
        })
    }

    fn consumer_region_plain_call(&self, expression: ExpressionId) -> bool {
        let ast = self.checker.parsed.ast().expressions();
        let Ok(node) = ast.get(expression) else {
            return false;
        };
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            return false;
        };
        let Some(call) = self
            .checker
            .typed
            .call(self.checker.unit_expression(expression))
        else {
            return false;
        };
        ast.get(*callee)
            .is_ok_and(|node| matches!(node.payload(), Expression::Name))
            && matches!(call.target(), UnitCallTarget::Declaration(_))
            && call.receiver().is_none()
            && !call.aborts()
            && call.instance().type_arguments().is_empty()
            && call.result_source() == CallableResultSource::Owned
            && arguments.iter().all(|argument| {
                ast.get(argument.value)
                    .is_ok_and(|node| match node.payload() {
                        Expression::Name | Expression::Literal(_) => true,
                        Expression::String { parts } => {
                            parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                        }
                        _ => false,
                    })
            })
    }
}
