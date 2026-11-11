//! Private first-body token transaction; no public loop/header or retirement capability.
use super::*;
#[cfg(test)]
mod tests;

/// Finite static catalog conservation, not a runtime activation or fixed-point proof.
#[derive(Clone, Debug)]
pub(in super::super::super::super) struct RootBoundaryEquation {
    pub(in super::super::super::super) control: UnitStatementId,
    pub(in super::super::super::super) entry: RootIncoming,
    pub(in super::super::super::super) normal: RootIncoming,
}
impl RootBoundaryEquation {
    fn same_roots(&self) -> bool {
        self.entry.len() == self.normal.len()
            && self
                .entry
                .iter()
                .zip(&self.normal)
                .all(|((root, old), (next_root, new))| {
                    root == next_root
                        && root.source_unit() == self.control.source_unit()
                        && old.callable == new.callable
                        && old != new
                })
    }
}

pub(in super::super) struct RootTokenTransaction {
    control: UnitStatementId,
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    roots: RootIncoming,
    initial_roots: RootIncoming,
    entry_definitions: Vec<UnitOwnerDefinition>,
    planned: Vec<(UnitExpressionId, UnitSymbolId, UnitExpressionId)>,
    definitions: Vec<UnitOwnerDefinition>,
    bindings: Vec<(UnitSymbolId, UnitStatementId)>,
    pending: Option<(UnitExpressionId, UnitOwnerId)>,
    bound: Option<(UnitSymbolId, UnitOwnerId)>,
    retired: usize,
    next: usize,
    valid: bool,
}
impl RootTokenTransaction {
    pub(in super::super) fn is_valid(&self) -> bool {
        self.valid
    }
    pub(in super::super) fn is_complete(&self) -> bool {
        self.valid
            && self.next == self.planned.len()
            && self.definitions.len() == self.planned.len()
            && self.pending.is_none()
            && self.bound.is_none()
            && self.retired == self.planned.len()
    }

