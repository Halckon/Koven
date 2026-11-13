//! SSA construction: slot liveness decides block parameters; operands map mechanically.
mod folding;
mod statements;
use super::body::{BlockRef, RAbort, ROp, RTerm, RefinedBody, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{
        AbortReason, BlockId, Edge, EntityId, Function, LoanId, Operation, Origin, PlaceId,
        TerminatorKind, ValueId,
    },
};
use folding::merge_straight_lines;
use lang_frontend::source::Span;
use std::collections::{BTreeMap, BTreeSet};

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
    let mut uses: BTreeSet<Slot> = data
        .pinned
        .iter()
        .copied()
        .filter(|slot| !defs.contains(slot))
        .collect();
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

fn abort_blocks(body: &RefinedBody, reachable: &[BlockRef]) -> BTreeSet<BlockRef> {
    let mut aborts: BTreeSet<BlockRef> = reachable
        .iter()
        .copied()
        .filter(|block| {
            let term = &body.blocks[block.0].terminator;
            matches!(term, Some((RTerm::Abort(_) | RTerm::Unreachable, _)))
        })
        .collect();
    while reachable.iter().rev().any(|&block| {
        !aborts.contains(&block)
            && body.blocks[block.0]
                .terminator
                .as_ref()
                .is_some_and(|(term, _)| {
                    let succs = term.successors();
                    !succs.is_empty() && succs.iter().all(|succ| aborts.contains(succ))
                })
            && aborts.insert(block)
    }) {}
    aborts
}

/// Abort blocks reached from a two-way branch, mapped to the branch block and the branch's other target.
fn abort_siblings(
    body: &RefinedBody,
    reachable: &[BlockRef],
    aborts: &BTreeSet<BlockRef>,
) -> BTreeMap<BlockRef, (BlockRef, BlockRef)> {
    let mut siblings = BTreeMap::new();
    for block in reachable {
        let Some((terminator, _)) = &body.blocks[block.0].terminator else {
            continue;
        };
        if let [first, second] = terminator.successors()[..] {
            for (abort, other) in [(first, second), (second, first)] {
                if aborts.contains(&abort) {
                    siblings.insert(abort, (*block, other));
                }
            }
        }
    }
    siblings
}

/// Successor of a checked-arithmetic branch mapped to its sibling abort block.
fn checked_arithmetic_siblings(
    body: &RefinedBody,
    reachable: &[BlockRef],
) -> BTreeMap<BlockRef, BlockRef> {
    let mut map = BTreeMap::new();
    for block in reachable {
        let Some((terminator, _)) = &body.blocks[block.0].terminator else {
            continue;
        };
        if let [first, second] = terminator.successors()[..] {
            for (abort, sibling) in [(first, second), (second, first)] {
                if matches!(
                    body.blocks[abort.0].terminator,
                    Some((RTerm::Abort(RAbort::CheckedArithmetic { .. }), _))
                ) {
                    map.insert(sibling, abort);
                }
            }
        }
    }
    map
}

fn excluded_abort_slots(body: &RefinedBody, pred: BlockRef, abort: BlockRef) -> BTreeSet<Slot> {
    let mut excluded = BTreeSet::new();
    if let Some((RTerm::Abort(RAbort::CheckedConversion { .. }), _)) =
        &body.blocks[abort.0].terminator
        && let Some(stmt) = body.blocks[pred.0].statements.last()
        && matches!(stmt.op, ROp::CheckedIntegerConvert { .. })
        && let Some(first) = stmt.results.first()
    {
        excluded.insert(*first);
    }
    excluded
}

