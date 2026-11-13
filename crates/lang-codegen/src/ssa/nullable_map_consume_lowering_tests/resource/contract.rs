//! Resource success obligations from the original sealed Unit facts and actual SSA.

use lang_frontend::{
    name_resolution::{DeclarationId, SymbolKind},
    ownership_checking::{OwnedCompilationUnitView, UnitDropTarget},
    type_checking::{BuiltinType, IntrinsicTypeConstructor, UnitTypeKind},
};

use super::*;
use crate::ssa::{
    lowering_support::parsed_by_source_unit,
    model::{
        EntityId, EntityType, Function, LoanKind, MapContainerKind, Operation, Origin, Ownership,
        SsaTypeId, SsaTypeKind, ValueId,
    },
    unit_plan::{MAX_UNIT_GENERIC_INSTANCES, plan_unit_instances_from_facts},
};

mod transfer {
    //! Exact remove result, assertion edge/proof/take and the final owner births.

    use std::collections::BTreeSet;

    use lang_frontend::{
        ownership_checking::NonNullAssertionTransferKind,
        parser::{Expression, LiteralKind},
        type_checking::{AssertionFailureEffect, UnitExpressionId, UnitTypeId},
    };

    use super::*;
    use crate::ssa::model::{Definition, Edge, TerminatorKind};

