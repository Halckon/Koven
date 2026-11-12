//! Ordinary source contracts for evaluated construction prefixes and later return.
//! These tests only inspect real products; no facts, IDs, or SSA are fabricated or mutated.

use super::{OwnedProducts, UnitSource};
use crate::test_support::diagnostics::assert_diagnostic;
use crate::{
    native_tests::{TestDirectory, emit_link_and_run},
    ssa::{
        model::{Operation, Origin, TerminatorKind},
        verify::verify_program,
    },
};
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
    source::Span,
    type_checking::{CompilationUnitTypes, Copyability, UnitExpressionId},
};
use std::fs;

fn products(source: &UnitSource) -> (&CompilationUnitTypes, &CompilationUnitOwnership) {
    match &source.products {
        OwnedProducts::Basic(typed, owned) => (typed.types(), owned.ownership()),
        OwnedProducts::Constant(typed, owned) => (typed.types(), owned.ownership()),
    }
}

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let units = source.names.names().index().source_units();
    let [unit] = units else {
        panic!("one real MemoryDocument")
    };
    assert_eq!(unit.source_id(), source.parsed.source_id());
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| source.sources.slice(node.span()) == Ok(text))
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one real expression {text:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn source_text(header: &str, binding: &str, construction: &str, constant: bool) -> String {
    let constants = if constant {
        "const val PREFIX = \"prefix\"\n"
    } else {
        ""
    };
    format!(
        "{constants}{header}\nfun probe(flag: Boolean): Unit {{\n{binding}\nval result = {construction}\n}}"
    )
}

fn assert_pending_source(
    text: &str,
    construction: &str,
    argument: &str,
    owner: &str,
    copyable: bool,
    constant: bool,
) {
    let source = UnitSource::analyze("pending-construction.ko", text)
        .expect("whole source passes strict final admission and real ownership");
    source.assert_frontend_clean();
    assert_eq!(
        matches!(&source.products, OwnedProducts::Constant(_, _)),
        constant
    );
    let (typed, owned) = products(&source);
    let (call, call_span) = expression(&source, construction);
    let (argument, _) = expression(&source, argument);
    let (owner, owner_span) = expression(&source, owner);
    let (returned, return_span) = expression(&source, "return");
    let argument_type = typed
        .expression_type(argument)
        .expect("actual argument type");
    assert_eq!(
        typed.copyability(argument_type),
        if copyable {
            Copyability::Copyable
        } else {
            Copyability::MoveOnly
        }
    );
    if argument != owner {
        let OwnedProducts::Constant(_, constant_owned) = &source.products else {
            panic!("only a real grouped constant use has a distinct materialization owner")
        };
        assert!(constant_owned.materialization_at(owner).is_some());
        assert!(constant_owned.materialization_at(argument).is_none());
    }
    if let Some(descriptor) = typed.construction(call) {
        assert_eq!(descriptor.arguments()[0].argument(), argument);
        let plans = owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == call)
            .collect::<Vec<_>>();
        let [plan] = plans.as_slice() else {
            panic!("one actual nominal plan")
        };
        assert!(
            plan.terminating_operand().is_none(),
            "normal branch still commits"
        );
        assert_eq!(plan.deliveries()[0].argument(), argument);
        assert_eq!(plan.deliveries()[0].evaluation_index(), 0);
    } else {
        let descriptor = typed
            .container_construction(call)
            .expect("actual container descriptor");
        assert_eq!(descriptor.expression(), call);
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call && delivery.argument() == argument)
            .collect::<Vec<_>>();
        assert_eq!(deliveries.len(), 1, "one actual evaluated prefix delivery");
    }
    // Producer oracle comes before lowering: nominal's old missing pending fact must fail here.
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Temporary(owner))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        usize::from(!copyable),
        "exact pending owner facts: {drops:?}"
    );
    if let [drop] = drops.as_slice() {
        assert_eq!(drop.point(), UnitDropPoint::ControlTransfer(returned));
        assert_eq!(drop.value_origin(), owner_span);
    }
    // No normal-completion cleanup for this owner remains after delivery to the construction.
    let entry = source.declaration("probe", SymbolKind::Function);
    let (program, function) = source
        .lower_entry(entry)
        .expect("actual products lower to SSA");
    verify_program(&program).expect("source-produced SSA verifies ownership on both branches");
    let function = program
        .module(function.module())
        .unwrap()
        .function(function)
        .unwrap();
    let exits = function
        .blocks
        .iter()
        .filter(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                matches!(terminator.kind, TerminatorKind::Return { .. })
                    && terminator.origin == Origin::Source(return_span)
            })
        })
        .collect::<Vec<_>>();
    let [early] = exits.as_slice() else {
        panic!("one explicit source return block")
    };
    let early_drops = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.block == early.id && matches!(instruction.operation, Operation::Drop { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        early_drops.len(),
        usize::from(!copyable),
        "only the evaluated prefix is abandoned"
    );
    if let [drop] = early_drops.as_slice() {
        assert_eq!(drop.origin, Origin::Source(owner_span));
    }
    let prefix_drops = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(owner_span)
                && matches!(instruction.operation, Operation::Drop { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prefix_drops.len(),
        usize::from(!copyable),
        "no duplicate drop on normal commit"
    );
    let commits = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(call_span)
                && matches!(
                    instruction.operation,
                    Operation::AggregateConstruct { .. } | Operation::ContainerConstruct { .. }
                )
        })
        .collect::<Vec<_>>();
    let [commit] = commits.as_slice() else {
        panic!("one actual normal construction commit")
    };
    assert_ne!(
        commit.block, early.id,
        "early return never commits the partially evaluated object"
    );
}

