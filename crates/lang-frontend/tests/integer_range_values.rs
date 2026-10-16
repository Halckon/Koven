//! SPEC-0328 ordinary source probes: no fabricated facts or language execution.
use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, Copyability, FinalTypeAdmissionError, TypeKind, UnitExpressionId,
        UnitTypeKind, UnitTypeRefId, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    root: Span,
    names: Vec<Diagnostic>,
    raw: Option<Vec<Diagnostic>>,
    shape: Option<String>,
    resolved: bool,
    boolean: bool,
    copyable: bool,
    matches_annotation: Option<bool>,
    complete: Option<Result<(), FinalTypeAdmissionError>>,
}

fn observe(text: &str, root_text: &str, annotation: Option<&str>) -> Vec<Observation> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex API");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parse API");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let roots = file
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| sources.slice(node.span()) == Ok(root_text))
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 1, "unique root {root_text}");
    let (root_id, root_node) = roots[0];
    let annotation_id = annotation.map(|text| {
        let found = file
            .ast()
            .type_refs()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok(text))
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "unique annotation {text}");
        found[0]
    });
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names API");
    let typed = check_types(&sources, &file, &names, &types).expect("single raw API");
    let root_type = typed.expression_type(root_id);
    let kind = root_type.and_then(|id| typed.types().get(id));
    let single = Observation {
        mode: "single",
        root: root_node.span(),
        names: names.diagnostics().to_vec(),
        raw: Some(typed.diagnostics().to_vec()),
        shape: Some(format!("{kind:?}")),
        resolved: matches!(kind, Some(kind) if !matches!(kind, TypeKind::Error | TypeKind::Deferred(_))),
        boolean: matches!(kind, Some(TypeKind::Builtin(BuiltinType::Boolean))),
        copyable: root_type.and_then(|id| typed.copyability(id)) == Some(Copyability::Copyable),
        matches_annotation: annotation_id
            .map(|id| root_type.is_some() && root_type == typed.type_ref_type(id)),
        complete: Some(complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ())),
    };
    let inputs = [SourceUnitInput::new("root", "range.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index API");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names API");
    let name_diagnostics = names.diagnostics().to_vec();
    let unit = match names.validate() {
        Err(_) => Observation {
            mode: "unit names rejected",
            root: root_node.span(),
            names: name_diagnostics,
            raw: None,
            shape: None,
            resolved: false,
            boolean: false,
            copyable: false,
            matches_annotation: None,
            complete: None,
        },
        Ok(names) => {
            let unit = names.names().index().source_units()[0].id();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .expect("unit raw API");
            let root_type = typed.expression_type(UnitExpressionId::new(unit, root_id));
            let kind = root_type.and_then(|id| typed.types().get(id));
            Observation {
                mode: "unit",
                root: root_node.span(),
                names: name_diagnostics,
                raw: Some(typed.diagnostics().to_vec()),
                shape: Some(format!("{kind:?}")),
                resolved: matches!(kind, Some(kind) if !matches!(kind, UnitTypeKind::Error | UnitTypeKind::Deferred(_))),
                boolean: matches!(kind, Some(UnitTypeKind::Builtin(BuiltinType::Boolean))),
                copyable: root_type.map(|id| typed.copyability(id)) == Some(Copyability::Copyable),
                matches_annotation: annotation_id.map(|id| {
                    root_type.is_some()
                        && root_type == typed.type_ref_type(UnitTypeRefId::new(unit, id))
                }),
                complete: Some(
                    complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                        .map(|_| ()),
                ),
            }
        }
    };
    let result = vec![single, unit];
    for observation in &result {
        println!(
            "SOURCE {text:?} ROOT {root_text:?} MODE {} SPAN {:?} SHAPE {:?} OBS {observation:?}",
            observation.mode, observation.root, observation.shape
        );
    }
    result
}

fn assert_complete(observation: &Observation) {
    assert!(observation.names.is_empty(), "{observation:?}");
    assert!(
        observation.raw.as_ref().is_some_and(Vec::is_empty),
        "{observation:?}"
    );
    assert!(observation.resolved, "{observation:?}");
    assert!(
        matches!(observation.complete, Some(Ok(()))),
        "{observation:?}"
    );
}

#[test]
fn implicit_range_construction_has_resolved_copyable_value() {
    let mut all = Vec::new();
    for ty in ["Int", "Long"] {
        for operator in ["..", "..<"] {
            let root = format!("left {operator} right");
            let text = format!("fun probe(left: {ty}, right: {ty}): Unit {{ val range = {root} }}");
            all.extend(observe(&text, &root, None));
        }
    }
    // Initial public-API red oracle; exact range identity is paired below.
    for observation in all {
        assert_complete(&observation);
        assert!(observation.copyable, "{observation:?}");
    }
}

