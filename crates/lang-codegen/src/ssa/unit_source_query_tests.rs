use lang_frontend::{name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments};
use super::{unit_lower_test_support::{analyze, parsed}, unit_source_query::parsed_by_source_unit, LoweringErrorKind};

#[test]
fn canonical_lookup_preserves_borrowed_identity_first_match_and_missing_error() {
    let mut sources = SourceMap::default();
    let (a, first) = parsed(&mut sources, "a.ko", "fun a(): Unit {}");
    let (z, last) = parsed(&mut sources, "z.ko", "fun z(): Unit {}");
    let inputs = [SourceUnitInput::new("root", "z.ko", z, &last), SourceUnitInput::new("root", "a.ko", a, &first)];
    let (names_env, types_env) = standard_environments();
    let (names, _, _) = analyze(&sources, &inputs, &names_env, &types_env);
    let result = parsed_by_source_unit(&inputs, &names).unwrap();
    assert!(std::ptr::eq(result[0], &first));
    assert!(std::ptr::eq(result[1], &last));
    let clone = first.clone();
    let duplicate = [inputs[1], SourceUnitInput::new("root", "a.ko", a, &clone), inputs[0]];
    assert!(std::ptr::eq(parsed_by_source_unit(&duplicate, &names).unwrap()[0], &first));
    let missing = parsed_by_source_unit(&inputs[..1], &names).unwrap_err();
    assert_eq!(missing.kind, LoweringErrorKind::MismatchedSource);
    assert_eq!(missing.span, None);
}
