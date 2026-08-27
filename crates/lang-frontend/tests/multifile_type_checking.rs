//! SPEC-0197 production compilation-unit body driver integration tests.

use lang_frontend::{
    ast::{ExpressionId, StatementId, TypeRefId},
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, Namespace, SourceUnitId, SourceUnitInput,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{Expression, ParsedFile, Statement, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypes, DeferredReason, DestructuringMode, ExpressionCategory,
        ParameterMode, TypeEnvironment, UnitCallTarget, UnitExpressionId, UnitStatementId,
        UnitTypeKind, UnitTypeRefId, check_compilation_unit_types, standard_environments,
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

fn expressions_with_text(sources: &SourceMap, file: &ParsedFile, text: &str) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect()
}

fn type_refs_with_text(sources: &SourceMap, file: &ParsedFile, text: &str) -> Vec<TypeRefId> {
    file.ast()
        .type_refs()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect()
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

fn if_expressions(file: &ParsedFile) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::If { .. }).then_some(id))
        .collect()
}

fn assignment_expressions(file: &ParsedFile) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Assignment { .. }).then_some(id)
        })
        .collect()
}

fn when_expressions(file: &ParsedFile) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::When { .. }).then_some(id))
        .collect()
}

fn destructuring_statements(file: &ParsedFile) -> Vec<StatementId> {
    file.ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Statement::LocalDestructuring { .. }).then_some(id)
        })
        .collect()
}

fn symbol_named(
    typed: &CompilationUnitTypes,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
) -> UnitSymbolId {
    let symbol = names.names().source_units()[source.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("source-local symbol exists")
        .id();
    typed
        .body_symbol_types()
        .keys()
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol)
        .copied()
        .expect("source-local body symbol has a typed fact")
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
fn nullable_when_does_not_enable_general_null_literals() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "null.ko",
        "package p\nfun unsupported(): Unit { val missing = null }",
    );
    let inputs = [SourceUnitInput::new("root", "p/null.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);

    let error = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect_err("general null literals remain outside this when slice");
    let lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span) = error
    else {
        panic!("expected UnsupportedBody, got {error:?}");
    };
    assert_eq!(sources.slice(span), Ok("null"));
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

#[test]
fn if_control_facts_feed_cross_file_calls_and_survive_input_permutation() {
    let mut sources = SourceMap::new();
    let (library_source, library) =
        parsed(&mut sources, "library.ko", "package p\nfun one(): Int = 1");
    let (caller_source, caller) = parsed(
        &mut sources,
        "caller.ko",
        "package p\n\
         fun choose(flag: Boolean): Int = if (flag) {\n\
         one() + 1\n\
         } else {\n\
         return one()\n\
         }\n\
         fun observe(flag: Boolean): Unit {\n\
         if (flag) one()\n\
         }\n\
         fun complete(flag: Boolean): Int {\n\
         if (flag) { return one() } else { return one() }\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/library.ko", library_source, &library),
        SourceUnitInput::new("root", "p/caller.ko", caller_source, &caller),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward if unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse if unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.calls(), reverse.calls());
    let caller_unit = source_unit(&forward_names, caller_source);
    let ifs = if_expressions(&caller);
    assert_eq!(ifs.len(), 3);
    let int = forward.types().builtin(BuiltinType::Int).expect("Int seed");
    let unit = forward
        .types()
        .builtin(BuiltinType::Unit)
        .expect("Unit seed");
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(caller_unit, ifs[0])),
        Some(int)
    );
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(caller_unit, ifs[1])),
        Some(unit)
    );
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(caller_unit, ifs[2])),
        forward.types().builtin(BuiltinType::Nothing)
    );
    assert_eq!(forward.calls().len(), 5);
    assert!(forward.calls().iter().all(|call| {
        call.target() == UnitCallTarget::Declaration(declaration(&forward_names, "one"))
    }));
}

