//! Actual language/cleanup baseline for the proposed one-replacement Break observation.
//! No new owner capability or cross-catalog state is constructed by these tests.
use super::*;
use crate::ownership_checking::{UnitDropPoint, UnitDropTarget};
use crate::parser::{Item, NameMarker, Statement};
use crate::type_checking::{OwnershipPrimitiveKind, UnitStatementId};

fn declaration(fixture: &Fixture, file: usize, name: &str) -> (UnitDropPoint, UnitExpressionId) {
    let ast = fixture.parsed[file].ast();
    let statements: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::LocalVariable { declaration } = node.payload() else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(marker),
                initializer,
                ..
            } = ast.items().get(*declaration).unwrap().payload()
            else {
                return None;
            };
            (fixture.sources.slice(*marker).unwrap() == name).then_some((
                UnitDropPoint::AfterStatement(UnitStatementId::new(fixture.source(file), id)),
                UnitExpressionId::new(fixture.source(file), *initializer),
            ))
        })
        .collect();
    assert_eq!(statements.len(), 1, "unique real declaration: {name}");
    statements[0]
}

#[test]
fn outer_replace_break_preserves_actual_primitive_and_cleanup_baseline() {
    for result in ["", ": Unit"] {
        let first = format!(
            r#"fun first(flag: Boolean){result} {{ var held = "outer"; while(flag) {{ val previous = replace(&held, "next"); break }}; val retired = held }}"#
        );
        let second = first.replacen("fun first(", "fun second(", 1);
        let fixture = Fixture::new(&[("a.ko", &first), ("b.ko", &second)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.clone().validate().is_ok());
        assert_eq!(owned.ownership_primitives().len(), 2);
        assert_eq!(owned.owner_observations().len(), 2);
        assert_eq!(owned.drops().len(), 4);
        for file in 0..2 {
            let expression = |text: &str| {
                UnitExpressionId::new(fixture.source(file), fixture.expression(file, text))
            };
            let call = expression(r#"replace(&held, "next")"#);
            let incoming = expression(r#""next""#);
            let held = fixture.local(file, "held");
            let previous = fixture.local(file, "previous");
            let retired = fixture.local(file, "retired");
            let plan = owned
                .ownership_primitive(call)
                .expect("actual checked Replace plan");
            assert_eq!(plan.descriptor().kind(), OwnershipPrimitiveKind::Replace);
            assert_eq!(plan.descriptor().expression(), call);
            assert_eq!(plan.places().len(), 1);
            assert!(plan.places()[0].is_root());
            assert_eq!(plan.places()[0].root(), held);
            assert_eq!(plan.descriptor().operands().len(), 2);
            assert_eq!(plan.descriptor().operands()[1], incoming);
            let (previous_point, previous_initializer) = declaration(&fixture, file, "previous");
            assert_eq!(previous_initializer, call);
            let (retired_point, retired_initializer) = declaration(&fixture, file, "retired");
            let retired_node = fixture.parsed[file]
                .ast()
                .expressions()
                .get(retired_initializer.expression())
                .unwrap();
            assert!(matches!(
                retired_node.payload(),
                crate::parser::Expression::Name
            ));
            assert_eq!(fixture.sources.slice(retired_node.span()).unwrap(), "held");
            let drops: Vec<_> = owned.drops().iter().filter(|drop| matches!(drop.target(), UnitDropTarget::Named(symbol) if symbol.source_unit() == fixture.source(file))).map(|drop| (drop.point(), drop.target())).collect();
            assert_eq!(
                drops,
                vec![
                    (previous_point, UnitDropTarget::Named(previous)),
                    (retired_point, UnitDropTarget::Named(retired))
                ]
            );
            assert!(
                owned
                    .drops()
                    .iter()
                    .all(|drop| drop.target() != UnitDropTarget::Named(held))
            );
            let loops: Vec<_> = fixture.parsed[file]
                .ast()
                .statements()
                .iter()
                .filter_map(|(id, node)| {
                    matches!(node.payload(), Statement::While { .. })
                        .then_some(UnitStatementId::new(fixture.source(file), id))
                })
                .collect();
            assert_eq!(loops.len(), 1);
            let observation = &owned.owner_observations()[file];
            assert_eq!(
                observation.status(),
                crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {
                    point: UnitDropPoint::AfterStatement(loops[0]),
                    reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax,
                }
            );
            assert!(observation.definitions().is_empty() && observation.events().is_empty());
            assert!(observation.loop_observation_v1().is_none());
            assert!(observation.invariant_loop_observation_v1().is_none());
            assert!(observation.invariant_break_observation_v1().is_none());
            assert!(observation.value_observation_v1().is_none());
        }
        assert_ne!(fixture.source(0), fixture.source(1));
        // compare() inspects source zero only: give each original file its own real fixture.
        for (path, source) in [("a.ko", first.as_str()), ("b.ko", second.as_str())] {
            let single = Fixture::new(&[(path, source)]);
            let owned = single.shared();
            assert!(owned.clone().validate().is_ok());
            compare(&single, &single.parts(&owned));
        }
    }
}
