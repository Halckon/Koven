//! Frozen ordinary frontend boundaries and successful original roots through strict Unit.

use lang_frontend::{
    diagnostic::{DiagnosticDetail, Severity},
    name_resolution::SymbolKind,
};

use crate::{
    llvm::render_verified_program,
    native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource},
    ssa::{
        model::{EntityId, EntityType, Operation, Origin, SsaTypeKind, TerminatorKind},
        verify::verify_program,
    },
};

struct ExpectedDiagnostic {
    stage: &'static str,
    code: &'static str,
    message: &'static str,
    range: (usize, usize),
    label: Option<(usize, usize)>,
}

// Frozen from the original Legacy consumers at a4b72d35, before their migration.
// Preserve severity, code, full message, source identity, primary/label ranges and detail order.
fn assert_rejected(source: &str, stages: &[(&str, usize)], expected: &[ExpectedDiagnostic]) {
    let rejected = UnitSource::analyze("lowering.ko", source)
        .err()
        .expect("the original frontend error cannot acquire any executable Unit capability");
    assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(rejected.report.stages, stages);
    assert_eq!(rejected.report.diagnostics.len(), expected.len());
    for ((stage, diagnostic), expected) in rejected.report.diagnostics.iter().zip(expected) {
        assert_eq!(*stage, expected.stage);
        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.code().to_string(), expected.code);
        assert_eq!(diagnostic.message(), expected.message);
        let primary = diagnostic.primary_span();
        assert_eq!((primary.start(), primary.end()), expected.range);
        assert_eq!(
            rejected.sources.source_name(primary.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(
            rejected.sources.source_text(primary.source_id()),
            Ok(source)
        );
        assert_eq!(
            rejected.sources.slice(primary),
            Ok(&source[expected.range.0..expected.range.1])
        );
        match (diagnostic.details(), expected.label) {
            ([DiagnosticDetail::Label(label)], Some((start, end))) => {
                assert_eq!(label.message(), "expected type introduced here");
                assert_eq!(label.span().source_id(), primary.source_id());
                assert_eq!((label.span().start(), label.span().end()), (start, end));
                assert_eq!(
                    rejected.sources.slice(label.span()),
                    Ok(&source[start..end])
                );
            }
            ([], None) => {}
            _ => panic!(
                "complete ordered diagnostic details changed: {:?}",
                diagnostic.details()
            ),
        }
    }
}

pub(super) fn assert_integer_literal_rejection(source: &str) {
    assert_rejected(
        source,
        &[("parse", 0), ("names", 0), ("types", 2)],
        &[
            ExpectedDiagnostic {
                stage: "types",
                code: "L0090",
                message: "numeric literal is outside the representable range",
                range: (26, 29),
                label: Some((18, 22)),
            },
            ExpectedDiagnostic {
                stage: "types",
                code: "L0090",
                message: "numeric literal is outside the representable range",
                range: (58, 80),
                label: Some((50, 55)),
            },
        ],
    );
}

pub(super) fn assert_missing_name_rejection(source: &str) {
    assert_rejected(
        source,
        &[("parse", 0), ("names", 1)],
        &[ExpectedDiagnostic {
            stage: "names",
            code: "L0080",
            message: "unresolved name",
            range: (30, 37),
            label: None,
        }],
    );
}

pub(super) fn assert_jump_rejection(source: &str) {
    assert_rejected(
        source,
        &[("parse", 0), ("names", 0), ("types", 1)],
        &[ExpectedDiagnostic {
            stage: "types",
            code: "L0142",
            message: "break is not inside an enclosing loop in this callable",
            range: (22, 27),
            label: None,
        }],
    );
}

fn analyze_original(source: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", source).expect("strict original Unit source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    assert_eq!(
        unit.sources().source_text(unit.parsed().source_id()),
        Ok(source)
    );
    unit
}

pub(super) fn assert_original_for_loop(source: &str) {
    let unit = analyze_original(source);
    assert_eq!(unit.ownership().iterations().len(), 1);
    let declaration = unit.declaration("iterate", SymbolKind::Function);
    let (program, entry) = unit
        .lower_entry(declaration)
        .expect("original for loop lowers");
    verify_program(&program).expect("original for loop SSA verifies");
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    assert_eq!(
        function.origin,
        Origin::Source(unit.declaration_span(declaration))
    );
    assert_eq!(module.functions.len(), 1);
    assert!(function.blocks[0].parameters.is_empty());
    assert!(function.return_types.is_empty());
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::ContainerConstruct { .. }))
            .count(),
        1
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        1
    );
    render_verified_program(&program).expect("original for loop LLVM verifies");
}

pub(super) fn assert_original_string_instance(source: &str) {
    let unit = analyze_original(source);
    let declaration = unit.declaration("text", SymbolKind::Function);
    let (program, entry) = unit
        .lower_entry(declaration)
        .expect("original String generic instance lowers");
    verify_program(&program).expect("original String generic SSA verifies");
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    assert_eq!(
        function.origin,
        Origin::Source(unit.declaration_span(declaration))
    );
    assert_eq!(module.functions.len(), 2);
    let input = function.blocks[0].parameters.as_slice();
    let [EntityId::Value(_)] = input else {
        panic!("original owned String input")
    };
    let [returned] = function.return_types.as_slice() else {
        panic!("one original String return")
    };
    assert_eq!(
        function.entity(input[0]).unwrap().ty,
        EntityType::Value(*returned)
    );
    assert_eq!(module.type_kind(*returned), Some(&SsaTypeKind::StringOwner));
    let calls = function
        .instructions
        .iter()
        .filter_map(|i| match &i.operation {
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => Some((i, callee, receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(call, callee, receiver, arguments)] = calls.as_slice() else {
        panic!("one concrete generic call")
    };
    assert!(receiver.is_none());
    assert_eq!(arguments.as_slice(), input);
    let callee = module.function(**callee).unwrap();
    assert_eq!(
        callee.origin,
        Origin::Source(unit.declaration_span(unit.declaration("identity", SymbolKind::Function)))
    );
    assert_eq!(callee.return_types, function.return_types);
    for current in [function, callee] {
        assert!(
            !current
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::Drop { .. }))
        );
        let returns = current
            .blocks
            .iter()
            .filter_map(|block| match &block.terminator.as_ref()?.kind {
                TerminatorKind::Return { values } => Some(
                    values
                        .iter()
                        .copied()
                        .map(EntityId::Value)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = if current.id() == function.id() {
            &call.results
        } else {
            &callee.blocks[0].parameters
        };
        assert_eq!(
            returns.as_slice(),
            std::slice::from_ref(expected),
            "each original owner is returned, never dropped"
        );
    }
    // Unit has source-qualified generated names. Preserve the original generic-instance
    // observation through its exact declaration and concrete owned String ABI instead.
    let [parameter] = callee.blocks[0].parameters.as_slice() else {
        panic!("the original generic identity has one concrete parameter");
    };
    assert!(matches!(parameter, EntityId::Value(_)));
    assert_eq!(
        callee.entity(*parameter).unwrap().ty,
        EntityType::Value(*returned)
    );
    render_verified_program(&program).expect("original String generic LLVM verifies");
}
