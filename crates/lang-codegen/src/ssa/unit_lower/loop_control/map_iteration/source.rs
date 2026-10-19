//! Source evaluation and factory identity are established before a provider is created.
use super::*;
use crate::ssa::unit_lower::builtin_type;
use lang_frontend::ownership_checking::{UnitDropTarget, UnitLoanTarget};
use lang_frontend::type_checking::UnitCallTarget;

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_in_current_callable(&self, span: Span) -> bool {
        map_iteration::enclosing_lambda(self.parsed, span)
            .map(|lambda| UnitExpressionId::new(self.source_unit, lambda))
            == self.thunk_expression
    }

    pub(super) fn borrowed_map_anchor(
        &self,
        proof: map_iteration::borrowed::Proof,
        current: EntityId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if self.bindings.contains_key(&proof.binding)
            || self
                .result_source_loans
                .get(&proof.binding)
                .is_none_or(|loans| !loans.is_empty())
        {
            return Err(bad());
        }
        let entry = self.function.entry_block().ok_or_else(bad)?;
        let parent = transport::parameter(self.function, entry, proof.slot, span)?;
        crate::ssa::borrow_result_support::stable_map::seed(
            self.function,
            current,
            parent,
            proof.declaration,
            span,
            transport::from_anchor,
        )
    }

    pub(super) fn map_source_seed(
        &mut self,
        qualified: map_iteration::QualifiedMap,
        access: UnitIterationSourceAccess,
        container: SsaTypeId,
        span: Span,
    ) -> Result<(EntityId, LoanId), LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
        if !self.map_in_current_callable(span) {
            return Err(missing());
        }
        if let Some(lambda) = self.thunk_expression {
            if !matches!(qualified.source, SourceIdentity::Local { .. }) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let plan = self
                .callable_plans
                .get(&(self.closure_scope, lambda))
                .ok_or_else(missing)?;
            if plan.scope != self.closure_scope
                || plan.source_token != self.source_token
                || plan.source_unit != self.source_unit
                || plan.expression != lambda
                || plan.thunk != self.function.id
                || plan.return_type != self.return_type
                || !plan.captures.is_empty()
                || builtin_type(self.typed, plan.return_type) != Some(BuiltinType::Unit)
                || map_iteration::qualify_local_lambda(
                    self.parsed,
                    self.typed,
                    self.owned,
                    self.source_unit,
                    plan.body,
                    span,
                )? != lambda
                || !matches!(self.parsed.ast().statements().get(plan.body).map_err(|_| missing())?.payload(),
                    lang_frontend::parser::Statement::LambdaBody { elements }
                        if elements.contains(&qualified.statement.statement()))
            {
                return Err(missing());
            }
        }
        let identity = qualified.source;
        let seed = match identity {
            SourceIdentity::BorrowedBinding(proof) => {
                if access != UnitIterationSourceAccess::Shared {
                    return Err(missing());
                }
                let current = identity.current(self, span)?;
                self.borrowed_map_anchor(proof, current, span)?;
                current
            }
            SourceIdentity::Parameter { slot, .. } => {
                let entry = self.function.entry_block().ok_or_else(missing)?;
                let anchor = transport::parameter(self.function, entry, slot, span)?;
                let current = identity.current(self, span)?;
                transport::from_anchor(self.function, current, anchor, span)?;
                current
            }
            SourceIdentity::Local { root, initializer } => {
                if access != UnitIterationSourceAccess::Owned {
                    return Err(missing());
                }
                let current = identity.current(self, span)?;
                let anchor = local::validate_birth(self, root, initializer, current, span)?;
                if current != anchor
                    || carrier::instruction(self.function, anchor, span)?.block != self.block
                {
                    return Err(missing());
                }
                current
            }
            SourceIdentity::Formed { root, birth } => {
                if access != UnitIterationSourceAccess::Owned {
                    return Err(missing());
                }
                let current = identity.current(self, span)?;
                local::validate_formed_birth(self, root, birth, current, span)?;
                if self
                    .bindings
                    .values()
                    .filter(|v| {
                        **v == match current {
                            EntityId::Value(v) => LoweredValue::Value(v),
                            _ => LoweredValue::Diverged,
                        }
                    })
                    .count()
                    != 1
                {
                    return Err(missing());
                }
                current
            }
            SourceIdentity::Temporary(expression) => {
                if access != UnitIterationSourceAccess::Temporary
                    || expression.source_unit() != self.source_unit
                    || identity.is_registered(self)
                {
                    return Err(missing());
                }
                let LoweredValue::Value(owner) = self.lower(expression.expression())? else {
                    return Err(missing());
                };
                // Reuse the ordinary argument CFG; provider acquisition begins
                // at the actual selected Call's completed delivery block.
                let preheader = self.block;
                let seed = EntityId::Value(owner);
                if identity.current(self, span)? != seed {
                    return Err(missing());
                }
                validate_temporary_birth(self, expression, seed, preheader, span)?;
                seed
            }
        };
        match identity {
            SourceIdentity::Parameter { root, .. } => {
                let current = match (access, seed) {
                    (UnitIterationSourceAccess::Owned, EntityId::Value(owner)) => {
                        self.bindings.get(&root) == Some(&LoweredValue::Value(owner))
                    }
                    (UnitIterationSourceAccess::Shared, EntityId::Loan(loan)) => {
                        self.borrow_bindings.get(&root) == Some(&loan)
                    }
                    _ => return Err(missing()),
                };
                // Both owned and Borrow seeds use the current, anchor-proven CFG view;
                // the published source plan independently determines its lifetime.
                if !current {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            SourceIdentity::BorrowedBinding(_)
            | SourceIdentity::Local { .. }
            | SourceIdentity::Formed { .. }
            | SourceIdentity::Temporary(_) => {
                if identity.current(self, span)? != seed {
                    return Err(missing());
                }
            }
        }
        let source = match (access, seed) {
            (
                UnitIterationSourceAccess::Owned | UnitIterationSourceAccess::Temporary,
                EntityId::Value(owner),
            ) => {
                if self.function.entity(seed).map(|data| data.ty)
                    != Some(EntityType::Value(container))
                {
                    return Err(missing());
                }
                self.iteration_owner_loan(owner, container, span)?
            }
            (UnitIterationSourceAccess::Shared, EntityId::Loan(loan)) => {
                if self.function.entity(seed).map(|data| data.ty)
                    != Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: container,
                    })
                {
                    return Err(missing());
                }
                self.iteration_loan(Operation::SharedReborrow { source: loan }, container, span)?
            }
            _ => return Err(missing()),
        };
        Ok((seed, source))
    }
}

