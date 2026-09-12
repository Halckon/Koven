use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::{
    LoweringErrorKind,
    model::{EntityType, Operation, ScalarConstant, SsaTypeKind},
    render::render_program,
    unit_lower::constant::lower_constant_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
};

#[test]
fn scalar_materializations_use_exact_values_and_reject_foreign_analysis() {
    for (ty, value, expected) in [
        ("Boolean", "true", ScalarConstant::Boolean(true)),
        ("Byte", "127", ScalarConstant::Integer(127)),
        ("Short", "32767", ScalarConstant::Integer(32767)),
        ("Int", "2147483647", ScalarConstant::Integer(2147483647)),
        (
            "Long",
            "9223372036854775807",
            ScalarConstant::Integer(9223372036854775807),
        ),
        ("UByte", "255u", ScalarConstant::Integer(255)),
        ("UShort", "65535u", ScalarConstant::Integer(65535)),
        ("UInt", "4294967295u", ScalarConstant::Integer(4294967295)),
        (
            "ULong",
            "18446744073709551615u",
            ScalarConstant::Integer(18446744073709551615),
        ),
        ("Char", "'界'", ScalarConstant::Char(u32::from('界'))),
    ] {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(
            &mut sources,
            "p/provider.ko",
            &format!(
                "package p\nconst val VALUE: {ty} = {value}\nfun unused(own flag: Boolean): Boolean = flag && true"
            ),
        );
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            &format!("package q\nimport p.VALUE\nfun entry(): {ty} = VALUE"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .unwrap()
            .validate_constants()
            .unwrap();
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .unwrap()
        .validate()
        .unwrap();
        let entry = declaration(&names, "q", "entry");
        let (forward, _) = lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap_or_else(|error| panic!("{ty}: {error:?}"));
        let (reverse, _) = lower_constant_unit_with_entry(
            &sources,
            &[inputs[1], inputs[0]],
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .unwrap();
        assert_eq!(render_program(&forward), render_program(&reverse));
        // No declaration initializer function or storage root may appear at runtime.
        assert_eq!(forward.modules[0].functions.len(), 1, "{ty}");
        let expected_type = match ty {
            "Boolean" => SsaTypeKind::Boolean,
            "Char" => SsaTypeKind::Char,
            name => SsaTypeKind::Integer {
                bits: match name {
                    "Byte" | "UByte" => 8,
                    "Short" | "UShort" => 16,
                    "Int" | "UInt" => 32,
                    _ => 64,
                },
                signed: !name.starts_with('U'),
            },
        };
        let module = &forward.modules[0];
        let function = &module.functions[0];
        assert_eq!(
            module.types[function.return_types[0].index()],
            expected_type,
            "{ty}"
        );
        for instruction in &function.instructions {
            if matches!(instruction.operation, Operation::Constant(_)) {
                let result_type = function.entity(instruction.results[0]).unwrap().ty;
                assert_eq!(
                    result_type,
                    EntityType::Value(function.return_types[0]),
                    "{ty}"
                );
            }
        }
        let constants = forward.modules[0].functions[0]
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::Constant(value) = &instruction.operation {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(constants, [expected], "{ty}");
        let other_typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .unwrap()
            .validate_constants()
            .unwrap();
        let error = lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &other_typed,
            &owned,
            entry,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
        let (_, other_environment) = standard_environments();
        let error = lower_constant_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &other_environment,
            &typed,
            &owned,
            entry,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
        let renamed = [
            inputs[0],
            SourceUnitInput::new("root", "q/renamed.ko", consumer_source, &consumer),
        ];
        let error = lower_constant_unit_with_entry(
            &sources,
            &renamed,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
    }
}

#[test]
fn string_uses_materialize_independent_owners_with_literal_cleanup() {
    for (signature, body, literals, drops) in [
        ("String", "= TEXT", 1, 0),
        ("String", "= ((TEXT))", 1, 0),
        ("Unit", "{ val done = println(TEXT) }", 1, 1),
        ("Unit", "{ val done = view(TEXT, (TEXT)) }", 2, 2),
        ("Unit", "{ val done = consume(TEXT) }", 1, 0),
        ("Unit", "{ val done = consume((TEXT)) }", 1, 0),
        ("String", "= TEXT + TEXT", 2, 2),
        ("Boolean", "= (TEXT) == TEXT", 2, 2),
        ("Boolean", "= TEXT != (TEXT)", 2, 2),
    ] {
        for spelling in ["TEXT", "p.TEXT", "\"界\\n\""] {
            let body = body.replace("TEXT", spelling);
            let mut sources = SourceMap::new();
            let (provider_source, provider) = parsed(
                &mut sources,
                "p/provider.ko",
                "package p\nconst val TEXT = \"界\\n\"\nfun view(first: String, second: String): Unit {}\nfun consume(own text: String): Unit {}",
            );
            let (consumer_source, consumer) = parsed(
                &mut sources,
                "q/consumer.ko",
                &format!(
                    "package q\nimport p.TEXT\nimport p.view\nimport p.consume\nfun entry(): {signature} {body}"
                ),
            );
            let inputs = [
                SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
                SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
            ];
            let (name_environment, environment) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
                    .unwrap()
                    .validate()
                    .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
                .unwrap()
                .validate_constants()
                .unwrap();
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .unwrap()
            .validate()
            .unwrap();
            let entry = declaration(&names, "q", "entry");
            let (program, entry) = lower_constant_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
            )
            .unwrap_or_else(|error| panic!("{body}: {error:?}"));
            let function = program.modules[0].function(entry).unwrap();
            let owners = function
                .instructions
                .iter()
                .filter_map(|instruction| match &instruction.operation {
                    Operation::StringLiteral { bytes, .. } => {
                        assert_eq!(bytes, "界\n".as_bytes());
                        Some(instruction.results[0])
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(owners.len(), literals, "{body}");
            assert_eq!(
                owners
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                literals,
                "each use owns a distinct value"
            );
            let dropped = function
                .instructions
                .iter()
                .filter_map(|instruction| match instruction.operation {
                    Operation::Drop { owner } => Some(super::model::EntityId::Value(owner)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(dropped.len(), drops, "{body}");
            if drops == literals {
                assert_eq!(
                    dropped,
                    owners.iter().copied().rev().collect::<Vec<_>>(),
                    "{body}"
                );
            }
            crate::llvm::render_verified_program(&program)
                .expect("String constants produce verified LLVM");
        }
    }
}
