//! Bounded safety gate for shared capture environments crossing loop edges.
use super::{Checker, Flows, OwnershipCheckingError, State};
use crate::{
    ast::StatementId,
    parser::{Expression, Statement},
};

impl Checker<'_> {
    /// A new shared environment needs conditional owner transport before it can
    /// survive a loop edge. Keep already-established environments and body-local
    /// callbacks on their existing path; never publish an unsafe cleanup plan.
    pub(super) fn check_loop_capture_transport(
        &mut self,
        statement: StatementId,
        body: StatementId,
        entry: &State,
        flows: &Flows,
    ) -> Result<(), OwnershipCheckingError> {
        // Discharge only independently proven zero-iteration shapes. Normal
        // source/condition evaluation is retained, including container allocation.
        // Never infer an empty provider from its spelling or runtime size.
        let (mut operand, is_while) = match self.parsed.ast().statements().get(statement)?.payload()
        {
            Statement::While { condition, .. } => (Some(*condition), true),
            Statement::For { source, .. } => (Some(*source), false),
            _ => (None, false),
        };
        while let Some(id) = operand {
            match self.parsed.ast().expressions().get(id)?.payload() {
                Expression::Group { expression } => operand = Some(*expression),
                Expression::Literal(crate::parser::LiteralKind::Boolean(false)) if is_while => {
                    return Ok(());
                }
                _ => {
                    if !is_while && self.typed.container_construction(self.unit_expression(id)).is_some_and(|descriptor|
                        matches!(descriptor.kind(), crate::type_checking::ContainerConstructionKind::ListForm | crate::type_checking::ContainerConstructionKind::EmptyMutableList)
                            && descriptor.parameter_modes().is_empty()) {
                        return Ok(());
                    }
                    break;
                }
            }
        }
        let body_span = self.parsed.ast().statements().get(body)?.span();
        if self.capture_index.is_empty() {
            return Ok(());
        }
        let mut unsupported = None;
        for (state, repeating) in [
            (flows.next.as_ref(), true),
            (flows.continues.as_ref(), true),
            (flows.breaks.as_ref(), false),
        ] {
            let Some(state) = state else { continue };
            for (&holder, origins) in &state.closures {
                let live = self.statement_live_after[statement.index()].contains(&holder)
                    || (repeating && self.statement_live_after[body.index()].contains(&holder));
                if !live || self.symbol_span(holder)?.start() >= body_span.start() {
                    continue;
                }
                let prior =
                    entry.closure_tree(entry.closures.get(&holder).into_iter().flatten().copied());
                let current = state.closure_tree(origins.iter().copied());
                // Lambda IDs do not encode capture publication order. Keep the
                // same first matching descriptor as the original ordered walk.
                for &lambda in current.difference(&prior) {
                    for (ordinal, capture) in self.capture_index.for_lambda(lambda) {
                        if unsupported.is_some_and(|(earlier, _)| earlier <= ordinal) {
                            break;
                        }
                        if capture.borrow_kind()
                            != Some(crate::ownership_checking::LoanKind::Shared)
                            || self.typed.copyability(capture.ty())
                                != crate::type_checking::Copyability::MoveOnly
                        {
                            continue;
                        }
                        if let crate::ownership_checking::UnitClosureCaptureSource::Symbol(source) =
                            capture.source()
                            && !entry.non_owning.contains_key(&source)
                            && self.bindings.get(&source).is_none_or(|binding| {
                                binding.kind()
                                    == crate::ownership_checking::OwnershipBindingKind::Owned
                            })
                        {
                            unsupported = Some((ordinal, capture.reference_span()));
                            break;
                        }
                    }
                }
                if unsupported.is_some() {
                    break;
                }
            }
            if unsupported.is_some() {
                break;
            }
        }
        if let Some((_, origin)) = unsupported {
            self.emit_borrow_binding_diagnostic(
                crate::diagnostic::codes::UNSUPPORTED_BORROW_FLOW,
                "shared closure capture across a loop requires conditional owner transport",
                origin,
            )?;
        }
        Ok(())
    }
}
