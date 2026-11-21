//! Same original sources, now through strict Unit's intentionally different behavior.
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{ClosureCaptureOperand, EntityId, Operation},
};
use lang_frontend::{
    diagnostic::{DiagnosticDetail, Severity},
    name_resolution::SymbolKind,
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode, UnitDropPoint, UnitDropTarget},
};

#[test]
fn conditional_owned_capture_lowers_to_a_real_thunk_and_owned_cleanup() {
    let text = "fun run(flag: Boolean) { val text = \"x\"\nif (flag) { val action: move () -> Unit = move { println(text) }\nval used = action() } }";
    let unit = UnitSource::analyze("lowering.ko", text).expect("original strict Unit source");
    unit.assert_frontend_clean();
    let source = unit.parsed().source_id();
    assert_eq!(unit.sources().source_text(source), Ok(text));
    let [capture] = unit.ownership().captures() else {
        panic!("one actual closure capture");
    };
    assert_eq!(capture.mode(), ClosureCaptureMode::Owned);
    assert_eq!(capture.effect(), ClosureCaptureEffect::Move);
    assert_eq!(capture.reference_span().source_id(), source);
    assert_eq!(
        (
            capture.reference_span().start(),
            capture.reference_span().end()
        ),
        (97, 101)
    );
    assert_eq!(unit.sources().slice(capture.reference_span()), Ok("text"));
    let drops = unit.ownership().drops();
    let [captured, callback_cleanup, untaken] = drops else {
        panic!("actual owned capture and branch-specific cleanup: {drops:?}");
    };
    assert_eq!(
        captured.target(),
        UnitDropTarget::Captured {
            closure: capture.lambda(),
            source: capture.source()
        }
    );
    assert_eq!(captured.value_origin(), capture.reference_span());
    assert_eq!(callback_cleanup.point(), captured.point());
    assert!(matches!(captured.point(), UnitDropPoint::CallReturn(_)));
    assert!(matches!(
        callback_cleanup.target(),
        UnitDropTarget::Named(_)
    ));
    let lang_frontend::ownership_checking::UnitClosureCaptureSource::Symbol(owner) =
        capture.source()
    else {
        panic!("actual source String binding");
    };
    assert_eq!(untaken.target(), UnitDropTarget::Named(owner));
    assert!(matches!(
        untaken.point(),
        UnitDropPoint::BranchExit { branch: 1, .. }
    ));
    assert_eq!(
        (untaken.value_origin().start(), untaken.value_origin().end()),
        (29, 33)
    );

    let declaration = unit.declaration("run", SymbolKind::Function);
    let ((program, entry), routes) = {
        let (output, routes) =
            crate::ssa::refine::observe_routing(|| unit.lower_entry(declaration));
        (
            output.expect("Unit supports the original conditional capture"),
            routes,
        )
    };
    crate::ssa::verify::verify_program(&program).unwrap();
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    assert!(routes.contains(&function.name));
    let formations: Vec<_> = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::ClosureConstruct { .. }))
        .collect();
    let [formation] = formations.as_slice() else {
        panic!("one actual formation")
    };
    let Operation::ClosureConstruct {
        thunk, captures, ..
    } = &formation.operation
    else {
        unreachable!()
    };
    let [ClosureCaptureOperand::Owned(captured)] = captures.as_slice() else {
        panic!("moves one real source")
    };
    let [EntityId::Value(callback)] = formation.results.as_slice() else {
        panic!("owned closure result")
    };
    let thunk = module
        .function(*thunk)
        .expect("real emitted thunk identity");
    assert!(routes.contains(&thunk.name));
    assert!(
        thunk
            .instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::PrintString { .. }))
    );
    let release: Vec<_> = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { owner } if owner==*callback))
        .collect();
    let [release] = release.as_slice() else {
        panic!("formed owner released once")
    };
    assert_eq!(release.block, formation.block);
    assert!(release.id.index() > formation.id.index());
    assert!(!function.instructions.iter().any(
        |i| matches!(i.operation, Operation::Drop { owner } if owner==*captured)
            && i.block == formation.block
    ));
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn loop_carried_capture_reports_both_source_lifetimes_before_lowering() {
    let text = "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val xs = listOf(1)
                block { f = ({ read(xs) }) }
                block { g = ({ read(xs) }) }
                break
            }
            val first = f()
            val second = g()
        }";
    let Err(rejected) = UnitSource::analyze("lowering.ko", text) else {
        panic!("the original two escaping loan captures are rejected in Unit ownership");
    };
    use crate::native_tests::boxed_enum_tests::unit_source::SourceRejectionKind;
    assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(
        rejected.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 2)]
    );
    assert_eq!(rejected.report.diagnostics.len(), 2);
    for ((stage, diagnostic), offset) in rejected.report.diagnostics.iter().zip([231, 276]) {
        assert_eq!(*stage, "ownership");
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), "L0135");
        assert_eq!(
            diagnostic.message(),
            "captured binding does not live as long as the closure that borrows it"
        );
        let span = diagnostic.primary_span();
        assert_eq!((span.start(), span.end()), (offset, offset + 2));
        assert_eq!(
            rejected.sources.source_name(span.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(rejected.sources.source_text(span.source_id()), Ok(text));
        assert_eq!(rejected.sources.slice(span), Ok("xs"));
        let [DiagnosticDetail::Label(binding)] = diagnostic.details() else {
            panic!("one exact source binding label");
        };
        assert_eq!(
            binding.message(),
            "declared in a scope that ends while the closure is still held outside it"
        );
        let label = binding.span();
        assert_eq!(label.source_id(), span.source_id());
        assert_eq!((label.start(), label.end()), (180, 182));
        assert_eq!(rejected.sources.slice(label), Ok("xs"));
    }
}

