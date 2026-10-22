//! Four whole ordinary nullable-When contracts, using independent complete sources.
//! The legacy native_tests nullable_when module and its CI-only mixed whole are untouched.

use super::{OwnedProducts, SourceRejectionKind, UnitSource};
use crate::ssa::{
    LoweringErrorKind,
    model::{
        BlockId, Edge, EntityId, EntityType, Function, FunctionId, Instruction, LoanId, LoanKind,
        Operation, Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
    },
    verify::verify_program,
};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{Expression, Statement},
    source::Span,
    type_checking::{UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind},
};
use std::collections::BTreeSet;

#[path = "native_nullable_when_tests/generation.rs"]
mod generation;
#[path = "native_nullable_when_tests/products.rs"]
mod products;
#[path = "native_nullable_when_tests/replacement_if.rs"]
mod replacement_if;
#[path = "native_nullable_when_tests/sources.rs"]
mod sources;
#[path = "native_nullable_when_tests/support.rs"]
mod support;
use sources::{Family, Fixture, NEGATIVES, fixtures};
use support::*;

fn positive_functions(family: Family) -> Vec<&'static str> {
    let mut names = vec![
        "repeated",
        "later",
        "earlyReturn",
        "earlyBreak",
        "earlyContinue",
        "temporary",
    ];
    if family == Family::Node {
        names.extend([
            "temporaryReturn",
            "temporaryBreak",
            "temporaryContinue",
            "doubleReturn",
            "temporaryDoubleReturn",
            "takenReturn",
        ]);
    }
    names
}

fn assert_repeated(source: &UnitSource, program: &Program, family: Family) {
    let function = function(source, program, "repeated");
    let controls = controls(source, "repeated");
    assert_eq!(controls.len(), 3);
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let [original @ EntityId::Value(_)] = entry.parameters.as_slice() else {
        panic!("one owned nullable input")
    };
    let owners = aliases(function, *original);
    let mut births = BTreeSet::new();
    for (index, control) in controls.iter().enumerate() {
        let birth = birth(function, control);
        assert!(owners.contains(&EntityId::Value(birth.owner)));
        assert!(
            births.insert(birth.view),
            "each real When, including each loop iteration site, births a fresh scoped view"
        );
        let module = program.module(function.id.module()).unwrap();
        let storage = function
            .entity(EntityId::Value(birth.owner))
            .unwrap()
            .ty
            .semantic_type();
        let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(storage) else {
            panic!("real nullable storage")
        };
        assert!(match family {
            Family::Node | Family::Box => matches!(
                module.type_kind(*inner),
                Some(SsaTypeKind::HeapOwner { .. })
            ),
            Family::Rc => matches!(
                module.type_kind(*inner),
                Some(SsaTypeKind::SharedOwner { .. })
            ),
        });
        assert_eq!(
            function.entity(EntityId::Loan(birth.view)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
        let body = function.block(birth.present.target).unwrap();
        let instructions = body
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        assert!(
            !instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::SharedRetain { .. } | Operation::BorrowBegin { .. }
            )),
            "observe reuses the view, not a second owner/loan"
        );
        if index < 2 {
            let calls = instructions.iter().filter(|instruction| matches!(&instruction.operation,
                Operation::DirectCall { callee, receiver: None, arguments } if *callee == self::function(source, program, "observe").id && arguments == &[EntityId::Loan(birth.view)])).collect::<Vec<_>>();
            assert_eq!(calls.len(), if index == 0 { 2 } else { 1 });
            assert!(!instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::NullableTake { .. } | Operation::Drop { .. }
            )));
            let ends = instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == birth.view)).collect::<Vec<_>>();
            let [end] = ends.as_slice() else {
                panic!("one view retirement on this path")
            };
            assert!(
                calls
                    .iter()
                    .all(|call| position(function, call) < position(function, end))
            );
            assert_eq!(end.origin, Origin::Source(control.span));
            let TerminatorKind::Branch(edge) = &body.terminator.as_ref().unwrap().kind else {
                panic!("normal When joins")
            };
            assert!(
                edge.arguments
                    .iter()
                    .any(|argument| owners.contains(argument))
            );
            assert!(
                edge.arguments
                    .iter()
                    .all(|argument| !birth.views.contains(argument)),
                "no view escapes to join or loop backedge"
            );
        } else {
            let argument = expression(source, control.span, "(subject)").1;
            let take = instruction(function, argument, |op| {
                matches!(op, Operation::NullableTake { .. })
            });
            assert_eq!(
                take.operation,
                Operation::NullableTake {
                    owner: branch_owner(function, birth.present, birth.owner),
                    proof: birth.view
                }
            );
            let [inner_owner @ EntityId::Value(_)] = take.results.as_slice() else {
                panic!("one unique extracted inner owner")
            };
            assert_eq!(
                function.entity(*inner_owner).unwrap().ty,
                EntityType::Value(*inner)
            );
            let call = instruction(
                function,
                expression(source, control.span, "consume((subject))").1,
                |op| matches!(op, Operation::DirectCall { .. }),
            );
            assert!(
                matches!(&call.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == self::function(source, program, "consume").id && arguments == &[*inner_owner])
            );
            assert_eq!(take.block, call.block);
            assert!(position(function, take) < position(function, call));
            let actions = instructions
                .iter()
                .copied()
                .filter(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::NullableTake { .. }
                            | Operation::BorrowEnd { .. }
                            | Operation::Drop { .. }
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actions,
                [take],
                "take retires wrapper and view, caller never drops inner after commit"
            );
            let absent = function.block(birth.null.target).unwrap();
            let drops = absent.instructions.iter().map(|id| function.instruction(*id).unwrap()).filter(|instruction|
                matches!(instruction.operation, Operation::Drop { owner } if birth.owners.contains(&EntityId::Value(owner)))).collect::<Vec<_>>();
            assert_eq!(
                drops.len(),
                1,
                "null path retires the original wrapper exactly once"
            );
        }
    }
}

