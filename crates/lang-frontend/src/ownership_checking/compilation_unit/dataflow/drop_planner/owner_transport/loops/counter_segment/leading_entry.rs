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
