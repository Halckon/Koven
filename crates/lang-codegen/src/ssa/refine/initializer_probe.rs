//! Test-only observation of the existing builder; never changes admission or emits SSA.
use std::cell::Cell;

use lang_frontend::source::Span;

use super::{
    EntityId, RefineInputs, RefineTypes,
    body::{BlockRef, ROp, RTerm, Slot},
    build,
};

thread_local! {
    static PROBE: Cell<Option<(Span, bool)>> = const { Cell::new(None) };
}

pub(in crate::ssa) fn run(span: Span, nested: bool, lower: impl FnOnce()) {
    PROBE.with(|probe| assert!(probe.replace(Some((span, nested))).is_none()));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PROBE.with(|probe| probe.set(None));
        }
    }
    let _reset = Reset;
    lower();
    PROBE.with(|probe| assert!(probe.get().is_none(), "initializer probe must run"));
}

pub(in crate::ssa) fn requested(span: Span) -> bool {
    PROBE.with(|probe| probe.get().is_some_and(|(expected, _)| expected == span))
}

#[derive(Clone, Default)]
struct Path {
    calls: Vec<(&'static str, Slot)>,
    prefix: Option<Slot>,
    binding: Option<Slot>,
}

pub(in crate::ssa) fn inspect(
    inputs: &RefineInputs<'_>,
    entities: &[EntityId],
    types: &RefineTypes<'_>,
) {
    let (_, nested) = PROBE.with(|probe| probe.take().expect("requested initializer probe"));
    let body = build::build(inputs, entities, types).expect("initializer candidate builds pre-SSA");
    let mut pending = vec![(BlockRef(0), Path::default())];
    let mut returns = [0, 0];
    let mut budget = 32;
    while let Some((id, mut path)) = pending.pop() {
        assert!(budget > 0, "fixture must remain acyclic and bounded");
        budget -= 1;
        let block = &body.blocks[id.0];
        for stmt in &block.statements {
            let text = inputs.sources.slice(stmt.span).unwrap();
            if matches!(stmt.op, ROp::DirectCall { receiver: None, .. }) {
                let [value] = stmt.results[..] else {
                    panic!("direct scalar call must have one result")
                };
                let call = text.split_once('(').unwrap().0;
                path.calls.push((
                    match call {
                        "outer" => "outer",
                        "wrap" => "wrap",
                        _ => panic!("unexpected call {text:?}"),
                    },
                    value,
                ));
            }
            if text.starts_with("val n = ") {
                let (ROp::Alias { .. }, [prefix]) = (&stmt.op, stmt.results.as_slice()) else {
                    panic!("the prefix must bind exactly one slot")
                };
                assert!(path.prefix.replace(*prefix).is_none());
            }
            if text.starts_with("val result = ") {
                let (ROp::Alias { source }, [binding]) = (&stmt.op, stmt.results.as_slice()) else {
                    panic!("normal initialization must bind one alias")
                };
                assert_eq!(Some(source), path.calls.last().map(|(_, slot)| slot));
                assert!(path.binding.replace(*binding).is_none());
            }
        }
        let (term, span) = block.terminator.as_ref().unwrap();
        match term {
            RTerm::Return(result) => match inputs.sources.slice(*span).unwrap() {
                "return n" => {
                    assert!(path.calls.is_empty() && path.binding.is_none());
                    assert!(path.prefix.is_some());
                    assert_eq!(*result, path.prefix);
                    returns[0] += 1;
                }
                "return result" => {
                    let expected: &[_] = if nested {
                        &["outer", "wrap"]
                    } else {
                        &["outer"]
                    };
                    assert!(path.calls.iter().map(|(name, _)| name).eq(expected));
                    assert!(path.binding.is_some());
                    assert_eq!(*result, path.binding);
                    returns[1] += 1;
                }
                other => panic!("unexpected return origin {other:?}"),
            },
            _ => pending.extend(
                term.successors()
                    .into_iter()
                    .map(|target| (target, path.clone())),
            ),
        }
    }
    assert!(returns.iter().all(|&count| count > 0));
}
