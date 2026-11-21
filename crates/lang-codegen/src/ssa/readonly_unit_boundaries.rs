// Exact original sources through both ordinary Unit layouts.
use super::*;
use crate::ssa::model::{Definition, TerminatorKind};

#[derive(Clone, Copy, Debug)]
enum Receiver {
    Clone,
    GroupedAlias,
    Field,
    Present,
    Element,
}

#[test]
fn direct_readonly_string_field_remains_a_unit_backend_boundary() {
    let name = "app/readonly-method.ko";
    let text = format!(
        "package app\n{}",
        string_field("println(this.text); return 7")
    );
    let unit = UnitSource::analyze(name, &text).unwrap();
    unit.assert_frontend_clean();
    let entry = unit.declaration("entry", SymbolKind::Function);
    let mut sources = SourceMap::new();
    let (source, parsed) = crate::ssa::unit_lower_test_support::parsed(&mut sources, name, &text);
    let inputs = [SourceUnitInput::new("app", name, source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        crate::ssa::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let rooted_entry = crate::ssa::unit_lower_test_support::declaration(&names, "app", "entry");
    let rooted = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        rooted_entry,
    );
    for (map, source, result) in [
        (
            unit.sources(),
            unit.parsed().source_id(),
            unit.lower_entry(entry),
        ),
        (&sources, source, rooted),
    ] {
        let error = result
            .err()
            .expect("direct readonly String field remains unsupported");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let span = error.span.unwrap();
        assert_eq!(span, map.span(source, 73, 82).unwrap());
        assert_eq!(map.source_text(source), Ok(text.as_str()));
        assert_eq!(map.slice(span), Ok("this.text"));
    }
}

#[test]
fn original_readonly_receivers_use_real_unit_loan_and_call_abi() {
    let cases = [
        (
            string_field("println(this.text.clone()); return 7"),
            "item.read()",
            Receiver::Clone,
        ),
        (
            GROUPED_ALIAS.to_owned(),
            "((alias)).read()",
            Receiver::GroupedAlias,
        ),
        (
            FIELD_RECEIVER.to_owned(),
            "outer.item.read()",
            Receiver::Field,
        ),
        (
            nullable_receiver_source("removed.read()"),
            "removed.read()",
            Receiver::Present,
        ),
        (
            nullable_receiver_source("((removed)).read()"),
            "((removed)).read()",
            Receiver::Present,
        ),
        (
            ITERATION_RECEIVER.to_owned(),
            "item.read()",
            Receiver::Element,
        ),
    ];
    for (source, call_text, receiver_kind) in cases {
        let original = format!("package app\n{source}");
        for layout in [Pipeline::Single, Pipeline::Unit] {
            let label = format!("{receiver_kind:?}/{layout:?}");
            let (program, entry_id) = lower(&source, layout, &label, call_text);
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
            let module = program.module(entry_id.module()).unwrap();
            let entry = module.function(entry_id).unwrap();
            let slice = |span: lang_frontend::source::Span| {
                assert_eq!(span.source_id(), entry.origin.span().source_id());
                &original[span.start()..span.end()]
            };
            let mut member_calls = entry.instructions.iter().filter(|i| {
                matches!(i.operation, Operation::DirectCall { .. })
                    && slice(i.origin.span()) == call_text
            });
            let call = member_calls.next().expect(&label);
            assert!(member_calls.next().is_none(), "{label}");
            let Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(receiver)),
                ref arguments,
            } = call.operation
            else {
                panic!("{label}: actual receiver loan");
            };
            let method = module.function(callee).unwrap();
            let method_text = if matches!(receiver_kind, Receiver::Clone) {
                "fun read():Int { println(this.text.clone()); return 7 }"
            } else {
                "fun read():Int=this.n"
            };
            assert_eq!(slice(method.origin.span()), method_text, "{label}");
            assert!(arguments.is_empty());
            let actual = entry.entity(EntityId::Loan(receiver)).unwrap();
            assert!(matches!(
                actual.ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            ));
            assert_eq!(method.receiver, Some(actual.ty));
            let [result_type] = method.return_types.as_slice() else {
                panic!("one Int result");
            };
            assert_eq!(
                module.types[result_type.index()],
                SsaTypeKind::Integer {
                    bits: 32,
                    signed: true
                }
            );
            let [EntityId::Value(value)] = call.results.as_slice() else {
                panic!("one actual result");
            };
            assert_eq!(
                entry.entity(EntityId::Value(*value)).unwrap().ty,
                EntityType::Value(*result_type)
            );

            let block = entry.block(call.block).unwrap();
            let positions = |wanted| {
                block
                    .instructions
                    .iter()
                    .position(|id| *id == wanted)
                    .unwrap()
            };
            let mut ends = block.instructions.iter().filter(|id| {
                matches!(entry.instruction(**id).unwrap().operation, Operation::BorrowEnd { loan } if loan == receiver)
            });
            let end = *ends.next().expect("end same receiver loan in call block");
            assert!(ends.next().is_none());
            assert!(positions(call.id) < positions(end));

            let producer = |id| {
                let Definition::InstructionResult { instruction, .. } =
                    entry.entity(id).unwrap().definition
                else {
                    panic!("{label}: instruction-produced receiver lineage");
                };
                entry.instruction(instruction).unwrap()
            };
            match receiver_kind {
                Receiver::GroupedAlias => {
                    let delivered = producer(EntityId::Loan(receiver));
                    let Operation::BorrowCall { source, .. } = delivered.operation else {
                        panic!("forward actual borrow result");
                    };
                    assert_eq!(slice(delivered.origin.span()), "view(item)");
                    assert!(positions(delivered.id) < positions(call.id));
                    assert!(block.instructions[positions(end)+1..].iter().any(|id| {
                        matches!(entry.instruction(*id).unwrap().operation, Operation::BorrowEnd { loan } if loan == source)
                    }));
                }
                Receiver::Present => {
                    assert!(entry.blocks.iter().any(|parent| {
                        matches!(&parent.terminator.as_ref().unwrap().kind,
                            TerminatorKind::NullableBranch { view, when_non_null, .. }
                                if *view == receiver && when_non_null.target == block.id)
                    }));
                }
                Receiver::Field | Receiver::Element | Receiver::Clone => {
                    let birth = producer(EntityId::Loan(receiver));
                    let Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    } = birth.operation
                    else {
                        panic!("actual shared birth");
                    };
                    assert!(positions(birth.id) < positions(call.id));
                    let place = producer(EntityId::Place(place));
                    if matches!(receiver_kind, Receiver::Element) {
                        assert!(matches!(
                            place.operation,
                            Operation::ContainerElementPlace { .. }
                        ));
                    } else {
                        let Operation::RootPlace { owner } = place.operation else {
                            panic!("actual projected/local receiver");
                        };
                        if matches!(receiver_kind, Receiver::Field) {
                            let project = producer(EntityId::Value(owner));
                            assert!(matches!(
                                project.operation,
                                Operation::AggregateProject { .. }
                            ));
                            assert_eq!(slice(project.origin.span()), "outer.item");
                        } else {
                            assert!(method.instructions.iter().any(|i| {
                                matches!(i.operation, Operation::StringClone { .. })
                                    && slice(i.origin.span()) == "this.text.clone()"
                            }));
                        }
                    }
                }
            }
        }
    }
}
