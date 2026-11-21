use crate::ssa::{
    model::{Edge, EntityId, FunctionId, Operation, Program, ScalarConstant, TerminatorKind},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey},
    source::SourceMap,
    type_checking::{complete_compilation_unit_types, standard_environments},
};

#[test]
fn original_live_maps_short_circuit_and_release_on_both_unit_edges() {
    let text = super::ORIGINAL_LIVE_MAP_SHORT_CIRCUIT;
    let path = "lowering.ko";
    with_verified_unit(text, path, |sources, program, entry| {
        let module = program.module(entry.module()).unwrap();
        let function = module.function(entry).unwrap();
        assert_eq!(sources.slice(function.origin.span()), Ok(text));
        let queries = function
            .instructions
            .iter()
            .filter(|inst| matches!(inst.operation, Operation::MapContains { .. }))
            .collect::<Vec<_>>();
        let [lhs, rhs] = queries.as_slice() else {
            panic!("the exact source has two Map queries")
        };
        let span =
            |inst: &crate::ssa::model::Instruction| sources.slice(inst.origin.span()).unwrap();
        assert_eq!(span(lhs), "first.contains(3)");
        assert_eq!(span(rhs), "second.contains(3)");
        let Operation::MapContains {
            owner: EntityId::Value(left),
            ..
        } = lhs.operation
        else {
            panic!("left owner is live")
        };
        let [EntityId::Value(predicate)] = lhs.results.as_slice() else {
            panic!("actual left predicate")
        };
        let block = function.block(lhs.block).unwrap();
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &block.terminator.as_ref().unwrap().kind
        else {
            panic!("actual short-circuit fork")
        };
        assert_eq!(condition, predicate);
        assert_eq!(when_true.target, rhs.block);
        assert_ne!(when_false.target, rhs.block);
        assert_eq!(when_true.arguments, when_false.arguments);
        let [second @ EntityId::Value(_)] = when_true.arguments.as_slice() else {
            panic!("only the same live second Map crosses both edges")
        };
        assert_ne!(second, &EntityId::Value(left));
        let transferred = |edge: &Edge| {
            let body = function.block(edge.target).unwrap();
            let [EntityId::Value(map)] = body.parameters.as_slice() else {
                panic!("only the transported Map")
            };
            let instructions = body
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .collect::<Vec<_>>();
            (body, *map, instructions)
        };
        let (_, second_when_true, taken) = transferred(when_true);
        let (skip, second_when_false, skipped) = transferred(when_false);
        assert!(
            matches!(rhs.operation, Operation::MapContains { owner: EntityId::Value(owner), .. } if owner == second_when_true)
        );
        let [_, query, release] = taken.as_slice() else {
            panic!("second query before releasing its Map")
        };
        assert_eq!(query.id, rhs.id);
        assert_eq!(
            release.operation,
            Operation::Drop {
                owner: second_when_true
            }
        );
        let [release] = skipped.as_slice() else {
            panic!("left false bypasses the rhs entirely")
        };
        assert_eq!(
            release.operation,
            Operation::Drop {
                owner: second_when_false
            }
        );
        assert!(matches!(
            skip.terminator.as_ref().unwrap().kind,
            TerminatorKind::Branch(_)
        ));

        let left_release = block
            .instructions
            .iter()
            .position(|id| {
                function.instruction(*id).unwrap().operation == Operation::Drop { owner: left }
            })
            .unwrap();
        let left_query = block
            .instructions
            .iter()
            .position(|id| *id == lhs.id)
            .unwrap();
        assert!(left_query < left_release);
        let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
        crate::ssa::unit_field_replace_tests::link_and_run(&llvm, "", b"maps\n");
    });
}

pub(super) fn with_verified_unit(
    text: &str,
    path: &str,
    mut assert_layout: impl FnMut(&SourceMap, &Program, FunctionId),
) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, text);
    assert_eq!(sources.source_text(source), Ok(text));
    let (ne, te) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", path, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        complete_compilation_unit_types(&sources, &inputs, &names, &te, typed.types()).unwrap();
        let index = names.names().index();
        let [input] = index.source_units() else {
            panic!("one original input")
        };
        assert_eq!(input.source_id(), source);
        assert_eq!(
            matches!(input.key(), SourceUnitKey::MemoryDocument),
            !rooted
        );
        let declaration = index
            .declarations()
            .iter()
            .find(|decl| decl.name() == "main")
            .unwrap()
            .id();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &te,
            &typed,
            &owned,
            declaration,
        )
        .expect("the preserved original source really is supported by Unit");
        crate::ssa::verify::verify_program(&program).unwrap();
        assert_layout(&sources, &program, entry);
    }
}

