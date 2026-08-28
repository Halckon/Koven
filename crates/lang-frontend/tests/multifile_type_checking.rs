//! SPEC-0197 production compilation-unit body driver integration tests.

use lang_frontend::{
    ast::{ExpressionId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, Namespace, SourceUnitId, SourceUnitInput, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{Expression, ParsedFile, Statement, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ContainerConstructionKind, DeferredReason,
        DestructuringMode, EnvironmentFunction, EnvironmentFunctionEffect, EnvironmentParameter,
        EnvironmentType, ExpressionCategory, IntrinsicTypeConstructor, ParameterMode,
        RcOperationKind, SequentialContainerKind, TypeEnvironment, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitCallTarget, UnitConstructionTarget, UnitExpressionId,
        UnitStatementId, UnitTypeKind, UnitTypeRefId, check_compilation_unit_types,
        standard_environments,
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
fn cross_file_source_constructions_publish_instantiated_value_mappings() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Marker<T> {}\n\
         value class Pair<T>(val first: T, val second: T)\n\
         enum class Maybe<T> { Some(item: T), None }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun marker(): Marker<Int> = Marker()\n\
         fun pair(): Pair<Int> = Pair(second = 2, first = 1)\n\
         fun some(): Maybe<Int> = Maybe.Some(3)\n\
         fun none(): Maybe<Int> = Maybe.None",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("source constructions type check internally");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed source constructions type check internally");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.diagnostics(), reverse.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.constructions(), reverse.constructions());
    assert!(typed.calls().is_empty());
    assert_eq!(typed.constructions().len(), 4);
    assert_eq!(
        typed.constructions()[0].target(),
        UnitConstructionTarget::Nominal(declaration(&names, "Marker"))
    );
    let pair = &typed.constructions()[1];
    assert_eq!(pair.instance().type_arguments().len(), 1);
    assert_eq!(
        pair.arguments()
            .iter()
            .map(|argument| (
                argument.parameter_name(),
                argument.evaluation_index(),
                argument.mode(),
            ))
            .collect::<Vec<_>>(),
        [
            ("first", 1, ParameterMode::Value),
            ("second", 0, ParameterMode::Value),
        ]
    );
    assert!(matches!(
        typed.constructions()[2].target(),
        UnitConstructionTarget::EnumCase(_)
    ));
    assert!(typed.constructions()[3].arguments().is_empty());
    for descriptor in typed.constructions() {
        assert_eq!(
            typed.expression_type(descriptor.expression()),
            Some(descriptor.result_type())
        );
        assert!(descriptor.arguments().iter().all(|argument| {
            argument
                .parameter_symbol()
                .expect("source construction parameter symbol")
                .source_unit()
                == source_unit(&names, models_source)
                && argument.argument().source_unit() == source_unit(&names, uses_source)
                && argument.category() == ExpressionCategory::Temporary
        }));
    }
}

#[test]
fn unit_intrinsic_box_and_rc_publish_complete_value_construction_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         class Resource {}",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun values(): Unit {\n\
             val inferredBox = Box(Point(1))\n\
             val explicitBox = Box<Point>(Point(2))\n\
             val inferredRc = Rc(Resource())\n\
             val explicitRc = Rc<Resource>(Resource())\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("intrinsic constructions type check internally");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed intrinsic constructions type check internally");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.constructions(), reverse.constructions());
    let intrinsic = typed
        .constructions()
        .iter()
        .filter(|descriptor| {
            matches!(
                descriptor.target(),
                UnitConstructionTarget::IntrinsicBox | UnitConstructionTarget::IntrinsicRc
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(intrinsic.len(), 4);
    assert_eq!(
        intrinsic
            .iter()
            .map(|descriptor| descriptor.target())
            .collect::<Vec<_>>(),
        [
            UnitConstructionTarget::IntrinsicBox,
            UnitConstructionTarget::IntrinsicBox,
            UnitConstructionTarget::IntrinsicRc,
            UnitConstructionTarget::IntrinsicRc,
        ]
    );
    for descriptor in intrinsic {
        assert_eq!(descriptor.instance().type_arguments().len(), 1);
        assert_eq!(descriptor.arguments().len(), 1);
        let argument = &descriptor.arguments()[0];
        assert_eq!(argument.parameter_symbol(), None);
        assert_eq!(argument.mode(), ParameterMode::Value);
        assert_eq!(argument.parameter_index(), 0);
        assert_eq!(argument.evaluation_index(), 0);
        assert_eq!(argument.category(), ExpressionCategory::Temporary);
        assert_eq!(
            argument.parameter_type(),
            descriptor.instance().type_arguments()[0]
        );
        assert_eq!(
            argument.parameter_name(),
            if descriptor.target() == UnitConstructionTarget::IntrinsicBox {
                "element"
            } else {
                "value"
            }
        );
        let constructor = if descriptor.target() == UnitConstructionTarget::IntrinsicBox {
            IntrinsicTypeConstructor::Box
        } else {
            IntrinsicTypeConstructor::Rc
        };
        assert_eq!(
            typed.types().get(descriptor.result_type()),
            Some(&UnitTypeKind::Intrinsic {
                constructor,
                arguments: descriptor.instance().type_arguments().to_vec(),
            })
        );
        assert_eq!(
            typed.expression_type(descriptor.expression()),
            Some(descriptor.result_type())
        );
    }
}

#[test]
fn invalid_unit_intrinsic_constructions_reuse_diagnostics_without_partial_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\nvalue class Point(val x: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun invalidBox(): Unit { Box(1) }\n\
         fun boxTypeArity(): Unit { Box<Int, Long>(1) }\n\
         fun invalidRc(): Unit { Rc<Nothing>(error(\"stop\")) }\n\
         fun rcTypeArity(): Unit { Rc<Int, Long>(1) }\n\
         fun missingOperand(): Unit { Rc() }\n\
         fun wrongMode(): Unit { Box(borrow 1) }\n\
         fun wrongOperand(): Unit { Box<Point>(true) }\n\
         fun wrongResult(): String = Rc(1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid intrinsic constructions stay recoverable");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed invalid intrinsic constructions stay recoverable");

    assert_eq!(typed.body_diagnostics(), reverse.body_diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0117", "L0091", "L0125", "L0091", "L0121", "L0122", "L0084", "L0084"
        ]
    );
    assert!(typed.constructions().is_empty());
    assert!(reverse.constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn source_box_and_rc_names_never_gain_unit_intrinsic_identity() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Box(val item: Int)\n\
         class Rc(val item: Int) { fun share(): Rc = this }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun values(): Unit {\n\
             val boxed = Box(1)\n\
             val counted = Rc(2)\n\
             val retained = counted.share()\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("source names retain nominal construction identity");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed
            .constructions()
            .iter()
            .map(|descriptor| descriptor.target())
            .collect::<Vec<_>>(),
        [
            UnitConstructionTarget::Nominal(declaration(&names, "Box")),
            UnitConstructionTarget::Nominal(declaration(&names, "Rc")),
        ]
    );
    assert!(typed.constructions().iter().all(|descriptor| {
        descriptor.arguments()[0]
            .parameter_symbol()
            .is_some_and(|symbol| symbol.source_unit() == source_unit(&names, models_source))
    }));
    assert!(typed.rc_operations().is_empty());
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
            .count(),
        1
    );
}

#[test]
fn deferred_expected_types_do_not_reject_complete_unit_intrinsic_constructions() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         fun values(): Unit {\n\
             val boxed: Opaque = Box(Point(1))\n\
             val counted: Opaque = Rc(2)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("fresh unbound external type");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("deferred expected type remains recoverable");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed
            .constructions()
            .iter()
            .filter(|descriptor| matches!(
                descriptor.target(),
                UnitConstructionTarget::IntrinsicBox | UnitConstructionTarget::IntrinsicRc
            ))
            .count(),
        2
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn intrinsic_construction_overload_lambda_trials_commit_only_the_unique_candidate() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         fun choose(action: () -> Box<Point>): Int = 1\n\
         fun choose(action: () -> Rc<Point>): Long = 1L\n\
         fun selected(): Int = choose({ Box(Point(1)) })",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("intrinsic construction trial is isolated");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.constructions().len(), 2);
    assert!(matches!(
        typed.constructions()[0].target(),
        UnitConstructionTarget::Nominal(_)
    ));
    assert_eq!(
        typed.constructions()[1].target(),
        UnitConstructionTarget::IntrinsicBox
    );
    assert!(
        typed
            .constructions()
            .iter()
            .all(|descriptor| { descriptor.target() != UnitConstructionTarget::IntrinsicRc })
    );
}

#[test]
fn invalid_cross_file_source_constructions_keep_existing_diagnostics_and_no_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Service\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         class Resource {}\n\
         class C {}\n\
         class Pair(val first: Int, val second: Int)\n\
         class Mixed<T>(val inferred: T, val fixed: Int)\n\
         class Partial<A, B>(val a: A)\n\
         class Empty<T> {}\n\
         class NeedsCopy<T: Copyable> {}",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun invalidService(): Unit { Service() }\n\
         fun invalidEnum(): Unit { Maybe<Int>() }\n\
         fun named(): Unit { Pair(missing = 1, second = 2) }\n\
         fun arity(): Unit { Pair(1) }\n\
         fun mode(): Unit { Pair(borrow 1, 2) }\n\
         fun typed(): Unit { Pair(true, 2) }\n\
         fun inferredThenTyped(): Unit { Mixed(1, true) }\n\
         fun badResult(): Int = C()\n\
         fun partial(): Unit { Partial(1) }\n\
         fun underconstrained(): Unit { Empty() }\n\
         fun bound(): Unit { NeedsCopy<Resource>() }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("construction errors stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0143", "L0143", "L0120", "L0121", "L0122", "L0084", "L0084", "L0084", "L0144",
            "L0144", "L0115"
        ]
    );
    let partial = typed
        .body_diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.code().to_string() == "L0144"
                && sources.slice(diagnostic.primary_span()) == Ok("Partial")
        })
        .expect("Partial inference diagnostic");
    assert_eq!(
        partial
            .details()
            .iter()
            .filter_map(|detail| match detail {
                DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .collect::<Vec<_>>(),
        ["B"]
    );
    assert!(typed.constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn deferred_explicit_constructor_type_arguments_publish_no_construction_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         class Holder<T> {}\n\
         class Wrapper<T> {}\n\
         class Consumer<T>(val item: T)\n\
         class Broken(val item: Opaque)\n\
         fun explicit(): Unit { Holder<Opaque>() }\n\
         fun broken(): Unit { Broken(1) }\n\
         fun inferred(): Consumer<Wrapper<Opaque>> {\n\
             val wrapped: Wrapper<Opaque> = error(\"stop\")\n\
             return Consumer(wrapped)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("fresh unbound external type");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unbound explicit type remains recoverable");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.constructions().is_empty());
}

#[test]
fn constructor_checks_operands_before_rejecting_its_result_type() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\nclass C(val item: Int)\nfun bad(): String = C(true)",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("operand mismatch remains a recoverable diagnostic");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).expect("span"),
            ))
            .collect::<Vec<_>>(),
        [("L0084".to_owned(), "true")]
    );
    assert!(typed.constructions().is_empty());
}

