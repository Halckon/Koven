//! Independent identity proof for the actual nested-normal pair and all its incoming edges.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, function},
    query_inputs::one,
};
use super::{
    ast::Input,
    query_body::{Context, INNER, OUTER, provider},
    query_graph::{Role, Roles, Witness, full_graph, ty},
    query_inputs::Inputs,
    query_tail,
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

pub(super) fn check(
    program: &Program,
    entry_id: FunctionId,
    f: &Input,
    inputs: &Inputs,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "nested SSA sole actual module")?;
    let entry = function(module, inputs.entry)?;
    let scan = function(module, inputs.scan)?;
    let read = function(module, inputs.read)?;
    need(
        entry.id == entry_id && module.functions.len() == 4,
        "nested SSA returned entry and exact real source functions",
        (entry_id, &module.functions),
    )?;
    let g = Graph::new(scan)?;
    full_graph(&g)?;
    let pre = scan
        .entry_block()
        .ok_or_else(|| failure("nested SSA actual scan entry", scan.id))?;
    let caller = *one(
        g.block(pre)?.parameters.iter(),
        "nested SSA sole actual Borrow caller parameter",
    )?;
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    } = ty(&g, caller)?
    else {
        return Err(failure("nested SSA actual Shared map entry", caller));
    };
    let (kind, key, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("nested SSA real map type", map_type))?;
    need(
        kind == if f.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            && scan.return_types.is_empty()
            && scan.receiver.is_none()
            && scan.borrow_return.is_none()
            && scan.carrier_return.is_none()
            && contains(inputs.scan, inputs.parameter),
        "nested SSA actual FE Borrow signature and exact intrinsic storage",
        scan,
    )?;
    query_callers::check(module, map_type, f, inputs)?;
    let c = Context {
        graph: &g,
        module,
        read,
    };
    let mut witness = Witness::default();
    let mut initial = Roles::default();
    initial.insert(Role::Caller, caller);
    c.birth(pre, &inputs.sites[0], OUTER, &mut initial, &mut witness)?;
    need(
        g.ops(pre)?.len() == 2,
        "nested SSA actual outer acquire entry",
        g.ops(pre)?,
    )?;
    let outer_entry = g.branch(pre)?;
    let oh = outer_entry.target;
    let outer_header = witness.edge(&g, pre, outer_entry, &initial)?;
    let (ob, mut outer_body, oe, outer_exit) =
        c.header(oh, &inputs.sites[0], OUTER, &outer_header, &mut witness)?;
    c.project(ob, &inputs.sites[0], OUTER, &mut outer_body, &mut witness)?;
    let before = c.read(ob, &inputs.reads[0], OUTER, &outer_body, &mut witness)?;
    c.birth(ob, &inputs.sites[1], INNER, &mut outer_body, &mut witness)?;
    let inner_birth = g.definition(outer_body.get(Role::InnerSource)?)?;
    need(
        g.position(before)? < g.position(inner_birth)?,
        "nested SSA outer read precedes fresh inner acquire",
        (before, inner_birth),
    )?;
    let inner_entry = g.branch(ob)?;
    let ih = inner_entry.target;
    let inner_header = witness.edge(&g, ob, inner_entry, &outer_body)?;
    let (ib, mut inner_body, ie, inner_exit) =
        c.header(ih, &inputs.sites[1], INNER, &inner_header, &mut witness)?;
    need(
        [pre, oh, ob, oe, ih, ib, ie]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len()
            == 7,
        "nested SSA seven distinct core roles including actual inner false resume",
        (pre, oh, ob, oe, ih, ib, ie),
    )?;
    g.incoming_from(pre, &[])?;
    g.incoming_from(oh, &[pre, ie])?;
    g.incoming_from(ih, &[ob, ib])?;
    for (block, from) in [(ob, oh), (oe, oh), (ib, ih), (ie, ih)] {
        g.incoming_from(block, &[from])?;
    }
    c.project(ib, &inputs.sites[1], INNER, &mut inner_body, &mut witness)?;
    let outer_in = c.read(ib, &inputs.reads[1], OUTER, &inner_body, &mut witness)?;
    let inner_read = c.read(ib, &inputs.reads[2], INNER, &inner_body, &mut witness)?;
    need(
        g.position(outer_in)? < g.position(inner_read)?,
        "nested SSA actual two inner-body consumers ordered",
        (outer_in, inner_read),
    )?;
    let inner_back_roles = c.cleanup_next(
        ib,
        &inputs.sites[1],
        INNER,
        &inner_body,
        inner_read,
        &mut witness,
    )?;
    let inner_back = g.branch(ib)?;
    need(
        inner_back.target == ih,
        "nested SSA inner Next returns to its own header",
        inner_back,
    )?;
    need(
        witness.edge(&g, ib, inner_back, &inner_back_roles)? == inner_header,
        "nested SSA every inner header incoming matches all seven precise roles",
        inner_back,
    )?;
    let inner_end = c.end_source(ie, &inputs.sites[1], INNER, &inner_exit, &mut witness)?;
    let resumed = inner_exit.retain(&[
        Role::Caller,
        Role::OuterSource,
        Role::OuterCursor,
        Role::OuterKey,
        Role::OuterValue,
    ])?;
    let after_read = c.read(ie, &inputs.reads[3], OUTER, &resumed, &mut witness)?;
    need(
        g.position(inner_end)? < g.position(after_read)?,
        "nested SSA inner-only EndSource before actual resumed outer read",
        (inner_end, after_read),
    )?;
    let outer_back_roles = c.cleanup_next(
        ie,
        &inputs.sites[0],
        OUTER,
        &resumed,
        after_read,
        &mut witness,
    )?;
    let outer_back = g.branch(ie)?;
    need(
        outer_back.target == oh,
        "nested SSA outer Next returns to its own header",
        outer_back,
    )?;
    need(
        witness.edge(&g, ie, outer_back, &outer_back_roles)? == outer_header,
        "nested SSA every outer header incoming matches caller/source/new cursor",
        outer_back,
    )?;
    c.end_source(oe, &inputs.sites[0], OUTER, &outer_exit, &mut witness)?;
    let mut tail_roles = outer_exit.retain(&[Role::Caller])?;
    let mut tail = oe;
    for &span in &inputs.sizes {
        let size = query_tail::size(&g, span, tail_roles.get(Role::Caller)?, &mut witness)?;
        need(
            size.block == tail,
            "nested SSA post-outer size uses actual current continuation",
            size,
        )?;
        (tail, tail_roles) = query_tail::conditional(&g, f, size, &tail_roles, &mut witness)?;
    }
    need(
        g.ops(tail)?.is_empty()
            && g.term(tail)?.origin == Origin::Source(inputs.scan)
            && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty()),
        "nested SSA after both sizes normal Unit return preserves caller",
        g.term(tail)?,
    )?;
    need(
        g.instructions
            .iter()
            .filter(|i| provider(i))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            == witness.providers
            && witness.providers.len() == 10,
        "nested SSA all provider operations belong exactly to the two proven sites",
        &witness.providers,
    )?;
    let actual_calls = g
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    need(
        actual_calls
            == [before.id, outer_in.id, inner_read.id, after_read.id]
                .into_iter()
                .collect()
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
                .count()
                == inputs.sizes.len(),
        "nested SSA complete actual read and post-loop size sets",
        actual_calls,
    )?;
    // The exact edge set has no OE-to-acquire path and no inner role on outer backedge.
    // Per-role aliases and Uses prove caller/OS stay live through the inner-only prefix.
    let related = [Role::Caller, Role::OuterSource, Role::InnerSource]
        .into_iter()
        .flat_map(|role| witness.aliases.get(&role).into_iter().flatten().copied())
        .collect::<BTreeSet<_>>();
    super::super::query_inventory::check(&g, &[map_type], &related)?;
    witness.finish(&g)
}

