//! Separate sealed child-then-parent source/fact handoff to actual ordinary Unit SSA.
use super::*;
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{
        OwnedCompilationUnitView, d10_catalog_experiment as experiment, owned_compilation_unit_view,
    },
    source::SourceMap,
    type_checking::standard_environments,
};
#[test]
fn sealed_after_child_handoff_replays_actual_operations_and_edges() {
    for (text, accepted) in [
        (AFTER_SOURCE.to_owned(), true),
        (super::super::renamed_source(AFTER_SOURCE), true),
        (
            AFTER_SOURCE
                .replace("first", "初始")
                .replace("next", "子")
                .replace("parent\"", "父\"")
                .replace("kept", "保留"),
            true,
        ),
        (
            AFTER_SOURCE.replace("replace(&a, \"parent\")", "replace(&b, \"parent\")"),
            false,
        ),
        (
            AFTER_SOURCE.replace("replace(&a, \"next\")", "replace(&b, \"next\")"),
            false,
        ),
        (
            AFTER_SOURCE.replace("var inner = childCount", "var inner = count"),
            false,
        ),
        (
            AFTER_SOURCE.replace("println(a); println(b)", "println(b)"),
            false,
        ),
        (
            AFTER_SOURCE.replace("println(a); println(b)", "println(b); println(a)"),
            false,
        ),
        (AFTER_SOURCE.replace("outer -= 1", "outer -= 2"), false),
    ] {
        let mut sources = SourceMap::new();
        let (source, ast) = unit_lower_test_support::parsed(&mut sources, "after-bound.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "after-bound.ko", source, &ast),
            SourceUnitInput::memory_document(source, &ast),
        ] {
            let inputs = [input];
            let (names, typed, _) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let bound = experiment::analyze_after_child_nested_loop(
                &sources, &inputs, &names, &types, &typed,
            )
            .unwrap();
            assert_eq!(bound.is_some(), accepted, "{text}");
            let Some(bound) = bound else { continue };
            assert!(
                experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .is_none()
            );
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            let callable = bound.controls().0;
            let entries = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| {
                    d.source_unit() == callable.source_unit() && d.root() == callable.item()
                })
                .collect::<Vec<_>>();
            let [entry] = entries.as_slice() else {
                panic!("one bound entry")
            };
            let (program, function) =
                unit_lower::lower_owned_unit_with_entry(&unit, entry.id()).unwrap();
            replay(&unit, &bound, &program, function);
            super::super::path::check_after_child(
                &program,
                function,
                &literal_bytes(&unit, bound.producer_sites()[0]),
                &literal_bytes(&unit, bound.producer_sites()[2]),
                &literal_bytes(&unit, bound.retained().source().1),
            );
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

fn replay(
    unit: &OwnedCompilationUnitView<'_, '_>,
    bound: &experiment::BoundAfterChildNestedLoop,
    program: &Program,
    function: FunctionId,
) {
    assert!(bound.matches(unit));
    assert_eq!(bound.order(), experiment::ReplacementOrder::ChildThenParent);
    verify_generation_edges(unit, program, function);
    let [module] = program.modules.as_slice() else {
        panic!("one module")
    };
    let [f] = module.functions.as_slice() else {
        panic!("one function")
    };
    let [input] = unit.inputs() else {
        panic!("one source")
    };
    let ast = input.parsed().ast();
    let [names] = unit.names().names().source_units() else {
        panic!("one source identity")
    };
    let source = names.source_unit();
    assert_eq!(bound.controls().0.source_unit(), source);
    let origin = |id: lang_frontend::type_checking::UnitExpressionId| {
        assert_eq!(id.source_unit(), source);
        Origin::Source(ast.expressions().get(id.expression()).unwrap().span())
    };
    let callable = Origin::Source(ast.items().get(bound.controls().0.item()).unwrap().span());
    assert_eq!(f.origin, callable);
    assert!(f.blocks.iter().all(|b| b.origin == callable));
    assert!(
        f.return_types.is_empty()
            && !f.never_returns
            && f.borrow_return.is_none()
            && f.carrier_return.is_none()
            && f.receiver.is_none()
    );
    let i = &f.instructions;
    let v = |n: usize, r: usize| match i[n].results[r] {
        EntityId::Value(v) => v,
        _ => panic!("value"),
    };
    let p = |b: usize, n: usize| match f.blocks[b].parameters[n] {
        EntityId::Value(v) => v,
        _ => panic!("parameter"),
    };
    let [EntityId::Loan(parent), EntityId::Loan(child)] = f.blocks[0].parameters.as_slice() else {
        panic!("two parameter loans")
    };
    assert_ne!(parent, child);
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    } = f.entity(EntityId::Loan(*parent)).unwrap().ty
    else {
        panic!("Shared Int")
    };
    assert_eq!(
        module.types[int.index()],
        SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }
    );
    assert_eq!(
        f.entity(EntityId::Loan(*child)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: int
        }
    );
    assert_eq!(
        i[2].operation,
        Operation::Read {
            source: PlaceAccess::Loan(*parent)
        }
    );
    let EntityId::Loan(child_carried) = f.blocks[2].parameters[0] else {
        panic!("child parameter transport")
    };
    assert_eq!(
        i[5].operation,
        Operation::Read {
            source: PlaceAccess::Loan(child_carried)
        }
    );
    for (n, id) in [2, 5].into_iter().zip(bound.counter_initializers()) {
        assert_eq!(i[n].origin, origin(id));
    }
    for (n, site) in [
        (0, bound.producer_sites()[0]),
        (1, bound.retained().source().1),
        (20, bound.producer_sites()[1]),
        (27, bound.producer_sites()[2]),
    ] {
        let Operation::StringLiteral { string, bytes } = &i[n].operation else {
            panic!("actual literal")
        };
        assert_eq!(module.types[string.index()], SsaTypeKind::StringOwner);
        assert_eq!(*bytes, literal_bytes(unit, site));
        assert_eq!(i[n].origin, origin(site));
    }
    for ((condition, step), zero, comparison, one, subtract, b, slot) in [
        (bound.counter_expressions()[0], 3, 4, 30, 31, 6, 3),
        (bound.counter_expressions()[1], 16, 17, 23, 24, 5, 4),
    ] {
        let header = if zero == 3 { 1 } else { 4 };
        assert_eq!(
            i[zero].operation,
            Operation::Constant(ScalarConstant::Integer(0))
        );
        assert_eq!(
            i[comparison].operation,
            Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                left: p(header, slot),
                right: v(zero, 0)
            }
        );
        assert_eq!(
            i[one].operation,
            Operation::Constant(ScalarConstant::Integer(1))
        );
        assert_eq!(
            i[subtract].operation,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                left: p(b, slot),
                right: v(one, 0)
            }
        );
        assert_eq!(i[comparison].origin, origin(condition));
        assert_eq!(i[subtract].origin, origin(step));
        let lang_frontend::parser::Expression::Binary { right, .. } = ast
            .expressions()
            .get(condition.expression())
            .unwrap()
            .payload()
        else {
            panic!("condition AST")
        };
        let lang_frontend::parser::Expression::Assignment { value, .. } =
            ast.expressions().get(step.expression()).unwrap().payload()
        else {
            panic!("step AST")
        };
        assert_eq!(
            i[zero].origin,
            Origin::Source(ast.expressions().get(*right).unwrap().span())
        );
        assert_eq!(
            i[one].origin,
            Origin::Source(ast.expressions().get(*value).unwrap().span())
        );
    }
    let mut drops = bound.cleanup().to_vec();
    drops.push(bound.retained().cleanup());
    assert_eq!(unit.ownership().drops(), drops);
    let mut loans = bound.loans().to_vec();
    loans.push(bound.final_root_use().loan().clone());
    loans.push(bound.retained().loan().clone());
    assert_eq!(unit.ownership().loans(), loans);
    assert_eq!(
        unit.ownership()
            .ownership_primitives()
            .iter()
            .map(|p| p.descriptor().expression())
            .collect::<Vec<_>>(),
        bound.replacements()
    );
    for (n, call) in [21, 28].into_iter().zip(bound.replacements()) {
        assert_eq!(i[n].origin, origin(call));
    }
    for (n, loan) in [
        (18, &bound.loans()[0]),
        (25, &bound.loans()[1]),
        (6, bound.final_root_use().loan()),
        (11, bound.retained().loan()),
    ] {
        assert_eq!(i[n].origin, Origin::Source(loan.begin_span()));
        assert_eq!(i[n + 1].origin, Origin::Source(loan.begin_span()));
    }
    for (n, use_) in [(8, bound.final_root_use()), (13, bound.retained())] {
        assert_eq!(i[n].origin, origin(use_.call()));
        assert_eq!(i[n + 1].origin, origin(use_.call()));
    }
    let control_origin = |id: lang_frontend::type_checking::UnitStatementId| {
        assert_eq!(id.source_unit(), source);
        Origin::Source(ast.statements().get(id.statement()).unwrap().span())
    };
    let (_, parent_control, child_control) = bound.controls();
    let [
        (parent_condition, parent_step),
        (child_condition, child_step),
    ] = bound.counter_expressions();
    for (b, expected) in [
        (0, control_origin(parent_control)),
        (1, origin(parent_condition)),
        (2, control_origin(child_control)),
        (3, callable),
        (4, origin(child_condition)),
        (5, origin(child_step)),
        (6, origin(parent_step)),
        (7, origin(child_step)),
        (8, control_origin(child_control)),
        (9, origin(parent_step)),
        (10, control_origin(parent_control)),
    ] {
        let term = f.blocks[b].terminator.as_ref().unwrap();
        assert_eq!(term.origin, expected);
        if ![7, 9].contains(&b) {
            assert!(term.abort.is_none());
        }
    }
    for (b, one) in [(7, 23), (9, 30)] {
        assert_eq!(
            f.blocks[b].terminator.as_ref().unwrap().abort,
            Some(AbortReason::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                divisor: v(one, 0)
            })
        );
    }
}