pub(super) fn validate_temporary_birth(
    lowerer: &UnitExpressionLowerer<'_>,
    expression: UnitExpressionId,
    seed: EntityId,
    preheader: BlockId,
    span: Span,
) -> Result<(), LoweringError> {
    validate_ordinary_birth(lowerer, expression, seed, span)?;
    let call = carrier::instruction(lowerer.function, seed, span)?;
    if call.block != preheader {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    Ok(())
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super::super) fn validate_pure_map_parameter_drop(
        &self,
        fact: lang_frontend::ownership_checking::UnitDropFact,
        owner: ValueId,
    ) -> Result<(), LoweringError> {
        let UnitDropTarget::Named(root) = fact.target() else {
            return Ok(());
        };
        let mut relevant_parameter = false;
        for descriptor in self.typed.map_iterations() {
            if descriptor.statement().source_unit() != self.source_unit
                || self.typed.is_resource_type(descriptor.source_type()) != Some(false)
            {
                continue;
            }
            let Some(plan) = self.owned.map_iteration(descriptor.statement()) else {
                // A type descriptor in another callable may never acquire a provider.
                // Only an actual ownership plan for this root can claim its Drop.
                continue;
            };
            if !matches!(plan.source(),Some(UnitLoanTarget::Place(place)) if place.is_root()&&place.root()==root)
            {
                continue;
            }
            let qualified = map_iteration::qualify(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                descriptor,
            )?;
            let SourceIdentity::Parameter { root: actual, slot } = qualified.source else {
                continue;
            };
            if actual != root {
                continue;
            }
            relevant_parameter = true;
            let mut claimed = [
                qualified.source_drop,
                qualified.late_drop,
                qualified.transfer_drop,
            ]
            .into_iter()
            .any(|candidate| candidate == Some(fact));
            if !claimed
                && let Some(condition) = qualified.conditional
                && plan.exits().iter().any(|exit| exit.point() == fact.point())
            {
                claimed =
                    condition.drop_at(self.owned, qualified.source, fact.point())? == Some(fact);
            }
            if !claimed {
                continue;
            }
            let entry = self.function.entry_block().ok_or_else(|| {
                lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
            })?;
            let anchor = transport::parameter(self.function, entry, slot, fact.value_origin())?;
            if !matches!(anchor, EntityId::Value(_))
                || self.borrow_bindings.contains_key(&root)
                || self.temporaries.values().any(|&v| v == owner)
                || self
                    .bindings
                    .values()
                    .any(|v| *v == LoweredValue::Value(owner))
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    fact.value_origin(),
                ));
            }
            transport::from_anchor(
                self.function,
                EntityId::Value(owner),
                anchor,
                fact.value_origin(),
            )?;
            return Ok(());
        }
        if relevant_parameter {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.value_origin(),
            ));
        }
        Ok(())
    }
}

