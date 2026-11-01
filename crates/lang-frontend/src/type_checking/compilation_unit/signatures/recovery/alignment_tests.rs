use super::*;
use crate::{
    lexer::lex,
    name_resolution::{NameEnvironment, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn parse(sources: &mut SourceMap, name: &str, text: &str) -> ParsedFile {
    let id = sources.add_source(name, text).unwrap();
    parse_file(sources, &lex(sources, id).unwrap()).unwrap()
}

fn names(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    environment: &NameEnvironment,
) -> crate::name_resolution::ValidatedCompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).unwrap();
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .unwrap()
        .validate()
        .unwrap()
}

#[test]
fn canonical_files_keep_original_ast_identity_across_root_and_path_permutations() {
    let mut sources = SourceMap::new();
    // Registration/display order deliberately disagrees with canonical root/path order.
    let z = parse(&mut sources, "first", "fun zebra(): Int = 3");
    let a = parse(&mut sources, "last", "fun alpha(): Int = 1");
    let b = parse(&mut sources, "middle", "fun beta(): Int = 2");
    let inputs = [
        SourceUnitInput::new("z", "same.ko", z.source_id(), &z),
        SourceUnitInput::new("a", "same.ko", a.source_id(), &a),
        SourceUnitInput::new("a", "before.ko", b.source_id(), &b),
    ];
    let (environment, types) = standard_environments();
    let names = names(&sources, &inputs, &environment);
    let expected = [&b, &a, &z];
    let mut baseline = None;
    for order in [[0, 1, 2], [2, 1, 0], [1, 0, 2]] {
        let reordered = order.map(|i| inputs[i]);
        let prepared =
            collect_signatures_with_files(&sources, &reordered, names.names(), &types).unwrap();
        assert_eq!(prepared.files.len(), expected.len());
        for ((actual, expected), source) in prepared
            .files
            .iter()
            .zip(expected)
            .zip(names.names().index().source_units())
        {
            assert!(std::ptr::eq(*actual, expected));
            assert_eq!(actual.source_id(), source.source_id());
        }
        let typed = check_compilation_unit_types(&sources, &reordered, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        assert!(typed.clone().validate().is_ok());
        if let Some(baseline) = &baseline {
            assert_eq!(&typed, baseline);
        } else {
            baseline = Some(typed);
        }
    }
}

#[test]
fn aligned_signature_and_body_inputs_preserve_memory_recovery_and_error_priority() {
    for memory in [false, true] {
        let mut sources = SourceMap::new();
        let text = "fun wrong(): Int = true";
        let file = parse(&mut sources, "document", text);
        let source = file.source_id();
        let input = if memory {
            SourceUnitInput::memory_document(source, &file)
        } else {
            SourceUnitInput::new("root", "main.ko", source, &file)
        };
        let inputs = [input];
        let (environment, types) = standard_environments();
        let names = names(&sources, &inputs, &environment);
        let prepared =
            collect_signatures_with_files(&sources, &inputs, names.names(), &types).unwrap();
        assert_eq!(prepared.files.len(), 1);
        assert!(std::ptr::eq(prepared.files[0], &file));
        assert!(prepared.signatures.diagnostics().is_empty());
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_eq!(typed.diagnostics().len(), 1);
        assert_eq!(typed.diagnostics()[0].code().to_string(), "L0084");
        assert_eq!(
            typed.diagnostics()[0].primary_span(),
            sources
                .span(source, text.find("true").unwrap(), text.len())
                .unwrap()
        );
        assert!(typed.validate().is_err());

        let (_, foreign_types) = standard_environments();
        assert!(matches!(
            collect_signatures_with_files(&sources, &inputs, names.names(), &foreign_types),
            Err(CompilationUnitTypeError::MismatchedNameEnvironment)
        ));
        // Invalid inputs must win over the independently foreign environment.
        let duplicate = [input, input];
        assert!(matches!(
            collect_signatures_with_files(&sources, &duplicate, names.names(), &foreign_types),
            Err(CompilationUnitTypeError::MismatchedInputs)
        ));
        let changed_key = [SourceUnitInput::new("other", "other.ko", source, &file)];
        assert!(matches!(
            collect_signatures_with_files(&sources, &changed_key, names.names(), &foreign_types),
            Err(CompilationUnitTypeError::MismatchedInputs)
        ));
        let mut foreign_sources = SourceMap::new();
        let foreign_file = parse(&mut foreign_sources, "document", text);
        assert!(matches!(
            collect_signatures_with_files(&foreign_sources, &inputs, names.names(), &types),
            Err(CompilationUnitTypeError::MismatchedInputs)
        ));
        let wrong_ast = [SourceUnitInput::new(
            "root",
            "main.ko",
            source,
            &foreign_file,
        )];
        assert!(matches!(
            collect_signatures_with_files(&sources, &wrong_ast, names.names(), &types),
            Err(CompilationUnitTypeError::MismatchedInputs)
        ));
    }
}
