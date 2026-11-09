//! Retain real prefix identity and independently read loop and optional jump states.
mod split_terminal;
use super::*;
use crate::ownership_checking::{
    UnitOwnerInvariantBreakObservationV1, UnitOwnerInvariantLoopObservationV1,
    UnitOwnerLoopBindingV1, UnitOwnerProducerCatalogV1,
};

pub(super) struct InvariantInput {
    symbol: UnitSymbolId,
    initializer: ExpressionId,
    expected: Option<OwnedValue>,
    catalog: Option<UnitOwnerProducerCatalogV1>,
    entry: Option<UnitOwnerLoopBindingV1>,
    body_completion: Option<UnitOwnerLoopBindingV1>,
    transfer: Option<(std::sync::Arc<()>, UnitExpressionId, UnitOwnerLoopBindingV1)>,
    split: Option<split_terminal::InvariantTerminalSplitInput>,
}
impl InvariantInput {
    pub(super) fn admit(
        checker: &Checker<'_>,
        prefix: StatementId,
        suffix: StatementId,
    ) -> Option<Self> {
        let ast = checker.parsed.ast();
        let variable = |statement| {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                name,
                kind: VariableKind::Val,
                initializer,
                ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            let symbol = *checker.marker_symbol(*name)?;
            (checker
                .typed
                .symbol_type(symbol)
                .and_then(|ty| checker.typed.types().get(ty))
                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && checker.is_move_only_variable(symbol)
                && exact_type(checker, *initializer, BuiltinType::String))
            .then_some((symbol, *initializer))
        };
        let (symbol, initializer) = variable(prefix)?;
        let Expression::String { parts } = ast.expressions().get(initializer).ok()?.payload()
        else {
            return None;
        };
        if parts
            .iter()
            .any(|part| matches!(part, StringPart::Interpolation { .. }))
        {
            return None;
        }
        let (retired, use_expression) = variable(suffix)?;
        let use_node = ast.expressions().get(use_expression).ok()?;
        if retired == symbol
            || !matches!(use_node.payload(), Expression::Name)
            || checker.reference_symbol(use_node.span()) != Some(symbol)
        {
            return None;
        }
        Some(Self {
            symbol,
            initializer,
            expected: None,
            catalog: None,
            entry: None,
            body_completion: None,
            transfer: None,
            split: None,
        })
    }

    // Before the ordinary While rejection clears its prefix, require the actual Result/Bind.
    fn capture(
        &mut self,
        checker: &Checker<'_>,
        observation: &UnitOwnerObservation,
        state: &ValueState,
        scope: usize,
    ) {
        let Some([value]) = boundary_values(state) else {
            return;
        };
        let Some(owner) = &value.owner else {
            return;
        };
        let initializer = checker.unit_expression(self.initializer);
        let [
            UnitOwnerTransportEvent::Result {
                expression,
                owner: result,
            },
            UnitOwnerTransportEvent::Binding {
                symbol,
                initializer: bound,
                owner: binding,
            },
        ] = observation.events.as_slice()
        else {
            return;
        };
        if value.symbol != self.symbol
            || value.scope_depth != scope
            || *expression != initializer
            || *bound != initializer
            || *symbol != self.symbol
            || result != owner
            || binding != owner
        {
            return;
        }
        let Some(catalog) = UnitOwnerProducerCatalogV1::from_legacy(observation) else {
            return;
        };
        if catalog.definitions().len() != 1
            || catalog.definition(owner.clone()).is_none_or(|definition| {
                definition.origin() != UnitOwnerOrigin::String(initializer)
            })
        {
            return;
        }
        self.expected = Some(value.clone());
        self.catalog = Some(catalog);
    }

    // Full OwnedValue equality includes origin, declaration and scope, not just symbol/token.
    fn snapshot(&self, state: &ValueState) -> Option<UnitOwnerLoopBindingV1> {
        let [value] = boundary_values(state)? else {
            return None;
        };
        if Some(value) != self.expected.as_ref() {
            return None;
        }
        Some(UnitOwnerLoopBindingV1 {
            symbol: value.symbol,
            owner: value.owner.clone()?,
        })
    }

    pub(super) fn finish(
        &mut self,
        closed: UnitOwnerLoopObservationV1,
        state: &ValueState,
    ) -> Option<InvariantLoopValueV1> {
        let exit = self.snapshot(state)?;
        if let Some(split) = self.split.take() {
            if self.transfer.is_some() || self.body_completion.is_some() {
                return None;
            }
            return split.finish(closed, self.catalog.take()?, self.entry.take()?, exit);
        }
        let evidence = InvariantLoopBoundaries {
            closed,
            inputs: self.catalog.take()?,
            entry: self.entry.take()?,
            body_completion: self.body_completion.take()?,
            exit,
            transfer: self.transfer.take(),
        };
        match evidence.closed.completion {
            LoopBodyCompletion::Break(_) => {
                let value = UnitOwnerInvariantBreakObservationV1 { evidence };
                value
                    .is_consistent()
                    .then_some(InvariantLoopValueV1::Break(value))
            }
            LoopBodyCompletion::Fallthrough | LoopBodyCompletion::Continue(_) => {
                let value = UnitOwnerInvariantLoopObservationV1 { evidence };
                value
                    .is_consistent()
                    .then_some(InvariantLoopValueV1::Backedge(value))
            }
            LoopBodyCompletion::Conditional { .. } => None,
        }
    }
}
impl LoopTransaction {
    // Called only after the real jump cleanup and precise active-sink/kind checks.
    pub(super) fn jump_boundary(
        &mut self,
        completion: LoopBodyCompletion,
        identity: &std::sync::Arc<()>,
        state: &ValueState,
    ) -> bool {
        let Some(input) = &mut self.invariant else {
            return empty_boundary(state);
        };
        let (LoopBodyCompletion::Continue(jump) | LoopBodyCompletion::Break(jump)) = completion
        else {
            return false;
        };
        if let Some(branch) = self.active_branch {
            return input.split_jump_boundary(branch, completion, identity, state);
        }
        if input.transfer.is_some() || input.split.is_some() {
            return false;
        }
        let Some(snapshot) = input.snapshot(state) else {
            return false;
        };
        input.transfer = Some((identity.clone(), jump, snapshot));
        true
    }
    pub(super) fn entry_boundary(&mut self, state: &ValueState) -> bool {
        if let Some(input) = &mut self.invariant {
            input.entry = input.snapshot(state);
            input.entry.is_some()
        } else {
            empty_boundary(state)
        }
    }
    pub(super) fn body_completion_boundary(&mut self, state: &ValueState) -> bool {
        if let Some(input) = &mut self.invariant {
            if let Some(split) = &input.split {
                return split.is_complete() && empty_boundary(state);
            }
            input.body_completion = input.snapshot(state);
            input.body_completion.is_some()
        } else {
            empty_boundary(state)
        }
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn prepare_owner_loop_input(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) {
        if let (Some(transaction), Some(observation)) =
            (&mut self.owner_loop, &self.owner_observation)
            && transaction.statement == id
            && let Some(input) = &mut transaction.invariant
        {
            input.capture(self.checker, observation, state, self.scope_depth);
        }
    }
}