#[test]
fn nullable_when_views_and_direct_consume_use_real_owner_proofs() {
    for prepared in prepared() {
        for name in positive_functions(prepared.fixture.family) {
            for control in controls(&prepared.source, name) {
                products::assert_existing_call_facts(&prepared.source, &control);
                products::assert_new_products(&prepared.source, &control);
            }
        }
        assert_repeated(&prepared.source, &prepared.program, prepared.fixture.family);
        let llvm =
            crate::llvm::render_verified_program_with_entry(&prepared.program, prepared.entry)
                .expect("verified real Unit SSA reaches LLVM");
        assert!(llvm.contains("define i32 @main("));
    }
}

fn enclosing_call(source: &UnitSource, scope: Span, control: Span, target: &str) -> Span {
    let OwnedProducts::Basic(typed, _) = &source.products else {
        panic!("Basic")
    };
    let declaration = source.declaration(target, SymbolKind::Function);
    let calls = typed
        .types()
        .calls()
        .iter()
        .filter(|call| {
            let origin = span(source, call.expression());
            call.target() == UnitCallTarget::Declaration(declaration)
                && contains(scope, origin)
                && contains(origin, control)
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one enclosing {target} transaction")
    };
    span(source, call.expression())
}

fn assert_cleanup_facts(
    source: &UnitSource,
    function: &Function,
    control: &Control,
    birth: &Birth<'_>,
    temporary: bool,
    exits: &[UnitExpressionId],
    normal: bool,
) {
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("Basic")
    };
    let target = if temporary {
        UnitDropTarget::Temporary(control.subject)
    } else {
        UnitDropTarget::Named(root(source, control.subject))
    };
    let drops = owned
        .ownership()
        .drops()
        .iter()
        .filter(|fact| fact.target() == target)
        .collect::<Vec<_>>();
    if temporary {
        let expected = usize::from(normal) + exits.len();
        assert_eq!(
            drops.len(),
            expected,
            "one original temporary obligation per actual completion/transfer path"
        );
        if normal {
            assert_eq!(
                drops
                    .iter()
                    .filter(|fact| fact.point() == UnitDropPoint::AfterExpression(control.id))
                    .count(),
                1
            );
        }
        for exit in exits {
            assert_eq!(
                drops
                    .iter()
                    .filter(|fact| fact.point() == UnitDropPoint::ControlTransfer(*exit))
                    .count(),
                1
            );
        }
        assert!(
            drops
                .iter()
                .all(|fact| fact.value_origin() == control.subject_span)
        );
        assert!(
            !drops
                .iter()
                .any(|fact| fact.point() == UnitDropPoint::AfterExpression(control.subject)),
            "temporary cannot die at subject evaluation"
        );
    } else {
        let [fact] = drops.as_slice() else {
            panic!("single pure-memory named ASAP fact")
        };
        assert_eq!(
            fact.point(),
            UnitDropPoint::AfterExpression(control.subject)
        );
    }
    let origins = drops
        .iter()
        .map(|fact| fact.value_origin())
        .collect::<Vec<_>>();
    for instruction in function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation,
        Operation::Drop { owner } if birth.owners.contains(&EntityId::Value(owner)))
    }) {
        assert!(
            origins.contains(&instruction.origin.span()),
            "SSA Drop keeps the real FE value origin"
        );
    }
    assert!(!function.block(birth.header).unwrap().instructions.iter().map(|id| function.instruction(*id).unwrap())
        .any(|instruction| matches!(instruction.operation, Operation::Drop { owner } if birth.owners.contains(&EntityId::Value(owner)))),
        "subject survives through the actual NullableBranch decision");
}

