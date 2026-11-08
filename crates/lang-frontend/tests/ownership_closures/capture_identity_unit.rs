//! Original Resource and nested capture witnesses through both Unit entry modes.
use super::{ClosureCaptureEffect, ClosureCaptureMode, Transferability};
use lang_frontend::{
    name_resolution::{
        NameResolution, SourceUnitInput, UnitSymbolId, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, UnitClosureCaptureSource as ClosureCaptureSource,
        UnitDropTarget as DropTarget, check_compilation_unit_ownership,
    },
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, complete_compilation_unit_types,
    },
};

fn checked(
    text: &str,
) -> Vec<(
    NameResolution,
    CompilationUnitOwnership,
    Vec<UnitExpressionId>,
)> {
    [false, true]
        .into_iter()
        .map(|rooted| {
            let mut sources = SourceMap::new();
            let source = sources.add_source("closures.ko", text).unwrap();
            let parsed = super::parse_file_twice(&sources, source, "capture identity Unit");
            assert!(parsed.diagnostics().is_empty());
            assert_eq!(sources.source_text(source), Ok(text));
            let (environment, types) = super::environments();
            let input = if rooted {
                SourceUnitInput::new("root", "closures.ko", source, &parsed)
            } else {
                SourceUnitInput::memory_document(source, &parsed)
            };
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            assert!(index.diagnostics().is_empty());
            let unit = index.source_units()[0].id();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            assert!(names.names().diagnostics().is_empty());
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
            assert!(complete.is_same_analysis(&typed));
            let typed = typed.validate().unwrap();
            let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap();
            assert!(owned.is_compatible_with(&typed));
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
            assert!(owned.clone().validate().is_ok());
            for capture in owned.captures() {
                assert_eq!(capture.lambda().source_unit(), unit);
                if let ClosureCaptureSource::Symbol(symbol) = capture.source() {
                    assert_eq!(symbol.source_unit(), unit);
                }
                assert!(sources.slice(capture.reference_span()).is_ok());
            }
            let lambdas = super::lambdas(&parsed)
                .into_iter()
                .map(|id| UnitExpressionId::new(unit, id))
                .collect();
            (
                names.names().source_units()[0].resolution().clone(),
                owned,
                lambdas,
            )
        })
        .collect()
}

fn symbol(names: &NameResolution, owned: &CompilationUnitOwnership, name: &str) -> UnitSymbolId {
    let original = super::symbol(names, name);
    owned
        .captures()
        .iter()
        .find_map(|capture| match capture.source() {
            ClosureCaptureSource::Symbol(symbol) if symbol.symbol() == original => Some(symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing original symbol {name}"))
}

#[test]
fn default_and_move_lambdas_publish_borrow_copy_and_move_facts() {
    let text = "class Resource {}\n\
                fun inspect(item: Resource): Unit {}\n\
                fun captures(own resource: Resource, number: Int): Unit {\n\
                    val shared: () -> Unit = {\n\
                        val first = number\n\
                        val second = inspect(resource)\n\
                        val again = number\n\
                    }\n\
                    val owned: move () -> Unit = move {\n\
                        val first = number\n\
                        val second = resource\n\
                    }\n\
                    val empty: () -> Unit = {}\n\
                }";
    for (names, owned, lambdas) in checked(text) {
        assert_eq!(lambdas.len(), 3);

        let resource = symbol(&names, &owned, "resource");
        let number = symbol(&names, &owned, "number");
        let shared = owned.captures_of(lambdas[0]).copied().collect::<Vec<_>>();
        assert_eq!(shared.len(), 2);
        assert_eq!(
            shared
                .iter()
                .map(|capture| capture.source())
                .collect::<Vec<_>>(),
            [
                ClosureCaptureSource::Symbol(number),
                ClosureCaptureSource::Symbol(resource)
            ]
        );
        assert!(
            shared
                .iter()
                .all(|capture| capture.mode() == ClosureCaptureMode::Shared
                    && capture.effect() == ClosureCaptureEffect::Borrow)
        );

        let moved = owned.captures_of(lambdas[1]).copied().collect::<Vec<_>>();
        assert_eq!(moved.len(), 2);
        assert_eq!(moved[0].source(), ClosureCaptureSource::Symbol(number));
        assert_eq!(moved[0].effect(), ClosureCaptureEffect::Copy);
        assert_eq!(moved[1].source(), ClosureCaptureSource::Symbol(resource));
        assert_eq!(moved[1].effect(), ClosureCaptureEffect::Move);
        assert!(
            moved
                .iter()
                .all(|capture| capture.mode() == ClosureCaptureMode::Owned)
        );

        assert_eq!(
            owned.closure(lambdas[0]).expect("shared").transferability(),
            Transferability::NotTransferable
        );
        assert_eq!(
            owned.closure(lambdas[1]).expect("move").transferability(),
            Transferability::Transferable
        );
        assert_eq!(
            owned.closure(lambdas[2]).expect("empty").transferability(),
            Transferability::Transferable
        );
        assert!(owned.drops().iter().any(|drop| {
        matches!(drop.target(), DropTarget::Captured { closure, source: ClosureCaptureSource::Symbol(symbol), .. } if closure == lambdas[1] && symbol == resource)
    }));
        assert!(
            !owned
                .drops()
                .iter()
                .any(|drop| drop.target() == DropTarget::Named(resource))
        );
    }
}

#[test]
fn nested_capture_uses_symbol_identity_and_shadowing_does_not_capture() {
    let text = "fun nested(number: Int): Unit {\n\
                    val outer: () -> Unit = {\n\
                        val local = 1\n\
                        val inner: () -> Unit = { val sum = number + local }\n\
                    }\n\
                    val shadow: (Int) -> Unit = { number -> val used = number }\n\
                }";
    for (names, owned, lambdas) in checked(text) {
        assert_eq!(lambdas.len(), 3);
        let number = symbol(&names, &owned, "number");
        let local = symbol(&names, &owned, "local");

        assert_eq!(
            owned
                .captures_of(lambdas[0])
                .map(|capture| capture.source())
                .collect::<Vec<_>>(),
            [ClosureCaptureSource::Symbol(number)]
        );
        assert_eq!(
            owned
                .captures_of(lambdas[1])
                .map(|capture| capture.source())
                .collect::<Vec<_>>(),
            [
                ClosureCaptureSource::Symbol(number),
                ClosureCaptureSource::Symbol(local)
            ]
        );
        assert_eq!(owned.captures_of(lambdas[2]).count(), 0);
    }
}
