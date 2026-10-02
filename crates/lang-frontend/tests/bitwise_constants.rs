//! Integer named bit operations share exact constant values across both Phase 2 entry points.

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        BuiltinType, CompilationUnitTypes, ConstValue, TypeKind, TypedFile, UnitTypeKind,
        check_compilation_unit_types, check_types, standard_environments,
    },
};

const INTEGER_TYPES: [(BuiltinType, u32, bool); 8] = [
    (BuiltinType::Byte, 8, true),
    (BuiltinType::Short, 16, true),
    (BuiltinType::Int, 32, true),
    (BuiltinType::Long, 64, true),
    (BuiltinType::UByte, 8, false),
    (BuiltinType::UShort, 16, false),
    (BuiltinType::UInt, 32, false),
    (BuiltinType::ULong, 64, false),
];

fn parse(sources: &SourceMap, source: lang_frontend::source::SourceId) -> ParsedFile {
    let parsed = parse_file(sources, &lex(sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    parsed
}

fn checked_single(text: &str) -> (SourceMap, TypedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("bitwise.ko", text).unwrap();
    let parsed = parse(&sources, source);
    let (ne, te) = standard_environments();
    let names = resolve_names(&sources, &parsed, &ne).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &te).unwrap();
    (sources, typed)
}

fn checked_unit(text: &str) -> (SourceMap, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("bitwise.ko", text).unwrap();
    let parsed = parse(&sources, source);
    let inputs = [SourceUnitInput::new("root", "bitwise.ko", source, &parsed)];
    let (ne, te) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
    (sources, typed)
}

fn literal(ty: BuiltinType, value: i128) -> String {
    let suffix = match ty {
        BuiltinType::Long => "L",
        BuiltinType::UByte | BuiltinType::UShort | BuiltinType::UInt => "u",
        BuiltinType::ULong => "uL",
        _ => "",
    };
    format!("{value}{suffix}")
}

// Explicit expected values distinguish bit-pattern truncation from checked arithmetic.
fn cases(width: u32, signed: bool) -> Vec<(&'static str, i128, i128, i128)> {
    let sign = 1i128 << (width - 1);
    let mask = 2 * sign - 1;
    let high = if signed { -sign } else { sign };
    let max = if signed { sign - 1 } else { mask };
    let all = if signed { -1 } else { mask };
    let width = i128::from(width);
    let mut cases = vec![
        ("and", 85, 51, 17),
        ("or", 85, 51, 119),
        ("xor", 85, 51, 102),
        ("and", all, high, high),
        ("or", high, max, all),
        ("xor", all, all, 0),
        ("shl", high, 0, high),
        ("shr", high, 0, high),
        ("ushr", high, 0, high),
        ("shl", 1, width, 1),
        ("shr", high, width, high),
        ("ushr", high, width, high),
        ("shl", 1, width + 1, 2),
        ("shr", all, width + 1, if signed { -1 } else { max / 2 }),
        ("ushr", all, width + 1, mask / 2),
        ("shl", 1, width - 1, high),
        ("shr", high, width - 1, if signed { -1 } else { 1 }),
        ("ushr", high, width - 1, 1),
        ("shl", max, 1, if signed { -2 } else { mask - 1 }),
        ("shl", high, 1, 0),
        ("shl", 1, max, high),
        ("shr", high, max, if signed { -1 } else { 1 }),
        ("ushr", high, max, 1),
    ];
    if signed {
        cases.extend([
            ("xor", -1, max, high),
            ("shl", 1, -1, high),
            ("shr", high, -1, -1),
            ("ushr", high, -1, 1),
            ("shl", high, -width, high),
            ("shr", high, -width, high),
            ("ushr", high, -width, high),
            ("shl", 1, -width - 1, high),
            ("shr", high, -width - 1, -1),
            ("ushr", high, -width - 1, 1),
            ("shl", high, -sign, high),
            ("shr", high, -sign, high),
            ("ushr", high, -sign, high),
        ]);
    }
    cases
}

fn matrix_source(ty: BuiltinType, width: u32, signed: bool) -> (String, Vec<ConstValue>) {
    let mut source = String::new();
    let mut expected = Vec::new();
    for (index, (operator, left, right, value)) in cases(width, signed).into_iter().enumerate() {
        // Forward references exercise dependency typing instead of relying on literal inference.
        source.push_str(&format!(
            "const val result{index} = left{index} {operator} right{index}\n\
             const val left{index}: {ty:?} = {}\n\
             const val right{index}: {ty:?} = {}\n",
            literal(ty, left),
            literal(ty, right),
        ));
        expected.push(ConstValue::Integer { ty, value });
    }
    (source, expected)
}

#[test]
fn single_file_bitwise_constants_cover_all_widths_and_masked_counts() {
    for (ty, width, signed) in INTEGER_TYPES {
        let (source, expected) = matrix_source(ty, width, signed);
        let (sources, typed) = checked_single(&source);
        assert!(
            typed.diagnostics().is_empty(),
            "{ty:?}: {:?}",
            typed.diagnostics()
        );
        let facts = typed.constants().unwrap();
        let mut actual = 0;
        for declaration in facts.declarations() {
            let name = sources.slice(declaration.declaration_span()).unwrap();
            if let Some(index) = name.strip_prefix("result") {
                assert_eq!(
                    declaration.value(),
                    &expected[index.parse::<usize>().unwrap()],
                    "{ty:?} {name}"
                );
                assert_eq!(
                    typed.types().get(declaration.ty()),
                    Some(&TypeKind::Builtin(ty))
                );
                assert_eq!(declaration.dependencies().len(), 2);
                actual += 1;
            }
        }
        assert_eq!(actual, expected.len());
    }
}

#[test]
fn compilation_unit_bitwise_constants_cover_all_widths_and_masked_counts() {
    for (ty, width, signed) in INTEGER_TYPES {
        let (source, expected) = matrix_source(ty, width, signed);
        let (sources, typed) = checked_unit(&source);
        assert!(
            typed.diagnostics().is_empty(),
            "{ty:?}: {:?}",
            typed.diagnostics()
        );
        let enabled = typed.validate_constants().unwrap();
        let mut actual = 0;
        for declaration in enabled.constants().declarations() {
            let name = sources.slice(declaration.declaration_span()).unwrap();
            if let Some(index) = name.strip_prefix("result") {
                assert_eq!(
                    declaration.value(),
                    &expected[index.parse::<usize>().unwrap()],
                    "{ty:?} {name}"
                );
                assert_eq!(
                    enabled.types().types().get(declaration.ty()),
                    Some(&UnitTypeKind::Builtin(ty))
                );
                assert_eq!(declaration.dependencies().len(), 2);
                actual += 1;
            }
        }
        assert_eq!(actual, expected.len());
    }
}

#[test]
fn cross_file_bitwise_dependencies_are_deterministic() {
    let mut sources = SourceMap::new();
    let a = sources.add_source("a.ko", "package a\nimport b.B\nobject A { const val RESULT = (B.LEFT ushr B.COUNT) xor B.MASK }\nfun result(): Long = A.RESULT").unwrap();
    let b = sources.add_source("b.ko", "package b\nobject B { const val LEFT: Long = -9223372036854775808L\nconst val COUNT: Long = -1L\nconst val MASK: Long = 3L }").unwrap();
    let fa = parse(&sources, a);
    let fb = parse(&sources, b);
    let inputs = [
        SourceUnitInput::new("root", "a/a.ko", a, &fa),
        SourceUnitInput::new("root", "b/b.ko", b, &fb),
    ];
    let (ne, te) = standard_environments();
    let mut previous = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let enabled = typed.validate_constants().unwrap();
        let facts = enabled.constants();
        let result = facts
            .declarations()
            .iter()
            .find(|declaration| sources.slice(declaration.declaration_span()).unwrap() == "RESULT")
            .unwrap();
        assert_eq!(
            result.value(),
            &ConstValue::Integer {
                ty: BuiltinType::Long,
                value: 2
            }
        );
        assert_eq!(result.dependencies().len(), 3);
        assert_eq!(facts.uses().len(), 4);
        if let Some(previous) = &previous {
            assert_eq!(facts, previous);
        }
        previous = Some(facts.clone());
    }
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn bitwise_constants_retain_operand_type_errors_without_evaluation_failures() {
    for operator in ["and", "or", "xor", "shl", "shr", "ushr"] {
        let source = format!(
            "const val left: Int = 1\nconst val right: Long = 1L\nconst val result = left {operator} right"
        );
        let (_, single) = checked_single(&source);
        let (_, unit) = checked_unit(&source);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert_eq!(codes(diagnostics), ["L0085"], "{operator}: {diagnostics:?}");
        }
        assert!(single.constants().is_none());
        assert!(unit.constants().is_none());
    }
}

