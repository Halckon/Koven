//! SPEC-0319: raw/base contracts that the new final qualification must preserve.
//! These source-only controls do not themselves claim final admission succeeds.

use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{
        SourceUnitId, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypes, DeferredReason, TypeKind, TypedFile, UnitExpressionId,
        UnitTypeKind, check_compilation_unit_types, check_types, standard_environments,
    },
};

const FROZEN: &[(&str, &str, DeferredReason)] = &[
    (
        include_str!("fixtures/final-type-admission-0319/r01_bogus_member.ko"),
        "a.fooBar()",
        DeferredReason::MemberAccess,
    ),
    (
        include_str!("fixtures/final-type-admission-0319/r02_bogus_member_str.ko"),
        "s.length",
        DeferredReason::MemberAccess,
    ),
    (
        include_str!("fixtures/final-type-admission-0319/p34_for_range.ko"),
        "0..<5",
        DeferredReason::Call,
    ),
    (
        include_str!("fixtures/final-type-admission-0319/s04_question_on_nullable.ko"),
        "x?",
        DeferredReason::ErrorPropagation,
    ),
];

fn parsed(sources: &mut SourceMap, path: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(path, text).expect("source");
    let lexed = lex(sources, source).expect("lex");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    (source, file)
}

fn expression(sources: &SourceMap, file: &ParsedFile, text: &str) -> ExpressionId {
    let matches = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "unique expression: {text}");
    matches[0]
}

fn with_types(
    text: &str,
    check: impl FnOnce(&SourceMap, &ParsedFile, &TypedFile, &CompilationUnitTypes, SourceUnitId),
) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &file, &names, &types).expect("single raw types");
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let unit_id = names.names().index().source_units()[0].id();
    let unit =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit raw types");
    check(&sources, &file, &single, &unit, unit_id);
}

#[test]
fn frozen_probe_roots_preserve_raw_deferred_recovery() {
    for &(text, root, reason) in FROZEN {
        for constants in [false, true] {
            let input = if constants {
                format!("const val PREFIX: Int = 1\n{text}")
            } else {
                text.to_owned()
            };
            with_types(&input, |sources, file, single, unit, unit_id| {
                assert!(
                    single.diagnostics().is_empty(),
                    "{:?}",
                    single.diagnostics()
                );
                assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
                let root = expression(sources, file, root);
                assert_eq!(
                    single
                        .expression_type(root)
                        .and_then(|ty| single.types().get(ty)),
                    Some(&TypeKind::Deferred(reason)),
                );
                assert_eq!(
                    unit.expression_type(UnitExpressionId::new(unit_id, root))
                        .and_then(|ty| unit.types().get(ty)),
                    Some(&UnitTypeKind::Deferred(reason)),
                );
                // Base/constant qualifications keep their published meaning. Neither is final.
                if constants {
                    assert!(unit.clone().validate().is_err());
                    assert!(unit.clone().validate_constants().is_ok());
                } else {
                    assert!(unit.clone().validate().is_ok());
                }
            });
        }
    }
}

const VALID_ROLES: &str = "value class Item(val n: Int)\n\
        fun identity(n: Int): Int = n\n\
        fun <T: Any> symbolic(value: T): T = value\n\
        fun invoke(f: (Int) -> Int): Int = f(1)\n\
        fun stop(): Nothing = error(\"stop\")\n\
        fun main(): Unit {\n\
            var n: Int = 0\n\
            n = identity(1)\n\
            val item = Item(n)\n\
            val text = \"ok\".clone()\n\
            val bits = n.inv()\n\
            val array = Array<Int>(2, { index -> index })\n\
            val list = List<Int>(2, { index -> index })\n\
            val mutable = MutableList<Int>()\n\
            val count = list.size\n\
            println(text)\n\
        }";

