//! Ordinary Range storage reaches its value ABI; independent nullable use stays bounded.
use super::{
    LoweringError, LoweringErrorKind,
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
fn lower_source(text: &str, unit: bool) -> Result<Program, LoweringError> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if unit {
        let text = format!("package p\n{text}");
        let (source, file) = parsed(&mut sources, "p/range.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/range.ko", source, &file)];
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
        let source = sources.add_source("range.ko", text).unwrap();
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

#[test]
fn range_type_uses_reach_value_abi_or_existing_nullable_boundary() {
    let mut results = Vec::new();
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        for text in [
            format!("fun entry(value: {range}): Unit {{}}"),
            format!("fun entry(value: {range}): {range} = value"),
            format!("fun entry(value: List<{range}>): Unit {{}}"),
            format!("fun entry(value: Map<Int, {range}>): Unit {{}}"),
            format!("fun entry(value: {range}?): Unit {{}}"),
            format!("fun entry(): Unit {{ val value: {range}? = null }}"),
            format!("class Holder(val value: {range})\nfun entry(value: Holder): Unit {{}}"),
            format!("fun entry(a: {integer}, b: {integer}): {range} = a..b"),
            format!("fun entry(a: {integer}, b: {integer}): List<{range}> = listOf(a..<b)"),
            format!("fun entry(a: {integer}, b: {integer}): Boolean = a in (a..b)"),
            format!("fun entry(a: {integer}, b: {integer}): Boolean = (a..b) == (a..<b)"),
        ] {
            for unit in [false, true] {
                println!("BEGIN unit={unit} SOURCE {text:?}");
                results.push((
                    text.contains(&format!("{range}?")),
                    lower_source(&text, unit),
                ));
            }
        }
    }
    let mut supported = 0;
    let mut nullable_rejected = 0;
    for (nullable, result) in results {
        if nullable {
            let error = result
                .err()
                .expect("independent nullable Range retains its existing ABI boundary");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            assert!(error.span.is_some());
            nullable_rejected += 1;
        } else {
            let program = result.expect("ordinary Range value storage is supported");
            verify_program(&program).unwrap();
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            println!("TYPE USE LLVM {llvm}");
            supported += 1;
        }
    }
    assert_eq!((supported, nullable_rejected), (36, 8));
}

#[test]
fn unused_range_environment_seeds_do_not_block_ordinary_lowering() {
    for unit in [false, true] {
        let program = lower_source("fun entry(value: Int): Int = value + 1", unit).unwrap();
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        assert!(llvm.contains("define"));
    }
}

#[test]
fn numeric_iteration_reaches_verified_value_backend() {
    let mut observed = Vec::new();
    for range in ["IntRange", "LongRange"] {
        for unit in [false, true] {
            let text = format!(
                "fun entry(value: {range}): Unit {{ for (item in value) {{ val copy = item }} }}"
            );
            let offset = if unit { "package p\n".len() } else { 0 };
            let start = text.find("for (").unwrap() + offset;
            let end = text.rfind(" }").unwrap() + offset;
            observed.push((lower_source(&text, unit), start, end));
        }
    }
    for (result, start, end) in observed {
        let program = result.expect("numeric provider reaches its value backend");
        verify_program(&program).unwrap();
        assert!(
            program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .flat_map(|function| &function.instructions)
                .any(
                    |instruction| matches!(&instruction.origin, super::model::Origin::Source(span)
                if (span.start(), span.end()) == (start, end))
                )
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}
