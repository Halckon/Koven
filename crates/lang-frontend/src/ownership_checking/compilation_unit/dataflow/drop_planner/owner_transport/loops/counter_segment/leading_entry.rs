//! First-control scalar completion, frozen at the actual while entry.
use super::*;
pub(in super::super) struct LeadingEntry {
    control: UnitStatementId,
    tracker: entry::Entry,
    entered: bool,
    valid: bool,
}
impl LeadingEntry {
    pub(in super::super) fn discover(checker: &Checker<'_>, item: ItemId) -> Option<Self> {
        let candidates = root_incoming::counter_prefixes(checker, item);
        if candidates.len() != 3 {
            return None;
        }
        let (control, comparison) = candidates[0];
        let ast = checker.parsed.ast();
        let Item::Function { form, .. } = ast.items().get(item).ok()?.payload() else {
            return None;
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return None,
        };
        let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
            return None;
        };
        let end = elements.iter().position(|id| *id == control.statement())?;
        let statements = elements[..end]
            .iter()
            .map(|id| UnitStatementId::new(checker.source_unit, *id))
            .collect::<Vec<_>>();
        Some(Self {
            control,
            tracker: entry::Entry::discover(checker, &statements, comparison)?,
            entered: false,
            valid: true,
        })
    }
    pub(super) fn completed(
        &mut self,
        statement: UnitStatementId,
        symbol: UnitSymbolId,
        expression: UnitExpressionId,
    ) {
        if self.entered || !self.valid {
            return;
        }
        #[cfg(test)]
        if skip((self.control, statement, symbol, expression)) {
            return;
        }
        #[cfg(test)]
        if let Some((phase, second)) = event_fault((self.control, statement, symbol, expression)) {
            if phase == 1 {
                self.tracker.completed(statement, symbol, expression, false);
            } else if phase == 2 {
                self.tracker.completed(second.1, second.2, second.3, false);
            }
        }
        self.tracker.completed(statement, symbol, expression, false);
    }
    pub(in super::super) fn enter(&mut self, control: UnitStatementId) {
        if control != self.control {
            return;
        }
        if self.entered || self.tracker.sites().is_none() {
            self.valid = false;
        }
        self.entered = true;
        #[cfg(test)]
        if self.valid
            && self
                .tracker
                .sites()
                .is_some_and(|(symbols, expressions, statements)| {
                    enter_fault(
                        (control, statements[0], symbols[0], expressions[0]),
                        (control, statements[1], symbols[1], expressions[1]),
                    )
                })
        {
            // Re-enter the real transition, rather than directly changing its validity bit.
            self.enter(control);
        }
    }
    pub(super) fn sites(
        &self,
        control: UnitStatementId,
    ) -> Option<(
        [UnitSymbolId; 2],
        [UnitExpressionId; 2],
        [UnitStatementId; 2],
    )> {
        if !self.valid || !self.entered || self.control != control {
            return None;
        }
        self.tracker.sites()
    }
}

#[cfg(test)]
thread_local! { static SKIP: std::cell::Cell<Option<SkippedEvent>> = const { std::cell::Cell::new(None) }; static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
type SkippedEvent = (
    UnitStatementId,
    UnitStatementId,
    UnitSymbolId,
    UnitExpressionId,
);
#[cfg(test)]
fn skip(event: SkippedEvent) -> bool {
    if SKIP.get() == Some(event) {
        HITS.set(HITS.get() + 1);
        true
    } else {
        false
    }
}
#[cfg(test)]
pub(super) fn inject_skip(event: Option<SkippedEvent>) {
    SKIP.set(event);
    HITS.set(0);
}
#[cfg(test)]
pub(super) fn hits() -> usize {
    HITS.get()
}

#[cfg(test)]
thread_local! { static FAULT: std::cell::Cell<Option<(usize, SkippedEvent, SkippedEvent)>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
fn event_fault(event: SkippedEvent) -> Option<(usize, SkippedEvent)> {
    let (phase, first, second) = FAULT.get()?;
    if first != event || !matches!(phase, 1 | 2) {
        return None;
    }
    HITS.set(HITS.get() + 1);
    Some((phase, second))
}
#[cfg(test)]
fn enter_fault(first: SkippedEvent, second: SkippedEvent) -> bool {
    if FAULT.get() == Some((3, first, second)) {
        HITS.set(HITS.get() + 1);
        true
    } else {
        false
    }
}
#[cfg(test)]
pub(super) fn inject_fault(fault: Option<(usize, SkippedEvent, SkippedEvent)>) {
    FAULT.set(fault);
    HITS.set(0);
}
