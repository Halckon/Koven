//! Private first-body token transaction; no public loop/header or retirement capability.
use super::*;

pub(in super::super) struct RootTokenTransaction {
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    roots: RootIncoming,
    entry_definitions: Vec<UnitOwnerDefinition>,
    planned: Vec<(UnitExpressionId, UnitSymbolId, UnitExpressionId)>,
    definitions: Vec<UnitOwnerDefinition>,
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
        for statement in elements {
            let (_, call) = replace_break::variable(checker, *statement, VariableKind::Val)?;
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
            identity: std::sync::Arc::new(()),
            callable,
            roots: roots.clone(),
            entry_definitions: entry_definitions.to_vec(),
            planned,
            definitions: Vec::new(),
            next: 0,
            valid: true,
        })
    }
    // This hook is called after the actual direct String expression completes.
    pub(in super::super) fn complete(&mut self, producer: UnitExpressionId) {
        if !self.valid {
            return;
        }
        if self.planned.get(self.next).map(|entry| entry.2) != Some(producer)
            || self.definitions.len() != self.next
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
        self.next += 1;
        Some((old, new))
    }
}