#[test]
fn bitwise_constant_qualification_still_rejects_calls_and_checked_overflow() {
    for (source, code, span) in [
        (
            "fun value(): Int = 1\nconst val result = 1 shl value()",
            "L0156",
            "value()",
        ),
        (
            "fun value(): Int = 1\nconst val result = false && ((1 shl value()) == 0)",
            "L0156",
            "value()",
        ),
        ("const val result = (1 shl 31) - 1", "L0158", "-"),
        ("const val result = (1 shl 31) / -1", "L0158", "/"),
        ("const val result = 1 shl (1 / 0)", "L0158", "/"),
    ] {
        let (single_sources, single) = checked_single(source);
        let (unit_sources, unit) = checked_unit(source);
        for (sources, diagnostics) in [
            (&single_sources, single.diagnostics()),
            (&unit_sources, unit.diagnostics()),
        ] {
            assert_eq!(codes(diagnostics), [code], "{source}: {diagnostics:?}");
            assert_eq!(sources.slice(diagnostics[0].primary_span()).unwrap(), span);
        }
        assert!(single.constants().is_none());
        assert!(unit.constants().is_none());
    }
}

#[test]
fn bitwise_constant_short_circuit_preserves_syntax_dependency_cycles() {
    let source = "const val result: Boolean = false && ((1 shl count) == 0)\nconst val count: Int = 1 shl count";
    let (_, single) = checked_single(source);
    let (_, unit) = checked_unit(source);
    assert_eq!(codes(single.diagnostics()), ["L0157"]);
    assert_eq!(codes(unit.diagnostics()), ["L0157"]);
    assert!(single.constants().is_none());
    assert!(unit.constants().is_none());
    let source = "const val result = false && ((1 shl (1 / 0)) == 0)";
    let (_, single) = checked_single(source);
    let (_, unit) = checked_unit(source);
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(
        single.constants().unwrap().declarations()[0].value(),
        &ConstValue::Boolean(false)
    );
    assert_eq!(
        unit.constants().unwrap().declarations()[0].value(),
        &ConstValue::Boolean(false)
    );
}
