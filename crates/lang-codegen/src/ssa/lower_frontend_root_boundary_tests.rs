//! Original declaration-only roots through strict Unit, with no runtime layouts or actions.

use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use crate::{
    llvm::render_verified_program,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{
        model::{Origin, TerminatorKind},
        render::render_program,
        verify::verify_program,
    },
    test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic, assert_text_eq},
};

fn original_unit(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original Unit source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original memory document");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

pub(super) fn assert_unused_declarations(text: &str) {
    let unit = original_unit(text);
    assert_eq!(unit.parsed().roots().len(), 5);
    let declaration = unit.declaration("entry", SymbolKind::Function);
    let lower = || {
        unit.lower_entry(declaration)
            .expect("declarative roots do not initialize")
    };
    let (program, entry) = lower();
    let (repeated, repeated_entry) = lower();
    verify_program(&program).expect("original declaration-root SSA verifies");
    verify_program(&repeated).expect("repeated declaration-root SSA verifies");
    assert_eq!(entry.index(), repeated_entry.index());
    assert_eq!(entry.module().index(), repeated_entry.module().index());
    assert_eq!(program.modules.len(), 1);
    let module = program.module(entry.module()).unwrap();
    assert_eq!(module.functions.len(), 1);
    assert!(
        module.types.is_empty(),
        "unused declarations have no runtime layout"
    );
    let function = module.function(entry).unwrap();
    assert_eq!(
        function.origin,
        Origin::Source(unit.declaration_span(declaration))
    );
    assert_eq!(
        unit.sources().slice(unit.declaration_span(declaration)),
        Ok("fun entry(): Unit {}")
    );
    assert!(function.receiver.is_none());
    assert!(function.return_types.is_empty());
    assert!(
        function.instructions.is_empty(),
        "no initialization or cleanup action"
    );
    assert_eq!(function.blocks.len(), 1);
    assert!(function.blocks[0].parameters.is_empty());
    assert_eq!(
        function.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values: vec![] }
    );
    let ssa = render_program(&program);
    assert_text_eq!(ssa, render_program(&repeated));
    assert_eq_diagnostic!(ssa.matches("func \"").count(), 1, "{ssa}");
    // Unit names are qualified. Bind this one function to the original declaration instead.
    assert_diagnostic!(
        ssa.contains(&format!("func \"{}\"", function.name)),
        "{ssa}"
    );
    let llvm = render_verified_program(&program).expect("declarative roots reach LLVM");
    assert_text_eq!(
        llvm,
        render_verified_program(&repeated).expect("repeated roots reach LLVM")
    );
    assert_eq_diagnostic!(llvm.matches("define internal").count(), 1, "{llvm}");
    for unused in ["Pair", "Holder", "Marker", "Outcome"] {
        assert_diagnostic!(!ssa.contains(unused), "{ssa}");
        assert_diagnostic!(!llvm.contains(unused), "{llvm}");
    }
}