#[test]
fn invalid_nested_container_operand_stops_outer_construction_before_result_mismatch() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         value class Point(val x: Int)\n\
         class Holder(val item: List<Int>)\n\
         fun bad(): String = Holder(List(1))",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid nested container remains a recoverable diagnostic");
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).expect("span"),
            ))
            .collect::<Vec<_>>(),
        [("L0091".to_owned(), "List(1)")]
    );
    assert!(typed.container_constructions().is_empty());
    assert!(typed.constructions().is_empty());
}

#[test]
fn rejected_constructor_inference_still_checks_every_container_operand() {
    for (text, expected_codes) in [
        (
            "package p\n\
         class Resource {}\n\
         value class Point(val x: Int)\n\
         class NeedsCopy<T: Copyable>(val item: Int)\n\
         fun bad(): Unit { NeedsCopy<Resource>(List(1)) }",
            vec!["L0115".to_owned(), "L0091".to_owned()],
        ),
        (
            "package p\n\
         value class Point(val x: Int)\n\
         class Holder<T>(val action: () -> T)\n\
         fun bad(): Unit { Holder({ List(1) }) }",
            vec!["L0144".to_owned(), "L0091".to_owned()],
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "uses.ko", text);
        let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("rejected construction still checks invalid container operands");
        assert_eq!(
            typed
                .body_diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes
        );
        assert!(typed.container_constructions().is_empty());
        assert!(typed.constructions().is_empty());
    }
}

