//! SPEC-0327 ordinary source probes. No fabricated facts, ownership, or native execution.
use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Expression, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, FinalTypeAdmissionError, IntegerOperationKind, TypeKind, UnitExpressionId,
        UnitTypeKind, check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

const INTEGERS: [(&str, BuiltinType); 8] = [
    ("Byte", BuiltinType::Byte),
    ("Short", BuiltinType::Short),
    ("Int", BuiltinType::Int),
    ("Long", BuiltinType::Long),
    ("UByte", BuiltinType::UByte),
    ("UShort", BuiltinType::UShort),
    ("UInt", BuiltinType::UInt),
    ("ULong", BuiltinType::ULong),
];

#[derive(Debug)]
struct Operation {
    kind: IntegerOperationKind,
    source: Option<BuiltinType>,
    target: Option<BuiltinType>,
    ast_identity_matches: bool,
}

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    root: Span,
    root_shape: String,
    result: Option<BuiltinType>,
    raw: Vec<Diagnostic>,
    operation: Option<Operation>,
    ordinary_call: bool,
    complete: Result<(), FinalTypeAdmissionError>,
}

fn single_builtin(kind: Option<&TypeKind>) -> Option<BuiltinType> {
    match kind {
        Some(TypeKind::Builtin(builtin)) => Some(*builtin),
        _ => None,
    }
}

fn unit_builtin(kind: Option<&UnitTypeKind>) -> Option<BuiltinType> {
    match kind {
        Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
        _ => None,
    }
}

fn observe(text: &str, root_text: &str) -> Vec<Observation> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("conversion.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let roots = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (sources.slice(node.span()) == Ok(root_text)).then_some((id, node))
        })
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 1, "unique root {root_text}");
    let (root_id, root_node) = roots[0];
    let Expression::Call { callee, .. } = root_node.payload() else {
        panic!("ordinary call root")
    };
    let Expression::Member { receiver, .. } =
        file.ast().expressions().get(*callee).unwrap().payload()
    else {
        panic!("ordinary member receiver")
    };
    let receiver = *receiver;
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).expect("single raw types");
    let raw = typed.diagnostics().to_vec();
    let kind = typed
        .expression_type(root_id)
        .and_then(|ty| typed.types().get(ty));
    let operation = typed.integer_operation(root_id).map(|op| Operation {
        kind: op.kind(),
        source: single_builtin(typed.types().get(op.receiver_type())),
        target: single_builtin(typed.types().get(op.result_type())),
        ast_identity_matches: op.expression() == root_id && op.receiver() == receiver,
    });
    let complete = complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ());
    assert_eq!(typed.diagnostics(), raw);
    let single = Observation {
        mode: "single",
        root: root_node.span(),
        root_shape: format!("{kind:?}"),
        result: single_builtin(kind),
        raw,
        operation,
        ordinary_call: typed.call(root_id).is_some(),
        complete,
    };
    let inputs = [SourceUnitInput::new("root", "conversion.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let unit = names.names().index().source_units()[0].id();
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit raw types");
    let id = UnitExpressionId::new(unit, root_id);
    let raw = typed.diagnostics().to_vec();
    let kind = typed
        .expression_type(id)
        .and_then(|ty| typed.types().get(ty));
    let operation = typed.integer_operation(id).map(|op| Operation {
        kind: op.kind(),
        source: unit_builtin(typed.types().get(op.receiver_type())),
        target: unit_builtin(typed.types().get(op.result_type())),
        ast_identity_matches: op.expression() == id
            && op.receiver() == UnitExpressionId::new(unit, receiver),
    });
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).map(|_| ());
    assert_eq!(typed.diagnostics(), raw);
    let unit = Observation {
        mode: "unit",
        root: root_node.span(),
        root_shape: format!("{kind:?}"),
        result: unit_builtin(kind),
        raw,
        operation,
        ordinary_call: typed.call(id).is_some(),
        complete,
    };
    let result = vec![single, unit];
    for observation in &result {
        println!(
            "SOURCE {text:?} ROOT {root_text:?} MODE {} SPAN {:?} SHAPE {} OBS {observation:?}",
            observation.mode, observation.root, observation.root_shape
        );
    }
    result
}

fn assert_conversion(observations: &[Observation], source: BuiltinType, target: BuiltinType) {
    for observation in observations {
        assert!(observation.raw.is_empty(), "{observation:?}");
        assert!(observation.complete.is_ok(), "{observation:?}");
        assert_eq!(observation.result, Some(target), "{observation:?}");
        let operation = observation
            .operation
            .as_ref()
            .expect("selected conversion identity");
        assert_eq!(operation.kind, IntegerOperationKind::CheckedConvert);
        assert_eq!(operation.source, Some(source));
        assert_eq!(operation.target, Some(target));
        assert!(operation.ast_identity_matches);
    }
}

