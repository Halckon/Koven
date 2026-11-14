//! Actual traversal events, independent of legacy UnitOwnerId state.
//! This is a source-evaluation trace, not a runtime loop or CFG certificate.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) enum Event {
    Literal(UnitExpressionId),
    Bound(UnitSymbolId, UnitExpressionId),
    Applied {
        call: UnitExpressionId,
        root: UnitSymbolId,
        incoming: UnitExpressionId,
    },
}

impl DropPlanner<'_, '_> {
    pub(in super::super) fn origin_literal(&mut self, expression: ExpressionId) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Literal(self.checker.unit_expression(expression)));
        }
    }
    pub(in super::super) fn origin_bound(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
    ) {
        if let Some(probe) = &mut self.origin_probe {
            probe.push(Event::Bound(
                symbol,
                self.checker.unit_expression(initializer),
            ));
        }
    }
    pub(in super::super) fn origin_applied(
        &mut self,
        call: ExpressionId,
        updated_root: UnitSymbolId,
    ) {
        let Some(probe) = &mut self.origin_probe else {
            return;
        };
        let call = self.checker.unit_expression(call);
        let Some(plan) = self
            .checker
            .ownership_primitives
            .iter()
            .find(|plan| plan.descriptor().expression() == call)
        else {
            return;
        };
        if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace {
            return;
        }
        let [place] = plan.places() else { return };
        let [_, incoming] = plan.descriptor().operands();
        if !place.is_root() || place.root() != updated_root {
            return;
        }
        probe.push(Event::Applied {
            call,
            root: place.root(),
            incoming,
        });
    }
}