#[test]
fn cross_file_constructor_overload_trials_are_transactional_and_ignore_candidate_expected() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Marker<T> {}\n\
         fun choose(action: () -> Marker<Int>): Int = 1\n\
         fun choose(action: () -> Marker<Long>): Long = 1L",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun selected(): Int = choose({ Marker<Int>() })\n\
         fun rejected(): Unit { val result = choose({ Marker() }) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("overload trials remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0123"]
    );
    assert_eq!(typed.constructions().len(), 1);
    assert_eq!(
        typed.constructions()[0].instance().type_arguments().len(),
        1
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
fn top_level_initializer_is_checked_instead_of_silently_skipped() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "state.ko", "package p\nval state: Int = 1");
    let inputs = [SourceUnitInput::new("root", "p/state.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);

    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("top-level initializer is supported");
    let unit = source_unit(&names, source);
    let state = symbol_named(&typed, &names, unit, "state");
    let initializer = expression_with_text(&sources, &file, "1");
    assert!(matches!(
        typed
            .types()
            .get(typed.symbol_type(state).expect("state symbol type")),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert_eq!(
        typed.expression_type(UnitExpressionId::new(unit, initializer)),
        typed.symbol_type(state)
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn uncontextual_null_literals_report_l0083() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "null.ko",
        "package p\nfun unsupported(): Unit { val missing = null }",
    );
    let inputs = [SourceUnitInput::new("root", "p/null.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);

    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("uncontextual null remains a recoverable inference error");
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).expect("span"),
            ))
            .collect::<Vec<_>>(),
        [("L0083".to_owned(), "null")]
    );
    assert!(typed.validate().is_err());
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

#[test]
fn cross_file_lambdas_publish_expected_contract_and_callable_boundaries() {
    let mut sources = SourceMap::new();
    let (callee_source, callee) = parsed(
        &mut sources,
        "callee.ko",
        "package p\nfun apply(callback: (borrow Int) -> Int): Unit",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun lambdas(): Int {\n\
             val reader: (borrow Int) -> Int = { borrowed -> borrowed }\n\
             val owner: (own Int) -> Int = { owned -> owned }\n\
             val writer: (inout Int) -> Int = { changed -> changed }\n\
             val moved: move (borrow Int) -> Int = move { captured -> captured }\n\
             val inferred = { 1 }\n\
             val boundary: () -> Unit = {\n\
                 loop { break }\n\
                 return\n\
             }\n\
             apply({ applied -> applied })\n\
             return 1\n\
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
            .expect("forward lambda unit succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reverse lambda unit succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward.body_parameter_modes(),
        reverse.body_parameter_modes()
    );
    let uses_unit = source_unit(&forward_names, uses_source);
    for (name, mode) in [
        ("borrowed", ParameterMode::Borrow),
        ("owned", ParameterMode::Value),
        ("changed", ParameterMode::Inout),
        ("captured", ParameterMode::Borrow),
        ("applied", ParameterMode::Borrow),
    ] {
        let symbol = symbol_named(&forward, &forward_names, uses_unit, name);
        assert_eq!(forward.body_parameter_mode(symbol), Some(mode));
        assert_eq!(
            forward
                .symbol_type(symbol)
                .and_then(|ty| forward.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    assert!(matches!(
        forward
            .symbol_type(symbol_named(
                &forward,
                &forward_names,
                uses_unit,
                "inferred",
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Function {
            move_only: false,
            parameters,
            return_type,
        }) if parameters.is_empty()
            && forward.types().get(*return_type)
                == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-lambdas.ko",
        "package p\n\
         fun takes(callback: (borrow Int) -> Int): Unit\n\
         fun invalid(): Unit {\n\
             val wrongMove: (Int) -> Int = move { item -> item }\n\
             val wrongArity: (Int) -> Int = { 1 }\n\
             val uninferred = { unknown -> unknown }\n\
             loop {\n\
                 val nested: () -> Unit = { break }\n\
                 break\n\
             }\n\
             val wrongReturn: () -> Unit = { return 1 }\n\
             takes(1)\n\
             val rejected = takes({ wrong -> true })\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/invalid-lambdas.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("lambda failures stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0084", "L0084", "L0083", "L0142", "L0087", "L0084", "L0084"
        ]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["->", "{ 1 }", "->", "break", "1", "1", "true"]
    );
    assert!(
        typed.body_diagnostics()[5]
            .details()
            .iter()
            .any(|detail| matches!(
                detail,
                DiagnosticDetail::Label(label)
                    if sources.slice(label.span()).ok()
                        == Some("callback: (borrow Int) -> Int")
            ))
    );
    assert_eq!(typed.calls().len(), 1);
    assert!(typed.validate().is_err());
}

#[test]
fn overload_lambda_trial_commits_only_the_unique_cross_file_candidate() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "declarations.ko",
        "package p\n\
         fun resolve(callback: (Int) -> Int): Int = 1\n\
         fun resolve(callback: (String) -> String): String = \"text\"\n\
         fun intResult(input: Int): Int = input\n\
         fun pick(input: Int, callback: (Int) -> Int): Int = input\n\
         fun pick(input: String, callback: (String) -> String): String = input\n\
         fun makeInt(): Int = 1",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(): Unit {\n\
             val selected = resolve({ item -> intResult(item) })\n\
             val filtered = pick(makeInt(), { filteredItem -> filteredItem })\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new(
            "root",
            "p/declarations.ko",
            declarations_source,
            &declarations,
        ),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("unique overload-lambda trial succeeds");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed overload-lambda trial succeeds");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward.body_parameter_modes(),
        reverse.body_parameter_modes()
    );
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.calls().len(), 4);
    assert!(forward.calls().iter().all(|call| {
        forward.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    }));
    let uses_unit = source_unit(&forward_names, uses_source);
    let parameter = symbol_named(&forward, &forward_names, uses_unit, "item");
    assert_eq!(
        forward.body_parameter_mode(parameter),
        Some(ParameterMode::Borrow)
    );
    assert_eq!(
        forward
            .symbol_type(parameter)
            .and_then(|ty| forward.types().get(ty)),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
}

#[test]
fn failed_and_ambiguous_unit_overload_lambda_trials_leak_no_candidate_facts() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "declarations.ko",
        "package p\n\
         fun resolve(callback: (Int) -> Int): Int = 1\n\
         fun resolve(callback: (String) -> String): String = \"text\"",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(): Unit {\n\
             val ambiguous = resolve({ first -> first })\n\
             val noMatch = resolve({ second -> true })\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new(
            "root",
            "p/declarations.ko",
            declarations_source,
            &declarations,
        ),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("overload-lambda failures stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed overload-lambda failures stay in the recovery product");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward.body_parameter_modes(),
        reverse.body_parameter_modes()
    );
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0124", "L0123"]
    );
    assert_eq!(
        forward.body_diagnostics()[0].message(),
        "call remains ambiguous after argument type checking"
    );
    assert_eq!(forward.body_diagnostics()[0].details().len(), 2);
    assert!(
        forward.body_diagnostics()[0]
            .details()
            .iter()
            .all(|detail| {
                matches!(
                    detail,
                    DiagnosticDetail::Label(label)
                        if sources.slice(label.span()) == Ok("resolve")
                            && label.message() == "matching callable declared here"
                )
            })
    );
    assert!(forward.calls().is_empty());
    let unit = source_unit(&forward_names, uses_source);
    assert!(forward.body_parameter_modes().is_empty());
    let lambda_parameters = forward_names.names().source_units()[unit.index()]
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
        .collect::<Vec<_>>();
    assert_eq!(lambda_parameters.len(), 2);
    for symbol in lambda_parameters {
        assert!(!forward.body_symbol_types().keys().any(|candidate| {
            candidate.source_unit() == unit && candidate.symbol() == symbol.id()
        }));
    }
    assert!(forward.validate().is_err());
}

#[test]
fn generic_body_type_refs_publish_complete_cross_file_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Marker\n\
         class Good : Marker\n\
         value class Pair<T>(val first: T, val second: T)\n\
         value class NeedsMarker<T: Marker>(val item: T)\n\
         value class NeedsCopy<T: Copyable>(val item: T)\n\
         value class NeedsTransfer<T: Transferable>(val item: T)\n\
         enum class Maybe<T> { Some(item: T), None }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(pair: Pair<Int>, boxed: Box<Pair<Int>>, list: List<String>, marked: NeedsMarker<Good>, copied: NeedsCopy<Int>, transferred: NeedsTransfer<String>, maybe: Maybe<Int>): Unit {\n\
             val localPair: Pair<Int> = pair\n\
             val localBox: Box<Pair<Int>> = boxed\n\
             val localList: List<String> = list\n\
             val localMarked: NeedsMarker<Good> = marked\n\
             val localCopied: NeedsCopy<Int> = copied\n\
             val localTransferred: NeedsTransfer<String> = transferred\n\
             val isSome = maybe is Maybe.Some<Int>\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("generic body type refs are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed generic body type refs are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.type_ref_types(), reverse.type_ref_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    for text in [
        "Pair<Int>",
        "Box<Pair<Int>>",
        "List<String>",
        "NeedsMarker<Good>",
        "NeedsCopy<Int>",
        "NeedsTransfer<String>",
    ] {
        let refs = type_refs_with_text(&sources, &uses, text);
        assert!(
            refs.len() >= 2,
            "expected signature and local refs for {text}"
        );
        assert!(refs.iter().all(|id| {
            forward
                .type_ref_type(UnitTypeRefId::new(
                    source_unit(&forward_names, uses_source),
                    *id,
                ))
                .is_some()
        }));
    }
    let uses_unit = source_unit(&forward_names, uses_source);
    let maybe_refs = type_refs_with_text(&sources, &uses, "Maybe<Int>");
    let case_refs = type_refs_with_text(&sources, &uses, "Maybe.Some<Int>");
    assert_eq!(maybe_refs.len(), 1);
    assert_eq!(case_refs.len(), 1);
    let maybe = forward
        .type_ref_type(UnitTypeRefId::new(uses_unit, maybe_refs[0]))
        .expect("generic root type fact");
    assert!(matches!(
        forward
            .type_ref_type(UnitTypeRefId::new(uses_unit, case_refs[0]))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::EnumCase { root, .. }) if *root == maybe
    ));
}

#[test]
fn generic_source_calls_publish_explicit_inferred_bound_and_lambda_instances() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\n\
         interface Marker\n\
         class Good : Marker\n\
         value class Pair<T>(val first: T, val second: T)\n\
         fun <T> identity(own input: T): T\n\
         fun <T> tagged(tag: Byte, own input: T): T\n\
         fun <A, B> second(own first: A, own second: B): B\n\
         fun <T> apply(own input: T, transform: (borrow T) -> T): T\n\
         fun <T> fromList(items: List<T>): T\n\
         fun <T> fromNullable(input: T?): T\n\
         fun <T> fromCallback(callback: (borrow T) -> T): T\n\
         fun <T> map(own input: T, callback: (borrow T) -> Int): Int\n\
         fun <T> map(own input: T, callback: (borrow T) -> String): String\n\
         fun route(input: Int): Int\n\
         fun <T> route(input: List<T>): T\n\
         fun <T: Marker> marked(own input: T): T",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(pair: Pair<Int>, good: Good, items: List<Int>, nullable: Int?, callback: (borrow Int) -> Int): Unit {\n\
             val explicit = identity<Int>(1)\n\
             val contextual = tagged<Int>(1, 2)\n\
             val ordered = second(1, \"ordered\")\n\
             val inferred = identity(2)\n\
             val nested = identity(pair)\n\
             val transformed = apply(3, { item -> item })\n\
             val listItem = fromList(items)\n\
             val nullableItem = fromNullable(nullable)\n\
             val callbackItem = fromCallback(callback)\n\
             val overloadLambda = map(4, { item -> item + 1 })\n\
             val mixedOverload = route(items)\n\
             val bounded = marked(good)\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("generic source calls are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed generic source calls are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());

    let uses_unit = source_unit(&forward_names, uses_source);
    let call = |text: &str| {
        let expression = expression_with_text(&sources, &uses, text);
        forward
            .call(UnitExpressionId::new(uses_unit, expression))
            .expect("generic call descriptor")
    };
    for text in [
        "identity<Int>(1)",
        "tagged<Int>(1, 2)",
        "identity(2)",
        "apply(3, { item -> item })",
        "fromList(items)",
        "fromNullable(nullable)",
        "fromCallback(callback)",
        "map(4, { item -> item + 1 })",
        "route(items)",
    ] {
        let descriptor = call(text);
        assert_eq!(descriptor.instance().type_arguments().len(), 1);
        assert_eq!(
            forward
                .types()
                .get(descriptor.instance().type_arguments()[0]),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        assert_eq!(
            forward.types().get(descriptor.return_type()),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
    let pair = call("identity(pair)");
    assert!(matches!(
        forward
            .types()
            .get(pair.instance().type_arguments()[0]),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&forward_names, "Pair") && arguments.len() == 1
    ));
    let marked = call("marked(good)");
    assert!(matches!(
        forward
            .types()
            .get(marked.instance().type_arguments()[0]),
        Some(UnitTypeKind::Nominal { declaration: owner, arguments })
            if *owner == declaration(&forward_names, "Good") && arguments.is_empty()
    ));
    let ordered = call("second(1, \"ordered\")");
    assert_eq!(ordered.instance().type_arguments().len(), 2);
    assert_eq!(
        forward.types().get(ordered.instance().type_arguments()[0]),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert_eq!(
        forward.types().get(ordered.instance().type_arguments()[1]),
        Some(&UnitTypeKind::Builtin(BuiltinType::String))
    );
    assert_eq!(
        forward.types().get(ordered.return_type()),
        Some(&UnitTypeKind::Builtin(BuiltinType::String))
    );
    let transform = call("apply(3, { item -> item })").arguments()[1];
    assert!(matches!(
        forward.types().get(transform.parameter_type()),
        Some(UnitTypeKind::Function { parameters, return_type, .. })
            if parameters.len() == 1
                && parameters[0].mode() == ParameterMode::Borrow
                && forward.types().get(parameters[0].ty())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && forward.types().get(*return_type)
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    ));
}

#[test]
fn invalid_generic_source_calls_keep_single_candidate_diagnostics_and_recovery() {
    let mut sources = SourceMap::new();
    let (api_source, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\n\
         interface Marker\n\
         class Resource\n\
         value class Pair<T>(val first: T, val second: T)\n\
         fun <T> identity(own input: T): T\n\
         fun <A, B> second(own first: A, own second: B): B\n\
         fun <T> choose(own first: T, own second: T): T\n\
         fun <T> tagged(flag: Boolean, own input: T): T\n\
         fun <T> make(): T\n\
         fun plain(own input: Int): Int\n\
         fun <T: Marker> marked(own input: T): T\n\
         fun <T: Copyable> copied(own input: T): T\n\
         fun <T: Transferable> sent(own input: T): T\n\
         fun <T> overloaded(input: List<T>): Int\n\
         fun <T> overloaded(input: Pair<T>): String\n\
         fun <T> pick(input: T): Int\n\
         fun <T> pick(input: List<T>): String\n\
         fun <T> fromFactory(factory: () -> T): T",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun bad(resource: Resource, shared: Rc<Int>, items: List<Int>): Unit {\n\
             val extra = identity<Int, String>(1)\n\
             val nongeneric = plain<Int>(1)\n\
             val missingType = second<Int>(1, \"x\")\n\
             val unresolved: Int = make()\n\
             val conflict = choose(1, \"x\")\n\
             val fixedMismatch = tagged(1, 2)\n\
             val interfaceBound = marked(resource)\n\
             val copyBound = copied(resource)\n\
             val transferBound = sent(shared)\n\
             val noOverload = overloaded(1)\n\
             val ambiguous = pick(items)\n\
             val lambdaOnly = fromFactory({ 1 })\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/api.ko", api_source, &api),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("generic call failures stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed generic call failures stay in the recovery product");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0091", "L0091", "L0091", "L0140", "L0140", "L0084", "L0093", "L0115", "L0141",
            "L0123", "L0124", "L0140"
        ]
    );
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("generic call diagnostic span"))
            .collect::<Vec<_>>(),
        [
            "String",
            "Int",
            "second",
            "make",
            "\"x\"",
            "1",
            "resource",
            "resource",
            "shared",
            "overloaded",
            "pick",
            "fromFactory"
        ]
    );
    let labels = |diagnostic: &Diagnostic| {
        diagnostic
            .details()
            .iter()
            .filter_map(|detail| match detail {
                DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
                DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&forward.body_diagnostics()[2]), ["B"]);
    assert_eq!(labels(&forward.body_diagnostics()[5]), ["flag: Boolean"]);
    for diagnostic in &forward.body_diagnostics()[6..9] {
        assert_eq!(labels(diagnostic), ["T"]);
    }
    assert!(forward.calls().is_empty());
    assert!(forward.validate().is_err());
}

#[test]
fn external_and_function_value_calls_publish_targets_modes_effects_and_stable_facts() {
    let mut names_environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names_environment
                .declare_type(builtin.name())
                .expect("unique builtin"),
            builtin,
        )
    });
    let consume = names_environment
        .declare_function("consumeExternal")
        .expect("consume external");
    let inspect = names_environment
        .declare_function("inspectExternal")
        .expect("inspect external");
    let send = names_environment
        .declare_function("sendExternal")
        .expect("send external");
    let stop = names_environment
        .declare_function("stopExternal")
        .expect("stop external");
    let print = names_environment
        .declare_function("printExternal")
        .expect("print external");
    let choose_int = names_environment
        .declare_function("chooseExternal")
        .expect("first external overload");
    let choose_string = names_environment
        .declare_function("chooseExternal")
        .expect("second external overload");
    let external_callback = names_environment
        .declare_value("externalCallback")
        .expect("external function value");
    let mut type_environment = TypeEnvironment::new(&names_environment);
    for (symbol, builtin) in builtins {
        type_environment
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    {
        let mut bind_function = |symbol, mode, parameter, result, effects| {
            type_environment
                .bind_function(
                    symbol,
                    EnvironmentFunction {
                        parameters: vec![EnvironmentParameter {
                            mode,
                            ty: EnvironmentType::Builtin(parameter),
                        }],
                        return_type: EnvironmentType::Builtin(result),
                        effects,
                    },
                )
                .expect("valid external function binding");
        };
        bind_function(
            consume,
            ParameterMode::Value,
            BuiltinType::Int,
            BuiltinType::Long,
            Vec::new(),
        );
        bind_function(
            inspect,
            ParameterMode::Borrow,
            BuiltinType::Int,
            BuiltinType::Long,
            Vec::new(),
        );
        bind_function(
            send,
            ParameterMode::Value,
            BuiltinType::String,
            BuiltinType::Unit,
            vec![EnvironmentFunctionEffect::CrossThreadTransfer { parameter: 0 }],
        );
        bind_function(
            stop,
            ParameterMode::Borrow,
            BuiltinType::String,
            BuiltinType::Nothing,
            vec![EnvironmentFunctionEffect::Abort],
        );
        bind_function(
            print,
            ParameterMode::Borrow,
            BuiltinType::String,
            BuiltinType::Unit,
            vec![EnvironmentFunctionEffect::PrintLine],
        );
        bind_function(
            choose_int,
            ParameterMode::Borrow,
            BuiltinType::Int,
            BuiltinType::Long,
            Vec::new(),
        );
        bind_function(
            choose_string,
            ParameterMode::Borrow,
            BuiltinType::String,
            BuiltinType::String,
            Vec::new(),
        );
    }
    type_environment
        .bind_value(
            external_callback,
            EnvironmentType::Function {
                move_only: false,
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::Int),
                }],
                return_type: Box::new(EnvironmentType::Builtin(BuiltinType::Long)),
            },
        )
        .expect("external function value binding");

    let mut sources = SourceMap::new();
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(callback: move (borrow Int) -> Long): Long {\n\
             val consumed = consumeExternal(1)\n\
             val inspected = inspectExternal(2)\n\
             val selected = chooseExternal(3)\n\
             val invoked = callback(4)\n\
             val externalInvoked = externalCallback(5)\n\
             val sent = sendExternal(\"payload\")\n\
             val printed = printExternal(\"line\")\n\
             return consumed + inspected + selected + invoked + externalInvoked\n\
         }\n\
         fun halt(): Unit { stopExternal(\"stop\") }",
    );
    let (stable_source, stable) = parsed(
        &mut sources,
        "stable.ko",
        "package p\nfun stable(): Int = 1",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/stable.ko", stable_source, &stable),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let forward_names = validated_names(&sources, &forward_inputs, &names_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &names_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("external and function-value calls succeed");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed external and function-value calls succeed");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(forward.clone().validate().is_ok());
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    let uses_unit = source_unit(&forward_names, uses_source);
    let call = |text: &str| {
        forward
            .call(UnitExpressionId::new(
                uses_unit,
                expression_with_text(&sources, &uses, text),
            ))
            .expect("call descriptor")
    };
    for (text, target, mode) in [
        (
            "consumeExternal(1)",
            UnitCallTarget::External(consume),
            ParameterMode::Value,
        ),
        (
            "inspectExternal(2)",
            UnitCallTarget::External(inspect),
            ParameterMode::Borrow,
        ),
        (
            "chooseExternal(3)",
            UnitCallTarget::External(choose_int),
            ParameterMode::Borrow,
        ),
    ] {
        let descriptor = call(text);
        assert_eq!(descriptor.target(), target);
        assert_eq!(descriptor.arguments()[0].mode(), mode);
        assert!(descriptor.instance().type_arguments().is_empty());
    }
    for text in ["callback(4)", "externalCallback(5)"] {
        assert_eq!(call(text).target(), UnitCallTarget::FunctionValue);
    }
    assert!(call("sendExternal(\"payload\")").arguments()[0].crosses_thread());
    assert!(call("printExternal(\"line\")").prints_line());
    assert!(call("stopExternal(\"stop\")").aborts());
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(
                uses_unit,
                expression_with_text(&sources, &uses, "consumeExternal"),
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Function { parameters, return_type, .. })
            if parameters.len() == 1
                && parameters[0].mode() == ParameterMode::Value
                && forward.types().get(*return_type)
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Long))
    ));
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(
                uses_unit,
                expression_with_text(&sources, &uses, "callback"),
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Function { move_only, parameters, return_type })
            if *move_only
                && parameters.len() == 1
                && parameters[0].mode() == ParameterMode::Borrow
                && forward.types().get(*return_type)
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Long))
    ));
}