fn assert_transfer(source: &UnitSource, program: &Program, name: &str, tag: &str, temporary: bool) {
    use Event::*;
    let function = function(source, program, name);
    let scope = item_span(source, name);
    let mut controls = controls(source, name);
    assert_eq!(controls.len(), 1);
    let control = controls.remove(0);
    let birth = birth(function, &control);
    let exit_text = if tag.ends_with("return") {
        "return"
    } else if tag.ends_with("break") {
        "break"
    } else {
        "continue"
    };
    let (exit, exit_span) = expression(source, control.span, exit_text);
    let stop = match exit_text {
        "return" => Stop::Return(exit_span),
        "break" => loop_stop(source, function, scope, exit_span, true),
        "continue" => loop_stop(source, function, scope, exit_span, false),
        _ => unreachable!(),
    };
    let checkpoint = expression(source, scope, &format!("println(\"{tag}-after\")")).1;
    let marker = if temporary {
        tag.to_owned()
    } else {
        format!("{tag}-present")
    };
    let body = expression(source, control.span, &format!("println(\"{marker}\")")).1;
    let path = Path {
        function,
        birth: &birth,
        prefix: Some(temporary_track(source, function, scope, "Prefix()")),
        outer: Some(temporary_track(source, function, scope, "Outer()")),
        body: Some(body),
        inner_call: Some(enclosing_call(source, scope, control.span, "accept")),
        outer_call: Some(enclosing_call(source, scope, control.span, "enclose")),
        observe: None,
        take: None,
        take_call: None,
        inner: BTreeSet::new(),
    };
    assert_cleanup_facts(
        source,
        function,
        &control,
        &birth,
        temporary,
        if temporary {
            std::slice::from_ref(&exit)
        } else {
            &[]
        },
        true,
    );
    path.check(
        birth.null.target,
        &Stop::Checkpoint(checkpoint),
        &[
            SubjectDrop,
            InnerCall,
            PrefixEnd,
            PrefixDrop,
            OuterCall,
            OuterEnd,
            OuterDrop,
        ],
    );
    if temporary {
        assert_make_once(source, program, name, control.subject_span, birth.owner);
        path.check(
            birth.present.target,
            &stop,
            &[
                ViewEnd,
                Body,
                PrefixEnd,
                OuterEnd,
                SubjectDrop,
                PrefixDrop,
                OuterDrop,
            ],
        );
    } else {
        path.check(
            birth.present.target,
            &stop,
            &[
                ViewEnd,
                SubjectDrop,
                Body,
                PrefixEnd,
                OuterEnd,
                PrefixDrop,
                OuterDrop,
            ],
        );
        if exit_text != "return" {
            assert_make_once(
                source,
                program,
                name,
                expression(source, scope, "make(present)").1,
                birth.owner,
            );
        }
    }
}