#[test]
fn if_expected_condition_and_join_errors_recover_without_cascades() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "if-errors.ko",
        "package p\n\
         fun contextual(flag: Boolean): Byte = if (flag) 127 else 128\n\
         fun mixed(flag: Boolean): Unit { val mixedResult = if (flag) { 1 } else { false } }\n\
         fun invalidCondition(): Unit { if (1) {} }\n\
         fun sound(flag: Boolean): Int = if (flag) { 1 } else { 2 }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/if-errors.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("if errors stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0090", "L0089", "L0084"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span belongs to the source"))
            .collect::<Vec<_>>(),
        ["128", "else", "1"]
    );
    let sound = UnitExpressionId::new(
        source_unit(&names, source),
        expression_with_text(&sources, &file, "if (flag) { 1 } else { 2 }"),
    );
    assert_eq!(
        typed
            .expression_type(sound)
            .and_then(|ty| typed.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert!(typed.validate().is_err());
}

#[test]
fn nested_if_return_uses_the_callable_return_annotation() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nested-return.ko",
        "package p\n\
         fun nestedReturn(flag: Boolean): Int {\n\
         val text: String = if (flag) { return false } else { \"ok\" }\n\
         return 1\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/nested-return.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("nested return mismatch stays in the recovery product");

    assert_eq!(typed.body_diagnostics().len(), 1);
    let diagnostic = &typed.body_diagnostics()[0];
    assert_eq!(diagnostic.code().to_string(), "L0084");
    assert_eq!(sources.slice(diagnostic.primary_span()), Ok("false"));
    let label = diagnostic
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("return mismatch labels the callable annotation");
    assert_eq!(sources.slice(label.span()), Ok("Int"));
    assert!(typed.validate().is_err());
}

#[test]
fn cross_file_enum_tests_publish_stable_flow_facts_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\n\
         enum class Shape { Circle(radius: Int), Point }\n\
         class Token {}",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun choose(shape: Shape): Shape =\n\
             if (shape is Shape.Circle) shape else shape\n\
         fun afterExit(shape: Shape): Shape {\n\
             if (shape !is Shape.Circle) { return shape }\n\
             return shape\n\
         }\n\
         fun conjunction(shape: Shape): Boolean =\n\
             shape is Shape.Circle && shape is Shape.Circle\n\
         fun disjunction(shape: Shape): Boolean =\n\
             shape !is Shape.Circle || shape is Shape.Circle\n\
         fun local(initial: Shape): Shape {\n\
             var current = initial\n\
             if (current is Shape.Circle) return current\n\
             return initial\n\
         }\n\
         fun grouped(shape: Shape): Shape =\n\
             if (!(shape !is Shape.Circle)) shape else shape\n\
         fun nullable(input: Token?): Token? =\n\
             if (input is Token) input else input",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward enum-flow unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse enum-flow unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.type_ref_types(), reverse.type_ref_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let root = forward
        .signatures()
        .declaration(declaration(&forward_names, "Shape"))
        .expect("Shape signature")
        .ty();
    let shape_uses = expressions_with_text(&sources, &uses, "shape");
    assert_eq!(shape_uses.len(), 13);
    let actual_kinds = shape_uses
        .iter()
        .map(|&expression| {
            forward
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .and_then(|ty| forward.types().get(ty))
                .expect("every shape use has a type")
        })
        .collect::<Vec<_>>();
    for index in [0, 2, 3, 4, 6, 8, 10, 12] {
        assert_eq!(
            actual_kinds[index],
            &UnitTypeKind::Nominal {
                declaration: declaration(&forward_names, "Shape"),
                arguments: Vec::new(),
            }
        );
    }
    for index in [1, 5, 7, 9, 11] {
        assert!(matches!(
            actual_kinds[index],
            UnitTypeKind::EnumCase { root: case_root, .. } if *case_root == root
        ));
    }
    let case_refs = type_refs_with_text(&sources, &uses, "Shape.Circle");
    assert_eq!(case_refs.len(), 8);
    assert!(case_refs.iter().all(|&type_ref| matches!(
        forward
            .type_ref_type(UnitTypeRefId::new(uses_unit, type_ref))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
    )));
    let current_uses = expressions_with_text(&sources, &uses, "current");
    assert_eq!(current_uses.len(), 2);
    assert_eq!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, current_uses[0]))
            .and_then(|ty| forward.types().get(ty)),
        Some(&UnitTypeKind::Nominal {
            declaration: declaration(&forward_names, "Shape"),
            arguments: Vec::new(),
        })
    );
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, current_uses[1]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
    ));
    let token = forward
        .signatures()
        .declaration(declaration(&forward_names, "Token"))
        .expect("Token signature")
        .ty();
    let input_uses = expressions_with_text(&sources, &uses, "input");
    assert_eq!(input_uses.len(), 3);
    let input_types = input_uses
        .iter()
        .map(|&expression| {
            forward
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .expect("every nullable input use has a type")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        forward.types().get(input_types[0]),
        Some(UnitTypeKind::Nullable(inner)) if *inner == token
    ));
    assert_eq!(input_types[1], token);
    assert_eq!(input_types[2], input_types[0]);
}

