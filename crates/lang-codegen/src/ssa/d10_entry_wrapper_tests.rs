//! Ordinary source-call composition only; not a new sealed D10 or native-entry capability.
use super::{model::Operation, unit_lower, unit_lower_test_support, verify::verify_program};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{d10_catalog_experiment as experiment, owned_compilation_unit_view},
    source::SourceMap,
    type_checking::standard_environments,
};

#[test]
fn ordinary_source_entry_wrappers_lower_both_replacement_orders() {
    let child = r#"var inner = childCount;
        while (inner > 0) { val old = replace(&a, "next"); inner -= 1 };"#;
    let parent = r#"val parentOld = replace(&a, "parent");"#;
    let mut failures = Vec::new();
    let mut verified = 0;
    for after_child in [false, true] {
        let body = if after_child {
            format!("{child} {parent}")
        } else {
            format!("{parent} {child}")
        };
        for (outer, inner) in [(0, 0), (1, 0), (1, 2)] {
            let text = format!(
                r#"fun run(count: Int, childCount: Int) {{
                    var a = "first"; var b = "kept"; var outer = count;
                    while (outer > 0) {{ {body} outer -= 1 }};
                    println(a); println(b)
                }}
                fun entry() {{ run({outer}, {inner}) }}"#
            );
            let mut sources = SourceMap::new();
            let (source, ast) =
                unit_lower_test_support::parsed(&mut sources, "entry-wrapper.ko", &text);
            let (environment, types) = standard_environments();
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "entry-wrapper.ko", source, &ast),
                ),
                ("memory", SourceUnitInput::memory_document(source, &ast)),
            ] {
                let inputs = [input];
                let (names, typed, owned) =
                    unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
                assert!(experiment::analyze_dual_nested_loop(
                    &sources, &inputs, &names, &types, &typed,
                ).expect("existing before-child analysis").is_none());
                assert!(
                    experiment::analyze_after_child_nested_loop(
                        &sources, &inputs, &names, &types, &typed,
                    )
                    .expect("existing after-child analysis")
                    .is_none()
                );
                let unit =
                    owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
                        .expect("ordinary validated view");
                let entries: Vec<_> = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|declaration| declaration.name() == "entry")
                    .collect();
                assert_eq!(entries.len(), 1);
                let result = unit_lower::lower_owned_unit_with_entry(&unit, entries[0].id());
                let (program, entry) = match result {
                    Ok(result) => result,
                    Err(error) => {
                        failures.push(format!(
                            "after_child={after_child}, arguments=({outer},{inner}), {layout}: {error:?}, exact span text={:?}",
                            error.span.and_then(|span| sources.slice(span).ok()),
                        ));
                        continue;
                    }
                };
                verify_program(&program).expect("actual wrapper graph verifies");
                let module = program.module(entry.module()).expect("actual module");
                let function = module.function(entry).expect("actual entry");
                assert_eq!(
                    function.origin,
                    super::model::Origin::Source(
                        ast.ast().items().get(entries[0].root()).unwrap().span()
                    )
                );
                assert!(function.return_types.is_empty());
                let calls: Vec<_> = function
                    .instructions
                    .iter()
                    .filter_map(|instruction| match &instruction.operation {
                        Operation::DirectCall {
                            callee,
                            receiver,
                            arguments,
                        } => {
                            assert!(receiver.is_none());
                            assert_eq!(arguments.len(), 2);
                            assert_eq!(
                                sources.slice(instruction.origin.span()).unwrap(),
                                format!("run({outer}, {inner})")
                            );
                            let graph = super::d10_after_child_graph::support::Graph {
                                f: function,
                                module,
                                used: Default::default(),
                            };
                            let callee_function = module.function(*callee).unwrap();
                            let parameters = &callee_function
                                .block(callee_function.entry_block().unwrap())
                                .unwrap()
                                .parameters;
                            assert_eq!(parameters.len(), 2);
                            for (argument, parameter) in arguments.iter().zip(parameters) {
                                assert!(graph.shared_int(*argument).is_some());
                                assert_eq!(
                                    function.entity(*argument).unwrap().ty,
                                    callee_function.entity(*parameter).unwrap().ty
                                );
                            }
                            check_argument_transport(
                                &unit,
                                &ast,
                                function,
                                instruction,
                                arguments,
                                [outer, inner],
                            );
                            Some(*callee)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(calls.len(), 1, "actual wrapper calls exactly one callable");
                let declarations: Vec<_> = names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|declaration| declaration.name() == "run")
                    .collect();
                assert_eq!(declarations.len(), 1);
                let callee_origin = super::model::Origin::Source(
                    ast.ast()
                        .items()
                        .get(declarations[0].root())
                        .unwrap()
                        .span(),
                );
                assert_eq!(
                    module.function(calls[0]).expect("actual callee").origin,
                    callee_origin
                );
                assert_eq!(
                    module
                        .functions
                        .iter()
                        .filter(|f| f.origin == callee_origin)
                        .count(),
                    1
                );
                crate::llvm::render_verified_program(&program).expect("actual LLVM verifies");
                verified += 1;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        crate::test_support::diagnostics::failure(format_args!(
            "ordinary wrapper lowering blockers: {failures:#?}"
        ))
    );
    assert_eq!(verified, 12);
}

