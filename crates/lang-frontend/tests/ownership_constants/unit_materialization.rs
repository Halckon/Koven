//! Original single-source materialization contracts through both Unit entries.
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstEnabledOwnedUnit, ConstantMaterializationKind, UnitDropTarget, UnitLoanTarget,
        check_compilation_unit_constant_ownership,
    },
    parser::{BinaryOperator, Expression, Item, NameMarker, StringPart},
    source::SourceMap,
    type_checking::{
        BuiltinType, ConstEnabledTypedUnit, ConstValue, UnitExpressionId,
        check_compilation_unit_types, standard_environments,
    },
};

pub(super) fn checked(
    text: &str,
    rooted: bool,
) -> (SourceMap, ConstEnabledTypedUnit, ConstEnabledOwnedUnit) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("constants.ko", text).unwrap();
    let parsed = super::parser_test_assertions::parse_file_twice(
        &sources,
        source,
        "constant ownership Unit",
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let input = if rooted {
        SourceUnitInput::new("root", "constants.ko", source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    };
    let inputs = [input];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    assert!(index.diagnostics().is_empty());
    let [entry] = index.source_units() else {
        panic!("one original source")
    };
    assert_eq!(entry.source_id(), source);
    match (rooted, entry.key()) {
        (false, SourceUnitKey::MemoryDocument) => {}
        (true, SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "constants.ko");
        }
        _ => panic!("original entry identity"),
    }
    let (environment, types) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate_constants().unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap();
    assert!(
        owned.ownership().diagnostics().is_empty(),
        "{:?}",
        owned.ownership().diagnostics()
    );
    assert!(
        owned.ownership().deferred().is_empty(),
        "{:?}",
        owned.ownership().deferred()
    );
    assert!(owned.is_compatible_with(&typed));
    let owned = owned.validate().unwrap();
    (sources, typed, owned)
}

pub(super) fn scalar(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = checked(text, rooted);
        assert_eq!(
            owned.materializations().len(),
            2,
            "group must not rematerialize"
        );
        assert!(
            owned
                .materializations()
                .iter()
                .all(|p| p.kind() == ConstantMaterializationKind::InlineCopy)
        );
        assert!(owned.ownership().loans().is_empty());
        assert!(owned.ownership().drops().is_empty());
        assert!(owned.ownership().captures().is_empty());
    }
}

