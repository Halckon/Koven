//! Private recovery effects for pending values written through named inout calls.

use super::*;
use crate::{parser::ParameterModeMarker, type_checking::UnitCallTarget};

pub(super) type PendingWritebacks = BTreeSet<(UnitCallTarget, usize)>;

#[derive(Clone)]
enum PendingReturnRoots {
    Parameters(Vec<(usize, UnitSymbolId)>),
    Captures,
}

#[derive(Clone)]
pub(super) struct PendingCallableReturns {
    roots: PendingReturnRoots,
    pending: BTreeSet<UnitSymbolId>,
    has_normal_exit: bool,
    pub(super) reachable: bool,
}

impl PendingCallableReturns {
    pub(super) fn for_callable(callable: &UnitCallableSignature) -> Option<Self> {
        let parameters: Vec<_> = callable
            .parameters()
            .iter()
            .enumerate()
            .filter(|(_, parameter)| parameter.mode() == ParameterMode::Inout)
            .filter_map(|(index, parameter)| parameter.symbol().map(|symbol| (index, symbol)))
            .collect();
        (!parameters.is_empty()).then_some(Self {
            roots: PendingReturnRoots::Parameters(parameters),
            pending: BTreeSet::new(),
            has_normal_exit: false,
            reachable: true,
        })
    }

    pub(super) fn for_closure() -> Self {
        Self {
            roots: PendingReturnRoots::Captures,
            pending: BTreeSet::new(),
            has_normal_exit: false,
            reachable: true,
        }
    }

    pub(super) fn record(&mut self, flow: &FlowVersions) {
        if !self.reachable {
            return;
        }
        self.has_normal_exit = true;
        match &self.roots {
            PendingReturnRoots::Parameters(parameters) => self.pending.extend(
                parameters
                    .iter()
                    .filter_map(|&(_, symbol)| flow.mutable_is_pending(symbol).then_some(symbol)),
            ),
            PendingReturnRoots::Captures => {
                self.pending
                    .extend(flow.pending_keys().filter_map(|key| match key {
                        FlowKey::Symbol(symbol) => Some(symbol),
                        FlowKey::This => None,
                    }))
            }
        }
    }

    pub(super) fn writebacks(
        self,
        target: UnitCallTarget,
    ) -> impl Iterator<Item = (UnitCallTarget, usize)> {
        let parameters = match self.roots {
            PendingReturnRoots::Parameters(parameters) => parameters,
            PendingReturnRoots::Captures => Vec::new(),
        };
        parameters.into_iter().filter_map(move |(index, symbol)| {
            self.pending.contains(&symbol).then_some((target, index))
        })
    }

    pub(super) fn captured_exits(self) -> (BTreeSet<UnitSymbolId>, bool) {
        (self.pending, self.has_normal_exit)
    }
}

impl<'a> BodyChecker<'a> {
    pub(super) fn check_bodies_to_fixed_point(mut self) -> Result<Self, CompilationUnitTypeError> {
        // Clean units check once. Preserve the input only if a loop or an inout
        // call can consume facts discovered later during body checking.
        let baseline = self
            .files
            .iter()
            .any(|file| {
                let ast = file.ast();
                ast.statements().iter().any(|(_, node)| {
                    matches!(
                        node.payload(),
                        Statement::While { .. } | Statement::Loop { .. } | Statement::For { .. }
                    )
                }) || ast.expressions().iter().any(|(_, node)| {
                    matches!(node.payload(), Expression::Call { arguments, .. }
                    if arguments.iter().any(|argument| matches!(
                        argument.mode_marker, Some(ParameterModeMarker::Inout(_))
                    )))
                })
            })
            .then(|| self.signatures.clone());
        loop {
            self.check_source_bodies()?;
            let needs_loop_replay = !self.discovered_loop_heads.is_empty();
            let mut writebacks = std::mem::take(&mut self.call_writebacks);
            let previous_count = writebacks.len();
            // The discovery sweep of a new loop may not have reached its
            // local fixed point. Publish callable facts only after replay.
            if !needs_loop_replay && baseline.is_some() {
                writebacks.append(&mut self.discovered_call_writebacks);
            }
            if !needs_loop_replay && writebacks.len() == previous_count {
                self.call_writebacks = writebacks;
                return Ok(self);
            }
            let mut sites = std::mem::take(&mut self.loop_replay_sites);
            sites.append(&mut self.discovered_loop_heads);
            let signatures = baseline
                .as_ref()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .clone();
            self = Self::new(
                self.sources,
                self.files,
                self.names,
                self.environment,
                signatures,
            );
            self.loop_replay_sites = sites;
            self.call_writebacks = writebacks;
        }
    }

