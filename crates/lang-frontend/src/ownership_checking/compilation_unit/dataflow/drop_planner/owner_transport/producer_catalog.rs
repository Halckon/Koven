//! Independent per-planner construction. No legacy owner tokens, no certificates.
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(in super::super) struct Token {
    identity: Arc<()>,
    index: usize,
}
impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity) && self.index == other.index
    }
}
impl Eq for Token {}

pub(in super::super) struct Catalog {
    identity: Arc<()>,
    source: crate::name_resolution::SourceUnitId,
    pub(in super::super) definitions: Vec<(UnitExpressionId, Token)>,
    pub(in super::super) roots: Vec<(UnitSymbolId, Token)>,
    pending: Vec<(UnitExpressionId, Token)>,
    pub(in super::super) replacements: Vec<(UnitSymbolId, Token, Token)>,
    pub(in super::super) failed: bool,
}
impl Catalog {
    pub(in super::super) fn pending_is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    fn new(source: crate::name_resolution::SourceUnitId) -> Self {
        Self {
            identity: Arc::new(()),
            source,
            definitions: Vec::new(),
            roots: Vec::new(),
            pending: Vec::new(),
            replacements: Vec::new(),
            failed: false,
        }
    }
    pub(super) fn literal(&mut self, expression: UnitExpressionId) {
        if self.failed {
            return;
        }
        if expression.source_unit() != self.source
            || self.definitions.iter().any(|(id, _)| *id == expression)
        {
            self.failed = true;
            return;
        }
        let token = Token {
            identity: self.identity.clone(),
            index: self.definitions.len(),
        };
        self.definitions.push((expression, token.clone()));
        self.pending.push((expression, token));
    }
    pub(super) fn bind(&mut self, root: UnitSymbolId, expression: UnitExpressionId) {
        if self.failed {
            return;
        }
        let Some(index) = self.pending.iter().position(|(id, _)| *id == expression) else {
            self.failed = true;
            return;
        };
        if root.source_unit() != self.source || self.roots.iter().any(|(id, _)| *id == root) {
            self.failed = true;
            return;
        }
        let (_, token) = self.pending.remove(index);
        self.roots.push((root, token));
    }
    pub(super) fn replace(
        &mut self,
        root: UnitSymbolId,
        call: UnitExpressionId,
        incoming: UnitExpressionId,
    ) {
        if self.failed {
            return;
        }
        let Some(index) = self.roots.iter().position(|(id, _)| *id == root) else {
            self.failed = true;
            return;
        };
        let Some(value) = self.pending.iter().position(|(id, _)| *id == incoming) else {
            self.failed = true;
            return;
        };
        if call.source_unit() != self.source || self.pending.iter().any(|(id, _)| *id == call) {
            self.failed = true;
            return;
        }
        let (_, new) = self.pending.remove(value);
        let old = std::mem::replace(&mut self.roots[index].1, new.clone());
        self.replacements.push((root, old.clone(), new));
        self.pending.push((call, old));
    }
    pub(super) fn drop_root(&mut self, root: UnitSymbolId) {
        if self.failed {
            return;
        }
        if let Some(index) = self.roots.iter().position(|(id, _)| *id == root) {
            self.roots.remove(index);
        } else {
            self.failed = true;
        }
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super) fn enable_producer_catalog(&mut self) {
        self.producer_catalog = Some(Catalog::new(self.checker.source_unit));
    }
}
