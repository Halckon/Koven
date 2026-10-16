//! Ordinary source red probes for SPEC-0327 backend delivery; never execute generated code.
use super::{
    LoweringError,
    lower_frontend::orchestrate::lower_scalar_file,
    model::Program,
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
    verify::verify_program,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

const INTEGERS: [&str; 8] = [
    "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
];

fn lower_source(text: &str, unit: bool) -> Result<Program, LoweringError> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if unit {
        let text = format!("package p\n{text}");
        let (source, file) = parsed(&mut sources, "p/convert.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/convert.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .map(|(program, _)| program)
    } else {
        let source = sources.add_source("convert.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let names = resolve_names(&sources, &file, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        complete_file_types(&sources, &file, &names, &types, &typed).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        lower_scalar_file(&sources, &file, &names, &typed, &owned)
    };
    match &result {
        Ok(program) => println!(
            "MODE unit={unit} SOURCE {text:?} SSA {}",
            render_program(program)
        ),
        Err(error) => println!("MODE unit={unit} SOURCE {text:?} LOWER {error:?}"),
    }
    result
}

fn checked_program(result: Result<Program, LoweringError>) -> Program {
    let program = result.expect("ordinary checked conversion must lower");
    verify_program(&program).expect("real lowered program verifies");
    program
}

fn pair_sources() -> Vec<(bool, String, String, Result<Program, LoweringError>)> {
    let mut cases = Vec::new();
    for unit in [false, true] {
        for source in INTEGERS {
            for target in INTEGERS {
                let text = format!("fun entry(value: {source}): {target} = value.to{target}()");
                cases.push((
                    unit,
                    source.to_owned(),
                    target.to_owned(),
                    lower_source(&text, unit),
                ));
            }
        }
    }
    cases
}

#[test]
fn all_integer_pairs_lower_from_real_typed_sources() {
    let cases = pair_sources();
    assert_eq!(cases.len(), 128);
    for (_, _, _, result) in cases {
        let _ = checked_program(result);
    }
    // Exact private operation/result/Abort-edge assertions are added when that API exists;
    // the initial red must be the actual UnsupportedNode, not a missing Rust symbol.
}

#[test]
fn conversion_receiver_call_has_one_source_evaluation() {
    let source =
        "fun tick(): Int { println(\"tick\"); return 1 }\nfun entry(): Long = tick().toLong()";
    let cases = [false, true].map(|unit| lower_source(source, unit));
    for result in cases {
        let program = checked_program(result);
        let rendered = render_program(&program);
        assert!(rendered.contains("tick"), "{rendered}");
    }
    // Bind the selected call result directly to the conversion operand in the implementation oracle.
}

#[test]
fn checked_conversion_preserves_live_owner_and_pending_argument() {
    let sources = [
        "fun entry(number: Int): Long { val retained = \"retained\"; val converted = number.toLong(); println(retained); return converted }",
        "fun accept(own label: String, number: Long): Long { println(label); return number }\nfun entry(number: Int): Long = accept(\"pending\", number.toLong())",
    ];
    let cases = sources
        .into_iter()
        .flat_map(|source| [false, true].map(|unit| lower_source(source, unit)))
        .collect::<Vec<_>>();
    for result in cases {
        let _ = checked_program(result);
    }
}

#[test]
fn all_integer_pairs_generate_verified_dynamic_llvm() {
    let cases = pair_sources();
    for (_, _, _, result) in cases {
        let program = checked_program(result);
        let llvm =
            crate::llvm::render_verified_program(&program).expect("real dynamic LLVM verifies");
        assert!(!llvm.contains("poison"), "{llvm}");
    }
    // Signed/unsigned i128 bounds and failure-result non-delivery are frozen with the adapter patch.
}
