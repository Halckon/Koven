//! 词法 scope 结束时的 closure 借用逃逸：外层仍持有的 closure 不得借用本 scope 的 binding。
//!
//! borrowed closure 的 shared capture loan 持续到 closure 的 ASAP drop point；closure 经赋值或
//! block 结果流到外层 binding 后，被捕获的 scope 内 binding 在 scope 结束时必须析构，loan 因此
//! 比被借 binding 活得更久（L0135）。
use std::collections::{BTreeMap, BTreeSet};

use super::{ActiveLoanOwner, ActiveLoanTarget, Checker, Flows, OwnershipCheckingError};
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    name_resolution::UnitSymbolId,
    parser::{Item, Statement},
    source::Span,
};

impl Checker<'_> {
    /// Bindings declared directly in a scope's elements, with their declaration spans.
    pub(super) fn scope_locals(
        &self,
        elements: &[StatementId],
    ) -> Result<BTreeMap<UnitSymbolId, Span>, OwnershipCheckingError> {
        let mut locals = BTreeMap::new();
        for element in elements {
            let Statement::LocalVariable { declaration } =
                self.parsed.ast().statements().get(*element)?.payload()
            else {
                continue;
            };
            let Item::Variable { name, .. } =
                self.parsed.ast().items().get(*declaration)?.payload()
            else {
                continue;
            };
            if let (Some(symbol), crate::parser::NameMarker::Present(span)) =
                (self.marker_symbol(*name).copied(), *name)
            {
                locals.insert(symbol, span);
            }
        }
        Ok(locals)
    }

    /// Reports shared capture loans on `locals` that are still held, when the scope ends, by
    /// closures reachable from an outer binding or from the scope's result `tail`.
    pub(super) fn check_scope_exit_loans(
        &mut self,
        locals: &BTreeMap<UnitSymbolId, Span>,
        tail: Option<ExpressionId>,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        if locals.is_empty() {
            return Ok(());
        }
        let tail = tail.map(|expression| self.unit_expression(expression));
        let mut escaping = Vec::<(Span, Span)>::new();
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues]
            .into_iter()
            .flatten()
        {
            let roots = state
                .closures
                .iter()
                .filter(|(holder, _)| !locals.contains_key(holder))
                .flat_map(|(_, closures)| closures.iter().copied())
                .chain(
                    tail.and_then(|tail| state.closure_results.get(&tail))
                        .into_iter()
                        .flatten()
                        .copied(),
                )
                .collect::<Vec<_>>();
            let direct = roots.iter().copied().collect::<BTreeSet<_>>();
            let held = state.closure_tree(roots);
            let mut released = BTreeSet::new();
            let mut found = Vec::new();
            for (index, loan) in state.loans.iter().enumerate() {
                if let (ActiveLoanOwner::Closure(closure), ActiveLoanTarget::Place(place)) =
                    (&loan.owner, &loan.target)
                    && held.contains(closure)
                    && let Some(declaration) = locals.get(&place.root())
                {
                    found.push((direct.contains(closure), loan.origin, *declaration));
                    released.insert(index);
                }
            }
            // A directly held closure's escape is the root cause; loans of closures it only
            // reaches through captures are reported when no direct escape explains them.
            let any_direct = found.iter().any(|(direct, _, _)| *direct);
            for (direct, origin, declaration) in found {
                if (direct || !any_direct)
                    && !escaping.iter().any(|(reported, _)| *reported == origin)
                {
                    escaping.push((origin, declaration));
                }
            }
            // One report per escape: the escaping loans no longer constrain this path.
            let mut index = 0;
            state.loans.retain(|_| {
                let keep = !released.contains(&index);
                index += 1;
                keep
            });
        }
        for (origin, declaration) in escaping {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.loan_conflict,
                "captured binding does not live as long as the closure that borrows it",
                origin,
            )?;
            diagnostic.add_label(
                self.sources,
                declaration,
                "declared in a scope that ends while the closure is still held outside it",
            )?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }
}
