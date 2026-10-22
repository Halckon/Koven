//! Ordinary sources; product assertions are green-only, not missing-API red probes.
use super::*;

struct Fixture {
    sources: SourceMap,
    file: ParsedFile,
    names: ValidatedCompilationUnitNames,
    typed: CompilationUnitTypes,
    unit: SourceUnitId,
}

fn checked(text: &str) -> Fixture {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "nullable-when-unit.ko", text);
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-when-unit.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("ordinary source checks internally");
    let unit = source_unit(&names, source);
    Fixture {
        sources,
        file,
        names,
        typed,
        unit,
    }
}

fn valid(fixture: &Fixture) {
    assert!(
        fixture.typed.diagnostics().is_empty(),
        "{:?}",
        fixture.typed.diagnostics()
    );
    assert!(fixture.typed.clone().validate().is_ok());
}

fn canonical_name(file: &ParsedFile, mut expression: ExpressionId) -> ExpressionId {
    loop {
        let node = file
            .ast()
            .expressions()
            .get(expression)
            .expect("real expression");
        match node.payload() {
            Expression::Name => return expression,
            Expression::Group { expression: inner } => expression = *inner,
            other => panic!("fixture requires Name/Group, got {other:?}"),
        }
    }
}

#[test]
fn unit_null_else_descriptors_keep_subject_entry_and_use_identity() {
    for (ty, make) in [
        ("Node", "Node()"),
        ("Box<Cell>", "Box(Cell(7))"),
        ("Rc<Int>", "Rc(7)"),
    ] {
        let text = format!(
            r#"class Node {{}}
value class Cell(val value: Int)
fun make(): {ty}? = {make}
fun read(value: {ty}): Int = 1
fun take(own value: {ty}): Unit {{}}
fun reads(own value: {ty}?): Int = when ((value)) {{
    null -> 0;
    else -> read(value) + read((value))
}}
fun consumes(own value: {ty}?): Int = when (value) {{
    null -> 0;
    else -> {{ take((value)); 1 }}
}}
fun unnamed(): Int = when (make()) {{ null -> 0; else -> 1 }}
fun noUse(own value: {ty}?): Int = when (value) {{ null -> 0; else -> return 1 }}
"#
        );
        let fixture = checked(&text);
        valid(&fixture);
        assert_eq!(when_expressions(&fixture.file).len(), 4);
        assert_eq!(fixture.typed.non_null_uses().len(), 5);
        for group in expressions_with_text(&fixture.sources, &fixture.file, "(value)") {
            let key = UnitExpressionId::new(fixture.unit, group);
            if fixture.typed.expression_type(key).is_some_and(|ty| {
                matches!(
                    fixture.typed.types().get(ty),
                    Some(UnitTypeKind::Nullable(_))
                )
            }) {
                assert!(fixture.typed.non_null_use(key).is_none());
                continue;
            }
            let leaf = UnitExpressionId::new(fixture.unit, canonical_name(&fixture.file, group));
            assert!(fixture.typed.non_null_use(key).is_some());
            assert!(fixture.typed.non_null_use(leaf).is_some());
        }
    }
}

#[test]
fn unit_when_witness_rejects_replaced_source_and_callable_capture() {
    let fixture = checked(
        r#"class Node {}
fun read(value: Node): Int = 1
fun changed(own input: Node?, own replacement: Node?): Int {
    var x = input
    return (when (x) {
        null -> 0;
        else -> { x = replacement; if (x == null) { 0 } else { read((x)) } }
    })
}
fun captured(value: Node?): Int = when (value) {
    null -> 0;
    else -> { val callback: () -> Int = { read(value) }; read((value)) }
}
"#,
    );
    valid(&fixture);
    assert_eq!(when_expressions(&fixture.file).len(), 2);
    let replaced = expression_with_text(&fixture.sources, &fixture.file, "(x)");
    let leaf = canonical_name(&fixture.file, replaced);
    for expression in [replaced, leaf] {
        let key = UnitExpressionId::new(fixture.unit, expression);
        assert!(
            fixture.typed.non_null_use(key).is_some(),
            "new inner If proof stays legal"
        );
    }
    let callback_read = expression_with_text(&fixture.sources, &fixture.file, "read(value)");
    let node = fixture
        .file
        .ast()
        .expressions()
        .get(callback_read)
        .expect("callback call");
    let Expression::Call { arguments, .. } = node.payload() else {
        panic!("real call");
    };
    let callback_argument = UnitExpressionId::new(fixture.unit, arguments[0].value);
    assert!(fixture.typed.non_null_use(callback_argument).is_some());
    let resumed = expression_with_text(&fixture.sources, &fixture.file, "(value)");
    let resumed = UnitExpressionId::new(fixture.unit, resumed);
    assert!(fixture.typed.non_null_use(resumed).is_some());
}

#[test]
fn unit_when_trials_and_type_errors_clear_executable_products() {
    let selected = checked(
        r#"class Node {}
fun read(value: Node): Int = 1
fun choose(action: () -> Int): Int = 1
fun choose(action: () -> String): String = "unused"
fun selected(value: Node?): Int = choose({
    when (value) { null -> 0; else -> read(value) }
})
fun resumed(own value: Node?): Int = when (value) {
    null -> 0;
    else -> { val result = choose({ read(value) }); read(value) }
}
"#,
    );
    valid(&selected);
    assert_eq!(when_expressions(&selected.file).len(), 2);
    let mut calls = 0;
    for (id, node) in selected.file.ast().expressions().iter() {
        let Expression::Call { callee, .. } = node.payload() else {
            continue;
        };
        let callee = selected
            .file
            .ast()
            .expressions()
            .get(*callee)
            .expect("callee");
        if selected.sources.slice(callee.span()) != Ok("choose") {
            continue;
        }
        calls += 1;
        let descriptor = selected
            .typed
            .call(UnitExpressionId::new(selected.unit, id))
            .expect("only successful overload is published");
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            panic!("actual selected declaration");
        };
        let declaration = selected
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.id() == target)
            .expect("selected declaration");
        let item = selected
            .file
            .ast()
            .items()
            .get(declaration.root())
            .expect("real declaration");
        assert_eq!(
            selected.sources.slice(item.span()),
            Ok("fun choose(action: () -> Int): Int = 1")
        );
        assert_eq!(
            selected.typed.types().get(descriptor.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    assert_eq!(calls, 2);
    let rejected_text = r#"class Node {}
fun read(value: Node): Int = 1
fun choose(action: () -> Int): Int = 1
fun choose(action: () -> String): String = "unused"
fun prior(own value: Node?): Int = when (value) { null -> 0; else -> read(value) }
fun rejected(value: Node?): Unit {
    val result = choose({ when (value) { null -> false; else -> true } })
}
"#;
    let rejected = checked(rejected_text);
    let [diagnostic] = rejected.typed.diagnostics() else {
        panic!("one no-match diagnostic");
    };
    assert_eq!(diagnostic.code().to_string(), "L0123");
    assert_eq!(
        diagnostic.message(),
        "no overload matches the call arguments"
    );
    assert_eq!(
        rejected.sources.slice(diagnostic.primary_span()),
        Ok("choose")
    );
    assert_eq!(rejected_text.find("choose({"), Some(271));
    assert_eq!(
        (
            diagnostic.primary_span().start(),
            diagnostic.primary_span().end()
        ),
        (271, 277)
    );
    assert!(diagnostic.details().is_empty());
    assert!(
        !rejected.typed.non_null_uses().is_empty(),
        "old recovery contract remains"
    );
    assert!(rejected.typed.validate().is_err());
}