    pub(super) fn record_pending_callable_return(&mut self) {
        if let Some(returns) = &mut self.pending_callable_returns {
            returns.record(&self.flow_versions);
        }
    }

    pub(super) fn tracks_pending_call_flow(&self) -> bool {
        self.pending_callable_returns.is_some()
            || !self.call_writebacks.is_empty()
            || !self.closure_effects.is_empty()
    }

    pub(super) fn checked_callee_exits(
        &self,
        source: SourceUnitId,
        callee: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        self.checked_call_operand_exits(source, self.call_callee_operand(source, callee)?)
    }

    fn call_callee_operand(
        &self,
        source: SourceUnitId,
        callee: ExpressionId,
    ) -> Result<ExpressionId, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        Ok(match node.payload() {
            Expression::Member { receiver, .. } => *receiver,
            _ => callee,
        })
    }

    pub(super) fn call_operands_fall_through(
        &self,
        source: SourceUnitId,
        callee: ExpressionId,
        arguments: &[crate::parser::CallArgument],
    ) -> Result<bool, CompilationUnitTypeError> {
        let callee_operand = self.call_callee_operand(source, callee)?;
        Ok(std::iter::once(callee_operand)
            .chain(arguments.iter().map(|argument| argument.value))
            .all(|operand| self.operand_falls_through(source, operand)))
    }

    pub(super) fn operand_falls_through(
        &self,
        source: SourceUnitId,
        operand: ExpressionId,
    ) -> bool {
        self.parts
            .expression_falls_through
            .get(&UnitExpressionId::new(source, operand))
            != Some(&false)
    }

    pub(super) fn checked_call_operand_exits(
        &self,
        source: SourceUnitId,
        operand: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let key = UnitExpressionId::new(source, operand);
        let Some(ty) = self.parts.expression_types.get(&key).copied() else {
            return Ok(true);
        };
        let falls_through = self
            .parts
            .expression_falls_through
            .get(&key)
            .copied()
            .unwrap_or_else(|| !self.is_builtin(ty, BuiltinType::Nothing));
        self.has_pending_expression_exit(source, operand, ExpressionCheck { ty, falls_through })
    }

    pub(super) fn record_pending_call_prefix(
        &mut self,
        expression: UnitExpressionId,
        callee: ExpressionId,
        arguments: &[crate::parser::CallArgument],
    ) -> Result<(), CompilationUnitTypeError> {
        if !self.tracks_pending_call_flow() {
            return Ok(());
        }
        let source = expression.source_unit();
        let mut completes = self.checked_callee_exits(source, callee)?;
        for argument in arguments {
            if !completes {
                break;
            }
            completes = self.checked_call_operand_exits(source, argument.value)?;
        }
        if !completes {
            self.note_pending_exit(
                while_flow::PendingControlSite::Expression(expression),
                false,
            );
        }
        Ok(())
    }

    pub(super) fn apply_pending_call_writebacks(
        &mut self,
        source: SourceUnitId,
        target: UnitCallTarget,
        arguments: &[crate::parser::CallArgument],
        mapping: &[usize],
    ) {
        if self
            .call_writebacks
            .range((target, 0)..=(target, usize::MAX))
            .next()
            .is_none()
        {
            return;
        }
        for (argument, &parameter) in arguments.iter().zip(mapping) {
            if self.call_writebacks.contains(&(target, parameter))
                && let Some(key) = self.pending_flow_key(source, argument.value)
            {
                self.flow_versions.replace_pending(key, true);
            }
        }
    }
}
