//! SPEC-0326 first source-only batch. No fact mutation or artifact execution.
use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, CompilationUnitTypes, FinalTypeAdmissionError, TypeId, TypeKind, TypedFile,
        UnitExpressionId, UnitTypeId, UnitTypeKind, check_compilation_unit_types, check_types,
        complete_compilation_unit_types, complete_file_types, standard_environments,
    },
};

#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Builtin(BuiltinType),
    Nullable(Box<Shape>),
    Other(String),
}

#[derive(Debug)]
struct Observation {
    mode: &'static str,
    root: String,
    shape: Shape,
    raw: Vec<Diagnostic>,
    primary_text: Vec<String>,
    complete: Result<(), FinalTypeAdmissionError>,
}

fn single_shape(typed: &TypedFile, id: TypeId) -> Shape {
    match typed.types().get(id).expect("single type") {
        TypeKind::Builtin(kind) => Shape::Builtin(*kind),
        TypeKind::Nullable(inner) => Shape::Nullable(Box::new(single_shape(typed, *inner))),
        other => Shape::Other(format!("{other:?}")),
    }
}

fn unit_shape(typed: &CompilationUnitTypes, id: UnitTypeId) -> Shape {
    match typed.types().get(id).expect("unit type") {
        UnitTypeKind::Builtin(kind) => Shape::Builtin(*kind),
        UnitTypeKind::Nullable(inner) => Shape::Nullable(Box::new(unit_shape(typed, *inner))),
        other => Shape::Other(format!("{other:?}")),
    }
}

fn observe(root: &str) -> Vec<Observation> {
    let text = format!(
        "fun test(flag: Boolean, optional: Int?, wide: Long): Unit {{ val result = {root} }}"
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", text).expect("source");
    let lexed = lex(&sources, source).expect("lex");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let ids: Vec<_> = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(root)).then_some(id))
        .collect();
    assert_eq!(ids.len(), 1, "unique root {root}");
    let id = ids[0];
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).expect("single types");
    let raw = typed.diagnostics().to_vec();
    let single = Observation {
        mode: "single",
        root: root.to_owned(),
        shape: single_shape(&typed, typed.expression_type(id).expect("single root type")),
        primary_text: raw
            .iter()
            .map(|d| sources.slice(d.primary_span()).unwrap().to_owned())
            .collect(),
        complete: complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ()),
        raw: raw.clone(),
    };
    assert_eq!(typed.diagnostics(), raw);
    eprintln!("SPEC-0326 {single:?}");
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let unit = names.names().index().source_units()[0].id();
    let typed =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
    let raw = typed.diagnostics().to_vec();
    let unit = Observation {
        mode: "unit",
        root: root.to_owned(),
        shape: unit_shape(
            &typed,
            typed
                .expression_type(UnitExpressionId::new(unit, id))
                .expect("unit root type"),
        ),
        primary_text: raw
            .iter()
            .map(|d| sources.slice(d.primary_span()).unwrap().to_owned())
            .collect(),
        complete: complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map(|_| ()),
        raw: raw.clone(),
    };
    assert_eq!(typed.diagnostics(), raw);
    eprintln!("SPEC-0326 {unit:?}");
    vec![single, unit]
}

fn roots(left: &str, right: &str) -> [String; 2] {
    [
        format!("if (flag) {left} else {right}"),
        format!("when (flag) {{ true -> {left}; false -> {right} }}"),
    ]
}

#[test]
fn known_mixed_value_branches_reject_implicit_any() {
    let observations: Vec<_> = ["\"text\"", "wide"]
        .into_iter()
        .flat_map(|right| roots("1", right))
        .flat_map(|root| observe(&root))
        .collect();
    for o in observations {
        let is_if = o.root.starts_with("if");
        let code = if is_if { "L0089" } else { "L0112" };
        assert_eq!(
            o.raw
                .iter()
                .map(|d| d.code().as_str().to_owned())
                .collect::<Vec<_>>(),
            [code],
            "{} {o:?}",
            o.mode
        );
        assert_eq!(
            o.primary_text,
            [if is_if {
                "else"
            } else if o.root.contains("text") {
                "\"text\""
            } else {
                "wide"
            }]
        );
        match o.complete {
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                assert_eq!(diagnostics, o.raw)
            }
            other => panic!("{} expected preserved diagnostics: {other:?}", o.mode),
        }
    }
}

#[test]
fn same_type_and_bottom_branches_keep_exact_result() {
    let observations: Vec<_> = [
        ("1", "2"),
        ("error(\"stop\")", "2"),
        ("2", "error(\"stop\")"),
    ]
    .into_iter()
    .flat_map(|(left, right)| roots(left, right))
    .flat_map(|root| observe(&root))
    .collect();
    for o in observations {
        assert_eq!(
            o.shape,
            Shape::Builtin(BuiltinType::Int),
            "{} {o:?}",
            o.mode
        );
        assert!(o.raw.is_empty(), "{o:?}");
        assert!(o.complete.is_ok(), "{o:?}");
    }
}

#[test]
fn null_and_nullable_branches_use_canonical_result() {
    let observations: Vec<_> = [
        ("null", "2"),
        ("2", "null"),
        ("optional", "2"),
        ("2", "optional"),
        ("optional", "null"),
        ("null", "optional"),
    ]
    .into_iter()
    .flat_map(|(left, right)| roots(left, right))
    .flat_map(|root| observe(&root))
    .collect();
    for o in observations {
        assert_eq!(
            o.shape,
            Shape::Nullable(Box::new(Shape::Builtin(BuiltinType::Int))),
            "{} {o:?}",
            o.mode
        );
        assert!(o.raw.is_empty(), "{o:?}");
        assert!(o.complete.is_ok(), "{o:?}");
    }
}