#[test]
fn resolved_roles_and_symbolic_types_keep_raw_value_facts() {
    let text = VALID_ROLES;
    with_types(text, |sources, file, single, unit, unit_id| {
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        for (text, builtin) in [
            ("identity(1)", BuiltinType::Int),
            ("f(1)", BuiltinType::Int),
            ("\"ok\".clone()", BuiltinType::String),
            ("n.inv()", BuiltinType::Int),
            ("list.size", BuiltinType::Int),
            ("println(text)", BuiltinType::Unit),
            ("error(\"stop\")", BuiltinType::Nothing),
        ] {
            let id = expression(sources, file, text);
            assert_eq!(
                single
                    .expression_type(id)
                    .and_then(|ty| single.types().get(ty)),
                Some(&TypeKind::Builtin(builtin)),
                "single {text}"
            );
            assert_eq!(
                unit.expression_type(UnitExpressionId::new(unit_id, id))
                    .and_then(|ty| unit.types().get(ty)),
                Some(&UnitTypeKind::Builtin(builtin)),
                "unit {text}"
            );
        }
        let local = |text| expression(sources, file, text);
        let qualified = |text| UnitExpressionId::new(unit_id, local(text));
        assert!(single.call(local("identity(1)")).is_some());
        assert!(unit.call(qualified("identity(1)")).is_some());
        assert!(single.construction(local("Item(n)")).is_some());
        assert!(unit.construction(qualified("Item(n)")).is_some());
        assert!(single.string_operation(local("\"ok\".clone()")).is_some());
        assert!(unit.string_operation(qualified("\"ok\".clone()")).is_some());
        assert!(single.integer_operation(local("n.inv()")).is_some());
        assert!(unit.integer_operation(qualified("n.inv()")).is_some());
        assert!(single.container_size(local("list.size")).is_some());
        assert!(unit.container_size(qualified("list.size")).is_some());
        for constructor in [
            "Array<Int>(2, { index -> index })",
            "List<Int>(2, { index -> index })",
            "MutableList<Int>()",
        ] {
            assert!(single.container_construction(local(constructor)).is_some());
            assert!(
                unit.container_construction(qualified(constructor))
                    .is_some()
            );
        }
        assert!(matches!(
            single
                .expression_type(local("value"))
                .and_then(|ty| single.types().get(ty)),
            Some(TypeKind::TypeParameter(_))
        ));
        assert!(matches!(
            unit.expression_type(qualified("value"))
                .and_then(|ty| unit.types().get(ty)),
            Some(UnitTypeKind::TypeParameter(_))
        ));
        assert_eq!(
            single
                .expression_type(local("n = identity(1)"))
                .and_then(|ty| single.types().get(ty)),
            Some(&TypeKind::Deferred(DeferredReason::Assignment))
        );
        assert!(unit.assignment(qualified("n = identity(1)")).is_some());
        assert!(unit.clone().validate().is_ok());
    });
}

#[test]
fn existing_root_errors_do_not_cascade_through_calls_or_iteration() {
    let text = "// 中文使字节偏移不同于字符偏移\n\
        fun take(n: Int): Int = n\n\
        fun bad(): Int = take((true + false)!!)\n\
        fun iterate(): Unit { for (_ in true + false) {} }\n\
        fun good(): Long = 2L";
    with_types(text, |sources, file, single, unit, unit_id| {
        let expected = text
            .match_indices('+')
            .map(|(start, _)| (start, start + 1))
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), 2);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
            for (diagnostic, &(start, end)) in diagnostics.iter().zip(&expected) {
                assert_eq!(diagnostic.code().to_string(), "L0085");
                assert_eq!(
                    (
                        diagnostic.primary_span().start(),
                        diagnostic.primary_span().end()
                    ),
                    (start, end)
                );
                assert_eq!(sources.slice(diagnostic.primary_span()), Ok("+"));
                assert!(start > text[..start].chars().count());
            }
        }
        assert!(single.sequential_iterations().is_empty());
        assert!(unit.sequential_iterations().is_empty());
        let good = expression(sources, file, "2L");
        assert_eq!(
            single
                .expression_type(good)
                .and_then(|ty| single.types().get(ty)),
            Some(&TypeKind::Builtin(BuiltinType::Long))
        );
        assert_eq!(
            unit.expression_type(UnitExpressionId::new(unit_id, good))
                .and_then(|ty| unit.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Long))
        );
        assert!(unit.clone().validate().is_err());
    });
}

#[test]
fn selected_source_order_and_forward_types_keep_basic_qualification() {
    let mut sources = SourceMap::new();
    // A SourceMap can hold files outside the selected compilation unit.
    let _unselected = sources
        .add_source("not-selected.ko", FROZEN[3].0)
        .expect("source");
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "consumer.ko",
        "package p\nfun before(): Int = inferred",
    );
    let (provider_source, provider) = parsed(
        &mut sources,
        "provider.ko",
        "package p\nval inferred = 1\nfun after(): Int = inferred",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-consumer.ko", consumer_source, &consumer),
        SourceUnitInput::new("root", "p/z-provider.ko", provider_source, &provider),
    ];
    let (environment, types) = standard_environments();
    let mut snapshots = Vec::new();
    for selected in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &selected).expect("index");
        let names = resolve_compilation_unit_names(&sources, &selected, &index, &environment)
            .expect("names")
            .validate()
            .expect("valid names");
        assert_eq!(names.names().index().source_units().len(), 2);
        let typed =
            check_compilation_unit_types(&sources, &selected, &names, &types).expect("raw types");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let source_unit = |source| {
            names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == source)
                .expect("selected source")
                .id()
        };
        let before = UnitExpressionId::new(
            source_unit(consumer_source),
            expression(&sources, &consumer, "inferred"),
        );
        let after = UnitExpressionId::new(
            source_unit(provider_source),
            expression(&sources, &provider, "inferred"),
        );
        assert_eq!(
            typed
                .expression_type(before)
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Deferred(DeferredReason::ForwardValueType))
        );
        assert_eq!(
            typed
                .expression_type(after)
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
        snapshots.push(typed.expression_types().clone());
        assert!(
            typed.validate().is_ok(),
            "base validation is deliberately not final"
        );
    }
    assert_eq!(snapshots[0], snapshots[1]);
}

#[path = "final_type_admission/complete.rs"]
mod complete;
