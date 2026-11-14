//! Final-owned loop observation must not silently bypass ordinary admission.
use super::*;
#[test]
fn bound_loop_replays_actual_replace_cfg() {
    use super::unit_lower_test_support::parsed;
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{d10_catalog_experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let base = "fun run(count: Int) { var a = \"first\"; var n = count; while(n > 0) { val old = replace(&a, \"next\"); n -= 1 } }";
    for (text, accepted) in [
        (base.to_owned(), true),
        (
            base.replace("count", "limit")
                .replace("first", "甲")
                .replace("next", "乙"),
            true,
        ),
        (base.replace("n -= 1", "n += 1"), false),
        (base.replace("n -= 1", "n -= 2"), false),
        (base.replace("n -= 1", "n -= 1; break"), false),
    ] {
        let mut sources = SourceMap::new();
        let (source, ast) = parsed(&mut sources, "loop-bound.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-bound.ko", source, &ast),
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
            let bound =
                d10_catalog_experiment::analyze_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap();
            if !accepted {
                assert!(bound.is_none(), "{text}");
                continue;
            }
            let bound = bound.unwrap();
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            assert!(bound.matches(&unit));
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let (program, function) =
                unit_lower::lower_owned_unit_with_entry(&unit, entry).unwrap();
            replay(&bound, &unit, &program, function).expect("same-run actual loop CFG replay");
        }
    }
}
use super::model::*;
use lang_frontend::ownership_checking::{
    OwnedCompilationUnitView, d10_catalog_experiment::BoundLoop,
};

