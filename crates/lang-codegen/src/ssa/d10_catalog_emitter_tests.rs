//! Opt-in isolated no-loop RootReplace emitter and independent replay. No ordinary gate changes.
use super::{model::*, verify::verify_program};
use lang_frontend::{
    ownership_checking::{
        OwnedCompilationUnitView, UnitDropPoint, UnitDropTarget,
        d10_catalog_experiment::BoundCatalog,
    },
    type_checking::{BuiltinType, UnitTypeKind},
};

fn cleanup_points(bound: &BoundCatalog, unit: &OwnedCompilationUnitView<'_, '_>) -> Option<()> {
    use lang_frontend::parser::{Item, NameMarker, Statement};
    let [input] = unit.inputs() else { return None };
    let [old, incoming] = bound.producers() else {
        return None;
    };
    let relation = bound.replacement();
    let UnitDropPoint::AfterStatement(statement) = old.cleanup().point() else {
        return None;
    };
    let UnitDropTarget::Named(old_root) = old.cleanup().target() else {
        return None;
    };
    if statement.source_unit() != relation.call().source_unit()
        || old_root.source_unit() != statement.source_unit()
        || old_root == relation.root()
        || incoming.cleanup().target() != UnitDropTarget::Named(relation.root())
        || incoming.cleanup().point() != UnitDropPoint::CallReturn(relation.call())
    {
        return None;
    }
    let Statement::LocalVariable { declaration } = input
        .parsed()
        .ast()
        .statements()
        .get(statement.statement())
        .ok()?
        .payload()
    else {
        return None;
    };
    let Item::Variable {
        name: NameMarker::Present(span),
        initializer,
        ..
    } = input
        .parsed()
        .ast()
        .items()
        .get(*declaration)
        .ok()?
        .payload()
    else {
        return None;
    };
    if *initializer != relation.call().expression() {
        return None;
    }
    let names = unit
        .names()
        .names()
        .source_units()
        .iter()
        .find(|n| n.source_unit() == old_root.source_unit())?;
    let symbol = names
        .resolution()
        .symbols()
        .iter()
        .find(|s| s.id() == old_root.symbol())?;
    if symbol.span() != *span || old.cleanup().value_origin() != *span {
        return None;
    }
    Some(())
}

fn emit(bound: &BoundCatalog, unit: &OwnedCompilationUnitView<'_, '_>) -> Option<Program> {
    if !bound.matches(unit) {
        return None;
    }
    cleanup_points(bound, unit)?;
    let [input] = unit.inputs() else { return None };
    let parsed = input.parsed();
    let relation = bound.replacement();
    let [old, incoming] = bound.producers() else {
        return None;
    };
    if relation.previous() != old.identity()
        || relation.incoming() != incoming.identity()
        || old.identity() == incoming.identity()
        || incoming.cleanup().point() != UnitDropPoint::CallReturn(relation.call())
        || incoming.cleanup().target() != UnitDropTarget::Named(relation.root())
        || !matches!(old.cleanup().point(), UnitDropPoint::AfterStatement(_))
    {
        return None;
    }
    let callable = parsed
        .ast()
        .items()
        .get(bound.callable().item())
        .ok()?
        .span();
    let call = parsed
        .ast()
        .expressions()
        .get(relation.call().expression())
        .ok()?
        .span();
    let text = unit.sources().source_text(input.source_id()).ok()?;
    let mut program = Program::default();
    let module_id = program.add_module("d10.catalog.experiment");
    let module = program.module_mut(module_id)?;
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let function_id = module
        .add_function("d10.catalog.replace", Vec::new(), Origin::Source(callable))
        .ok()?;
    let function = module.function_mut(function_id)?;
    let block = function
        .add_block(Vec::new(), Origin::Source(callable))
        .ok()?;
    let mut literals = Vec::new();
    for producer in [old, incoming] {
        let expression = producer.expression();
        if unit
            .types()
            .types()
            .get(unit.types().expression_type(expression)?)
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            return None;
        }
        let origin = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .ok()?
            .span();
        let bytes = super::lowering_support::string_literal::decode_plain(
            parsed,
            text,
            expression.expression(),
        )
        .ok()??;
        literals.push((bytes, Origin::Source(origin)));
    }
    let literal = |function: &mut Function, index: usize| -> Option<ValueId> {
        let (bytes, origin) = &literals[index];
        let (_, results) = function
            .append_instruction(
                block,
                Operation::StringLiteral {
                    string,
                    bytes: bytes.clone(),
                },
                vec![EntityType::Value(string)],
                origin.clone(),
            )
            .ok()?;
        let [EntityId::Value(value)] = results.as_slice() else {
            return None;
        };
        Some(*value)
    };
    let initial = literal(function, 0)?;
    let (_, places) = function
        .append_instruction(
            block,
            Operation::RootPlace { owner: initial },
            vec![EntityType::Place(string)],
            Origin::Source(call),
        )
        .ok()?;
    let [EntityId::Place(place)] = places.as_slice() else {
        return None;
    };
    let (_, loans) = function
        .append_instruction(
            block,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: string,
            }],
            Origin::Source(call),
        )
        .ok()?;
    let [EntityId::Loan(loan)] = loans.as_slice() else {
        return None;
    };
    let next = literal(function, 1)?;
    let (_, results) = function
        .append_instruction(
            block,
            Operation::RootReplace {
                owner: initial,
                loan: *loan,
                replacement: next,
            },
            vec![EntityType::Value(string), EntityType::Value(string)],
            Origin::Source(call),
        )
        .ok()?;
    let [EntityId::Value(new_root), EntityId::Value(old_result)] = results.as_slice() else {
        return None;
    };
    // Actual CallReturn(root) precedes AfterStatement(old binding), not origin-span sorting.
    for (owner, fact) in [
        (*new_root, incoming.cleanup()),
        (*old_result, old.cleanup()),
    ] {
        function
            .append_instruction(
                block,
                Operation::Drop { owner },
                Vec::new(),
                Origin::Source(fact.value_origin()),
            )
            .ok()?;
    }
    function
        .set_terminator(
            block,
            TerminatorKind::Return { values: Vec::new() },
            Origin::Source(callable),
        )
        .ok()?;
    verify_program(&program).ok()?;
    replay(&program, bound, unit)?;
    Some(program)
}