impl UnitExpressionLowerer<'_> {
    /// A formed root's final cleanup must still transport the exact selected Call owner.
    pub(in super::super::super) fn validate_formed_map_drop(
        &self,
        fact: lang_frontend::ownership_checking::UnitDropFact,
        owner: ValueId,
    ) -> Result<(), LoweringError> {
        let UnitDropTarget::Named(root) = fact.target() else {
            return Ok(());
        };
        let bad = || lowering_error(LoweringErrorKind::MissingFact, fact.value_origin());
        let mut relevant = false;
        let mut other_epoch = None;
        for descriptor in self.typed.map_iterations() {
            if descriptor.statement().source_unit() != self.source_unit {
                continue;
            }
            let Some(plan) = self.owned.map_iteration(descriptor.statement()) else {
                continue;
            };
            if !matches!(plan.source(), Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root() == root)
            {
                continue;
            }
            let qualified = map_iteration::qualify(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                descriptor,
            )?;
            let SourceIdentity::Formed {
                root: actual,
                birth,
            } = qualified.source
            else {
                continue;
            };
            if actual != root {
                continue;
            }
            let span = self
                .parsed
                .ast()
                .statements()
                .get(descriptor.statement().statement())
                .map_err(|_| bad())?
                .span();
            let formation = map_iteration::formed::formation(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                root,
                descriptor.source_type(),
                span,
            )?;
            if !map_iteration::formed::drops(self.parsed, self.owned, root, &formation)?
                .contains(&fact)
            {
                let point = crate::ssa::map_source::unit_drop_event(self.parsed, fact.point())
                    .ok_or_else(bad)?;
                let epoch = crate::ssa::map_source::epoch_at(self.parsed, &formation, point)?;
                let call = crate::ssa::map_source::epoch_call(self.parsed, epoch.rhs)?;
                let call = UnitExpressionId::new(root.source_unit(), call);
                if other_epoch.is_some_and(|previous| previous != call) {
                    return Err(bad());
                }
                other_epoch = Some(call);
                continue;
            }
            relevant = true;
            let mut claimed = [
                qualified.source_drop,
                qualified.late_drop,
                qualified.transfer_drop,
            ]
            .into_iter()
            .any(|candidate| candidate == Some(fact));
            if !claimed
                && let Some(shape) = qualified.conditional
                && plan.exits().iter().any(|exit| exit.point() == fact.point())
            {
                claimed = shape.drop_at(self.owned, qualified.source, fact.point())? == Some(fact);
            }
            if !claimed {
                continue;
            }
            if formation.birth != birth
                || self.borrow_bindings.contains_key(&root)
                || self
                    .bindings
                    .values()
                    .any(|v| *v == LoweredValue::Value(owner))
            {
                return Err(bad());
            }
            local::validate_formed_birth(self, root, birth, EntityId::Value(owner), span)?;
            return Ok(());
        }
        if relevant {
            return Err(bad());
        }
        if let Some(producer) = other_epoch {
            local::validate_other_epoch_birth(
                self,
                root,
                producer,
                EntityId::Value(owner),
                fact.value_origin(),
            )?;
        }
        Ok(())
    }
}

