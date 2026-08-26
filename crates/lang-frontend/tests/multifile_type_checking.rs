//! SPEC-0197 production compilation-unit body driver integration tests.

use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, Namespace, SourceUnitId, SourceUnitInput,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, ExpressionCategory, ParameterMode, TypeEnvironment, UnitCallTarget,
        UnitExpressionId, UnitTypeKind, check_compilation_unit_types, standard_environments,
    },
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn validated_names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> ValidatedCompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names")
}

fn source_unit(names: &ValidatedCompilationUnitNames, source: SourceId) -> SourceUnitId {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == source)
        .expect("source belongs to unit")
        .id()
}

fn declaration(names: &ValidatedCompilationUnitNames, name: &str) -> DeclarationId {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == name)
        .expect("declaration exists")
        .id()
}

fn expression_with_text(sources: &SourceMap, file: &ParsedFile, text: &str) -> ExpressionId {
    file.ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()) == Ok(text))
        .expect("expression text exists")
        .0
}

fn symbol_for_expression(
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    file: &ParsedFile,
    expression: ExpressionId,
) -> UnitSymbolId {
    let span = file
        .ast()
        .expressions()
        .get(expression)
        .expect("expression belongs to file")
        .span();
    names
        .names()
        .references()
        .iter()
        .find_map(|reference| {
            (reference.source_unit() == source
                && reference.span() == span
                && reference.namespace() == Some(Namespace::Value))
            .then(|| match reference.target() {
                UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                _ => None,
            })
            .flatten()
        })
        .expect("expression resolves to a source-local symbol")
}

#[test]
fn cross_file_named_call_publishes_declaration_and_argument_facts() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "declarations.ko",
        "package p\nfun choose(own first: Int, second: Long): String = \"chosen\"",
    );
    let (use_source, use_file) = parsed(
        &mut sources,
        "use.ko",
        "package p\nfun use(): String = choose(second = 2L, first = 1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/use.ko", use_source, &use_file),
        SourceUnitInput::new(
            "root",
            "p/declarations.ko",
            declarations_source,
            &declarations,
        ),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit type checking succeeds internally");

    assert!(typed.diagnostics().is_empty());
    assert!(typed.clone().validate().is_ok());
    let call = expression_with_text(&sources, &use_file, "choose(second = 2L, first = 1)");
    let call = UnitExpressionId::new(source_unit(&names, use_source), call);
    let descriptor = typed.call(call).expect("cross-file call fact");
    assert_eq!(
        descriptor.target(),
        UnitCallTarget::Declaration(declaration(&names, "choose"))
    );
    assert_eq!(
        descriptor
            .arguments()
            .iter()
            .map(|argument| argument.parameter_index())
            .collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(
        descriptor
            .arguments()
            .iter()
            .map(|argument| argument.mode())
            .collect::<Vec<_>>(),
        [ParameterMode::Borrow, ParameterMode::Value]
    );
    assert!(
        descriptor
            .arguments()
            .iter()
            .all(|argument| argument.category() == ExpressionCategory::Temporary)
    );
    assert_eq!(typed.expression_type(call), Some(descriptor.return_type()));
    assert_eq!(
        typed.types().get(descriptor.return_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::String))
    );
}