// Reads only actual ordinary-route SSA. No receipt, new admission, or dynamic token synthesis.
fn replay(
    bound: &BoundLoop,
    unit: &OwnedCompilationUnitView<'_, '_>,
    program: &Program,
    id: FunctionId,
) -> Option<()> {
    if !bound.matches(unit) || program.modules.len() != 1 {
        return None;
    }
    verify::verify_program(program).ok()?;
    let module = program.modules.first()?;
    if module.functions.len() != 1 {
        return None;
    }
    let f = module.function(id)?;
    if f.blocks.len() != 6
        || f.instructions.len() != 12
        || !f.return_types.is_empty()
        || f.receiver.is_some()
        || f.never_returns
        || f.borrow_return.is_some()
        || f.carrier_return.is_some()
    {
        return None;
    }
    let [input] = unit.inputs() else { return None };
    let ast = input.parsed().ast();
    let origin = |e: lang_frontend::type_checking::UnitExpressionId| {
        ast.expressions()
            .get(e.expression())
            .ok()
            .map(|n| Origin::Source(n.span()))
    };
    let call = origin(bound.replacement())?;
    let (condition, step) = bound.counter_expressions();
    let control = Origin::Source(
        ast.statements()
            .get(bound.control().1.statement())
            .ok()?
            .span(),
    );
    let callable = Origin::Source(ast.items().get(bound.control().0.item()).ok()?.span());
    if f.origin != callable || f.blocks.iter().any(|b| b.origin != callable) {
        return None;
    }
    use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};
    let [old, current] = bound.cleanup();
    let UnitDropPoint::AfterStatement(old_statement) = old.point() else {
        return None;
    };
    let UnitDropTarget::Named(old_root) = old.target() else {
        return None;
    };
    if current.point() != UnitDropPoint::LoopExit(bound.control().1)
        || current.target() != UnitDropTarget::Named(bound.roots().0)
        || old_root == bound.roots().0
    {
        return None;
    }
    let lang_frontend::parser::Statement::LocalVariable { declaration } = ast
        .statements()
        .get(old_statement.statement())
        .ok()?
        .payload()
    else {
        return None;
    };
    let lang_frontend::parser::Item::Variable { initializer, .. } =
        ast.items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    if *initializer != bound.replacement().expression() {
        return None;
    }
    let i = &f.instructions;
    let value = |n: usize, r: usize| match i.get(n)?.results.get(r)? {
        EntityId::Value(v) => Some(*v),
        _ => None,
    };
    let entry = f.block(f.entry_block()?)?;
    let header = f.block(i[2].block)?;
    let body = f.block(i[4].block)?;
    let finished = f.block(i[11].block)?;
    let [EntityId::Loan(parameter)] = entry.parameters.as_slice() else {
        return None;
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    } = f.entity(EntityId::Loan(*parameter))?.ty
    else {
        return None;
    };
    if module.types.get(int.index())?
        != &(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        })
    {
        return None;
    }
    let [EntityId::Value(hr), EntityId::Value(hc)] = header.parameters.as_slice() else {
        return None;
    };
    let [EntityId::Value(br), EntityId::Value(bc)] = body.parameters.as_slice() else {
        return None;
    };
    let [EntityId::Value(fr)] = finished.parameters.as_slice() else {
        return None;
    };
    if f.entity(EntityId::Value(*hc))?.ty != EntityType::Value(int)
        || f.entity(EntityId::Value(*bc))?.ty != EntityType::Value(int)
    {
        return None;
    }
    let [EntityId::Place(place)] = i[4].results.as_slice() else {
        return None;
    };
    let [EntityId::Loan(loan)] = i[5].results.as_slice() else {
        return None;
    };
    let (initial, incoming) = bound.producer_sites();
    let literal = |n: usize, site| -> Option<()> {
        let bytes = super::lowering_support::string_literal::decode_plain(
            input.parsed(),
            unit.sources().source_text(input.source_id()).ok()?,
            site,
        )
        .ok()??;
        let Operation::StringLiteral {
            string,
            bytes: actual,
        } = &i[n].operation
        else {
            return None;
        };
        if *actual != bytes || module.types.get(string.index())? != &SsaTypeKind::StringOwner {
            return None;
        }
        Some(())
    };
    literal(0, initial.expression())?;
    literal(6, incoming.expression())?;
    let expected = [
        (
            1,
            Operation::Read {
                source: PlaceAccess::Loan(*parameter),
            },
        ),
        (2, Operation::Constant(ScalarConstant::Integer(0))),
        (
            3,
            Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                left: *hc,
                right: value(2, 0)?,
            },
        ),
        (4, Operation::RootPlace { owner: *br }),
        (
            5,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Exclusive,
            },
        ),
        (
            7,
            Operation::RootReplace {
                owner: *br,
                loan: *loan,
                replacement: value(6, 0)?,
            },
        ),
        (
            8,
            Operation::Drop {
                owner: value(7, 1)?,
            },
        ),
        (9, Operation::Constant(ScalarConstant::Integer(1))),
        (
            10,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                left: *bc,
                right: value(9, 0)?,
            },
        ),
        (11, Operation::Drop { owner: *fr }),
    ];
    if expected.iter().any(|(n, op)| i[*n].operation != *op) {
        return None;
    }
    for (n, expected) in [
        (0, origin(initial)?),
        (3, origin(condition)?),
        (6, origin(incoming)?),
        (7, call),
        (10, origin(step)?),
        (8, Origin::Source(bound.cleanup()[0].value_origin())),
        (11, Origin::Source(bound.cleanup()[1].value_origin())),
    ] {
        if i[n].origin != expected {
            return None;
        }
    }
    let edge = |target, arguments| Edge { target, arguments };
    fn term(b: &Block) -> Option<&TerminatorKind> {
        b.terminator.as_ref().map(|t| &t.kind)
    }
    if term(entry)?
        != &TerminatorKind::Branch(edge(
            header.id,
            vec![EntityId::Value(value(0, 0)?), EntityId::Value(value(1, 0)?)],
        ))
        || term(header)?
            != &(TerminatorKind::Conditional {
                condition: value(3, 0)?,
                when_true: edge(body.id, vec![EntityId::Value(*hr), EntityId::Value(*hc)]),
                when_false: edge(finished.id, vec![EntityId::Value(*hr)]),
            })
        || term(finished)? != &(TerminatorKind::Return { values: vec![] })
    {
        return None;
    }
    let TerminatorKind::Conditional {
        condition: failure,
        when_true,
        when_false,
    } = term(body)?
    else {
        return None;
    };
    if *failure != value(10, 1)?
        || when_true.arguments
            != vec![
                EntityId::Value(value(7, 0)?),
                EntityId::Value(value(10, 0)?),
            ]
        || when_false.arguments != when_true.arguments
    {
        return None;
    }
    let abort = f.block(when_true.target)?;
    let resume = f.block(when_false.target)?;
    if abort.terminator.as_ref()?.abort
        != Some(AbortReason::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Subtract,
            divisor: value(9, 0)?,
        })
        || f.blocks
            .iter()
            .any(|b| b.id != abort.id && b.terminator.as_ref().is_some_and(|t| t.abort.is_some()))
    {
        return None;
    }
    if i[4].origin != Origin::Source(bound.loan().begin_span())
        || i[5].origin != i[4].origin
        || finished.terminator.as_ref()?.origin != callable
    {
        return None;
    }
    if term(abort)? != &TerminatorKind::Abort
        || term(resume)? != &TerminatorKind::Branch(edge(header.id, resume.parameters.clone()))
    {
        return None;
    }
    if std::collections::BTreeSet::from([
        entry.id,
        header.id,
        body.id,
        finished.id,
        abort.id,
        resume.id,
    ])
    .len()
        != 6
    {
        return None;
    }
    for (block, indices) in [
        (entry, vec![0, 1]),
        (header, vec![2, 3]),
        (body, (4..11).collect()),
        (finished, vec![11]),
        (abort, vec![]),
        (resume, vec![]),
    ] {
        if block.instructions != indices.iter().map(|n| i[*n].id).collect::<Vec<_>>()
            || indices.iter().any(|n| i[*n].block != block.id)
        {
            return None;
        }
    }
    if entry.terminator.as_ref()?.origin != control
        || resume.terminator.as_ref()?.origin != control
        || header.terminator.as_ref()?.origin != origin(condition)?
        || body.terminator.as_ref()?.origin != origin(step)?
        || abort.terminator.as_ref()?.origin != origin(step)?
    {
        return None;
    }
    Some(())
}