/// Anchor the selected normal call result for formed owners and other binding
/// epochs. Argument/receiver delivery and effective routes stay with the ordinary
/// planner and emitter; their operands are never evaluated again here.
pub(super) fn validate_ordinary_birth(
    lowerer: &UnitExpressionLowerer<'_>,
    expression: UnitExpressionId,
    seed: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let node = lowerer
        .parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| bad())?;
    let result_type = lowerer.typed.expression_type(expression).ok_or_else(bad)?;
    let ty = lowerer.iteration_type(result_type, span)?;
    let call = carrier::instruction(lowerer.function, seed, span)?;
    if expression.source_unit() != lowerer.source_unit
        || call.origin != Origin::Source(node.span())
        || lowerer.function.entity(seed).map(|e| e.ty) != Some(EntityType::Value(ty))
    {
        return Err(bad());
    }
    if let Some(construction) = lowerer.typed.map_construction(expression) {
        if construction.expression() != expression
            || construction.map_type() != result_type
            || call.operation != (Operation::MapConstruct { map_type: ty })
        {
            return Err(bad());
        }
    } else if let Some(consume) = lowerer.typed.map_consume(expression) {
        let Operation::MapConsume { owner, map_type } = call.operation else {
            return Err(bad());
        };
        if consume.expression() != expression
            || consume.result_type() != result_type
            || consume.receiver().source_unit() != expression.source_unit()
            || map_type != ty
            || lowerer
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(
                    lowerer.iteration_type(consume.receiver_type(), span)?,
                ))
        {
            return Err(bad());
        }
    } else {
        let descriptor = lowerer.typed.call(expression).ok_or_else(bad)?;
        if descriptor.expression() != expression
            || descriptor.return_type() != result_type
            || descriptor.result_source()
                != lang_frontend::type_checking::CallableResultSource::Owned
        {
            return Err(bad());
        }
        match descriptor.target() {
            UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_) => {
                let resolved = lowerer
                    .source_plan
                    .call_site(lowerer.source_token, expression)
                    .ok_or_else(bad)?;
                let callee = *lowerer.function_ids.get(resolved.key()).ok_or_else(bad)?;
                if !matches!(&call.operation, Operation::DirectCall { callee: actual, receiver, arguments }
                    if *actual == callee && receiver.is_some() == descriptor.receiver().is_some()
                        && arguments.len() == descriptor.arguments().len())
                {
                    return Err(bad());
                }
            }
            UnitCallTarget::FunctionValue => {
                let lang_frontend::parser::Expression::Call { callee, .. } = node.payload() else {
                    return Err(bad());
                };
                let callee = UnitExpressionId::new(expression.source_unit(), *callee);
                let expected = lowerer.callable_abi.expression_type(
                    lowerer.source_plan,
                    lowerer.typed,
                    lowerer.owned,
                    lowerer.source_token,
                    callee,
                    span,
                )?;
                if descriptor.receiver().is_some()
                    || !matches!(&call.operation, Operation::CallableInvoke { callable, arguments }
                        if lowerer.function.entity(EntityId::Value(*callable)).map(|data| data.ty)
                            == Some(EntityType::Value(expected))
                            && arguments.len() == descriptor.arguments().len())
                {
                    return Err(bad());
                }
            }
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }
    let calls = birth_instructions(lowerer, node.span());
    if !matches!(calls.as_slice(), [actual] if actual.id == call.id) {
        return Err(bad());
    }
    Ok(())
}

fn birth_instructions<'a>(
    lowerer: &'a UnitExpressionLowerer<'_>,
    origin: Span,
) -> Vec<&'a crate::ssa::model::Instruction> {
    lowerer
        .function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(origin)
                && matches!(
                    instruction.operation,
                    Operation::DirectCall { .. }
                        | Operation::CallableInvoke { .. }
                        | Operation::MapConstruct { .. }
                        | Operation::MapConsume { .. }
                )
        })
        .collect()
}

/// Recover the one actual result before validating its selected typed birth.
/// This never evaluates a receiver, callable or argument a second time.
pub(super) fn ordinary_birth(
    lowerer: &UnitExpressionLowerer<'_>,
    expression: UnitExpressionId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let origin = lowerer
        .parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| bad())?
        .span();
    let births = birth_instructions(lowerer, origin);
    let [birth] = births.as_slice() else {
        return Err(bad());
    };
    let [owner @ EntityId::Value(_)] = birth.results.as_slice() else {
        return Err(bad());
    };
    validate_ordinary_birth(lowerer, expression, *owner, span)?;
    Ok(*owner)
}
