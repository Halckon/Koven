//! compilation-unit `if` 的 owner-aware CFG 与 branch-state 合流。

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::{BuiltinType, UnitExpressionId},
};

use super::{LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{BlockId, Edge, EntityId, EntityType, Origin, TerminatorKind, ValueId},
};

#[derive(Clone, Copy)]
struct CarriedBinding {
    symbol: UnitSymbolId,
    source: ValueId,
    ty: EntityType,
}

struct BranchExit {
    block: BlockId,
    bindings: BTreeMap<UnitSymbolId, LoweredValue>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_if(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let expression_type = self
            .typed
            .types()
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if builtin_type(self.typed, expression_type) != Some(BuiltinType::Unit)
            || !self.temporaries.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let condition = self.require_expression_value(condition)?;
        let baseline = self.bindings.clone();
        let carried = self.carried_bindings(&baseline, span)?;
        let then_block = self.add_carried_block(&carried, self.statement_span(then_branch)?)?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let else_block = self.add_carried_block(&carried, else_origin)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition,
                    when_true: carried_edge(then_block, &carried),
                    when_false: carried_edge(else_block, &carried),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        let then_baseline = self.rebind_carried(&baseline, then_block, &carried, span)?;
        let else_baseline = self.rebind_carried(&baseline, else_block, &carried, span)?;
        let control = UnitExpressionId::new(self.source_unit, expression);
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_if_branch(
            then_block,
            then_branch,
            then_baseline,
            UnitDropPoint::BranchExit { control, branch: 0 },
        )? {
            exits.push(exit);
        }
        if let Some(else_branch) = else_branch {
            if let Some(exit) = self.lower_if_branch(
                else_block,
                else_branch,
                else_baseline,
                UnitDropPoint::BranchExit { control, branch: 1 },
            )? {
                exits.push(exit);
            }
        } else {
            self.block = else_block;
            self.bindings = else_baseline;
            self.temporaries.clear();
            self.emit_drops(UnitDropPoint::BranchExit { control, branch: 1 })?;
            exits.push(BranchExit {
                block: else_block,
                bindings: self.bindings.clone(),
            });
        }
        self.merge_unit_exits(exits, span)
    }

    fn lower_if_branch(
        &mut self,
        block: BlockId,
        statement: StatementId,
        bindings: BTreeMap<UnitSymbolId, LoweredValue>,
        drop_point: UnitDropPoint,
    ) -> Result<Option<BranchExit>, LoweringError> {
        self.block = block;
        let entry_symbols = bindings.keys().copied().collect::<BTreeSet<_>>();
        self.bindings = bindings;
        self.temporaries.clear();
        let result = self.lower_statement(statement)?;
        if result == LoweredValue::Diverged {
            return Ok(None);
        }
        if result != LoweredValue::Unit || !self.temporaries.is_empty() {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                self.statement_span(statement)?,
            ));
        }
        self.emit_drops(drop_point)?;
        self.discard_branch_locals(&entry_symbols, self.statement_span(statement)?)?;
        Ok(Some(BranchExit {
            block: self.block,
            bindings: self.bindings.clone(),
        }))
    }

    fn discard_branch_locals(
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
            if self.typed.types().copyability(ty)
                != lang_frontend::type_checking::Copyability::Copyable
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.bindings.remove(&symbol);
        }
        Ok(())
    }

    fn carried_bindings(
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
                symbol: *symbol,
                source,
                ty,
            });
        }
        Ok(carried)
    }

    fn add_carried_block(
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

    fn rebind_carried(
        &self,
        baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
        block: BlockId,
        carried: &[CarriedBinding],
        span: Span,
    ) -> Result<BTreeMap<UnitSymbolId, LoweredValue>, LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        if parameters.len() != carried.len() {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        }
        let mut rebound = baseline.clone();
        for (slot, parameter) in carried.iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            rebound.insert(slot.symbol, LoweredValue::Value(*value));
        }
        Ok(rebound)
    }

    fn merge_unit_exits(
        &mut self,
        exits: Vec<BranchExit>,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(first) = exits.first() else {
            self.bindings.clear();
            self.temporaries.clear();
            return Ok(LoweredValue::Diverged);
        };
        let symbols = first.bindings.keys().copied().collect::<Vec<_>>();
        if exits.iter().any(|exit| {
            exit.bindings.keys().copied().collect::<Vec<_>>() != symbols
                || symbols.iter().any(|symbol| {
                    std::mem::discriminant(&exit.bindings[symbol])
                        != std::mem::discriminant(&first.bindings[symbol])
                })
        }) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let value_symbols = symbols
            .iter()
            .copied()
            .filter(|symbol| matches!(first.bindings[symbol], LoweredValue::Value(_)))
            .collect::<Vec<_>>();
        let parameter_types = value_symbols
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
            .collect::<Result<Vec<_>, _>>()?;
        let merge = self
            .function
            .add_block(parameter_types, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for exit in &exits {
            let arguments = value_symbols
                .iter()
                .map(|symbol| match exit.bindings[symbol] {
                    LoweredValue::Value(value) => Ok(EntityId::Value(value)),
                    LoweredValue::Unit | LoweredValue::Diverged => {
                        Err(lowering_error(LoweringErrorKind::InvalidModel, span))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
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
        let mut bindings = first.bindings.clone();
        for (symbol, parameter) in value_symbols.into_iter().zip(parameters) {
            let EntityId::Value(value) = parameter else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            bindings.insert(symbol, LoweredValue::Value(value));
        }
        self.block = merge;
        self.bindings = bindings;
        self.temporaries.clear();
        Ok(LoweredValue::Unit)
    }

    fn statement_span(&self, statement: StatementId) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .statements()
            .get(statement)
            .map(|statement| statement.span())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })
    }
}

fn carried_edge(target: BlockId, carried: &[CarriedBinding]) -> Edge {
    Edge {
        target,
        arguments: carried
            .iter()
            .map(|binding| EntityId::Value(binding.source))
            .collect(),
    }
}
