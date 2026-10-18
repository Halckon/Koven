//! 小型只读图查询：真实 block 引用、全 incoming 和 operand 对应列。
use super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use crate::ssa::model::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Graph<'a> {
    pub function: &'a Function,
    pub blocks: Vec<BlockId>,
    pub edges: Vec<(BlockId, &'a Edge)>,
    pub instructions: Vec<&'a Instruction>,
}

impl<'a> Graph<'a> {
    pub fn new(function: &'a Function) -> Result<Self, Failure> {
        Self::collect(function, false)
    }

    /// Local later-use fixtures contain real `error` calls after size failure.
    /// Keep the existing graph entry strict for all other identity queries.
    pub fn with_abort(function: &'a Function) -> Result<Self, Failure> {
        Self::collect(function, true)
    }

    fn collect(function: &'a Function, allow_abort: bool) -> Result<Self, Failure> {
        let mut graph = Self {
            function,
            blocks: vec![],
            edges: vec![],
            instructions: vec![],
        };
        let mut pending = vec![
            function
                .entry_block()
                .ok_or_else(|| failure("identity SSA entry", function.id))?,
        ];
        let mut seen_instructions = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if graph.blocks.contains(&id) {
                continue;
            }
            let block = graph.block(id)?;
            graph.blocks.push(id);
            for &instruction in &block.instructions {
                let i = function
                    .instruction(instruction)
                    .ok_or_else(|| failure("identity SSA referenced instruction", instruction))?;
                need(
                    i.block == id && seen_instructions.insert(instruction),
                    "identity SSA unique instruction membership",
                    i,
                )?;
                for (index, &result) in i.results.iter().enumerate() {
                    need(
                        function.entity(result).is_some_and(|data| {
                            data.definition == Definition::InstructionResult { instruction, index }
                        }),
                        "identity SSA actual result definition",
                        (i, result),
                    )?;
                }
                graph.instructions.push(i);
            }
            let term = graph.term(id)?;
            let edges = match &term.kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                TerminatorKind::Return { .. } => vec![],
                TerminatorKind::Abort if allow_abort => vec![],
                _ => return Err(failure("identity SSA unexpected fixture terminator", term)),
            };
            for edge in edges {
                let target = graph.block(edge.target)?;
                need(
                    edge.arguments.len() == target.parameters.len(),
                    "identity SSA every edge full arity",
                    edge,
                )?;
                for (index, (&argument, &parameter)) in
                    edge.arguments.iter().zip(&target.parameters).enumerate()
                {
                    let a = function
                        .entity(argument)
                        .ok_or_else(|| failure("identity SSA incoming argument", argument))?;
                    let p = function
                        .entity(parameter)
                        .ok_or_else(|| failure("identity SSA incoming parameter", parameter))?;
                    need(
                        a.ty == p.ty
                            && p.definition
                                == Definition::BlockParameter {
                                    block: target.id,
                                    index,
                                },
                        "identity SSA every incoming definition/type",
                        (edge, parameter),
                    )?;
                }
                graph.edges.push((id, edge));
                pending.push(edge.target);
            }
        }
        Ok(graph)
    }

    pub fn block(&self, id: BlockId) -> Result<&'a Block, Failure> {
        self.function
            .block(id)
            .ok_or_else(|| failure("identity SSA block", id))
    }
    pub fn term(&self, id: BlockId) -> Result<&'a Terminator, Failure> {
        self.block(id)?
            .terminator
            .as_ref()
            .ok_or_else(|| failure("identity SSA terminator", id))
    }
    pub fn branch(&self, id: BlockId) -> Result<&'a Edge, Failure> {
        match &self.term(id)?.kind {
            TerminatorKind::Branch(edge) => Ok(edge),
            term => Err(failure("identity SSA actual unconditional branch", term)),
        }
    }
    pub fn ops(&self, id: BlockId) -> Result<Vec<&'a Instruction>, Failure> {
        self.block(id)?
            .instructions
            .iter()
            .map(|&id| {
                self.function
                    .instruction(id)
                    .ok_or_else(|| failure("identity SSA block instruction", id))
            })
            .collect()
    }
    pub fn incoming(&self, target: BlockId) -> Vec<(BlockId, &'a Edge)> {
        self.edges
            .iter()
            .copied()
            .filter(|(_, edge)| edge.target == target)
            .collect()
    }
    pub fn incoming_from(&self, target: BlockId, expected: &[BlockId]) -> Result<(), Failure> {
        let actual = self
            .incoming(target)
            .iter()
            .map(|(from, _)| *from)
            .collect::<Vec<_>>();
        need(
            actual.len() == expected.len()
                && actual.iter().copied().collect::<BTreeSet<_>>()
                    == expected.iter().copied().collect::<BTreeSet<_>>(),
            "identity SSA complete incoming predecessor set",
            (target, actual, expected),
        )
    }
    pub fn transported(&self, edge: &Edge, input: EntityId) -> Result<EntityId, Failure> {
        let slot = one(
            edge.arguments
                .iter()
                .enumerate()
                .filter_map(|(i, &v)| (v == input).then_some(i)),
            "identity SSA unique actual transport column",
        )?;
        self.block(edge.target)?
            .parameters
            .get(slot)
            .copied()
            .ok_or_else(|| failure("identity SSA target parameter slot", (edge, slot)))
    }
    pub fn argument(&self, edge: &Edge, parameter: EntityId) -> Result<EntityId, Failure> {
        let data = self
            .function
            .entity(parameter)
            .ok_or_else(|| failure("identity SSA transport entity", parameter))?;
        let Definition::BlockParameter { block, index } = data.definition else {
            return Err(failure("identity SSA transport target is parameter", data));
        };
        need(
            block == edge.target,
            "identity SSA actual transport target",
            (edge, data),
        )?;
        edge.arguments
            .get(index)
            .copied()
            .ok_or_else(|| failure("identity SSA incoming slot", edge))
    }
    pub fn definition(&self, entity: EntityId) -> Result<&'a Instruction, Failure> {
        let data = self
            .function
            .entity(entity)
            .ok_or_else(|| failure("identity SSA entity definition", entity))?;
        let Definition::InstructionResult { instruction, index } = data.definition else {
            return Err(failure("identity SSA instruction result required", data));
        };
        let i = self
            .function
            .instruction(instruction)
            .ok_or_else(|| failure("identity SSA result producer", instruction))?;
        need(
            i.results.get(index) == Some(&entity)
                && self.instructions.iter().any(|v| v.id == instruction),
            "identity SSA real reachable producer",
            i,
        )?;
        Ok(i)
    }
    pub fn position(&self, instruction: &Instruction) -> Result<usize, Failure> {
        one(
            self.block(instruction.block)?
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(i, id)| (*id == instruction.id).then_some(i)),
            "identity SSA instruction position",
        )
    }
}

