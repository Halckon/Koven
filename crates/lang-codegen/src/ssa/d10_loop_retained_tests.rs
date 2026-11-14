//! Final-owned loop observation must not silently bypass ordinary admission.
use super::*;
#[test]
fn retained_root_replays_every_actual_loop_edge_and_suffix_cleanup() {
    use super::unit_lower_test_support::parsed;
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{d10_catalog_experiment, owned_compilation_unit_view},
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let base = "fun run(count: Int) { var a = \"first\"; var b = \"kept\"; var n = count; while(n > 0) { val old = replace(&a, \"next\"); n -= 1 }; println(b) }";
    for text in [
        base.to_owned(),
        base.replace("count", "limit")
            .replace("kept", "保留")
            .replace("first", "甲")
            .replace("next", "乙"),
    ] {
        let mut sources = SourceMap::new();
        let (source, ast) = parsed(&mut sources, "loop-bound.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "loop-bound.ko", source, &ast),
            SourceUnitInput::memory_document(source, &ast),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let bound =
                d10_catalog_experiment::analyze_loop(&sources, &inputs, &names, &types, &typed)
                    .unwrap();
            let bound = bound.unwrap();
            let unit = owned_compilation_unit_view(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                bound.ownership(),
            )
            .unwrap();
            assert!(bound.matches(&unit));
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let (program, function) =
                unit_lower::lower_owned_unit_with_entry(&unit, entry).unwrap();
            super::d10_loop_handoff_tests::replay(&bound, &unit, &program, function)
                .expect("actual two-root loop replay");
        }
    }
}

use super::model::*;
use lang_frontend::ownership_checking::{
    OwnedCompilationUnitView, UnitDropPoint, UnitDropTarget,
    d10_catalog_experiment::RetainedLoopRoot,
};
// Reads the extra original instructions and returns the actual entry owner identity.
pub(super) fn replay_suffix(
    kept: &RetainedLoopRoot,
    unit: &OwnedCompilationUnitView<'_, '_>,
    module: &Module,
    f: &Function,
    entry: &Block,
    finished: &Block,
    owner: ValueId,
) -> Option<ValueId> {
    let [input] = unit.inputs() else {
        return None;
    };
    let i = &f.instructions;
    let construction = &i[1];
    let Operation::StringLiteral { string, bytes } = &construction.operation else {
        return None;
    };
    let [EntityId::Value(initial)] = construction.results.as_slice() else {
        return None;
    };
    let ast = input.parsed().ast();
    let site = kept.source().1.expression();
    if construction.block != entry.id
        || construction.origin != Origin::Source(ast.expressions().get(site).ok()?.span())
        || module.types.get(string.index())? != &SsaTypeKind::StringOwner
        || *bytes
            != super::lowering_support::string_literal::decode_plain(
                input.parsed(),
                unit.sources().source_text(input.source_id()).ok()?,
                site,
            )
            .ok()??
    {
        return None;
    }
    if kept.cleanup().point() != UnitDropPoint::CallReturn(kept.call())
        || kept.cleanup().target() != UnitDropTarget::Named(kept.source().0)
    {
        return None;
    }
    let [EntityId::Place(place)] = i[13].results.as_slice() else {
        return None;
    };
    let [EntityId::Loan(loan)] = i[14].results.as_slice() else {
        return None;
    };
    let begin = Origin::Source(kept.loan().begin_span());
    let call = Origin::Source(ast.expressions().get(kept.call().expression()).ok()?.span());
    let expected = [
        (13, Operation::RootPlace { owner }, begin.clone()),
        (
            14,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared,
            },
            begin,
        ),
        (15, Operation::PrintString { value: *loan }, call.clone()),
        (16, Operation::BorrowEnd { loan: *loan }, call),
        (
            17,
            Operation::Drop { owner },
            Origin::Source(kept.cleanup().value_origin()),
        ),
    ];
    if expected.iter().any(|(n, op, origin)| {
        i[*n].block != finished.id || i[*n].operation != *op || i[*n].origin != *origin
    }) {
        return None;
    }
    Some(*initial)
}
