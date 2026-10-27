//! SSA construction: slot liveness decides block parameters; operands map mechanically.
use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::source::Span;

use super::body::{BlockRef, RAbort, ROp, RTerm, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{
        AbortReason, BlockId, Edge, EntityId, Function, LoanId, Operation, Origin, PlaceId,
        TerminatorKind, ValueId,
    },
};

struct Shape {
    reachable: Vec<BlockRef>,
    live_in: BTreeMap<BlockRef, BTreeSet<Slot>>,
}

fn reachable(body: &RefinedBody, span: Span) -> Result<Vec<BlockRef>, LoweringError> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![BlockRef(0)];
    while let Some(block) = stack.pop() {
        if !seen.insert(block) {
            continue;
        }
        let (terminator, _) = body.blocks[block.0]
            .terminator
            .as_ref()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        stack.extend(terminator.successors().into_iter().rev());
    }
    Ok(seen.into_iter().collect())
}

/// Upward-exposed uses and definitions of one block.
fn uses_and_defs(body: &RefinedBody, block: BlockRef) -> (BTreeSet<Slot>, BTreeSet<Slot>) {
    let data = &body.blocks[block.0];
    let mut defs: BTreeSet<Slot> = data.view.into_iter().collect();
    let mut uses = BTreeSet::new();
    for statement in &data.statements {
        for operand in statement.op.operands() {
            if !defs.contains(&operand) {
                uses.insert(operand);
            }
        }
        defs.extend(statement.results.iter().copied());
    }
    if let Some((terminator, _)) = &data.terminator {
        for operand in terminator.operands() {
            if !defs.contains(&operand) {
                uses.insert(operand);
            }
        }
    }
    (uses, defs)
}

/// Abort blocks reached from a two-way branch, mapped to the branch's other target.
fn abort_siblings(body: &RefinedBody, reachable: &[BlockRef]) -> BTreeMap<BlockRef, BlockRef> {
    let mut siblings = BTreeMap::new();
    for block in reachable {
        let Some((terminator, _)) = &body.blocks[block.0].terminator else {
            continue;
        };
        if let [first, second] = terminator.successors()[..] {
            for (abort, other) in [(first, second), (second, first)] {
                if matches!(body.blocks[abort.0].terminator, Some((RTerm::Abort(_), _))) {
                    siblings.insert(abort, other);
                }
            }
        }
    }
    siblings
}

/// Folds a block into its `Goto` predecessor when that predecessor is its only one, so
/// no SSA block merely forwards its parameters.
fn merge_straight_lines(body: &RefinedBody, span: Span) -> Result<RefinedBody, LoweringError> {
    let mut body = body.clone();
    loop {
        let reachable = reachable(&body, span)?;
        let mut predecessors: BTreeMap<BlockRef, usize> = BTreeMap::new();
        for block in &reachable {
            let (terminator, _) = body.blocks[block.0].terminator.as_ref().expect("reachable");
            for successor in terminator.successors() {
                *predecessors.entry(successor).or_default() += 1;
            }
        }
        let candidate = reachable.iter().find_map(|block| {
            match body.blocks[block.0].terminator.as_ref().expect("reachable") {
                (RTerm::Goto(successor), _)
                    if successor != block
                        && successor.0 != 0
                        && predecessors.get(successor) == Some(&1)
                        && body.blocks[successor.0].view.is_none() =>
                {
                    Some((*block, *successor))
                }
                _ => None,
            }
        });
        let Some((block, successor)) = candidate else {
            return Ok(body);
        };
        let folded = std::mem::take(&mut body.blocks[successor.0]);
        let target = &mut body.blocks[block.0];
        target.statements.extend(folded.statements);
        target.terminator = folded.terminator;
    }
}

