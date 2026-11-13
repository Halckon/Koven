//! Actual scalar declaration completion. Discovery alone never seals an entry.
use super::*;
pub(super) struct Entry {
    symbols: [UnitSymbolId; 2],
    expressions: [UnitExpressionId; 2],
    statements: [UnitStatementId; 2],
    next: usize,
    valid: bool,
}
impl Entry {
    // Called only with counter_prefixes' admitted comparison (pair[1].1).
    // Its prefix_segment already proves Greater, the same Int counter, and integer RHS 0.
    // This tracker adds ordered actual initializer completion, not wider syntax admission.
    pub(super) fn discover(
        checker: &Checker<'_>,
        statements: &[UnitStatementId],
        comparison: UnitExpressionId,
    ) -> Option<Self> {
        let ast = checker.parsed.ast();
        let Expression::Binary {
            left,
            operator: crate::parser::BinaryOperator::Greater,
            right,
            ..
        } = ast
            .expressions()
            .get(comparison.expression())
            .ok()?
            .payload()
        else {
            return None;
        };
        if !matches!(
            ast.expressions().get(*left).ok()?.payload(),
            Expression::Name
        ) || !exact_type(checker, *left, BuiltinType::Int)
            || !exact_type(checker, comparison.expression(), BuiltinType::Boolean)
            || !matches!(
                ast.expressions().get(*right).ok()?.payload(),
                Expression::Literal(crate::parser::LiteralKind::Integer(_))
            )
        {
            return None;
        }
        let counter = checker.reference_symbol(ast.expressions().get(*left).ok()?.span())?;
        let mut counter_site = None;
        let mut guard_site = None;
        for (index, statement) in statements.iter().enumerate() {
            let Statement::LocalVariable { declaration } =
                ast.statements().get(statement.statement()).ok()?.payload()
            else {
                continue;
            };
            let Item::Variable {
                name, initializer, ..
            } = ast.items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            let symbol = *checker.marker_symbol(*name)?;
            if symbol == counter {
                if counter_site.is_some()
                    || !exact_type(checker, *initializer, BuiltinType::Int)
                    || !matches!(
                        ast.expressions().get(*initializer).ok()?.payload(),
                        Expression::Name
                            | Expression::Literal(crate::parser::LiteralKind::Integer(_))
                    )
                {
                    return None;
                }
                counter_site = Some((index, *statement, checker.unit_expression(*initializer)));
            }
            if checker.unit_expression(*initializer) == comparison {
                guard_site = Some((index, *statement, symbol));
            }
        }
        let (counter_index, counter_statement, initial) = counter_site?;
        let (guard_index, guard_statement, guard) = guard_site?;
        if counter_index >= guard_index || counter == guard {
            return None;
        }
        Some(Self {
            symbols: [counter, guard],
            expressions: [initial, comparison],
            statements: [counter_statement, guard_statement],
            next: 0,
            valid: true,
        })
    }
    pub(super) fn completed(
        &mut self,
        statement: UnitStatementId,
        symbol: UnitSymbolId,
        expression: UnitExpressionId,
        inject: bool,
    ) {
        let _ = inject;
        if !self.valid || !self.symbols.contains(&symbol) {
            return;
        }
        #[cfg(test)]
        if inject && tests::skip(self.next + 1) {
            return;
        }
        #[cfg(test)]
        let expression = if inject && tests::skip(4) {
            self.expressions[1]
        } else {
            expression
        };
        if self.statements.get(self.next) != Some(&statement)
            || self.symbols.get(self.next) != Some(&symbol)
            || self.expressions.get(self.next) != Some(&expression)
        {
            self.valid = false;
            return;
        }
        self.next += 1;
        #[cfg(test)]
        if inject && tests::skip(3) {
            self.completed(statement, symbol, expression, inject);
        }
    }
    pub(super) fn sites(
        &self,
    ) -> Option<(
        [UnitSymbolId; 2],
        [UnitExpressionId; 2],
        [UnitStatementId; 2],
    )> {
        (self.valid && self.next == 2).then_some((self.symbols, self.expressions, self.statements))
    }
}
impl ControlTransactions {
    pub(in super::super::super::super) fn bridge_scalar_completed(
        &mut self,
        statement: UnitStatementId,
        symbol: UnitSymbolId,
        expression: UnitExpressionId,
    ) {
        if self.root_branches_invalid || !self.root_branch_arms.is_empty() {
            return;
        }
        if let Some(leading) = &mut self.leading_entry {
            leading.completed(statement, symbol, expression);
        }
        let Some(segment) = self
            .active_segment()
            .filter(|s| s.bridge.is_some() || s.continuation.is_some())
        else {
            return;
        };
        if let Some(entry) = &mut segment.entry {
            #[cfg(test)]
            if segment.continuation.is_some()
                && entry.symbols.contains(&symbol)
                && super::chain::tests::skip_scalar()
            {
                return;
            }
            if segment
                .next
                .checked_sub(1)
                .and_then(|i| segment.statements.get(i))
                != Some(&statement)
            {
                entry.valid = false;
                return;
            }
            entry.completed(statement, symbol, expression, true);
        }
    }
}
#[cfg(test)]
mod tests {
    thread_local! {static PHASE:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};static HITS:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
    #[test]
    fn ci_only_bridge_scalar_entry_requires_actual_ordered_completion() {
        use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
        let text = r#"fun run(n: Int) {
            var n0=n; var g0=n0>0; var a0="a"; var b0="b";
            while(g0) {val x=replace(&a0,"x"); val y=replace(&b0,"y"); n0=n0-1; g0=n0>0};
            var n1=n; var g1=n1>0; var a1=a0; var b1=b0;
            while(g1) {val x=replace(&a1,"x1"); val y=replace(&b1,"y1"); n1=n1-1; g1=n1>0};
            val x=a1; val y=b1
        }"#;
        let fixture = Fixture::new(&[("bridge-entry-events.ko", text)]);
        let baseline = fixture.shared();
        assert!(
            baseline.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap()
                .next_counter_entry()
                .is_some()
        );
        for phase in 1..=4 {
            PHASE.set(phase);
            HITS.set(0);
            let owned = fixture.shared();
            PHASE.set(0);
            assert_eq!(HITS.get(), 1);
            let bridge = owned.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap();
            assert!(bridge.next_normal_transport().is_some());
            assert!(bridge.next_counter_tail().is_some());
            assert!(bridge.next_counter_entry().is_none());
        }
    }
    pub(super) fn skip(phase: usize) -> bool {
        if PHASE.get() == phase && HITS.get() == 0 {
            HITS.set(1);
            true
        } else {
            false
        }
    }
}