#[test]
fn derived_live_maps_observe_rhs_skip_and_owner_on_both_dynamic_routes() {
    // This has additional functions and a parameter; it is not the exact Single fixture.
    let drive = super::ORIGINAL_LIVE_MAP_SHORT_CIRCUIT
        .replace("fun main()", "fun drive(probe: Int)")
        .replace("first.contains(3)", "first.contains(probe)")
        .replace("second.contains(3)", "right(second)");
    for (probe, stdout) in [
        (3, b"rhs\nmaps\ndone\n".as_slice()),
        (8, b"done\n".as_slice()),
    ] {
        let text = format!(
            "fun right(own map: MutableMap<Int, String>): Boolean {{ println(\"rhs\"); return map.contains(3) }}\n{drive}\nfun main(): Unit {{ drive({probe}); println(\"done\") }}"
        );
        with_verified_unit(
            &text,
            "derived-map-short-circuit.ko",
            |sources, program, entry| {
                let module = program.module(entry.module()).unwrap();
                let function = |prefix: &str| {
                    let mut found = module
                        .functions
                        .iter()
                        .filter(|f| sources.slice(f.origin.span()).unwrap().starts_with(prefix));
                    let result = found.next().expect("real source declaration");
                    assert!(found.next().is_none());
                    result
                };
                let drive = function("fun drive(");
                let right = function("fun right(");
                let main = module.function(entry).unwrap();
                let main_call = main.instructions.iter().find(|inst|
                matches!(inst.operation, Operation::DirectCall { callee, .. } if callee == drive.id())).unwrap();
                let Operation::DirectCall { arguments, .. } = &main_call.operation else {
                    unreachable!()
                };
                let [EntityId::Loan(probe_loan)] = arguments.as_slice() else {
                    panic!("test entry calls actual drive with one probe")
                };
                let main_def = |entity| {
                    &main
                        .instructions
                        .iter()
                        .find(|inst| inst.results.as_slice() == [entity])
                        .unwrap()
                        .operation
                };
                let Operation::BorrowBegin { place, .. } = main_def(EntityId::Loan(*probe_loan))
                else {
                    panic!("borrow of the actual probe value")
                };
                let Operation::RootPlace { owner } = main_def(EntityId::Place(*place)) else {
                    panic!("probe's original value")
                };
                assert_eq!(
                    main_def(EntityId::Value(*owner)),
                    &Operation::Constant(ScalarConstant::Integer(probe))
                );

                let [lhs]: [_; 1] = drive
                    .instructions
                    .iter()
                    .filter(|inst| matches!(inst.operation, Operation::MapContains { .. }))
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("drive only queries the left Map directly");
                assert_eq!(
                    sources.slice(lhs.origin.span()),
                    Ok("first.contains(probe)")
                );
                let TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } = &drive
                    .block(lhs.block)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind
                else {
                    panic!("actual left query controls the fork")
                };
                assert_eq!(lhs.results.as_slice(), [EntityId::Value(*condition)]);
                assert_ne!(when_true.target, when_false.target);
                assert_eq!(when_true.arguments, when_false.arguments);
                let [EntityId::Value(_)] = when_true.arguments.as_slice() else {
                    panic!("the same unique right owner is transferred on both edges")
                };
                let edge = |edge: &Edge| {
                    let block = drive.block(edge.target).unwrap();
                    let [EntityId::Value(owner)] = block.parameters.as_slice() else {
                        panic!("owner transported by the actual edge")
                    };
                    let [instruction] = block.instructions.as_slice() else {
                        panic!("each branch performs only its one ownership action")
                    };
                    (*owner, drive.instruction(*instruction).unwrap())
                };
                let (taken, invoke) = edge(when_true);
                let (skipped, drop) = edge(when_false);
                assert_eq!(sources.slice(invoke.origin.span()), Ok("right(second)"));
                assert_eq!(
                    invoke.operation,
                    Operation::DirectCall {
                        callee: right.id(),
                        receiver: None,
                        arguments: vec![EntityId::Value(taken)]
                    }
                );
                assert_eq!(drop.operation, Operation::Drop { owner: skipped });

                let right_entry = right.block(right.entry_block().unwrap()).unwrap();
                let [EntityId::Value(received)] = right_entry.parameters.as_slice() else {
                    panic!("the observer takes exact ownership")
                };
                let [query]: [_; 1] = right
                    .instructions
                    .iter()
                    .filter(|inst| matches!(inst.operation, Operation::MapContains { .. }))
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("observer performs the real query");
                assert_eq!(sources.slice(query.origin.span()), Ok("map.contains(3)"));
                assert!(
                    matches!(query.operation, Operation::MapContains { owner: EntityId::Value(owner), .. } if owner == *received)
                );
                let [drop]: [_; 1] = right
                    .instructions
                    .iter()
                    .filter(|inst| inst.operation == Operation::Drop { owner: *received })
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("helper releases the original owner after its query");
                let ops = &right_entry.instructions;
                let at = |id| ops.iter().position(|op| *op == id).unwrap();
                assert!(at(query.id) < at(drop.id));
                let [EntityId::Value(result)] = query.results.as_slice() else {
                    panic!("actual rhs result")
                };
                assert!(matches!(&right_entry.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { values } if values.as_slice() == [*result]));

                let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
                crate::ssa::unit_field_replace_tests::link_and_run(&llvm, "", stdout);
            },
        );
    }
}