fn assert_temporary_normal(source: &UnitSource, program: &Program) {
    use Event::*;
    let function = function(source, program, "temporary");
    let scope = item_span(source, "temporary");
    let controls = controls(source, "temporary");
    let [control] = controls.as_slice() else {
        panic!("one whole temporary When")
    };
    let birth = birth(function, control);
    assert_make_once(
        source,
        program,
        "temporary",
        control.subject_span,
        birth.owner,
    );
    assert_cleanup_facts(source, function, control, &birth, true, &[], true);
    let mut path = Path {
        function,
        birth: &birth,
        prefix: None,
        outer: Some(temporary_track(source, function, scope, "Outer()")),
        body: Some(expression(source, control.span, "println(\"temporary-null\")").1),
        inner_call: None,
        outer_call: Some(enclosing_call(source, scope, control.span, "enclose")),
        observe: None,
        take: None,
        take_call: None,
        inner: BTreeSet::new(),
    };
    let stop = Stop::Checkpoint(expression(source, scope, "println(\"temporary-after\")").1);
    path.check(
        birth.null.target,
        &stop,
        &[Body, SubjectDrop, OuterCall, OuterEnd, OuterDrop],
    );
    path.body = Some(expression(source, control.span, "println(\"temporary-present\")").1);
    path.check(
        birth.present.target,
        &stop,
        &[ViewEnd, Body, SubjectDrop, OuterCall, OuterEnd, OuterDrop],
    );
}

fn assert_later_live(source: &UnitSource, program: &Program) {
    let function = function(source, program, "later");
    let controls = controls(source, "later");
    let [control] = controls.as_slice() else {
        panic!("one later-live When")
    };
    let birth = birth(function, control);
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("Basic")
    };
    let root = root(source, control.subject);
    assert!(!owned.ownership().drops().iter().any(|fact| fact.target() == UnitDropTarget::Named(root) &&
        (fact.point() == UnitDropPoint::AfterExpression(control.subject) || matches!(fact.point(), UnitDropPoint::BranchExit { control: id, .. } if id == control.id))),
        "later-live subject has no early or branch-exit drop fact");
    for edge in [birth.null, birth.present] {
        let block = function.block(edge.target).unwrap();
        assert!(!block.instructions.iter().map(|id| function.instruction(*id).unwrap()).any(|instruction|
            matches!(instruction.operation, Operation::Drop { owner } if birth.owners.contains(&EntityId::Value(owner)))));
        let TerminatorKind::Branch(join) = &block.terminator.as_ref().unwrap().kind else {
            panic!("later-live branch joins")
        };
        assert!(
            join.arguments
                .iter()
                .any(|argument| birth.owners.contains(argument))
        );
        assert!(
            join.arguments
                .iter()
                .all(|argument| !birth.views.contains(argument))
        );
    }
    let call = instruction(
        function,
        expression(source, item_span(source, "later"), "touchNullable(subject)").1,
        |op| matches!(op, Operation::DirectCall { .. }),
    );
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    let [EntityId::Loan(loan)] = arguments.as_slice() else {
        panic!("later nullable borrow")
    };
    let end = function.instructions.iter().find(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan: actual } if actual == *loan)).unwrap();
    let drops = function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::Drop { owner } if birth.owners.contains(&EntityId::Value(owner)))).collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one post-join owner retirement")
    };
    assert_eq!(call.block, end.block);
    assert_eq!(call.block, drop.block);
    assert!(
        position(function, call) < position(function, end)
            && position(function, end) < position(function, drop)
    );
}

fn assert_double_escape(source: &UnitSource, program: &Program, temporary: bool) {
    use Event::*;
    let name = if temporary {
        "temporaryDoubleReturn"
    } else {
        "doubleReturn"
    };
    let function = function(source, program, name);
    let controls = controls(source, name);
    let [control] = controls.as_slice() else {
        panic!("one doubly escaping When")
    };
    let birth = birth(function, control);
    let null_return = expression(source, control.entries[0].1, "return");
    let present_return = expression(source, control.entries[1].1, "return");
    if temporary {
        assert_make_once(source, program, name, control.subject_span, birth.owner);
        assert_cleanup_facts(
            source,
            function,
            control,
            &birth,
            true,
            &[null_return.0, present_return.0],
            false,
        );
    }
    let mut path = Path {
        function,
        birth: &birth,
        prefix: None,
        outer: None,
        body: Some(
            expression(
                source,
                control.entries[0].1,
                if temporary {
                    "println(\"temporary-double-null\")"
                } else {
                    "println(\"double-null\")"
                },
            )
            .1,
        ),
        inner_call: None,
        outer_call: None,
        observe: if temporary {
            None
        } else {
            Some(expression(source, control.entries[1].1, "observe(subject)").1)
        },
        take: None,
        take_call: None,
        inner: BTreeSet::new(),
    };
    path.check(
        birth.null.target,
        &Stop::Return(null_return.1),
        &[Body, SubjectDrop],
    );
    path.body = if temporary {
        Some(
            expression(
                source,
                control.entries[1].1,
                "println(\"temporary-double-present\")",
            )
            .1,
        )
    } else {
        None
    };
    path.check(
        birth.present.target,
        &Stop::Return(present_return.1),
        if temporary {
            &[ViewEnd, Body, SubjectDrop]
        } else {
            &[Observe, ViewEnd, SubjectDrop]
        },
    );
    let returns = function
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        returns.len(),
        2,
        "both source transfers end normally; no synthetic residual normal return"
    );
}

