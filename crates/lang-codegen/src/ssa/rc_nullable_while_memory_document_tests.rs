//! Stage A: unchanged SPEC-0332 sources through MemoryDocument admission.
//! Keep the original Single/Rooted witnesses; these assertions require CI execution.
use super::*;
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use lang_frontend::{name_resolution::SourceUnitKey, parser::Expression};

fn original_memory_source(case: &str, text: &str) -> UnitSource {
    let name = format!("{case}.ko");
    let unit = UnitSource::analyze(&name, text)
        .expect("unchanged source must pass strict MemoryDocument frontend admission");
    unit.assert_frontend_clean();
    let source = unit.parsed().source_id();
    assert_eq!(unit.sources().source_name(source), Ok(name.as_str()));
    assert_eq!(unit.sources().source_text(source), Ok(text));
    let [indexed] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument source");
    };
    assert_eq!(indexed.source_id(), source);
    assert_eq!(indexed.key(), &SourceUnitKey::MemoryDocument);
    unit
}

#[test]
fn original_payload_sources_keep_precise_memory_document_return_boundary() {
    for (case, text) in sources() {
        let unit = original_memory_source(case, text);
        // Locate the actual Return node in the original bytes, not an offset
        // obtained by subtracting the Rooted witness's injected package prefix.
        let returns = unit
            .parsed()
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| {
                (matches!(node.payload(), Expression::Return { .. })
                    && unit.sources().slice(node.span()) == Ok("return owner.value"))
                .then_some(node.span())
            })
            .collect::<Vec<_>>();
        let [expected] = returns.as_slice() else {
            panic!("{case}: one original payload-return expression");
        };
        let occurrence = text.match_indices("return owner.value").collect::<Vec<_>>();
        let [(start, matched)] = occurrence.as_slice() else {
            panic!("{case}: one exact original return spelling");
        };
        assert_eq!(
            (expected.start(), expected.end()),
            (*start, start + matched.len())
        );
        assert_eq!(expected.source_id(), unit.parsed().source_id());
        let entry = unit.declaration("entry", SymbolKind::Function);
        let error = unit
            .lower_entry(entry)
            .err()
            .expect("implementation boundary is rejection, not successful payload lowering");
        assert_eq!(
            error.kind,
            crate::ssa::LoweringErrorKind::UnsupportedNode,
            "{case}"
        );
        assert_eq!(
            error.span,
            Some(*expected),
            "{case}: complete original return span"
        );
    }
}

#[test]
fn original_presence_sources_reach_source_anchored_memory_document_llvm() {
    for (case, text, _golden) in presence_cases() {
        let unit = original_memory_source(case, text);
        let expected = source_functions(unit.sources(), unit.parsed())
            .unwrap_or_else(|error| panic!("{case}: {}: {}", error.stage, error.detail));
        let entry = unit.declaration("entry", SymbolKind::Function);
        let entry_span = unit.declaration_span(entry);
        let (program, entry) = unit
            .lower_entry(entry)
            .unwrap_or_else(|error| panic!("{case}: original presence source: {error:?}"));
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        assert_eq!(function.origin.span(), entry_span);
        verify_entry_sources(unit.sources(), &program, entry, &expected)
            .unwrap_or_else(|error| panic!("{case}: {}: {}", error.stage, error.detail));
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program(&program)
            .unwrap_or_else(|error| panic!("{case}: verified LLVM: {error:?}"));
        assert!(!llvm.is_empty(), "{case}: nonempty rendered module");
    }
}