fn replay(
    program: &Program,
    bound: &BoundCatalog,
    unit: &OwnedCompilationUnitView<'_, '_>,
) -> Option<()> {
    if !bound.matches(unit) {
        return None;
    }
    cleanup_points(bound, unit)?;
    let [module] = program.modules.as_slice() else {
        return None;
    };
    let [function] = module.functions.as_slice() else {
        return None;
    };
    let [block] = function.blocks.as_slice() else {
        return None;
    };
    if !block.parameters.is_empty()
        || function.entry_block()? != block.id
        || !matches!(&block.terminator.as_ref()?.kind, TerminatorKind::Return { values } if values.is_empty())
    {
        return None;
    }
    let [first, place, borrow, next, replace, drop_new, drop_old] =
        function.instructions.as_slice()
    else {
        return None;
    };
    if block.instructions
        != function
            .instructions
            .iter()
            .map(|i| i.id)
            .collect::<Vec<_>>()
        || function.instructions.iter().any(|i| i.block != block.id)
    {
        return None;
    }
    let [old, incoming] = bound.producers() else {
        return None;
    };
    let [input] = unit.inputs() else { return None };
    let parsed = input.parsed();
    let callable_origin = Origin::Source(
        parsed
            .ast()
            .items()
            .get(bound.callable().item())
            .ok()?
            .span(),
    );
    if function.origin != callable_origin
        || block.origin != callable_origin
        || block.terminator.as_ref()?.origin != callable_origin
        || !function.return_types.is_empty()
        || function.never_returns
        || function.borrow_return.is_some()
        || function.carrier_return.is_some()
        || function.receiver.is_some()
    {
        return None;
    }

    let text = unit.sources().source_text(input.source_id()).ok()?;
    for (instruction, producer) in [(first, old), (next, incoming)] {
        let expression = producer.expression();
        let span = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .ok()?
            .span();
        let Operation::StringLiteral { bytes, .. } = &instruction.operation else {
            return None;
        };
        if instruction.origin != Origin::Source(span)
            || *bytes
                != super::lowering_support::string_literal::decode_plain(
                    parsed,
                    text,
                    expression.expression(),
                )
                .ok()??
        {
            return None;
        }
    }
    let [EntityId::Value(initial)] = first.results.as_slice() else {
        return None;
    };
    let [EntityId::Value(new)] = next.results.as_slice() else {
        return None;
    };
    let [EntityId::Place(p)] = place.results.as_slice() else {
        return None;
    };
    let [EntityId::Loan(l)] = borrow.results.as_slice() else {
        return None;
    };
    let [EntityId::Value(new_root), EntityId::Value(detached)] = replace.results.as_slice() else {
        return None;
    };
    let relation = bound.replacement();
    let call = parsed
        .ast()
        .expressions()
        .get(relation.call().expression())
        .ok()?
        .span();
    if relation.previous() != old.identity()
        || relation.incoming() != incoming.identity()
        || place.operation != (Operation::RootPlace { owner: *initial })
        || borrow.operation
            != (Operation::BorrowBegin {
                place: *p,
                kind: LoanKind::Exclusive,
            })
        || replace.operation
            != (Operation::RootReplace {
                owner: *initial,
                loan: *l,
                replacement: *new,
            })
        || [place, borrow, replace]
            .iter()
            .any(|i| i.origin != Origin::Source(call))
        || drop_new.operation != (Operation::Drop { owner: *new_root })
        || drop_old.operation != (Operation::Drop { owner: *detached })
        || drop_new.origin != Origin::Source(incoming.cleanup().value_origin())
        || drop_old.origin != Origin::Source(old.cleanup().value_origin())
        || !drop_new.results.is_empty()
        || !drop_old.results.is_empty()
        || incoming.cleanup().point() != UnitDropPoint::CallReturn(relation.call())
        || !matches!(old.cleanup().point(), UnitDropPoint::AfterStatement(_))
    {
        return None;
    }
    verify_program(program).ok()?;
    Some(())
}

#[test]
fn original_owned_catalog_emits_and_replays_actual_replace_and_cleanup() {
    use super::unit_lower_test_support::parsed;
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{d10_catalog_experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    for text in [
        "fun run() { var a = \"first\"; val old = replace(&a, \"next\") }",
        "fun run() { var a = \"甲\\n\"; val old = replace(&a, \"乙\") }",
    ] {
        let mut sources = SourceMap::new();
        let (source, ast) = parsed(&mut sources, "catalog.ko", text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "catalog.ko", source, &ast),
            SourceUnitInput::memory_document(source, &ast),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let bound = d10_catalog_experiment::analyze(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .unwrap();
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            let program = emit(&bound, &unit).unwrap();
            assert!(replay(&program, &bound, &unit).is_some());
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}