fn check_argument_transport(
    unit: &lang_frontend::ownership_checking::OwnedCompilationUnitView<'_, '_>,
    parsed: &lang_frontend::parser::ParsedFile,
    function: &super::model::Function,
    call: &super::model::Instruction,
    arguments: &[super::model::EntityId],
    expected: [i32; 2],
) {
    use super::model::{EntityId, LoanKind, Origin, ScalarConstant, TerminatorKind};
    use lang_frontend::ownership_checking::UnitLoanTarget;
    let ast = parsed.ast();
    let facts: Vec<_> = unit
        .ownership()
        .loans()
        .iter()
        .filter(|fact| {
            ast.expressions()
                .get(fact.call().expression())
                .is_ok_and(|node| Origin::Source(node.span()) == call.origin)
        })
        .collect();
    assert_eq!(facts.len(), 2, "two real wrapper temporary loans");
    let [names] = unit.names().names().source_units() else {
        panic!("single source")
    };
    let source = names.source_unit();
    let call_node = ast.expressions().get(facts[0].call().expression()).unwrap();
    let lang_frontend::parser::Expression::Call {
        arguments: source_arguments,
        ..
    } = call_node.payload()
    else {
        panic!("real source call")
    };
    assert_eq!(source_arguments.len(), 2);
    let mut sequence = Vec::new();
    let mut ends = Vec::new();
    for (((argument, fact), value), source_argument) in arguments
        .iter()
        .zip(facts)
        .zip(expected)
        .zip(source_arguments)
    {
        assert_eq!(fact.argument().source_unit(), source);
        assert_eq!(fact.call().source_unit(), source);
        assert_eq!(fact.argument().expression(), source_argument.value);
        assert_eq!(
            fact.kind(),
            lang_frontend::ownership_checking::LoanKind::Shared
        );
        assert!(source_argument.mode_marker.is_none());
        let actual_ends: Vec<_> = unit
            .ownership()
            .direct_call_loan_ends()
            .iter()
            .filter(|end| end.loan() == fact)
            .collect();
        // Literal Int is outside the public planner EndCallLoan observation whitelist.
        // Actual SSA BorrowEnd below is necessary, but is not a second frontend authority.
        assert_eq!(
            actual_ends.len(),
            0,
            "literal lacks independent frontend end authority"
        );
        assert!(matches!(fact.target(), UnitLoanTarget::Temporary(id) if *id == fact.argument()));
        let argument_origin = Origin::Source(
            ast.expressions()
                .get(fact.argument().expression())
                .unwrap()
                .span(),
        );
        assert_eq!(fact.begin_span(), argument_origin.span());
        assert_eq!(fact.end_span(), call.origin.span());
        let begins: Vec<_> = function
            .instructions
            .iter()
            .filter(|i| i.results.as_slice() == [*argument])
            .collect();
        assert_eq!(begins.len(), 1);
        let begin = begins[0];
        let Operation::BorrowBegin { place, kind } = begin.operation else {
            panic!("argument loan producer")
        };
        assert_eq!(kind, LoanKind::Shared);
        assert_eq!(begin.origin, argument_origin);
        let places: Vec<_> = function
            .instructions
            .iter()
            .filter(|i| i.results.as_slice() == [EntityId::Place(place)])
            .collect();
        assert_eq!(places.len(), 1);
        let root = places[0];
        assert_eq!(root.origin, Origin::Source(source_argument.span));
        let Operation::RootPlace { owner } = root.operation else {
            panic!("temporary root")
        };
        let values: Vec<_> = function
            .instructions
            .iter()
            .filter(|i| i.results.as_slice() == [EntityId::Value(owner)])
            .collect();
        assert_eq!(values.len(), 1);
        let constant = values[0];
        assert_eq!(
            constant.operation,
            Operation::Constant(ScalarConstant::Integer(value.into()))
        );
        assert_eq!(constant.origin, argument_origin);
        let EntityId::Loan(loan) = *argument else {
            panic!("shared argument")
        };
        let retirement: Vec<_> = function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd {loan: ended} if ended == loan))
            .collect();
        assert_eq!(retirement.len(), 1);
        assert_eq!(retirement[0].origin, Origin::Source(fact.end_span()));
        assert!(retirement[0].results.is_empty());
        sequence.extend([constant.id, root.id, begin.id]);
        ends.push(retirement[0].id);
    }
    sequence.push(call.id);
    sequence.extend(ends.into_iter().rev());
    assert!(call.results.is_empty());
    assert_eq!(function.blocks.len(), 1);
    let block = function.block(function.entry_block().unwrap()).unwrap();
    assert!(block.parameters.is_empty());
    assert_eq!(block.origin, function.origin);
    assert_eq!(block.instructions, sequence);
    assert_eq!(function.instructions.len(), sequence.len());
    assert!(function.instructions.iter().all(|i| i.block == block.id));
    let terminator = block.terminator.as_ref().unwrap();
    assert_eq!(terminator.origin, function.origin);
    assert!(matches!(&terminator.kind, TerminatorKind::Return {values} if values.is_empty()));
}