    fn edges(function: &Function) -> Vec<&Edge> {
        function
            .blocks
            .iter()
            .flat_map(
                |block| match block.terminator.as_ref().map(|term| &term.kind) {
                    Some(TerminatorKind::Branch(edge)) => vec![edge],
                    Some(TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    }) => {
                        vec![when_true, when_false]
                    }
                    Some(TerminatorKind::NullableBranch {
                        when_null,
                        when_non_null,
                        ..
                    }) => {
                        vec![when_null, when_non_null]
                    }
                    _ => Vec::new(),
                },
            )
            .collect()
    }

    pub(super) fn same_owner(function: &Function, current: ValueId, birth: ValueId) -> Outcome<()> {
        let mut seen = BTreeSet::new();
        let mut pending = vec![current];
        let ty = value_type(function, birth)?;
        while let Some(value) = pending.pop() {
            gate(
                "Resource owner transport keeps exact storage",
                value_type(function, value)? == ty,
                value,
            )?;
            if value == birth || !seen.insert(value) {
                continue;
            }
            let data = function
                .entity(EntityId::Value(value))
                .ok_or_else(|| missing("Resource transported entity", value))?;
            let Definition::BlockParameter { block, index } = data.definition else {
                return Err(missing(
                    "Resource cleanup cannot substitute a same-typed foreign owner",
                    data,
                ));
            };
            let incoming: Vec<_> = edges(function)
                .into_iter()
                .filter(|edge| edge.target == block)
                .collect();
            gate(
                "Resource transported owner has its real incoming column",
                !incoming.is_empty(),
                data,
            )?;
            for edge in incoming {
                let Some(EntityId::Value(parent)) = edge.arguments.get(index) else {
                    return Err(missing("Resource owner incoming argument", edge));
                };
                // These frozen straight-line programs have only an assertion branch, no loop.
                gate(
                    "Resource owner lineage is acyclic",
                    !seen.contains(parent),
                    (value, parent),
                )?;
                pending.push(*parent);
            }
        }
        Ok(())
    }

    pub(super) fn check(
        view: &OwnedCompilationUnitView<'_, '_>,
        parsed: &ParsedFile,
        function: &Function,
        nullable_type: UnitTypeId,
        nullable: SsaTypeId,
        inner: SsaTypeId,
    ) -> Outcome<Vec<(&'static str, ValueId)>> {
        let source_unit = view
            .names()
            .names()
            .index()
            .source_units()
            .iter()
            .find(|source| source.source_id() == parsed.source_id())
            .ok_or_else(|| missing("Resource transfer original source unit", parsed.source_id()))?
            .id();
        let mut births = Vec::new();
        let nulls: Vec<_> = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableNull { .. }))
            .collect();
        let ordinary: Vec<_> = view
            .names()
            .names()
            .source_units()
            .iter()
            .filter(|source| source.source_unit() == source_unit)
            .flat_map(|source| source.resolution().symbols())
            .filter(|symbol| symbol.name() == "ordinary" && !symbol.is_synthetic())
            .collect();
        gate(
            "ordinary null retains its own operation",
            nulls.len() == ordinary.len(),
            (&nulls, &ordinary),
        )?;
        for null in nulls {
            let expressions: Vec<_> = parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| {
                    node.span() == null.origin.span()
                        && matches!(node.payload(), Expression::Literal(LiteralKind::Null))
                })
                .collect();
            let &(expression, _) = one(
                &expressions,
                "ordinary N comes from its original null expression",
            )?;
            gate(
                "ordinary null and real remove result share the exact N identity",
                null.operation == Operation::NullableNull { nullable }
                    && value_type(function, result(null, 0)?)? == nullable
                    && view
                        .types()
                        .expression_type(UnitExpressionId::new(source_unit, expression))
                        == Some(nullable_type),
                null,
            )?;
            births.push(("ordinary", result(null, 0)?));
        }
        let removes: Vec<_> = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::MapRemove { .. }))
            .collect();
        let descriptors = view.types().map_removes();
        gate(
            "all original remove descriptors have exactly one real operation",
            removes.len() == descriptors.len(),
            (&removes, descriptors),
        )?;
        if descriptors.is_empty() {
            gate(
                "null-only source has no fake remove, assertion or take",
                view.ownership().non_null_assertions().is_empty()
                    && function.instructions.iter().all(|instruction| {
                        !matches!(instruction.operation, Operation::NullableTake { .. })
                    })
                    && function.blocks.iter().all(|block| {
                        !matches!(
                            block.terminator.as_ref().map(|term| &term.kind),
                            Some(TerminatorKind::NullableBranch { .. })
                        )
                    }),
                function,
            )?;
            return Ok(births);
        }
        let descriptor = one(descriptors, "one original Resource remove descriptor")?;
        let remove = *one(&removes, "one actual original Resource remove")?;
        let remove_span = parsed
            .ast()
            .expressions()
            .get(descriptor.expression().expression())
            .map_err(|error| missing("Resource remove original expression", error))?
            .span();
        let plan = one(
            view.ownership().non_null_assertions(),
            "one original Resource assertion ownership plan",
        )?;
        let assertion = plan.descriptor();
        let assertion_span = parsed
            .ast()
            .expressions()
            .get(assertion.expression().expression())
            .map_err(|error| missing("Resource assertion original expression", error))?
            .span();
        gate(
            "remove result and assertion plan retain the original qualified identity",
            remove.origin == Origin::Source(remove_span)
                && descriptor.expression().source_unit() == source_unit
                && descriptor.result_type() == nullable_type
                && assertion.operand() == descriptor.expression()
                && assertion.nullable_type() == nullable_type
                && view.types().non_null_assertions().contains(assertion)
                && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
                && plan.null_effect() == AssertionFailureEffect::Abort,
            (descriptor, plan, remove),
        )?;
        gate(
            "remove returns updated outer and one separate nullable owner",
            remove.results.len() == 2,
            remove,
        )?;
        let updated_outer = result(remove, 0)?;
        let wrapper = result(remove, 1)?;
        gate(
            "remove result uses N, not updated outer storage",
            value_type(function, wrapper)? == nullable
                && value_type(function, updated_outer)? != nullable,
            remove,
        )?;
        births.push(("outer", updated_outer));
        let branches: Vec<_> = function
            .blocks
            .iter()
            .filter_map(|block| {
                let term = block.terminator.as_ref()?;
                matches!(term.kind, TerminatorKind::NullableBranch { .. }).then_some((block, term))
            })
            .collect();
        let &(header, branch) = one(&branches, "one actual assertion branch")?;
        let TerminatorKind::NullableBranch {
            owner,
            when_null,
            when_non_null,
            view: proof,
        } = &branch.kind
        else {
            unreachable!("filtered nullable branch")
        };
        gate(
            "assertion tests the exact remove result once",
            *owner == wrapper && branch.origin == Origin::Source(assertion_span),
            branch,
        )?;
        let absent = function
            .block(when_null.target)
            .ok_or_else(|| missing("assertion null block", when_null))?;
        let present = function
            .block(when_non_null.target)
            .ok_or_else(|| missing("assertion present block", when_non_null))?;
        let columns: Vec<_> = when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (*value == EntityId::Value(wrapper)).then_some(index))
            .collect();
        let &column = one(&columns, "one exact non-null owner column")?;
        let Some(EntityId::Value(carried)) = present.parameters.get(column).copied() else {
            return Err(missing("non-null owner parameter", present));
        };
        gate(
            "assertion has exact owner/proof edge identity and no null unwind",
            absent.id != present.id
                && when_null.arguments.get(column) == Some(&EntityId::Value(wrapper))
                && present.parameters.len() == when_non_null.arguments.len() + 1
                && present.parameters.last() == Some(&EntityId::Loan(*proof))
                && absent.parameters.len() == when_null.arguments.len()
                && absent.instructions.is_empty()
                && absent
                    .terminator
                    .as_ref()
                    .is_some_and(|term| matches!(term.kind, TerminatorKind::Abort)),
            (header, absent, present),
        )?;
        for edge in [when_null, when_non_null] {
            let incoming: Vec<_> = edges(function)
                .into_iter()
                .filter(|actual| actual.target == edge.target)
                .collect();
            gate(
                "assertion successor has only its original incoming edge",
                incoming.as_slice() == [edge],
                incoming,
            )?;
        }
        let takes: Vec<_> = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
            .collect();
        let take = *one(&takes, "one actual Resource NullableTake")?;
        gate(
            "NullableTake consumes the exact non-null wrapper/proof once",
            take.block == present.id
                && take.origin == Origin::Source(assertion_span)
                && take.operation
                    == Operation::NullableTake {
                        owner: carried,
                        proof: *proof,
                    }
                && take.results.len() == 1
                && value_type(function, result(take, 0)?)? == inner,
            take,
        )?;
        same_owner(function, carried, wrapper)?;
        for instruction in &function.instructions {
            if instruction.id != take.id {
                gate(
                    "consumed nullable wrapper and proof have no other instruction user",
                    !instruction.operation.entities().iter().any(|entity| {
                        [
                            EntityId::Value(wrapper),
                            EntityId::Value(carried),
                            EntityId::Loan(*proof),
                        ]
                        .contains(entity)
                    }),
                    instruction,
                )?;
            }
        }
        gate(
            "taken wrapper never returns or crosses a later edge",
            function
                .blocks
                .iter()
                .filter(|block| block.id != header.id)
                .all(|block| {
                    block.terminator.as_ref().is_none_or(|term| {
                        !term.kind.entities().contains(&EntityId::Value(carried))
                    })
                }),
            carried,
        )?;
        births.push(("got", result(take, 0)?));
        Ok(births)
    }
}

