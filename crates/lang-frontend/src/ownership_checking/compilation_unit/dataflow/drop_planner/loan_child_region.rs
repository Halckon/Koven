//! Exact four-statement syntax and independent typed/current-state admission.
use super::*;
use crate::{
    name_resolution::SymbolKind,
    ownership_checking::compilation_unit::loan_child_region_validation::{Expected, Scope},
    ownership_checking::{
        BorrowBindingStorage, OwnershipBindingKind, UnitDropPoint, UnitLoanTarget,
    },
    parser::{NameMarker, VariableKind},
    source::Span,
    type_checking::{BorrowReturnOrigin, CallableResultSource, UnitItemId},
};

struct Shape {
    scope: Scope,
    owner_initializer: ExpressionId,
    owner_origin: Span,
    source_argument: ExpressionId,
    consumer_argument: ExpressionId,
    child_initializer: ExpressionId,
}

impl DropPlanner<'_, '_> {
    pub(super) fn enter_child_region(&mut self, statement: StatementId, state: &ValueState) {
        if !self.iteration_actions.enabled {
            return;
        }
        let Some(shape) = self.child_region_shape(statement) else {
            return;
        };
        let expected = self.child_region_expected(&shape, state);
        self.iteration_actions
            .enter_child_region(shape.scope, expected);
    }

    pub(super) fn finish_child_region_statement(
        &mut self,
        statement: StatementId,
        continues: bool,
        state: &ValueState,
    ) {
        // Completion follows the enclosing block's drop_scope and its borrow-end wrapper.
        // Empty active state also proves the following item-level drop_all is inert.
        let empty = self.scope_depth == 0
            && self.binding_region_boundary_clear(state)
            && state.values.is_empty();
        self.iteration_actions.finish_child_region_statement(
            UnitStatementId::new(self.checker.source_unit, statement),
            continues,
            empty,
        );
    }

    fn child_region_shape(&self, entry: StatementId) -> Option<Shape> {
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
        let source_argument = self.child_region_operand(*source_call)?;
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
            let [
                owner_statement,
                binding_statement,
                child_statement,
                consumer_statement,
            ] = elements.as_slice()
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
            let Statement::LocalVariable { declaration } =
                ast.statements().get(*child_statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                kind: VariableKind::BorrowVal(_),
                name: NameMarker::Present(child_origin),
                initializer: child_initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if !matches!(
                ast.expressions().get(*child_initializer).ok()?.payload(),
                Expression::Name
            ) {
                return None;
            }
            let child = *self
                .checker
                .marker_symbol(NameMarker::Present(*child_origin))?;
            let child_place = self.checker.place(*child_initializer).ok()??;
            let consumer_argument = self.child_region_operand(*consumer_call)?;
            let source_place = self.checker.place(source_argument).ok()??;
            let consumer_place = self.checker.place(consumer_argument).ok()??;
            if !source_place.is_root()
                || source_place.root() != root
                || !consumer_place.is_root()
                || consumer_place.root() != child
                || !child_place.is_root()
                || child_place.root() != binding
                || child == binding
                || child == root
                || binding == root
            {
                return None;
            }
            let statement = |id| UnitStatementId::new(self.checker.source_unit, id);
            Some(Shape {
                scope: Scope {
                    callable: UnitItemId::new(self.checker.source_unit, item),
                    parent: binding,
                    child,
                    child_initializer: self.checker.unit_expression(*child_initializer),
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
                consumer_argument,
                child_initializer: *child_initializer,
            })
        })
    }

    fn child_region_operand(&self, expression: ExpressionId) -> Option<ExpressionId> {
        let ast = self.checker.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = ast.get(expression).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        (argument.mode_marker.is_none()
            && matches!(ast.get(*callee).ok()?.payload(), Expression::Name)
            && matches!(ast.get(argument.value).ok()?.payload(), Expression::Name))
        .then_some(argument.value)
    }

    fn child_region_expected(&self, shape: &Shape, state: &ValueState) -> Option<Expected> {
        if self.scope_depth != 1 || !self.binding_region_boundary_clear(state) {
            return None;
        }
        let scope = &shape.scope;
        let [owner] = state.values.as_slice() else {
            return None;
        };
        let provenance = self.loan_provenance();
        if owner.owner.as_ref().is_none_or(|id| {
            id.callable != crate::ownership_checking::UnitOwnerCallable::Function(scope.callable)
        }) || owner.symbol != scope.root
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
            || self.checker.typed.symbol_type(scope.parent) != Some(ty)
            || self.checker.typed.symbol_type(scope.child) != Some(ty)
            || self.checker.typed.expression_type(scope.child_initializer) != Some(ty)
            || self
                .checker
                .typed
                .expression_category(scope.child_initializer)
                != Some(ExpressionCategory::Place)
        {
            return None;
        }
        let (source, parameter) = self.consumer_region_loan(
            scope.source_call.expression(),
            shape.source_argument,
            ExpressionCategory::Place,
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
            || binding.binding() != scope.parent
            || binding.parent().is_some()
            || binding.storage() != BorrowBindingStorage::BorrowedStorage
            || binding.initializer() != scope.source_call
            || binding.origin() != source.target()
            || source_loan.call() != source.call()
            || source_loan.argument() != source.argument()
        {
            return None;
        }
        let mut children = self
            .borrow_bindings_initialized_by(self.checker.unit_expression(shape.child_initializer));
        let child = children.next()?;
        if children.next().is_some()
            || child.binding() != scope.child
            || child.parent() != Some(scope.parent)
            || child.source_loan().is_some()
            || child.storage() != BorrowBindingStorage::BorrowedStorage
            || child.initializer() != scope.child_initializer
            || child.origin() != source.target()
        {
            return None;
        }
        for symbol in [scope.parent, scope.child] {
            let mut facts = self
                .checker
                .borrow_results
                .bindings
                .iter()
                .filter(|fact| fact.binding() == symbol);
            facts.next()?;
            if facts.next().is_some() {
                return None;
            }
            let mut ends = self
                .checker
                .borrow_results
                .ends
                .iter()
                .filter(|end| end.binding == symbol);
            let end = ends.next()?;
            if ends.next().is_some()
                || end.point != UnitDropPoint::AfterStatement(scope.consumer_statement)
            {
                return None;
            }
        }
        Some(Expected {
            source,
            consumer,
            owner: crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot {
                owner: owner.owner.clone(),
                symbol: owner.symbol,
                origin: owner.origin,
                declaration: owner.declaration,
                scope_depth: owner.scope_depth,
            },
        })
    }
}
