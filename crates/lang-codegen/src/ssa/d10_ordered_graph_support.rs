//! Order-neutral graph access, strict transport and dependency groups. No source admission.
use crate::ssa::model::*;
use lang_frontend::type_checking::UnitExpressionId;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::ssa) enum Role {
    A,
    B,
    Outer,
    Inner,
    ChildLoan,
}
pub(in crate::ssa) type Env = BTreeMap<Role, EntityId>;
pub(in crate::ssa) struct Graph<'a> {
    pub(in crate::ssa) f: &'a Function,
    pub(in crate::ssa) module: &'a Module,
    pub(in crate::ssa) used: BTreeSet<BlockId>,
}
impl<'a> Graph<'a> {
    pub(in crate::ssa) fn unique(
        &self,
        origin: &Origin,
        predicate: impl Fn(&Operation) -> bool,
    ) -> Option<&'a Instruction> {
        let mut found = self
            .f
            .instructions
            .iter()
            .filter(|i| &i.origin == origin && predicate(&i.operation));
        let one = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(one)
    }
    pub(in crate::ssa) fn term(&self, block: BlockId) -> Option<&'a Terminator> {
        self.f.block(block)?.terminator.as_ref()
    }
    pub(in crate::ssa) fn branch(&self, block: BlockId) -> Option<&'a Edge> {
        match &self.term(block)?.kind {
            TerminatorKind::Branch(e) => Some(e),
            _ => None,
        }
    }
    pub(in crate::ssa) fn conditional(
        &self,
        block: BlockId,
        condition: ValueId,
    ) -> Option<(&'a Edge, &'a Edge)> {
        match &self.term(block)?.kind {
            TerminatorKind::Conditional {
                condition: c,
                when_true,
                when_false,
            } if *c == condition => Some((when_true, when_false)),
            _ => None,
        }
    }
    pub(in crate::ssa) fn sequence(
        &mut self,
        block: BlockId,
        expected: &[&Instruction],
        origin: &Origin,
        term_origin: &Origin,
    ) -> Option<()> {
        let b = self.f.block(block)?;
        if !self.used.insert(block)
            || &b.origin != origin
            || b.instructions.len() != expected.len()
            || b.instructions
                .iter()
                .zip(expected)
                .any(|(id, i)| *id != i.id || i.block != block)
            || &self.term(block)?.origin != term_origin
        {
            return None;
        }
        Some(())
    }
    // Edge arguments and destination parameters determine the role mapping, never slot numbers.
    pub(in crate::ssa) fn transport(&self, edge: &Edge, env: &Env, roles: &[Role]) -> Option<Env> {
        let target = self.f.block(edge.target)?;
        if edge.arguments.len() != roles.len() || target.parameters.len() != roles.len() {
            return None;
        }
        let mut result = Env::new();
        let mut positions = BTreeSet::new();
        for role in roles {
            let value = *env.get(role)?;
            let mut found = edge
                .arguments
                .iter()
                .enumerate()
                .filter(|(_, arg)| **arg == value);
            let (index, _) = found.next()?;
            if found.next().is_some() || !positions.insert(index) {
                return None;
            }
            let parameter = *target.parameters.get(index)?;
            if self.f.entity(value)?.ty != self.f.entity(parameter)?.ty {
                return None;
            }
            result.insert(*role, parameter);
        }
        Some(result)
    }
    pub(in crate::ssa) fn integer(&self, value: ValueId) -> Option<()> {
        let EntityType::Value(ty) = self.f.entity(EntityId::Value(value))?.ty else {
            return None;
        };
        matches!(
            self.module.types.get(ty.index()),
            Some(SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        )
        .then_some(())
    }
    pub(in crate::ssa) fn owner(&self, value: ValueId) -> Option<()> {
        let EntityType::Value(ty) = self.f.entity(EntityId::Value(value))?.ty else {
            return None;
        };
        matches!(
            self.module.types.get(ty.index()),
            Some(SsaTypeKind::StringOwner)
        )
        .then_some(())
    }
    pub(in crate::ssa) fn shared_int(&self, id: EntityId) -> Option<LoanId> {
        let EntityId::Loan(loan) = id else {
            return None;
        };
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = self.f.entity(id)?.ty
        else {
            return None;
        };
        matches!(
            self.module.types.get(target.index()),
            Some(SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        )
        .then_some(loan)
    }
    pub(in crate::ssa) fn no_abort(&self, block: BlockId) -> Option<()> {
        self.term(block)?.abort.is_none().then_some(())
    }
}
pub(in crate::ssa) fn value(i: &Instruction) -> Option<ValueId> {
    match i.results.as_slice() {
        [EntityId::Value(v)] => Some(*v),
        _ => None,
    }
}
pub(in crate::ssa) fn role_value(env: &Env, role: Role) -> Option<ValueId> {
    match env.get(&role)? {
        EntityId::Value(v) => Some(*v),
        _ => None,
    }
}

#[path = "d10_after_child_graph_groups.rs"]
mod groups;
pub(in crate::ssa) use groups::*;