#[test]
fn invalid_external_and_function_value_calls_keep_precise_diagnostics_and_deferred_recovery() {
    let mut names_environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names_environment
                .declare_type(builtin.name())
                .expect("unique builtin"),
            builtin,
        )
    });
    let take = names_environment
        .declare_function("takeExternal")
        .expect("external function");
    let pick_int = names_environment
        .declare_function("pickExternal")
        .expect("first overload");
    let pick_string = names_environment
        .declare_function("pickExternal")
        .expect("second overload");
    let print = names_environment
        .declare_function("printExternal")
        .expect("effectful external function");
    names_environment
        .declare_function("unboundExternal")
        .expect("unbound external identity");
    let mut type_environment = TypeEnvironment::new(&names_environment);
    for (symbol, builtin) in builtins {
        type_environment
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, parameter, result) in [
        (take, BuiltinType::Int, BuiltinType::Int),
        (pick_int, BuiltinType::Int, BuiltinType::Int),
        (pick_string, BuiltinType::String, BuiltinType::String),
    ] {
        type_environment
            .bind_function(
                symbol,
                EnvironmentFunction {
                    parameters: vec![EnvironmentParameter {
                        mode: ParameterMode::Borrow,
                        ty: EnvironmentType::Builtin(parameter),
                    }],
                    return_type: EnvironmentType::Builtin(result),
                    effects: Vec::new(),
                },
            )
            .expect("external binding");
    }
    type_environment
        .bind_function(
            print,
            EnvironmentFunction {
                parameters: vec![EnvironmentParameter {
                    mode: ParameterMode::Borrow,
                    ty: EnvironmentType::Builtin(BuiltinType::String),
                }],
                return_type: EnvironmentType::Builtin(BuiltinType::Unit),
                effects: vec![EnvironmentFunctionEffect::PrintLine],
            },
        )
        .expect("effectful external binding");

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-calls.ko",
        "package p\n\
         fun bad(callback: (borrow Int) -> Int, number: Int): Unit {\n\
             val named = callback(input = 1)\n\
             val nonCallable = number()\n\
             val mismatch = takeExternal(true)\n\
             val missing = takeExternal()\n\
             val noOverload = pickExternal(true)\n\
             val genericExternal = takeExternal<Int>(1)\n\
             val genericCallback = callback<Int>(1)\n\
             val genericEffect = printExternal<String>(\"line\")\n\
             val unbound = unboundExternal(1)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/invalid-calls.ko",
        source,
        &file,
    )];
    let names = validated_names(&sources, &inputs, &names_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("call failures remain in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0120", "L0119", "L0084", "L0121", "L0123"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["input", "number", "true", "takeExternal()", "pickExternal"]
    );
    assert!(typed.calls().is_empty());
    let unit = source_unit(&names, source);
    for text in [
        "takeExternal<Int>(1)",
        "callback<Int>(1)",
        "printExternal<String>(\"line\")",
    ] {
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(
                    unit,
                    expression_with_text(&sources, &file, text),
                ))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Deferred(DeferredReason::Call))
        ));
    }
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &file, "unboundExternal(1)"),
            ))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(typed.validate().is_err());
}

