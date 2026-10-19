//! Current local owners and their one final consumption, inside the existing Map state.
mod evidence;
use super::*;
use crate::ssa::model::InstructionId;
use lang_frontend::{
    ownership_checking::{DropFact, DropTarget},
    type_checking::CallableTarget,
};

#[derive(Clone, Copy)]
struct Consumption {
    block: BlockId,
    owner: ValueId,
    instruction: InstructionId,
}

#[derive(Clone)]
pub(super) struct Carrier {
    proof: local::Proof,
    statement: StatementId,
    birth: ValueId,
    current: ValueId,
    continuation: BlockId,
    source: LoanId,
    consumed: Option<Consumption>,
    size: Option<ValueId>,
}

impl Carrier {
    pub(super) fn root(&self) -> SymbolId {
        self.proof.root
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn local_map_seed(
        &self,
        symbol: SymbolId,
        initializer: ExpressionId,
        container: SsaTypeId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(missing());
        };
        if self.borrow_bindings.contains_key(&symbol)
            || self.temporaries.values().any(|value| *value == owner)
            || self
                .bindings
                .iter()
                .any(|(other, value)| *other != symbol && *value == LoweredValue::Value(owner))
            || self.map_iteration_state.locals.contains_key(&symbol)
        {
            return Err(missing());
        }
        let seed = EntityId::Value(owner);
        let instruction = carrier::definition(self.function, seed).ok_or_else(missing)?;
        if instruction.block != self.block
            || self.map_call_birth(initializer, span)? != owner
            || self.function.entity(seed).map(|entity| entity.ty)
                != Some(EntityType::Value(container))
        {
            return Err(missing());
        }
        Ok(seed)
    }

    pub(super) fn validate_local_map_entry(
        &self,
        proof: &local::Proof,
        span: Span,
    ) -> Result<(), LoweringError> {
        for dead in &proof.dead {
            self.validate_dead_local_map(*dead, span)?;
        }
        if let Some(path) = proof.path {
            evidence::path(self, path, span)?;
        }
        Ok(())
    }

    fn validate_dead_local_map(
        &self,
        dead: local::DeadColumn,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let previous = self
            .map_iteration_state
            .locals
            .get(&dead.root)
            .ok_or_else(missing)?;
        if previous.statement != dead.previous
            || previous.proof.initializer != dead.initializer
            || previous.proof.drop != dead.consumed
            || previous.consumed.is_none()
            || self.bindings.contains_key(&dead.root)
            || self.borrow_bindings.contains_key(&dead.root)
            || self
                .temporaries
                .values()
                .any(|owner| *owner == previous.birth || *owner == previous.current)
        {
            return Err(missing());
        }
        validate_consumption(self.function, previous, span)
    }