pub(in crate::ssa) mod query_callers {
    //! Real entry owner -> call loan -> scan parameter, and read's borrowed consumer slots.
    use super::super::super::{
        fixture::{Failure, contains, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    };
    use super::super::{
        ast::Input,
        query_graph::{Roles, Witness, full_graph, ty},
        query_inputs::Inputs,
        query_tail,
    };
    use crate::ssa::model::*;
    use lang_frontend::parser::Expression;
    use std::collections::BTreeSet;

    fn signature(
        function: &Function,
        expected: &[EntityType],
        returns: &[SsaTypeId],
    ) -> Result<(), Failure> {
        let entry = function
            .entry_block()
            .and_then(|b| function.block(b))
            .ok_or_else(|| failure("nested SSA callable entry", function.id))?;
        need(
            function.receiver.is_none()
                && function.borrow_return.is_none()
                && function.carrier_return.is_none()
                && function.return_types == returns
                && entry.origin == function.origin
                && entry.parameters.len() == expected.len()
                && entry
                    .parameters
                    .iter()
                    .zip(expected)
                    .enumerate()
                    .all(|(index, (&p, ty))| {
                        function.entity(p).is_some_and(|e| {
                            e.ty == *ty
                                && e.origin == function.origin
                                && e.definition
                                    == Definition::BlockParameter {
                                        block: entry.id,
                                        index,
                                    }
                        })
                    }),
            "nested SSA source callable actual parameter definitions/signature",
            function,
        )
    }

    fn consumer(
        read: &Function,
        f: &Input,
        inputs: &Inputs,
        string: SsaTypeId,
    ) -> Result<(), Failure> {
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: string,
        };
        signature(read, &[shared, shared], &[])?;
        let g = Graph::new(read)?;
        full_graph(&g)?;
        let block = one(
            g.blocks.iter().copied(),
            "nested SSA actual read single body",
        )?;
        let params = &g.block(block)?.parameters;
        let calls = f
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, n)| {
                matches!(n.payload(), Expression::Call { .. })
                    .then_some(n.span())
                    .filter(|&s| contains(inputs.read, s))
            })
            .collect::<Vec<_>>();
        need(
            calls.len() == 2 && g.instructions.len() == 2 && params[0] != params[1],
            "nested SSA exact source read consumers",
            (&calls, read),
        )?;
        let mut calls = calls;
        calls.sort_by_key(|s| s.start());
        let mut uses = Uses::default();
        for (index, &param) in params.iter().enumerate() {
            uses.protect(param);
            let instruction = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.origin == Origin::Source(calls[index])),
                "nested SSA actual println source consumer",
            )?;
            let EntityId::Loan(actual) = param else {
                return Err(failure("nested SSA actual read Borrow slot", param));
            };
            need(
                instruction.operation == Operation::PrintString { value: actual }
                    && instruction.results.is_empty()
                    && g.position(instruction)? == index,
                "nested SSA consumer reads its own ordered Borrow parameter",
                instruction,
            )?;
            uses.permit(param, instruction);
        }
        need(
            matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.is_empty())
                && g.term(block)?.origin == Origin::Source(inputs.read),
            "nested SSA read Unit return no borrowed escape",
            g.term(block)?,
        )?;
        uses.check(&g, None)
    }

    pub(in crate::ssa) fn check(
        module: &Module,
        map_type: SsaTypeId,
        f: &Input,
        inputs: &Inputs,
    ) -> Result<(), Failure> {
        let entry = function(module, inputs.entry)?;
        let scan = function(module, inputs.scan)?;
        let read = function(module, inputs.read)?;
        let factory = function(module, inputs.factory)?;
        let (_, string, val) = module
            .map_container(map_type)
            .ok_or_else(|| failure("nested SSA caller map type", map_type))?;
        need(
            string == val,
            "nested SSA exact fixture String K/V",
            map_type,
        )?;
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: map_type,
        };
        signature(entry, &[], &[])?;
        signature(scan, &[shared], &[])?;
        signature(factory, &[], &[map_type])?;
        consumer(read, f, inputs, string)?;
        let g = Graph::new(entry)?;
        full_graph(&g)?;
        let mut witness = Witness::default();
        let mut protected = BTreeSet::new();
        let mut calls = BTreeSet::new();
        let mut tail = entry
            .entry_block()
            .ok_or_else(|| failure("nested SSA source entry", entry.id))?;
        g.incoming_from(tail, &[])?;
        for (index, anchor) in inputs.callers.iter().enumerate() {
            let birth = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(anchor.initializer)
            && if index == 0 { matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == factory.id && arguments.is_empty()) } else { i.operation == Operation::MapConstruct { map_type } }),
            "nested SSA actual distinct caller owner initializer")?;
            let owner = value(birth)?;
            need(
                birth.block == tail && ty(&g, result(birth)?)? == EntityType::Value(map_type),
                "nested SSA actual owning caller result in source-order continuation",
                birth,
            )?;
            if index == 0 {
                calls.insert(birth.id);
            }
            let root = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::RootPlace { owner }),
                "nested SSA actual caller owner unique RootPlace",
            )?;
            let EntityId::Place(place) = result(root)? else {
                return Err(failure("nested SSA actual caller Place", root));
            };
            let begin = one(
                g.instructions.iter().copied().filter(|i| {
                    i.operation
                        == Operation::BorrowBegin {
                            place,
                            kind: LoanKind::Shared,
                        }
                }),
                "nested SSA actual caller Shared loan",
            )?;
            let source = loan(begin)?;
            let call = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(anchor.call)
            && matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == scan.id && arguments.as_slice() == [EntityId::Loan(source)])), "nested SSA actual caller scan argument")?;
            calls.insert(call.id);
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
                "nested SSA actual caller call-loan end",
            )?;
            let size = query_tail::size(&g, anchor.size, EntityId::Value(owner), &mut witness)?;
            let drop = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "nested SSA caller exact owner Drop",
            )?;
            let sequence = [birth, root, begin, call, end, size, drop];
            need(
                sequence.iter().all(|i| i.block == tail)
                    && sequence.windows(2).all(|p| {
                        g.position(p[0])
                            .ok()
                            .zip(g.position(p[1]).ok())
                            .is_some_and(|(a, b)| a < b)
                    })
                    && root.origin == Origin::Source(anchor.argument)
                    && begin.origin == Origin::Source(anchor.argument)
                    && end.origin == Origin::Source(anchor.call)
                    && drop.origin == Origin::Source(anchor.drop)
                    && anchor.drop == anchor.local
                    && call.results.is_empty()
                    && end.results.is_empty()
                    && drop.results.is_empty()
                    && ty(&g, EntityId::Place(place))? == EntityType::Place(map_type)
                    && ty(&g, EntityId::Loan(source))? == shared,
                "nested SSA caller loan protects actual owner through scan then size before actual FE Drop",
                (&sequence, anchor),
            )?;
            for entity in [
                EntityId::Value(owner),
                EntityId::Place(place),
                EntityId::Loan(source),
            ] {
                need(
                    protected.insert(entity),
                    "nested SSA two caller identities remain disjoint",
                    entity,
                )?;
                witness.uses.protect(entity);
            }
            witness.uses.permit(EntityId::Value(owner), root);
            witness.uses.permit(EntityId::Value(owner), drop);
            witness.uses.permit(EntityId::Place(place), begin);
            witness.uses.permit(EntityId::Loan(source), call);
            witness.uses.permit(EntityId::Loan(source), end);
            // A call links distinct function-scoped entity IDs by the actual callee/slot/type only.
            (tail, _) = query_tail::conditional(&g, f, size, &Roles::default(), &mut witness)?;
        }
        need(
            g.ops(tail)?.is_empty()
                && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty())
                && g.term(tail)?.origin == Origin::Source(inputs.entry),
            "nested SSA caller reaches actual Unit return",
            g.term(tail)?,
        )?;
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                .map(|i| i.id)
                .collect::<BTreeSet<_>>()
                == calls
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
                    .count()
                    == 2
                && !g
                    .instructions
                    .iter()
                    .any(|i| super::super::query_body::provider(i)),
            "nested SSA exact complete caller call/size set with no hidden provider",
            &calls,
        )?;
        super::super::super::query_inventory::check(&g, &[map_type], &protected)?;
        witness.finish(&g)
    }
}

