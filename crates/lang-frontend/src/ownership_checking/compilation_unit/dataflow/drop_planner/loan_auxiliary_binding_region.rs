//! Exact three-statement syntax and independent typed/current-state admission.
#[path = "loan_distinct_auxiliary_binding_region.rs"]
mod distinct;
use super::*;
use crate::{
    name_resolution::SymbolKind,
    ownership_checking::compilation_unit::loan_auxiliary_binding_region_validation::Expected,
    ownership_checking::compilation_unit::loan_binding_region_validation::{OwnerSnapshot, Scope},
    ownership_checking::{
        BorrowBindingStorage, OwnershipBindingKind, UnitDropPoint, UnitLoanTarget,
    },
    ownership_checking::{LoanKind, UnitLoanFact},
    parser::{NameMarker, VariableKind},
    source::Span,
    type_checking::{BorrowReturnOrigin, CallableResultSource, UnitCallTarget, UnitItemId},
};

struct Shape {
    scope: Scope,
    owner_initializer: ExpressionId,
    owner_origin: Span,
    source_argument: ExpressionId,
    source_index: usize,
    auxiliary_argument: ExpressionId,
    consumer_argument: ExpressionId,
}

impl DropPlanner<'_, '_> {
    pub(super) fn enter_auxiliary_binding_region(
        &mut self,
        statement: StatementId,
        state: &ValueState,
    ) {
        self.enter_distinct_auxiliary_binding_region(statement, state);
        if !self.iteration_actions.enabled {
            return;
        }
        let Some(shape) = self.auxiliary_binding_region_shape(statement) else {
            return;
        };
        let expected = self.auxiliary_binding_region_expected(&shape, state);
        self.iteration_actions
            .enter_auxiliary_binding_region(shape.scope, expected);
    }

    pub(super) fn finish_auxiliary_binding_region_statement(
        &mut self,
        statement: StatementId,
        continues: bool,
        state: &ValueState,
    ) {
        self.finish_distinct_auxiliary_binding_region_statement(statement, continues, state);
        // Completion follows the enclosing block's drop_scope and its borrow-end wrapper.
        // Empty active state also proves the following item-level drop_all is inert.
        let empty = self.scope_depth == 0
            && self.binding_region_boundary_clear(state)
            && state.values.is_empty();
        self.iteration_actions
            .finish_auxiliary_binding_region_statement(
                UnitStatementId::new(self.checker.source_unit, statement),
                continues,
                empty,
            );
    }

