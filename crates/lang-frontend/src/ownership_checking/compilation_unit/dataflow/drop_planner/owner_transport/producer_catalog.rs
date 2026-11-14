//! Independent per-planner construction. No legacy owner tokens, no certificates.
use super::*;
use std::sync::Arc;
mod coverage;

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
    coverage: Option<coverage::Coverage>,
    bindings: Vec<(UnitSymbolId, UnitExpressionId)>,
    source: crate::name_resolution::SourceUnitId,
    definitions: Vec<(UnitExpressionId, Token)>,
    roots: Vec<(UnitSymbolId, Token)>,
    pending: Vec<(UnitExpressionId, Token)>,
    retirements: Vec<(Token, crate::ownership_checking::UnitDropFact)>,
    replacements: Vec<(UnitSymbolId, Token, Token)>,
    failed: bool,
}
pub(in super::super) struct Closed {
    _identity: Arc<()>,
}

impl Catalog {
    pub(in super::super) fn definitions(&self) -> &[(UnitExpressionId, Token)] {
        &self.definitions
    }
    pub(in super::super) fn roots(&self) -> &[(UnitSymbolId, Token)] {
        &self.roots
    }
    pub(in super::super) fn retirements(
        &self,
    ) -> &[(Token, crate::ownership_checking::UnitDropFact)] {
        &self.retirements
    }
    pub(in super::super) fn replacements(&self) -> &[(UnitSymbolId, Token, Token)] {
        &self.replacements
    }
    pub(in super::super) fn failed(&self) -> bool {
        self.failed
    }
    pub(super) fn reject(&mut self) {
        self.failed = true;
    }
    pub(in super::super) fn close(&self) -> Option<Closed> {
        let expected = self.coverage.as_ref()?;
        if self.failed
            || !self.roots.is_empty()
            || !self.pending.is_empty()
            || self.bindings != expected.bindings
            || self
                .definitions
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>()
                != expected.literals
            || self.replacements.len() != 1
            || self.replacements[0].0 != expected.bindings[0].0
            || self.bindings[1].1 != expected.call
            || self.retirements.len() != self.definitions.len()
        {
            return None;
        }
        if self.definitions.iter().any(|(_, token)| {
            self.retirements
                .iter()
                .filter(|(actual, _)| actual == token)
                .count()
                != 1
        }) {
            return None;
        }
        let (_, old, new) = &self.replacements[0];
        if old != &self.definitions[0].1 || new != &self.definitions[1].1 {
            return None;
        }
        for (root, token) in [(expected.bindings[0].0, new), (expected.bindings[1].0, old)] {
            if !self.retirements.iter().any(|(retired, fact)| {
                retired == token
                    && fact.target() == crate::ownership_checking::UnitDropTarget::Named(root)
            }) {
                return None;
            }
        }
        if !self.retirements.iter().any(|(token, fact)| {
            token == old
                && fact.point()
                    == crate::ownership_checking::UnitDropPoint::AfterStatement(
                        expected.old_statement,
                    )
        }) {
            return None;
        }
        if !self.retirements.iter().any(|(token, fact)| {
            token == new
                && fact.point()
                    == crate::ownership_checking::UnitDropPoint::CallReturn(expected.call)
        }) {
            return None;
        }
        Some(Closed {
            _identity: self.identity.clone(),
        })
    }

    pub(in super::super) fn pending_is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    fn new(source: crate::name_resolution::SourceUnitId) -> Self {
        Self {
            identity: Arc::new(()),
            coverage: None,
            bindings: Vec::new(),
            source,
            definitions: Vec::new(),
            roots: Vec::new(),
            pending: Vec::new(),
            retirements: Vec::new(),
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
        self.bindings.push((root, expression));
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
    pub(super) fn drop_root(
        &mut self,
        root: UnitSymbolId,
        fact: crate::ownership_checking::UnitDropFact,
    ) {
        if self.failed {
            return;
        }
        if let Some(index) = self.roots.iter().position(|(id, _)| *id == root) {
            let (_, token) = self.roots.remove(index);
            self.retirements.push((token, fact));
        } else {
            self.failed = true;
        }
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super) fn enable_producer_catalog(&mut self) {
        let mut catalog = Catalog::new(self.checker.source_unit);
        catalog.coverage = self.catalog_coverage();
        catalog.failed = catalog.coverage.is_none();
        self.producer_catalog = Some(catalog);
    }
}
