//! Source evaluation and factory identity are established before a provider is created.
use super::*;
use crate::ssa::unit_plan::UnitFunctionInstanceKey;
use lang_frontend::ownership_checking::{UnitDropTarget, UnitLoanTarget};
use lang_frontend::type_checking::{UnitCallTarget, UnitCallableTarget};

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_source_seed(
        &mut self,
        qualified: map_iteration::QualifiedMap,
        access: UnitIterationSourceAccess,
        container: SsaTypeId,
        span: Span,
    ) -> Result<(EntityId, LoanId), LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
        let identity = qualified.source;
        let seed = match identity {
            SourceIdentity::Parameter { slot, .. } => {
                let entry = self.function.entry_block().ok_or_else(missing)?;
                let anchor = transport::parameter(self.function, entry, slot, span)?;
                let current = identity.current(self, span)?;
                transport::from_anchor(self.function, current, anchor, span)?;
                current
            }
            SourceIdentity::Local { root, initializer }
            | SourceIdentity::Formed {
                root,
                producer: initializer,
            } => {
                if access != UnitIterationSourceAccess::Owned {
                    return Err(missing());
                }
                let current = identity.current(self, span)?;
                let formation = map_iteration::formed::formation(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    root,
                    self.typed
                        .expression_type(initializer)
                        .ok_or_else(missing)?,
                    span,
                )?;
                if formation.producer != initializer.expression()
                    || formation.roots.iter().skip(1).any(|root| {
                        self.bindings.contains_key(root) || self.borrow_bindings.contains_key(root)
                    })
                {
                    return Err(missing());
                }
                let anchor = local::validate_birth(self, root, initializer, current, span)?;
                if matches!(identity, SourceIdentity::Local { .. })
                    && (current != anchor
                        || carrier::instruction(self.function, anchor, span)?.block != self.block)
                {
                    return Err(missing());
                }
                if self.bindings.values().filter(|value| matches!(value, LoweredValue::Value(owner) if EntityId::Value(*owner) == current)).count() != 1 {
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
                let preheader = self.block;
                let LoweredValue::Value(owner) = self.lower(expression.expression())? else {
                    return Err(missing());
                };
                let seed = EntityId::Value(owner);
                if self.block != preheader || identity.current(self, span)? != seed {
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
            SourceIdentity::Local { .. }
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
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let node = lowerer
        .parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| missing())?;
    let descriptor = lowerer.typed.call(expression).ok_or_else(missing)?;
    let UnitCallTarget::Declaration(factory) = descriptor.target() else {
        return Err(missing());
    };
    let resolved = lowerer
        .source_plan
        .call_site(lowerer.source_token, expression)
        .ok_or_else(missing)?;
    let expected_key = UnitFunctionInstanceKey::new(factory, Vec::new());
    if expression.source_unit() != lowerer.source_unit
        || descriptor.expression() != expression
        || resolved.key() != &expected_key
        || resolved.key().target() != UnitCallableTarget::Declaration(factory)
        || !resolved.delegation().is_empty()
    {
        return Err(missing());
    }
    let callee = *lowerer
        .function_ids
        .get(&expected_key)
        .ok_or_else(missing)?;
    let call = carrier::instruction(lowerer.function, seed, span)?;
    let ty = lowerer.iteration_type(descriptor.return_type(), span)?;
    if !matches!(seed, EntityId::Value(_))
        || call.block != preheader
        || call.origin != Origin::Source(node.span())
        || call.operation
            != (Operation::DirectCall {
                callee,
                receiver: None,
                arguments: Vec::new(),
            })
        || lowerer.function.entity(seed).map(|data| data.ty) != Some(EntityType::Value(ty))
    {
        return Err(missing());
    }
    let calls = lowerer
        .function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(node.span())
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    if !matches!(calls.as_slice(), [actual] if actual.id == call.id) {
        return Err(missing());
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
            if ![
                qualified.source_drop,
                qualified.late_drop,
                qualified.transfer_drop,
            ]
            .into_iter()
            .chain(
                qualified
                    .conditional
                    .into_iter()
                    .flat_map(|condition| condition.drops),
            )
            .any(|candidate| candidate == Some(fact))
            {
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