#[test]
fn body_error_recovers_and_preserves_independent_source_qualified_facts() {
    let mut sources = SourceMap::new();
    let (broken_source, broken) = parsed(
        &mut sources,
        "broken.ko",
        "package p\nfun broken(): String = 1",
    );
    let (sound_source, sound) = parsed(
        &mut sources,
        "sound.ko",
        "package p\nfun sound(): Long = 2L",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/sound.ko", sound_source, &sound),
        SourceUnitInput::new("root", "p/broken.ko", broken_source, &broken),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("body errors stay in recovery product");
    let broken_local = expression_with_text(&sources, &broken, "1");
    let sound_local = expression_with_text(&sources, &sound, "2L");
    assert_eq!(broken_local.index(), sound_local.index());
    let broken_id = UnitExpressionId::new(source_unit(&names, broken_source), broken_local);
    let sound_id = UnitExpressionId::new(source_unit(&names, sound_source), sound_local);

    assert_ne!(broken_id, sound_id);
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"]
    );
    assert!(matches!(
        typed
            .expression_type(broken_id)
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    assert_eq!(
        typed
            .expression_type(sound_id)
            .and_then(|ty| typed.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Long))
    );
    assert!(typed.validate().is_err());
}

#[test]
fn signature_error_still_checks_an_independent_body() {
    let mut sources = SourceMap::new();
    let (first_source, first) = parsed(
        &mut sources,
        "first.ko",
        "package p\nfun clash(input: Int): Int = input",
    );
    let (second_source, second) = parsed(
        &mut sources,
        "second.ko",
        "package p\nfun clash(other: Int): String = \"x\"",
    );
    let (sound_source, sound) = parsed(
        &mut sources,
        "sound.ko",
        "package p\nfun sound(): Long = 2L",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/second.ko", second_source, &second),
        SourceUnitInput::new("root", "p/sound.ko", sound_source, &sound),
        SourceUnitInput::new("root", "p/first.ko", first_source, &first),
    ];
    let (name_environment, type_environment): (NameEnvironment, TypeEnvironment) =
        standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("signature error stays in recovery product");

    assert_eq!(
        typed
            .signatures()
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0097"]
    );
    let sound = UnitExpressionId::new(
        source_unit(&names, sound_source),
        expression_with_text(&sources, &sound, "2L"),
    );
    assert_eq!(
        typed
            .expression_type(sound)
            .and_then(|ty| typed.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Long))
    );
    assert!(typed.validate().is_err());
}

#[test]
fn cross_file_call_inside_block_return_does_not_report_missing_return() {
    let mut sources = SourceMap::new();
    let (answer_source, answer) = parsed(
        &mut sources,
        "answer.ko",
        "package p\nfun answer(): Int = 1",
    );
    let (relay_source, relay) = parsed(
        &mut sources,
        "relay.ko",
        "package p\nfun relay(): Int { return answer() }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/relay.ko", relay_source, &relay),
        SourceUnitInput::new("root", "p/answer.ko", answer_source, &answer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("block body is supported");

    assert!(typed.diagnostics().is_empty());
    let call = UnitExpressionId::new(
        source_unit(&names, relay_source),
        expression_with_text(&sources, &relay, "answer()"),
    );
    assert_eq!(
        typed.call(call).map(|descriptor| descriptor.target()),
        Some(UnitCallTarget::Declaration(declaration(&names, "answer")))
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn body_facts_and_diagnostics_are_stable_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (z_source, z_file) = parsed(
        &mut sources,
        "z.ko",
        "package p\nfun use(): String = choose(second = 2L, first = 1)\nfun badZ(): String = 3",
    );
    let (a_source, a_file) = parsed(
        &mut sources,
        "a.ko",
        "package p\nfun choose(own first: Int, second: Long): String = \"ok\"\nfun badA(): Int = false",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/z.ko", z_source, &z_file),
        SourceUnitInput::new("root", "p/a.ko", a_source, &a_file),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward unit succeeds internally");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse unit succeeds internally");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert!(!forward.is_same_analysis(&reverse));
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084", "L0084"]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("valid span"))
            .collect::<Vec<_>>(),
        ["false", "3"]
    );
}

#[test]
fn unsupported_executable_declaration_cannot_validate_silently() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "state.ko", "package p\nval state: Int = 1");
    let inputs = [SourceUnitInput::new("root", "p/state.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);

    assert!(matches!(
        check_compilation_unit_types(&sources, &inputs, &names, &type_environment),
        Err(lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(_))
    ));
}

#[test]
fn unit_literal_expected_type_keeps_numeric_range_diagnostic() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "range.ko", "package p\nfun bad(): Byte = 128");
    let inputs = [SourceUnitInput::new("root", "p/range.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("range failure is a source diagnostic");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0090"]
    );
    assert!(typed.validate().is_err());
}