fn assert_take_then_return(source: &UnitSource, program: &Program) {
    use Event::*;
    let function = function(source, program, "takenReturn");
    let scope = item_span(source, "takenReturn");
    let controls = controls(source, "takenReturn");
    let [control] = controls.as_slice() else {
        panic!("one outer nullable When")
    };
    let birth = birth(function, control);
    let take_span = expression(source, control.span, "(subject)").1;
    let take = instruction(function, take_span, |operation| {
        matches!(operation, Operation::NullableTake { .. })
    });
    assert_eq!(
        take.operation,
        Operation::NullableTake {
            owner: branch_owner(function, birth.present, birth.owner),
            proof: birth.view
        }
    );
    let [inner @ EntityId::Value(_)] = take.results.as_slice() else {
        panic!("one extracted inner")
    };
    let inner_control = expression(
        source,
        control.span,
        "when (stop) { true -> return; else -> 7 }",
    )
    .1;
    let branches = function
        .blocks
        .iter()
        .filter(|block| {
            block.terminator.as_ref().is_some_and(|term| {
                term.origin == Origin::Source(inner_control)
                    && matches!(term.kind, TerminatorKind::Conditional { .. })
            })
        })
        .collect::<Vec<_>>();
    let [branch] = branches.as_slice() else {
        panic!("the exact source Boolean When decides cancellation")
    };
    assert_eq!(
        branch.id, take.block,
        "the one take precedes evaluation of the subsequent argument"
    );
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &branch.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    for edge in [when_true, when_false] {
        assert!(edge.arguments.iter().all(|argument| !birth.owners.contains(argument) && !birth.views.contains(argument)));
        assert_eq!(
            edge.arguments
                .iter()
                .filter(|argument| **argument == *inner)
                .count(),
            1
        );
    }
    let call_span = enclosing_call(source, scope, inner_control, "delayed");
    let path = Path {
        function,
        birth: &birth,
        prefix: None,
        outer: None,
        body: None,
        inner_call: None,
        outer_call: None,
        observe: None,
        take: Some(take_span),
        take_call: Some(call_span),
        inner: aliases(function, *inner),
    };
    let (returned, returned_span) = expression(source, inner_control, "return");
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("Basic")
    };
    let argument = expression(source, control.span, "(subject)").0;
    let drops = owned
        .ownership()
        .drops()
        .iter()
        .filter(|fact| {
            fact.point() == UnitDropPoint::ControlTransfer(returned)
                && fact.target() == UnitDropTarget::Temporary(argument)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        1,
        "the already-extracted Value argument has one actual pending inner obligation"
    );
    path.check(when_true.target, &Stop::Return(returned_span), &[InnerDrop]);
    let checkpoint = Stop::Checkpoint(expression(source, scope, "println(\"taken-after\")").1);
    path.check(when_false.target, &checkpoint, &[TakeCommit]);
    path.check(birth.null.target, &checkpoint, &[SubjectDrop]);
    assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if birth.views.contains(&EntityId::Loan(loan)))),
        "take itself consumes its view, no second end");
}

#[test]
fn nullable_when_asap_temporaries_and_transfers_have_exact_cleanup() {
    for prepared in prepared() {
        let source = &prepared.source;
        let program = &prepared.program;
        assert_later_live(source, program);
        for (name, tag) in [
            ("earlyReturn", "return"),
            ("earlyBreak", "break"),
            ("earlyContinue", "continue"),
        ] {
            assert_transfer(source, program, name, tag, false);
        }
        assert_temporary_normal(source, program);
        if prepared.fixture.family == Family::Node {
            for (name, tag) in [
                ("temporaryReturn", "temporary-return"),
                ("temporaryBreak", "temporary-break"),
                ("temporaryContinue", "temporary-continue"),
            ] {
                assert_transfer(source, program, name, tag, true);
            }
            assert_double_escape(source, program, false);
            assert_double_escape(source, program, true);
            assert_take_then_return(source, program);
        }
    }
}