#[test]
fn partial_external_overloads_and_nested_deferred_calls_never_publish_error_facts() {
    let mut names_environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL.map(|builtin| {
        (
            names_environment
                .declare_type(builtin.name())
                .expect("unique builtin"),
            builtin,
        )
    });
    let partial_bound = names_environment
        .declare_function("partialExternal")
        .expect("bound partial overload");
    names_environment
        .declare_function("partialExternal")
        .expect("unbound partial overload");
    let pick_int = names_environment
        .declare_function("pickExternal")
        .expect("integer overload");
    let pick_string = names_environment
        .declare_function("pickExternal")
        .expect("string overload");
    names_environment
        .declare_function("unboundExternal")
        .expect("unbound nested call");
    let mut type_environment = TypeEnvironment::new(&names_environment);
    for (symbol, builtin) in builtins {
        type_environment
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    for (symbol, parameter, result) in [
        (partial_bound, BuiltinType::Int, BuiltinType::Int),
        (pick_int, BuiltinType::Int, BuiltinType::Int),
        (pick_string, BuiltinType::String, BuiltinType::String),
    ] {
        type_environment
            .bind_function(
                symbol,
                EnvironmentFunction {
                    parameters: vec![EnvironmentParameter {
                        mode: ParameterMode::Borrow,
                        ty: EnvironmentType::Builtin(parameter),
                    }],
                    return_type: EnvironmentType::Builtin(result),
                    effects: Vec::new(),
                },
            )
            .expect("external function binding");
    }

    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "deferred-calls.ko",
        "package p\n\
         fun deferred(): Unit {\n\
             val partial = partialExternal(1)\n\
             val nested = pickExternal(unboundExternal())\n\
             val lambda = resolve({ item -> unboundExternal() })\n\
         }\n\
         fun resolve(callback: (borrow Int) -> Int): Int = 1\n\
         fun resolve(callback: (borrow String) -> String): String = \"resolved\"",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/deferred-calls.ko",
        source,
        &file,
    )];
    let names = validated_names(&sources, &inputs, &names_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("deferred external calls keep a recovery product");
    let unit = source_unit(&names, source);
    let expression_kind = |text| {
        typed
            .expression_type(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &file, text),
            ))
            .and_then(|ty| typed.types().get(ty))
    };

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(matches!(
        expression_kind("partialExternal(1)"),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(matches!(
        expression_kind("unboundExternal()"),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(matches!(
        expression_kind("pickExternal(unboundExternal())"),
        Some(UnitTypeKind::Deferred(DeferredReason::Call))
    ));
    assert!(matches!(
        expression_kind("resolve({ item -> unboundExternal() })"),
        Some(UnitTypeKind::Deferred(DeferredReason::Call))
    ));
    assert!(typed.calls().is_empty());
    assert!(typed.validate().is_ok());
}

#[test]
fn effectful_external_function_values_fail_loud_until_effect_identity_is_preserved() {
    for (name, text) in [
        (
            "p/grouped-effect.ko",
            "package p\nfun bad(): Unit { (println)(\"line\") }",
        ),
        (
            "p/aliased-effect.ko",
            "package p\nfun bad(): Unit { val output = println }",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let error = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect_err("effectful external function values must not erase compiler-bound effects");
        let lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span) = error
        else {
            panic!("expected UnsupportedBody, got {error:?}");
        };
        assert_eq!(sources.slice(span), Ok("println"));
    }
}

#[test]
fn unbound_external_body_type_keeps_single_file_deferred_recovery() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "uses.ko",
        "package p\nfun use(): Unit { val local: Opaque = return }",
    );
    let inputs = [SourceUnitInput::new("root", "p/uses.ko", source, &file)];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("fresh external type name");
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unbound external type remains a deferred recovery fact");
    let refs = type_refs_with_text(&sources, &file, "Opaque");

    assert_eq!(refs.len(), 1);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(matches!(
        typed
            .type_ref_type(UnitTypeRefId::new(source_unit(&names, source), refs[0]))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
    ));
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_generic_body_type_refs_keep_existing_codes_and_recovery() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Marker\n\
         class Resource\n\
         value class Pair<T>(val first: T, val second: T)\n\
         value class NeedsMarker<T: Marker>(val item: T)\n\
         value class NeedsCopy<T: Copyable>(val item: T)\n\
         value class NeedsTransfer<T: Transferable>(val item: T)\n\
         value class Loop(val next: Loop)\n\
         enum class Maybe<T> { Some(item: T), None }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun <T> invalid(): Unit {\n\
             val builtin: Int<String> = return\n\
             val nested: Pair<Int<String>> = return\n\
             val tooMany: Pair<Int, String> = return\n\
             val arity: Pair = return\n\
             val parameter: T<Int> = return\n\
             val capability: Copyable<Int> = return\n\
             val interfaceValue: Marker = return\n\
             val interfaceBound: NeedsMarker<Resource> = return\n\
             val interfaceAny: NeedsMarker<Any> = return\n\
             val copyBound: NeedsCopy<Resource> = return\n\
             val copyAny: NeedsCopy<Any> = return\n\
             val transferBound: NeedsTransfer<Rc<Int>> = return\n\
             val transferAny: NeedsTransfer<Any> = return\n\
             val badBox: Box<Resource> = return\n\
             val badList: List<Any> = return\n\
             val badInline: List<Loop> = return\n\
             val case: Maybe.Some = return\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("invalid generic body refs stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed invalid generic body refs stay in the recovery product");

    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.body_symbol_types(), reverse.body_symbol_types());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0082", "L0082", "L0091", "L0091", "L0091", "L0082", "L0094", "L0093", "L0093",
            "L0115", "L0115", "L0141", "L0141", "L0117", "L0125", "L0125", "L0114"
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
        [
            "String",
            "String",
            "Pair",
            "Pair",
            "Int",
            "Int",
            "Marker",
            "Resource",
            "Any",
            "Resource",
            "Any",
            "Rc<Int>",
            "Any",
            "Resource",
            "Any",
            "Loop",
            "Maybe.Some"
        ]
    );
    let l0082_labels = forward
        .body_diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code().to_string() == "L0082")
        .flat_map(|diagnostic| diagnostic.details())
        .filter_map(|detail| match detail {
            DiagnosticDetail::Label(label) => sources.slice(label.span()).ok(),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(l0082_labels, ["Int", "Int", "Copyable"]);
    let nested = type_refs_with_text(&sources, &uses, "Pair<Int<String>>");
    assert_eq!(nested.len(), 1);
    assert!(matches!(
        forward
            .type_ref_type(UnitTypeRefId::new(
                source_unit(&forward_names, uses_source),
                nested[0],
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Error)
    ));
    let loop_declaration = declaration(&forward_names, "Loop");
    assert!(
        forward
            .signatures()
            .invalid_inline_nominals()
            .contains(&loop_declaration)
    );
    assert!(forward.validate().is_err());
}

#[test]
fn cross_file_member_bodies_calls_and_fields_publish_source_qualified_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Named {\n\
             fun current(): Unit {\n\
                 val current = this\n\
             }\n\
             fun label(): String\n\
         }\n\
         class Holder<T>(val item: T) : Named {\n\
             override fun label(): String = \"holder\"\n\
             fun self(): Holder<T> = this\n\
             fun field(): T = this.item\n\
             fun <R> pick(own selected: R, fallback: T): R = selected\n\
             fun forwarded(): String = label()\n\
         }\n\
         value class Pair<T>(val first: T, val second: T)\n\
         class Tools { companion object { fun identity(input: Int): Int = input } }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(holder: Holder<Int>, pair: Pair<Long>): String {\n\
             val field = holder.item\n\
             val picked = holder.pick<String>(\"ok\", 1)\n\
             val component = pair.component1()\n\
             return holder.label()\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("cross-file member bodies and uses are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed cross-file member bodies and uses are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(
        forward.aggregate_projections(),
        reverse.aggregate_projections()
    );
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.calls().len(), 4);
    let member_calls = forward
        .calls()
        .iter()
        .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
        .collect::<Vec<_>>();
    assert_eq!(member_calls.len(), 3);
    let mut member_instance_arities = member_calls
        .iter()
        .map(|call| call.instance().type_arguments().len())
        .collect::<Vec<_>>();
    member_instance_arities.sort_unstable();
    assert_eq!(member_instance_arities, [1, 1, 2]);
    assert!(matches!(
        forward.calls()[2].target(),
        UnitCallTarget::StructuralComponent(_)
    ));
    assert_eq!(forward.aggregate_projections().len(), 3);
    assert!(forward.aggregate_projections().iter().all(|projection| {
        projection.field().source_unit() == source_unit(&forward_names, models_source)
    }));
    assert_eq!(
        forward
            .aggregate_projections()
            .iter()
            .filter(|projection| projection.kind() == UnitAggregateProjectionKind::Field)
            .count(),
        2
    );
    assert_eq!(
        forward
            .aggregate_projections()
            .iter()
            .filter(|projection| {
                projection.kind() == UnitAggregateProjectionKind::StructuralComponent
            })
            .count(),
        1
    );
    for projection in forward.aggregate_projections() {
        let expected = match projection.kind() {
            UnitAggregateProjectionKind::Field => ExpressionCategory::Place,
            UnitAggregateProjectionKind::StructuralComponent => ExpressionCategory::Temporary,
        };
        assert_eq!(
            forward.expression_category(projection.expression()),
            Some(expected)
        );
    }

    let model_unit = source_unit(&forward_names, models_source);
    let this_types = expressions_with_text(&sources, &models, "this")
        .into_iter()
        .map(|expression| {
            forward
                .expression_type(UnitExpressionId::new(model_unit, expression))
                .and_then(|ty| forward.types().get(ty))
                .expect("member this expression has a type")
        })
        .collect::<Vec<_>>();
    assert!(matches!(this_types[0], UnitTypeKind::StaticSelf(_)));
    assert!(
        this_types[1..]
            .iter()
            .all(|kind| matches!(kind, UnitTypeKind::Nominal { .. }))
    );
    let companion_input = expression_with_text(&sources, &models, "input");
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(model_unit, companion_input))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(forward.validate().is_ok());
}

#[test]
fn member_overload_trials_and_payload_errors_recover_without_leaking_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Resolver {\n\
             fun choose(callback: (Int) -> Int): Int = 1\n\
             fun choose(callback: (String) -> String): String = \"text\"\n\
         }\n\
         enum class Maybe {\n\
             Some(item: Int), None;\n\
             fun payloadOrZero(): Int = when (this) {\n\
                 is Some -> item\n\
                 is None -> 0\n\
             }\n\
         }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun selected(resolver: Resolver): Int = resolver.choose({ input -> input + 1 })\n\
         fun mismatch(resolver: Resolver): String = resolver.choose({ input -> input + 1 })\n\
         fun payload(subject: Maybe): Int = subject.item\n\
         fun nullablePayload(subject: Maybe?): Int = subject.item",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("member errors stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed member errors stay in the recovery product");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(
        forward.aggregate_projections(),
        reverse.aggregate_projections()
    );
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084", "L0113", "L0113"]
    );
    assert_eq!(forward.calls().len(), 2);
    assert!(
        forward
            .calls()
            .iter()
            .all(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
    );
    assert_eq!(forward.aggregate_projections().len(), 1);
    assert!(matches!(
        forward.aggregate_projections()[0].receiver(),
        UnitAggregateProjectionReceiver::This(_)
    ));
    let payload_item = expression_with_text(&sources, &models, "item");
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(
                source_unit(&forward_names, models_source),
                payload_item,
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(forward.validate().is_err());
}

#[test]
fn member_visibility_shapes_and_owner_dependent_bounds_are_preserved() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Marker<T>\n\
         interface GenericBase { fun <T> id(input: T): T }\n\
         class Marked : Marker<Int>\n\
         class Host<T>(private val secret: T) : GenericBase {\n\
             private fun hidden(): T = secret\n\
             fun same(other: Host<T>): T = other.hidden()\n\
             fun sameField(other: Host<T>): T = other.secret\n\
             override fun <R> id(input: R): R = input\n\
             fun <R : Marker<T>> accept(input: R): R = input\n\
             fun <A> make(input: A): A = input\n\
             fun <A, B> make(first: A, second: B): B = second\n\
             fun <A> tag(input: Int): Int = input\n\
             fun <A, B> tag(input: Int): Int = input\n\
         }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun valid(host: Host<Int>, marked: Marked): Int {\n\
             val inferred = host.accept(marked)\n\
             val explicit = host.accept<Marked>(marked)\n\
             val one = host.make(1)\n\
             val two = host.make(1, \"ok\")\n\
             val tagOne = host.tag<String>(1)\n\
             val tagTwo = host.tag<String, Long>(1)\n\
             return host.id(1)\n\
         }\n\
         fun privateField(host: Host<Int>): Int = host.secret\n\
         fun privateCall(host: Host<Int>): Int = host.hidden()\n\
         fun privateSafeField(host: Host<Int>?): Int? = host?.secret\n\
         fun privateSafeCall(host: Host<Int>?): Int? = host?.hidden()\n\
         fun publicSafeCall(host: Host<Int>?): Int? = host?.sameField(host)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("private access errors stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0080", "L0080", "L0080", "L0080"]
    );
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
            .count(),
        8
    );
    assert_eq!(typed.aggregate_projections().len(), 1);
    assert!(typed.validate().is_err());
}