fn shape(body: &RefinedBody, span: Span) -> Result<Shape, LoweringError> {
    let reachable = reachable(body, span)?;
    let local: BTreeMap<_, _> = reachable
        .iter()
        .map(|block| (*block, uses_and_defs(body, *block)))
        .collect();
    let aborts = abort_blocks(body, &reachable);
    let siblings = abort_siblings(body, &reachable, &aborts);
    let checked_siblings = checked_arithmetic_siblings(body, &reachable);
    let mut abort_preds: BTreeMap<BlockRef, Vec<(BlockRef, BTreeSet<Slot>)>> = BTreeMap::new();
    for &pred in &aborts {
        let retired: BTreeSet<Slot> = body.blocks[pred.0]
            .statements
            .iter()
            .flat_map(|s| s.op.retired_slots(&body.slots))
            .collect();
        if let Some((term, _)) = &body.blocks[pred.0].terminator {
            for succ in term.successors() {
                if aborts.contains(&succ) {
                    abort_preds
                        .entry(succ)
                        .or_default()
                        .push((pred, retired.clone()));
                }
            }
        }
    }
    let mut live_in: BTreeMap<BlockRef, BTreeSet<Slot>> = BTreeMap::new();
    loop {
        let mut changed = false;
        for block in reachable.iter().rev() {
            let (uses, defs) = &local[block];
            let mut live = uses.clone();
            if let Some((pred, sibling)) = siblings.get(block) {
                let excluded = excluded_abort_slots(body, *pred, *block);
                for slot in live_in.get(sibling).into_iter().flatten() {
                    if !excluded.contains(slot) {
                        live.insert(*slot);
                    }
                }
            }
            if let Some(preds) = abort_preds.get(block) {
                for (pred, retired) in preds {
                    let excluded = excluded_abort_slots(body, *pred, *block);
                    for slot in live_in.get(pred).into_iter().flatten() {
                        if !retired.contains(slot)
                            && !excluded.contains(slot)
                            && !matches!(
                                body.slots[slot.0],
                                crate::ssa::model::EntityType::Place(_)
                            )
                        {
                            live.insert(*slot);
                        }
                    }
                    for slot in &local[pred].1 {
                        if !retired.contains(slot)
                            && !excluded.contains(slot)
                            && matches!(
                                body.slots[slot.0],
                                crate::ssa::model::EntityType::Loan { .. }
                            )
                        {
                            live.insert(*slot);
                        }
                    }
                }
            }
            if let Some(abort) = checked_siblings.get(block) {
                for slot in live_in.get(abort).into_iter().flatten() {
                    live.insert(*slot);
                }
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
    defined: BTreeMap<Slot, EntityId>,
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

    fn entities(&self, slots: &[Slot], span: Span) -> Result<Vec<EntityId>, LoweringError> {
        slots.iter().map(|slot| self.get(*slot, span)).collect()
    }

    fn values(&self, slots: &[Slot], span: Span) -> Result<Vec<ValueId>, LoweringError> {
        slots.iter().map(|slot| self.value(*slot, span)).collect()
    }

    fn abort_divisor(&self, slot: Slot, span: Span) -> Result<ValueId, LoweringError> {
        let entity = self
            .current
            .get(&slot)
            .or_else(|| self.defined.get(&slot))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        value(entity, span)
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
            ROp::Binary {
                operator,
                left,
                right,
            } => Operation::Binary {
                operator: *operator,
                left: self.value(*left, span)?,
                right: self.value(*right, span)?,
            },
            ROp::IntegerNot { operand } => Operation::IntegerNot {
                operand: self.value(*operand, span)?,
            },
            ROp::IntegerBitwise {
                operator,
                left,
                right,
            } => Operation::IntegerBitwise {
                operator: *operator,
                left: self.value(*left, span)?,
                right: self.value(*right, span)?,
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
            ROp::CheckedIntegerConvert { operand } => Operation::CheckedIntegerConvert {
                operand: self.value(*operand, span)?,
            },
            ROp::IntegerRangeConstruct {
                first,
                bound,
                inclusive,
            } => Operation::IntegerRangeConstruct {
                first: self.value(*first, span)?,
                bound: self.value(*bound, span)?,
                inclusive: *inclusive,
            },
            ROp::IntegerRangeContains { value, range } => Operation::IntegerRangeContains {
                value: self.value(*value, span)?,
                range: self.value(*range, span)?,
            },
            ROp::IntegerRangeEqual { left, right } => Operation::IntegerRangeEqual {
                left: self.value(*left, span)?,
                right: self.value(*right, span)?,
            },
            ROp::StringClone { source } => Operation::StringClone {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::StringCopyBytes { source } => Operation::StringCopyBytes {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::StringCopySlice { source, start, end } => Operation::StringCopySlice {
                source: loan(self.get(*source, span)?, span)?,
                start: self.value(*start, span)?,
                end: self.value(*end, span)?,
            },
            ROp::Utf8ErrorOffset { source } => Operation::Utf8ErrorOffset {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::StringByteLength { source } => Operation::StringByteLength {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::StringByteAt { source, index } => Operation::StringByteAt {
                source: loan(self.get(*source, span)?, span)?,
                index: self.value(*index, span)?,
            },
            ROp::DirectCall {
                callee,
                receiver,
                arguments,
            } => Operation::DirectCall {
                callee: *callee,
                receiver: receiver.map(|slot| self.get(slot, span)).transpose()?,
                arguments: self.entities(arguments, span)?,
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
            ROp::NullableLoanIsNull { source } => Operation::NullableLoanIsNull {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::NullableTake { owner, proof } => Operation::NullableTake {
                owner: self.value(*owner, span)?,
                proof: loan(self.get(*proof, span)?, span)?,
            },
            ROp::AggregateConstruct { aggregate, fields } => Operation::AggregateConstruct {
                aggregate: *aggregate,
                fields: self.values(fields, span)?,
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
            ROp::SharedFieldLoan { base, field } => Operation::SharedFieldLoan {
                base: loan(self.get(*base, span)?, span)?,
                field: *field,
            },
            ROp::SharedHeapFieldLoan { base, field } => Operation::SharedHeapFieldLoan {
                base: loan(self.get(*base, span)?, span)?,
                field: *field,
            },
            ROp::SharedReferenceFollow { source } => Operation::SharedReferenceFollow {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::AggregateProject { aggregate, field } => Operation::AggregateProject {
                aggregate: self.value(*aggregate, span)?,
                field: *field,
            },
            ROp::AggregateCopyExplode { aggregate } => Operation::AggregateCopyExplode {
                aggregate: self.value(*aggregate, span)?,
            },
            ROp::AggregateExplode { aggregate } => Operation::AggregateExplode {
                aggregate: self.value(*aggregate, span)?,
            },
            ROp::SharedAllocate { owner, payload } => Operation::SharedAllocate {
                owner: *owner,
                payload: self.value(*payload, span)?,
            },
            ROp::SharedRetain { owner } => Operation::SharedRetain {
                owner: self.get(*owner, span)?,
            },
            ROp::SharedPayloadPlace { owner } => Operation::SharedPayloadPlace {
                owner: self.get(*owner, span)?,
            },
            ROp::SharedReborrow { source } => Operation::SharedReborrow {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::FunctionAddress { target } => Operation::FunctionAddress { target: *target },
            ROp::ClosureConstruct {
                closure,
                thunk,
                captures,
            } => Operation::ClosureConstruct {
                closure: *closure,
                thunk: *thunk,
                captures: captures
                    .iter()
                    .map(|slot| {
                        Ok(match self.get(*slot, span)? {
                            EntityId::Loan(loan) => {
                                crate::ssa::model::ClosureCaptureOperand::Shared(loan)
                            }
                            EntityId::Value(value) => {
                                crate::ssa::model::ClosureCaptureOperand::Owned(value)
                            }
                            EntityId::Place(_) => {
                                return Err(error(LoweringErrorKind::InvalidModel, span));
                            }
                        })
                    })
                    .collect::<Result<_, LoweringError>>()?,
            },
            ROp::CallableViewOf { source } => Operation::CallableViewOf {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::CallableInvokeView { view, arguments } => Operation::CallableInvokeView {
                view: loan(self.get(*view, span)?, span)?,
                arguments: self.entities(arguments, span)?,
            },
            ROp::CallableInvoke {
                callable,
                arguments,
            } => Operation::CallableInvoke {
                callable: self.value(*callable, span)?,
                arguments: self.entities(arguments, span)?,
            },
            ROp::RootPlaceTake { owner, place: slot } => Operation::RootPlaceTake {
                owner: self.value(*owner, span)?,
                place: place(self.get(*slot, span)?, span)?,
            },
            ROp::Copy { source } => Operation::Copy {
                source: self.value(*source, span)?,
            },
            ROp::RootReplace {
                owner,
                loan: slot,
                replacement,
            } => Operation::RootReplace {
                owner: self.value(*owner, span)?,
                loan: loan(self.get(*slot, span)?, span)?,
                replacement: self.value(*replacement, span)?,
            },
            ROp::RootSwap {
                left_owner,
                right_owner,
                left_loan,
                right_loan,
            } => Operation::RootSwap {
                owners: [
                    self.value(*left_owner, span)?,
                    self.value(*right_owner, span)?,
                ],
                loans: [
                    loan(self.get(*left_loan, span)?, span)?,
                    loan(self.get(*right_loan, span)?, span)?,
                ],
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
            ROp::InlineFieldReplace {
                receiver,
                field,
                value,
            } => Operation::InlineFieldReplace {
                receiver: loan(self.get(*receiver, span)?, span)?,
                field: *field,
                value: self.value(*value, span)?,
            },
            ROp::HeapPayloadPlace { owner } => Operation::HeapPayloadPlace {
                owner: self.value(*owner, span)?,
            },
            ROp::HeapFieldExchange {
                owner,
                field,
                loan: slot,
                replacement,
            } => Operation::HeapFieldExchange {
                owner: self.value(*owner, span)?,
                field: *field,
                loan: loan(self.get(*slot, span)?, span)?,
                replacement: self.value(*replacement, span)?,
            },
            ROp::ContainerConstruct {
                container,
                elements,
            } => Operation::ContainerConstruct {
                container: *container,
                elements: self.values(elements, span)?,
            },
            ROp::ContainerLength { owner } => Operation::ContainerLength {
                owner: self.get(*owner, span)?,
            },
            ROp::ContainerElementPlace { owner, index } => Operation::ContainerElementPlace {
                owner: self.get(*owner, span)?,
                index: self.value(*index, span)?,
            },
            ROp::ContainerAppend { owner, element } => Operation::ContainerAppend {
                owner: self.value(*owner, span)?,
                element: self.value(*element, span)?,
            },
            ROp::ContainerClear { owner } => Operation::ContainerClear {
                owner: self.value(*owner, span)?,
            },
            ROp::ContainerRemoveAt { owner, index } => Operation::ContainerRemoveAt {
                owner: self.value(*owner, span)?,
                index: self.value(*index, span)?,
            },
            ROp::ContainerRemoveFirst { owner } => Operation::ContainerRemoveFirst {
                owner: self.value(*owner, span)?,
            },
            ROp::ContainerRemoveLast { owner } => Operation::ContainerRemoveLast {
                owner: self.value(*owner, span)?,
            },
            ROp::ContainerInsertAt {
                owner,
                index,
                element,
            } => Operation::ContainerInsertAt {
                owner: self.value(*owner, span)?,
                index: self.value(*index, span)?,
                element: self.value(*element, span)?,
            },
            ROp::ContainerReplace {
                owner,
                index,
                value,
            } => Operation::ContainerReplace {
                owner: self.value(*owner, span)?,
                index: self.value(*index, span)?,
                value: self.value(*value, span)?,
            },
            ROp::ContainerGenerateBorrowed {
                container,
                length,
                initializer,
            } => Operation::ContainerGenerateBorrowed {
                container: *container,
                length: self.value(*length, span)?,
                initializer: loan(self.get(*initializer, span)?, span)?,
            },
            ROp::MapConstruct { map_type } => Operation::MapConstruct {
                map_type: *map_type,
            },
            ROp::MapConsume { owner, map_type } => Operation::MapConsume {
                owner: self.value(*owner, span)?,
                map_type: *map_type,
            },
            ROp::MapSize { owner } => Operation::MapSize {
                owner: self.get(*owner, span)?,
            },
            ROp::MapContains { owner, key } => Operation::MapContains {
                owner: self.get(*owner, span)?,
                key: self.get(*key, span)?,
            },
            ROp::MapGet { owner, key } => Operation::MapGet {
                owner: self.get(*owner, span)?,
                key: self.get(*key, span)?,
            },
            ROp::MapResultUnwrap { result } => Operation::MapResultUnwrap {
                result: self.value(*result, span)?,
            },
            ROp::MapPut { owner, key, value } => Operation::MapPut {
                owner: self.value(*owner, span)?,
                key: self.value(*key, span)?,
                value: self.value(*value, span)?,
            },
            ROp::MapRemove { owner, key } => Operation::MapRemove {
                owner: self.value(*owner, span)?,
                key: self.get(*key, span)?,
            },
            ROp::MapWithValue {
                source,
                key,
                action,
            } => Operation::MapWithValue {
                source: loan(self.get(*source, span)?, span)?,
                key: EntityId::Loan(loan(self.get(*key, span)?, span)?),
                action: loan(self.get(*action, span)?, span)?,
            },
            ROp::MapRequireValue { source, key } => Operation::MapRequireValue {
                source: loan(self.get(*source, span)?, span)?,
                key: EntityId::Loan(loan(self.get(*key, span)?, span)?),
            },
            ROp::MapIterationFirst { source } => Operation::MapIterationFirst {
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::MapIterationHasNext { source, cursor } => Operation::MapIterationHasNext {
                source: loan(self.get(*source, span)?, span)?,
                cursor: self.value(*cursor, span)?,
            },
            ROp::MapIterationNext { source, cursor } => Operation::MapIterationNext {
                source: loan(self.get(*source, span)?, span)?,
                cursor: self.value(*cursor, span)?,
            },
            ROp::MapIterationKeyLoan { source, cursor } => Operation::MapIterationKeyLoan {
                source: loan(self.get(*source, span)?, span)?,
                cursor: self.value(*cursor, span)?,
            },
            ROp::MapIterationValueLoan { source, cursor } => Operation::MapIterationValueLoan {
                source: loan(self.get(*source, span)?, span)?,
                cursor: self.value(*cursor, span)?,
            },
            ROp::RangeConstruct {
                view,
                source,
                begin,
                end,
            } => Operation::RangeConstruct {
                view: *view,
                source: loan(self.get(*source, span)?, span)?,
                begin: self.value(*begin, span)?,
                end: self.value(*end, span)?,
            },
            ROp::RangeCall {
                callee,
                arguments,
                source,
            } => Operation::RangeCall {
                callee: *callee,
                arguments: self.entities(arguments, span)?,
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::RangeLength { view } => Operation::RangeLength {
                view: self.get(*view, span)?,
            },
            ROp::RangeElementPlace { view, index } => Operation::RangeElementPlace {
                view: loan(self.get(*view, span)?, span)?,
                index: self.value(*index, span)?,
            },
            ROp::RangeEnd { view, source } => Operation::RangeEnd {
                view: self.value(*view, span)?,
                source: loan(self.get(*source, span)?, span)?,
            },
            ROp::BorrowCall {
                callee,
                arguments,
                source,
            } => Operation::BorrowCall {
                callee: *callee,
                arguments: self.entities(arguments, span)?,
                source: loan(self.get(*source, span)?, span)?,
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
    let mut receipts = statements::LoanReceipts::new(body, function, span)?;
    receipts.layout(body, function, &blocks, &parameters, &entities, span)?;
    let mut emitter = Emitter {
        function,
        current: BTreeMap::new(),
        defined: BTreeMap::new(),
    };
    for block in &reachable {
        let target = blocks[block];
        emitter.current = parameters[block]
            .iter()
            .copied()
            .zip(entities[block].iter().copied())
            .collect();
        emitter
            .defined
            .extend(emitter.current.iter().map(|(k, v)| (*k, *v)));
        emitter.statements(body, *block, target, &mut receipts)?;
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
        if let RTerm::Abort(reason) = terminator {
            let reason = match reason {
                RAbort::CheckedArithmetic { operator, divisor } => AbortReason::CheckedArithmetic {
                    operator: *operator,
                    divisor: emitter.abort_divisor(*divisor, *term_span)?,
                },
                RAbort::Error { message } => AbortReason::Error {
                    message: loan(emitter.get(*message, *term_span)?, *term_span)?,
                },
                RAbort::CheckedConversion { target: converted } => {
                    AbortReason::CheckedConversion { target: *converted }
                }
                RAbort::NullAssertion => AbortReason::Text("null assertion failed".to_owned()),
                RAbort::NegativeSize => AbortReason::Text("negative size".to_owned()),
                RAbort::UnmatchedWhen => {
                    AbortReason::Text("internal error: when subject matched no branch".to_owned())
                }
            };
            emitter
                .function
                .set_abort_terminator(target, reason, origin)
                .map_err(model_error)?;
            emitter.owner_terminator(body, *block, &mut receipts, *term_span)?;
            continue;
        }
        let kind = match terminator {
            RTerm::Abort(_) => unreachable!(),
            RTerm::Unreachable => TerminatorKind::Unreachable,
            RTerm::Goto(successor) => TerminatorKind::Branch(edge(&emitter, *successor)?),
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            } => TerminatorKind::Conditional {
                condition: emitter.value(*condition, *term_span)?,
                when_true: edge(&emitter, *when_true)?,
                when_false: edge(&emitter, *when_false)?,
            },
            RTerm::NullBranch {
                owner,
                when_null,
                when_non_null,
            } => {
                let view = entities[when_non_null]
                    .last()
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, *term_span))?;
                TerminatorKind::NullableBranch {
                    owner: emitter.value(*owner, *term_span)?,
                    when_null: edge(&emitter, *when_null)?,
                    when_non_null: edge(&emitter, *when_non_null)?,
                    view: loan(view, *term_span)?,
                }
            }
            RTerm::Return(result) => TerminatorKind::Return {
                values: result
                    .map(|slot| emitter.value(slot, *term_span))
                    .transpose()?
                    .into_iter()
                    .collect(),
            },
            RTerm::BorrowReturn { loan: slot } => TerminatorKind::BorrowReturn {
                loan: loan(emitter.get(*slot, *term_span)?, *term_span)?,
            },
            RTerm::RangeReturn { view, source: slot } => TerminatorKind::RangeReturn {
                view: emitter.value(*view, *term_span)?,
                source: loan(emitter.get(*slot, *term_span)?, *term_span)?,
            },
        };
        emitter
            .function
            .set_terminator(target, kind, origin)
            .map_err(model_error)?;
        emitter.owner_terminator(body, *block, &mut receipts, *term_span)?;
        receipts.terminator(body, emitter.function, target, &emitter.current, *term_span)?;
    }
    #[cfg(test)]
    statements::inject_owner_final(body, emitter.function, &mut receipts);
    receipts.finish(body, emitter.function, span)?;
    Ok(())
}
