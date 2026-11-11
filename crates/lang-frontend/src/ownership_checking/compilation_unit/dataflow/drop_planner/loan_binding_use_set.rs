//! Strict multi-binding-consumer declaration plan; not a complete region certificate.
use super::super::action_source::BindingUseSetExpected as Expected;
use super::*;
use crate::{
    name_resolution::SymbolKind,
    ownership_checking::compilation_unit::loan_binding_region_validation::{OwnerSnapshot, Scope},
    ownership_checking::compilation_unit::loan_later_auxiliary_binding_region_validation::MAX_AUXILIARY_SEQUENCE_USES,
    ownership_checking::{
        BorrowBindingStorage, OwnershipBindingKind, UnitDropPoint, UnitLoanTarget,
    },
    parser::{NameMarker, VariableKind},
    source::Span,
    type_checking::{BorrowReturnOrigin, CallableResultSource, UnitItemId},
};

struct Shape {
    scope: Scope,
    owners: [(UnitSymbolId, ExpressionId, Span); 2],
    source_argument: ExpressionId,
    source_index: usize,
    auxiliary_argument: ExpressionId,
    consumers: Vec<(ExpressionId, ExpressionId, UnitStatementId)>,
    later_uses: Vec<(ExpressionId, ExpressionId, UnitStatementId)>,
}

impl DropPlanner<'_, '_> {
    pub(super) fn enter_binding_use_set(&mut self, statement: StatementId, state: &ValueState) {
        if !self.iteration_actions.enabled {
            return;
        }
        let Some(shape) = self.binding_use_set_shape(statement) else {
            return;
        };
        if let Some(expected) = self.binding_use_set_expected(&shape, state) {
            self.iteration_actions.enter_binding_use_set(expected);
        }
    }