pub(super) fn result(i: &Instruction) -> Result<EntityId, Failure> {
    one(
        i.results.iter().copied(),
        "identity SSA unique operation result",
    )
}
pub(super) fn value(i: &Instruction) -> Result<ValueId, Failure> {
    match result(i)? {
        EntityId::Value(id) => Ok(id),
        e => Err(failure("identity SSA Value result", e)),
    }
}
pub(super) fn loan(i: &Instruction) -> Result<LoanId, Failure> {
    match result(i)? {
        EntityId::Loan(id) => Ok(id),
        e => Err(failure("identity SSA Loan result", e)),
    }
}
pub(super) fn function(
    module: &Module,
    span: lang_frontend::source::Span,
) -> Result<&Function, Failure> {
    one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(span)),
        "identity SSA original source function",
    )
}

#[derive(Default)]
pub(super) struct Uses(BTreeMap<EntityId, BTreeSet<InstructionId>>);

impl Uses {
    pub fn protect(&mut self, entity: EntityId) {
        self.0.entry(entity).or_default();
    }
    pub fn permit(&mut self, entity: EntityId, instruction: &Instruction) {
        self.0.entry(entity).or_default().insert(instruction.id);
    }
    pub fn check(
        &self,
        graph: &Graph<'_>,
        returned: Option<(BlockId, ValueId)>,
    ) -> Result<(), Failure> {
        for instruction in &graph.instructions {
            for entity in instruction.operation.entities() {
                if let Some(allowed) = self.0.get(&entity) {
                    need(
                        allowed.contains(&instruction.id),
                        "identity SSA complete protected operand uses",
                        (entity, instruction),
                    )?;
                }
            }
        }
        for &block in &graph.blocks {
            let term = graph.term(block)?;
            for entity in term.kind.entities() {
                if self.0.contains_key(&entity) {
                    let allowed = match (&term.kind, entity) {
                        (TerminatorKind::Return { values }, EntityId::Value(v)) => {
                            returned == Some((block, v)) && values.as_slice() == [v]
                        }
                        (TerminatorKind::Branch(_), _)
                        | (TerminatorKind::Conditional { .. }, _) => graph
                            .edges
                            .iter()
                            .any(|(from, edge)| *from == block && edge.arguments.contains(&entity)),
                        _ => false,
                    };
                    need(
                        allowed,
                        "identity SSA protected terminal operands only proven edge/Return",
                        (entity, term),
                    )?;
                }
            }
        }
        Ok(())
    }
}
