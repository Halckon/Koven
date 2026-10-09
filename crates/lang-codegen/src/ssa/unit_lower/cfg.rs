//! compilation-unit CFG 中跨 edge 携带 value/loan 与出口合流的共享基元。

mod range;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{Copyability, UnitExpressionId},
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        BlockId, Edge, EntityId, EntityType, LoanId, Operation, Origin, TerminatorKind, ValueId,
    },
};

pub(super) struct BranchExit {
    pub(super) block: BlockId,
    pub(super) result: LoweredValue,
    pub(super) receiver: Option<super::ReceiverBinding>,
    pub(super) consumed_receiver: Option<super::ConsumedReceiver>,
    pub(super) bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    pub(super) borrow_bindings: BTreeMap<UnitSymbolId, LoanId>,
    pub(super) pending_operands: Vec<EntityId>,
    pub(super) temporaries: BTreeMap<UnitExpressionId, ValueId>,
    pub(super) closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
    pub(super) capture_loans: BTreeMap<(UnitExpressionId, usize), LoanId>,
    pub(super) result_source_loans: BTreeMap<UnitSymbolId, Vec<LoanId>>,
}

impl BranchExit {
    fn base_arguments(&self) -> Vec<EntityId> {
        let mut arguments = match self.result {
            LoweredValue::Value(value) => vec![EntityId::Value(value)],
            _ => Vec::new(),
        };
        arguments.extend(self.receiver.map(|receiver| receiver.entity));
        arguments.extend(self.bindings.values().filter_map(|value| match value {
            LoweredValue::Value(value) => Some(EntityId::Value(*value)),
            _ => None,
        }));
        arguments.extend(
            self.borrow_bindings
                .values()
                .map(|loan| EntityId::Loan(*loan)),
        );
        arguments
    }
}

#[derive(Clone)]
pub(super) struct CarriedBinding {
    symbol: Option<UnitSymbolId>,
    receiver: Option<super::ReceiverBinding>,
    source: ValueId,
    ty: EntityType,
    pending: Vec<usize>,
    temporaries: Vec<UnitExpressionId>,
}