pub(super) fn identity(text: &str) {
    for rooted in [false, true] {
        let (sources, typed, owned) = checked(text, rooted);
        assert!(owned.is_compatible_with(&typed));
        assert_eq!(owned.materializations().len(), 3);
        assert_eq!(owned.ownership().drops().len(), 2);
        let (_, other, repeated) = checked(text, rooted);
        assert!(!owned.is_compatible_with(&other));
        let snapshot = |owned: &ConstEnabledOwnedUnit| {
            owned
                .materializations()
                .iter()
                .map(|plan| {
                    let d = plan.descriptor();
                    (
                        d.expression().expression().index(),
                        d.target().symbol().index(),
                        d.value().clone(),
                        plan.kind(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(snapshot(&owned), snapshot(&repeated));
        for (plan, kind) in owned.materializations().iter().zip([
            ConstantMaterializationKind::StringTemporary,
            ConstantMaterializationKind::StringTemporary,
            ConstantMaterializationKind::InlineCopy,
        ]) {
            assert_eq!(plan.kind(), kind);
            assert_eq!(
                owned
                    .materialization_at(plan.descriptor().expression())
                    .unwrap()
                    .descriptor()
                    .target(),
                plan.descriptor().target()
            );
        }
        assert_eq!(
            sources
                .slice(owned.ownership().drops()[0].value_origin())
                .unwrap(),
            "COPY"
        );
    }
}

pub(super) fn grouped(text: &str) {
    for rooted in [false, true] {
        let (_, _, owned) = checked(text, rooted);
        assert_eq!(owned.materializations().len(), 3);
        assert_eq!(owned.ownership().loans().len(), 1);
        for loan in owned.ownership().loans() {
            if let UnitLoanTarget::Temporary(owner) = loan.target() {
                assert!(owned.materialization_at(*owner).is_some(), "{loan:?}");
            }
        }
        let temporaries = owned
            .ownership()
            .drops()
            .iter()
            .filter_map(|drop| match drop.target() {
                UnitDropTarget::Temporary(owner) => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(temporaries.len(), 3);
        for owner in temporaries {
            assert!(owned.materialization_at(owner).is_some(), "{owner:?}");
        }
    }
}

#[test]
fn loop_successor_cleanup_requires_a_reachable_break() {
    for (body, reaches_tail) in [
        ("loop { continue }", false),
        ("loop { break }", true),
        ("loop { loop { break }; continue }", false),
        ("loop { loop { continue }; break }", false),
        ("loop { loop { break }; break }", true),
        ("loop { if (flag) { break } else { continue } }", true),
    ] {
        let text = format!(
            "const val TEXT = \"hi\"\nfun view(text: String): Unit {{}}\nfun run(flag: Boolean): Unit {{ {body}; val tail = view(TEXT) }}"
        );
        for rooted in [false, true] {
            let (sources, _, owned) = checked(&text, rooted);
            let expected = usize::from(reaches_tail);
            assert_eq!(owned.materializations().len(), expected, "{body}");
            assert_eq!(owned.ownership().drops().len(), expected, "{body}");
            for drop in owned.ownership().drops() {
                let UnitDropTarget::Temporary(expression) = drop.target() else {
                    panic!("tail String owner")
                };
                assert!(owned.materialization_at(expression).is_some());
                assert_eq!(sources.slice(drop.value_origin()), Ok("TEXT"));
            }
        }
    }
}

pub(super) fn aborting_string(text: &str) {
    aborting_binary(text, BinaryOperator::Add);
}

pub(super) fn aborting_equality(text: &str) {
    aborting_binary(text, BinaryOperator::Equal);
}

fn aborting_binary(text: &str, expected_operator: BinaryOperator) {
    for rooted in [false, true] {
        let (sources, typed, owned) = checked(text, rooted);
        assert!(owned.is_compatible_with(&typed));

        // The two reads must be real, resolved constants, not missing typed
        // facts incidentally making the materialization assertions pass.
        let [declaration] = typed.constants().declarations() else {
            panic!("one original TEXT constant");
        };
        let source = declaration.declaration_span().source_id();
        assert_eq!(sources.source_text(source), Ok(text));
        assert_eq!(sources.slice(declaration.declaration_span()), Ok("TEXT"));
        let ConstValue::String(bytes) = declaration.value() else {
            panic!("expected String constant");
        };
        assert_eq!(bytes.as_ref(), b"hi");
        let parsed = super::parser_test_assertions::parse_file_twice(
            &sources,
            source,
            "aborting string original input",
        );
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let ast = parsed.ast();
        let initializer = |name| {
            let matching: Vec<_> = ast
                .items()
                .iter()
                .filter_map(|(_, node)| match node.payload() {
                    Item::Variable {
                        name: NameMarker::Present(marker),
                        initializer,
                        ..
                    } if sources.slice(*marker) == Ok(name) => Some(*initializer),
                    _ => None,
                })
                .collect();
            let [expression] = matching.as_slice() else {
                panic!("one original initializer: {name}");
            };
            *expression
        };
        let stopped = initializer("stopped");
        let unreachable = initializer("unreachable");
        let expression = |id| ast.expressions().get(id).unwrap();
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = expression(stopped).payload()
        else {
            panic!("original binary expression");
        };
        assert_eq!(*operator, expected_operator);
        let stop = match expected_operator {
            BinaryOperator::Equal => *left,
            BinaryOperator::Add => {
                let Expression::String { parts } = expression(*left).payload() else {
                    panic!("original interpolated LHS")
                };
                let [StringPart::Interpolation { expression, .. }] = parts.as_slice() else {
                    panic!("the original Nothing call is the only interpolation part")
                };
                *expression
            }
            _ => unreachable!("only the original tested operators"),
        };
        assert_eq!(sources.slice(expression(stop).span()), Ok("stop()"));
        let qualify = |id| UnitExpressionId::new(declaration.symbol().source_unit(), id);
        let stop = qualify(stop);
        let actual_call = typed
            .types()
            .call(stop)
            .expect("the actual non-returning call");
        assert_eq!(actual_call.expression(), stop);
        assert!(actual_call.arguments().is_empty());
        assert_eq!(
            Some(actual_call.return_type()),
            typed.types().types().builtin(BuiltinType::Nothing)
        );

        let [rhs, following] = typed.constants().uses() else {
            panic!("both original TEXT occurrences have typed constant facts");
        };
        for (read, expected) in [(rhs, *right), (following, unreachable)] {
            assert_eq!(read.expression(), qualify(expected));
            assert_eq!(sources.slice(expression(expected).span()), Ok("TEXT"));
            assert_eq!(read.target(), declaration.symbol());
            assert_eq!(read.value(), declaration.value());
            assert_eq!(read.ty(), declaration.ty());
            assert!(owned.materialization_at(qualify(expected)).is_none());
        }
        assert!(owned.materializations().is_empty());
        assert!(
            owned.ownership().drops().is_empty(),
            "{:?}",
            owned.ownership().drops()
        );
        assert!(owned.ownership().loans().is_empty());
        if expected_operator == BinaryOperator::Equal {
            assert!(owned.ownership().direct_call_loan_ends().is_empty());
        }
        assert!(owned.ownership().value_deliveries().is_empty());
    }
}