    pub(super) fn local_map_completion(
        &self,
        proof: local::Proof,
        evidence: carrier::Evidence<'_>,
    ) -> Result<Carrier, LoweringError> {
        let span = evidence.span;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        evidence::edges(evidence, &proof)?;
        evidence::consumer(self, evidence, &proof)?;
        let Some((
            owner_proof,
            SourceIdentity::Local {
                symbol,
                initializer,
            },
            current,
        )) = evidence.owner
        else {
            return Err(missing());
        };
        let EntityId::Value(birth) = evidence.seed else {
            return Err(missing());
        };
        if symbol != proof.root
            || initializer != proof.initializer
            || owner_proof.drop() != Some(proof.drop)
            || evidence.continuation != evidence.blocks[3]
            || self.block != evidence.continuation
        {
            return Err(missing());
        }
        let parameters = &self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .parameters;
        let (source, _) = roles(parameters, evidence.source_slot, evidence.cursor_slot, span)?;
        let instructions = carrier::instructions(self.function, self.block).ok_or_else(missing)?;
        let source_end = unique(instructions.iter().enumerate().filter_map(
            |(index, instruction)| {
                (instruction.operation == Operation::BorrowEnd { loan: source }).then_some(index)
            },
        ))
        .ok_or_else(missing)?;
        let drops = instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| instruction.operation == Operation::Drop { owner: current })
            .collect::<Vec<_>>();
        let consumed = if proof.late_size.is_some() {
            if self.bindings.get(&symbol) != Some(&LoweredValue::Value(current))
                || !drops.is_empty()
            {
                return Err(missing());
            }
            None
        } else {
            let [(index, instruction)] = drops.as_slice() else {
                return Err(missing());
            };
            if *index <= source_end
                || self.bindings.contains_key(&symbol)
                || instruction.origin != Origin::Source(proof.drop.value_origin())
            {
                return Err(missing());
            }
            Some(Consumption {
                block: self.block,
                owner: current,
                instruction: instruction.id,
            })
        };
        let statement = match self.owned.cleanup_conditions().owner_value(proof.header) {
            Some(lang_frontend::ownership_checking::CleanupOwnerValue::IterationPhi {
                statement,
                symbol: actual,
                ..
            }) if *actual == symbol => *statement,
            _ => return Err(missing()),
        };
        Ok(Carrier {
            proof,
            statement,
            birth,
            current,
            continuation: self.block,
            source,
            consumed,
            size: None,
        })
    }

    /// This hook precedes the ordinary conditional-drop route. Only exact qualified
    /// late facts or proved absent columns are consumed; everything else is untouched.
    pub(in super::super) fn emit_local_map_drop(
        &mut self,
        fact: DropFact,
    ) -> Result<bool, LoweringError> {
        let span = fact.value_origin();
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let DropTarget::Named(root) = fact.target() else {
            return Ok(false);
        };
        if let Some(state) = self.map_iteration_state.locals.get(&root).cloned()
            && state.proof.drop == fact
        {
            let size = state.proof.late_size.ok_or_else(missing)?;
            if state.consumed.is_some()
                || fact.point() != DropPoint::CallReturn(size)
                || self.block != state.continuation
                || self.bindings.get(&root) != Some(&LoweredValue::Value(state.current))
                || self.borrow_bindings.contains_key(&root)
                || self
                    .temporaries
                    .values()
                    .any(|value| *value == state.current)
            {
                return Err(missing());
            }
            nested::transport(
                self.function,
                EntityId::Value(state.current),
                EntityId::Value(state.birth),
                span,
            )?;
            let instructions =
                carrier::instructions(self.function, self.block).ok_or_else(missing)?;
            let read = instructions.last().copied().ok_or_else(missing)?;
            let [EntityId::Value(result)] = read.results.as_slice() else {
                return Err(missing());
            };
            if read.operation
                != (Operation::MapSize {
                    owner: EntityId::Value(state.current),
                })
                || read.origin != Origin::Source(self.expression_span(size)?)
                || !instructions.iter().any(|instruction| {
                    instruction.operation == Operation::BorrowEnd { loan: state.source }
                })
            {
                return Err(missing());
            }
            let result = *result;
            self.bindings.remove(&root);
            let (instruction, _) = self.append(
                Operation::Drop {
                    owner: state.current,
                },
                Vec::new(),
                span,
            )?;
            let state = self
                .map_iteration_state
                .locals
                .get_mut(&root)
                .ok_or_else(missing)?;
            state.consumed = Some(Consumption {
                block: self.block,
                owner: state.current,
                instruction,
            });
            state.size = Some(result);
            return Ok(true);
        }
        let DropPoint::LoopExit(statement) = fact.point() else {
            return Ok(false);
        };
        let Some(descriptor) = self.typed.map_iteration(statement).copied() else {
            return Ok(false);
        };
        let qualified =
            qualify::qualify(self.parsed, self.names, self.typed, self.owned, descriptor)?;
        let Some(proof) = qualified.local else {
            return Ok(false);
        };
        let Some(dead) = unique(proof.dead.iter().filter(|dead| dead.drop == fact)).copied() else {
            return Ok(false);
        };
        self.validate_dead_local_map(dead, span)?;
        Ok(true)
    }

    pub(in super::super) fn validate_local_map_carriers(&self) -> Result<(), LoweringError> {
        for state in self.map_iteration_state.locals.values() {
            let span = state.proof.drop.value_origin();
            let missing = || error(LoweringErrorKind::MissingFact, span);
            if self.bindings.contains_key(&state.proof.root)
                || self.borrow_bindings.contains_key(&state.proof.root)
                || state.proof.late_size.is_some() != state.size.is_some()
            {
                return Err(missing());
            }
            validate_consumption(self.function, state, span)?;
            if let Some(size) = state.size {
                evidence::size_use(self, state, size, span)?;
            }
        }
        Ok(())
    }
}

fn validate_consumption(
    function: &Function,
    state: &Carrier,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let consumed = state.consumed.ok_or_else(missing)?;
    let drop = function
        .instruction(consumed.instruction)
        .ok_or_else(missing)?;
    if consumed.owner != state.current
        || drop.block != consumed.block
        || drop.operation
            != (Operation::Drop {
                owner: consumed.owner,
            })
        || drop.origin != Origin::Source(state.proof.drop.value_origin())
        || unique(function.instructions.iter().filter(|instruction| {
            instruction.operation
                == Operation::Drop {
                    owner: consumed.owner,
                }
        }))
        .map(|instruction| instruction.id)
            != Some(consumed.instruction)
    {
        return Err(missing());
    }
    nested::transport(
        function,
        EntityId::Value(consumed.owner),
        EntityId::Value(state.birth),
        span,
    )
}
