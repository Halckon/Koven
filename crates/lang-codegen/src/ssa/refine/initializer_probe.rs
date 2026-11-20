//! Test-only observation of the existing builder; never changes admission or emits SSA.
use std::cell::Cell;

use lang_frontend::source::Span;

use crate::ssa::model::LoanKind;

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
    flag_read: Option<Slot>,
    flag_edge: Option<bool>,
    owner: Option<Slot>,
    place: Option<Slot>,
    loan: Option<Slot>,
    ended: bool,
    dropped: bool,
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
            if let ROp::ReadLoan { loan } = stmt.op
                && text == "flag"
            {
                assert_eq!(Some(&loan), body.parameters.last());
                let [read] = stmt.results[..] else {
                    panic!("shared Boolean read has one result")
                };
                assert!(path.flag_read.replace(read).is_none());
            }
            if !nested {
                match stmt.op {
                    ROp::HeapAllocate { .. } if text == "Node()" => {
                        let [owner] = stmt.results[..] else {
                            panic!("constructor must produce one owner")
                        };
                        assert!(path.owner.replace(owner).is_none());
                    }
                    ROp::RootPlace { owner } if path.owner == Some(owner) => {
                        let [place] = stmt.results[..] else {
                            panic!("constructor place has one result")
                        };
                        assert!(path.place.replace(place).is_none());
                    }
                    ROp::BorrowBegin { place, kind } if path.place == Some(place) => {
                        assert_eq!(kind, LoanKind::Shared);
                        let [loan] = stmt.results[..] else {
                            panic!("constructor shared loan has one result")
                        };
                        assert!(path.loan.replace(loan).is_none());
                    }
                    ROp::BorrowEnd { loan } if path.loan == Some(loan) => {
                        assert!(!path.ended && !path.dropped);
                        assert_eq!(
                            path.calls.len(),
                            if path.flag_edge == Some(false) { 1 } else { 0 }
                        );
                        path.ended = true;
                    }
                    ROp::Drop { owner } if path.owner == Some(owner) => {
                        assert!(path.ended && !path.dropped);
                        path.dropped = true;
                    }
                    _ => {}
                }
            }
            if let ROp::DirectCall {
                receiver,
                ref arguments,
                ..
            } = stmt.op
            {
                assert!(receiver.is_none());
                if !nested {
                    assert_eq!(arguments.first().copied(), path.loan);
                    assert!(path.loan.is_some() && !path.ended && !path.dropped);
                }
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
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            } if inputs
                .sources
                .slice(*span)
                .unwrap()
                .starts_with("when (flag)") =>
            {
                assert_eq!(
                    Some(*condition),
                    path.flag_read.or_else(|| body.parameters.last().copied()),
                    "branch uses the actual Boolean input, possibly read through its loan",
                );
                assert!(path.flag_edge.is_none());
                if !nested {
                    assert!(path.owner.is_some() && path.loan.is_some());
                    assert!(!path.ended && !path.dropped);
                }
                for (target, edge) in [(*when_true, true), (*when_false, false)] {
                    let mut next = path.clone();
                    next.flag_edge = Some(edge);
                    pending.push((target, next));
                }
            }
            RTerm::Return(result) => match inputs.sources.slice(*span).unwrap() {
                "return n" => {
                    assert_eq!(path.flag_edge, Some(true));
                    if !nested {
                        assert!(path.loan.is_some() && path.ended && path.dropped);
                    }
                    assert!(path.calls.is_empty() && path.binding.is_none());
                    assert!(path.prefix.is_some());
                    assert_eq!(*result, path.prefix);
                    returns[0] += 1;
                }
                "return result" => {
                    assert_eq!(path.flag_edge, Some(false));
                    if !nested {
                        assert!(path.loan.is_some() && path.ended && path.dropped);
                    }
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
