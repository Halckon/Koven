//! Ordinary sources; product assertions are green-only, not missing-API red probes.
use super::*;
// BEGIN GREEN
use lang_frontend::type_checking::{
    NullableWhenSubjectCategory, UnitNullableWhenDescriptor, UnitNullableWhenDomain,
};
// END GREEN

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

// BEGIN GREEN
fn assert_entries(fixture: &Fixture, descriptor: &UnitNullableWhenDescriptor) {
    use UnitNullableWhenDomain::{Empty, NonNull, Null, Nullable};
    let node = fixture
        .file
        .ast()
        .expressions()
        .get(descriptor.expression().expression())
        .expect("real When");
    let Expression::When {
        subject: Some(subject),
        entries,
        ..
    } = node.payload()
    else {
        panic!("descriptor belongs to actual subject When");
    };
    assert_eq!(descriptor.expression().source_unit(), fixture.unit);
    assert_eq!(
        descriptor.subject(),
        UnitExpressionId::new(fixture.unit, *subject)
    );
    assert_eq!(
        fixture.typed.expression_type(descriptor.subject()),
        Some(descriptor.nullable_type())
    );
    assert_eq!(
        fixture.typed.types().get(descriptor.nullable_type()),
        Some(&UnitTypeKind::Nullable(descriptor.inner_type()))
    );
    assert_eq!(descriptor.entries().len(), 2);
    assert_eq!(entries.len(), 2);
    for (index, (actual, checked)) in entries.iter().zip(descriptor.entries()).enumerate() {
        assert_eq!(checked.index(), index);
        assert_eq!(
            checked.body(),
            UnitStatementId::new(fixture.unit, actual.body)
        );
        assert_eq!(checked.span(), actual.span);
        assert_eq!(checked.is_else(), actual.else_span.is_some());
    }
    let null = &descriptor.entries()[0];
    assert_eq!(
        (
            null.input_domain(),
            null.body_domain(),
            null.remaining_domain()
        ),
        (Nullable, Null, NonNull)
    );
    let [condition] = null.alternatives() else {
        panic!("one null condition");
    };
    let [lang_frontend::parser::WhenCondition::Expression(expression)] =
        entries[0].conditions.as_slice()
    else {
        panic!("literal null condition");
    };
    assert_eq!(
        condition.expression(),
        UnitExpressionId::new(fixture.unit, *expression)
    );
    assert_eq!(
        condition.span(),
        fixture
            .file
            .ast()
            .expressions()
            .get(*expression)
            .expect("condition node")
            .span()
    );
    assert_eq!(fixture.sources.slice(condition.span()), Ok("null"));
    assert_eq!(
        (
            condition.input_domain(),
            condition.matched_domain(),
            condition.remaining_domain()
        ),
        (Nullable, Null, NonNull)
    );
    let otherwise = &descriptor.entries()[1];
    assert_eq!(
        (
            otherwise.input_domain(),
            otherwise.body_domain(),
            otherwise.remaining_domain()
        ),
        (NonNull, NonNull, Empty)
    );
    assert!(otherwise.alternatives().is_empty());
}

