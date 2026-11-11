//! Private first-body token transaction; no public loop/header or retirement capability.
use super::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in super::super) fn skip_body_entry() -> bool {
    tests::injected_failure(5)
}

/// A predecessor alternative, not a runtime iteration number or activation identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super::super::super) enum StaticRootPredecessor {
    ZeroBody,
    FirstBody,
}

/// Static transfer signature; root denotes an input slot, not a fresh runtime owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super::super::super) struct RootTransferSlot {
    pub(in super::super::super::super) root: UnitSymbolId,
    pub(in super::super::super::super) producer: UnitExpressionId,
    pub(in super::super::super::super) retired_through: crate::ownership_checking::UnitDropFact,
}

/// Sealed whole-vector alternatives. Selecting roots independently would invent paths.
#[derive(Clone, Debug)]
pub(in super::super::super::super) struct RootBoundaryEquation {
    control: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    condition_binding: UnitSymbolId,
    entry: RootIncoming,
    normal: RootIncoming,
    transfers: Vec<RootTransferSlot>,
}
impl RootBoundaryEquation {
    pub(in super::super::super::super) fn control(&self) -> UnitStatementId {
        self.control
    }

    /// These are candidate header inputs only; no reachability or induction is proved.
    pub(in super::super::super::super) fn incoming(
        &self,
        control: UnitStatementId,
        predecessor: StaticRootPredecessor,
    ) -> Option<&RootIncoming> {
        if control != self.control() {
            return None;
        }
        Some(match predecessor {
            StaticRootPredecessor::ZeroBody => &self.entry,
            StaticRootPredecessor::FirstBody => &self.normal,
        })
    }

    /// Match a whole known static snapshot, never infer a future activation from shape.
    pub(in super::super::super::super) fn predecessor_for(
        &self,
        control: UnitStatementId,
        incoming: &RootIncoming,
    ) -> Option<StaticRootPredecessor> {
        [
            StaticRootPredecessor::ZeroBody,
            StaticRootPredecessor::FirstBody,
        ]
        .into_iter()
        .find(|edge| self.incoming(control, *edge) == Some(incoming))
    }

    pub(in super::super::super::super) fn transfers(
        &self,
        control: UnitStatementId,
    ) -> Option<&[RootTransferSlot]> {
        (control == self.control()).then_some(self.transfers.as_slice())
    }

    /// Real AST/reference identities; no condition outcome or activation count.
    pub(in super::super::super::super) fn sites(
        &self,
    ) -> (UnitExpressionId, UnitStatementId, UnitSymbolId) {
        (self.condition, self.body, self.condition_binding)
    }

    fn same_roots(&self) -> bool {
        let Some(entry) = self.incoming(self.control, StaticRootPredecessor::ZeroBody) else {
            return false;
        };
        let Some(normal) = self.incoming(self.control, StaticRootPredecessor::FirstBody) else {
            return false;
        };
        let (condition, body, binding) = self.sites();
        condition.source_unit() == self.control.source_unit()
            && body.source_unit() == self.control.source_unit()
            && binding.source_unit() == self.control.source_unit()
            && entry.iter().all(|(root, _)| *root != binding)
            && self.transfers(self.control).is_some_and(|slots| {
                slots.len() == entry.len()
                    && slots
                        .iter()
                        .zip(entry)
                        .all(|(slot, (root, _))| slot.root == *root)
            })
            && entry.len() == normal.len()
            && entry
                .iter()
                .zip(normal)
                .all(|((root, old), (next_root, new))| {
                    root == next_root
                        && root.source_unit() == self.control.source_unit()
                        && old.callable == new.callable
                        && old != new
                        && !std::sync::Arc::ptr_eq(&old.identity, &new.identity)
                })
    }
}

pub(in super::super) struct RootTokenTransaction {
    control: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    condition_binding: UnitSymbolId,
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
    consumed: Vec<(UnitOwnerId, crate::ownership_checking::UnitDropFact)>,
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
            && self.consumed.len() == self.retired
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
        // Join by root identity, not by declaration order. Preserve actual drop facts.
        let mut transfers = Vec::new();
        for (root, old) in entry {
            let index = self
                .planned
                .iter()
                .position(|(_, known, _)| known == root)?;
            let (_, _, producer) = self.planned[index];
            let (binding, statement) = self.bindings.get(index)?;
            let (consumed_old, fact) = self.consumed.get(index)?;
            let definition = self.definitions.get(index)?;
            let current = self.roots.iter().find(|(known, _)| known == root)?;
            if consumed_old != old
                || fact.target() != crate::ownership_checking::UnitDropTarget::Named(*binding)
                || fact.point()
                    != crate::ownership_checking::UnitDropPoint::AfterStatement(*statement)
                || definition.origin != UnitOwnerOrigin::String(producer)
                || definition.id != current.1
            {
                return None;
            }
            transfers.push(RootTransferSlot {
                root: *root,
                producer,
                retired_through: *fact,
            });
        }
        let equation = RootBoundaryEquation {
            control: self.control,
            condition: self.condition,
            body: self.body,
            condition_binding: self.condition_binding,
            entry: entry.clone(),
            normal: self.roots.clone(),
            transfers,
        };
        (equation.same_roots()
            && equation.predecessor_for(self.control, &equation.entry)
                == Some(StaticRootPredecessor::ZeroBody)
            && equation.predecessor_for(self.control, &equation.normal)
                == Some(StaticRootPredecessor::FirstBody))
        .then_some(equation)
    }

    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        control: UnitStatementId,
        roots: &RootIncoming,
        entry_definitions: &[UnitOwnerDefinition],
        entered_body: Option<(UnitExpressionId, UnitStatementId)>,
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
        if entered_body
            != Some((
                checker.unit_expression(*condition),
                UnitStatementId::new(control.source_unit(), *body),
            ))
        {
            return None;
        }
        if !matches!(
            ast.expressions().get(*condition).ok()?.payload(),
            Expression::Name
        ) || !exact_type(checker, *condition, BuiltinType::Boolean)
        {
            return None;
        }
        let condition_binding =
            checker.reference_symbol(ast.expressions().get(*condition).ok()?.span())?;
        if condition_binding.source_unit() != control.source_unit()
            || roots.iter().any(|(root, _)| *root == condition_binding)
            || checker
                .typed
                .symbol_type(condition_binding)
                .and_then(|ty| checker.typed.types().get(ty))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
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
            condition: checker.unit_expression(*condition),
            body: UnitStatementId::new(control.source_unit(), *body),
            condition_binding,
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
            consumed: Vec::new(),
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
        self.consumed.push((old.clone(), fact));
        self.retired += 1;
        Some((self.planned[index].0, binding, old))
    }
}
