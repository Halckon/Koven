//! Normal-return tape; only actual call cleanup can produce these events.
use super::{BorrowFrameChecker, CAPTURE};
#[cfg(test)]
mod tests;
use crate::{
    ast::ExpressionId,
    name_resolution::{SourceUnitId, SymbolId, UnitSymbolId},
    source::SourceId,
    type_checking::UnitExpressionId,
};
use std::{any::Any, cell::RefCell, rc::Rc};

/// Checker-qualified original expression identity, not a source-span match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowLoopExpression {
    /// File checker expression in its original source.
    Single {
        /// Original source.
        source: SourceId,
        /// Original AST expression.
        expression: ExpressionId,
    },
    /// Source-qualified compilation-unit expression.
    Unit(UnitExpressionId),
}
impl BorrowLoopExpression {
    fn source_matches(self, source: SourceId, unit: Option<SourceUnitId>) -> bool {
        match self {
            Self::Single { source: actual, .. } => unit.is_none() && actual == source,
            Self::Unit(actual) => unit == Some(actual.source_unit()),
        }
    }
    fn raw(self) -> ExpressionId {
        match self {
            Self::Single { expression, .. } => expression,
            Self::Unit(id) => id.expression(),
        }
    }
    fn checker(self) -> BorrowFrameChecker {
        match self {
            Self::Single { .. } => BorrowFrameChecker::Single,
            Self::Unit(_) => BorrowFrameChecker::Unit,
        }
    }
}
/// Actual incoming binding used by one call operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowLoopBinding {
    /// File-local binding in its source.
    Single {
        /// Original source.
        source: SourceId,
        /// Resolved binding symbol.
        symbol: SymbolId,
    },
    /// Source-qualified binding symbol.
    Unit(UnitSymbolId),
}
/// An event produced only after exact call-loan removal and normal call completion.
#[derive(Clone, Debug)]
pub struct BorrowLoopConsumerReturn {
    pub(super) identity: Rc<()>,
    call: BorrowLoopExpression,
    arguments: Vec<(BorrowLoopExpression, BorrowLoopBinding)>,
    released: usize,
}
impl BorrowLoopConsumerReturn {
    /// Original call identity.
    pub fn call(&self) -> BorrowLoopExpression {
        self.call
    }
    /// Original argument and actual carried binding identities in argument order.
    pub fn arguments(&self) -> &[(BorrowLoopExpression, BorrowLoopBinding)] {
        &self.arguments
    }
    /// Number of actual temporary call loans removed, not carried result loans.
    pub fn released_loan_count(&self) -> usize {
        self.released
    }
}
pub(super) struct Protocol {
    identity: Rc<()>,
    checker: BorrowFrameChecker,
    calls: Vec<ExpressionId>,
    entry: Box<dyn Any>,
    source: SourceId,
    source_unit: Option<SourceUnitId>,
    pub(super) valid: bool,
    pending: bool,
    pub(super) returns: Vec<BorrowLoopConsumerReturn>,
}
impl Protocol {
    pub(super) fn new<S: 'static>(
        identity: Rc<()>,
        checker: BorrowFrameChecker,
        calls: Vec<ExpressionId>,
        entry: S,
        source: SourceId,
        source_unit: Option<SourceUnitId>,
    ) -> Self {
        Self {
            identity,
            checker,
            calls,
            entry: Box::new(entry),
            source,
            source_unit,
            valid: true,
            pending: false,
            returns: Vec::new(),
        }
    }
}
pub(crate) struct Prepared<S> {
    protocol: Rc<RefCell<Protocol>>,
    expected: S,
    event: BorrowLoopConsumerReturn,
}
pub(crate) struct Released<S> {
    prepared: Prepared<S>,
}
pub(crate) fn active() -> bool {
    CAPTURE.with(|c| c.borrow().as_ref().is_some_and(|c| c.active.is_some()))
}
pub(crate) fn prepare<S: PartialEq + 'static>(
    call: BorrowLoopExpression,
    arguments: Vec<(BorrowLoopExpression, BorrowLoopBinding)>,
    expected: S,
    released: usize,
) -> Option<Prepared<S>> {
    CAPTURE.with(|c| {
        let protocol = c.borrow().as_ref()?.active.clone()?;
        let mut p = protocol.borrow_mut();
        #[cfg(test)]
        tests::before_prepare(&mut p);
        if !p.valid
            || p.pending
            || p.checker != call.checker()
            || !call.source_matches(p.source, p.source_unit)
            || arguments.iter().any(|(argument, binding)| {
                !argument.source_matches(p.source, p.source_unit)
                    || match binding {
                        BorrowLoopBinding::Single { source, .. } => {
                            p.source_unit.is_some() || *source != p.source
                        }
                        BorrowLoopBinding::Unit(binding) => {
                            p.source_unit != Some(binding.source_unit())
                        }
                    }
            })
            || p.calls.get(p.returns.len()) != Some(&call.raw())
            || arguments.is_empty()
            || released != arguments.len()
            || p.entry.downcast_ref::<S>() != Some(&expected)
        {
            p.valid = false;
            return None;
        }
        p.pending = true;
        let event = BorrowLoopConsumerReturn {
            identity: p.identity.clone(),
            call,
            arguments,
            released,
        };
        drop(p);
        Some(Prepared {
            protocol,
            expected,
            event,
        })
    })
}
pub(crate) fn release<S: PartialEq>(
    prepared: Option<Prepared<S>>,
    actual: Option<&S>,
) -> Option<Released<S>> {
    let prepared = prepared?;
    #[cfg(test)]
    if tests::skip_release() {
        return None;
    }
    if actual != Some(&prepared.expected) {
        prepared.protocol.borrow_mut().valid = false;
        return None;
    }
    Some(Released { prepared })
}
pub(crate) fn complete<S: PartialEq>(
    released: Option<Released<S>>,
    actual: Option<&S>,
    clean: bool,
) {
    let Some(Released { prepared }) = released else {
        return;
    };
    #[cfg(test)]
    let prepared = {
        let mut prepared = prepared;
        if !tests::before_complete(&mut prepared) {
            return;
        }
        prepared
    };
    CAPTURE.with(|c| {
        let c = c.borrow();
        let Some(c) = c.as_ref() else { return };
        let mut p = prepared.protocol.borrow_mut();
        if !c
            .active
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &prepared.protocol))
            || !p.valid
            || !p.pending
            || !clean
            || actual != Some(&prepared.expected)
        {
            p.valid = false;
            return;
        }
        p.pending = false;
        p.returns.push(prepared.event);
    });
}
pub(super) fn complete_tape(p: &Protocol) -> bool {
    p.valid
        && !p.pending
        && p.returns.len() == p.calls.len()
        && p.returns.iter().zip(&p.calls).all(|(event, call)| {
            Rc::ptr_eq(&event.identity, &p.identity)
                && event.call.checker() == p.checker
                && event.call.raw() == *call
                && event.call.source_matches(p.source, p.source_unit)
                && !event.arguments.is_empty()
                && event.released == event.arguments.len()
                && event.arguments.iter().all(|(argument, binding)| {
                    argument.source_matches(p.source, p.source_unit)
                        && match binding {
                            BorrowLoopBinding::Single { source, .. } => {
                                p.source_unit.is_none() && *source == p.source
                            }
                            BorrowLoopBinding::Unit(binding) => {
                                p.source_unit == Some(binding.source_unit())
                            }
                        }
                })
        })
}