#[test]
fn invalid_cross_file_type_tests_and_case_annotations_keep_precise_diagnostics() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\n\
         enum class Shape { Circle, Point }\n\
         class Other {}\n\
         interface Marker",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun unrelated(shape: Shape): Boolean = shape is Other\n\
         fun interfaceTest(shape: Shape): Boolean = shape !is Marker\n\
         fun illegalAnnotation(shape: Shape): Unit {\n\
             val case: Shape.Circle? = shape\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid tests stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0106", "L0106", "L0114"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span belongs to source"))
            .collect::<Vec<_>>(),
        ["is", "!is", "Shape.Circle?"]
    );
    let labels = typed
        .body_diagnostics()
        .iter()
        .map(|diagnostic| {
            diagnostic
                .details()
                .iter()
                .find_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                    DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
                })
                .expect("diagnostic has a source label")
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Other", "Marker", "Shape"]);
    assert!(typed.validate().is_err());
}

#[test]
fn assignment_checks_rhs_before_killing_flow_facts_and_stays_unit_deferred() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nenum class Shape { Circle, Point }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun assigned(initial: Shape): Shape {\n\
             var current = initial\n\
             if (current is Shape.Circle) {\n\
                 current = current\n\
                 return current\n\
             }\n\
             return current\n\
         }\n\
         fun addAssign(inout number: Int): Unit { number += 2 }\n\
         fun subtractAssign(inout number: Int): Unit { number -= 2 }\n\
         fun multiplyAssign(inout number: Int): Unit { number *= 2 }\n\
         fun divideAssign(inout number: Int): Unit { number /= 2 }\n\
         fun remainderAssign(inout number: Int): Unit { number %= 2 }\n\
         fun deferredMismatch(inout number: Int): Unit { number = false }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward assignment unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse assignment unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let root = forward
        .signatures()
        .declaration(declaration(&forward_names, "Shape"))
        .expect("Shape signature")
        .ty();
    let current_uses = expressions_with_text(&sources, &uses, "current");
    assert_eq!(current_uses.len(), 5);
    let current_types = current_uses
        .iter()
        .map(|&expression| {
            forward
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .expect("every current use has a type")
        })
        .collect::<Vec<_>>();
    assert_eq!(current_types[0], root);
    for index in [1, 2] {
        assert!(matches!(
            forward.types().get(current_types[index]),
            Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
        ));
    }
    assert_eq!(current_types[3], root);
    assert_eq!(current_types[4], root);

    let assignments = assignment_expressions(&uses);
    assert_eq!(assignments.len(), 7);
    assert!(assignments.iter().all(|&expression| matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, expression))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Deferred(DeferredReason::Assignment))
    )));
}

#[test]
fn cross_file_value_class_destructuring_publishes_copy_and_consume_facts() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\n\
         value class Pair<A, B>(val first: A, val second: B)\n\
         class Ref\n\
         fun echo(pair: Pair<Int, Int>): Pair<Int, Int> = pair",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun split(copy: Pair<Int, Int>, moved: Pair<Int, String>, ref: Ref): Unit {\n\
             val (a, b) = echo(copy)\n\
             val (c, d) = moved\n\
             val (e, f) = ref\n\
         }\n\
         fun <T: Copyable> splitBound(pair: Pair<T, T>): Unit {\n\
             val (g, h) = pair\n\
         }\n\
         fun <T> splitUnbound(pair: Pair<T, T>): Unit {\n\
             val (i, j) = pair\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward destructuring unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse destructuring unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.destructurings(), reverse.destructurings());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward.calls().len(),
        1,
        "destructuring initializer is checked exactly once"
    );

    let uses_unit = source_unit(&forward_names, uses_source);
    let statements = destructuring_statements(&uses);
    assert_eq!(
        statements.len(),
        5,
        "three concrete and two generic destructurings"
    );
    assert_eq!(forward.destructurings().len(), 4);
    assert_eq!(
        forward
            .destructurings()
            .iter()
            .map(|descriptor| descriptor.mode())
            .collect::<Vec<_>>(),
        [
            DestructuringMode::Copy,
            DestructuringMode::Consume,
            DestructuringMode::Copy,
            DestructuringMode::Consume,
        ]
    );
    assert_eq!(
        forward.destructurings()[0].statement(),
        UnitStatementId::new(uses_unit, statements[0])
    );
    assert_eq!(
        forward.destructurings()[1].statement(),
        UnitStatementId::new(uses_unit, statements[1])
    );
    assert!(
        forward
            .destructuring(UnitStatementId::new(uses_unit, statements[2]))
            .is_none()
    );
    assert_eq!(
        forward.destructurings()[2].statement(),
        UnitStatementId::new(uses_unit, statements[3])
    );
    assert_eq!(
        forward.destructurings()[3].statement(),
        UnitStatementId::new(uses_unit, statements[4])
    );

    let first_component_types = forward.destructurings()[0]
        .components()
        .iter()
        .map(|component| forward.types().get(component.ty()))
        .collect::<Vec<_>>();
    assert_eq!(
        first_component_types,
        [
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        ]
    );
    let moved_component_types = forward.destructurings()[1]
        .components()
        .iter()
        .map(|component| forward.types().get(component.ty()))
        .collect::<Vec<_>>();
    assert_eq!(
        moved_component_types,
        [
            Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
            Some(&UnitTypeKind::Builtin(BuiltinType::String)),
        ]
    );
    assert_eq!(
        forward.destructurings()[0]
            .components()
            .iter()
            .map(|component| component.symbol())
            .collect::<Vec<_>>(),
        [
            symbol_named(&forward, &forward_names, uses_unit, "a"),
            symbol_named(&forward, &forward_names, uses_unit, "b"),
        ]
    );
    for name in ["e", "f"] {
        assert!(matches!(
            forward
                .symbol_type(symbol_named(&forward, &forward_names, uses_unit, name))
                .and_then(|ty| forward.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::Destructuring))
        ));
    }
}

