//! Strict declaration admission for a loop-carried binding; no executed-edge evidence.
use super::*;
use crate::{
    ownership_checking::{BorrowBindingStorage, UnitLoanTarget},
    parser::{NameMarker, VariableKind},
    type_checking::{BorrowReturnOrigin, CallableResultSource, UnitCallTarget},
};

impl DropPlanner<'_, '_> {
    pub(super) fn loop_invariant_plan(
        &self,
        id: StatementId,
        condition: ExpressionId,
        body: StatementId,
        state: &ValueState,
    ) -> Option<action_source::LoopInvariantPlan> {
        if !self.iteration_actions.enabled
            || self.scope_depth != 1
            || !self.loop_boundaries.is_empty()
            || state.lambda.is_some()
            || state.this.is_some()
            || self.owner_branch_depth != 0
            || !state.iterations.is_empty()
            || self.checker.is_constant_use(condition)
        {
            return None;
        }
        let ast = self.checker.parsed.ast();
        if !matches!(ast.statements().get(id).ok()?.payload(),Statement::While{condition:c,body:b,..} if *c==condition && *b==body)
            || !matches!(
                ast.expressions().get(condition).ok()?.payload(),
                Expression::Name
            )
            || self
                .checker
                .typed
                .expression_type(self.checker.unit_expression(condition))
                != Some(
                    self.checker
                        .typed
                        .symbol_type(self.checker.place(condition).ok()??.root())?,
                )
        {
            return None;
        }
        let condition_type = self
            .checker
            .typed
            .expression_type(self.checker.unit_expression(condition))?;
        if self.checker.typed.types().get(condition_type)
            != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
        {
            return None;
        }

        self.checker.parsed.roots().iter().find_map(|&item| {
            let Item::Function {
                name,
                type_parameters,
                extension_receiver: None,
                form,
                ..
            } = ast.items().get(item).ok()?.payload()
            else {
                return None;
            };
            if !type_parameters.is_empty() || self.checker.receiver_context(*name).is_some() {
                return None;
            }
            let completion = match form {
                FunctionForm::ImplicitUnitBlock(block)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(block),
                    ..
                } => *block,
                _ => return None,
            };
            let Statement::Block { elements } = ast.statements().get(completion).ok()?.payload()
            else {
                return None;
            };
            let [owner_statement, declaration_statement, loop_statement] = elements.as_slice()
            else {
                return None;
            };
            if *loop_statement != id {
                return None;
            }
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*declaration_statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: VariableKind::BorrowVal(_),
                name: NameMarker::Present(binding_marker),
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            let binding = *self
                .checker
                .marker_symbol(NameMarker::Present(*binding_marker))?;
            if state.borrow_bindings.as_slice() != [binding] {
                return None;
            }

            let argument = self.loop_invariant_operand(*initializer)?;
            let source_place = self.checker.place(argument).ok()??;
            let Statement::LocalVariable {
                declaration: owner_declaration,
            } = ast.statements().get(*owner_statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: VariableKind::Val,
                name: NameMarker::Present(owner_marker),
                initializer: owner_initializer,
                ..
            } = ast.items().get(*owner_declaration).ok()?.payload()
            else {
                return None;
            };
            let Expression::String { parts } =
                ast.expressions().get(*owner_initializer).ok()?.payload()
            else {
                return None;
            };
            let root = *self
                .checker
                .marker_symbol(NameMarker::Present(*owner_marker))?;
            if !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                || !source_place.is_root()
                || source_place.root() != root
                || root == binding
            {
                return None;
            }

            let mut owners = state.values.iter().filter(|owner| owner.symbol == root);
            let owner = owners.next()?;

            let owner_id = owner.owner.as_ref()?;

            let observation = self.owner_observation.as_ref()?;

            if owners.next().is_some()
                || owner.origin != *owner_marker
                || owner.declaration != *owner_marker
                || owner.scope_depth != 1
                || owner_id.callable
                    != crate::ownership_checking::UnitOwnerCallable::Function(
                        crate::type_checking::UnitItemId::new(self.checker.source_unit, item),
                    )
                || observation.callable != owner_id.callable
                || !std::sync::Arc::ptr_eq(&owner_id.identity, &observation.identity)
                || !self.loan_provenance().formed_owned.contains(&root)
                || self
                    .checker
                    .typed
                    .expression_type(self.checker.unit_expression(*owner_initializer))
                    != self.checker.typed.symbol_type(root)
                || self
                    .checker
                    .typed
                    .expression_category(self.checker.unit_expression(*owner_initializer))
                    != Some(ExpressionCategory::Temporary)
            {
                return None;
            }

            let source_call = self.checker.unit_expression(*initializer);
            let call = self.checker.typed.call(source_call)?;
            let UnitCallTarget::Declaration(target) = call.target() else {
                return None;
            };
            let signature = self
                .checker
                .typed
                .signatures()
                .declaration(target)?
                .callable()?;
            if call.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
                || signature.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
                || call.return_type() != self.checker.typed.symbol_type(source_place.root())?
                || self.checker.typed.symbol_type(binding) != Some(call.return_type())
            {
                return None;
            }

            let (source, _) =
                self.consumer_region_loan(*initializer, argument, ExpressionCategory::Place)?;
            if !matches!(source.target(),UnitLoanTarget::Place(place) if place==&source_place) {
                return None;
            }

            let mut matches = self.borrow_bindings_initialized_by(source_call);
            let fact = matches.next()?;
            let source_fact = fact.source_loan()?;
            if matches.next().is_some()
                || fact.binding() != binding
                || fact.parent().is_some()
                || fact.storage() != BorrowBindingStorage::BorrowedStorage
                || fact.initializer() != source_call
                || fact.origin() != source.target()
                || source_fact.call() != source.call()
                || source_fact.argument() != source.argument()
            {
                return None;
            }

            // The actual binding end does not exist at loop entry. Its unique
            // outer-block completion is checked by the final lifecycle validator.
            let (Statement::Block { elements } | Statement::ControlBody { elements }) =
                ast.statements().get(body).ok()?.payload()
            else {
                return None;
            };
            if elements.is_empty() {
                return None;
            }

            let consumers = elements
                .iter()
                .map(|statement| {
                    let Statement::Expression { expression } =
                        ast.statements().get(*statement).ok()?.payload()
                    else {
                        return None;
                    };
                    let argument = self.loop_invariant_operand(*expression)?;
                    let place = self.checker.place(argument).ok()??;
                    if !place.is_root() || place.root() != binding {
                        return None;
                    }

                    let (loan, _) = self.consumer_region_loan(
                        *expression,
                        argument,
                        ExpressionCategory::Place,
                    )?;

                    let typed = self
                        .checker
                        .typed
                        .call(self.checker.unit_expression(*expression))?;
                    if loan.target() != source.target()
                        || typed.result_source() != CallableResultSource::Owned
                        || self.checker.typed.types().get(typed.return_type())
                            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                    {
                        return None;
                    }
                    Some((
                        self.checker.unit_expression(*expression),
                        self.checker.unit_expression(argument),
                    ))
                })
                .collect::<Option<Vec<_>>>()?;

            Some(action_source::LoopInvariantPlan {
                binding,
                statement: UnitStatementId::new(self.checker.source_unit, id),
                condition: self.checker.unit_expression(condition),
                body: UnitStatementId::new(self.checker.source_unit, body),
                completion: UnitStatementId::new(self.checker.source_unit, completion),
                consumers,
            })
        })
    }
    fn loop_invariant_operand(&self, call: ExpressionId) -> Option<ExpressionId> {
        let ast = self.checker.parsed.ast().expressions();
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = ast.get(call).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        if !type_arguments.is_empty()
            || argument.named_prefix.is_some()
            || argument.mode_marker.is_some()
            || !matches!(ast.get(*callee).ok()?.payload(), Expression::Name)
            || !matches!(ast.get(argument.value).ok()?.payload(), Expression::Name)
        {
            return None;
        }
        Some(argument.value)
    }
}