pub(super) fn inspect(
    view: &OwnedCompilationUnitView<'_, '_>,
    entry: DeclarationId,
    program: &Program,
    lowered_entry: FunctionId,
    trace: &mut Trace,
) {
    if let Err(failure) = check(view, entry, program, lowered_entry, trace) {
        trace.check(&failure.stage, false, failure.detail);
    }
}

fn missing(stage: &str, detail: impl std::fmt::Debug) -> Failure {
    Failure {
        stage: stage.to_owned(),
        detail: format!("{detail:#?}"),
    }
}

fn one<'a, T: std::fmt::Debug>(values: &'a [T], stage: &str) -> Outcome<&'a T> {
    match values {
        [value] => Ok(value),
        _ => Err(missing(stage, values)),
    }
}

fn value_type(function: &Function, value: ValueId) -> Outcome<SsaTypeId> {
    match function
        .entity(EntityId::Value(value))
        .map(|entity| entity.ty)
    {
        Some(EntityType::Value(ty)) => Ok(ty),
        actual => Err(missing("Resource owner has a real Value storage", actual)),
    }
}

fn result(instruction: &crate::ssa::model::Instruction, position: usize) -> Outcome<ValueId> {
    match instruction.results.get(position) {
        Some(EntityId::Value(value)) => Ok(*value),
        _ => Err(missing("Resource operation result identity", instruction)),
    }
}