#[test]
fn cross_file_destructuring_arity_reports_l0118_and_keeps_recovery_types() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nvalue class Pair<A, B>(val first: A, val second: B)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun split(pair: Pair<Int, String>): Unit {\n\
             val (one) = pair\n\
             val (x, y, extra) = pair\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("arity errors stay in recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse arity errors stay in recovery product");

    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert!(forward.destructurings().is_empty());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0118", "L0118"]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("pattern span"))
            .collect::<Vec<_>>(),
        ["(one)", "(x, y, extra)"]
    );
    let labels = forward
        .body_diagnostics()
        .iter()
        .map(|diagnostic| {
            diagnostic
                .details()
                .iter()
                .find_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                    DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
                })
                .expect("L0118 labels the value-class declaration")
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Pair", "Pair"]);

    let uses_unit = source_unit(&forward_names, uses_source);
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "one",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "extra",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    assert!(forward.validate().is_err());
}

#[test]
fn cross_file_when_covers_enum_boolean_nullable_and_statement_contexts() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nenum class Shape { Circle, Point }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun classify(shape: Shape): Shape = when (shape) {\n\
             is Shape.Circle -> shape\n\
             is Shape.Point -> shape\n\
         }\n\
         fun negative(shape: Shape): Int = when (shape) {\n\
             !is Shape.Circle -> 0\n\
             is Shape.Circle -> 1\n\
         }\n\
         fun alternatives(choice: Shape): Shape = when (choice) {\n\
             is Shape.Circle, is Shape.Point -> choice\n\
         }\n\
         fun noRemaining(remaining: Shape): Shape = when (remaining) {\n\
             is Shape.Circle -> remaining\n\
             else -> remaining\n\
         }\n\
         fun boolean(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             false -> 0\n\
         }\n\
         fun nullable(shape: Shape?): Int = when (shape) {\n\
             null -> 0\n\
             is Shape.Circle -> 1\n\
             is Shape.Point -> 2\n\
         }\n\
         fun subjectless(flag: Boolean): Int = when {\n\
             flag -> 1\n\
             else -> 0\n\
         }\n\
         fun inferred(flag: Boolean): Unit {\n\
             val mixed = when (flag) { true -> 1; false -> \"text\" }\n\
         }\n\
         fun statement(shape: Shape): Int {\n\
             when (shape) { is Shape.Circle -> shape }\n\
             return 0\n\
         }\n\
         fun nested(flag: Boolean, shape: Shape): Int {\n\
             if (flag) {\n\
                 when (shape) { is Shape.Circle -> shape }\n\
             }\n\
             return 0\n\
         }\n\
         fun terminal(flag: Boolean): Int {\n\
             when (flag) {\n\
                 true -> return 1\n\
                 false -> return 0\n\
             }\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward when unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse when unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let classify_uses = expressions_with_text(&sources, &uses, "shape");
    assert!(classify_uses.len() >= 3);
    let root = forward
        .signatures()
        .declaration(declaration(&forward_names, "Shape"))
        .expect("Shape signature")
        .ty();
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(uses_unit, classify_uses[0])),
        Some(root)
    );
    for expression in &classify_uses[1..3] {
        assert!(matches!(
            forward
                .expression_type(UnitExpressionId::new(uses_unit, *expression))
                .and_then(|ty| forward.types().get(ty)),
            Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
        ));
    }
    let alternative_uses = expressions_with_text(&sources, &uses, "choice");
    assert_eq!(alternative_uses.len(), 2);
    assert!(alternative_uses.iter().all(|expression| {
        forward.expression_type(UnitExpressionId::new(uses_unit, *expression)) == Some(root)
    }));
    let remaining_uses = expressions_with_text(&sources, &uses, "remaining");
    assert_eq!(remaining_uses.len(), 3);
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(uses_unit, remaining_uses[0])),
        Some(root)
    );
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, remaining_uses[1]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root: case_root, .. }) if *case_root == root
    ));
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(uses_unit, remaining_uses[2])),
        Some(root),
        "v0.35 remaining-domain facts are not active under v0.32"
    );
    assert!(matches!(
        forward
            .symbol_type(symbol_named(&forward, &forward_names, uses_unit, "mixed",))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Any))
    ));
    let when_nodes = when_expressions(&uses);
    assert_eq!(when_nodes.len(), 11);
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(uses_unit, when_nodes[8]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Unit))
    ));
}