    fn auxiliary_binding_region_shape(&self, entry: StatementId) -> Option<Shape> {
        let ast = self.checker.parsed.ast();
        let Statement::LocalVariable { declaration } = ast.statements().get(entry).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            kind: VariableKind::BorrowVal(_),
            name: NameMarker::Present(binding_origin),
            initializer: source_call,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        let binding = *self
            .checker
            .marker_symbol(NameMarker::Present(*binding_origin))?;
        let source_index = match self
            .checker
            .typed
            .call(self.checker.unit_expression(*source_call))?
            .borrow_return()?
            .origin()
        {
            BorrowReturnOrigin::Parameter(index @ (0 | 1)) => index,
            _ => return None,
        };
        let source_argument =
            self.auxiliary_binding_region_operand(*source_call, source_index, 2)?;
        let auxiliary_argument =
            self.auxiliary_binding_region_operand(*source_call, 1 - source_index, 2)?;
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
            if !parameters.is_empty()
                || !type_parameters.is_empty()
                || self.checker.receiver_context(*name).is_some()
            {
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
            let [owner_statement, binding_statement, consumer_statement] = elements.as_slice()
            else {
                return None;
            };
            if *binding_statement != entry {
                return None;
            }
            let function = *self.checker.marker_symbol(*name)?;
            let caller = self
                .checker
                .typed
                .signatures()
                .declarations()
                .iter()
                .find(|declaration| declaration.symbol() == function)?
                .callable()?;
            if !caller.parameters().is_empty()
                || !caller.type_parameters().is_empty()
                || caller.receiver().is_some()
                || caller.result_source() != CallableResultSource::Owned
                || self.checker.typed.types().get(caller.return_type())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            {
                return None;
            }
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*owner_statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: VariableKind::Val,
                name: NameMarker::Present(owner_origin),
                initializer: owner_initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            let Expression::String { parts } =
                ast.expressions().get(*owner_initializer).ok()?.payload()
            else {
                return None;
            };
            if !parts.iter().all(|part| matches!(part, StringPart::Text(_))) {
                return None;
            }
            let root = *self
                .checker
                .marker_symbol(NameMarker::Present(*owner_origin))?;
            let Statement::Expression {
                expression: consumer_call,
            } = ast.statements().get(*consumer_statement).ok()?.payload()
            else {
                return None;
            };
            let consumer_argument = self.auxiliary_binding_region_operand(*consumer_call, 0, 1)?;
            let source_place = self.checker.place(source_argument).ok()??;
            let consumer_place = self.checker.place(consumer_argument).ok()??;
            let auxiliary_place = self.checker.place(auxiliary_argument).ok()??;
            if auxiliary_place != source_place
                || auxiliary_argument == source_argument
                || !source_place.is_root()
                || source_place.root() != root
                || !consumer_place.is_root()
                || consumer_place.root() != binding
                || binding == root
            {
                return None;
            }
            let statement = |id| UnitStatementId::new(self.checker.source_unit, id);
            Some(Shape {
                scope: Scope {
                    callable: UnitItemId::new(self.checker.source_unit, item),
                    binding,
                    root,
                    source_call: self.checker.unit_expression(*source_call),
                    consumer_call: self.checker.unit_expression(*consumer_call),
                    entry: statement(entry),
                    completion: statement(body),
                    consumer_statement: statement(*consumer_statement),
                },
                owner_initializer: *owner_initializer,
                owner_origin: *owner_origin,
                source_argument,
                source_index,
                auxiliary_argument,
                consumer_argument,
            })
        })
    }

    fn auxiliary_binding_region_operand(
        &self,
        expression: ExpressionId,
        index: usize,
        count: usize,
    ) -> Option<ExpressionId> {
        let ast = self.checker.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = ast.get(expression).ok()?.payload()
        else {
            return None;
        };
        if arguments.len() != count {
            return None;
        }
        let argument = arguments.get(index)?;
        (argument.mode_marker.is_none()
            && matches!(ast.get(*callee).ok()?.payload(), Expression::Name)
            && matches!(ast.get(argument.value).ok()?.payload(), Expression::Name))
        .then_some(argument.value)
    }

    fn auxiliary_binding_region_expected(
        &self,
        shape: &Shape,
        state: &ValueState,
    ) -> Option<Expected> {
        if self.scope_depth != 1 || !self.binding_region_boundary_clear(state) {
            return None;
        }
        let scope = &shape.scope;
        let [owner] = state.values.as_slice() else {
            return None;
        };
        let provenance = self.loan_provenance();
        if owner.owner.is_none()
            || owner.symbol != scope.root
            || owner.origin != shape.owner_origin
            || owner.declaration != shape.owner_origin
            || owner.scope_depth != 1
            || self.checker.symbol_kind(scope.root) != Some(SymbolKind::Variable)
            || !self.checker.is_move_only_variable(scope.root)
            || !provenance.formed_owned.contains(&scope.root)
            || provenance.iteration_providers.contains(&scope.root)
            || provenance.borrow_bindings.contains(&scope.root)
            || self
                .checker
                .bindings
                .get(&scope.root)
                .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
        {
            return None;
        }
        let literal = self.checker.unit_expression(shape.owner_initializer);
        let ty = self.checker.typed.symbol_type(scope.root)?;
        if self.checker.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || self.checker.typed.expression_type(literal) != Some(ty)
            || self.checker.typed.expression_category(literal)
                != Some(ExpressionCategory::Temporary)
            || self.checker.typed.symbol_type(scope.binding) != Some(ty)
        {
            return None;
        }
        let (source, parameter) = self.auxiliary_region_loan(
            scope.source_call.expression(),
            shape.source_argument,
            shape.source_index,
        )?;
        let (auxiliary, _) = self.auxiliary_region_loan(
            scope.source_call.expression(),
            shape.auxiliary_argument,
            1 - shape.source_index,
        )?;
        let (consumer, _) = self.consumer_region_loan(
            scope.consumer_call.expression(),
            shape.consumer_argument,
            ExpressionCategory::Place,
        )?;
        let source_call = self.checker.typed.call(scope.source_call)?;
        let consumer_call = self.checker.typed.call(scope.consumer_call)?;
        if source_call.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(parameter)
            || source_call.return_type() != ty
            || consumer_call.result_source() != CallableResultSource::Owned
            || self.checker.typed.types().get(consumer_call.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            || !matches!(source.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == scope.root)
            || consumer.target() != source.target()
        {
            return None;
        }
        let mut bindings = self.borrow_bindings_initialized_by(scope.source_call);
        let binding = bindings.next()?;
        let source_loan = binding.source_loan()?;
        if bindings.next().is_some()
            || binding.binding() != scope.binding
            || binding.parent().is_some()
            || binding.storage() != BorrowBindingStorage::BorrowedStorage
            || binding.initializer() != scope.source_call
            || binding.origin() != source.target()
            || source_loan.call() != source.call()
            || source_loan.argument() != source.argument()
        {
            return None;
        }
        let mut ends = self
            .checker
            .borrow_results
            .ends
            .iter()
            .filter(|end| end.binding == scope.binding);
        let end = ends.next()?;
        if ends.next().is_some()
            || end.point != UnitDropPoint::AfterStatement(scope.consumer_statement)
        {
            return None;
        }
        Some(Expected {
            source_index: shape.source_index,
            source,
            auxiliary,
            consumer,
            owner: OwnerSnapshot {
                owner: owner.owner.clone(),
                symbol: owner.symbol,
                origin: owner.origin,
                declaration: owner.declaration,
                scope_depth: owner.scope_depth,
            },
        })
    }
    pub(super) fn auxiliary_region_loan(
        &self,
        expression: ExpressionId,
        argument: ExpressionId,
        index: usize,
    ) -> Option<(UnitLoanFact, usize)> {
        if self.auxiliary_binding_region_operand(expression, index, 2) != Some(argument) {
            return None;
        }
        let category = ExpressionCategory::Place;
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
        if signature.parameters().len() != 2 || call.arguments().len() != 2 {
            return None;
        }
        let parameter = signature.parameters().get(index)?;
        let typed = call.arguments().get(index)?;
        let contracts = self.checker.contracts_by_call.get(&id)?;
        let indices = self.loan_region_index().loans.get(&id)?;
        if contracts.len() != 2 || indices.len() != 2 {
            return None;
        }
        let mut matching = contracts
            .iter()
            .filter(|contract| contract.argument() == argument_id);
        let contract = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        let mut matching = indices
            .iter()
            .map(|i| &self.checker.loans[*i])
            .filter(|loan| loan.argument() == argument_id);
        let loan = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
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
            || typed.argument_index() != index
            || typed.parameter_index() != index
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
}