#[test]
fn named_int_entry_arguments_have_real_frontend_end_facts() {
    use lang_frontend::ownership_checking::{LoanKind, UnitDropPoint, UnitLoanTarget};
    let child = r#"var inner = childCount;
        while (inner > 0) { val old = replace(&a, "next"); inner -= 1 };"#;
    let parent = r#"val parentOld = replace(&a, "parent");"#;
    let mut failures = Vec::new();
    let mut verified = 0;
    for after_child in [false, true] {
        let body = if after_child {
            format!("{child} {parent}")
        } else {
            format!("{parent} {child}")
        };
        let text = format!(
            r#"fun run(count: Int, childCount: Int) {{
            var a = "first"; var b = "kept"; var outer = count;
            while (outer > 0) {{ {body} outer -= 1 }};
            println(a); println(b)
        }}
        fun entry() {{ val p = 1; val c = 2; run(p, c) }}"#
        );
        let mut sources = SourceMap::new();
        let (source, ast) =
            unit_lower_test_support::parsed(&mut sources, "named-wrapper.ko", &text);
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "named-wrapper.ko", source, &ast),
            ),
            ("memory", SourceUnitInput::memory_document(source, &ast)),
        ] {
            let inputs = [input];
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let unit =
                owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
                    .expect("ordinary named view");
            let [source_names] = names.names().source_units() else {
                panic!("one source")
            };
            let loans: Vec<_> = unit
                .ownership()
                .loans()
                .iter()
                .filter(|loan| {
                    loan.call().source_unit() == source_names.source_unit()
                        && ast
                            .ast()
                            .expressions()
                            .get(loan.call().expression())
                            .is_ok_and(|node| {
                                sources
                                    .slice(node.span())
                                    .is_ok_and(|text| text == "run(p, c)")
                            })
                })
                .collect();
            assert_eq!(loans.len(), 2, "two real named wrapper loans");
            let mut end_count = 0;
            for loan in loans {
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert!(matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root()));
                let ends: Vec<_> = unit
                    .ownership()
                    .direct_call_loan_ends()
                    .iter()
                    .filter(|end| end.loan() == loan)
                    .collect();
                end_count += ends.len();
                if ends.len() != 1 {
                    failures.push(format!("after_child={after_child} {layout}: named loan {loan:?}, actual ends={ends:?}"));
                }
                for end in ends {
                    assert_eq!(end.point(), UnitDropPoint::CallReturn(loan.call()));
                }
            }
            if end_count != 2 {
                failures.push(format!(
                    "after_child={after_child} {layout}: expected two real ends, actual={end_count}"
                ));
            }
            let entries: Vec<_> = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.name() == "entry")
                .collect();
            assert_eq!(entries.len(), 1);
            let (program, _) = unit_lower::lower_owned_unit_with_entry(&unit, entries[0].id())
                .expect("ordinary named wrapper lowering");
            verify_program(&program).expect("named wrapper SSA");
            crate::llvm::render_verified_program(&program).expect("named wrapper LLVM");
            verified += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        crate::test_support::diagnostics::failure(format_args!(
            "named Int end authority blockers: {failures:#?}"
        ))
    );
    assert_eq!(verified, 4);
}