#[test]
fn nullable_when_class_box_rc_ordinary_sources_link_run() {
    use std::{fs, process::Command};
    // All three complete documents must pass real SSA preflight before launching any native work.
    for prepared in prepared() {
        let directory = crate::native_tests::TestDirectory::create();
        let object = directory.join("ordinary-nullable-when.o");
        let executable = directory.join("ordinary-nullable-when");
        prepared
            .source
            .emit(
                prepared.source.declaration("entry", SymbolKind::Function),
                &object,
            )
            .expect("original Basic Unit emits the actual object");
        let bytes = fs::read(&object).expect("actual emitted object bytes");
        assert!(bytes.len() >= 20);
        crate::test_support::assert_native_object(&bytes);
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("existing native linker launches");
        assert_eq!(
            linked.status.code(),
            Some(0),
            "{}: {linked:?}",
            prepared.fixture.name
        );
        assert!(
            linked.stderr.is_empty(),
            "{}: {linked:?}",
            prepared.fixture.name
        );
        let run = Command::new(&executable)
            .output()
            .expect("newly linked ordinary executable launches");
        assert_eq!(
            run.status.code(),
            Some(0),
            "{}: {run:?}",
            prepared.fixture.name
        );
        assert!(run.stderr.is_empty(), "{}: {run:?}", prepared.fixture.name);
        assert_eq!(
            run.stdout, prepared.fixture.stdout,
            "{}: full independently source-authored stdout",
            prepared.fixture.name
        );
    }
}

#[test]
fn nullable_when_out_of_slice_sources_keep_exact_boundaries() {
    for fixture in NEGATIVES {
        if fixture.name == "replaced-generation" {
            replacement_if::assert_original_replacement(fixture);
            continue;
        }
        let expected = fixture.green_span; // RED VERSION selects red_span instead.
        match UnitSource::analyze(fixture.name, fixture.text) {
            Err(rejection) => {
                assert_ne!(
                    fixture.stage, "lower",
                    "{}: unexpected earlier frontend boundary {rejection:?}",
                    fixture.name
                );
                assert!(matches!(rejection.kind, SourceRejectionKind::Diagnostics));
                let [(stage, diagnostic)] = rejection.report.diagnostics.as_slice() else {
                    panic!(
                        "{}: one exact source diagnostic, no extra cascades",
                        fixture.name
                    )
                };
                assert_eq!(*stage, fixture.stage);
                assert_eq!(diagnostic.code().to_string(), fixture.code);
                let origin = diagnostic.primary_span();
                assert_eq!((origin.start(), origin.end()), expected, "{}", fixture.name);
                assert_eq!(
                    rejection.sources.source_text(origin.source_id()),
                    Ok(fixture.text)
                );
                assert_eq!(
                    rejection.sources.slice(origin),
                    Ok(&fixture.text[expected.0..expected.1])
                );
                assert_eq!(
                    rejection.report.stages,
                    [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
                );
            }
            Ok(source) => {
                assert_eq!(
                    fixture.stage, "lower",
                    "{}: a required original frontend rejection disappeared",
                    fixture.name
                );
                source.assert_frontend_clean();
                assert!(matches!(&source.products, OwnedProducts::Basic(..)));
                let error = source
                    .lower_entry(source.declaration("entry", SymbolKind::Function))
                    .err()
                    .unwrap_or_else(|| {
                        panic!("{}: unsupported source must not lower", fixture.name)
                    });
                assert_eq!(
                    error.kind,
                    LoweringErrorKind::UnsupportedNode,
                    "{}",
                    fixture.name
                );
                let origin = error.span.expect("source-qualified backend boundary");
                assert_eq!(origin.source_id(), source.parsed.source_id());
                assert_eq!((origin.start(), origin.end()), expected, "{}", fixture.name);
                assert_eq!(
                    source.sources.slice(origin),
                    Ok(&fixture.text[expected.0..expected.1])
                );
            }
        }
    }
}
