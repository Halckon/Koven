//! Positive strict Unit ABI/CFG for the unchanged legacy inline-nullable fixture.
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::ssa::{model::*, verify::verify_program};
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

pub(super) fn assert_unit(text: &str) {
    let unit = UnitSource::analyze("lowering.ko", text).unwrap();
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    assert_eq!(unit.parsed().roots().len(), 2);
    let [document] = unit.names().names().index().source_units() else {
        panic!("one complete document")
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(
        unit.sources().source_text(document.source_id()).unwrap(),
        text
    );
    let inspect = unit.declaration("inspect", SymbolKind::Function);
    let span = unit.declaration_span(inspect);
    assert_eq!(
        unit.sources().slice(span).unwrap(),
        &text[text.find("fun inspect").unwrap()..]
    );
    let (program, id) = unit.lower_entry(inspect).unwrap();
    verify_program(&program).unwrap();
    let module = program.module(id.module()).unwrap();
    let function = module.function(id).unwrap();
    assert_eq!(function.origin, Origin::Source(span));
    assert_eq!(function.blocks.len(), 4);
    assert!(function.receiver.is_none() && function.return_types.is_empty());
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let [EntityId::Value(owner)] = entry.parameters.as_slice() else {
        panic!("owned nullable argument")
    };
    let EntityType::Value(nullable) = function.entity(EntityId::Value(*owner)).unwrap().ty else {
        panic!("owned value ABI")
    };
    let Some(SsaTypeKind::InlineNullable {
        inner,
        niche: false,
    }) = module.type_kind(nullable)
    else {
        panic!("tagged inline nullable Token ABI")
    };
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(*inner) else {
        panic!("inline Token payload")
    };
    let [field] = fields.as_slice() else {
        panic!("original single item field")
    };
    assert!(matches!(
        module.type_kind(*field),
        Some(SsaTypeKind::Integer {
            bits: 32,
            signed: true
        })
    ));
    let terminator = entry.terminator.as_ref().unwrap();
    assert_eq!(
        unit.sources().slice(terminator.origin.span()).unwrap(),
        "token != null"
    );
    let TerminatorKind::NullableBranch {
        owner: tested,
        when_null,
        when_non_null,
        view,
    } = &terminator.kind
    else {
        panic!("real nullable branch")
    };
    assert_eq!(tested, owner);
    assert!(when_null.arguments.is_empty());
    assert_eq!(when_non_null.arguments, vec![EntityId::Value(*owner)]);
    let null = function.block(when_null.target).unwrap();
    let present = function.block(when_non_null.target).unwrap();
    assert!(null.parameters.is_empty() && null.instructions.is_empty());
    let [EntityId::Value(present_owner), EntityId::Loan(proof)] = present.parameters.as_slice()
    else {
        panic!("actual non-null owner/proof block parameters")
    };
    assert_eq!(proof, view);
    assert_eq!(
        function.entity(EntityId::Value(*present_owner)).unwrap().ty,
        EntityType::Value(nullable)
    );
    assert_eq!(
        function.entity(EntityId::Loan(*proof)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *inner
        }
    );
    let [take_id] = present.instructions.as_slice() else {
        panic!("one original nullable take")
    };
    let take = function.instruction(*take_id).unwrap();
    assert_eq!(take.block, present.id);
    assert!(
        matches!(take.operation, Operation::NullableTake { owner, proof: actual } if owner == *present_owner && actual == *proof)
    );
    assert_eq!(take.origin, terminator.origin);
    let [EntityId::Value(payload)] = take.results.as_slice() else {
        panic!("Token payload")
    };
    assert_eq!(
        function.entity(EntityId::Value(*payload)).unwrap().ty,
        EntityType::Value(*inner)
    );
    let next = |block: &Block| {
        let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
            panic!("real path to join")
        };
        assert!(edge.arguments.is_empty());
        edge.target
    };
    let join_id = next(null);
    assert_eq!(next(present), join_id);
    let join = function.block(join_id).unwrap();
    assert!(join.parameters.is_empty() && join.instructions.is_empty());
    assert!(
        matches!(&join.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
    );
    assert_eq!(
        function.instructions.len(),
        1,
        "only the real nullable take, no synthesized copy/drop path"
    );
    // inspect is an owned-parameter library root, not a zero-argument native entry.
    crate::llvm::render_verified_program(&program).unwrap();
}
