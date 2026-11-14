//! Replay a finite SSA graph certificate without Plans, builder slots, or emitter current maps.
//! The retained FE identity stays Partial; no catalog or arbitrary iteration proof is claimed.
use super::roles::Role;
use super::*;
use crate::ssa::model::{
    Edge, EntityId, EntityType, Function, Instruction, LoanKind, Operation, PlaceAccess,
    TerminatorKind,
};
impl Receipt {
    pub(crate) fn validate(&self, function: &Function) -> Result<(), LoweringError> {
        let span = self.validation_span;
        let p = &self.proof;
        let b = p.blocks;
        if self.caller != function.id()
            || function.never_returns
            || function.receiver.is_some()
            || function.borrow_return.is_some()
            || function.carrier_return.is_some()
            || !function.return_types.is_empty()
            || self.lineage.coverage()
                != lang_frontend::ownership_checking::UnitLoanActionCoverage::Partial
            || p.consumers.is_empty()
            || p.consumers.len() != self.lineage.uses().len()
        {
            return Err(invalid(span));
        }
        let originals = [b.entry, b.header, b.body, b.finished, b.exit];
        let original_edges = [
            (b.entry, b.header),
            (b.header, b.body),
            (b.header, b.finished),
            (b.body, b.header),
            (b.finished, b.exit),
        ];
        if originals.into_iter().collect::<BTreeSet<_>>().len() != 5
            || p.original_edges != original_edges
            || p.original_return != b.exit
            || !(p.folds.is_empty() || p.folds.as_slice() == [(b.finished, b.exit)])
        {
            return Err(invalid(span));
        }
        let raw = originals
            .into_iter()
            .map(|b| p.canonical(b))
            .collect::<BTreeSet<_>>();
        if self.blocks.keys().copied().collect::<BTreeSet<_>>() != raw {
            return Err(invalid(span));
        }
        let resolve = |raw| {
            self.blocks
                .get(&p.canonical(raw))
                .copied()
                .ok_or_else(|| invalid(span))
        };
        let entry = resolve(b.entry)?;
        let header = resolve(b.header)?;
        let body = resolve(b.body)?;
        let exit = resolve(b.exit)?;
        if function.entry_block() != Some(entry) {
            return Err(invalid(span));
        }
        let covered = self.blocks.values().copied().collect::<BTreeSet<_>>();
        let mut expected_tags = vec![
            Tag::Owner,
            Tag::Place,
            Tag::Source,
            Tag::Result,
            Tag::Condition,
        ];
        expected_tags.extend((0..p.consumers.len()).map(Tag::Consumer));
        expected_tags.extend([Tag::ResultEnd, Tag::SourceEnd, Tag::OwnerDrop]);
        let tags = self
            .instructions
            .iter()
            .map(|(tag, _)| *tag)
            .collect::<BTreeSet<_>>();
        let ids = self
            .instructions
            .iter()
            .map(|(_, id)| *id)
            .collect::<BTreeSet<_>>();
        if tags != expected_tags.iter().copied().collect()
            || self.instructions.len() != expected_tags.len()
            || ids.len() != expected_tags.len()
        {
            return Err(invalid(span));
        }
        super::whole_function::check(function, &covered, &ids, span)?;
        if self.entities.len() != covered.len()
            || self.entities.iter().any(|(id, entities)| {
                function
                    .block(*id)
                    .is_none_or(|b| b.parameters != *entities)
            })
        {
            return Err(invalid(span));
        }
        // The actual ordered statements must be exactly the roles originally emitted in each block.
        for block in &function.blocks {
            let local_tags = if block.id == entry {
                vec![Tag::Owner, Tag::Place, Tag::Source, Tag::Result]
            } else if block.id == header {
                vec![Tag::Condition]
            } else if block.id == body {
                (0..p.consumers.len()).map(Tag::Consumer).collect()
            } else if block.id == exit {
                vec![Tag::ResultEnd, Tag::SourceEnd, Tag::OwnerDrop]
            } else {
                Vec::new()
            };
            let expected = local_tags
                .iter()
                .map(|tag| {
                    self.instructions
                        .iter()
                        .find(|(t, _)| t == tag)
                        .map(|(_, id)| *id)
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid(span))?;
            if block.instructions != expected {
                return Err(invalid(span));
            }
        }
        // Establish seeds from actual defining instructions, not stored slot identities.
        let literal = self.instruction(function, Tag::Owner)?;
        let owner = single(literal, span)?;
        let Operation::StringLiteral { string, .. } = literal.operation else {
            return Err(invalid(span));
        };
        if !matches!(owner, EntityId::Value(_))
            || function.entity(owner).map(|e| e.ty) != Some(EntityType::Value(string))
            || p.owner_alias != Some((owner, owner))
        {
            return Err(invalid(span));
        }
        let place_definition = self.instruction(function, Tag::Place)?;
        let place = single(place_definition, span)?;
        if !matches!(place_definition.operation,Operation::RootPlace{owner:o} if EntityId::Value(o)==owner)
            || !matches!(place, EntityId::Place(_))
            || function.entity(place).map(|e| e.ty) != Some(EntityType::Place(string))
        {
            return Err(invalid(span));
        }
        let source_definition = self.instruction(function, Tag::Source)?;
        let source = single(source_definition, span)?;
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: string,
        };
        if !matches!(source_definition.operation,Operation::BorrowBegin{place:p,kind:LoanKind::Shared} if EntityId::Place(p)==place)
            || !matches!(source, EntityId::Loan(_))
            || function.entity(source).map(|e| e.ty) != Some(shared)
        {
            return Err(invalid(span));
        }
        let result_definition = self.instruction(function, Tag::Result)?;
        let result = single(result_definition, span)?;
        if !matches!(&result_definition.operation,Operation::BorrowCall{callee,source:s,arguments} if *callee==p.source_callee && EntityId::Loan(*s)==source && arguments.as_slice()==[source])
            || !matches!(result, EntityId::Loan(_))
            || source == result
            || function.entity(result).map(|e| e.ty) != Some(shared)
        {
            return Err(invalid(span));
        }
        let entry_data = function.block(entry).ok_or_else(|| invalid(span))?;
        let [flag] = entry_data.parameters.as_slice() else {
            return Err(invalid(span));
        };
        if p.flag_parameter != 0
            || !matches!(flag, EntityId::Loan(_))
            || !matches!(
                function.entity(*flag).map(|e| e.ty),
                Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                })
            )
        {
            return Err(invalid(span));
        }
        let roles = super::roles::prove(
            function,
            [entry, header, body, exit],
            &[
                (owner, Role::Owner),
                (source, Role::Source),
                (result, Role::Result),
                (*flag, Role::Flag),
            ],
            span,
        )?;
        // Operations consume the current block's unique resource parameter,
        // never an older entry definition that merely carries the same role.
        let role = |block, id, expected| {
            roles.get(&id) == Some(&expected)
                && function.block(block).is_some_and(|b| {
                    b.parameters
                        .iter()
                        .filter(|entity| roles.get(entity) == Some(&expected))
                        .copied()
                        .collect::<Vec<_>>()
                        == [id]
                })
        };
        let condition = self.instruction(function, Tag::Condition)?;
        let condition_value = single(condition, span)?;
        if !matches!(condition.operation,Operation::Read{source:PlaceAccess::Loan(loan)} if role(condition.block,EntityId::Loan(loan),Role::Flag))
            || !matches!(condition_value, EntityId::Value(_))
            || condition.origin.span() != p.condition_span
        {
            return Err(invalid(span));
        }
        if function
            .entity(condition_value)
            .map(|e| e.ty.semantic_type())
            != function.entity(*flag).map(|e| e.ty.semantic_type())
        {
            return Err(invalid(span));
        }
        for (i, callee) in p.consumers.iter().enumerate() {
            let call = self.instruction(function, Tag::Consumer(i))?;
            if !call.results.is_empty()
                || !matches!(&call.operation,Operation::DirectCall{callee:c,receiver:None,arguments} if c==callee && arguments.len()==1 && role(call.block,arguments[0],Role::Result))
                || call.origin.span() != self.lineage.uses()[i].loan().end_span()
            {
                return Err(invalid(span));
            }
        }
        for (tag, expected) in [
            (Tag::ResultEnd, Role::Result),
            (Tag::SourceEnd, Role::Source),
        ] {
            let end = self.instruction(function, tag)?;
            if !end.results.is_empty()
                || !matches!(end.operation,Operation::BorrowEnd{loan} if role(end.block,EntityId::Loan(loan),expected))
            {
                return Err(invalid(span));
            }
        }
        let drop = self.instruction(function, Tag::OwnerDrop)?;
        if !drop.results.is_empty()
            || !matches!(drop.operation,Operation::Drop{owner} if role(drop.block,EntityId::Value(owner),Role::Owner))
        {
            return Err(invalid(span));
        }
        // Original control edges and actual fold provenance determine the expected finite graph.
        let mut observed_edges = Vec::new();
        for block in &function.blocks {
            let actual = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
            let expected = p
                .original_edges
                .iter()
                .filter(|(from, to)| {
                    resolve(*from).ok() == Some(block.id) && p.canonical(*from) != p.canonical(*to)
                })
                .map(|(_, to)| resolve(*to))
                .collect::<Result<Vec<_>, _>>()?;
            let actual_edges: Vec<&Edge> = match actual {
                TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } if block.id == header && EntityId::Value(*condition) == condition_value => {
                    vec![when_true, when_false]
                }
                TerminatorKind::Branch(edge) if block.id != header => vec![edge],
                TerminatorKind::Return { values } if block.id == exit && values.is_empty() => {
                    Vec::new()
                }
                _ => return Err(invalid(span)),
            };
            if actual_edges.iter().map(|e| e.target).collect::<Vec<_>>() != expected {
                return Err(invalid(span));
            }
            let captured = self
                .edges
                .iter()
                .filter(|(from, _)| *from == block.id)
                .map(|(_, e)| e)
                .collect::<Vec<_>>();
            if captured != actual_edges {
                return Err(invalid(span));
            }
            observed_edges.extend(actual_edges);
        }
        if observed_edges.len() != self.edges.len() {
            return Err(invalid(span));
        }
        Ok(())
    }
    fn instruction<'a>(
        &self,
        function: &'a Function,
        tag: Tag,
    ) -> Result<&'a Instruction, LoweringError> {
        let id = self
            .instructions
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, id)| *id)
            .ok_or_else(|| invalid(self.validation_span))?;
        function
            .instruction(id)
            .ok_or_else(|| invalid(self.validation_span))
    }
}
fn single(instruction: &Instruction, span: Span) -> Result<EntityId, LoweringError> {
    let [result] = instruction.results.as_slice() else {
        return Err(invalid(span));
    };
    Ok(*result)
}