fn shape(body: &RefinedBody, span: Span) -> Result<Shape, LoweringError> {
    let reachable = reachable(body, span)?;
    let local: BTreeMap<_, _> = reachable
        .iter()
        .map(|block| (*block, uses_and_defs(body, *block)))
        .collect();
    let siblings = abort_siblings(body, &reachable);
    let mut live_in: BTreeMap<BlockRef, BTreeSet<Slot>> = BTreeMap::new();
    loop {
        let mut changed = false;
        for block in reachable.iter().rev() {
            let (uses, defs) = &local[block];
            let mut live = uses.clone();
            if let Some(sibling) = siblings.get(block) {
                // An abort edge carries every owner the other edge continues with, so
                // nothing is left unaccounted at the branch.
                live.extend(live_in.get(sibling).into_iter().flatten().copied());
            }
            let (terminator, _) = body.blocks[block.0].terminator.as_ref().expect("reachable");
            for successor in terminator.successors() {
                for slot in live_in.get(&successor).into_iter().flatten() {
                    if !defs.contains(slot) {
                        live.insert(*slot);
                    }
                }
            }
            if live_in.get(block) != Some(&live) {
                live_in.insert(*block, live);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let parameters: BTreeSet<_> = body.parameters.iter().copied().collect();
    if !live_in[&BlockRef(0)].is_subset(&parameters) {
        // A slot is read on some path before any definition reaches it.
        return Err(error(LoweringErrorKind::InvalidModel, span));
    }
    Ok(Shape { reachable, live_in })
}

fn value(entity: EntityId, span: Span) -> Result<ValueId, LoweringError> {
    match entity {
        EntityId::Value(value) => Ok(value),
        _ => Err(error(LoweringErrorKind::InvalidModel, span)),
    }
}

fn loan(entity: EntityId, span: Span) -> Result<LoanId, LoweringError> {
    match entity {
        EntityId::Loan(loan) => Ok(loan),
        _ => Err(error(LoweringErrorKind::InvalidModel, span)),
    }
}

fn place(entity: EntityId, span: Span) -> Result<PlaceId, LoweringError> {
    match entity {
        EntityId::Place(place) => Ok(place),
        _ => Err(error(LoweringErrorKind::InvalidModel, span)),
    }
}

struct Emitter<'f> {
    function: &'f mut Function,
    current: BTreeMap<Slot, EntityId>,
}

impl Emitter<'_> {
    fn get(&self, slot: Slot, span: Span) -> Result<EntityId, LoweringError> {
        self.current
            .get(&slot)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))
    }

    fn value(&self, slot: Slot, span: Span) -> Result<ValueId, LoweringError> {
        value(self.get(slot, span)?, span)
    }

    fn operation(&self, op: &ROp, span: Span) -> Result<Operation, LoweringError> {
        Ok(match op {
            ROp::Alias { .. } => return Err(error(LoweringErrorKind::InvalidModel, span)),
            ROp::Constant(constant) => Operation::Constant(constant.clone()),
            ROp::StringLiteral { string, bytes } => Operation::StringLiteral {
                string: *string,
                bytes: bytes.clone(),
            },
            ROp::PrintString { value } => Operation::PrintString {
                value: loan(self.get(*value, span)?, span)?,
            },
            ROp::ReadLoan { loan: slot } => Operation::Read {
                source: crate::ssa::model::PlaceAccess::Loan(loan(self.get(*slot, span)?, span)?),
            },
            ROp::StringConcat { left, right } => Operation::StringConcat {
                left: self.get(*left, span)?,
                right: self.get(*right, span)?,
            },
            ROp::StringEqual { left, right } => Operation::StringEqual {
                left: self.get(*left, span)?,
                right: self.get(*right, span)?,
            },
            ROp::RootPlace { owner } => Operation::RootPlace {
                owner: self.value(*owner, span)?,
            },
            ROp::BorrowBegin { place: slot, kind } => Operation::BorrowBegin {
                place: place(self.get(*slot, span)?, span)?,
                kind: *kind,
            },
            ROp::BorrowEnd { loan: slot } => Operation::BorrowEnd {
                loan: loan(self.get(*slot, span)?, span)?,
            },
            ROp::Drop { owner } => Operation::Drop {
                owner: self.value(*owner, span)?,
            },
            ROp::Compare {
                operator,
                left,
                right,
            } => Operation::Compare {
                operator: *operator,
                left: self.value(*left, span)?,
                right: self.value(*right, span)?,
            },
            ROp::BooleanNot { operand } => Operation::BooleanNot {
                operand: self.value(*operand, span)?,
            },
            ROp::CheckedArithmetic {
                operator,
                left,
                right,
            } => Operation::CheckedArithmetic {
                operator: *operator,
                left: self.value(*left, span)?,
                right: self.value(*right, span)?,
            },
            ROp::DirectCall {
                callee,
                receiver,
                arguments,
            } => Operation::DirectCall {
                callee: *callee,
                receiver: receiver.map(|slot| self.get(slot, span)).transpose()?,
                arguments: arguments
                    .iter()
                    .map(|slot| self.get(*slot, span))
                    .collect::<Result<_, _>>()?,
            },
            ROp::NullableWrap { nullable, owner } => Operation::NullableWrap {
                nullable: *nullable,
                owner: self.value(*owner, span)?,
            },
            ROp::NullableNull { nullable } => Operation::NullableNull {
                nullable: *nullable,
            },
            ROp::NullableIsNull { owner } => Operation::NullableIsNull {
                owner: self.value(*owner, span)?,
            },
            ROp::NullableTake { owner, proof } => Operation::NullableTake {
                owner: self.value(*owner, span)?,
                proof: loan(self.get(*proof, span)?, span)?,
            },
            ROp::AggregateConstruct { aggregate, fields } => Operation::AggregateConstruct {
                aggregate: *aggregate,
                fields: fields
                    .iter()
                    .map(|slot| self.value(*slot, span))
                    .collect::<Result<_, _>>()?,
            },
            ROp::HeapAllocate { owner, payload } => Operation::HeapAllocate {
                owner: *owner,
                payload: self.value(*payload, span)?,
            },
            ROp::HeapFieldRead { receiver, field } => Operation::HeapFieldRead {
                receiver: loan(self.get(*receiver, span)?, span)?,
                field: *field,
            },
            ROp::TaggedConstruct {
                tagged,
                variant,
                payload,
            } => Operation::TaggedConstruct {
                tagged: *tagged,
                variant: *variant,
                payload: self.value(*payload, span)?,
            },
            ROp::TaggedDiscriminant { owner } => Operation::TaggedDiscriminant {
                owner: self.value(*owner, span)?,
            },
            ROp::TaggedPayloadPlace { owner, variant } => Operation::TaggedPayloadPlace {
                owner: self.value(*owner, span)?,
                variant: *variant,
            },
            ROp::FieldPlace { base, field } => Operation::FieldPlace {
                base: place(self.get(*base, span)?, span)?,
                field: *field,
            },
            ROp::SharedReborrow { source } => Operation::SharedReborrow {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::RootPlaceTake { owner, place: slot } => Operation::RootPlaceTake {
                owner: self.value(*owner, span)?,
                place: place(self.get(*slot, span)?, span)?,
            },
            ROp::HeapFieldReplace {
                receiver,
                field,
                value,
            } => Operation::HeapFieldReplace {
                receiver: loan(self.get(*receiver, span)?, span)?,
                field: *field,
                value: self.value(*value, span)?,
            },
            ROp::LoanMutate { loan: slot, value } => Operation::LoanMutate {
                loan: loan(self.get(*slot, span)?, span)?,
                value: self.value(*value, span)?,
            },
            ROp::ReadPlace { place: slot } => Operation::Read {
                source: crate::ssa::model::PlaceAccess::Place(place(self.get(*slot, span)?, span)?),
            },
        })
    }
}