fn assert_duplicate_move_stays_rejected() {
    const TEXT: &str = "class Pair(val first: String, val second: String)\nfun entry(): Unit { val prefix = \"move\"; val pair = Pair(prefix, prefix) }";
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    fs::write(&object, b"previous object").unwrap();
    fs::write(directory.join("neighbor.txt"), b"ordinary neighbor").unwrap();
    let before = directory.snapshot();
    let rejected = UnitSource::analyze("duplicate-construction-move.ko", TEXT)
        .err()
        .expect("second delivery of one MoveOnly owner remains a real FE rejection");
    assert_eq!(
        rejected.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
    );
    let [(stage, diagnostic)] = rejected.report.diagnostics.as_slice() else {
        panic!("one original ownership diagnostic")
    };
    assert_eq!(*stage, "ownership");
    assert_eq!(diagnostic.code().to_string(), "L0131");
    let span = diagnostic.primary_span();
    assert_eq!(
        rejected.sources.source_name(span.source_id()).unwrap(),
        "duplicate-construction-move.ko"
    );
    let second = TEXT.rfind("prefix").unwrap();
    assert_eq!(
        (span.start(), span.end()),
        (second, second + "prefix".len())
    );
    assert_eq!(rejected.sources.slice(span).unwrap(), "prefix");
    assert_eq!(
        directory.snapshot(),
        before,
        "frontend rejection never touches existing output"
    );
}

