//! Default-off, fail-closed AfterChild graph validation. No test-layout indices or graph rewriting.
use super::model::*;
use lang_frontend::{
    ownership_checking::{
        OwnedCompilationUnitView,
        d10_catalog_experiment::{BoundAfterChildNestedLoop, ReplacementOrder},
    },
    type_checking::{UnitExpressionId, UnitStatementId},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphError {
    Rejected,
}

/// Consumes neither the bound nor graph; callers must keep a successfully checked graph immutable.
pub(crate) fn check(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &BoundAfterChildNestedLoop,
    program: &Program,
    entry: FunctionId,
) -> Result<(), GraphError> {
    inspect(unit, bound, program, entry).ok_or(GraphError::Rejected)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    A,
    B,
    Outer,
    Inner,
    ChildLoan,
}
type Env = BTreeMap<Role, EntityId>;
struct Graph<'a> {
    f: &'a Function,
    module: &'a Module,
    used: BTreeSet<BlockId>,
}
impl<'a> Graph<'a> {
    fn unique(
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
    fn term(&self, block: BlockId) -> Option<&'a Terminator> {
        self.f.block(block)?.terminator.as_ref()
    }
    fn branch(&self, block: BlockId) -> Option<&'a Edge> {
        match &self.term(block)?.kind {
            TerminatorKind::Branch(e) => Some(e),
            _ => None,
        }
    }
    fn conditional(&self, block: BlockId, condition: ValueId) -> Option<(&'a Edge, &'a Edge)> {
        match &self.term(block)?.kind {
            TerminatorKind::Conditional {
                condition: c,
                when_true,
                when_false,
            } if *c == condition => Some((when_true, when_false)),
            _ => None,
        }
    }
    fn sequence(
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
    fn transport(&self, edge: &Edge, env: &Env, roles: &[Role]) -> Option<Env> {
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
    fn integer(&self, value: ValueId) -> Option<()> {
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
    fn owner(&self, value: ValueId) -> Option<()> {
        let EntityType::Value(ty) = self.f.entity(EntityId::Value(value))?.ty else {
            return None;
        };
        matches!(
            self.module.types.get(ty.index()),
            Some(SsaTypeKind::StringOwner)
        )
        .then_some(())
    }
    fn shared_int(&self, id: EntityId) -> Option<LoanId> {
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
    fn no_abort(&self, block: BlockId) -> Option<()> {
        self.term(block)?.abort.is_none().then_some(())
    }
}
fn value(i: &Instruction) -> Option<ValueId> {
    match i.results.as_slice() {
        [EntityId::Value(v)] => Some(*v),
        _ => None,
    }
}
fn role_value(env: &Env, role: Role) -> Option<ValueId> {
    match env.get(&role)? {
        EntityId::Value(v) => Some(*v),
        _ => None,
    }
}

fn inspect(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &BoundAfterChildNestedLoop,
    program: &Program,
    entry: FunctionId,
) -> Option<()> {
    if !bound.matches(unit) || bound.order() != ReplacementOrder::ChildThenParent {
        return None;
    }
    super::verify::verify_program(program).ok()?;
    let [module] = program.modules.as_slice() else {
        return None;
    };
    let [f] = module.functions.as_slice() else {
        return None;
    };
    if f.id() != entry
        || module.id != entry.module()
        || f.receiver.is_some()
        || f.never_returns
        || !f.return_types.is_empty()
        || f.borrow_return.is_some()
        || f.carrier_return.is_some()
    {
        return None;
    }
    let [input] = unit.inputs() else {
        return None;
    };
    let [names] = unit.names().names().source_units() else {
        return None;
    };
    let source = names.source_unit();
    let ast = input.parsed().ast();
    let expr_origin = |id: UnitExpressionId| {
        (id.source_unit() == source)
            .then(|| ast.expressions().get(id.expression()).ok())
            .flatten()
            .map(|n| Origin::Source(n.span()))
    };
    let control_origin = |id: UnitStatementId| {
        (id.source_unit() == source)
            .then(|| ast.statements().get(id.statement()).ok())
            .flatten()
            .map(|n| Origin::Source(n.span()))
    };
    let (callable, parent, child) = bound.controls();
    if callable.source_unit() != source {
        return None;
    }
    let function_origin = Origin::Source(ast.items().get(callable.item()).ok()?.span());
    if f.origin != function_origin {
        return None;
    }
    let mut graph = Graph {
        f,
        module,
        used: BTreeSet::new(),
    };
    let [(pc, ps), (cc, cs)] = bound.counter_expressions();
    let pc_o = expr_origin(pc)?;
    let cc_o = expr_origin(cc)?;
    let ps_o = expr_origin(ps)?;
    let cs_o = expr_origin(cs)?;
    let parent_o = control_origin(parent)?;
    let child_o = control_origin(child)?;
    let cmp = |op: &Operation| {
        matches!(
            op,
            Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                ..
            }
        )
    };
    let pcmp = graph.unique(&pc_o, cmp)?;
    let ccmp = graph.unique(&cc_o, cmp)?;
    let checked = |op: &Operation| {
        matches!(
            op,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                ..
            }
        )
    };
    let pstep = graph.unique(&ps_o, checked)?;
    let cstep = graph.unique(&cs_o, checked)?;
    let calls = bound.replacements();
    let creplace = graph.unique(&expr_origin(calls[0])?, |o| {
        matches!(o, Operation::RootReplace { .. })
    })?;
    let preplace = graph.unique(&expr_origin(calls[1])?, |o| {
        matches!(o, Operation::RootReplace { .. })
    })?;
    if creplace.block != cstep.block || preplace.block != pstep.block {
        return None;
    }
    let literals = bound.producer_sites();
    let literal = |id| -> Option<&Instruction> {
        let ins = graph.unique(&expr_origin(id)?, |o| {
            matches!(o, Operation::StringLiteral { .. })
        })?;
        let Operation::StringLiteral { string, bytes } = &ins.operation else {
            return None;
        };
        if !matches!(
            module.types.get(string.index()),
            Some(SsaTypeKind::StringOwner)
        ) {
            return None;
        }
        let decoded = super::lowering_support::string_literal::decode_plain(
            input.parsed(),
            unit.sources().source_text(input.source_id()).ok()?,
            id.expression(),
        )
        .ok()??;
        if *bytes != decoded {
            return None;
        }
        graph.owner(value(ins)?)?;
        Some(ins)
    };
    let initial = literal(literals[0])?;
    let child_literal = literal(literals[1])?;
    let parent_literal = literal(literals[2])?;
    let retained = literal(bound.retained().source().1)?;
    let [pi, ci] = bound.counter_initializers();
    let pread = graph.unique(&expr_origin(pi)?, |o| matches!(o, Operation::Read { .. }))?;
    let cread = graph.unique(&expr_origin(ci)?, |o| matches!(o, Operation::Read { .. }))?;
    let entry_block = f.entry_block()?;
    let [parent_arg, child_arg] = f.block(entry_block)?.parameters.as_slice() else {
        return None;
    };
    let parent_loan = graph.shared_int(*parent_arg)?;
    graph.shared_int(*child_arg)?;
    if parent_arg == child_arg
        || pread.operation
            != (Operation::Read {
                source: PlaceAccess::Loan(parent_loan),
            })
    {
        return None;
    }
    graph.integer(value(pread)?)?;
    graph.integer(value(cread)?)?;
    let entry_env = Env::from([
        (Role::A, EntityId::Value(value(initial)?)),
        (Role::B, EntityId::Value(value(retained)?)),
        (Role::Outer, EntityId::Value(value(pread)?)),
        (Role::ChildLoan, *child_arg),
    ]);
    let outer_roles = [Role::A, Role::B, Role::Outer, Role::ChildLoan];
    let inner_roles = [Role::A, Role::B, Role::Outer, Role::Inner, Role::ChildLoan];
    let into_parent = graph.branch(entry_block)?;
    if into_parent.target != pcmp.block {
        return None;
    }
    let ph = graph.transport(into_parent, &entry_env, &outer_roles)?;
    let (parent_true, parent_false) = graph.conditional(pcmp.block, value(pcmp)?)?;
    if parent_true.target != cread.block {
        return None;
    }
    let mut init_env = graph.transport(parent_true, &ph, &outer_roles)?;
    let child_loan = graph.shared_int(*init_env.get(&Role::ChildLoan)?)?;
    if cread.operation
        != (Operation::Read {
            source: PlaceAccess::Loan(child_loan),
        })
    {
        return None;
    }
    init_env.insert(Role::Inner, EntityId::Value(value(cread)?));
    let into_child = graph.branch(cread.block)?;
    if into_child.target != ccmp.block {
        return None;
    }
    let ch = graph.transport(into_child, &init_env, &inner_roles)?;
    let (child_true, child_false) = graph.conditional(ccmp.block, value(ccmp)?)?;
    if child_true.target != creplace.block || child_false.target != preplace.block {
        return None;
    }
    let cb = graph.transport(child_true, &ch, &inner_roles)?;
    let after = graph.transport(child_false, &ch, &outer_roles)?;
    let final_env = graph.transport(parent_false, &ph, &[Role::A, Role::B])?;
    // All semantic instruction groups are discovered by dependencies and source anchors.
    let pzero = condition(
        &graph,
        pcmp,
        &ph,
        Role::Outer,
        constant_origin(ast, pc, true, source)?,
    )?;
    let czero = condition(
        &graph,
        ccmp,
        &ch,
        Role::Inner,
        constant_origin(ast, cc, true, source)?,
    )?;
    let cgroup = replacement(
        &graph,
        creplace,
        child_literal,
        role_value(&cb, Role::A)?,
        &bound.loans()[0],
        bound.cleanup()[0],
    )?;
    let pgroup = replacement(
        &graph,
        preplace,
        parent_literal,
        role_value(&after, Role::A)?,
        &bound.loans()[1],
        bound.cleanup()[1],
    )?;
    let cone = step(
        &graph,
        cstep,
        &cb,
        Role::Inner,
        constant_origin(ast, cs, false, source)?,
    )?;
    let pone = step(
        &graph,
        pstep,
        &after,
        Role::Outer,
        constant_origin(ast, ps, false, source)?,
    )?;
    let mut child_updated = cb.clone();
    child_updated.insert(Role::A, creplace.results.first().copied()?);
    child_updated.insert(Role::Inner, cstep.results.first().copied()?);
    let mut parent_updated = after.clone();
    parent_updated.insert(Role::A, preplace.results.first().copied()?);
    parent_updated.insert(Role::Outer, pstep.results.first().copied()?);
    let (ca, cr) = step_edges(
        &graph,
        cstep,
        cone,
        &child_updated,
        &inner_roles,
        ccmp.block,
        &ch,
    )?;
    let (pa, pr) = step_edges(
        &graph,
        pstep,
        pone,
        &parent_updated,
        &outer_roles,
        pcmp.block,
        &ph,
    )?;
    let exit = parent_false.target;
    let agroup = suffix(
        &graph,
        bound.final_root_use(),
        role_value(&final_env, Role::A)?,
        exit,
        expr_origin(bound.final_root_use().call())?,
    )?;
    let bgroup = suffix(
        &graph,
        bound.retained(),
        role_value(&final_env, Role::B)?,
        exit,
        expr_origin(bound.retained().call())?,
    )?;
    if !matches!(&graph.term(exit)?.kind,TerminatorKind::Return{values} if values.is_empty()) {
        return None;
    }
    graph.sequence(
        entry_block,
        &[initial, retained, pread],
        &function_origin,
        &parent_o,
    )?;
    graph.sequence(pcmp.block, &[pzero, pcmp], &function_origin, &pc_o)?;
    graph.sequence(cread.block, &[cread], &function_origin, &child_o)?;
    graph.sequence(ccmp.block, &[czero, ccmp], &function_origin, &cc_o)?;
    let mut cseq = cgroup;
    cseq.extend([cone, cstep]);
    let mut pseq = pgroup;
    pseq.extend([pone, pstep]);
    graph.sequence(creplace.block, &cseq, &function_origin, &cs_o)?;
    graph.sequence(preplace.block, &pseq, &function_origin, &ps_o)?;
    let mut out = agroup;
    out.extend(bgroup);
    graph.sequence(exit, &out, &function_origin, &function_origin)?;
    for (block, origin) in [(ca, &cs_o), (cr, &child_o), (pa, &ps_o), (pr, &parent_o)] {
        graph.sequence(block, &[], &function_origin, origin)?;
    }
    // Distinct role blocks exhaust the actual graph, including disconnected/extra blocks.
    if graph.used.len() != f.blocks.len() || f.blocks.iter().any(|b| !graph.used.contains(&b.id)) {
        return None;
    }
    for block in &f.blocks {
        if block.id != ca && block.id != pa {
            graph.no_abort(block.id)?;
        }
    }
    let flattened = f
        .blocks
        .iter()
        .flat_map(|b| b.instructions.iter().copied())
        .collect::<Vec<_>>();
    if flattened.len() != f.instructions.len()
        || f.instructions
            .iter()
            .any(|i| flattened.iter().filter(|id| **id == i.id).count() != 1)
    {
        return None;
    }
    Some(())
}

#[path = "d10_after_child_graph_groups.rs"]
mod groups;
use groups::*;
