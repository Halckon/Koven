//! Real source shapes only: no injected facts, mutated SSA, or cross-analysis mixing.
use super::*;
use crate::ssa::{unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn identity_leaf_certifies_only_one_or_two_plain_string_parameters() {
    for (source, expected) in [
        (
            "fun identity(source:String,aux:String):borrow String from source=source",
            true,
        ),
        (
            "fun identity(aux:String,source:String):borrow String from source=source",
            true,
        ),
        (
            "fun identity(source:String,a:String,b:String):borrow String from source=source",
            false,
        ),
        (
            "fun identity(source:String):borrow String from source=source",
            true,
        ),
        (
            "fun identity(source:String):borrow String from source=(source)",
            false,
        ),
        (
            "fun identity(source:String):borrow String from source {return source}",
            false,
        ),
        (
            "fun identity(source:String,unused:Boolean):borrow String from source=source",
            false,
        ),
    ] {
        let mut sources = SourceMap::new();
        let (id, parsed) = unit_lower_test_support::parsed(&mut sources, "identity.ko", source);
        let inputs = [SourceUnitInput::new("root", "identity.ko", id, &parsed)];
        let (environment, types) = standard_environments();
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "identity")
            .unwrap();
        for _ in 0..2 {
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    OBSERVED.with(|s| {
                        s.borrow_mut().take();
                    });
                }
            }
            let _reset = Reset;
            OBSERVED.with(|s| {
                assert!(s.borrow().is_none());
                *s.borrow_mut() = Some(Vec::new());
            });
            let lowered = unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                declaration.id(),
            );
            let receipts = OBSERVED.with(|s| s.borrow_mut().take().unwrap());
            assert_eq!(receipts.len(), usize::from(expected), "{source}");
            let (program, function) = match lowered {
                Ok(result) => result,
                Err(error) if !expected => {
                    assert_eq!(
                        error.kind,
                        crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode,
                        "{source}"
                    );
                    continue;
                }
                Err(error) => panic!("positive identity leaf failed: {source}: {error:?}"),
            };
            crate::ssa::verify::verify_program(&program).unwrap();
            if let [receipt] = receipts.as_slice() {
                assert_eq!(
                    receipt.callable(),
                    UnitItemId::new(declaration.source_unit(), declaration.root())
                );
                assert_eq!(
                    receipt.key(),
                    &UnitFunctionInstanceKey::for_entry(declaration.id())
                );
                assert_eq!(receipt.function(), function);
                assert_eq!(
                    receipt.expression().source_unit(),
                    declaration.source_unit()
                );
                assert_eq!(receipt.loan().function(), function);
                assert!(receipt.matches_final(&program));
                let _same_plan_token = receipt.source_token();
            }
        }
    }
}