fn assert_generic_copyable_specialization() {
    let source = UnitSource::analyze(
        "pending-generic-copyable.ko",
        r#"
class Holder<T>(val first: T, val number: Int)
fun <T> probe(own prefix: T, flag: Boolean): Unit {
    val holder = Holder<T>(prefix, if (flag) { return } else { 1 })
}
fun entry(): Unit { probe<Int>(7, true); probe<Int>(7, false) }
"#,
    )
    .expect("existing generic function and Holder specialization remain admitted");
    source.assert_frontend_clean();
    let (typed, owned) = products(&source);
    let (argument, origin) = expression(&source, "prefix");
    let (returned, _) = expression(&source, "return");
    assert_eq!(
        typed.copyability(typed.expression_type(argument).unwrap()),
        Copyability::MoveOnly
    );
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| {
            drop.target() == UnitDropTarget::Temporary(argument)
                && drop.point() == UnitDropPoint::ControlTransfer(returned)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        1,
        "real generic template has a pending owner obligation"
    );
    let (program, _) = source
        .lower_entry(source.declaration("entry", SymbolKind::Function))
        .expect("T to Int specialization consumes the template obligation without dropping Int");
    verify_program(&program).expect("real specialized SSA verifies");
    let declaration = source.declaration("probe", SymbolKind::Function);
    let declaration = &source.names.names().index().declarations()[declaration.index()];
    let probe_span = source
        .parsed
        .ast()
        .items()
        .get(declaration.root())
        .unwrap()
        .span();
    assert_eq!(probe_span.source_id(), source.parsed.source_id());
    let probes = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin.span() == probe_span)
        .collect::<Vec<_>>();
    let [probe] = probes.as_slice() else {
        panic!("one actual Int specialization")
    };
    assert!(
        !probe.instructions.iter().any(|instruction| {
            instruction.origin == Origin::Source(origin)
                && matches!(instruction.operation, Operation::Drop { .. })
        }),
        "concrete Copyable prefix has no runtime drop"
    );
}

#[test]
fn nominal_pending_prefix_facts_and_verified_ssa_preserve_transfer() {
    for constant in [false, true] {
        let binding = if constant {
            "val prefix = PREFIX"
        } else {
            "val prefix = \"prefix\""
        };
        for (header, argument) in [
            ("class Holder(val first: String, val number: Int)", "prefix"),
            (
                "class Holder(val first: String, val number: Int)",
                "(prefix)",
            ),
            (
                "class Leaf(val text: String)\nclass Holder(val first: Leaf, val number: Int)",
                "Leaf(prefix)",
            ),
        ] {
            let construction = format!("Holder({argument}, if (flag) {{ return }} else {{ 1 }})");
            let text = source_text(header, binding, &construction, constant);
            assert_pending_source(&text, &construction, argument, argument, false, constant);
        }
    }
    let construction = "Holder((PREFIX), if (flag) { return } else { 1 })";
    let text = source_text(
        "class Holder(val first: String, val number: Int)",
        "",
        construction,
        true,
    );
    assert_pending_source(&text, construction, "(PREFIX)", "PREFIX", false, true);
    let construction = "Holder(prefix, if (flag) { return } else { 1 })";
    let text = source_text(
        "class Holder(val first: Int, val number: Int)",
        "val prefix = 7",
        construction,
        false,
    );
    assert_pending_source(&text, construction, "prefix", "prefix", true, false);
    assert_generic_copyable_specialization();
    assert_duplicate_move_stays_rejected();
}

#[test]
fn container_pending_prefix_facts_and_verified_ssa_preserve_transfer() {
    for constant in [false, true] {
        let binding = if constant {
            "val prefix = PREFIX"
        } else {
            "val prefix = \"prefix\""
        };
        for (header, element, argument, tail) in [
            ("", "String", "prefix", "\"last\""),
            ("", "String", "(prefix)", "\"last\""),
            (
                "class Leaf(val text: String)",
                "Leaf",
                "Leaf(prefix)",
                "Leaf(\"last\")",
            ),
        ] {
            let construction =
                format!("arrayOf<{element}>({argument}, if (flag) {{ return }} else {{ {tail} }})");
            let text = source_text(header, binding, &construction, constant);
            assert_pending_source(&text, &construction, argument, argument, false, constant);
        }
    }
    let construction = "arrayOf<String>((PREFIX), if (flag) { return } else { \"last\" })";
    let text = source_text("", "", construction, true);
    assert_pending_source(&text, construction, "(PREFIX)", "PREFIX", false, true);
    let construction = "arrayOf<Int>(prefix, if (flag) { return } else { 1 })";
    let text = source_text("", "val prefix = 7", construction, false);
    assert_pending_source(&text, construction, "prefix", "prefix", true, false);
}

