//! Independent ordinary source: current owners survive if joins and List backedges.
//! No source or helper is extracted from the CI-only atomic fault-injection whole.

use super::{OwnedProducts, UnitSource, assert_native_output};
use crate::ssa::{
    model::{
        BlockId, Edge, EntityId, Function, Instruction, Operation, Origin, TerminatorKind, ValueId,
    },
    verify::verify_program,
};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitIterationExitKind,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item},
    source::Span,
    type_checking::{
        Copyability, IterationProvider, SequentialContainerKind, UnitExpressionId, UnitStatementId,
    },
};

const SOURCE: &str = r#"class Receipt(val label: String) {
    deinit() { println(this.label) }
}
fun choose(enabled: Boolean): Unit {
    println("begin")
    var retained = Receipt("seed")
    var result = "seed-text"
    for (item in listOf(4, 7, 9)) {
        println("step")
        if (enabled) {
            result = "changed-text"
            retained = Receipt("changed")
        }
        println("body")
    }
    println(result)
    println("end")
}
fun entry(): Unit {
    choose(false)
    choose(true)
}
"#;

// Guide-derived oracle, not stdout obtained by running a candidate.
const EXPECTED: &[u8] = b"begin\nstep\nbody\nstep\nbody\nstep\nbody\nseed-text\nend\nseed\n\
begin\nstep\nseed\nbody\nstep\nchanged\nbody\nstep\nchanged\nbody\nchanged-text\nend\nchanged\n";

fn source_text(constant: bool) -> String {
    if constant {
        // A real constant declaration/use must earn its own NotBasic recovery capability.
        format!(
            "const val CHANGED_TEXT: String = \"changed-text\"\n{}",
            SOURCE.replace("result = \"changed-text\"", "result = CHANGED_TEXT")
        )
    } else {
        SOURCE.to_owned()
    }
}

fn analyze(constant: bool) -> UnitSource {
    let text = source_text(constant);
    let source = UnitSource::analyze("ordinary-owner-replacement.ko", &text)
        .expect("independent whole source passes strict final admission and real ownership");
    source.assert_frontend_clean();
    assert_eq!(
        source.sources.source_text(source.parsed.source_id()),
        Ok(text.as_str())
    );
    assert_eq!(
        matches!(&source.products, OwnedProducts::Constant(..)),
        constant
    );
    let owned = match &source.products {
        OwnedProducts::Basic(_, owned) => owned.ownership(),
        OwnedProducts::Constant(typed, owned) => {
            assert_eq!(typed.constants().declarations().len(), 1);
            assert_eq!(owned.materializations().len(), 1);
            assert!(
                owned
                    .materialization_at(expression(&source, "CHANGED_TEXT").0)
                    .is_some()
            );
            owned.ownership()
        }
    };
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    source
}

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one original MemoryDocument")
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
        panic!("one original expression {text:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn assignment_root(source: &UnitSource, target: UnitExpressionId) -> UnitSymbolId {
    let span = source
        .parsed
        .ast()
        .expressions()
        .get(target.expression())
        .unwrap()
        .span();
    let references = source
        .names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == target.source_unit() && reference.span() == span
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        panic!("one source-qualified assignment target reference")
    };
    let UnitReferenceTarget::Symbol(symbol) = reference.target() else {
        panic!("ordinary local binding, not a field or foreign declaration")
    };
    *symbol
}

fn choose_origin(source: &UnitSource) -> (Span, UnitStatementId) {
    let declaration = source.declaration("choose", SymbolKind::Function);
    let declaration = &source.names.names().index().declarations()[declaration.index()];
    let node = source.parsed.ast().items().get(declaration.root()).unwrap();
    let Item::Function {
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            },
        ..
    } = node.payload()
    else {
        panic!("the original choose function has its declared lexical body")
    };
    (
        node.span(),
        UnitStatementId::new(declaration.source_unit(), *body),
    )
}