#[test]
fn guarded_owner_choice_remains_a_source_qualified_unit_lowering_boundary() {
    use crate::ssa::LoweringErrorKind;
    use lang_frontend::ownership_checking::UnitClosureCaptureSource;

    let text = "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val f: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })\nval used = f() }";
    let unit = UnitSource::analyze("lowering.ko", text).expect("original strict Unit source");
    unit.assert_frontend_clean();
    let source = unit.parsed().source_id();
    assert_eq!(unit.sources().source_text(source), Ok(text));
    let captures = unit.ownership().captures();
    let [left, right] = captures else {
        panic!("two actual original branch captures");
    };
    assert_ne!(left.lambda(), right.lambda());
    assert_ne!(left.source(), right.source());
    for (fact, name) in [(left, "xs"), (right, "ys")] {
        assert_eq!(fact.mode(), ClosureCaptureMode::Shared);
        assert_eq!(fact.effect(), ClosureCaptureEffect::Borrow);
        let UnitClosureCaptureSource::Symbol(symbol) = fact.source() else {
            panic!("the real enclosing parameter, not an inferred temporary");
        };
        let context = &unit.names().names().source_units()[symbol.source_unit().index()];
        let declaration = context
            .resolution()
            .symbols()
            .get(symbol.symbol().index())
            .unwrap();
        assert_eq!(declaration.kind(), SymbolKind::ValueParameter);
        assert_eq!(declaration.name(), name);
        assert_eq!(fact.reference_span().source_id(), source);
        assert_eq!(unit.sources().slice(fact.reference_span()), Ok(name));
    }

    let entry = unit.declaration("run", SymbolKind::Function);
    let Err(error) = unit.lower_entry(entry) else {
        panic!("the exact guarded selector cannot publish a partial ordinary Unit program");
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let span = error.span.expect("source-qualified whole selector");
    assert_eq!(span.source_id(), source);
    assert_eq!((span.start(), span.end()), (110, 154));
    assert_eq!(
        unit.sources().slice(span),
        Ok("if (flag) ({ read(xs) }) else ({ read(ys) })")
    );
    // This checks the ordinary compiler boundary, not the Single-only SaveOwnerSnapshot encoding.
}