#[test]
fn pending_construction_returns_and_commits_run_with_exact_resource_output() {
    for constant in [false, true] {
        let constants = if constant {
            "const val NORMAL = false"
        } else {
            ""
        };
        let normal = if constant { "NORMAL" } else { "false" };
        let text = format!(
            r#"{constants}
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
class Holder(val first: Leaf, val number: Int)
fun nominal(flag: Boolean): Unit {{
    val prefix = Leaf("nominal-prefix")
    val holder = Holder((prefix), if (flag) {{ println("nominal-return"); return }} else {{ 1 }})
    println("nominal-commit")
}}
fun container(flag: Boolean): Unit {{
    val prefix = Leaf("container-prefix")
    val array = arrayOf<Leaf>((prefix), if (flag) {{ println("container-return"); return }} else {{ Leaf("container-last") }})
    println("container-commit")
}}
fun entry(): Unit {{ nominal(true); nominal({normal}); container(true); container({normal}) }}
"#
        );
        let output = emit_link_and_run("pending-construction-resource.ko", &text, "entry");
        assert_diagnostic!(output.status.success(), "{output:?}");
        assert_eq!(
            output.stdout,
            b"nominal-return\nnominal-prefix\nnominal-commit\nnominal-prefix\ncontainer-return\ncontainer-prefix\ncontainer-commit\ncontainer-last\ncontainer-prefix\n"
        );
        assert_diagnostic!(output.stderr.is_empty(), "{output:?}");
    }
}

pub(crate) mod nullable_rc_proof_tests {
//! Ordinary nullable Rc source contracts; only real strict Unit products are inspected.

use super::super::{OwnedProducts, UnitSource};
use crate::{
    native_tests::emit_link_and_run,
    ssa::{
        model::{
            Edge, EntityId, EntityType, Function, Instruction, LoanKind, Operation, PlaceAccess,
            SsaTypeKind, TerminatorKind, ValueId,
        },
        verify::verify_program,
    },
};
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::RcOwnershipEffectKind,
    parser::Expression,
    source::Span,
    type_checking::{RcOperationKind, UnitExpressionId},
};

const NULLABLE_SOURCE: &str = r#"fun inspect(own owner: Rc<Int>?): Rc<Int>? {
    if (owner != null) {
        val retained = owner.share()
        val copied = owner.value
        val retainedCopy = retained.value
    }
    return owner
}"#;

const PLAIN_SOURCE: &str = r#"fun inspect(own owner: Rc<Int>): Rc<Int> {
    val retained = owner.share()
    val copied = owner.value
    val retainedCopy = retained.value
    return owner
}"#;

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one real memory document")
    };
    assert_eq!(unit.source_id(), source.parsed.source_id());
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| source.sources.slice(node.span()) == Ok(text))
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one actual expression {text:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn instruction<'a>(
    source: &UnitSource,
    function: &'a Function,
    text: &str,
    accepts: impl Fn(&Operation) -> bool,
) -> &'a Instruction {
    let (_, span) = expression(source, text);
    let found = function
        .instructions
        .iter()
        .filter(|instruction| instruction.origin.span() == span && accepts(&instruction.operation))
        .collect::<Vec<_>>();
    let [instruction] = found.as_slice() else {
        panic!("one actual SSA operation at {text:?}")
    };
    instruction
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("actual owned value")
    };
    value
}

fn forwarded(function: &Function, edge: &Edge, owner: ValueId) -> ValueId {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(column, entity)| (*entity == EntityId::Value(owner)).then_some(column))
        .collect::<Vec<_>>();
    let [column] = columns.as_slice() else {
        panic!("the same nullable owner crosses each edge exactly once")
    };
    value(function.block(edge.target).unwrap().parameters[*column])
}