pub(in crate::ssa) mod query_body {
    //! Each actual For operation consumes its own current role; no production witness is used.
    use super::super::super::{
        fixture::{Failure, contains, failure, need},
        query_graph::{Graph, loan, result, value},
        query_inputs::one,
    };
    use super::super::{
        query_graph::{Role, Roles, Witness, ty},
        query_inputs::{ReadInput, SiteInput},
    };
    use crate::ssa::model::*;

    pub(in crate::ssa) fn provider(i: &Instruction) -> bool {
        matches!(
            i.operation,
            Operation::MapIterationFirst { .. }
                | Operation::MapIterationHasNext { .. }
                | Operation::MapIterationNext { .. }
                | Operation::MapIterationKeyLoan { .. }
                | Operation::MapIterationValueLoan { .. }
        )
    }
    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Layer {
        pub source: Role,
        pub cursor: Role,
        pub key: Role,
        pub val: Role,
    }
    pub(in crate::ssa) const OUTER: Layer = Layer {
        source: Role::OuterSource,
        cursor: Role::OuterCursor,
        key: Role::OuterKey,
        val: Role::OuterValue,
    };
    pub(in crate::ssa) const INNER: Layer = Layer {
        source: Role::InnerSource,
        cursor: Role::InnerCursor,
        key: Role::InnerKey,
        val: Role::InnerValue,
    };
    pub(in crate::ssa) struct Context<'a, 'ir> {
        pub graph: &'a Graph<'ir>,
        pub module: &'ir Module,
        pub read: &'ir Function,
    }
    impl<'ir> Context<'_, 'ir> {
        pub(in crate::ssa::map_provider_exits_pipeline_tests::nested) fn birth(
            &self,
            block: BlockId,
            site: &SiteInput,
            layer: Layer,
            roles: &mut Roles,
            w: &mut Witness,
        ) -> Result<(), Failure> {
            let g = self.graph;
            need(
                contains(site.span, site.source) && contains(site.span, site.body),
                "nested SSA consumes validated source/body bridge",
                site,
            )?;
            let parent = roles.loan(Role::Caller)?;
            let born = one(
                g.ops(block)?.into_iter().filter(|i| {
                    i.origin == Origin::Source(site.span)
                        && matches!(i.operation, Operation::SharedReborrow { .. })
                }),
                "nested SSA unique For lease birth",
            )?;
            need(
                born.operation == Operation::SharedReborrow { source: parent },
                "nested SSA For lease from current caller, not outer lease",
                born,
            )?;
            let source = loan(born)?;
            need(
                ty(g, result(born)?)? == ty(g, EntityId::Loan(parent))?,
                "nested SSA lease preserves Shared map target",
                born,
            )?;
            let first = one(
                g.ops(block)?.into_iter().filter(|i| {
                    i.origin == Origin::Source(site.span)
                        && matches!(i.operation, Operation::MapIterationFirst { .. })
                }),
                "nested SSA unique For First",
            )?;
            need(
                first.operation == Operation::MapIterationFirst { source }
                    && matches!(ty(g, result(first)?)?, EntityType::Value(t) if self.module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))
                    && g.ops(block)?[g.position(born)?..]
                        .iter()
                        .map(|i| i.id)
                        .collect::<Vec<_>>()
                        == [born.id, first.id],
                "nested SSA actual reborrow/First suffix and cursor type",
                (born, first),
            )?;
            roles.insert(layer.source, result(born)?);
            roles.insert(layer.cursor, result(first)?);
            w.protect(roles)?;
            w.uses.permit(EntityId::Loan(parent), born);
            w.uses.permit(EntityId::Loan(source), first);
            w.providers.insert(first.id);
            need(
                g.term(block)?.origin == Origin::Source(site.span),
                "nested SSA actual preheader branch origin",
                g.term(block)?,
            )
        }
        pub(in crate::ssa::map_provider_exits_pipeline_tests::nested) fn header(
            &self,
            block: BlockId,
            site: &SiteInput,
            layer: Layer,
            roles: &Roles,
            w: &mut Witness,
        ) -> Result<(BlockId, Roles, BlockId, Roles), Failure> {
            let g = self.graph;
            let has = one(
                g.ops(block)?.into_iter(),
                "nested SSA header only actual HasNext",
            )?;
            need(
                has.operation
                    == Operation::MapIterationHasNext {
                        source: roles.loan(layer.source)?,
                        cursor: roles.value(layer.cursor)?,
                    }
                    && has.origin == Origin::Source(site.span)
                    && matches!(ty(g, result(has)?)?, EntityType::Value(t) if self.module.type_kind(t) == Some(&SsaTypeKind::Boolean)),
                "nested SSA header current lease/cursor condition",
                has,
            )?;
            w.uses.permit(roles.get(layer.source)?, has);
            w.uses.permit(roles.get(layer.cursor)?, has);
            w.providers.insert(has.id);
            let term = g.term(block)?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &term.kind
            else {
                return Err(failure("nested SSA actual provider conditional", term));
            };
            need(
                *condition == value(has)?
                    && term.origin == Origin::Source(site.span)
                    && when_true.target != when_false.target
                    && when_true.target != block
                    && when_false.target != block,
                "nested SSA HasNext Bool drives both real provider arms",
                term,
            )?;
            w.conditions.insert(block, *condition);
            for i in &g.instructions {
                need(
                    !i.operation
                        .entities()
                        .contains(&EntityId::Value(*condition)),
                    "nested SSA provider Bool has no instruction use",
                    i,
                )?;
            }
            for &b in &g.blocks {
                let t = g.term(b)?;
                if t.kind.entities().contains(&EntityId::Value(*condition)) {
                    need(
                        b == block
                            && !when_true.arguments.contains(&EntityId::Value(*condition))
                            && !when_false.arguments.contains(&EntityId::Value(*condition)),
                        "nested SSA provider Bool sole terminal use",
                        t,
                    )?;
                }
            }
            for b in [block, when_true.target, when_false.target] {
                need(
                    g.block(b)?.origin == Origin::Source(site.span),
                    "nested SSA actual For role block Origin",
                    g.block(b)?,
                )?;
            }
            let yes = w.edge(g, block, when_true, roles)?;
            let no = w.edge(g, block, when_false, roles)?;
            Ok((when_true.target, yes, when_false.target, no))
        }
        pub(in crate::ssa::map_provider_exits_pipeline_tests::nested) fn project(
            &self,
            block: BlockId,
            site: &SiteInput,
            layer: Layer,
            roles: &mut Roles,
            w: &mut Witness,
        ) -> Result<(), Failure> {
            let g = self.graph;
            let source = roles.loan(layer.source)?;
            let cursor = roles.value(layer.cursor)?;
            let key = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
                "nested SSA unique real key projection",
            )?;
            let val = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
                "nested SSA unique real value projection",
            )?;
            need(
                key.operation == Operation::MapIterationKeyLoan { source, cursor }
                    && val.operation == Operation::MapIterationValueLoan { source, cursor }
                    && key.origin == Origin::Source(site.span)
                    && val.origin == Origin::Source(site.span)
                    && g.position(key)? == 0
                    && g.position(val)? == 1,
                "nested SSA ordered K/V birth from own current lease/cursor",
                (key, val),
            )?;
            for i in [key, val] {
                need(
                    matches!(ty(g, result(i)?)?, EntityType::Loan { kind: LoanKind::Shared, target }
                if self.module.type_kind(target) == Some(&SsaTypeKind::StringOwner)),
                    "nested SSA actual Shared String component",
                    i,
                )?;
                w.uses.permit(EntityId::Loan(source), i);
                w.uses.permit(EntityId::Value(cursor), i);
                w.providers.insert(i.id);
            }
            roles.insert(layer.key, result(key)?);
            roles.insert(layer.val, result(val)?);
            w.protect(roles)
        }
        pub(in crate::ssa::map_provider_exits_pipeline_tests::nested) fn read(
            &self,
            block: BlockId,
            input: &ReadInput,
            layer: Layer,
            roles: &Roles,
            w: &mut Witness,
        ) -> Result<&'ir Instruction, Failure> {
            let g = self.graph;
            let i = one(
                g.ops(block)?.into_iter().filter(|i| {
                    i.origin == Origin::Source(input.span)
                        && matches!(i.operation, Operation::DirectCall { .. })
                }),
                "nested SSA actual read Call by source origin",
            )?;
            let args = [roles.get(layer.key)?, roles.get(layer.val)?];
            let params = &g.function; // Keep function-scoped entity lookup separate from callee slots.
            let callee_entry = self
                .read
                .entry_block()
                .and_then(|b| self.read.block(b))
                .ok_or_else(|| failure("nested SSA actual read entry", self.read.id))?;
            need(
                matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == self.read.id && arguments.as_slice() == args)
                    && i.results.is_empty()
                    && callee_entry.parameters.len() == 2
                    && callee_entry.parameters.iter().zip(args).all(|(&p, a)| {
                        self.read.entity(p).map(|d| d.ty) == params.entity(a).map(|d| d.ty)
                    })
                    && input.arguments[0] != input.arguments[1]
                    && input.arguments.iter().all(|&s| contains(input.span, s))
                    && input.site
                        == if layer.source == Role::OuterSource {
                            0
                        } else {
                            1
                        },
                "nested SSA actual ordered current Borrow operands and callee parameter positions",
                (i, input, &args),
            )?;
            for a in args {
                w.uses.permit(a, i);
            }
            Ok(i)
        }
        pub(in crate::ssa::map_provider_exits_pipeline_tests::nested) fn end_source(
            &self,
            block: BlockId,
            site: &SiteInput,
            layer: Layer,
            roles: &Roles,
            w: &mut Witness,
        ) -> Result<&'ir Instruction, Failure> {
            let g = self.graph;
            let source = roles.loan(layer.source)?;
            let i = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
                "nested SSA unique current EndSource",
            )?;
            need(
                i.origin == Origin::Source(site.span) && g.position(i)? == 0,
                "nested SSA exhaustion EndSource actual prefix",
                i,
            )?;
            w.uses.permit(EntityId::Loan(source), i);
            Ok(i)
        }
        pub(in crate::ssa::map_provider_exits_pipeline_tests::nested) fn cleanup_next(
            &self,
            block: BlockId,
            site: &SiteInput,
            layer: Layer,
            roles: &Roles,
            last_read: &Instruction,
            w: &mut Witness,
        ) -> Result<Roles, Failure> {
            let g = self.graph;
            let mut suffix = Vec::new();
            for r in [layer.val, layer.key] {
                let loan = roles.loan(r)?;
                let end = one(
                    g.ops(block)?
                        .into_iter()
                        .filter(|i| i.operation == Operation::BorrowEnd { loan }),
                    "nested SSA unique current component EndBinding",
                )?;
                need(
                    end.origin == Origin::Source(site.span)
                        && g.position(last_read)? < g.position(end)?,
                    "nested SSA actual component cleanup after last Borrow read",
                    end,
                )?;
                w.uses.permit(EntityId::Loan(loan), end);
                suffix.push(end.id);
            }
            let next = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
                "nested SSA this tail unique Next",
            )?;
            need(
                next.operation
                    == Operation::MapIterationNext {
                        source: roles.loan(layer.source)?,
                        cursor: roles.value(layer.cursor)?,
                    }
                    && next.origin == Origin::Source(site.span)
                    && ty(g, result(next)?)? == ty(g, roles.get(layer.cursor)?)?,
                "nested SSA Next actual current lease/cursor and result type",
                next,
            )?;
            suffix.push(next.id);
            let first = g
                .function
                .instruction(suffix[0])
                .ok_or_else(|| failure("nested SSA actual cleanup suffix head", &suffix))?;
            need(
                g.ops(block)?[g.position(first)?..]
                    .iter()
                    .map(|i| i.id)
                    .collect::<Vec<_>>()
                    == suffix
                    && g.term(block)?.origin == Origin::Source(site.span),
                "nested SSA complete V/K/Next suffix matches FE normal cleanup",
                (&suffix, g.ops(block)?),
            )?;
            w.uses.permit(roles.get(layer.source)?, next);
            w.uses.permit(roles.get(layer.cursor)?, next);
            w.providers.insert(next.id);
            let mut after = roles.clone();
            after.0.remove(&layer.key);
            after.0.remove(&layer.val);
            after.insert(layer.cursor, result(next)?);
            w.protect(&after)?;
            Ok(after)
        }
    }
}