#[test]
fn intrinsic_rc_members_publish_unit_operations_and_trial_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Resource\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun choose(callback: (Int) -> Int): Int = 1\n\
         fun choose(callback: (String) -> String): String = \"text\"",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun read(shared: Rc<Int>): Int = shared.value\n\
         fun retain(shared: Rc<Int>): Rc<Int> = shared.share()\n\
         fun borrowPayload(shared: Rc<Resource>): Unit = inspect(shared.value)\n\
         fun selected(shared: Rc<Int>): Int = choose({ ignored -> shared.value })",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("intrinsic Rc member operations are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed intrinsic Rc member operations are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.rc_operations(), reverse.rc_operations());
    assert_eq!(forward.rc_operations().len(), 4);
    assert_eq!(
        forward
            .rc_operations()
            .iter()
            .filter(|operation| operation.kind() == RcOperationKind::Value)
            .count(),
        3
    );
    assert_eq!(
        forward
            .rc_operations()
            .iter()
            .filter(|operation| operation.kind() == RcOperationKind::Share)
            .count(),
        1
    );
    for operation in forward.rc_operations() {
        assert_eq!(
            forward.expression_category(operation.expression()),
            Some(match operation.kind() {
                RcOperationKind::Share => ExpressionCategory::Temporary,
                RcOperationKind::Value => ExpressionCategory::Place,
            })
        );
        assert_eq!(
            operation.expression().source_unit(),
            operation.receiver().source_unit()
        );
    }
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_intrinsic_rc_share_calls_publish_no_partial_operation() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-rc.ko",
        "fun badType(shared: Rc<Int>): Rc<Int> = shared.share<String>()\n\
         fun badArgument(shared: Rc<Int>): Rc<Int> = shared.share(1)",
    );
    let inputs = [SourceUnitInput::new("root", "invalid-rc.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid Rc calls stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0091", "L0121"]
    );
    assert!(typed.rc_operations().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn invalid_intrinsic_rc_share_traverses_nested_container_operands() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nested-rc.ko",
        "fun bad(shared: Rc<Int>): Rc<Int> = shared.share(arrayOf<Int>(1))",
    );
    let inputs = [SourceUnitInput::new("root", "nested-rc.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid Rc call still checks the now-supported nested container");
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0121"]
    );
    assert_eq!(typed.container_constructions().len(), 1);
    assert!(typed.rc_operations().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn safe_and_nullable_intrinsic_rc_members_remain_fail_loud() {
    for (name, text, expected) in [
        (
            "safe-value.ko",
            "fun bad(shared: Rc<Int>): Int = shared?.value",
            "value",
        ),
        (
            "nullable-value.ko",
            "fun bad(shared: Rc<Int>?): Int = shared?.value",
            "value",
        ),
        (
            "safe-share.ko",
            "fun bad(shared: Rc<Int>): Rc<Int> = shared?.share()",
            "share",
        ),
        (
            "nullable-share.ko",
            "fun bad(shared: Rc<Int>?): Rc<Int>? = shared?.share()",
            "share",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &file)];
        let (name_environment, type_environment) = standard_environments();
        let names = validated_names(&sources, &inputs, &name_environment);
        let error = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect_err("safe/nullable Rc members must not validate as deferred facts");
        let lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span) = error
        else {
            panic!("expected UnsupportedBody, got {error:?}");
        };
        assert_eq!(sources.slice(span), Ok(expected));
    }
}

#[test]
fn poisoned_intrinsic_rc_payloads_remain_fail_loud() {
    for (name, text, expected) in [
        (
            "poisoned-value.ko",
            "fun bad(shared: Rc<List<Opaque>>): List<Opaque> = shared.value",
            "value",
        ),
        (
            "poisoned-share.ko",
            "fun bad(shared: Rc<List<Opaque>>): Rc<List<Opaque>> = shared.share()",
            "share",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, name, text);
        let inputs = [SourceUnitInput::new("root", name, source, &file)];
        let (mut name_environment, type_environment) = standard_environments();
        name_environment
            .declare_type("Opaque")
            .expect("external type name is unique");
        let names = validated_names(&sources, &inputs, &name_environment);
        let error = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect_err("nested Error payloads must not publish validated Rc operations");
        let lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span) = error
        else {
            panic!("expected UnsupportedBody, got {error:?}");
        };
        assert_eq!(sources.slice(span), Ok(expected));
    }
}

#[test]
fn intrinsic_container_constructions_publish_stable_unit_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "p/models.ko",
        "package p\nvalue class Resource(val id: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun build(size: Int): Unit {\n\
             val inferred = listOf(Resource(1), Resource(2))\n\
             val expected: List<Int> = listOf()\n\
             val nullable: List<Int?> = listOf(null)\n\
             val explicit = arrayOf<Long>()\n\
             val initializer: (Int) -> Int = { index -> index }\n\
             val array = Array<Int>(borrow size, borrow initializer)\n\
             val list = List<Int>(size, initializer)\n\
             val mutable = MutableList<Int>()\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("core container constructions are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed core container constructions are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(
        forward.container_constructions(),
        reverse.container_constructions()
    );
    assert_eq!(forward.container_constructions().len(), 7);
    assert_eq!(
        forward
            .container_constructions()
            .iter()
            .map(|descriptor| (descriptor.kind(), descriptor.container()))
            .collect::<Vec<_>>(),
        [
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::ListForm,
                SequentialContainerKind::Array,
            ),
            (
                ContainerConstructionKind::RuntimeLength,
                SequentialContainerKind::Array,
            ),
            (
                ContainerConstructionKind::RuntimeLength,
                SequentialContainerKind::List,
            ),
            (
                ContainerConstructionKind::EmptyMutableList,
                SequentialContainerKind::MutableList,
            ),
        ]
    );
    assert_eq!(
        forward.container_constructions()[0].parameter_modes(),
        [ParameterMode::Value, ParameterMode::Value]
    );
    assert_eq!(
        forward.container_constructions()[4].parameter_modes(),
        [ParameterMode::Borrow, ParameterMode::Borrow]
    );
    for descriptor in forward.container_constructions() {
        assert_eq!(
            forward.expression_type(descriptor.expression()),
            Some(descriptor.container_type())
        );
        assert_eq!(
            forward.expression_category(descriptor.expression()),
            Some(ExpressionCategory::Temporary)
        );
    }
    let resource = declaration(&forward_names, "Resource");
    assert!(matches!(
        forward
            .types()
            .get(forward.container_constructions()[0].element_type()),
        Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == resource
    ));
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_intrinsic_container_constructions_keep_diagnostics_and_no_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-containers.ko",
        "fun invalid(size: Boolean): Unit {\n\
             val empty = listOf()\n\
             val absent = listOf(null)\n\
             val mixed = listOf(1, true)\n\
             val initializer: (Int) -> Int = { index -> index }\n\
             val runtime = Array<Int>(size, initializer)\n\
             val wrongMutable = MutableList<Int>(1)\n\
             val tooMany = listOf<Int, Long>()\n\
             val named = listOf(element = 1)\n\
             val marked = listOf(borrow 1)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-containers.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid core container calls stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0126", "L0126", "L0084", "L0084", "L0127", "L0091", "L0120", "L0122"
        ]
    );
    assert!(typed.container_constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn contextual_null_recovery_matches_single_file_rules() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-trial.ko",
        "fun nullable(): Int? = null\n\
         fun invalid(): Unit {\n\
             val missing = null\n\
             val mismatch: Int = null\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("null recovery remains in the typed product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0083", "L0084"]
    );
    assert!(typed.container_constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn intrinsic_container_overload_trial_commits_only_the_unique_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-trial.ko",
        "fun choose(action: () -> List<Int>): Int\n\
         fun choose(action: () -> List<String>): String\n\
         fun selected(): Int = choose({ listOf<Int>() })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("one container-returning lambda candidate is valid");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.container_constructions().len(), 1);
    assert_eq!(
        typed.container_constructions()[0].element_type(),
        typed.types().builtin(BuiltinType::Int).expect("Int")
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn source_container_names_never_gain_intrinsic_construction_identity() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "source-containers.ko",
        "class List<T>(val item: T)\n\
         fun listOf(input: Int): Int = input\n\
         fun source(): List<Int> = List(listOf(1))\n\
         fun sourceIndex(items: List<Int>): Unit { val item = items[0] }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "source-containers.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("source container names use ordinary source identities");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.container_constructions().is_empty());
    assert!(typed.element_places().is_empty());
    assert_eq!(typed.constructions().len(), 1);
    assert!(matches!(
        typed.constructions()[0].target(),
        UnitConstructionTarget::Nominal(_)
    ));
    assert_eq!(typed.calls().len(), 1);
    assert!(matches!(
        typed.calls()[0].target(),
        UnitCallTarget::Declaration(_)
    ));
    assert!(typed.validate().is_ok());
}

