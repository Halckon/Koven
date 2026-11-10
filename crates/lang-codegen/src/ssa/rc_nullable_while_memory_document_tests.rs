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
        // Locate the actual Return node and its payload in the original bytes, not
        // offsets obtained by subtracting the Rooted witness's injected prefix.
        let returns = unit
            .parsed()
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| {
                let Expression::Return {
                    value: Some(value), ..
                } = node.payload()
                else {
                    return None;
                };
                if unit.sources().slice(node.span()) != Ok("return owner.value") {
                    return None;
                }
                let payload = unit.parsed().ast().expressions().get(*value).ok()?;
                Some((node.span(), payload.span()))
            })
            .collect::<Vec<_>>();
        let [(return_span, payload_span)] = returns.as_slice() else {
            panic!("{case}: one original payload-return expression");
        };
        let occurrence = text.match_indices("return owner.value").collect::<Vec<_>>();
        let [(start, matched)] = occurrence.as_slice() else {
            panic!("{case}: one exact original return spelling");
        };
        assert_eq!(
            (return_span.start(), return_span.end()),
            (*start, start + matched.len())
        );
        assert_eq!(return_span.source_id(), unit.parsed().source_id());
        assert_eq!(unit.sources().slice(*payload_span), Ok("owner.value"));
        let entry = unit.declaration("entry", SymbolKind::Function);
        let lower_result = crate::ssa::with_per_family_lowering(|| unit.lower_entry(entry));
        let error = lower_result
            .err()
            .expect("implementation boundary is rejection, not successful payload lowering");
        assert_eq!(
            error.kind,
            crate::ssa::LoweringErrorKind::UnsupportedNode,
            "{case}"
        );
        assert_eq!(
            error.span,
            Some(*payload_span),
            "{case}: payload operation span inside the original return"
        );
    }
}

#[test]
fn original_presence_sources_reach_source_anchored_memory_document_llvm() {
    for (case, text, _golden) in presence_cases() {
        let unit = original_memory_source(case, text);
        // Stage A for the stronger identity oracle: keep the independent Single
        // identity witness until this exact MemoryDocument path has CI evidence.
        let facts = identity_facts::unit(
            unit.sources(),
            unit.parsed(),
            unit.names(),
            unit.basic_types(),
            unit.ownership(),
            case,
        )
        .unwrap_or_else(|error| panic!("{case}: original frontend owner identity: {error}"));
        let expected = source_functions(unit.sources(), unit.parsed())
            .unwrap_or_else(|error| panic!("{case}: {}: {}", error.stage, error.detail));
        let entry = unit.declaration("entry", SymbolKind::Function);
        let entry_span = unit.declaration_span(entry);
        let lower_result = crate::ssa::with_per_family_lowering(|| unit.lower_entry(entry));
        let (program, entry) = lower_result
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
        identity::inspect_program(&program, entry, case, &facts)
            .unwrap_or_else(|error| panic!("{case}: original SSA cleanup identity: {error}"));
        let llvm = crate::llvm::render_verified_program(&program)
            .unwrap_or_else(|error| panic!("{case}: verified LLVM: {error:?}"));
        assert!(!llvm.is_empty(), "{case}: nonempty rendered module");
    }
}
