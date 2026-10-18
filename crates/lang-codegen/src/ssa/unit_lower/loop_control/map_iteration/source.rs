//! Source evaluation and factory identity are established before a provider is created.
use super::*;
use crate::ssa::unit_plan::UnitFunctionInstanceKey;
use lang_frontend::type_checking::{UnitCallTarget, UnitCallableTarget};

impl UnitExpressionLowerer<'_> {
    pub(super) fn map_source_seed(
        &mut self,
        identity: SourceIdentity,
        access: UnitIterationSourceAccess,
        container: SsaTypeId,
        span: Span,
    ) -> Result<(EntityId, LoanId), LoweringError> {
        let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
        let seed = match identity {
            SourceIdentity::Parameter { slot, .. } => self
                .function
                .entry_block()
                .and_then(|entry| self.function.block(entry))
                .and_then(|block| block.parameters.get(slot))
                .copied()
                .ok_or_else(missing)?,
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
                // Keep the existing named-parameter lifetime capability boundary.
                if !current {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            SourceIdentity::Temporary(_) => {
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