fn assert_witnesses(fixture: &Fixture) {
    let mut expressions = std::collections::BTreeSet::new();
    for witness in fixture.typed.nullable_when_uses() {
        assert!(
            expressions.insert(witness.expression()),
            "no trial duplicate use"
        );
        assert_eq!(
            fixture.typed.nullable_when_use(witness.expression()),
            Some(*witness)
        );
        let descriptor = fixture
            .typed
            .nullable_when(witness.control())
            .expect("real control");
        assert_eq!(witness.subject(), descriptor.subject());
        assert_eq!(Some(witness.symbol()), descriptor.stable_symbol());
        assert_eq!(
            Some(witness.source_generation()),
            descriptor.source_generation()
        );
        assert_ne!(witness.source_generation(), u64::MAX);
        assert_eq!(witness.nullable_type(), descriptor.nullable_type());
        assert_eq!(witness.inner_type(), descriptor.inner_type());
        assert_eq!(
            witness.body(),
            descriptor.entries()[witness.entry_index()].body()
        );
        assert_eq!(
            descriptor.entries()[witness.entry_index()].body_domain(),
            UnitNullableWhenDomain::NonNull
        );
        assert_eq!(
            witness.reference(),
            UnitExpressionId::new(
                fixture.unit,
                canonical_name(&fixture.file, witness.expression().expression())
            )
        );
        let body_span = fixture
            .file
            .ast()
            .statements()
            .get(witness.body().statement())
            .expect("witness body")
            .span();
        let use_span = fixture
            .file
            .ast()
            .expressions()
            .get(witness.expression().expression())
            .expect("witness use")
            .span();
        assert_eq!(body_span.source_id(), use_span.source_id());
        assert!(body_span.start() <= use_span.start() && use_span.end() <= body_span.end());
        let ordinary = fixture
            .typed
            .non_null_use(witness.expression())
            .expect("old real fact");
        assert_eq!(ordinary.symbol(), witness.symbol());
        assert_eq!(ordinary.declared_type(), witness.nullable_type());
        assert_eq!(ordinary.narrowed_type(), witness.inner_type());
        let reference = fixture
            .typed
            .nullable_when_use(witness.reference())
            .expect("leaf witness");
        assert_eq!(reference.reference(), witness.reference());
        assert_eq!(reference.control(), witness.control());
        assert_eq!(reference.body(), witness.body());
        assert_eq!(reference.source_generation(), witness.source_generation());
    }
}
// END GREEN
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
        // BEGIN GREEN
        assert_eq!(fixture.typed.nullable_whens().len(), 4);
        for control in when_expressions(&fixture.file) {
            let descriptor = fixture
                .typed
                .nullable_when(UnitExpressionId::new(fixture.unit, control))
                .expect("descriptor exists even without a use");
            assert_entries(&fixture, descriptor);
            assert!(descriptor.falls_through());
            assert!(descriptor.entries()[0].falls_through());
            let body = fixture
                .file
                .ast()
                .statements()
                .get(descriptor.entries()[1].body().statement())
                .expect("entry body");
            assert_eq!(
                descriptor.entries()[1].falls_through(),
                fixture.sources.slice(body.span()) != Ok("return 1")
            );
            let expected_uses = match fixture.sources.slice(body.span()).expect("body source") {
                "read(value) + read((value))" => 3,
                "{ take((value)); 1 }" => 2,
                "1" | "return 1" => 0,
                other => panic!("unexpected complete body: {other}"),
            };
            assert_eq!(
                fixture
                    .typed
                    .nullable_when_uses()
                    .iter()
                    .filter(|witness| witness.control() == descriptor.expression())
                    .count(),
                expected_uses
            );
            let subject = fixture
                .file
                .ast()
                .expressions()
                .get(descriptor.subject().expression())
                .expect("subject");
            if fixture.sources.slice(subject.span()) == Ok("make()") {
                assert_eq!(
                    descriptor.source_category(),
                    NullableWhenSubjectCategory::Temporary
                );
                assert_eq!(descriptor.stable_symbol(), None);
                assert_eq!(descriptor.subject_reference(), None);
                assert_eq!(descriptor.source_generation(), None);
            } else {
                assert_eq!(
                    descriptor.source_category(),
                    NullableWhenSubjectCategory::OwnedRoot
                );
                assert!(descriptor.stable_symbol().is_some());
                assert_eq!(descriptor.source_generation(), Some(0));
                assert_eq!(
                    descriptor.subject_reference(),
                    Some(UnitExpressionId::new(
                        fixture.unit,
                        canonical_name(&fixture.file, descriptor.subject().expression())
                    ))
                );
            }
        }
        assert_eq!(fixture.typed.nullable_when_uses().len(), 5);
        assert_witnesses(&fixture);
        // END GREEN
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
            // BEGIN GREEN
            let outer = fixture.typed.nullable_when_use(key).expect("Group witness");
            let reference = fixture.typed.nullable_when_use(leaf).expect("Name witness");
            assert_eq!(outer.reference(), reference.expression());
            assert_ne!(outer.expression(), reference.expression());
            assert_eq!(outer.control(), reference.control());
            assert_eq!(outer.source_generation(), reference.source_generation());
            // END GREEN
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
        // BEGIN GREEN
        assert!(
            fixture.typed.nullable_when_use(key).is_none(),
            "old When generation invalid"
        );
        // END GREEN
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
    // BEGIN GREEN
    assert_eq!(fixture.typed.nullable_whens().len(), 2);
    assert!(
        fixture.typed.nullable_when_use(callback_argument).is_none(),
        "lambda must not reuse an outer dynamic When witness"
    );
    let witness = fixture
        .typed
        .nullable_when_use(resumed)
        .expect("outer frame restored");
    assert_eq!(
        fixture
            .typed
            .nullable_when(witness.control())
            .expect("control")
            .source_category(),
        NullableWhenSubjectCategory::BorrowRoot
    );
    assert_eq!(
        fixture.typed.nullable_when_uses().len(),
        2,
        "actual nullable When uses: {:?}",
        fixture
            .typed
            .nullable_when_uses()
            .iter()
            .map(|witness| {
                let expression = fixture
                    .file
                    .ast()
                    .expressions()
                    .get(witness.expression().expression())
                    .expect("real witness expression");
                let control = fixture
                    .file
                    .ast()
                    .expressions()
                    .get(witness.control().expression())
                    .expect("real witness When");
                (
                    *witness,
                    expression.span(),
                    fixture.sources.slice(expression.span()),
                    control.span(),
                    fixture.typed.expression_category(witness.expression()),
                    fixture
                        .typed
                        .nullable_when(witness.control())
                        .map(|descriptor| descriptor.source_category()),
                )
            })
            .collect::<Vec<_>>()
    );
    assert_witnesses(&fixture);
    // END GREEN
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
    // BEGIN GREEN
    assert_eq!(selected.typed.nullable_whens().len(), 2);
    assert_eq!(selected.typed.nullable_when_uses().len(), 2);
    assert_witnesses(&selected);
    // END GREEN
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
    // BEGIN GREEN
    assert!(
        rejected.typed.nullable_whens().is_empty(),
        "earlier successful control cleared too"
    );
    assert!(
        rejected.typed.nullable_when_uses().is_empty(),
        "no trial or prior witness escapes"
    );
    for control in when_expressions(&rejected.file) {
        assert!(
            rejected
                .typed
                .nullable_when(UnitExpressionId::new(rejected.unit, control))
                .is_none()
        );
    }
    // END GREEN
    assert!(rejected.typed.validate().is_err());
}