// Freeze the observed ordinary source diagnostics, including existing recovery additions.
fn assert_source_rejection(
    observations: &[Observation],
    raw_expected: &[(&str, usize, usize)],
    complete_expected: &[(&str, usize, usize)],
) {
    let identities = |diagnostics: &[Diagnostic]| {
        diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code().to_string(),
                    diagnostic.primary_span().start(),
                    diagnostic.primary_span().end(),
                )
            })
            .collect::<Vec<_>>()
    };
    let expected = |items: &[(&str, usize, usize)]| {
        items
            .iter()
            .map(|(code, start, end)| (code.to_string(), *start, *end))
            .collect::<Vec<_>>()
    };
    for observation in observations {
        assert_eq!(
            identities(&observation.raw),
            expected(raw_expected),
            "{observation:?}"
        );
        match &observation.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(
                    identities(diagnostics),
                    expected(complete_expected),
                    "{observation:?}"
                );
                for original in &observation.raw {
                    assert!(
                        diagnostics.contains(original),
                        "original diagnostic must be preserved"
                    );
                }
            }
            other => panic!("source rejection required, got {other:?}: {observation:?}"),
        }
        assert!(observation.operation.is_none(), "{observation:?}");
    }
}

#[test]
fn all_integer_pairs_publish_target_types() {
    let mut cases = Vec::new();
    for (source_name, source) in INTEGERS {
        for (target_name, target) in INTEGERS {
            let root = format!("value.to{target_name}()");
            let text = format!("fun convert(value: {source_name}): {target_name} = {root}");
            cases.push((source, target, observe(&text, &root)));
        }
    }
    assert_eq!(cases.len(), 64);
    for (source, target, observations) in cases {
        assert_conversion(&observations, source, target);
    }
}

#[test]
fn identity_conversions_remain_explicit_intrinsics() {
    let mut cases = Vec::new();
    for (name, builtin) in INTEGERS {
        let root = format!("value.to{name}()");
        cases.push((
            builtin,
            observe(
                &format!("fun convert(value: {name}): {name} = {root}"),
                &root,
            ),
            observe(
                &format!("fun invert(value: {name}): {name} = value.inv()"),
                "value.inv()",
            ),
        ));
    }
    for (builtin, conversion, invert) in cases {
        for observation in invert {
            assert!(
                observation.raw.is_empty() && observation.complete.is_ok(),
                "{observation:?}"
            );
            assert_eq!(observation.result, Some(builtin));
            assert_eq!(
                observation.operation.unwrap().kind,
                IntegerOperationKind::Invert
            );
        }
        assert_conversion(&conversion, builtin, builtin);
    }
}

#[test]
fn conversion_call_shape_rejects_arguments_and_type_arguments() {
    let cases = [
        "value.toLong(1)",
        "value.toLong<Int>()",
        "value.toLongOrNull()",
        "value.toUnknown()",
    ]
    .map(|root| observe(&format!("fun reject(value: Int): Unit {{ {root} }}"), root));
    for (index, observations) in cases.into_iter().enumerate() {
        match index {
            0 => assert_source_rejection(
                &observations,
                &[("L0121", 44, 45)],
                &[("L0080", 37, 43), ("L0121", 44, 45)],
            ),
            1 => assert_source_rejection(&observations, &[("L0091", 37, 43)], &[("L0091", 37, 43)]),
            2 => assert_source_rejection(&observations, &[], &[("L0080", 37, 49)]),
            3 => assert_source_rejection(&observations, &[], &[("L0080", 37, 46)]),
            _ => unreachable!(),
        }
    }
}

#[test]
fn non_integer_receivers_do_not_gain_conversion_members() {
    let cases = ["Float", "Double", "Char", "Boolean", "String", "Int?"].map(|ty| {
        observe(
            &format!("fun reject(value: {ty}): Unit {{ value.toInt() }}"),
            "value.toInt()",
        )
    });
    for (observations, (start, end)) in
        cases
            .into_iter()
            .zip([(39, 44), (40, 45), (38, 43), (41, 46), (40, 45), (38, 43)])
    {
        let expected = [("L0080", start, end)];
        assert_source_rejection(&observations, &expected, &expected);
    }
}

#[test]
fn user_named_conversion_members_keep_nominal_dispatch() {
    let ordinary = observe(
        "class Sample { fun toInt(): Int = 7 }\nfun convert(value: Sample): Int = value.toInt()",
        "value.toInt()",
    );
    let intrinsic = observe(
        "fun convert(value: Int): Int = value.toInt()",
        "value.toInt()",
    );
    for observation in ordinary {
        assert!(
            observation.raw.is_empty() && observation.complete.is_ok(),
            "{observation:?}"
        );
        assert_eq!(observation.result, Some(BuiltinType::Int));
        assert!(observation.ordinary_call && observation.operation.is_none());
    }
    assert_conversion(&intrinsic, BuiltinType::Int, BuiltinType::Int);
}

#[test]
fn checked_conversion_calls_remain_outside_const_evaluation() {
    let runtime = observe("fun convert(): Long = 1.toLong()", "1.toLong()");
    let constant = observe("const val VALUE: Long = 1.toLong()", "1.toLong()");
    // Until runtime selection succeeds, a const diagnostic cannot prove the const whitelist gate.
    assert_conversion(&runtime, BuiltinType::Int, BuiltinType::Long);
    assert_source_rejection(
        &constant,
        &[("L0156", 24, 34)],
        &[("L0156", 24, 34), ("L0080", 26, 32)],
    );
}
