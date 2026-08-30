//! compilation-unit CFG 中跨 edge 携带 value/loan 与出口合流的共享基元。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{Copyability, UnitExpressionId},
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, resolve_concrete_type};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Edge, EntityId, EntityType, LoanId, Origin, TerminatorKind, ValueId},
};

pub(super) struct BranchExit {
    pub(super) block: BlockId,
    pub(super) result: LoweredValue,
    pub(super) receiver: Option<super::ReceiverBinding>,
    pub(super) bindings: BTreeMap<UnitSymbolId, LoweredValue>,
    pub(super) borrow_bindings: BTreeMap<UnitSymbolId, LoanId>,
    pub(super) closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
}

#[derive(Clone, Copy)]
pub(super) struct CarriedBinding {
    symbol: Option<UnitSymbolId>,
    receiver: Option<super::ReceiverBinding>,
    source: ValueId,
    ty: EntityType,
}

#[derive(Clone, Copy)]
pub(super) struct CarriedLoan {
    symbol: Option<UnitSymbolId>,
    receiver: Option<super::ReceiverBinding>,
    source: LoanId,
    ty: EntityType,
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
                .types()
                .symbol_type(*symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let ty = resolve_concrete_type(self.typed, ty, self.substitutions, span)?;
            if self.typed.types().copyability(ty) == Copyability::MoveOnly {
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
            });
        }
        Ok(carried)
    }

    pub(super) fn carried_loans(
        &self,
        loans: &BTreeMap<UnitSymbolId, LoanId>,
        span: Span,
    ) -> Result<Vec<CarriedLoan>, LoweringError> {
        let mut carried = loans
            .iter()
            .map(|(symbol, source)| {
                let ty = self
                    .function
                    .entity(EntityId::Loan(*source))
                    .map(|entity| entity.ty)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Ok(CarriedLoan {
                    symbol: Some(*symbol),
                    receiver: None,
                    source: *source,
                    ty,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(receiver) = self.current_receiver
            && let EntityId::Loan(source) = receiver.entity
        {
            let ty = self
                .function
                .entity(receiver.entity)
                .map(|entity| entity.ty)
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            carried.push(CarriedLoan {
                symbol: None,
                receiver: Some(receiver),
                source,
                ty,
            });
        }
        Ok(carried)
    }

    pub(super) fn add_carried_control_block(
        &mut self,
        bindings: &[CarriedBinding],
        loans: &[CarriedLoan],
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

    pub(super) fn add_carried_block(
        &mut self,
        carried: &[CarriedBinding],
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        self.function
            .add_block(
                carried.iter().map(|binding| binding.ty).collect(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))
    }

    pub(super) fn rebind_carried(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        carried: &[CarriedBinding],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        self.rebind_carried_prefix(baseline, block, carried, carried.len(), span)
    }

    pub(super) fn rebind_carried_control(
        &mut self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        bindings: &[CarriedBinding],
        loans: &[CarriedLoan],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        self.rebind_carried_prefix(
            baseline,
            block,
            bindings,
            bindings.len() + loans.len(),
            span,
        )
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
            } else {
                let mut receiver = slot
                    .receiver
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                receiver.entity = EntityId::Value(*value);
                self.current_receiver = Some(receiver);
            }
        }
        Ok(rebound)
    }

    pub(super) fn rebind_carried_loans(
        &mut self,
        block: BlockId,
        binding_count: usize,
        loans: &[CarriedLoan],
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
        for (slot, parameter) in loans.iter().zip(parameters.iter().skip(binding_count)) {
            let EntityId::Loan(loan) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            if let Some(symbol) = slot.symbol {
                rebound.insert(symbol, *loan);
            } else {
                let mut receiver = slot
                    .receiver
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                receiver.entity = EntityId::Loan(*loan);
                self.current_receiver = Some(receiver);
            }
        }
        Ok(rebound)
    }

    pub(super) fn merge_unit_exits(
        &mut self,
        exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(first) = exits.first() else {
            self.current_receiver = None;
            self.bindings.clear();
            self.borrow_bindings.clear();
            self.closure_bindings.clear();
            self.temporaries.clear();
            return Ok(LoweredValue::Diverged);
        };
        if exits.len() == 1 {
            self.block = first.block;
            self.current_receiver = first.receiver;
            self.bindings = first.bindings.clone();
            self.borrow_bindings = first.borrow_bindings.clone();
            self.closure_bindings = first.closure_bindings.clone();
            self.temporaries.clear();
            return Ok(first.result);
        }
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
                || exit.closure_bindings != first.closure_bindings
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
        let merge = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for exit in &exits {
            let mut arguments = match exit.result {
                LoweredValue::Unit => Vec::new(),
                LoweredValue::Value(value) => vec![EntityId::Value(value)],
                LoweredValue::Diverged => {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
            };
            if let Some(receiver) = exit.receiver {
                arguments.push(receiver.entity);
            }
            arguments.extend(
                value_symbols
                    .iter()
                    .map(|symbol| match exit.bindings[symbol] {
                        LoweredValue::Value(value) => Ok(EntityId::Value(value)),
                        LoweredValue::Unit | LoweredValue::Diverged => {
                            Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            arguments.extend(
                loan_symbols
                    .iter()
                    .map(|symbol| EntityId::Loan(exit.borrow_bindings[symbol])),
            );
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
        self.temporaries.clear();
        Ok(result)
    }

    pub(super) fn carried_edge_from(
        &self,
        target: BlockId,
        carried: &[CarriedBinding],
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        receiver: Option<super::ReceiverBinding>,
        span: Span,
    ) -> Result<Edge, LoweringError> {
        let arguments = carried
            .iter()
            .map(|slot| match slot.symbol {
                Some(symbol) => match bindings.get(&symbol) {
                    Some(LoweredValue::Value(value)) => Ok(EntityId::Value(*value)),
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        Err(lowering_error(LoweringErrorKind::MissingFact, span))
                    }
                },
                None => match receiver {
                    Some(receiver)
                        if slot.receiver.is_some_and(|expected| {
                            expected.owner == receiver.owner
                                && expected.mode == receiver.mode
                                && expected.ty == receiver.ty
                        }) && self
                            .function
                            .entity(receiver.entity)
                            .map(|entity| entity.ty)
                            == Some(slot.ty) =>
                    {
                        Ok(receiver.entity)
                    }
                    Some(_) | None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                },
            })
            .collect::<Result<Vec<_>, _>>()?;
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
                .types()
                .body_symbol_types()
                .get(&symbol)
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if self.typed.types().copyability(ty) != Copyability::Copyable {
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
    loans: &[CarriedLoan],
) -> Edge {
    Edge {
        target,
        arguments: bindings
            .iter()
            .map(|binding| EntityId::Value(binding.source))
            .chain(loans.iter().map(|loan| EntityId::Loan(loan.source)))
            .collect(),
    }
}

pub(super) fn carried_edge(target: BlockId, carried: &[CarriedBinding]) -> Edge {
    Edge {
        target,
        arguments: carried
            .iter()
            .map(|binding| EntityId::Value(binding.source))
            .collect(),
    }
}