#[test]
fn local_operator_facts_feed_a_cross_file_call_and_survive_input_permutation() {
    let mut sources = SourceMap::new();
    let (callee_source, callee) = parsed(
        &mut sources,
        "callee.ko",
        "package p\nfun addOne(number: Int): Int = number + 1",
    );
    let (caller_source, caller) = parsed(
        &mut sources,
        "caller.ko",
        "package p\nfun answer(): Int {\nval base: Int = 40 + 1\nreturn addOne(base)\n}",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/caller.ko", caller_source, &caller),
        SourceUnitInput::new("root", "p/callee.ko", callee_source, &callee),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward local/operator unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse local/operator unit succeeds");

    assert!(forward.diagnostics().is_empty());
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.calls(), reverse.calls());

    let caller_unit = source_unit(&forward_names, caller_source);
    let base_expression = expression_with_text(&sources, &caller, "base");
    let base_symbol = symbol_for_expression(&forward_names, caller_unit, &caller, base_expression);
    let int = forward.types().builtin(BuiltinType::Int).expect("Int seed");
    assert_eq!(forward.symbol_type(base_symbol), Some(int));
    assert_eq!(
        forward
            .type_ref_types()
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [int]
    );
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(
            caller_unit,
            expression_with_text(&sources, &caller, "40 + 1")
        )),
        Some(int)
    );
    let call = UnitExpressionId::new(
        caller_unit,
        expression_with_text(&sources, &caller, "addOne(base)"),
    );
    assert_eq!(
        forward.call(call).map(|descriptor| descriptor.target()),
        Some(UnitCallTarget::Declaration(declaration(
            &forward_names,
            "addOne"
        )))
    );
    assert_eq!(
        forward.call(call).expect("call fact").arguments()[0].category(),
        ExpressionCategory::Place
    );
}

#[test]
fn basic_operator_errors_keep_l0085_and_signed_minimum_is_in_range() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "operators.ko",
        "package p\nfun minimum(): Int = -2147483648\nfun badPrefix(): Boolean = !1\nfun badBinary(): Int = 1 + false",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/operators.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("operator type errors stay in recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0085", "L0085"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("operator span belongs to source"))
            .collect::<Vec<_>>(),
        ["!", "+"]
    );
    let minimum = UnitExpressionId::new(
        source_unit(&names, source),
        expression_with_text(&sources, &file, "-2147483648"),
    );
    assert_eq!(
        typed
            .expression_type(minimum)
            .and_then(|ty| typed.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert!(typed.validate().is_err());
}

#[test]
fn local_annotation_error_recovers_declared_type_and_checks_an_independent_local() {
    let mut sources = SourceMap::new();
    let (broken_source, broken) = parsed(
        &mut sources,
        "broken-local.ko",
        "package p\nfun broken(): String {\nval text: String = 1\nreturn text\n}",
    );
    let (sound_source, sound) = parsed(
        &mut sources,
        "sound-local.ko",
        "package p\nfun sound(): Int {\nval number = 2\nreturn number\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/sound-local.ko", sound_source, &sound),
        SourceUnitInput::new("root", "p/broken-local.ko", broken_source, &broken),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("local mismatch stays in recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"]
    );
    assert_eq!(
        sources.slice(typed.body_diagnostics()[0].primary_span()),
        Ok("1")
    );
    let broken_unit = source_unit(&names, broken_source);
    let sound_unit = source_unit(&names, sound_source);
    let text = expression_with_text(&sources, &broken, "text");
    let number = expression_with_text(&sources, &sound, "number");
    let text_symbol = symbol_for_expression(&names, broken_unit, &broken, text);
    let number_symbol = symbol_for_expression(&names, sound_unit, &sound, number);
    let string = typed
        .types()
        .builtin(BuiltinType::String)
        .expect("String seed");
    let int = typed.types().builtin(BuiltinType::Int).expect("Int seed");
    assert_eq!(typed.symbol_type(text_symbol), Some(string));
    assert_eq!(typed.symbol_type(number_symbol), Some(int));
    assert!(typed.validate().is_err());
}