#[test]
fn intrinsic_container_places_members_and_assignments_publish_stable_unit_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "p/models.ko",
        "package p\nvalue class Resource(val id: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun mutate(inout input: Resource): Unit {}\n\
         fun operate(\n\
             items: List<Resource>, array: Array<Resource>,\n\
             mutable: MutableList<Resource>, replacement: Resource\n\
         ): Unit {\n\
             val first: Resource = items[0]\n\
             val replaced = (array[0] = replacement)\n\
             val changed = (mutable[0] = replacement)\n\
             val count: Int = items.size\n\
             val borrowed = mutate(&(array[0]))\n\
         }",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("intrinsic container places and members are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed intrinsic container places are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.element_places(), reverse.element_places());
    assert_eq!(forward.element_places().len(), 4);
    assert_eq!(
        forward
            .element_places()
            .iter()
            .map(|place| (place.container(), place.is_mutable()))
            .collect::<Vec<_>>(),
        [
            (SequentialContainerKind::List, false),
            (SequentialContainerKind::Array, true),
            (SequentialContainerKind::MutableList, true),
            (SequentialContainerKind::Array, true),
        ]
    );
    let resource = declaration(&forward_names, "Resource");
    for place in forward.element_places() {
        assert_eq!(
            place.expression().source_unit(),
            place.receiver().source_unit()
        );
        assert_eq!(
            place.expression().source_unit(),
            place.index().source_unit()
        );
        assert_eq!(
            forward.expression_type(place.expression()),
            Some(place.element_type())
        );
        assert_eq!(
            forward.expression_category(place.expression()),
            Some(ExpressionCategory::Place)
        );
        assert!(matches!(
            forward.types().get(place.element_type()),
            Some(UnitTypeKind::Nominal { declaration, .. }) if *declaration == resource
        ));
    }
    let size = expression_with_text(&sources, &uses, "items.size");
    assert_eq!(
        forward.expression_type(UnitExpressionId::new(
            source_unit(&forward_names, uses_source),
            size,
        )),
        forward.types().builtin(BuiltinType::Int)
    );
    assert_eq!(
        forward.expression_category(UnitExpressionId::new(
            source_unit(&forward_names, uses_source),
            size,
        )),
        Some(ExpressionCategory::Temporary)
    );
    assert_eq!(forward.calls().len(), 1);
    assert_eq!(
        forward.calls()[0].arguments()[0].category(),
        ExpressionCategory::Place
    );
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_intrinsic_container_operations_keep_exact_diagnostics_and_no_partial_calls() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-container-operations.ko",
        "fun mutate(inout input: Int): Unit {}\n\
         fun invalid(list: List<Int>, array: Array<Int>, strings: Array<String>): Unit {\n\
             val badIndex = list[true]\n\
             val replacement = (list[0] = 1)\n\
             val groupedReplacement = ((list[0]) = 1)\n\
             val resize = (list.size = 2)\n\
             val get = list.get(0)\n\
             val set = list.set(0, 1)\n\
             val borrowed = mutate(&list[0])\n\
             val groupedBorrowed = mutate(&(list[0]))\n\
             val borrowedSize = mutate(&list.size)\n\
             val changed = mutate(&array[0])\n\
             val compound = (strings[0] += \"x\")\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-container-operations.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid container operations remain in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        [
            "L0128", "L0129", "L0129", "L0129", "L0130", "L0130", "L0122", "L0122", "L0122",
            "L0085"
        ]
    );
    assert_eq!(typed.calls().len(), 1);
    assert_eq!(typed.element_places().len(), 6);
    assert!(typed.validate().is_err());
}

#[test]
fn intrinsic_container_place_overload_trial_commits_only_the_unique_fact() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-place-trial.ko",
        "fun choose(action: () -> Int): Int\n\
         fun choose(action: () -> String): String\n\
         fun selected(items: List<Int>): Int = choose({ items[0] })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-place-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("one element-place lambda candidate is valid");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.element_places().len(), 1);
    assert_eq!(
        typed.element_places()[0].element_type(),
        typed.types().builtin(BuiltinType::Int).expect("Int")
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn poisoned_intrinsic_container_element_places_remain_fail_loud() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "poisoned-container-place.ko",
        "fun bad(items: List<Opaque>): Opaque = items[0]",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "poisoned-container-place.ko",
        source,
        &file,
    )];
    let (mut name_environment, type_environment) = standard_environments();
    name_environment
        .declare_type("Opaque")
        .expect("external type name is unique");
    let names = validated_names(&sources, &inputs, &name_environment);
    let error = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect_err("poisoned element types must not publish unit element places");
    let lang_frontend::type_checking::CompilationUnitTypeError::UnsupportedBody(span) = error
    else {
        panic!("expected UnsupportedBody, got {error:?}");
    };
    assert_eq!(sources.slice(span), Ok("items[0]"));
}

#[test]
fn null_comparisons_publish_stable_source_qualified_flow_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "p/models.ko",
        "package p\n\
         class Resource(val id: Int)\n\
         fun modelRead(input: Resource?): Int =\n\
             if (input != null) input.id else 0",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun read(input: Resource?): Int =\n\
             if (input != null) input.id else 0\n\
         fun mirrored(input: Resource?): Int =\n\
             if (null == input) 0 else input.id\n\
         fun shortCircuit(input: Resource?): Int =\n\
             if (input != null && input.id > 0) input.id else 0",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("null comparisons type check across files");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed null comparisons type check across files");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.null_comparisons(), reverse.null_comparisons());
    assert_eq!(forward.non_null_uses(), reverse.non_null_uses());
    assert_eq!(forward.null_comparisons().len(), 4);
    assert_eq!(forward.non_null_uses().len(), 5);
    assert_eq!(
        forward
            .null_comparisons()
            .iter()
            .filter(|comparison| comparison.non_null_when_true())
            .count(),
        3
    );
    let model_unit = source_unit(&forward_names, models_source);
    let uses_unit = source_unit(&forward_names, uses_source);
    assert!(
        forward
            .null_comparisons()
            .iter()
            .any(|comparison| comparison.expression().source_unit() == model_unit)
    );
    assert!(
        forward
            .null_comparisons()
            .iter()
            .any(|comparison| comparison.expression().source_unit() == uses_unit)
    );
    let resource = forward
        .signatures()
        .declaration(declaration(&forward_names, "Resource"))
        .expect("Resource signature")
        .ty();
    for use_fact in forward.non_null_uses() {
        assert_eq!(
            use_fact.symbol().source_unit(),
            use_fact.expression().source_unit()
        );
        assert_eq!(use_fact.narrowed_type(), resource);
        assert_eq!(
            forward.types().get(use_fact.declared_type()),
            Some(&UnitTypeKind::Nullable(resource))
        );
        assert_eq!(
            forward.expression_type(use_fact.expression()),
            Some(use_fact.narrowed_type())
        );
        assert_eq!(forward.non_null_use(use_fact.expression()), Some(*use_fact));
    }
    for comparison in forward.null_comparisons() {
        assert_eq!(
            comparison.symbol().source_unit(),
            comparison.expression().source_unit()
        );
        assert_eq!(
            forward.types().get(comparison.nullable_type()),
            Some(&UnitTypeKind::Nullable(resource))
        );
        assert_eq!(
            forward.null_comparison(comparison.expression()),
            Some(*comparison)
        );
    }
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_null_comparisons_report_inference_errors_without_partial_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-null-comparisons.ko",
        "fun nonNullable(input: Int): Boolean = input != null\n\
         fun bothNull(): Boolean = null == null",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-null-comparisons.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid null comparisons remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0083", "L0083", "L0083"]
    );
    assert!(
        typed
            .body_diagnostics()
            .iter()
            .all(|diagnostic| { sources.slice(diagnostic.primary_span()) == Ok("null") })
    );
    assert!(typed.null_comparisons().is_empty());
    assert!(typed.non_null_uses().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn null_comparison_overload_trial_commits_only_the_unique_candidate_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nullable-trial.ko",
        "class Resource(val id: Int)\n\
         fun choose(action: () -> Int): Int\n\
         fun choose(action: () -> String): String\n\
         fun selected(input: Resource?): Int =\n\
             choose({ if (input != null) input.id else 0 })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("one nullable lambda candidate is valid");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.null_comparisons().len(), 1);
    assert_eq!(typed.non_null_uses().len(), 1);
    assert_eq!(typed.calls().len(), 1);
    assert!(typed.null_comparisons()[0].non_null_when_true());
    assert_eq!(
        typed.non_null_uses()[0].symbol(),
        typed.null_comparisons()[0].symbol()
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn nullable_lambda_parameters_are_stable_but_captured_mutable_locals_are_not() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nullable-stability.ko",
        "class Resource(val id: Int)\n\
         fun apply(callback: (Resource?) -> Int): Int = 0\n\
         fun lambdaParameter(): Int =\n\
             apply({ input -> if (input != null) input.id else 0 })\n\
         fun captured(initial: Resource?): Boolean {\n\
             var current = initial\n\
             val capture = { current }\n\
             return current != null\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-stability.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("flow stability matches the single-file capture rules");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.null_comparisons().len(), 1);
    assert_eq!(typed.non_null_uses().len(), 1);
    let unit = source_unit(&names, source);
    let input = symbol_named(&typed, &names, unit, "input");
    let current = symbol_named(&typed, &names, unit, "current");
    assert_eq!(typed.null_comparisons()[0].symbol(), input);
    assert_eq!(typed.non_null_uses()[0].symbol(), input);
    assert!(
        typed
            .null_comparisons()
            .iter()
            .all(|comparison| comparison.symbol() != current)
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn conflicting_short_circuit_facts_are_removed_instead_of_overwritten() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nullable-conflict.ko",
        "enum class Shape { Circle(radius: Int), Point }\n\
         fun conflict(input: Shape?): Shape? =\n\
             if (input != null && input is Shape.Circle) input else input",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-conflict.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("conflicting flow facts use the conservative merge rule");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let unit = source_unit(&names, source);
    let shape = typed
        .signatures()
        .declaration(declaration(&names, "Shape"))
        .expect("Shape signature")
        .ty();
    let input_uses = expressions_with_text(&sources, &file, "input");
    assert_eq!(input_uses.len(), 4);
    let input_types = input_uses
        .iter()
        .map(|&expression| {
            typed
                .expression_type(UnitExpressionId::new(unit, expression))
                .expect("every input use has a type")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        typed.types().get(input_types[0]),
        Some(UnitTypeKind::Nullable(inner)) if *inner == shape
    ));
    assert_eq!(input_types[1], shape);
    for index in [2, 3] {
        assert!(matches!(
            typed.types().get(input_types[index]),
            Some(UnitTypeKind::Nullable(inner)) if *inner == shape
        ));
    }
    assert_eq!(typed.null_comparisons().len(), 1);
    assert_eq!(typed.non_null_uses().len(), 1);
    assert!(typed.validate().is_ok());
}

#[test]
fn interpolated_strings_traverse_cross_file_expressions_in_source_order() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "interpolation-declarations.ko",
        "package p\nfun label(input: Int): Int = input",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "interpolation-uses.ko",
        "package p\nfun render(input: Int): String = \"before ${label(input)} after\"",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
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
        .expect("interpolation traverses its nested expression");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed interpolation inputs type check");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.calls(), reverse.calls());
    assert_eq!(typed.calls().len(), 1);
    let unit = source_unit(&names, uses_source);
    let string = expression_with_text(&sources, &uses, "\"before ${label(input)} after\"");
    let call = expression_with_text(&sources, &uses, "label(input)");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(unit, string))
                .expect("interpolated string has a type")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::String))
    ));
    assert_eq!(
        typed
            .call(UnitExpressionId::new(unit, call))
            .map(|call| call.target()),
        Some(UnitCallTarget::Declaration(declaration(&names, "label")))
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_interpolation_recovers_the_outer_string_and_later_body() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-interpolation.ko",
        "fun invalid(): String = \"bad ${1 + true}\"\n\
         fun later(): String = \"ok ${42}\"",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-interpolation.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid nested interpolation remains recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0085"]
    );
    let unit = source_unit(&names, source);
    for text in ["\"bad ${1 + true}\"", "\"ok ${42}\""] {
        let expression = expression_with_text(&sources, &file, text);
        let ty = typed
            .expression_type(UnitExpressionId::new(unit, expression))
            .expect("outer string keeps its String type");
        assert!(matches!(
            typed.types().get(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        ));
    }
    let literal = expression_with_text(&sources, &file, "42");
    assert!(
        typed
            .expression_type(UnitExpressionId::new(unit, literal))
            .is_some()
    );
    assert!(typed.validate().is_err());
}