    fn binding_use_set_shape(&self, entry: StatementId) -> Option<Shape> {
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
        let source_argument = self.binding_use_set_operand(*source_call, source_index, 2)?;
        let auxiliary_argument = self.binding_use_set_operand(*source_call, 1 - source_index, 2)?;
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
            let [first_owner, second_owner, binding_statement, uses @ ..] = elements.as_slice()
            else {
                return None;
            };
            // At least two B uses and one A use; each family has its own finite budget.
            if !(3..=MAX_AUXILIARY_SEQUENCE_USES + 8).contains(&uses.len())
                || *binding_statement != entry
            {
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
            let owners = [
                self.use_set_literal(*first_owner)?,
                self.use_set_literal(*second_owner)?,
            ];
            if owners[0].0 == owners[1].0 {
                return None;
            }
            let source_place = self.checker.place(source_argument).ok()??;
            let auxiliary_place = self.checker.place(auxiliary_argument).ok()??;
            let root = source_place.root();
            if auxiliary_place == source_place
                || !auxiliary_place.is_root()
                || !owners.iter().any(|owner| owner.0 == root)
                || !owners.iter().any(|owner| owner.0 == auxiliary_place.root())
                || auxiliary_argument == source_argument
                || !source_place.is_root()
                || owners.iter().any(|owner| owner.0 == binding)
            {
                return None;
            }
            let statement = |id| UnitStatementId::new(self.checker.source_unit, id);
            let mut later_uses = Vec::new();
            let mut consumers = Vec::new();
            for &use_statement in uses {
                let Statement::Expression { expression: call } =
                    ast.statements().get(use_statement).ok()?.payload()
                else {
                    return None;
                };
                let argument = self.binding_use_set_operand(*call, 0, 1)?;
                let place = self.checker.place(argument).ok()??;
                if place == auxiliary_place {
                    later_uses.push((*call, argument, statement(use_statement)));
                } else if place.is_root() && place.root() == binding {
                    consumers.push((*call, argument, statement(use_statement)));
                } else {
                    return None;
                }
            }
            if !(1..=MAX_AUXILIARY_SEQUENCE_USES).contains(&later_uses.len())
                || !(2..=8).contains(&consumers.len())
            {
                return None;
            }
            let &(consumer_call, _, consumer_statement) = consumers.last()?;
            Some(Shape {
                scope: Scope {
                    callable: UnitItemId::new(self.checker.source_unit, item),
                    binding,
                    root,
                    source_call: self.checker.unit_expression(*source_call),
                    consumer_call: self.checker.unit_expression(consumer_call),
                    entry: statement(entry),
                    completion: statement(body),
                    consumer_statement,
                },
                owners,
                source_argument,
                source_index,
                auxiliary_argument,
                consumers,
                later_uses,
            })
        })
    }

    fn binding_use_set_operand(
        &self,
        expression: ExpressionId,
        index: usize,
        count: usize,
    ) -> Option<ExpressionId> {
        let ast = self.checker.parsed.ast().expressions();
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = ast.get(expression).ok()?.payload()
        else {
            return None;
        };
        if arguments.len() != count || !type_arguments.is_empty() {
            return None;
        }
        let argument = arguments.get(index)?;
        (argument.mode_marker.is_none()
            && argument.named_prefix.is_none()
            && matches!(ast.get(*callee).ok()?.payload(), Expression::Name)
            && matches!(ast.get(argument.value).ok()?.payload(), Expression::Name))
        .then_some(argument.value)
    }

    fn binding_use_set_expected(&self, shape: &Shape, state: &ValueState) -> Option<Expected> {
        if self.scope_depth != 1 || !self.binding_region_boundary_clear(state) {
            return None;
        }
        let scope = &shape.scope;
        if state.values.len() != 2 {
            return None;
        }
        let source_owner = self.use_set_snapshot(scope.root, shape, state)?;
        let auxiliary_root = shape.owners.iter().find(|owner| owner.0 != scope.root)?.0;
        let auxiliary_owner = self.use_set_snapshot(auxiliary_root, shape, state)?;
        let source_id = source_owner.owner.as_ref()?;
        let auxiliary_id = auxiliary_owner.owner.as_ref()?;
        let observation = self.owner_observation.as_ref()?;
        let callable = crate::ownership_checking::UnitOwnerCallable::Function(scope.callable);
        if source_id == auxiliary_id
            || source_id.callable != callable
            || auxiliary_id.callable != callable
            || observation.callable != callable
            || !std::sync::Arc::ptr_eq(&source_id.identity, &observation.identity)
            || !std::sync::Arc::ptr_eq(&auxiliary_id.identity, &observation.identity)
        {
            return None;
        }
        let ty = self.checker.typed.symbol_type(scope.root)?;
        if self.checker.typed.symbol_type(scope.binding) != Some(ty) {
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
        let consumers = shape
            .consumers
            .iter()
            .map(|&(call, argument, _)| self.use_set_consumer(call, argument, source.target()))
            .collect::<Option<Vec<_>>>()?;
        let consumer_statements = shape.consumers.iter().map(|u| u.2).collect();
        let auxiliary_uses = shape
            .later_uses
            .iter()
            .map(|&(call, argument, _)| self.use_set_consumer(call, argument, auxiliary.target()))
            .collect::<Option<Vec<_>>>()?;
        let source_call = self.checker.typed.call(scope.source_call)?;
        if source_call.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(parameter)
            || source_call.return_type() != ty
            || !matches!(source.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == scope.root)
            || !matches!(auxiliary.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == auxiliary_root)
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
            scope: scope.clone(),
            source_index: shape.source_index,
            source,
            auxiliary,
            consumers,
            consumer_statements,
            auxiliary_uses,
        })
    }

    fn use_set_consumer(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        target: &UnitLoanTarget,
    ) -> Option<crate::ownership_checking::UnitLoanFact> {
        let (loan, _) = self.consumer_region_loan(call, argument, ExpressionCategory::Place)?;
        let typed = self
            .checker
            .typed
            .call(self.checker.unit_expression(call))?;
        if loan.target() != target
            || loan.kind() != crate::ownership_checking::LoanKind::Shared
            || typed.result_source() != CallableResultSource::Owned
            || self.checker.typed.types().get(typed.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        {
            return None;
        }
        Some(loan)
    }

    fn use_set_literal(
        &self,
        statement: StatementId,
    ) -> Option<(UnitSymbolId, ExpressionId, Span)> {
        let ast = self.checker.parsed.ast();
        let Statement::LocalVariable { declaration } =
            ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(origin),
            initializer,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        let Expression::String { parts } = ast.expressions().get(*initializer).ok()?.payload()
        else {
            return None;
        };
        if !parts.iter().all(|part| matches!(part, StringPart::Text(_))) {
            return None;
        }
        Some((
            *self.checker.marker_symbol(NameMarker::Present(*origin))?,
            *initializer,
            *origin,
        ))
    }
    fn use_set_snapshot(
        &self,
        root: UnitSymbolId,
        shape: &Shape,
        state: &ValueState,
    ) -> Option<OwnerSnapshot> {
        let (_, initializer, origin) = shape.owners.iter().find(|owner| owner.0 == root)?;
        let mut matching = state.values.iter().filter(|owner| owner.symbol == root);
        let owner = matching.next()?;
        let provenance = self.loan_provenance();
        if matching.next().is_some()
            || owner.owner.is_none()
            || owner.origin != *origin
            || owner.declaration != *origin
            || owner.scope_depth != 1
            || self.checker.symbol_kind(root) != Some(SymbolKind::Variable)
            || !self.checker.is_move_only_variable(root)
            || !provenance.formed_owned.contains(&root)
            || provenance.iteration_providers.contains(&root)
            || provenance.borrow_bindings.contains(&root)
            || self
                .checker
                .bindings
                .get(&root)
                .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
        {
            return None;
        }
        let literal = self.checker.unit_expression(*initializer);
        let ty = self.checker.typed.symbol_type(root)?;
        if self.checker.typed.types().get(ty) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
            || self.checker.typed.expression_type(literal) != Some(ty)
            || self.checker.typed.expression_category(literal)
                != Some(ExpressionCategory::Temporary)
        {
            return None;
        }
        Some(OwnerSnapshot {
            owner: owner.owner.clone(),
            symbol: root,
            origin: owner.origin,
            declaration: owner.declaration,
            scope_depth: owner.scope_depth,
        })
    }
}
