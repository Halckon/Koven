//! Observation of the distinct child-then-parent shape; not a sealed candidate admission.
use super::*;
#[test]
fn actual_child_then_parent_edges_preserve_old_generation() {
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::standard_environments,
    };
    let text = "fun run(count: Int, childCount: Int) { var a = \"first\"; var b = \"kept\"; var outer = count; while(outer > 0) { var inner = childCount; while(inner > 0) { val old = replace(&a, \"next\"); inner -= 1 }; val parentOld = replace(&a, \"parent\"); outer -= 1 }; println(a); println(b) }";
    let mut sources = SourceMap::new();
    let (source, ast) = unit_lower_test_support::parsed(&mut sources, "after.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "after.ko", source, &ast),
        SourceUnitInput::memory_document(source, &ast),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        assert!(
            experiment::analyze_dual_nested_loop(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .is_none()
        );
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (program, function) = unit_lower::lower_owned_unit_with_entry(&unit, entry).unwrap();
        verify_generation_edges(&unit, &program, function);
        crate::llvm::render_verified_program(&program).expect("actual child-then-parent LLVM");
    }
}

// Narrow structural witness only. It does not publish a new bound or claim full source admission.
fn verify_generation_edges(
    unit: &lang_frontend::ownership_checking::OwnedCompilationUnitView<'_, '_>,
    program: &model::Program,
    function: model::FunctionId,
) {
    use model::*;
    verify::verify_program(program).unwrap();
    let [module] = program.modules.as_slice() else {
        panic!("one module")
    };
    let [f] = module.functions.as_slice() else {
        panic!("one function")
    };
    assert_eq!(f.id(), function);
    assert_eq!((f.blocks.len(), f.instructions.len()), (11, 32));
    let blocks = &f.blocks;
    let ins = &f.instructions;
    let membership: [&[usize]; 11] = [
        &[0, 1, 2],
        &[3, 4],
        &[5],
        &[6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        &[16, 17],
        &[18, 19, 20, 21, 22, 23, 24],
        &[25, 26, 27, 28, 29, 30, 31],
        &[],
        &[],
        &[],
        &[],
    ];
    for (block, indices) in blocks.iter().zip(membership) {
        assert_eq!(
            block.instructions,
            indices.iter().map(|n| ins[*n].id).collect::<Vec<_>>()
        );
        for n in indices {
            assert_eq!(ins[*n].block, block.id);
        }
    }

    let value = |n: usize, result: usize| match ins[n].results[result] {
        EntityId::Value(v) => v,
        _ => panic!("actual value result"),
    };
    let parameter = |b: usize, n: usize| match blocks[b].parameters[n] {
        EntityId::Value(v) => v,
        _ => panic!("actual value parameter"),
    };
    let term = |b: usize| &blocks[b].terminator.as_ref().unwrap().kind;
    let edge = |b: usize, args: Vec<EntityId>| Edge {
        target: blocks[b].id,
        arguments: args,
    };
    let v = EntityId::Value;
    let mut initial = vec![blocks[0].parameters[1]];
    initial.extend([v(value(0, 0)), v(value(1, 0)), v(value(2, 0))]);
    assert_eq!(term(0), &TerminatorKind::Branch(edge(1, initial)));
    assert_eq!(
        term(1),
        &TerminatorKind::Conditional {
            condition: value(4, 0),
            when_true: edge(2, blocks[1].parameters.clone()),
            when_false: edge(3, vec![v(parameter(1, 1)), v(parameter(1, 2))]),
        }
    );
    let mut into_child = blocks[2].parameters.clone();
    into_child.push(v(value(5, 0)));
    assert_eq!(term(2), &TerminatorKind::Branch(edge(4, into_child)));
    // The child false edge carries the header's current owner, not the static producer site.
    assert_eq!(
        term(4),
        &TerminatorKind::Conditional {
            condition: value(17, 0),
            when_true: edge(5, blocks[4].parameters.clone()),
            when_false: edge(6, blocks[4].parameters[..4].to_vec()),
        }
    );
    let child_updated = vec![
        blocks[5].parameters[0],
        v(value(21, 0)),
        v(parameter(5, 2)),
        v(parameter(5, 3)),
        v(value(24, 0)),
    ];
    assert_eq!(
        term(5),
        &TerminatorKind::Conditional {
            condition: value(24, 1),
            when_true: edge(7, child_updated.clone()),
            when_false: edge(8, child_updated),
        }
    );
    assert_eq!(
        term(8),
        &TerminatorKind::Branch(edge(4, blocks[8].parameters.clone()))
    );
    let parent_updated = vec![
        blocks[6].parameters[0],
        v(value(28, 0)),
        v(parameter(6, 2)),
        v(value(31, 0)),
    ];
    assert_eq!(
        term(6),
        &TerminatorKind::Conditional {
            condition: value(31, 1),
            when_true: edge(9, parent_updated.clone()),
            when_false: edge(10, parent_updated),
        }
    );
    assert_eq!(
        term(10),
        &TerminatorKind::Branch(edge(1, blocks[10].parameters.clone()))
    );
    assert_eq!(term(3), &TerminatorKind::Return { values: vec![] });
    for n in [7, 9] {
        assert_eq!(term(n), &TerminatorKind::Abort);
        assert!(blocks[n].instructions.is_empty());
        assert!(blocks[n].terminator.as_ref().unwrap().abort.is_some());
    }
    let plans = unit.ownership().ownership_primitives();
    assert_eq!(plans.len(), 2);
    let drops = unit.ownership().drops();
    assert_eq!(drops.len(), 4);
    for (replace, literal, drop, block, fact, text) in
        [(21, 20, 22, 5, 0, "next"), (28, 27, 29, 6, 1, "parent")]
    {
        let Operation::RootReplace {
            owner,
            loan,
            replacement,
        } = ins[replace].operation
        else {
            panic!("actual Replace")
        };
        assert_eq!(owner, parameter(block, 1));
        assert_eq!(replacement, value(literal, 0));
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Exclusive,
        } = ins[replace - 2].operation
        else {
            panic!("actual exclusive loan")
        };
        assert_eq!(ins[replace - 2].results, vec![EntityId::Loan(loan)]);
        assert_eq!(ins[replace - 3].operation, Operation::RootPlace { owner });
        assert_eq!(ins[replace - 3].results, vec![EntityId::Place(place)]);
        assert!(
            matches!(&ins[literal].operation,Operation::StringLiteral{bytes,..} if bytes==text.as_bytes())
        );
        assert_eq!(
            ins[drop].operation,
            Operation::Drop {
                owner: value(replace, 1)
            }
        );
        assert_eq!(ins[drop].origin, Origin::Source(drops[fact].value_origin()));
        let call = plans[fact].descriptor().expression();
        let [input] = unit.inputs() else {
            panic!("one input")
        };
        assert_eq!(
            ins[replace].origin,
            Origin::Source(
                input
                    .parsed()
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span()
            )
        );
    }
    // Both final roots are the parent-exit parameters, after their actual shared uses.
    for (drop, slot, fact) in [(10, 0, 2), (15, 1, 3)] {
        let owner = parameter(3, slot);
        assert_eq!(ins[drop - 4].operation, Operation::RootPlace { owner });
        let [EntityId::Place(place)] = ins[drop - 4].results.as_slice() else {
            panic!("actual final place")
        };
        assert_eq!(
            ins[drop - 3].operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Loan(loan)] = ins[drop - 3].results.as_slice() else {
            panic!("actual final loan")
        };
        assert_eq!(
            ins[drop - 2].operation,
            Operation::PrintString { value: *loan }
        );
        assert_eq!(
            ins[drop - 1].operation,
            Operation::BorrowEnd { loan: *loan }
        );
        assert_eq!(ins[drop].block, blocks[3].id);
        assert_eq!(
            ins[drop].operation,
            Operation::Drop {
                owner: parameter(3, slot)
            }
        );
        assert_eq!(ins[drop].origin, Origin::Source(drops[fact].value_origin()));
    }
}
