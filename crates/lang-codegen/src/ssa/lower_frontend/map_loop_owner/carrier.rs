//! Check actual SSA owner provenance independently of the source binding name.
use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, control::LinearBindings,
    error,
};
use super::{OwnerPlan, owner_plan, unique};
use crate::ssa::model::{
    BlockId, Definition, Edge, EntityId, Function, Operation, Origin, TerminatorKind, ValueId,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{CleanupOwnerValue, DropFact, IterationPhiIncomingKind},
    source::Span,
};

struct Carrier {
    plan: OwnerPlan,
    preheader: BlockId,
    header_block: BlockId,
    body_block: BlockId,
    exhaustion_block: BlockId,
    slot: usize,
    entry: ValueId,
    header: ValueId,
    body: ValueId,
    exhaustion: ValueId,
    exit_live_checked: bool,
    drops: Vec<(DropFact, BlockId, ValueId)>,
    break_exit: Option<(BlockId, ValueId)>,
    return_current: Option<(BlockId, ValueId)>,
    return_exit: Option<BlockId>,
    backedges: Vec<(IterationPhiIncomingKind, BlockId, ValueId)>,
    merged: Option<ValueId>,
    merge_checked: bool,
}

#[derive(Default)]
pub(in super::super) struct Carriers {
    parameter_seeds: BTreeMap<SymbolId, ValueId>,
    loops: Vec<Carrier>,
}

impl Carriers {
    pub(in super::super) fn new(bindings: &BTreeMap<SymbolId, LoweredValue>) -> Self {
        Self {
            parameter_seeds: bindings
                .iter()
                .filter_map(|(&symbol, binding)| match binding {
                    LoweredValue::Value(value) => Some((symbol, *value)),
                    _ => None,
                })
                .collect(),
            loops: Vec::new(),
        }
    }
}