#[derive(Clone)]
pub(super) struct CarriedAccess {
    range_sources: Vec<(UnitSymbolId, usize)>,
    captures: Vec<(UnitExpressionId, usize)>,
    symbol: Option<UnitSymbolId>,
    receiver: Option<super::ReceiverBinding>,
    source: EntityId,
    ty: EntityType,
    pending: Vec<usize>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn move_only_carried_bindings(
        &self,
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<Vec<CarriedBinding>, LoweringError> {
        let mut move_only = BTreeMap::new();
        for (symbol, binding) in bindings {
            let ty = self
                .typed
                .symbol_type(*symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty =
                resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
            if self.typed.copyability(ty) == Copyability::MoveOnly {
                move_only.insert(*symbol, *binding);
            }
        }
        self.carried_bindings(&move_only, span)
    }

    pub(super) fn carried_bindings(
        &self,
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        span: Span,
    ) -> Result<Vec<CarriedBinding>, LoweringError> {
        let mut carried = Vec::new();
        for (symbol, binding) in bindings {
            let source = match binding {
                LoweredValue::Unit => continue,
                LoweredValue::Value(value) => *value,
                LoweredValue::Diverged => {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
            };
            let ty = self
                .function
                .entity(EntityId::Value(source))
                .map(|entity| entity.ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            carried.push(CarriedBinding {
                symbol: Some(*symbol),
                receiver: None,
                source,
                ty,
                pending: Vec::new(),
                temporaries: Vec::new(),
            });
        }
        if let Some(receiver) = self.current_receiver
            && let EntityId::Value(source) = receiver.entity
        {
            let ty = self
                .function
                .entity(receiver.entity)
                .map(|entity| entity.ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            carried.push(CarriedBinding {
                symbol: None,
                receiver: Some(receiver),
                source,
                ty,
                pending: Vec::new(),
                temporaries: Vec::new(),
            });
        }
        Ok(carried)
    }

    pub(super) fn carried_loans(
        &self,
        loans: &BTreeMap<UnitSymbolId, LoanId>,
        span: Span,
    ) -> Result<Vec<CarriedAccess>, LoweringError> {
        let mut carried = loans
            .iter()
            .map(|(symbol, source)| {
                let ty = self
                    .function
                    .entity(EntityId::Loan(*source))
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Ok(CarriedAccess {
                    range_sources: Vec::new(),
                    captures: Vec::new(),
                    symbol: Some(*symbol),
                    receiver: None,
                    source: EntityId::Loan(*source),
                    ty,
                    pending: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, LoweringError>>()?;
        if let Some(receiver) = self.current_receiver
            && let EntityId::Loan(source) = receiver.entity
        {
            let ty = self
                .function
                .entity(receiver.entity)
                .map(|entity| entity.ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            carried.push(CarriedAccess {
                range_sources: Vec::new(),
                captures: Vec::new(),
                symbol: None,
                receiver: Some(receiver),
                source: EntityId::Loan(source),
                ty,
                pending: Vec::new(),
            });
        }
        for (&key, &loan) in &self.capture_loans {
            let entity = EntityId::Loan(loan);
            if let Some(slot) = carried.iter_mut().find(|slot| slot.source == entity) {
                slot.captures.push(key);
            } else {
                let ty = self
                    .function
                    .entity(entity)
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                carried.push(CarriedAccess {
                    symbol: None,
                    receiver: None,
                    source: entity,
                    ty,
                    pending: Vec::new(),
                    range_sources: Vec::new(),
                    captures: vec![key],
                });
            }
        }
        self.carry_range_sources(&mut carried, span)?;
        Ok(carried)
    }

    /// Pending operands can alias named loans; carry each entity once and retain its slots.
    pub(super) fn carry_pending_operands(
        &self,
        bindings: &mut Vec<CarriedBinding>,
        loans: &mut Vec<CarriedAccess>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (index, entity) in self.pending_operands.iter().copied().enumerate() {
            let ty = self
                .function
                .entity(entity)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
                .ty;
            match entity {
                EntityId::Value(source) => {
                    if let Some(slot) = bindings.iter_mut().find(|slot| slot.source == source) {
                        slot.pending.push(index);
                    } else {
                        bindings.push(CarriedBinding {
                            symbol: None,
                            receiver: None,
                            source,
                            ty,
                            pending: vec![index],
                            temporaries: Vec::new(),
                        });
                    }
                }
                EntityId::Loan(_) | EntityId::Place(_) => {
                    if let Some(slot) = loans.iter_mut().find(|slot| slot.source == entity) {
                        slot.pending.push(index);
                    } else {
                        loans.push(CarriedAccess {
                            range_sources: Vec::new(),
                            captures: Vec::new(),
                            symbol: None,
                            receiver: None,
                            source: entity,
                            ty,
                            pending: vec![index],
                        });
                    }
                }
            }
        }
        // A pending borrowed temporary needs its owner as well as its loan on each edge.
        for (expression, source) in &self.temporaries {
            if let Some(slot) = bindings.iter_mut().find(|slot| slot.source == *source) {
                slot.temporaries.push(*expression);
            } else {
                let ty = self
                    .function
                    .entity(EntityId::Value(*source))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
                    .ty;
                bindings.push(CarriedBinding {
                    symbol: None,
                    receiver: None,
                    source: *source,
                    ty,
                    pending: Vec::new(),
                    temporaries: vec![*expression],
                });
            }
        }
        Ok(())
    }

    /// A loop may reassign a Copyable local while an earlier argument keeps its old value.
    pub(super) fn separate_loop_pending_copies(
        &self,
        bindings: &mut Vec<CarriedBinding>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut snapshots = Vec::new();
        for slot in bindings.iter_mut() {
            let Some(symbol) = slot.symbol else { continue };
            if slot.pending.is_empty() {
                continue;
            }
            // 普通 Value 实参保留独立 snapshot；原语 root 已被 exclusive loan 锁定，
            // 不允许循环改写，必须与其 loan 一起携带同一个 binding/owner identity。
            if self.pending_call_frames.iter().any(|frame| {
                frame
                    .exclusive_root_owners
                    .iter()
                    .any(|index| slot.pending.contains(index))
            }) {
                continue;
            }
            let ty = self
                .typed
                .symbol_type(symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty =
                resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
            if self.typed.copyability(ty) == Copyability::Copyable {
                let mut snapshot = slot.clone();
                snapshot.symbol = None;
                snapshot.pending = std::mem::take(&mut slot.pending);
                snapshot.temporaries = std::mem::take(&mut slot.temporaries);
                snapshots.push(snapshot);
            }
        }
        bindings.extend(snapshots);
        Ok(())
    }

    pub(super) fn add_carried_control_block(
        &mut self,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(
                bindings
                    .iter()
                    .map(|binding| binding.ty)
                    .chain(loans.iter().map(|loan| loan.ty))
                    .collect(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

    pub(super) fn rebind_carried_control(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let rebound = self.rebind_carried_prefix(
            baseline,
            block,
            bindings,
            bindings.len() + loans.len(),
            span,
        )?;
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        let mut pending = BTreeMap::new();
        for (indices, parameter) in bindings
            .iter()
            .map(|slot| &slot.pending)
            .chain(loans.iter().map(|slot| &slot.pending))
            .zip(parameters)
        {
            for index in indices {
                pending.insert(*index, *parameter);
            }
        }
        if pending.keys().copied().ne(0..pending.len()) {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        self.pending_operands = pending.into_values().collect();
        self.temporaries.clear();
        for (slot, parameter) in bindings.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            for expression in &slot.temporaries {
                self.temporaries.insert(*expression, *value);
            }
        }
        let source_parameters = parameters[bindings.len()..].to_vec();
        self.rebind_range_sources(loans, &source_parameters, span)?;
        Ok(rebound)
    }

    fn rebind_carried_prefix(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        carried: &[CarriedBinding],
        expected_parameter_count: usize,
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != expected_parameter_count {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = baseline.clone();
        for (slot, parameter) in carried.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, LoweredValue::Value(*value));
            } else if let Some(mut receiver) = slot.receiver {
                receiver.entity = EntityId::Value(*value);
                self.current_receiver = Some(receiver);
                self.consumed_receiver = None;
            }
        }
        Ok(rebound)
    }

    pub(super) fn rebind_carried_loans(
        &mut self,
        block: BlockId,
        binding_count: usize,
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoanId>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != binding_count + loans.len() {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = BTreeMap::new();
        self.capture_loans.clear();
        for (slot, parameter) in loans.iter().zip(parameters.iter().skip(binding_count)) {
            if slot.symbol.is_none() && slot.receiver.is_none() && slot.captures.is_empty() {
                continue;
            }
            let EntityId::Loan(loan) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            for key in &slot.captures {
                self.capture_loans.insert(*key, *loan);
            }
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, *loan);
            } else if let Some(mut receiver) = slot.receiver {
                receiver.entity = EntityId::Loan(*loan);
                self.current_receiver = Some(receiver);
                self.consumed_receiver = None;
            }
        }
        Ok(rebound)
    }

    pub(super) fn merge_unit_exits(
        &mut self,
        mut exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if exits.is_empty() {
            self.current_receiver = None;
            self.consumed_receiver = None;
            self.bindings.clear();
            self.borrow_bindings.clear();
            self.closure_bindings.clear();
            self.capture_loans.clear();
            self.result_source_loans.clear();
            self.temporaries.clear();
            self.pending_operands.clear();
            return Ok(LoweredValue::Diverged);
        }
        if exits.len() == 1 {
            let first = &exits[0];
            self.block = first.block;
            self.current_receiver = first.receiver;
            self.consumed_receiver = first.consumed_receiver;
            self.bindings = first.bindings.clone();
            self.borrow_bindings = first.borrow_bindings.clone();
            self.closure_bindings = first.closure_bindings.clone();
            self.capture_loans = first.capture_loans.clone();
            self.result_source_loans = first.result_source_loans.clone();
            self.temporaries = first.temporaries.clone();
            self.pending_operands = first.pending_operands.clone();
            return Ok(first.result);
        }
        self.normalize_receiver_exits(&mut exits, span)?;
        let first = &exits[0];
        let symbols = first.bindings.keys().copied().collect::<Vec<_>>();
        let loan_symbols = first.borrow_bindings.keys().copied().collect::<Vec<_>>();
        let receiver_type = first
            .receiver
            .map(|receiver| {
                self.function
                    .entity(receiver.entity)
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))
            })
            .transpose()?;
        if exits.iter().any(|exit| {
            exit.bindings.keys().copied().collect::<Vec<_>>() != symbols
                || exit.borrow_bindings.keys().copied().collect::<Vec<_>>() != loan_symbols
                || exit.temporaries.keys().ne(first.temporaries.keys())
                || exit.closure_bindings != first.closure_bindings
                || exit.capture_loans.keys().ne(first.capture_loans.keys())
                || range::keys(&exit.result_source_loans) != range::keys(&first.result_source_loans)
                || exit.consumed_receiver != first.consumed_receiver
                || match (first.receiver, exit.receiver) {
                    (None, None) => false,
                    (Some(expected), Some(actual)) => {
                        expected.owner != actual.owner
                            || expected.mode != actual.mode
                            || expected.ty != actual.ty
                            || self.function.entity(actual.entity).map(|entity| entity.ty)
                                != receiver_type
                    }
                    (None, Some(_)) | (Some(_), None) => true,
                }
                || std::mem::discriminant(&exit.result) != std::mem::discriminant(&first.result)
                || symbols.iter().any(|symbol| {
                    std::mem::discriminant(&exit.bindings[symbol])
                        != std::mem::discriminant(&first.bindings[symbol])
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let result_type = match first.result {
            LoweredValue::Unit => None,
            LoweredValue::Value(value) => Some(
                self.function
                    .entity(EntityId::Value(value))
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?,
            ),
            LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            }
        };
        if let Some(expected) = result_type
            && exits.iter().any(|exit| match exit.result {
                LoweredValue::Value(value) => self
                    .function
                    .entity(EntityId::Value(value))
                    .is_none_or(|entity| entity.ty != expected),
                LoweredValue::Unit | LoweredValue::Diverged => true,
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let value_symbols = symbols
            .iter()
            .copied()
            .filter(|symbol| matches!(first.bindings[symbol], LoweredValue::Value(_)))
            .collect::<Vec<_>>();
        let mut parameter_types = result_type.into_iter().collect::<Vec<_>>();
        parameter_types.extend(receiver_type);
        parameter_types.extend(
            value_symbols
                .iter()
                .map(|symbol| match first.bindings[symbol] {
                    LoweredValue::Value(value) => self
                        .function
                        .entity(EntityId::Value(value))
                        .map(|entity| entity.ty)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span)),
                    LoweredValue::Unit | LoweredValue::Diverged => {
                        Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        let loan_types = loan_symbols
            .iter()
            .map(|symbol| {
                self.function
                    .entity(EntityId::Loan(first.borrow_bindings[symbol]))
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if exits.iter().any(|exit| {
            loan_symbols
                .iter()
                .zip(&loan_types)
                .any(|(symbol, expected)| {
                    self.function
                        .entity(EntityId::Loan(exit.borrow_bindings[symbol]))
                        .is_none_or(|entity| entity.ty != *expected)
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        parameter_types.extend(loan_types);
        let mut edge_arguments = exits
            .iter()
            .map(BranchExit::base_arguments)
            .collect::<Vec<_>>();
        if exits
            .iter()
            .any(|exit| exit.pending_operands.len() != first.pending_operands.len())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let carried_operands = exits
            .iter()
            .map(|exit| {
                exit.pending_operands
                    .iter()
                    .copied()
                    .chain(exit.temporaries.values().copied().map(EntityId::Value))
                    .chain(exit.capture_loans.values().copied().map(EntityId::Loan))
                    .chain(range::arguments(&exit.result_source_loans))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        // An alias is reusable only if the same slot aliases it on every incoming edge.
        let mut pending_indices = Vec::new();
        for index in 0..carried_operands[0].len() {
            let slot = (0..edge_arguments[0].len())
                .find(|slot| {
                    carried_operands
                        .iter()
                        .zip(&edge_arguments)
                        .all(|(operands, arguments)| arguments[*slot] == operands[index])
                })
                .unwrap_or_else(|| {
                    let slot = edge_arguments[0].len();
                    for (operands, arguments) in carried_operands.iter().zip(&mut edge_arguments) {
                        arguments.push(operands[index]);
                    }
                    slot
                });
            pending_indices.push(slot);
        }
        for entity in &edge_arguments[0][parameter_types.len()..] {
            parameter_types.push(
                self.function
                    .entity(*entity)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
                    .ty,
            );
        }
        for arguments in &edge_arguments {
            if arguments.len() != parameter_types.len()
                || arguments.iter().zip(&parameter_types).any(|(entity, ty)| {
                    self.function
                        .entity(*entity)
                        .is_none_or(|entity| entity.ty != *ty)
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let merge = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for (exit, arguments) in exits.iter().zip(edge_arguments) {
            self.function
                .set_terminator(
                    exit.block,
                    TerminatorKind::Branch(Edge {
                        target: merge,
                        arguments,
                    }),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        let parameters = self
            .function
            .block(merge)
            .expect("new merge block exists")
            .parameters
            .clone();
        self.pending_operands = pending_indices[..first.pending_operands.len()]
            .iter()
            .map(|index| parameters[*index])
            .collect();
        self.temporaries = first
            .temporaries
            .keys()
            .zip(&pending_indices[first.pending_operands.len()..])
            .map(|(expression, index)| {
                let EntityId::Value(value) = parameters[*index] else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                Ok((*expression, value))
            })
            .collect::<Result<_, _>>()?;
        self.capture_loans = first
            .capture_loans
            .keys()
            .zip(&pending_indices[first.pending_operands.len() + first.temporaries.len()..])
            .map(|(key, index)| {
                let EntityId::Loan(loan) = parameters[*index] else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                Ok((*key, loan))
            })
            .collect::<Result<_, _>>()?;
        let source_offset =
            first.pending_operands.len() + first.temporaries.len() + first.capture_loans.len();
        self.result_source_loans = range::rebind(
            &first.result_source_loans,
            pending_indices[source_offset..]
                .iter()
                .map(|index| parameters[*index]),
            span,
        )?;
        let mut parameters = parameters.into_iter();
        let result = if result_type.is_some() {
            let EntityId::Value(value) = parameters
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            LoweredValue::Value(value)
        } else {
            LoweredValue::Unit
        };
        self.current_receiver = match first.receiver {
            Some(mut receiver) => {
                receiver.entity = parameters
                    .next()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Some(receiver)
            }
            None => None,
        };
        self.consumed_receiver = first.consumed_receiver;
        let mut bindings = first.bindings.clone();
        for (symbol, parameter) in value_symbols.into_iter().zip(parameters.by_ref()) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            bindings.insert(symbol, LoweredValue::Value(value));
        }
        let mut borrow_bindings = BTreeMap::new();
        for (symbol, parameter) in loan_symbols.into_iter().zip(parameters) {
            let EntityId::Loan(loan) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            borrow_bindings.insert(symbol, loan);
        }
        self.block = merge;
        self.bindings = bindings;
        self.borrow_bindings = borrow_bindings;
        self.closure_bindings = first.closure_bindings.clone();
        Ok(result)
    }

    fn normalize_receiver_exits(
        &mut self,
        exits: &mut [BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(consumed) = exits.iter().find_map(|exit| exit.consumed_receiver) else {
            return Ok(());
        };
        if consumed.mode != lang_frontend::type_checking::ParameterMode::Value
            || self.typed.copyability(consumed.ty) != Copyability::MoveOnly
            || exits.iter().any(|exit| {
                (exit.receiver.is_some() && exit.consumed_receiver.is_some())
                    || exit
                        .consumed_receiver
                        .is_some_and(|actual| actual != consumed)
                    || exit
                        .receiver
                        .is_some_and(|receiver| super::ConsumedReceiver::from(receiver) != consumed)
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let drop_origin = self.require_conditional_receiver_drop_obligation(
            consumed.owner,
            consumed.template_ty,
            consumed.ty,
            consumed.origin,
        )?;
        for exit in exits {
            let Some(receiver) = exit.receiver.take() else {
                continue;
            };
            let EntityId::Value(owner) = receiver.entity else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            self.function
                .append_instruction(
                    exit.block,
                    Operation::Drop { owner },
                    Vec::new(),
                    Origin::Source(drop_origin),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            exit.consumed_receiver = Some(consumed);
        }
        Ok(())
    }

    /// Resolve loop edges from the jump snapshot, including pending aliases after CFG joins.
    pub(super) fn carried_edge_from(
        &self,
        target: BlockId,
        carried: &[CarriedBinding],
        loans: &[CarriedAccess],
        state: &BranchExit,
        span: Span,
    ) -> Result<Edge, LoweringError> {
        let receiver = |expected: Option<super::ReceiverBinding>| {
            expected
                .and_then(|expected| {
                    state.receiver.filter(|actual| {
                        expected.owner == actual.owner
                            && expected.mode == actual.mode
                            && expected.ty == actual.ty
                    })
                })
                .map(|actual| actual.entity)
        };
        let mut arguments = Vec::new();
        for slot in carried {
            let entity = if let Some(symbol) = slot.symbol {
                match state.bindings.get(&symbol) {
                    Some(LoweredValue::Value(value)) => Some(EntityId::Value(*value)),
                    _ => None,
                }
            } else if slot.receiver.is_some() {
                receiver(slot.receiver)
            } else if let Some(index) = slot.pending.first() {
                state.pending_operands.get(*index).copied()
            } else {
                slot.temporaries
                    .first()
                    .and_then(|expression| state.temporaries.get(expression))
                    .copied()
                    .map(EntityId::Value)
            }
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.function.entity(entity).map(|entity| entity.ty) != Some(slot.ty)
                || slot
                    .pending
                    .iter()
                    .any(|index| state.pending_operands.get(*index) != Some(&entity))
                || slot.temporaries.iter().any(|expression| {
                    state
                        .temporaries
                        .get(expression)
                        .copied()
                        .map(EntityId::Value)
                        != Some(entity)
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            arguments.push(entity);
        }
        for slot in loans {
            let entity = if let Some(symbol) = slot.symbol {
                state
                    .borrow_bindings
                    .get(&symbol)
                    .copied()
                    .map(EntityId::Loan)
            } else if slot.receiver.is_some() {
                receiver(slot.receiver)
            } else if let Some((symbol, index)) = slot.range_sources.first() {
                state
                    .result_source_loans
                    .get(symbol)
                    .and_then(|loans| loans.get(*index))
                    .copied()
                    .map(EntityId::Loan)
            } else if let Some(key) = slot.captures.first() {
                state.capture_loans.get(key).copied().map(EntityId::Loan)
            } else {
                slot.pending
                    .first()
                    .and_then(|index| state.pending_operands.get(*index))
                    .copied()
            }
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.function.entity(entity).map(|entity| entity.ty) != Some(slot.ty)
                || slot
                    .pending
                    .iter()
                    .any(|index| state.pending_operands.get(*index) != Some(&entity))
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            if slot.range_sources.iter().any(|(symbol, index)| {
                state
                    .result_source_loans
                    .get(symbol)
                    .and_then(|loans| loans.get(*index))
                    .copied()
                    .map(EntityId::Loan)
                    != Some(entity)
            }) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            if slot.captures.iter().any(|key| {
                state.capture_loans.get(key).copied().map(EntityId::Loan) != Some(entity)
            }) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            arguments.push(entity);
        }
        Ok(Edge { target, arguments })
    }

    pub(super) fn discard_non_entry_bindings(
        &mut self,
        entry_symbols: &BTreeSet<UnitSymbolId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let locals = self
            .bindings
            .keys()
            .filter(|symbol| !entry_symbols.contains(symbol))
            .copied()
            .collect::<Vec<_>>();
        for symbol in locals {
            let ty = self
                .typed
                .body_symbol_types()
                .get(&symbol)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.copyability(ty) != Copyability::Copyable {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.bindings.remove(&symbol);
        }
        Ok(())
    }
}

pub(super) fn carried_control_edge(
    target: BlockId,
    bindings: &[CarriedBinding],
    loans: &[CarriedAccess],
) -> Edge {
    Edge {
        target,
        arguments: bindings
            .iter()
            .map(|binding| EntityId::Value(binding.source))
            .chain(loans.iter().map(|loan| loan.source))
            .collect(),
    }
}
