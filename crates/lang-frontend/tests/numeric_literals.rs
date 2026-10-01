//! SPEC-0229: radix/separator literals keep identical single-file and unit semantics.

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, CompilationUnitTypes, ConstValue, TypedFile, check_compilation_unit_types,
        check_types, standard_environments,
    },
};

fn checked(text: &str) -> (SourceMap, TypedFile, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("literals.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let single_names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(
        single_names.diagnostics().is_empty(),
        "{text}: {:?}",
        single_names.diagnostics()
    );
    let single = check_types(&sources, &parsed, &single_names, &types).unwrap();
    let inputs = [SourceUnitInput::new("root", "literals.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let unit_names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &unit_names, &types).unwrap();
    (sources, single, unit)
}

#[test]
fn radix_and_separator_constants_preserve_exact_values_and_types() {
    for (ty, literal, value) in [
        (BuiltinType::Byte, "0x7F", 127),
        (BuiltinType::Byte, "-0X80", -128),
        (BuiltinType::Short, "0b111_1111_1111_1111", 32767),
        (BuiltinType::Int, "1_000_000", 1_000_000),
        (BuiltinType::Int, "0x1f", 31),
        (BuiltinType::Int, "-0x8000_0000", -2_147_483_648),
        (
            BuiltinType::Long,
            "0x7FFF_FFFF_FFFF_FFFFL",
            i64::MAX as i128,
        ),
        (
            BuiltinType::Long,
            "-0x8000_0000_0000_0000L",
            i64::MIN as i128,
        ),
        (BuiltinType::UByte, "0xFFu", 255),
        (BuiltinType::UShort, "0XFFFFU", 65535),
        (BuiltinType::UInt, "0xFFFF_FFFFu", u32::MAX as i128),
        (
            BuiltinType::ULong,
            "0xFFFF_FFFF_FFFF_FFFFUL",
            u64::MAX as i128,
        ),
        (
            BuiltinType::ULong,
            "18_446_744_073_709_551_615uL",
            u64::MAX as i128,
        ),
        (BuiltinType::Int, "0B0010_1010", 42),
        (BuiltinType::Int, "000_042", 42),
    ] {
        let text = format!(
            "const val VALUE: {} = {literal}\nconst val COPY = VALUE",
            ty.name()
        );
        let (_, single, unit) = checked(&text);
        assert!(
            single.diagnostics().is_empty(),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        let expected = ConstValue::Integer { ty, value };
        let single = single.constants().expect("single-file constants");
        let unit = unit.validate_constants().expect("unit constants");
        assert_eq!(single.declarations().len(), 2);
        assert_eq!(unit.constants().declarations().len(), 2);
        for declaration in single.declarations() {
            assert_eq!(declaration.value(), &expected, "{text}");
        }
        for declaration in unit.constants().declarations() {
            assert_eq!(declaration.value(), &expected, "{text}");
        }
    }
}

#[test]
fn radix_and_separator_runtime_literals_keep_expected_and_default_types() {
    for (ty, literal) in [
        ("Int", "0x2A"),
        ("Int", "0X2a"),
        ("Int", "4_2"),
        ("Long", "0x8000_0000"),
        ("UInt", "0B1010u"),
        ("ULong", "0x1_0000_0000u"),
        ("Float", "1_234.5_625f"),
        ("Float", "1_000F"),
        ("Double", "1_234.5_625"),
    ] {
        let text = format!("fun value(): {ty} {{ val inferred = {literal}\nreturn inferred }}");
        let (_, single, unit) = checked(&text);
        assert!(
            single.diagnostics().is_empty(),
            "{text}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{text}: {:?}",
            unit.diagnostics()
        );
        assert!(unit.validate().is_ok(), "{text}");
    }
}

#[test]
fn extended_literal_overflow_keeps_numeric_diagnostics_and_source_spans() {
    for (ty, literal) in [
        ("Byte", "0x80"),
        ("Byte", "-0x81"),
        ("UByte", "0b1_0000_0000u"),
        ("Long", "0x8000_0000_0000_0000L"),
        ("ULong", "0x1_0000_0000_0000_0000uL"),
        ("Int", "0x1_0000_0000_0000_0000_0000_0000_0000_0000"),
        ("Int", "999_999_999_999_999_999_999_999_999_999_999_999_999"),
        (
            "Float",
            "999_999_999_999_999_999_999_999_999_999_999_999_999f",
        ),
    ] {
        let text = format!("fun value(): {ty} = {literal}");
        let (sources, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
            assert_eq!(diagnostics[0].code().to_string(), "L0090");
            assert_eq!(
                diagnostics[0].message(),
                "numeric literal is outside the representable range"
            );
            assert_eq!(
                sources.slice(diagnostics[0].primary_span()).unwrap(),
                literal.trim_start_matches('-')
            );
        }
    }
}

#[test]
fn extended_literal_range_errors_keep_expected_type_labels() {
    let double_overflow = format!("{}.0", ["999"; 110].join("_"));
    for (ty, literal) in [("Byte", "0x80"), ("Double", double_overflow.as_str())] {
        let text = format!("fun value(): {ty} = {literal}");
        let (sources, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
            assert_eq!(diagnostics[0].code().to_string(), "L0090");
            assert_eq!(
                sources.slice(diagnostics[0].primary_span()).unwrap(),
                literal
            );
            let labels = diagnostics[0]
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DiagnosticDetail::Label(label) => Some(sources.slice(label.span()).unwrap()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(labels, [ty]);
        }
    }
}

#[test]
fn extended_constants_are_exact_across_files_and_source_order() {
    let mut sources = SourceMap::new();
    let provider = sources
        .add_source(
            "provider.ko",
            "package p\nconst val BASE = 0x2A\nconst val WIDE = 0xFFFF_FFFF_FFFF_FFFFuL",
        )
        .unwrap();
    let consumer = sources.add_source("consumer.ko", "package q\nimport p.BASE\nconst val TOTAL = BASE + 0b10_1010 + 4_2\nconst val COPY = p.WIDE").unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let consumer_file = parse_file(&sources, &lex(&sources, consumer).unwrap()).unwrap();
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider, &provider_file),
        SourceUnitInput::new("root", "q/consumer.ko", consumer, &consumer_file),
    ];
    let (names, types) = standard_environments();
    let mut previous = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate_constants().unwrap();
        let facts = typed.constants();
        for declaration in facts.declarations() {
            let expected = match sources.slice(declaration.declaration_span()).unwrap() {
                "BASE" => ConstValue::Integer {
                    ty: BuiltinType::Int,
                    value: 42,
                },
                "TOTAL" => ConstValue::Integer {
                    ty: BuiltinType::Int,
                    value: 126,
                },
                "WIDE" | "COPY" => ConstValue::Integer {
                    ty: BuiltinType::ULong,
                    value: u64::MAX as i128,
                },
                other => panic!("unexpected constant {other}"),
            };
            assert_eq!(declaration.value(), &expected);
        }
        assert_eq!(facts.declarations().len(), 4);
        if let Some(previous) = previous {
            assert_eq!(facts, &previous);
        }
        previous = Some(facts.clone());
    }
}

#[test]
fn negative_unsigned_extended_literals_keep_operator_diagnostics() {
    for literal in ["0x1u", "0B1U", "1_0uL", "0xFFUL"] {
        let text = format!("fun value(): Unit {{ val invalid = -{literal} }}");
        let (sources, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
            assert_eq!(diagnostics[0].code().to_string(), "L0085");
            assert_eq!(sources.slice(diagnostics[0].primary_span()).unwrap(), "-");
        }
    }
}

#[test]
fn malformed_extended_literals_remain_lexical_errors() {
    for literal in [
        "0x", "0b", "0x_1", "0b2", "1__0", "100_", "1_.0", "1._0", "0x1ul",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("invalid.ko", literal).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert_eq!(lexed.diagnostics().len(), 1, "{literal}");
        assert_eq!(lexed.diagnostics()[0].code().to_string(), "L0008");
        assert_eq!(
            sources
                .slice(lexed.diagnostics()[0].primary_span())
                .unwrap(),
            literal
        );
    }
}

fn ownership_diagnostics(text: &str) -> [Vec<Diagnostic>; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source("indices.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let single_names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(single_names.diagnostics().is_empty());
    let single_types = check_types(&sources, &parsed, &single_names, &types).unwrap();
    assert!(
        single_types.diagnostics().is_empty(),
        "{:?}",
        single_types.diagnostics()
    );
    let single = check_ownership(&sources, &parsed, &single_names, &single_types).unwrap();
    let inputs = [SourceUnitInput::new("root", "indices.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let unit_names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
        .unwrap()
        .validate()
        .unwrap();
    let unit_types = check_compilation_unit_types(&sources, &inputs, &unit_names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit =
        check_compilation_unit_ownership(&sources, &inputs, &unit_names, &types, &unit_types)
            .unwrap();
    [single.diagnostics().to_vec(), unit.diagnostics().to_vec()]
}

#[test]
fn extended_indices_preserve_proven_disjoint_places_and_same_value_aliasing() {
    for (left, right, overlaps) in [
        ("0x0", "0b1", false),
        ("0_0", "0_1", false),
        ("(+0X0)", "0B1", false),
        ("0x1", "0b1", true),
        ("0_1", "1", true),
    ] {
        let source = format!(
            "class Resource {{}}\nfun mutate(inout first: Resource, inout second: Resource): Unit {{}}\nfun example(own list: MutableList<Resource>): Unit {{ val result = mutate(&list[{left}], &list[{right}]) }}"
        );
        for diagnostics in ownership_diagnostics(&source) {
            if overlaps {
                assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
                assert_eq!(diagnostics[0].code().to_string(), "L0135");
            } else {
                assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
            }
        }
    }
}
