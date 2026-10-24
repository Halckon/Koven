//! Ordinary source: identity and cross-width conversion preserve the real LLVM pipeline.
use super::{
    unit_lower::lower_owned_unit_with_entry, unit_lower_test_support::parsed,
    verify::verify_program,
};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

#[test]
fn single_conversion_identity_reaches_verified_llvm() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("fun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let source = sources.add_source("conversion.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source, &file)];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let mut entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "entry"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == source
            });
        let entry = entries.next().expect("conversion entry exists").id();
        assert!(entries.next().is_none(), "conversion entry is unique");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let (program, _) = lower_owned_unit_with_entry(&unit, entry).unwrap();
        assert_verified_llvm(&program);
    }
}

#[test]
fn unit_conversion_identity_reaches_verified_llvm() {
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
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let mut entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "entry"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == source
            });
        let entry = entries.next().expect("conversion entry exists").id();
        assert!(entries.next().is_none(), "conversion entry is unique");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let (program, _) = lower_owned_unit_with_entry(&unit, entry).unwrap();
        assert_verified_llvm(&program);
    }
}

fn assert_verified_llvm(program: &super::model::Program) {
    verify_program(program).unwrap();
    let llvm = crate::llvm::render_verified_program(program).unwrap();
    println!("verified conversion LLVM: {llvm}");
    assert!(llvm.contains("icmp slt i128"));
    assert!(llvm.contains("icmp sgt i128"));
    assert!(llvm.contains("call void @koven.abort.report"));
}
