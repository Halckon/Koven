use super::*;
use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_unit_names, append_builtin_display},
    type_checking::{UnitDisplayTarget, UnitInterpolationPart},
};

#[test]
fn explicit_display_uses_the_authorized_requirement_and_real_receiver() {
    use lang_frontend::type_checking::{ParameterMode, UnitCallTarget, UnitCallableTarget};

    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "probe.ko",
            r#"fun <T : Display> generic(borrow value: T): String = value.toString()
fun text(borrow value: String): String = value.toString()
fun integer(value: Int): String = value.toString()
fun floating(value: Double): String = value.toString()
class Unrelated() { fun toString(): String = "unrelated" }
fun unrelated(value: Unrelated): String = value.toString()
"#,
        )
        .unwrap();
    let (mut names, types) = standard_environments();
    let mut descriptors = vec![UnitSourceDescriptor::memory_document(source)];
    append_builtin_display(&mut sources, &mut descriptors, &mut names, "std").unwrap();
    let snapshot = analyze_unit_names(sources, descriptors, names).unwrap();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &snapshot.inputs(), names, &types)
        .unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let display = typed
        .signatures()
        .declaration(declaration(names, "Display"))
        .unwrap();
    let UnitCallableTarget::Symbol(requirement) = display.nominal().unwrap().members()[0].target()
    else {
        panic!("Display requirement must have a source symbol");
    };
    let calls = typed.calls();
    let selected = calls
        .iter()
        .filter(|call| call.is_display_requirement())
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 4);
    for call in selected {
        assert_eq!(call.target(), UnitCallTarget::Symbol(requirement));
        let receiver = call.receiver().unwrap();
        assert_eq!(receiver.mode(), ParameterMode::Borrow);
        assert_ne!(receiver.ty(), display.ty());
        assert_eq!(
            typed.types().get(call.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::String))
        );
        assert!(call.borrow_return().is_none());
    }
    assert_eq!(calls.len(), 5);
    assert!(calls.iter().any(|call| !call.is_display_requirement()
        && call.target() != UnitCallTarget::Symbol(requirement)));
}

#[test]
fn unit_interpolation_plan_keeps_source_order_and_canonical_display_target() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "probe.ko",
            r#"class Wrapped() : Display {
    override fun toString(): String = "wrapped"
}
fun <T: Display> render(value: T): String = "a${value}b"
fun mixed(value: Wrapped): String = "${1}-${value}-${"s"}"
fun jump(): Int { val text = "x${2}${return 7}${3}"; return 8 }
"#,
        )
        .unwrap();
    let (mut names, types) = standard_environments();
    let mut descriptors = vec![UnitSourceDescriptor::memory_document(source)];
    append_builtin_display(&mut sources, &mut descriptors, &mut names, "std").unwrap();
    let snapshot = analyze_unit_names(sources, descriptors, names).unwrap();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &snapshot.inputs(), names, &types)
        .unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

    let unit = source_unit(names, source);
    let file = &snapshot.parsed_files()[unit.index()];
    let steps = |text| {
        let expression = expression_with_text(snapshot.sources(), file, text);
        typed
            .interpolation_plan(UnitExpressionId::new(unit, expression))
            .unwrap()
    };
    let plan = |text| {
        steps(text).iter()
            .map(|part| match part {
                UnitInterpolationPart::Text(span) => (snapshot.sources().slice(*span).unwrap(), None),
                UnitInterpolationPart::Format { operand, target } => {
                    let span = file.ast().expressions().get(operand.expression()).unwrap().span();
                    assert_eq!(operand.source_unit(), unit);
                    (snapshot.sources().slice(span).unwrap(), Some(*target))
                }
                UnitInterpolationPart::Exit(operand) => {
                    let span = file.ast().expressions().get(operand.expression()).unwrap().span();
                    assert_eq!(operand.source_unit(), unit);
                    (snapshot.sources().slice(span).unwrap(), None)
                }
            })
            .collect::<Vec<_>>()
    };
    let display = typed.signatures().declaration(declaration(names, "Display")).unwrap();
    let requirement = display.nominal().unwrap().members()[0].target();
    let nominal = Some(UnitDisplayTarget::Requirement(requirement));
    let builtin = |kind| Some(UnitDisplayTarget::Builtin(kind));
    assert_eq!(plan(r#""a${value}b""#), [("a", None), ("value", nominal), ("b", None)]);
    assert_eq!(
        plan(r#""${1}-${value}-${"s"}""#),
        [
            ("1", builtin(BuiltinType::Int)),
            ("-", None),
            ("value", nominal),
            ("-", None),
            (r#""s""#, builtin(BuiltinType::String)),
        ]
    );
    let jumping = r#""x${2}${return 7}${3}""#;
    assert!(matches!(
        steps(jumping).last(),
        Some(UnitInterpolationPart::Exit(_))
    ));
    assert_eq!(
        plan(jumping),
        [("x", None), ("2", builtin(BuiltinType::Int)), ("return 7", None)]
    );
}


#[test]
fn unit_interpolation_plan_excludes_failed_conversion_and_keeps_neighbor() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "bad.ko",
        r#"fun unit(): Unit {}
fun rejected(): String = "bad:${unit()}"
fun accepted(): String = "ok:${true}"
"#,
    );
    let (names, types) = standard_environments();
    let inputs = [SourceUnitInput::new("main", "bad.ko", source, &file)];
    let names = validated_names(&sources, &inputs, &names);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let [error] = typed.diagnostics() else {
        panic!("expected a single operand diagnostic: {:?}", typed.diagnostics());
    };
    assert_eq!(error.code().to_string(), "L0085");
    assert_eq!(sources.slice(error.primary_span()), Ok("unit()"));
    let id = |text| UnitExpressionId::new(
        source_unit(&names, source),
        expression_with_text(&sources, &file, text),
    );
    assert!(typed.interpolation_plan(id(r#""bad:${unit()}""#)).is_none());
    assert!(matches!(
        typed.interpolation_plan(id(r#""ok:${true}""#)),
        Some([UnitInterpolationPart::Text(_), UnitInterpolationPart::Format {
            target: UnitDisplayTarget::Builtin(BuiltinType::Boolean),
            ..
        }])
    ));
}
