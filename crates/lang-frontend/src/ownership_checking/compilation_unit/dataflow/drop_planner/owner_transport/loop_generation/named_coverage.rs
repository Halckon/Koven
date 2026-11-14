//! Independent wrapper roles. Combined with the callee visit set, never a cropped ParsedFile.
use super::*;
use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    parser::Item,
    type_checking::UnitItemId,
};

pub(in super::super) struct NamedCoverage {
    pub(super) entry: UnitItemId,
    pub(super) callee: UnitItemId,
    pub(super) roots: [UnitSymbolId; 2],
    pub(super) initializers: [UnitExpressionId; 2],
    pub(super) call: UnitExpressionId,
    pub(super) arguments: [UnitExpressionId; 2],
    pub(super) items: Vec<ItemId>,
    pub(super) statements: Vec<StatementId>,
    pub(super) expressions: Vec<ExpressionId>,
}

impl DropPlanner<'_, '_> {
    pub(in super::super) fn named_wrapper_coverage(&self) -> Option<NamedCoverage> {
        let c = self.checker;
        let ast = c.parsed.ast();
        let [first_root, second_root] = c.parsed.roots() else {
            return None;
        };
        if !c.parsed.diagnostics().is_empty() {
            return None;
        }
        // Locate roles without changing source traversal order. The typed direct call below
        // must target the other original root; two zero-parameter roots are not a selection.
        let zero_parameters = |root| {
            matches!(ast.items().get(root).ok()?.payload(), Item::Function { parameters, .. } if parameters.is_empty()).then_some(())
        };
        let (callee, entry) = match (zero_parameters(*first_root), zero_parameters(*second_root)) {
            (None, Some(())) => (first_root, second_root),
            (Some(()), None) => (second_root, first_root),
            _ => return None,
        };
        let roles = self.wrapper_roles(*entry)?;
        if roles.callee.source_unit() != c.source_unit || roles.callee.item() != *callee {
            return None;
        }
        Some(NamedCoverage {
            entry: roles.entry,
            callee: roles.callee,
            roots: roles.roots,
            initializers: roles.initializers,
            call: roles.call,
            arguments: roles.arguments,
            items: roles.items,
            statements: roles.statements,
            expressions: roles.expressions,
        })
    }

    /// Independent complete single-root source envelope. It grants no final-owned capability.
    pub(super) fn split_wrapper_coverage(&self) -> Option<SplitWrapperCoverage> {
        let c = self.checker;
        let [entry] = c.parsed.roots() else {
            return None;
        };
        let roles = self.wrapper_roles(*entry)?;
        if roles.callee.source_unit() == c.source_unit {
            return None;
        }
        let declaration = c.names.index().declarations().get(roles.target.index())?;
        if declaration.id() != roles.target
            || declaration.kind() != crate::name_resolution::SymbolKind::Function
            || declaration.source_unit() != roles.callee.source_unit()
            || declaration.root() != roles.callee.item()
        {
            return None;
        }
        let ast = c.parsed.ast();
        if roles.items.len() != 3
            || roles.statements.len() != 4
            || roles.expressions.len() != 6
            || ast.items().iter().count() != 3
            || ast.statements().iter().count() != 4
            || ast.expressions().iter().count() != 6
            || ast
                .items()
                .iter()
                .any(|(id, _)| roles.items.iter().filter(|v| **v == id).count() != 1)
            || ast
                .statements()
                .iter()
                .any(|(id, _)| roles.statements.iter().filter(|v| **v == id).count() != 1)
            || ast
                .expressions()
                .iter()
                .any(|(id, _)| roles.expressions.iter().filter(|v| **v == id).count() != 1)
        {
            return None;
        }
        Some(SplitWrapperCoverage {
            entry: roles.entry,
            callee: roles.callee,
            target: roles.target,
            roots: roles.roots,
            initializers: roles.initializers,
            call: roles.call,
            arguments: roles.arguments,
        })
    }
}

/// Separate admission result; never converted to the old two-root NamedCoverage.
pub(super) struct SplitWrapperCoverage {
    pub(super) entry: UnitItemId,
    pub(super) callee: UnitItemId,
    pub(super) target: crate::name_resolution::DeclarationId,
    pub(super) roots: [UnitSymbolId; 2],
    pub(super) initializers: [UnitExpressionId; 2],
    pub(super) call: UnitExpressionId,
    pub(super) arguments: [UnitExpressionId; 2],
}
