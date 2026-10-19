//! Nested-only role witness, beside the unchanged generic read-only graph helper.
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses},
};
use crate::ssa::model::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Role {
    Caller,
    OuterSource,
    OuterCursor,
    OuterKey,
    OuterValue,
    InnerSource,
    InnerCursor,
    InnerKey,
    InnerValue,
    OuterGuard,
    InnerGuard,
    CallerOwnerA,
    CallerOwnerB,
    CallerResultA,
    CallerResultB,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Roles(pub BTreeMap<Role, EntityId>);
impl Roles {
    pub fn get(&self, role: Role) -> Result<EntityId, Failure> {
        self.0
            .get(&role)
            .copied()
            .ok_or_else(|| failure("nested exits SSA missing current role", (role, self)))
    }
    pub fn loan(&self, role: Role) -> Result<LoanId, Failure> {
        as_loan(self.get(role)?)
    }
    pub fn value(&self, role: Role) -> Result<ValueId, Failure> {
        as_value(self.get(role)?)
    }
    pub fn insert(&mut self, role: Role, entity: EntityId) {
        self.0.insert(role, entity);
    }
    pub fn retain(&self, roles: &[Role]) -> Result<Self, Failure> {
        Ok(Self(
            roles
                .iter()
                .map(|&r| Ok((r, self.get(r)?)))
                .collect::<Result<_, Failure>>()?,
        ))
    }
}
pub(super) fn as_loan(entity: EntityId) -> Result<LoanId, Failure> {
    match entity {
        EntityId::Loan(v) => Ok(v),
        _ => Err(failure("nested exits SSA role Loan", entity)),
    }
}
pub(super) fn as_value(entity: EntityId) -> Result<ValueId, Failure> {
    match entity {
        EntityId::Value(v) => Ok(v),
        _ => Err(failure("nested exits SSA role Value", entity)),
    }
}
pub(super) fn ty(graph: &Graph<'_>, entity: EntityId) -> Result<EntityType, Failure> {
    graph
        .function
        .entity(entity)
        .map(|e| e.ty)
        .ok_or_else(|| failure("nested exits SSA entity type", entity))
}
pub(super) fn full_graph(graph: &Graph<'_>) -> Result<(), Failure> {
    let blocks = graph.blocks.iter().copied().collect::<BTreeSet<_>>();
    let instructions = graph
        .instructions
        .iter()
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    let entities = graph
        .function
        .blocks
        .iter()
        .flat_map(|b| b.parameters.iter().copied())
        .chain(
            graph
                .instructions
                .iter()
                .flat_map(|i| i.results.iter().copied()),
        )
        .collect::<Vec<_>>();
    let unique_entities = entities.iter().copied().collect::<BTreeSet<_>>();
    need(
        entities.len() == unique_entities.len()
            && entities.len()
                == graph.function.values.len()
                    + graph.function.places.len()
                    + graph.function.loans.len()
            && graph.instructions.iter().all(|i| {
                i.results.iter().all(|&e| {
                    graph
                        .function
                        .entity(e)
                        .is_some_and(|d| d.origin == i.origin)
                })
            }),
        "nested exits SSA complete entity arena and exact result origins",
        graph.function,
    )?;
    need(
        blocks.len() == graph.function.blocks.len()
            && graph.function.blocks.iter().all(|b| blocks.contains(&b.id))
            && instructions.len() == graph.function.instructions.len()
            && graph
                .function
                .instructions
                .iter()
                .all(|i| instructions.contains(&i.id)),
        "nested exits SSA complete arenas equal referenced reachable graph",
        graph.function,
    )
}

#[derive(Default)]
pub(super) struct Witness {
    pub uses: Uses,
    pub aliases: BTreeMap<Role, BTreeSet<EntityId>>,
    edges: BTreeMap<(BlockId, BlockId), Vec<EntityId>>,
    pub conditions: BTreeMap<BlockId, ValueId>,
    pub providers: BTreeSet<InstructionId>,
    pub returns: BTreeMap<BlockId, Vec<ValueId>>,
    pub returned: Option<(BlockId, ValueId)>,
}
impl Witness {
    pub fn protect(&mut self, roles: &Roles) -> Result<(), Failure> {
        for (&role, &entity) in &roles.0 {
            need(
                self.aliases
                    .iter()
                    .all(|(&other, ids)| other == role || !ids.contains(&entity)),
                "nested exits SSA distinct caller/lease/cursor/component role lineage",
                (role, entity, &self.aliases),
            )?;
            self.aliases.entry(role).or_default().insert(entity);
            self.uses.protect(entity);
        }
        Ok(())
    }
    pub fn edge(
        &mut self,
        graph: &Graph<'_>,
        from: BlockId,
        edge: &Edge,
        roles: &Roles,
    ) -> Result<Roles, Failure> {
        self.protect(roles)?;
        let expected = roles.0.values().copied().collect::<BTreeSet<_>>();
        need(
            expected.len() == roles.0.len()
                && edge.arguments.len() == expected.len()
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>() == expected,
            "nested exits SSA full exact current-role edge operands",
            (from, edge, roles),
        )?;
        need(
            self.edges
                .insert((from, edge.target), edge.arguments.clone())
                .is_none(),
            "nested exits SSA each actual edge proved once",
            (from, edge),
        )?;
        let target = graph.block(edge.target)?;
        need(
            target.parameters.iter().all(|&parameter| {
                graph
                    .function
                    .entity(parameter)
                    .is_some_and(|data| data.origin == target.origin)
            }),
            "nested exits SSA every role parameter retains actual target block origin",
            target,
        )?;
        let next = Roles(
            roles
                .0
                .iter()
                .map(|(&r, &e)| Ok((r, graph.transported(edge, e)?)))
                .collect::<Result<_, Failure>>()?,
        );
        self.protect(&next)?;
        Ok(next)
    }
    pub fn finish(&self, graph: &Graph<'_>) -> Result<(), Failure> {
        full_graph(graph)?;
        need(
            self.edges.len() == graph.edges.len(),
            "nested exits SSA all actual edges covered",
            (&self.edges, &graph.edges),
        )?;
        for &(from, edge) in &graph.edges {
            need(
                self.edges.get(&(from, edge.target)) == Some(&edge.arguments),
                "nested exits SSA every edge exact proved operands",
                (from, edge),
            )?;
        }
        // The shared Uses helper accepts any occurrence on some edge. This layer checks every
        // terminator and every edge separately, and excludes protected roles as conditions.
        for &block in &graph.blocks {
            let term = graph.term(block)?;
            match &term.kind {
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } => {
                    need(
                        self.conditions.get(&block) == Some(condition)
                            && !self
                                .aliases
                                .values()
                                .any(|s| s.contains(&EntityId::Value(*condition)))
                            && !when_true.arguments.contains(&EntityId::Value(*condition))
                            && !when_false.arguments.contains(&EntityId::Value(*condition)),
                        "nested exits SSA actual condition distinct from transport roles",
                        term,
                    )?;
                }
                TerminatorKind::Branch(_) => {}
                TerminatorKind::Return { values } => need(
                    self.returns
                        .get(&block)
                        .map_or(values.is_empty(), |expected| expected == values),
                    "nested exits SSA Unit return with no escaped role",
                    term,
                )?,
                _ => return Err(failure("nested exits SSA unexpected terminal", term)),
            }
        }
        self.uses.check(graph, self.returned)
    }
}