    pub(in super::super) fn boundary_equation(&self) -> Option<RootBoundaryEquation> {
        let entry = &self.initial_roots;
        if !self.is_complete()
            || entry.len() != self.roots.len()
            || self.definitions.len() != self.roots.len()
            || entry.iter().any(|(_, old)| {
                self.entry_definitions
                    .iter()
                    .filter(|definition| definition.id == *old)
                    .count()
                    != 1
            })
            || self.definitions.iter().any(|definition| {
                self.roots
                    .iter()
                    .filter(|(_, owner)| *owner == definition.id)
                    .count()
                    != 1
            })
        {
            return None;
        }
        let equation = RootBoundaryEquation {
            control: self.control,
            entry: entry.clone(),
            normal: self.roots.clone(),
        };
        equation.same_roots().then_some(equation)
    }

    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        control: UnitStatementId,
        roots: &RootIncoming,
        entry_definitions: &[UnitOwnerDefinition],
    ) -> Option<Self> {
        if roots.len() < 2
            || entry_definitions.len() != roots.len()
            || roots.iter().any(|(_, owner)| {
                entry_definitions
                    .iter()
                    .filter(|definition| definition.id == *owner)
                    .count()
                    != 1
            })
        {
            return None;
        }
        let ast = checker.parsed.ast();
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(control.statement()).ok()?.payload()
        else {
            return None;
        };
        if !matches!(
            ast.expressions().get(*condition).ok()?.payload(),
            Expression::Name
        ) || !exact_type(checker, *condition, BuiltinType::Boolean)
        {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        if elements.len() != roots.len() {
            return None;
        }
        let mut planned = Vec::new();
        let mut bindings = Vec::new();
        for statement in elements {
            let (binding, call) = replace_break::variable(checker, *statement, VariableKind::Val)?;
            bindings.push((
                binding,
                UnitStatementId::new(control.source_unit(), *statement),
            ));
            let call = checker.unit_expression(call);
            let plan = checker
                .ownership_primitives
                .iter()
                .find(|plan| plan.descriptor().expression() == call)?;
            if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
                || plan.places().len() != 1
                || !plan.places()[0].is_root()
                || plan.descriptor().operands().len() != 2
            {
                return None;
            }
            let root = plan.places()[0].root();
            let producer = plan.descriptor().operands()[1];
            if !replace_break::literal(checker, producer.expression())
                || !roots.iter().any(|(known, _)| *known == root)
                || planned.iter().any(|(_, known, _)| *known == root)
            {
                return None;
            }
            planned.push((call, root, producer));
        }
        let callable = roots.first()?.1.callable;
        if roots.iter().any(|(_, owner)| owner.callable != callable) {
            return None;
        }
        Some(Self {
            control,
            identity: std::sync::Arc::new(()),
            callable,
            roots: roots.clone(),
            initial_roots: roots.clone(),
            entry_definitions: entry_definitions.to_vec(),
            planned,
            definitions: Vec::new(),
            bindings,
            pending: None,
            bound: None,
            retired: 0,
            next: 0,
            valid: true,
        })
    }
    // This hook is called after the actual direct String expression completes.
    pub(in super::super) fn complete(&mut self, producer: UnitExpressionId) {
        #[cfg(test)]
        if self.next == 1 && tests::injected_failure(1) {
            self.valid = false;
            return;
        }
        if !self.valid {
            return;
        }
        if self.planned.get(self.next).map(|entry| entry.2) != Some(producer)
            || self.definitions.len() != self.next
            || self.pending.is_some()
            || self.bound.is_some()
            || self.retired != self.next
        {
            self.valid = false;
            return;
        }
        let id = UnitOwnerId {
            identity: self.identity.clone(),
            callable: self.callable,
            index: self.definitions.len(),
        };
        self.definitions.push(UnitOwnerDefinition {
            id,
            origin: UnitOwnerOrigin::String(producer),
        });
    }
    // Validate every input before atomically changing this private root/token ledger.
    pub(in super::super) fn commit(
        &mut self,
        call: UnitExpressionId,
        root: UnitSymbolId,
        producer: UnitExpressionId,
    ) -> Option<(UnitOwnerId, UnitOwnerId)> {
        #[cfg(test)]
        if self.next == 1 && tests::injected_failure(2) {
            self.valid = false;
            return None;
        }
        if !self.valid {
            return None;
        }
        let result = (|| {
            if self.planned.get(self.next) != Some(&(call, root, producer))
                || self.definitions.len() != self.next + 1
            {
                return None;
            }
            let definition = self.definitions.get(self.next)?;
            if definition.origin != UnitOwnerOrigin::String(producer)
                || !std::sync::Arc::ptr_eq(&definition.id.identity, &self.identity)
                || definition.id.callable != self.callable
                || definition.id.index != self.next
            {
                return None;
            }
            let index = self.roots.iter().position(|(known, _)| *known == root)?;
            let old = self.roots[index].1.clone();
            if !self
                .entry_definitions
                .iter()
                .any(|definition| definition.id == old)
            {
                return None;
            }
            let new = definition.id.clone();
            if old == new {
                return None;
            }
            Some((index, old, new))
        })();
        let Some((index, old, new)) = result else {
            self.valid = false;
            return None;
        };
        self.roots[index].1 = new.clone();
        self.pending = Some((call, old.clone()));
        self.next += 1;
        Some((old, new))
    }
    pub(in super::super) fn invalidate(&mut self) {
        self.valid = false;
    }
    pub(in super::super) fn bind(&mut self, call: UnitExpressionId, binding: UnitSymbolId) {
        #[cfg(test)]
        if self.next == 1 && tests::injected_failure(3) {
            return;
        }
        if !self.valid {
            return;
        }
        let Some(index) = self.next.checked_sub(1) else {
            self.valid = false;
            return;
        };
        if self.bindings.get(index).map(|entry| entry.0) != Some(binding)
            || self.pending.as_ref().map(|entry| entry.0) != Some(call)
            || self.bound.is_some()
        {
            self.valid = false;
            return;
        }
        let Some((_, old)) = self.pending.take() else {
            self.valid = false;
            return;
        };
        self.bound = Some((binding, old));
    }
    pub(in super::super) fn drop_result(
        &mut self,
        fact: crate::ownership_checking::UnitDropFact,
    ) -> Option<(UnitExpressionId, UnitSymbolId, UnitOwnerId)> {
        #[cfg(test)]
        if self.next == 1 && tests::injected_failure(4) {
            return None;
        }
        if !self.valid {
            return None;
        }
        let crate::ownership_checking::UnitDropTarget::Named(binding) = fact.target() else {
            return None;
        };
        if !self.bindings.iter().any(|entry| entry.0 == binding) {
            return None;
        }
        let Some(index) = self.next.checked_sub(1) else {
            self.valid = false;
            return None;
        };
        if self.bound.as_ref().map(|entry| entry.0) != Some(binding)
            || self
                .bindings
                .get(index)
                .map(|entry| crate::ownership_checking::UnitDropPoint::AfterStatement(entry.1))
                != Some(fact.point())
            || self.retired != index
        {
            self.valid = false;
            return None;
        }
        let Some((_, old)) = self.bound.take() else {
            self.valid = false;
            return None;
        };
        self.retired += 1;
        Some((self.planned[index].0, binding, old))
    }
}