fn assert_facts(source: &UnitSource, constant: bool) {
    let (typed, owned) = match &source.products {
        OwnedProducts::Basic(typed, owned) => (typed.types(), owned.ownership()),
        OwnedProducts::Constant(typed, owned) => (typed.types(), owned.ownership()),
    };
    let result_rhs = if constant {
        "CHANGED_TEXT"
    } else {
        "\"changed-text\""
    };
    let mut roots = Vec::new();
    for (name, rhs, resource) in [
        ("result", result_rhs, false),
        ("retained", "Receipt(\"changed\")", true),
    ] {
        let assignment = expression(source, &format!("{name} = {rhs}")).0;
        let descriptor = typed
            .assignment(assignment)
            .expect("real assignment descriptor");
        assert_eq!(descriptor.expression(), assignment);
        assert_eq!(descriptor.value(), expression(source, rhs).0);
        assert!(descriptor.falls_through());
        let root = assignment_root(source, descriptor.target());
        roots.push(root);
        assert_eq!(typed.symbol_type(root), Some(descriptor.target_type()));
        assert_eq!(
            typed.copyability(descriptor.target_type()),
            Copyability::MoveOnly
        );
        assert_eq!(
            typed.is_resource_type(descriptor.target_type()),
            Some(resource)
        );
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "one RHS replacement and one final current-owner drop: {drops:?}"
        );
        let replacement = UnitDropPoint::AfterExpression(descriptor.value());
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.point() == replacement)
                .count(),
            1
        );
        let final_point = if resource {
            UnitDropPoint::AfterStatement(choose_origin(source).1)
        } else {
            UnitDropPoint::CallReturn(expression(source, "println(result)").0)
        };
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.point() == final_point)
                .count(),
            1
        );
    }
    assert_ne!(roots[0], roots[1]);
    let [plan] = owned.iterations() else {
        panic!("one original List provider")
    };
    let descriptor = plan.descriptor();
    assert_eq!(
        typed.sequential_iteration(descriptor.statement()),
        Some(descriptor)
    );
    assert_eq!(
        descriptor.provider(),
        IterationProvider::Sequential(SequentialContainerKind::List)
    );
    assert_eq!(descriptor.source(), expression(source, "listOf(4, 7, 9)").0);
    assert_eq!(plan.source_access(), UnitIterationSourceAccess::Temporary);
    assert_eq!(
        plan.source(),
        Some(&UnitLoanTarget::Temporary(descriptor.source()))
    );
    let bindings = descriptor.binding().symbols().collect::<Vec<_>>();
    let [item] = bindings.as_slice() else {
        panic!("one original element binding")
    };
    assert_eq!(plan.exits().len(), 2);
    let normal = plan
        .exits()
        .iter()
        .find(|exit| exit.kind() == UnitIterationExitKind::Fallthrough)
        .unwrap();
    assert_eq!(
        normal.actions(),
        &[
            UnitIterationCleanupAction::EndBinding {
                statement: descriptor.statement(),
                symbol: *item,
            },
            UnitIterationCleanupAction::EndElement(descriptor.statement()),
        ]
    );
    let exhausted = plan
        .exits()
        .iter()
        .find(|exit| exit.kind() == UnitIterationExitKind::Exhaustion)
        .unwrap();
    assert_eq!(
        exhausted.point(),
        UnitDropPoint::LoopExit(descriptor.statement())
    );
    let [
        UnitIterationCleanupAction::FinishProvider(finish),
        UnitIterationCleanupAction::EndSource(end),
        UnitIterationCleanupAction::Drop(hidden),
    ] = exhausted.actions()
    else {
        panic!("complete ordered exhaustion cleanup, without an outer-owner drop")
    };
    assert_eq!(*finish, descriptor.statement());
    assert_eq!(*end, descriptor.statement());
    assert_eq!(hidden.point(), exhausted.point());
    assert_eq!(
        hidden.target(),
        UnitDropTarget::Temporary(descriptor.source())
    );
    assert_eq!(
        hidden.value_origin(),
        expression(source, "listOf(4, 7, 9)").1
    );
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("real owned SSA value")
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
        panic!("this exact current owner crosses once")
    };
    value(function.block(edge.target).unwrap().parameters[*column])
}

fn branch(function: &Function, block: BlockId) -> &Edge {
    let TerminatorKind::Branch(edge) = &function
        .block(block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("actual unconditional transport edge")
    };
    edge
}

fn conditional(function: &Function, block: BlockId) -> (&Edge, &Edge) {
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &function
        .block(block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("actual source/provider conditional")
    };
    (when_true, when_false)
}

fn formed_at(function: &Function, span: Span, resource: bool) -> &Instruction {
    let matches = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span)
                && if resource {
                    matches!(instruction.operation, Operation::HeapAllocate { .. })
                } else {
                    matches!(instruction.operation, Operation::StringLiteral { .. })
                }
        })
        .collect::<Vec<_>>();
    let [instruction] = matches.as_slice() else {
        panic!("one real complete owner formation")
    };
    assert_eq!(instruction.results.len(), 1);
    instruction
}