impl ExpressionLowerer<'_> {
    pub(in super::super) fn record_map_loop_carriers(
        &mut self,
        statement: StatementId,
        blocks: [BlockId; 4],
        carried: &LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        for fact in self.owned.drops() {
            let Some(plan) = owner_plan(self.typed, self.owned, *fact)
                .filter(|plan| plan.statement == statement && plan.drop == *fact)
            else {
                continue;
            };
            if self.thunk_expression.is_some()
                || self
                    .map_loop_owners
                    .loops
                    .iter()
                    .any(|carrier| carrier.plan.header == plan.header)
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let (slot, binding) = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, slot)| slot.symbol == Some(plan.symbol)),
            )
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let EntityId::Value(entry) = binding.source else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let parameter = |block| {
                self.function
                    .block(block)
                    .and_then(|block| block.parameters.get(slot))
                    .and_then(|entity| match entity {
                        EntityId::Value(value) => Some(*value),
                        _ => None,
                    })
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            };
            let [preheader, header_block, body_block, exhaustion_block] = blocks;
            let [header, body, exhaustion] = [
                parameter(header_block)?,
                parameter(body_block)?,
                parameter(exhaustion_block)?,
            ];
            if [header, body, exhaustion].into_iter().any(|value| {
                self.function
                    .entity(EntityId::Value(value))
                    .map(|entity| entity.ty)
                    != Some(binding.ty)
            }) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            self.map_loop_owners.loops.push(Carrier {
                plan,
                preheader,
                header_block,
                body_block,
                exhaustion_block,
                slot,
                entry,
                header,
                body,
                exhaustion,
                exit_live_checked: false,
                drops: Vec::new(),
                break_exit: None,
                return_current: None,
                return_exit: None,
                backedges: Vec::new(),
                merged: None,
                merge_checked: false,
            });
        }
        Ok(())
    }

    // Called after ordered cleanup and argument assembly, while bindings still
    // identify the actual current owner in this exact predecessor block.
    pub(in super::super) fn record_map_loop_backedge(
        &mut self,
        statement: StatementId,
        kind: IterationPhiIncomingKind,
        edge: &Edge,
        span: Span,
    ) -> Result<(), LoweringError> {
        for carrier in self
            .map_loop_owners
            .loops
            .iter_mut()
            .filter(|carrier| carrier.plan.statement == statement)
        {
            let fail = || error(LoweringErrorKind::MissingFact, span);
            let expected = match kind {
                IterationPhiIncomingKind::Fallthrough => true,
                IterationPhiIncomingKind::Continue(expression) => {
                    carrier.plan.continue_expression == Some(expression)
                }
                _ => false,
            };
            let Some(LoweredValue::Value(current)) = self.bindings.get(&carrier.plan.symbol) else {
                return Err(fail());
            };
            if !expected
                || carrier.exit_live_checked
                || edge.target != carrier.header_block
                || edge.arguments.get(carrier.slot) != Some(&EntityId::Value(*current))
                || carrier
                    .backedges
                    .iter()
                    .any(|(recorded, block, _)| *recorded == kind || *block == self.block)
                || carrier
                    .drops
                    .iter()
                    .any(|(_, block, _)| *block == self.block)
            {
                return Err(fail());
            }
            carrier.backedges.push((kind, self.block, *current));
        }
        Ok(())
    }

    pub(in super::super) fn finish_map_loop_carriers(
        &mut self,
        statement: StatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for carrier in self
            .map_loop_owners
            .loops
            .iter_mut()
            .filter(|carrier| carrier.plan.statement == statement)
        {
            // The SSA exhaustion parameter still denotes Header until the ordered cleanup ends.
            let binding = self.bindings.get(&carrier.plan.symbol).copied();
            let valid = if carrier.plan.exit_live {
                binding == Some(LoweredValue::Value(carrier.exhaustion))
                    && carrier
                        .drops
                        .iter()
                        .all(|(fact, _, _)| Some(*fact) == carrier.plan.return_drop)
            } else {
                binding.is_none()
                    && carrier
                        .drops
                        .iter()
                        .filter(|(fact, _, _)| *fact == carrier.plan.drop)
                        .copied()
                        .collect::<Vec<_>>()
                        == [(
                            carrier.plan.drop,
                            carrier.exhaustion_block,
                            carrier.exhaustion,
                        )]
            };
            if carrier.exit_live_checked || !valid {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            carrier.exit_live_checked = true;
        }
        Ok(())
    }

    pub(in super::super) fn finish_map_loop_break(
        &mut self,
        statement: StatementId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for carrier in self
            .map_loop_owners
            .loops
            .iter_mut()
            .filter(|carrier| carrier.plan.statement == statement)
        {
            let fail = || error(LoweringErrorKind::MissingFact, span);
            if carrier.plan.break_expression != Some(expression)
                || carrier.break_exit.is_some()
                || carrier.exit_live_checked
            {
                return Err(fail());
            }
            let value = if carrier.plan.exit_live {
                if !carrier.drops.is_empty() {
                    return Err(fail());
                }
                match self.bindings.get(&carrier.plan.symbol) {
                    Some(LoweredValue::Value(value)) => *value,
                    _ => return Err(fail()),
                }
            } else {
                if self.bindings.contains_key(&carrier.plan.symbol) {
                    return Err(fail());
                }
                let (_, block, value) = *unique(
                    carrier
                        .drops
                        .iter()
                        .filter(|(fact, _, _)| Some(*fact) == carrier.plan.break_drop),
                )
                .ok_or_else(fail)?;
                if block != self.block {
                    return Err(fail());
                }
                value
            };
            carrier.break_exit = Some((self.block, value));
        }
        Ok(())
    }

    pub(in super::super) fn finish_map_loop_merge(
        &mut self,
        statement: StatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for carrier in self
            .map_loop_owners
            .loops
            .iter_mut()
            .filter(|carrier| carrier.plan.statement == statement)
        {
            let fail = || error(LoweringErrorKind::MissingFact, span);
            if !carrier.exit_live_checked
                || carrier.merge_checked
                || carrier.break_exit.is_some() != carrier.plan.break_expression.is_some()
            {
                return Err(fail());
            }
            if carrier.plan.exit_live {
                let Some(LoweredValue::Value(value)) = self.bindings.get(&carrier.plan.symbol)
                else {
                    return Err(fail());
                };
                if let Some((break_block, break_value)) = carrier.break_exit {
                    let Some(Definition::BlockParameter { block, index }) = self
                        .function
                        .entity(EntityId::Value(*value))
                        .map(|entity| entity.definition)
                    else {
                        return Err(fail());
                    };
                    let mut inputs = incoming(self.function, block, index).ok_or_else(fail)?;
                    let mut expected = vec![
                        (break_block, break_value),
                        (carrier.exhaustion_block, carrier.exhaustion),
                    ];
                    inputs.sort();
                    expected.sort();
                    if block != self.block || inputs != expected {
                        return Err(fail());
                    }
                } else if *value != carrier.exhaustion {
                    return Err(fail());
                }
                carrier.merged = Some(*value);
            } else if self.bindings.contains_key(&carrier.plan.symbol) {
                return Err(fail());
            }
            carrier.merge_checked = true;
        }
        Ok(())
    }

    // Snapshot after the return operand has completed: it may itself update the Map.
    pub(in super::super) fn prepare_map_loop_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for carrier in self
            .map_loop_owners
            .loops
            .iter_mut()
            .filter(|carrier| carrier.plan.return_expression == Some(expression))
        {
            let fail = || error(LoweringErrorKind::MissingFact, span);
            let Some(LoweredValue::Value(current)) = self.bindings.get(&carrier.plan.symbol) else {
                return Err(fail());
            };
            if carrier.return_current.is_some()
                || carrier.return_exit.is_some()
                || carrier.exit_live_checked
                || carrier.merge_checked
            {
                return Err(fail());
            }
            carrier.return_current = Some((self.block, *current));
        }
        Ok(())
    }

    // Ordered cleanup can change the current block; record the actual Return terminator.
    pub(in super::super) fn finish_map_loop_return(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for carrier in self
            .map_loop_owners
            .loops
            .iter_mut()
            .filter(|carrier| carrier.plan.return_expression == Some(expression))
        {
            let fail = || error(LoweringErrorKind::MissingFact, span);
            let (_, _, value) = *unique(
                carrier
                    .drops
                    .iter()
                    .filter(|(fact, _, _)| Some(*fact) == carrier.plan.return_drop),
            )
            .ok_or_else(fail)?;
            if carrier.return_exit.is_some()
                || self.bindings.contains_key(&carrier.plan.symbol)
                || !forwarded(
                    self.function,
                    value,
                    carrier.return_current.ok_or_else(fail)?.1,
                )
                || !matches!(
                    self.function
                        .block(self.block)
                        .and_then(|block| block.terminator.as_ref())
                        .map(|term| &term.kind),
                    Some(TerminatorKind::Return { .. })
                )
            {
                return Err(fail());
            }
            carrier.return_exit = Some(self.block);
        }
        Ok(())
    }

    pub(in super::super) fn record_map_loop_drop(
        &mut self,
        fact: DropFact,
        value: ValueId,
    ) -> Result<(), LoweringError> {
        if fact.condition().is_none()
            || super::super::constant_presence::constant_drop(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                fact,
            )
        {
            return Ok(());
        }
        let plan = owner_plan(self.typed, self.owned, fact)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, fact.value_origin()))?;
        let carrier = self
            .map_loop_owners
            .loops
            .iter_mut()
            .find(|carrier| {
                carrier.plan.header == plan.header
                    && carrier.plan.exit == plan.exit
                    && (carrier.plan.drop == fact
                        || carrier.plan.break_drop == Some(fact)
                        || carrier.plan.return_drop == Some(fact))
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
        let valid_phase = if Some(fact) == carrier.plan.return_drop {
            !carrier.exit_live_checked
                && carrier.return_exit.is_none()
                && carrier
                    .return_current
                    .is_some_and(|(_, current)| forwarded(self.function, value, current))
        } else if plan.exit_live {
            carrier.merge_checked && fact == carrier.plan.drop
        } else if Some(fact) == carrier.plan.break_drop {
            carrier.break_exit.is_none() && !carrier.exit_live_checked
        } else {
            !carrier.exit_live_checked
                && self.block == carrier.exhaustion_block
                && value == carrier.exhaustion
        };
        if !valid_phase
            || carrier
                .drops
                .iter()
                .any(|(recorded, _, _)| *recorded == fact)
        {
            return Err(error(LoweringErrorKind::MissingFact, fact.value_origin()));
        }
        carrier.drops.push((fact, self.block, value));
        Ok(())
    }

    pub(in super::super) fn validate_map_loop_carriers(&self) -> Result<(), LoweringError> {
        for carrier in &self.map_loop_owners.loops {
            let span = carrier.plan.drop.value_origin();
            let fail = || error(LoweringErrorKind::MissingFact, span);
            if !carrier.merge_checked
                || carrier.drops.len()
                    != 1 + usize::from(carrier.plan.break_drop.is_some())
                        + usize::from(carrier.plan.return_drop.is_some())
                || carrier.return_exit.is_some() != carrier.plan.return_expression.is_some()
            {
                return Err(fail());
            }
            let seed = match self
                .owned
                .cleanup_conditions()
                .owner_value(carrier.plan.seed)
            {
                Some(CleanupOwnerValue::Expression { expression, origin }) => {
                    // A constructor is a real owner definition; same-typed operands are not seeds.
                    if self.typed.map_construction(*expression).is_none()
                        || self.expression_span(*expression)? != *origin
                    {
                        return Err(error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    unique(self.function.instructions.iter().filter_map(|instruction| {
                        if instruction.origin == Origin::Source(*origin)
                            && matches!(instruction.operation, Operation::MapConstruct { .. })
                        {
                            match instruction.results.first() {
                                Some(EntityId::Value(value)) => Some(*value),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    }))
                    .ok_or_else(fail)?
                }
                Some(CleanupOwnerValue::Parameter { symbol, .. }) => *self
                    .map_loop_owners
                    .parameter_seeds
                    .get(symbol)
                    .ok_or_else(fail)?,
                _ => return Err(fail()),
            };
            if !lineage(self.function, carrier.entry, seed) {
                return Err(fail());
            }
            let mut header_inputs =
                incoming(self.function, carrier.header_block, carrier.slot).ok_or_else(fail)?;
            let mut expected = vec![(carrier.preheader, carrier.entry)];
            let fallthrough = unique(
                carrier
                    .backedges
                    .iter()
                    .filter(|(kind, _, _)| *kind == IterationPhiIncomingKind::Fallthrough),
            )
            .ok_or_else(fail)?;
            expected.push((fallthrough.1, fallthrough.2));
            if let Some(expression) = carrier.plan.continue_expression {
                let continued = unique(carrier.backedges.iter().filter(|(kind, _, _)| {
                    *kind == IterationPhiIncomingKind::Continue(expression)
                }))
                .ok_or_else(fail)?;
                expected.push((continued.1, continued.2));
            }
            if carrier.backedges.len() + 1 != expected.len()
                || carrier.backedges.iter().any(|(_, block, value)| {
                    *block == carrier.preheader || !lineage(self.function, *value, carrier.body)
                })
            {
                return Err(fail());
            }
            header_inputs.sort();
            expected.sort();
            if header_inputs != expected
                || incoming(self.function, carrier.body_block, carrier.slot).ok_or_else(fail)?
                    != [(carrier.header_block, carrier.header)]
                || incoming(self.function, carrier.exhaustion_block, carrier.slot)
                    .ok_or_else(fail)?
                    != [(carrier.header_block, carrier.header)]
            {
                return Err(fail());
            }
            if let Some((_, value)) = carrier.break_exit
                && !lineage(self.function, value, carrier.body)
            {
                return Err(fail());
            }
            for (fact, block, value) in &carrier.drops {
                if Some(*fact) == carrier.plan.return_drop {
                    let (prepared, current) = carrier.return_current.ok_or_else(fail)?;
                    let terminal = carrier.return_exit.ok_or_else(fail)?;
                    if !lineage(self.function, current, carrier.body)
                        || !forwarded(self.function, *value, current)
                        || !cleanup_reaches(self.function, prepared, *block)
                        || !cleanup_reaches(self.function, *block, terminal)
                        || !matches!(self.function.block(terminal).and_then(|block| block.terminator.as_ref()),
                            Some(term) if term.origin == Origin::Source(self.expression_span(carrier.plan.return_expression.ok_or_else(fail)?)?)
                                && matches!(&term.kind, TerminatorKind::Return { values } if !values.contains(value)))
                        || self.function.instructions.iter().filter(|instruction| {
                            instruction.block == *block
                                && instruction.origin == Origin::Source(fact.value_origin())
                                && matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }).count() != 1
                    {
                        return Err(fail());
                    }
                } else if carrier.plan.exit_live {
                    if !lineage(self.function, *value, carrier.merged.ok_or_else(fail)?) {
                        return Err(fail());
                    }
                } else if Some(*fact) == carrier.plan.break_drop {
                    if carrier.break_exit != Some((*block, *value)) {
                        return Err(fail());
                    }
                } else if (*block, *value) != (carrier.exhaustion_block, carrier.exhaustion) {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}

fn incoming(function: &Function, target: BlockId, index: usize) -> Option<Vec<(BlockId, ValueId)>> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges: Vec<&Edge> = match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Branch(edge)) => vec![edge],
                Some(TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                }) => vec![when_true, when_false],
                Some(TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                }) => vec![when_null, when_non_null],
                _ => Vec::new(),
            };
            edges
                .into_iter()
                .filter(move |edge| edge.target == target)
                .map(move |edge| match edge.arguments.get(index) {
                    Some(EntityId::Value(value)) => Some((block.id, *value)),
                    _ => None,
                })
        })
        .collect()
}

fn lineage(function: &Function, value: ValueId, seed: ValueId) -> bool {
    fn visit(
        function: &Function,
        value: ValueId,
        seed: ValueId,
        visiting: &mut BTreeSet<ValueId>,
        proven: &mut BTreeSet<ValueId>,
    ) -> bool {
        if value == seed || proven.contains(&value) {
            return true;
        }
        // Unanchored cycles are not evidence. The expected loop is cut only at its checked carrier.
        if !visiting.insert(value) {
            return false;
        }
        let valid = match function
            .entity(EntityId::Value(value))
            .map(|entity| entity.definition)
        {
            Some(Definition::InstructionResult {
                instruction,
                index: 0,
            }) => {
                match function
                    .instruction(instruction)
                    .map(|instruction| &instruction.operation)
                {
                    Some(Operation::MapPut { owner, .. } | Operation::MapRemove { owner, .. }) => {
                        visit(function, *owner, seed, visiting, proven)
                    }
                    _ => false,
                }
            }
            Some(Definition::BlockParameter { block, index }) => incoming(function, block, index)
                .is_some_and(|sources| {
                    !sources.is_empty()
                        && sources
                            .into_iter()
                            .all(|(_, value)| visit(function, value, seed, visiting, proven))
                }),
            _ => false,
        };
        visiting.remove(&value);
        if valid {
            proven.insert(value);
        }
        valid
    }
    visit(
        function,
        value,
        seed,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
}

// Cleanup may forward an owner through block parameters, but must not substitute
// an older or newer Map update for the exact value captured after the operand.
fn forwarded(function: &Function, value: ValueId, expected: ValueId) -> bool {
    fn visit(function: &Function, value: ValueId, expected: ValueId, depth: usize) -> bool {
        if value == expected {
            return true;
        }
        if depth > function.blocks.len() {
            return false;
        }
        let Some(Definition::BlockParameter { block, index }) = function
            .entity(EntityId::Value(value))
            .map(|entity| entity.definition)
        else {
            return false;
        };
        incoming(function, block, index).is_some_and(|inputs| {
            !inputs.is_empty()
                && inputs
                    .into_iter()
                    .all(|(_, source)| visit(function, source, expected, depth + 1))
        })
    }
    visit(function, value, expected, 0)
}

// Every cleanup successor must converge to the recorded point without a cycle or
// another terminal. No return record can be borrowed from an unrelated CFG path.
fn cleanup_reaches(function: &Function, start: BlockId, target: BlockId) -> bool {
    fn visit(
        function: &Function,
        block: BlockId,
        target: BlockId,
        seen: &mut BTreeSet<BlockId>,
    ) -> bool {
        if block == target {
            return true;
        }
        if !seen.insert(block) {
            return false;
        }
        let valid = match function
            .block(block)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        {
            Some(TerminatorKind::Branch(edge)) => visit(function, edge.target, target, seen),
            Some(TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            }) => {
                visit(function, when_true.target, target, seen)
                    && visit(function, when_false.target, target, seen)
            }
            Some(TerminatorKind::NullableBranch {
                when_null,
                when_non_null,
                ..
            }) => {
                visit(function, when_null.target, target, seen)
                    && visit(function, when_non_null.target, target, seen)
            }
            _ => false,
        };
        seen.remove(&block);
        valid
    }
    visit(function, start, target, &mut BTreeSet::new())
}
