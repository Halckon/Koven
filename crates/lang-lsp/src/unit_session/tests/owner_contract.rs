//! 直接读取生产 UnitSnapshot 的唯一名称 owner，不能由独立工厂测试代替。
use lang_frontend::{
    analysis::UnitNameAnalysisError,
    lexer::lex,
    name_resolution::{
        LogicalPathError, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::parse_file,
};

use super::*;

#[test]
fn session_name_owner_matches_complete_same_source_manual_prefix_and_ast_pairing() {
    for consumer in [VALID, NAME_RECOVERY, TYPED_RECOVERY, OWNERSHIP, CONSTANT] {
        let session = UnitSession::new(config(consumer, true)).unwrap();
        let owner = &session.snapshot.name_snapshot;
        let parsed = owner
            .descriptors()
            .iter()
            .map(|unit| {
                let lexed = lex(owner.sources(), unit.source_id()).unwrap();
                parse_file(owner.sources(), &lexed).unwrap()
            })
            .collect::<Vec<_>>();
        let inputs = owner
            .descriptors()
            .iter()
            .zip(&parsed)
            .map(|(unit, parsed)| {
                SourceUnitInput::new(
                    unit.root_identity(),
                    unit.logical_path(),
                    unit.source_id(),
                    parsed,
                )
            })
            .collect::<Vec<_>>();
        let index = index_compilation_unit(owner.sources(), &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(owner.sources(), &inputs, &index, owner.environment())
                .unwrap();
        assert_eq!(owner.names(), &names);
        assert_eq!(owner.names().index(), &index);
        assert_eq!(owner.names().diagnostics(), names.diagnostics());
        assert_eq!(owner.validated_names().is_some(), names.validate().is_ok());
        let actual_inputs = owner.inputs();
        assert_eq!(actual_inputs.len(), parsed.len());
        assert_eq!(owner.parsed_files().len(), parsed.len());
        assert_eq!(session.snapshot.source_ids.len(), parsed.len());
        for (i, ((descriptor, actual), expected)) in owner
            .descriptors()
            .iter()
            .zip(owner.parsed_files())
            .zip(&parsed)
            .enumerate()
        {
            let source = &session.config.sources()[i];
            let indexed = &owner.names().index().source_units()[i];
            assert_eq!(session.snapshot.source_ids[i], descriptor.source_id());
            assert_eq!(descriptor.root_identity(), source.root());
            assert_eq!(descriptor.logical_path(), source.logical_path());
            assert_eq!(descriptor.source_id(), indexed.source_id());
            assert_eq!(indexed.key().root().as_str(), source.root());
            assert_eq!(indexed.key().logical_path().as_str(), source.logical_path());
            assert_eq!(actual.source_id(), descriptor.source_id());
            assert_eq!(
                owner.sources().source_text(actual.source_id()).unwrap(),
                source.text()
            );
            assert_eq!(actual.package(), expected.package());
            assert_eq!(actual.imports(), expected.imports());
            assert_eq!(actual.roots(), expected.roots());
            assert_eq!(actual.diagnostics(), expected.diagnostics());
            assert_eq!(
                format!("{:?}", actual.ast()),
                format!("{:?}", expected.ast())
            );
            assert_eq!(actual_inputs[i].source_id(), actual.source_id());
            assert!(std::ptr::eq(actual_inputs[i].parsed(), actual));
        }
    }
}

#[test]
fn unit_name_analysis_errors_preserve_session_variants_and_messages() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("source", "").unwrap();
    let lexer = || LexerInternalError::Source(SourceError::InvalidSourceId { source_id: id });
    let parser = || ParserInternalError::InvalidLexemeStream;
    let input = || CompilationUnitInputError::InvalidLogicalPath {
        path: "../bad.ko".to_owned(),
        reason: LogicalPathError::ParentSegment,
    };
    let name = || CompilationUnitNameError::MismatchedIndex;
    for (factory, original) in [
        (
            UnitNameAnalysisError::Lexer(lexer()),
            UnitSessionError::Lexer(lexer()),
        ),
        (
            UnitNameAnalysisError::Parser(parser()),
            UnitSessionError::Parser(parser()),
        ),
        (
            UnitNameAnalysisError::Input(input()),
            UnitSessionError::Input(input()),
        ),
        (
            UnitNameAnalysisError::Name(name()),
            UnitSessionError::Name(name()),
        ),
    ] {
        let actual = UnitSessionError::from(factory);
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&original)
        );
        assert_eq!(actual.to_string(), original.to_string());
        assert_eq!(format!("{actual:?}"), format!("{original:?}"));
        assert!(Error::source(&actual).is_none());
    }
}