#[test]
fn top_level_initializers_publish_stable_cross_file_symbol_and_expression_types() {
    let mut sources = SourceMap::new();
    let (values_source, values) = parsed(
        &mut sources,
        "top-level-values.ko",
        "package p\n\
         val inferred = 1\n\
         val annotated: Long = 2L\n\
         const val text: String = \"ready\"\n\
         val forward: Boolean = later\n\
         val later: Boolean = true",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "top-level-uses.ko",
        "package p\n\
         fun number(): Int = inferred\n\
         fun wide(): Long = annotated\n\
         fun message(): String = text\n\
         fun flag(): Boolean = forward",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-values.ko", values_source, &values),
        SourceUnitInput::new("root", "p/b-uses.ko", uses_source, &uses),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("top-level initializers type check");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed top-level inputs type check");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.body_symbol_types(), reverse.body_symbol_types());
    let values_unit = source_unit(&names, values_source);
    for (name, builtin) in [
        ("inferred", BuiltinType::Int),
        ("annotated", BuiltinType::Long),
        ("text", BuiltinType::String),
        ("forward", BuiltinType::Boolean),
        ("later", BuiltinType::Boolean),
    ] {
        let symbol = symbol_named(&typed, &names, values_unit, name);
        assert!(matches!(
            typed.types().get(typed.symbol_type(symbol).expect("top-level symbol type")),
            Some(UnitTypeKind::Builtin(actual)) if *actual == builtin
        ));
    }
    let uses_unit = source_unit(&names, uses_source);
    for name in ["inferred", "annotated", "text", "forward"] {
        let expression = expression_with_text(&sources, &uses, name);
        assert!(
            typed
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .is_some()
        );
    }
    assert!(typed.validate().is_ok());
}

#[test]
fn cross_file_unannotated_forward_use_preserves_the_single_file_deferred_boundary() {
    let mut sources = SourceMap::new();
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "top-level-consumer.ko",
        "package p\nfun before(): Int = inferred",
    );
    let (provider_source, provider) = parsed(
        &mut sources,
        "top-level-provider.ko",
        "package p\nval inferred = 1\nfun after(): Int = inferred",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-consumer.ko", consumer_source, &consumer),
        SourceUnitInput::new("root", "p/z-provider.ko", provider_source, &provider),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unannotated forward value remains a deterministic deferred boundary");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed unannotated forward inputs keep the same boundary");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    let before = expression_with_text(&sources, &consumer, "inferred");
    let after = expression_with_text(&sources, &provider, "inferred");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(
                    source_unit(&names, consumer_source),
                    before,
                ))
                .expect("forward use type")
        ),
        Some(UnitTypeKind::Deferred(DeferredReason::ForwardValueType))
    ));
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(
                    source_unit(&names, provider_source),
                    after,
                ))
                .expect("later use type")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_top_level_initializers_recover_and_reject_return_outside_callable() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-top-level.ko",
        "val mismatch: Int = true\n\
         val outside = return 1\n\
         const val invalidConst: String = 2\n\
         fun later(): Int = 3",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-top-level.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid top-level initializers remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084", "L0086", "L0084"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["true", "return", "2"]
    );
    let unit = source_unit(&names, source);
    let later = expression_with_text(&sources, &file, "3");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(unit, later))
                .expect("later function still checked")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    let outside = symbol_named(&typed, &names, unit, "outside");
    assert!(matches!(
        typed
            .types()
            .get(typed.symbol_type(outside).expect("outside symbol type")),
        Some(UnitTypeKind::Error)
    ));
    assert!(typed.validate().is_err());
}

#[test]
fn remaining_expression_tails_publish_stable_types_and_traverse_cross_file_children() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "expression-tail-provider.ko",
        "package p\n\
         interface Parent { fun ping(input: Int): Unit {} }\n\
         fun produce(): Int = 1\n\
         fun target(input: Int): Int = input",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "expression-tail-consumer.ko",
        "package p\n\
         class Child : Parent {\n\
             override fun ping(input: Int): Unit {\n\
                 super<Parent>.ping(produce())\n\
                 val nullable: Int? = null\n\
                 val asserted: Int = nullable!!\n\
                 val casted = produce() as Long\n\
                 val propagated = produce()?\n\
                 val callable = ::target\n\
                 val bound = this::ping\n\
                 val closed = produce()..produce()\n\
                 val open = produce()..<produce()\n\
                 val pair = produce() to produce()\n\
                 val contains = produce() in produce()\n\
                 val missing = produce() !in produce()\n\
                 val fallback: Int = nullable ?: produce()\n\
             }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "p/b-consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("remaining expression tails preserve a typed recovery product");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed expression-tail inputs preserve the same product");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.type_ref_types(), reverse.type_ref_types());
    assert_eq!(typed.calls(), reverse.calls());
    let unit = source_unit(&names, consumer_source);
    let expression_kind = |text| {
        typed
            .expression_type(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &consumer, text),
            ))
            .and_then(|ty| typed.types().get(ty))
    };
    for text in ["nullable!!", "nullable ?: produce()"] {
        assert!(matches!(
            expression_kind(text),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
    for (text, reason) in [
        ("produce() as Long", DeferredReason::CastOrTypeTest),
        ("produce()?", DeferredReason::ErrorPropagation),
        ("::target", DeferredReason::OverloadSelection),
        ("this::ping", DeferredReason::OverloadSelection),
        ("produce()..produce()", DeferredReason::Call),
        ("produce()..<produce()", DeferredReason::Call),
        ("produce() to produce()", DeferredReason::Call),
        ("produce() in produce()", DeferredReason::Call),
        ("produce() !in produce()", DeferredReason::Call),
        (
            "super<Parent>.ping(produce())",
            DeferredReason::MemberAccess,
        ),
    ] {
        assert!(matches!(
            expression_kind(text),
            Some(UnitTypeKind::Deferred(actual)) if *actual == reason
        ));
    }
    for expression in expressions_with_text(&sources, &consumer, "produce()") {
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(unit, expression))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
    assert!(
        type_refs_with_text(&sources, &consumer, "Parent")
            .into_iter()
            .all(|type_ref| typed
                .type_ref_type(UnitTypeRefId::new(unit, type_ref))
                .is_some())
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_expression_tails_keep_single_file_diagnostics_and_later_recovery() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-expression-tails.ko",
        "fun consume(input: Int): Int = input\n\
         fun invalid(): Unit {\n\
             val asserted = 1!!\n\
             val fallback = 1 ?: consume(true)\n\
             val later: Int = consume(2)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-expression-tails.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid expression tails remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0085", "L0085", "L0084"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["!!", "?:", "true"]
    );
    let unit = source_unit(&names, source);
    let later = expression_with_text(&sources, &file, "consume(2)");
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(unit, later))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.validate().is_err());
}

#[test]
fn expression_tail_fallthrough_is_stable_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (tails_source, tails) = parsed(
        &mut sources,
        "expression-tail-flow.ko",
        "package p\n\
         fun missing(input: Int?): Int { input ?: return 0 }\n\
         fun closed(input: Nothing?): Int { input ?: return 0 }\n\
         fun casted(): Int { (return 1) as Int }\n\
         fun propagated(): Int { (return 1)? }\n\
         fun referenced(): Int { (return 1)::next }",
    );
    let (other_source, other) = parsed(
        &mut sources,
        "expression-tail-other.ko",
        "package p\nfun unaffected(): Int = 2",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-tails.ko", tails_source, &tails),
        SourceUnitInput::new("root", "p/b-other.ko", other_source, &other),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("expression-tail flow diagnostics remain recoverable");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed expression-tail flow inputs remain recoverable");

    assert_eq!(typed.body_diagnostics(), reverse.body_diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0088"]
    );
    let unit = source_unit(&names, other_source);
    let unaffected = expression_with_text(&sources, &other, "2");
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(unit, unaffected))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.validate().is_err());
}