#[test]
fn cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join() {
    let mut sources = SourceMap::new();
    let (types_source, types) = parsed(
        &mut sources,
        "types.ko",
        "package p\nenum class Shape { Circle, Point }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun missing(shape: Shape): Int = when (shape) {\n\
             is Shape.Circle -> 1\n\
         }\n\
         fun invalid(): Int = when { 1 -> 1; else -> 0 }\n\
         fun repeated(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             true -> 2\n\
         }\n\
         fun misplaced(flag: Boolean): Int = when (flag) {\n\
             else -> 0\n\
             true -> 1\n\
             else -> 2\n\
         }\n\
         fun branchConflict(flag: Boolean, inout number: Int): Unit {\n\
             val result = when (flag) {\n\
                 true -> number = number\n\
                 false -> 0\n\
             }\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/types.ko", types_source, &types),
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("when errors stay in recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse when errors stay in recovery product");

    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0111", "L0107", "L0111", "L0110", "L0109", "L0108", "L0112",
        ]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["when", "1", "when", "true", "else", "else", "0"]
    );
    let missing_case = forward.body_diagnostics()[0]
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("enum non-exhaustiveness labels the missing case");
    assert_eq!(missing_case, "Point");
    assert!(forward.validate().is_err());
}

#[test]
fn cross_file_loops_publish_jump_and_deferred_binding_facts() {
    let mut sources = SourceMap::new();
    let (callee_source, callee) = parsed(
        &mut sources,
        "callee.ko",
        "package p\nfun truth(): Boolean = true",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun loops(): Unit {\n\
             while (truth()) { continue }\n\
             for (item in truth()) {\n\
                 val deferred = item\n\
                 break\n\
             }\n\
             loop { break }\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/callee.ko", callee_source, &callee),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("forward loop unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse loop unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    let uses_unit = source_unit(&forward_names, uses_source);
    for jump in ["continue", "break"] {
        assert!(
            expressions_with_text(&sources, &uses, jump)
                .iter()
                .all(|id| {
                    forward
                        .expression_type(UnitExpressionId::new(uses_unit, *id))
                        .and_then(|ty| forward.types().get(ty))
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
                })
        );
    }
    for symbol in ["item", "deferred"] {
        assert!(matches!(
            forward
                .symbol_type(symbol_named(&forward, &forward_names, uses_unit, symbol,))
                .and_then(|ty| forward.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::LoopSource))
        ));
    }
}

#[test]
fn unit_loop_and_return_diagnostics_match_callable_boundaries() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-loops.ko",
        "package p\n\
         fun invalid(): Unit {\n\
             break\n\
             continue\n\
             while (1) { break }\n\
             loop { break }\n\
             break\n\
             return 1\n\
         }\n\
         fun bare(): Int { return }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/invalid-loops.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("loop and return failures stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0142", "L0142", "L0084", "L0142", "L0087", "L0087"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["break", "continue", "1", "break", "1", "return"]
    );
    assert!(typed.validate().is_err());
}