fn assert_operations(source: &UnitSource, function: &Function, owner: EntityId) -> ValueId {
    let retain = instruction(source, function, "owner.share()", |operation| {
        matches!(operation, Operation::SharedRetain { .. })
    });
    assert_eq!(retain.operation, Operation::SharedRetain { owner });
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::SharedRetain { .. })
            })
            .count(),
        1,
        "only the explicit share creates a second owner"
    );
    let [retained] = retain.results.as_slice() else {
        panic!("share creates exactly one independent owner")
    };
    for (text, expected) in [("owner.value", owner), ("retained.value", *retained)] {
        let payload = instruction(source, function, text, |operation| {
            matches!(operation, Operation::SharedPayloadPlace { .. })
        });
        assert_eq!(
            payload.operation,
            Operation::SharedPayloadPlace { owner: expected }
        );
        let [EntityId::Place(place)] = payload.results.as_slice() else {
            panic!("payload is a read-only place, not another owner")
        };
        let read = instruction(source, function, text, |operation| {
            matches!(operation, Operation::Read { .. })
        });
        assert_eq!(
            read.operation,
            Operation::Read {
                source: PlaceAccess::Place(*place)
            }
        );
        assert_eq!(read.block, payload.block);
    }
    let drops = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops,
        [value(*retained)],
        "only the retained owner is released here"
    );
    value(*retained)
}

#[test]
fn nullable_rc_if_uses_one_shared_proof_and_returns_the_original_owner() {
    let source = UnitSource::analyze("nullable-rc-proof.ko", NULLABLE_SOURCE)
        .expect("strict final admission and real ownership succeed");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("this ordinary source has basic capability")
    };
    let typed = typed.types();
    let condition = typed
        .null_comparison(expression(&source, "owner != null").0)
        .expect("real typed null comparison");
    assert!(condition.non_null_when_true());
    for (text, kind, effect_kind) in [
        (
            "owner.share()",
            RcOperationKind::Share,
            RcOwnershipEffectKind::Retain,
        ),
        (
            "owner.value",
            RcOperationKind::Value,
            RcOwnershipEffectKind::BorrowPayload,
        ),
    ] {
        let operation = typed.rc_operation(expression(&source, text).0).unwrap();
        assert_eq!(operation.kind(), kind);
        let proof = typed.non_null_use(operation.receiver()).unwrap();
        assert_eq!(proof.symbol(), condition.symbol());
        assert_eq!(proof.declared_type(), condition.nullable_type());
        let effects = owned
            .ownership()
            .rc_effects()
            .iter()
            .filter(|effect| effect.expression() == operation.expression())
            .collect::<Vec<_>>();
        let [effect] = effects.as_slice() else {
            panic!("one real Rc ownership effect per source operation")
        };
        assert_eq!(effect.receiver(), operation.receiver());
        assert_eq!(effect.kind(), effect_kind);
        assert_eq!(effect.payload_type(), operation.payload_type());
    }
    let (program, entry) = source
        .lower_entry(source.declaration("inspect", SymbolKind::Function))
        .expect("nullable Rc source must lower to verified SSA, not InvalidSsa");
    verify_program(&program).expect("all ownership and proof edges verify");
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    let branches = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => Some((block.id, *owner, when_null, when_non_null, *view)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(header, owner, null, present, view)] = branches.as_slice() else {
        panic!("one nullable branch creates the actual non-null proof")
    };
    let controls = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    let [control] = controls.as_slice() else {
        panic!("one full original if expression")
    };
    let condition = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| {
            if let Expression::If { condition, .. } = node.payload() {
                Some(
                    source
                        .parsed
                        .ast()
                        .expressions()
                        .get(*condition)
                        .unwrap()
                        .span(),
                )
            } else {
                None
            }
        });
    let actual_span = function
        .block(*header)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .origin
        .span();
    assert!(actual_span == *control || Some(actual_span) == condition);
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let [input_owner] = entry_block.parameters.as_slice() else {
        panic!("the original function has exactly one owner parameter")
    };
    assert_eq!(EntityId::Value(*owner), *input_owner);
    let owner_type = function
        .entity(EntityId::Value(*owner))
        .unwrap()
        .ty
        .semantic_type();
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(owner_type) else {
        panic!("source owner keeps its nullable storage identity")
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::SharedOwner { .. })
    ));
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *inner
        }
    );
    assert_eq!(
        function.block(present.target).unwrap().parameters.last(),
        Some(&EntityId::Loan(*view))
    );
    let predecessors = function
        .blocks
        .iter()
        .filter(|block| {
            block
                .terminator
                .as_ref()
                .unwrap()
                .kind
                .targets()
                .contains(&present.target)
        })
        .map(|block| block.id)
        .collect::<Vec<_>>();
    assert_eq!(
        predecessors,
        [*header],
        "no ordinary edge can mint this proof"
    );
    let retained = assert_operations(&source, function, EntityId::Loan(*view));
    assert_eq!(
        function.entity(EntityId::Value(retained)).unwrap().ty,
        EntityType::Value(*inner)
    );
    let present_block = function.block(present.target).unwrap();
    let ends = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)
        })
        .collect::<Vec<_>>();
    let [end] = ends.as_slice() else {
        panic!("the proof ends exactly once")
    };
    assert_eq!(end.block, present.target);
    let payload = instruction(&source, function, "owner.value", |operation| {
        matches!(operation, Operation::SharedPayloadPlace { .. })
    });
    assert_eq!(payload.block, present.target);
    let position = |id| {
        present_block
            .instructions
            .iter()
            .position(|actual| *actual == id)
            .unwrap()
    };
    assert!(position(payload.id) < position(end.id));
    let null_owner = forwarded(function, null, *owner);
    let present_owner = forwarded(function, present, *owner);
    let TerminatorKind::Branch(null_join) = &function
        .block(null.target)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("null branch joins normally")
    };
    let TerminatorKind::Branch(present_join) = &present_block.terminator.as_ref().unwrap().kind
    else {
        panic!("non-null branch retires its proof and joins normally")
    };
    assert_eq!(null_join.target, present_join.target);
    let joined = forwarded(function, null_join, null_owner);
    assert_eq!(forwarded(function, present_join, present_owner), joined);
    let returns = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Return { values } => Some(values.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        returns,
        [&[joined][..]],
        "the original owner remains live after the join"
    );
    assert!(
        !function
            .instructions
            .iter()
            .any(|instruction| { matches!(instruction.operation, Operation::NullableTake { .. }) })
    );
}