/// Emits `body` into `function`, whose `entry` block already holds `entry_parameters`.
pub(super) fn emit(
    body: &RefinedBody,
    function: &mut Function,
    entry: BlockId,
    entry_parameters: &[EntityId],
    span: Span,
) -> Result<(), LoweringError> {
    let invalid = |_| error(LoweringErrorKind::InvalidModel, span);
    let merged = merge_straight_lines(body, span)?;
    let body = &merged;
    let Shape { reachable, live_in } = shape(body, span)?;
    if entry_parameters.len() != body.parameters.len() {
        return Err(error(LoweringErrorKind::InvalidModel, span));
    }
    // Parameter order: live slots by number, then the non-null proof view.
    let mut parameters: BTreeMap<BlockRef, Vec<Slot>> = BTreeMap::new();
    let mut blocks: BTreeMap<BlockRef, BlockId> = BTreeMap::new();
    let mut entities: BTreeMap<BlockRef, Vec<EntityId>> = BTreeMap::new();
    for block in &reachable {
        if block.0 == 0 {
            parameters.insert(*block, body.parameters.clone());
            blocks.insert(*block, entry);
            entities.insert(*block, entry_parameters.to_vec());
            continue;
        }
        let view = body.blocks[block.0].view;
        let mut slots: Vec<Slot> = live_in[block]
            .iter()
            .copied()
            .filter(|slot| Some(*slot) != view)
            .collect();
        slots.extend(view);
        let types = slots.iter().map(|slot| body.slots[slot.0]).collect();
        let origin = body.blocks[block.0].origin.unwrap_or(span);
        let id = function
            .add_block(types, Origin::Source(origin))
            .map_err(invalid)?;
        let block_parameters = function
            .block(id)
            .map(|data| data.parameters.clone())
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        entities.insert(*block, block_parameters);
        parameters.insert(*block, slots);
        blocks.insert(*block, id);
    }
    let mut emitter = Emitter {
        function,
        current: BTreeMap::new(),
    };
    for block in &reachable {
        let target = blocks[block];
        emitter.current = parameters[block]
            .iter()
            .copied()
            .zip(entities[block].iter().copied())
            .collect();
        for statement in &body.blocks[block.0].statements {
            if let (ROp::Alias { source }, [result]) = (&statement.op, statement.results.as_slice())
            {
                let entity = emitter.get(*source, statement.span)?;
                emitter.current.insert(*result, entity);
                continue;
            }
            let operation = emitter.operation(&statement.op, statement.span)?;
            let types = statement
                .results
                .iter()
                .map(|slot| body.slots[slot.0])
                .collect();
            let (_, results) = emitter
                .function
                .append_instruction(target, operation, types, Origin::Source(statement.span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, statement.span))?;
            for (slot, entity) in statement.results.iter().zip(results) {
                emitter.current.insert(*slot, entity);
            }
        }
        let (terminator, term_span) = body.blocks[block.0].terminator.as_ref().expect("reachable");
        let origin = Origin::Source(*term_span);
        let edge = |emitter: &Emitter<'_>, successor: BlockRef| -> Result<Edge, LoweringError> {
            let view = body.blocks[successor.0].view;
            Ok(Edge {
                target: blocks[&successor],
                arguments: parameters[&successor]
                    .iter()
                    .filter(|slot| Some(**slot) != view)
                    .map(|slot| emitter.get(*slot, *term_span))
                    .collect::<Result<_, _>>()?,
            })
        };
        let model_error = |_| error(LoweringErrorKind::InvalidModel, *term_span);
        match terminator {
            RTerm::Abort(RAbort::CheckedArithmetic { operator, divisor }) => {
                let divisor = emitter.value(*divisor, *term_span)?;
                emitter
                    .function
                    .set_abort_terminator(
                        target,
                        AbortReason::CheckedArithmetic {
                            operator: *operator,
                            divisor,
                        },
                        origin,
                    )
                    .map_err(model_error)?;
                continue;
            }
            RTerm::Abort(reason @ (RAbort::NullAssertion | RAbort::UnmatchedWhen)) => {
                let text = match reason {
                    RAbort::NullAssertion => "null assertion failed",
                    _ => "internal error: when subject matched no branch",
                };
                emitter
                    .function
                    .set_abort_terminator(target, AbortReason::Text(text.to_owned()), origin)
                    .map_err(model_error)?;
                continue;
            }
            RTerm::Goto(successor) => {
                let kind = TerminatorKind::Branch(edge(&emitter, *successor)?);
                emitter
                    .function
                    .set_terminator(target, kind, origin)
                    .map_err(model_error)?;
            }
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            } => {
                let kind = TerminatorKind::Conditional {
                    condition: emitter.value(*condition, *term_span)?,
                    when_true: edge(&emitter, *when_true)?,
                    when_false: edge(&emitter, *when_false)?,
                };
                emitter
                    .function
                    .set_terminator(target, kind, origin)
                    .map_err(model_error)?;
            }
            RTerm::NullBranch {
                owner,
                when_null,
                when_non_null,
            } => {
                let view = entities[when_non_null]
                    .last()
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, *term_span))?;
                let kind = TerminatorKind::NullableBranch {
                    owner: emitter.value(*owner, *term_span)?,
                    when_null: edge(&emitter, *when_null)?,
                    when_non_null: edge(&emitter, *when_non_null)?,
                    view: loan(view, *term_span)?,
                };
                emitter
                    .function
                    .set_terminator(target, kind, origin)
                    .map_err(model_error)?;
            }
            RTerm::Return(result) => {
                let values = result
                    .map(|slot| emitter.value(slot, *term_span))
                    .transpose()?
                    .into_iter()
                    .collect();
                let kind = TerminatorKind::Return { values };
                emitter
                    .function
                    .set_terminator(target, kind, origin)
                    .map_err(model_error)?;
            }
        }
    }
    Ok(())
}