#[test]
fn explicit_range_identity_and_capabilities_are_available() {
    let mut all = Vec::new();
    for (ty, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        let text = format!("fun probe(left: {ty}, right: {ty}): {range} = left..right");
        all.extend(observe(&text, "left..right", Some(range)));
        // Ordinary bound checking exercises Transferable without starting a thread.
        let text = format!(
            "fun <T: Transferable> accept(value: T): Unit {{}}\nfun probe(left: {ty}, right: {ty}): Unit {{\n val range: {range} = left..right\n val copy = range\n accept(copy)\n accept(range)\n}}"
        );
        all.extend(observe(&text, "left..right", Some(range)));
    }
    for observation in all {
        assert_complete(&observation);
        assert_eq!(
            observation.matches_annotation,
            Some(true),
            "{observation:?}"
        );
        assert!(observation.copyable, "{observation:?}");
    }
}

#[test]
fn range_membership_and_equality_publish_boolean_results() {
    let mut all = Vec::new();
    for ty in ["Int", "Long"] {
        for root in [
            "value in (left..right)",
            "value !in (left..<right)",
            "(left..right) == (left..<right)",
            "(left..right) != (left..<right)",
        ] {
            let text = format!("fun probe(left: {ty}, right: {ty}, value: {ty}): Boolean = {root}");
            all.extend(observe(&text, root, None));
        }
    }
    for observation in all {
        assert_complete(&observation);
        assert!(observation.boolean, "{observation:?}");
    }
}

#[test]
fn range_values_cross_existing_storage_boundaries() {
    let mut all = Vec::new();
    for (ty, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        for text in [
            format!(
                "fun accept(value: {range}): Unit {{}}\nfun probe(left: {ty}, right: {ty}): Unit {{\n val stored = left..right\n val copied = stored\n accept(copied)\n accept(stored)\n}}"
            ),
            format!(
                "fun pass(own value: {range}): {range} = value\nfun probe(left: {ty}, right: {ty}): {range} = pass(left..right)"
            ),
            format!(
                "class Holder(val value: {range})\nfun probe(left: {ty}, right: {ty}): Holder = Holder(left..right)"
            ),
            format!("fun probe(left: {ty}, right: {ty}): List<{range}> = listOf(left..right)"),
        ] {
            all.extend(observe(&text, "left..right", None));
        }
    }
    for observation in all {
        assert_complete(&observation);
        assert!(observation.copyable, "{observation:?}");
    }
}

#[test]
fn invalid_range_endpoints_have_operand_diagnostics() {
    let mut all = Vec::new();
    let pairs = [
        ("Int", "Long"),
        ("Long", "Int"),
        ("Byte", "Byte"),
        ("Short", "Short"),
        ("UByte", "UByte"),
        ("UShort", "UShort"),
        ("UInt", "UInt"),
        ("ULong", "ULong"),
        ("Float", "Float"),
        ("Double", "Double"),
        ("Char", "Char"),
        ("Boolean", "Boolean"),
        ("String", "String"),
        ("Int?", "Int?"),
        ("Long?", "Long?"),
    ];
    for (left, right) in pairs {
        for operator in ["..", "..<"] {
            let root = format!("left {operator} right");
            let text =
                format!("fun probe(left: {left}, right: {right}): Unit {{ val bad = {root} }}");
            all.extend(observe(&text, &root, None));
        }
    }
    for observation in all {
        assert!(observation.names.is_empty(), "{observation:?}");
        let raw = observation
            .raw
            .as_ref()
            .expect("types reached for known endpoint types");
        assert!(
            raw.iter().any(|d| d.code().to_string() == "L0085"),
            "{observation:?}"
        );
        let Some(Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics))) =
            &observation.complete
        else {
            panic!("source diagnostics required: {observation:?}");
        };
        assert!(
            raw.iter().all(|d| final_diagnostics.contains(d)),
            "{observation:?}"
        );
    }
}

#[test]
fn range_identity_does_not_enable_extra_public_apis() {
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for (ty, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        let text = format!("fun probe(left: {ty}, right: {ty}): {range} = left..right");
        positive.extend(observe(&text, "left..right", Some(range)));
        let call = format!("{range}()");
        negative.extend(observe(
            &format!("fun probe(): Unit {{ val bad = {call} }}"),
            &call,
            None,
        ));
        for member in ["first", "last", "step", "size", "hash"] {
            let root = format!("value.{member}");
            negative.extend(observe(
                &format!("fun probe(value: {range}): Unit {{ val bad = {root} }}"),
                &root,
                None,
            ));
        }
        negative.extend(observe(
            &format!("fun probe(left: {ty}, right: {ty}): {range}<Int> = left..right"),
            "left..right",
            None,
        ));
        for container in ["Map", "MutableMap"] {
            negative.extend(observe(
                &format!("fun probe(value: {container}<{range}, Int>): Unit {{ val marker = 0 }}"),
                "0",
                None,
            ));
        }
    }
    // A missing Range identity cannot count as proof of its dedicated API gates.
    for observation in positive {
        assert_complete(&observation);
        assert_eq!(
            observation.matches_annotation,
            Some(true),
            "{observation:?}"
        );
    }
    // Initial observation oracle only; freeze exact reason/span once identity exists.
    for observation in negative {
        match observation.complete {
            Some(Err(FinalTypeAdmissionError::Diagnostics(ref diagnostics))) => {
                assert!(!diagnostics.is_empty(), "{observation:?}");
            }
            None => assert!(!observation.names.is_empty(), "{observation:?}"),
            _ => panic!("structured source rejection required: {observation:?}"),
        }
    }
}