fn drop_of(function: &Function, owner: ValueId) -> &Instruction {
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| instruction.operation == (Operation::Drop { owner }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one drop of this exact current SSA owner")
    };
    drop
}

fn print_at(function: &Function, span: Span) -> &Instruction {
    let prints = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span)
                && matches!(instruction.operation, Operation::PrintString { .. })
        })
        .collect::<Vec<_>>();
    let [print] = prints.as_slice() else {
        panic!("one original print operation")
    };
    print
}

fn before(function: &Function, first: &Instruction, second: &Instruction) {
    assert_eq!(first.block, second.block);
    let order = &function.block(first.block).unwrap().instructions;
    assert!(
        order.iter().position(|id| *id == first.id).unwrap()
            < order.iter().position(|id| *id == second.id).unwrap()
    );
}

fn assert_owner_transport(source: &UnitSource, function: &Function, constant: bool) {
    let replacements = if constant {
        "CHANGED_TEXT"
    } else {
        "\"changed-text\""
    };
    let controls = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::If { .. }))
        .collect::<Vec<_>>();
    let [(_, control)] = controls.as_slice() else {
        panic!("one original if")
    };
    let mut exit_owners = Vec::new();
    for (initial, replacement, resource) in [
        ("\"seed-text\"", replacements, false),
        ("Receipt(\"seed\")", "Receipt(\"changed\")", true),
    ] {
        let initial = formed_at(function, expression(source, initial).1, resource);
        let replacement = formed_at(function, expression(source, replacement).1, resource);
        let into_header = branch(function, initial.block);
        let header_owner = forwarded(function, into_header, value(initial.results[0]));
        let (into_body, into_exit) = conditional(function, into_header.target);
        let body_owner = forwarded(function, into_body, header_owner);
        let (into_true, into_false) = conditional(function, into_body.target);
        assert_eq!(
            function
                .block(into_body.target)
                .unwrap()
                .terminator
                .as_ref()
                .unwrap()
                .origin,
            Origin::Source(control.span())
        );
        let replaced_owner = forwarded(function, into_true, body_owner);
        let unchanged_owner = forwarded(function, into_false, body_owner);
        let replacement_drop = drop_of(function, replaced_owner);
        assert_eq!(replacement.block, into_true.target);
        before(function, replacement, replacement_drop);
        let true_to_join = branch(function, into_true.target);
        let false_to_join = branch(function, into_false.target);
        assert_eq!(true_to_join.target, false_to_join.target);
        let joined = forwarded(function, true_to_join, value(replacement.results[0]));
        assert_eq!(joined, forwarded(function, false_to_join, unchanged_owner));
        let backedge = branch(function, true_to_join.target);
        assert_eq!(backedge.target, into_header.target);
        assert_eq!(forwarded(function, backedge, joined), header_owner);
        let final_owner = forwarded(function, into_exit, header_owner);
        exit_owners.push(final_owner);
        assert_ne!(final_owner, value(initial.results[0]));
        let final_drop = drop_of(function, final_owner);
        assert_eq!(final_drop.block, into_exit.target);
        let print = if resource {
            "println(\"end\")"
        } else {
            "println(result)"
        };
        before(
            function,
            print_at(function, expression(source, print).1),
            final_drop,
        );
        assert!(
            !function.instructions.iter().any(|instruction| {
                instruction.operation
                    == (Operation::Drop {
                        owner: value(initial.results[0]),
                    })
                    || instruction.operation == (Operation::Drop { owner: joined })
            }),
            "initial and join owners transfer, never become stale drop operands"
        );
    }
    assert_ne!(exit_owners[0], exit_owners[1]);
}

#[test]
fn independent_replacement_facts_ssa_and_verified_llvm_preserve_current_owners() {
    for constant in [false, true] {
        let source = analyze(constant);
        assert_facts(&source, constant);
        let (program, entry) = source
            .lower_entry(source.declaration("entry", SymbolKind::Function))
            .expect("actual basic or constant Unit products lower");
        verify_program(&program).expect("complete source-derived SSA verifies");
        let functions = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| function.origin.span() == choose_origin(&source).0)
            .collect::<Vec<_>>();
        let [choose] = functions.as_slice() else {
            panic!("one original choose definition")
        };
        assert_owner_transport(&source, choose, constant);
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("unmodified source-derived LLVM verifies");
        assert!(llvm.contains("define i32 @main("), "{llvm}");
    }
}

#[test]
fn independent_replacement_native_preserves_lexical_resource_and_final_string() {
    for constant in [false, true] {
        let source = analyze(constant);
        assert_native_output(&source, EXPECTED);
    }
}
