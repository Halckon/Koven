//! Experimental authorization domain; never exported as ordinary validated ownership.
use super::*;
use crate::type_checking::{CompilationUnitTypes, UnitCallTarget, UnitCallableInstanceKey};
use crate::{ast::ExpressionId, ownership_checking::loop_no_carry::CounterPlan};
use std::cell::Cell;
thread_local! { static DOMAIN: Cell<(bool, usize)> = const { Cell::new((false, 0)) }; }
pub(crate) fn enabled() -> bool {
    DOMAIN.get().0
}
fn with_domain<T>(analyze: impl FnOnce() -> T) -> (T, usize) {
    struct Restore((bool, usize));
    impl Drop for Restore {
        fn drop(&mut self) {
            DOMAIN.set(self.0);
        }
    }
    let _restore = Restore(DOMAIN.replace((true, 0)));
    let result = analyze();
    (result, DOMAIN.get().1)
}

/// Opaque experimental two-source fact, never ordinary lowering authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwoSourceCounterFact {
    identity: Rc<()>,
    branch: UnitExpressionId,
    arms: [UnitLoopNoCarryFact; 2],
    calls: [Vec<(UnitExpressionId, UnitCallableInstanceKey)>; 2],
    counter: UnitSymbolId,
    rounds: UnitSymbolId,
    initializer: UnitExpressionId,
    initial_statement: UnitStatementId,
    condition: UnitExpressionId,
    update: UnitStatementId,
    decrement: UnitExpressionId,
    decrement_operand: UnitExpressionId,
}
impl TwoSourceCounterFact {
    // Only the real completion consumer calls this after unique final End matching.
    pub(in crate::ownership_checking::compilation_unit) fn new(
        unit: SourceUnitId,
        branch: ExpressionId,
        arms: [UnitLoopNoCarryFact; 2],
        c: CounterPlan<UnitSymbolId>,
        typed: &CompilationUnitTypes,
    ) -> Option<Self> {
        if !enabled()
            || arms[0].source() == arms[1].source()
            || arms[0].callable() != arms[1].callable()
            || arms[0].loop_statement() != arms[1].loop_statement()
            || arms[0].body() == arms[1].body()
            || arms[0].binding() == arms[1].binding()
            || arms[0].end() == arms[1].end()
            || arms[0].same_identity(&arms[1])
        {
            return None;
        }
        // The closed arm sequence is the only ordering source. Never deduplicate calls.
        let calls = |arm: &UnitLoopNoCarryFact| {
            std::iter::once(arm.initializer())
                .chain(arm.consumers().iter().map(|(_, call, _)| *call))
                .map(|expression| {
                    let call = typed.call(expression)?;
                    (call.expression() == expression
                        && expression.source_unit() == unit
                        && matches!(call.target(), UnitCallTarget::Declaration(_)))
                    .then(|| (expression, call.instance().clone()))
                })
                .collect::<Option<Vec<_>>>()
        };
        let calls = [calls(&arms[0])?, calls(&arms[1])?];
        DOMAIN.set((true, DOMAIN.get().1 + 1));
        Some(Self {
            identity: Rc::new(()),
            branch: UnitExpressionId::new(unit, branch),
            arms,
            calls,
            counter: c.counter,
            rounds: c.rounds,
            initializer: UnitExpressionId::new(unit, c.initializer),
            initial_statement: UnitStatementId::new(unit, c.initial_statement),
            condition: UnitExpressionId::new(unit, c.condition),
            update: UnitStatementId::new(unit, c.update),
            decrement: UnitExpressionId::new(unit, c.decrement),
            decrement_operand: UnitExpressionId::new(unit, c.decrement_operand),
        })
    }
}
impl CompilationUnitOwnership {
    #[cfg(all(test, feature = "loan-observation-test-support"))]
    fn owns_two_source_counter(&self, fact: &TwoSourceCounterFact) -> bool {
        self.no_carry
            .two_sources
            .iter()
            .any(|f| Rc::ptr_eq(&f.identity, &fact.identity))
    }
}

#[cfg(all(test, feature = "loan-observation-test-support"))]
mod tests;

#[test]
fn domain_scope_restores() {
    assert!(!enabled());
    let (_, hits) = with_domain(|| {
        DOMAIN.set((true, 7));
        assert_eq!(with_domain(enabled), (true, 0));
        assert_eq!(DOMAIN.get(), (true, 7));
        assert!(std::panic::catch_unwind(|| with_domain(|| panic!("RAII sentinel"))).is_err());
        assert_eq!(DOMAIN.get(), (true, 7));
    });
    assert_eq!(hits, 7);
    assert!(!enabled());
}

#[cfg(feature = "e2-two-source-test-support")]
mod bridge;
#[cfg(feature = "e2-two-source-test-support")]
pub use bridge::{TwoSourceExperiment, analyze_two_source_experiment};

#[cfg(feature = "e2-two-source-test-support")]
mod leaves;
#[cfg(feature = "e2-two-source-test-support")]
pub use leaves::{CalleeLeaf, CalleeLeafBody};