fn check(
    view: &OwnedCompilationUnitView<'_, '_>,
    entry: DeclarationId,
    program: &Program,
    lowered_entry: FunctionId,
    trace: &mut Trace,
) -> Outcome<()> {
    let typed = view.types();
    let names = view.names();
    let files = parsed_by_source_unit(view.inputs(), names)
        .map_err(|error| missing("Resource same-source parsed inputs", error))?;
    let declarations: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "Resource" && declaration.kind() == SymbolKind::Classifier
        })
        .collect();
    let resource = *one(&declarations, "one original Resource declaration")?;
    let nominal = typed
        .signatures()
        .declaration(resource.id())
        .and_then(|signature| signature.nominal())
        .ok_or_else(|| missing("Resource published nominal", resource))?;
    let descriptor = nominal
        .deinit()
        .ok_or_else(|| missing("Resource published deinit", nominal))?;
    let parsed = files
        .get(resource.source_unit().index())
        .copied()
        .ok_or_else(|| missing("Resource original parsed source", resource))?;
    let deinit_item = parsed
        .ast()
        .items()
        .get(descriptor.item().item())
        .map_err(|error| missing("Resource original deinit item", error))?;
    let plan = plan_unit_instances_from_facts(
        &files,
        names,
        typed,
        view.ownership(),
        entry,
        MAX_UNIT_GENERIC_INSTANCES,
    )
    .map_err(|error| missing("complete Resource instance planning", error))?;
    let deinits: Vec<_> = plan
        .iter()
        .filter(|instance| instance.key().deinit_owner().is_some())
        .collect();
    let planned = *one(&deinits, "exactly one reachable Resource deinit plan")?;
    gate(
        "Resource plan uses its real source-qualified descriptor exactly once",
        planned.key().deinit_owner() == Some(resource.id())
            && planned.source_unit() == resource.source_unit()
            && planned.item() == descriptor.item().item()
            && planned.span() == deinit_item.span()
            && descriptor.owner() == resource.id()
            && descriptor.item().source_unit() == resource.source_unit()
            && descriptor.body().source_unit() == resource.source_unit(),
        (planned, descriptor),
    )?;
    let module = one(&program.modules, "one original Resource module")?;
    let deinit_functions: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(deinit_item.span()))
        .collect();
    let deinit = *one(
        &deinit_functions,
        "one actual source-origin Resource deinit function",
    )?;
    let Some(EntityType::Loan {
        kind: LoanKind::Shared,
        target: resource_type,
    }) = deinit.receiver()
    else {
        return Err(missing("Resource deinit borrows its exact owner", deinit));
    };
    gate(
        "Resource source deinit registration is unique and exact",
        module.deinit(resource_type) == Some(deinit.id())
            && module.deinits.len() == 1
            && matches!(
                module.type_kind(resource_type),
                Some(SsaTypeKind::HeapOwner { .. })
            )
            && deinit.return_types.is_empty(),
        (&module.deinits, deinit),
    )?;
    let entry_decl = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.id() == entry)
        .ok_or_else(|| missing("actual selected Resource entry declaration", entry))?;
    let entry_file = files[entry_decl.source_unit().index()];
    let entry_span = entry_file
        .ast()
        .items()
        .get(entry_decl.root())
        .map_err(|error| missing("actual selected Resource entry item", error))?
        .span();
    gate(
        "Resource selected entry has exact source Function Item identity",
        module
            .function(lowered_entry)
            .is_some_and(|function| function.origin == Origin::Source(entry_span)),
        (entry_decl, lowered_entry),
    )?;
    let mains: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == resource.source_unit()
        })
        .collect();
    let main = *one(&mains, "one original Resource main")?;
    let main_span = parsed
        .ast()
        .items()
        .get(main.root())
        .map_err(|error| missing("Resource main item", error))?
        .span();
    let functions: Vec<_> = module
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(main_span))
        .collect();
    let function = *one(&functions, "one actual Resource main body")?;
    if entry != main.id() {
        let caller = module
            .function(lowered_entry)
            .expect("entry identity checked");
        gate(
            "real q.entry reaches the original p.main body",
            caller.instructions.iter().filter(|instruction| matches!(
                instruction.operation, Operation::DirectCall { callee, .. } if callee == function.id()
            )).count() == 1,
            caller,
        )?;
    }
    let int = typed
        .types()
        .builtin(BuiltinType::Int)
        .ok_or_else(|| missing("Resource Int key type", typed.types()))?;
    let nullables: Vec<_> = [
        IntrinsicTypeConstructor::Map,
        IntrinsicTypeConstructor::MutableMap,
    ]
    .into_iter()
    .filter_map(|constructor| {
        let inner = typed.types().find(&UnitTypeKind::Intrinsic {
            constructor,
            arguments: vec![int, nominal.ty()],
        })?;
        let nullable = typed.types().find(&UnitTypeKind::Nullable(inner))?;
        Some((nullable, inner, constructor))
    })
    .collect();
    let &(nullable, inner, constructor) = one(&nullables, "one exact Resource nullable Map type")?;
    let storage: Vec<_> = module.nullable_maps.iter().collect();
    let &(nullable_storage, inner_storage) = one(&storage, "one canonical Resource N storage")?;
    let kind = match constructor {
        IntrinsicTypeConstructor::Map => MapContainerKind::Map,
        IntrinsicTypeConstructor::MutableMap => MapContainerKind::MutableMap,
        _ => unreachable!("Map constructor checked"),
    };
    gate(
        "N is Map-only tag/payload storage, not legacy R or pointer nullable",
        module.nullable_map_inner(*nullable_storage) == Some(*inner_storage)
            && module.map_result_value(*nullable_storage).is_none()
            && module.valid_nullable_map_type(*nullable_storage, *inner_storage)
            && module
                .map_container(*inner_storage)
                .is_some_and(|(actual, _, value)| actual == kind && value == resource_type)
            && module.type_ownership(*nullable_storage) == Some(Ownership::MoveOnly)
            && module
                .map_results
                .values()
                .all(|value| module.map_container(*value).is_none()),
        (nullable, inner, &module.nullable_maps, &module.map_results),
    )?;
    let births = transfer::check(
        view,
        parsed,
        function,
        nullable,
        *nullable_storage,
        *inner_storage,
    )?;
    let facts: Vec<_> = view
        .ownership()
        .drops()
        .iter()
        .filter(|fact| {
            fact.value_origin().source_id() == main_span.source_id()
                && fact.value_origin().start() >= main_span.start()
                && fact.value_origin().end() <= main_span.end()
        })
        .collect();
    let drops: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .collect();
    gate(
        "all original Resource Drop facts are consumed exactly once",
        drops.len() == facts.len(),
        (&facts, &drops),
    )?;
    let resolution = names
        .names()
        .source_units()
        .iter()
        .find(|source| source.source_unit() == resource.source_unit())
        .ok_or_else(|| missing("Resource original symbol resolution", resource))?
        .resolution();
    let mut order = Vec::new();
    for fact in facts {
        let UnitDropTarget::Named(root) = fact.target() else {
            return Err(missing(
                "original Resource cleanup remains a named owner fact",
                fact,
            ));
        };
        gate(
            "Resource DropFact keeps its original source-qualified root",
            root.source_unit() == resource.source_unit()
                && view.ownership().formed_owned_locals().contains(&root),
            root,
        )?;
        let symbol = resolution
            .symbols()
            .get(root.symbol().index())
            .ok_or_else(|| missing("real Resource cleanup symbol", root))?;
        let birth = births
            .iter()
            .find(|(name, _)| *name == symbol.name())
            .map(|(_, value)| *value)
            .ok_or_else(|| {
                missing(
                    "cleanup comes from ordinary/got/updated outer owner",
                    symbol,
                )
            })?;
        let actual: Vec<_> = drops
            .iter()
            .copied()
            .filter(|instruction| instruction.origin == Origin::Source(fact.value_origin()))
            .collect();
        let drop = *one(&actual, "one Drop at the original UnitDropFact origin")?;
        let Operation::Drop { owner } = drop.operation else {
            unreachable!("filtered Drop")
        };
        transfer::same_owner(function, owner, birth)?;
        for &(point, previous) in &order {
            if point == fact.point() {
                gate(
                    "Resource cleanup retains original same-point fact order",
                    previous < drop.id.index(),
                    fact,
                )?;
            }
        }
        order.push((fact.point(), drop.id.index()));
    }
    trace.check(
        "complete Resource plan, N storage, owner transfer and original DropFact contract",
        true,
        (entry, lowered_entry, deinit.id()),
    );
    Ok(())
}