#[test]
fn non_nullable_rc_keeps_value_operands_without_a_nullable_proof() {
    let source = UnitSource::analyze("plain-rc-neighbor.ko", PLAIN_SOURCE)
        .expect("ordinary non-null Rc source passes strict admission");
    source.assert_frontend_clean();
    let (program, entry) = source
        .lower_entry(source.declaration("inspect", SymbolKind::Function))
        .expect("existing non-null Rc lowering remains accepted");
    verify_program(&program).expect("ordinary Rc ownership verifies");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let [owner] = entry_block.parameters.as_slice() else {
        panic!("the ordinary function has exactly one owner parameter")
    };
    assert_operations(&source, function, *owner);
    assert!(!function.blocks.iter().any(|block| matches!(
        block.terminator.as_ref().unwrap().kind,
        TerminatorKind::NullableBranch { .. }
    )));
    assert!(
        !function
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
    );
    let returns = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Return { values } => Some(values.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(returns, [&[value(*owner)][..]]);
}

#[test]
fn nullable_rc_null_and_present_paths_run_and_keep_the_returned_owner_live() {
    let run = emit_link_and_run(
        "nullable-rc-normal.ko",
        r#"fun inspect(own owner: Rc<Int>?): Rc<Int>? {
    var copied = 0
    var retainedCopy = 0
    if (owner != null) {
        val retained = owner.share()
        copied = owner.value
        retainedCopy = retained.value
    }
    if (copied == 41) { println("owner") }
    if (retainedCopy == 41) { println("retained") }
    return owner
}
fun entry(): Unit {
    val present: Rc<Int>? = Rc(41)
    val again = inspect(present)
    val finalPresent = inspect(again)
    println("present-done")
    val absent: Rc<Int>? = null
    val stillAbsent = inspect(absent)
    val finalAbsent = inspect(stillAbsent)
    println("null-done")
}"#,
        "entry",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"owner\nretained\nowner\nretained\npresent-done\nnull-done\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}
}
