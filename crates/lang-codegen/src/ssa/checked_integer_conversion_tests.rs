//! Ordinary source: verified conversion SSA reaches the explicit pending LLVM boundary.
use super::{
    lower_frontend::orchestrate::lower_scalar_file,
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

#[test]
fn single_conversion_identity_reaches_pending_llvm_boundary() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("fun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let source = sources.add_source("conversion.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &file, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        complete_file_types(&sources, &file, &names, &types, &typed).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty());
        let program = lower_scalar_file(&sources, &file, &names, &typed, &owned).unwrap();
        assert_pending_llvm(&program);
    }
}

#[test]
fn unit_conversion_identity_reaches_pending_llvm_boundary() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("package p\nfun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "p/conversion.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/conversion.ko",
            source,
            &file,
        )];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, typed.types()).unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .unwrap();
        assert_pending_llvm(&program);
    }
}

fn assert_pending_llvm(program: &super::model::Program) {
    verify_program(program).unwrap();
    let error = crate::llvm::render_verified_program(program).unwrap_err();
    println!("verified conversion SSA: {error:?}");
    assert!(
        matches!(error, crate::llvm::LlvmAdapterError::Unsupported(message)
        if message == "checked integer conversion LLVM lowering is not implemented")
    );
}
