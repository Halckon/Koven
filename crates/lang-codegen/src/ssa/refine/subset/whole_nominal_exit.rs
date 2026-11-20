//! Exact cleanup admission for a whole temporary in a bounded terminal call.
//! The existing builder still emits, transports and verifies all cleanup.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolKind,
    ownership_checking::{
        ConstructionRootKind, LoanKind, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitDropTarget, UnitLoanTarget,
    },
    parser::{Expression, Item, LiteralKind, NameMarker, Statement, VariableKind, WhenCondition},
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::{Checker, RefineBody};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct WholeNominalExit {
    pub(super) lender: UnitExpressionId,
    pub(super) owner: UnitExpressionId,
    pub(super) transfer: UnitExpressionId,
}

impl Checker<'_, '_> {
    /// Called only after the usual callable and argument admission succeeds. Absence
    /// changes none of those checks; it leaves the existing pending-owner gate in place.
    pub(super) fn whole_nominal_exit(
        &self,
        call: ExpressionId,
        values: &[ExpressionId],
    ) -> Option<WholeNominalExit> {
        let [owner, later] = *values else { return None };
        let RefineBody::Block(body) = self.inputs.body else {
            return None;
        };
        let ast = self.inputs.parsed.ast();
        let node = |id| ast.expressions().get(id).ok();
        let payload = |id| node(id).map(|n| n.payload());
        let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
            return None;
        };
        let Expression::Return {
            value: Some(tail), ..
        } = payload(self.exit_expression(*elements.last()?)?)?
        else {
            return None;
        };
        if (*tail != call && self.terminal_direct_val(elements, call, *tail).is_none())
            || self.inputs.function_item.is_none()
            || self.inputs.receiver.is_some()
            || self.inputs.borrow_return.is_some()
            || self.inputs.carrier_return.is_some()
            || !self.inputs.substitutions.is_empty()
            || self.inputs.static_self.is_some()
        {
            return None;
        }
        let is_builtin = |expression, builtin| {
            self.expression_type(expression)
                .is_some_and(|ty| self.kind(ty) == Some(&UnitTypeKind::Builtin(builtin)))
        };
        let lender = self.id(call);
        let typed = self.inputs.typed;
        let owned = self.inputs.owned;
        let descriptor = typed.call(lender)?;
        let UnitCallTarget::Declaration(callee) = descriptor.target() else {
            return None;
        };
        let [first, second] = descriptor.arguments() else {
            return None;
        };
        let site = self
            .inputs
            .source_plan
            .call_site(self.inputs.source_token, lender)?;
        let key = site.dispatch_key()?;
        if key.target() != UnitCallableTarget::Declaration(callee)
            || key.is_specialized()
            || !descriptor.instance().type_arguments().is_empty()
            || descriptor.receiver().is_some()
            || descriptor.result_source() != CallableResultSource::Owned
            || descriptor.borrow_return().is_some()
            || descriptor.aborts()
            || descriptor.range_construction().is_some()
            || !site.delegation().is_empty()
            || !self.inputs.function_ids.contains_key(key)
            || owned.call_may_dispatch(lender) != Some(true)
            || !is_builtin(call, BuiltinType::Int)
            || !is_builtin(later, BuiltinType::Int)
            || self.kind(self.inputs.return_type) != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            || first.argument_index() != 0
            || first.parameter_index() != 0
            || first.mode() != ParameterMode::Borrow
            || second.argument_index() != 1
            || second.parameter_index() != 1
            || second.mode() != ParameterMode::Value
            || self.kind(self.c(second.parameter_type()))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        {
            return None;
        }
        let Expression::Call { arguments, .. } = payload(owner)? else {
            return None;
        };
        let owner_span = node(owner)?.span();
        let owner = self.id(owner);
        let construction = typed.construction(owner)?;
        let UnitConstructionTarget::Nominal(declaration) = construction.target() else {
            return None;
        };
        let result = self.c(construction.result_type());
        let (signature, type_arguments) = self.class_signature(result)?;
        if !arguments.is_empty()
            || !construction.arguments().is_empty()
            || !construction.instance().type_arguments().is_empty()
            || !signature.fields().is_empty()
            || !self.class_fields(result)?.is_empty()
            || !type_arguments.is_empty()
            || self.class_has_deinit(result)
            || self.copyability_of(result) != super::Copyability::MoveOnly
            || self.c(first.parameter_type()) != result
            || construction.expression() != owner
            || typed.expression_category(owner) != Some(ExpressionCategory::Temporary)
            || !owned.call_argument_completed(owner)
        {
            return None;
        }
        let mut plans = owned
            .construction_plans()
            .iter()
            .filter(|p| p.construction() == owner);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return None;
        };
        let root = plan.root_obligation()?;
        if plan.target() != UnitConstructionTarget::Nominal(declaration)
            || plan.terminating_operand().is_some()
            || !plan.deliveries().is_empty()
            || root.kind() != ConstructionRootKind::HeapOwner
            || root.construction() != owner
            || self.c(root.result_type()) != result
        {
            return None;
        }
        let mut contracts = owned
            .call_argument_contracts_for(lender)
            .filter(|c| c.argument() == owner);
        let (Some(contract), None) = (contracts.next(), contracts.next()) else {
            return None;
        };
        if contract.parameter_index() != 0
            || self.c(contract.parameter_type()) != result
            || contract.category() != ExpressionCategory::Temporary
            || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
            || contract.crosses_thread()
        {
            return None;
        }
        // A dynamic parameter Name and literal true/else select the emitter's
        // two-edge Boolean fast path. Restrict both values to plain Int reads.
        let Expression::When {
            subject: Some(flag),
            entries,
            ..
        } = payload(later)?
        else {
            return None;
        };
        let [yes, no] = entries.as_slice() else {
            return None;
        };
        let [WhenCondition::Expression(selector)] = yes.conditions.as_slice() else {
            return None;
        };
        let parameter = self.reference(*flag)?;
        if !matches!(payload(*flag), Some(Expression::Name))
            || !self.inputs.parameters.contains(&parameter)
            || !matches!(
                self.parameter_mode(parameter),
                Some(ParameterMode::Borrow | ParameterMode::Value)
            )
            || typed.expression_falls_through(self.id(*flag)) != Some(true)
            || self
                .inputs
                .constant_owned
                .is_some_and(|facts| facts.materialization_at(self.id(*flag)).is_some())
            || typed.nullable_when(self.id(later)).is_some()
            || !is_builtin(*flag, BuiltinType::Boolean)
            || !matches!(
                payload(*selector),
                Some(Expression::Literal(LiteralKind::Boolean(true)))
            )
            || yes.else_span.is_some()
            || no.else_span.is_none()
            || !no.conditions.is_empty()
            || typed
                .when_closed_after(self.id(later))
                .is_some_and(|entries| entries != 2)
            || typed
                .when_final_selector_count(self.id(later), 0)
                .is_some_and(|count| count != 1)
        {
            return None;
        }
        let transfer = self.exit_expression(yes.body)?;
        let normal = self.exit_expression(no.body)?;
        let Expression::Return {
            value: Some(returned),
            ..
        } = payload(transfer)?
        else {
            return None;
        };
        let safe_read = |id| {
            matches!(payload(id), Some(Expression::Name))
                && self
                    .reference(id)
                    .is_some_and(|symbol| self.known.contains(&symbol))
                && is_builtin(id, BuiltinType::Int)
                && typed.expression_falls_through(self.id(id)) == Some(true)
        };
        if !safe_read(*returned)
            || !safe_read(normal)
            || typed.expression_falls_through(self.id(transfer)) != Some(false)
            || typed.expression_falls_through(self.id(later)) != Some(true)
        {
            return None;
        }
        let transfer = self.id(transfer);
        let mut loans = owned.loans().iter().filter(|loan| loan.call() == lender);
        let (Some(loan), None) = (loans.next(), loans.next()) else {
            return None;
        };
        if loan.argument() != owner
            || loan.target() != &UnitLoanTarget::Temporary(owner)
            || loan.kind() != LoanKind::Shared
        {
            return None;
        }
        let points = [
            UnitDropPoint::CallReturn(lender),
            UnitDropPoint::ControlTransfer(transfer),
        ];
        if owned.drops().iter().any(|drop| {
            drop.target() == UnitDropTarget::Temporary(owner) && !points.contains(&drop.point())
        }) {
            return None;
        }
        for point in points {
            let mut ends = owned
                .direct_call_loan_ends()
                .iter()
                .filter(|end| end.point() == point);
            let (Some(end), None) = (ends.next(), ends.next()) else {
                return None;
            };
            if end.loan() != loan
                || end.preceding_call_loan_ends() != 0
                || end.preceding_drops() != 0
            {
                return None;
            }
            let mut drops = owned.drops().iter().filter(|drop| drop.point() == point);
            let (Some(first), None) = (drops.next(), drops.next()) else {
                return None;
            };
            if first.target() != UnitDropTarget::Temporary(owner)
                || first.value_origin() != owner_span
                || first.is_conditional_temporary()
            {
                return None;
            }
        }
        Some(WholeNominalExit {
            lender,
            owner,
            transfer,
        })
    }

    /// Match only the top-level final pair, with no wrapper or result adaptation.
    /// The usual local binding/return builder still handles both statements.
    fn terminal_direct_val(
        &self,
        statements: &[StatementId],
        call: ExpressionId,
        returned: ExpressionId,
    ) -> Option<()> {
        let [.., local, terminal] = statements else {
            return None;
        };
        let ast = self.inputs.parsed.ast();
        if !matches!(
            ast.statements().get(*terminal).ok()?.payload(),
            Statement::Expression { .. }
        ) {
            return None;
        }
        let Statement::LocalVariable { declaration } = ast.statements().get(*local).ok()?.payload()
        else {
            return None;
        };
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(name),
            initializer,
            ..
        } = ast.items().get(*declaration).ok()?.payload()
        else {
            return None;
        };
        if *initializer != call
            || !matches!(
                ast.expressions().get(returned).ok()?.payload(),
                Expression::Name
            )
            || !self.plain(returned)
        {
            return None;
        }

        let symbol = self.reference(returned)?;
        if symbol.source_unit() != self.inputs.source_unit {
            return None;
        }
        let source = self
            .inputs
            .names
            .names()
            .source_units()
            .get(symbol.source_unit().index())?;
        let declared = source.resolution().symbols().get(symbol.symbol().index())?;
        let int = self.expression_type(call)?;
        let typed = self.inputs.typed;
        (declared.kind() == SymbolKind::Variable
            && declared.span() == *name
            && self.kind(int) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            && self.c(typed.call(self.id(call))?.return_type()) == int
            && self.symbol_type(symbol) == Some(int)
            && self.expression_type(returned) == Some(int)
            && self.inputs.return_type == int
            && typed.non_null_use(self.id(returned)).is_none()
            && typed.expression_falls_through(self.id(call)) == Some(true)
            && typed.expression_falls_through(self.id(returned)) == Some(true))
        .then_some(())
    }

    fn exit_expression(&self, statement: StatementId) -> Option<ExpressionId> {
        let statements = self.inputs.parsed.ast().statements();
        let mut node = statements.get(statement).ok()?;
        if let Statement::ControlBody { elements } = node.payload() {
            let [only] = elements.as_slice() else {
                return None;
            };
            node = statements.get(*only).ok()?;
        }
        let Statement::Expression { expression } = node.payload() else {
            return None;
        };
        Some(*expression)
    }
}
