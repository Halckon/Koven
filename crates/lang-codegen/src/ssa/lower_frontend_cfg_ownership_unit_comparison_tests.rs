//! E2 Stage A: five unchanged sources, twenty independently selected Unit roots.
//! The paired legacy tests remain live; this is not whole-file or native equivalence.
//! Admission, ownership, SSA and LLVM failures are hard failures, never skips.

use std::collections::BTreeSet;

use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use crate::{
    llvm::render_verified_program,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{
        model::{
            BlockId, Edge, EntityId, Function, FunctionId, Operation, Origin, Program,
            ScalarConstant, TerminatorKind,
        },
        render::render_program,
        verify::verify_program,
    },
};

struct Root {
    program: Program,
    entry: FunctionId,
    body: String,
}

impl Root {
    fn function(&self) -> &Function {
        self.program
            .module(self.entry.module())
            .unwrap()
            .function(self.entry)
            .unwrap()
    }
}

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict unchanged Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

fn lower_roots(unit: &UnitSource, names: &[&str]) -> Vec<Root> {
    let index = unit.names().names().index();
    assert_eq!(
        index
            .declarations()
            .iter()
            .filter(|item| item.kind() == SymbolKind::Function)
            .count(),
        names.len(),
        "every original function, including helpers and diverging roots, must be selected"
    );
    names
        .iter()
        .map(|&name| {
            let declaration = unit.declaration(name, SymbolKind::Function);
            let origin = Origin::Source(unit.declaration_span(declaration));
            let (program, entry) = unit
                .lower_entry(declaration)
                .unwrap_or_else(|error| panic!("original {name} root failed: {error:?}"));
            verify_program(&program).expect("complete original-root SSA and ownership verifier");
            render_verified_program(&program).expect("complete original-root LLVM verifier");
            let module = program.module(entry.module()).expect("actual root module");
            let function = module.function(entry).expect("actual root function");
            assert_eq!(function.origin, origin);
            assert_eq!(
                function.name,
                format!("koven.{name}.d{}", declaration.index())
            );
            assert_eq!(
                module
                    .functions
                    .iter()
                    .filter(|item| item.origin == origin)
                    .count(),
                1
            );
            assert!(!function.blocks.is_empty());
            let rendered = render_program(&program);
            let marker = format!("  func {:?}(", function.name);
            assert_eq!(rendered.matches(&marker).count(), 1);
            let start = rendered.find(&marker).unwrap();
            let end = start + rendered[start..].find("\n  }").expect("root body end") + 4;
            // Preserve old renderer oracles without counting reachable helper bodies twice.
            // Only the source root header is renamed; block/entity/callee text is untouched.
            let body = rendered[start..end].replacen(&marker, &format!("  func {name:?}("), 1);
            Root {
                program,
                entry,
                body,
            }
        })
        .collect()
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
        panic!("expected a real CFG branch");
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
        panic!("expected a real conditional");
    };
    (when_true, when_false)
}

fn forwarded(function: &Function, edge: &Edge, owner: EntityId) -> EntityId {
    let positions = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(slot, actual)| (*actual == owner).then_some(slot))
        .collect::<Vec<_>>();
    let [slot] = positions.as_slice() else {
        panic!("this exact owner must cross the edge once");
    };
    function.block(edge.target).unwrap().parameters[*slot]
}

fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
    match kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        _ => vec![],
    }
}

fn returned(function: &Function) -> Vec<EntityId> {
    function
        .blocks
        .iter()
        .flat_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Return { values } => {
                values.iter().copied().map(EntityId::Value).collect()
            }
            _ => vec![],
        })
        .collect()
}

fn constant(function: &Function, block: BlockId, expected: ScalarConstant) -> EntityId {
    let values = function
        .block(block)
        .unwrap()
        .instructions
        .iter()
        .filter_map(|id| {
            let instruction = function.instruction(*id).unwrap();
            if instruction.operation == Operation::Constant(expected.clone()) {
                Some(instruction.results[0])
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        panic!("one expected constant in the actual block");
    };
    *value
}

#[test]
fn lowers_if_short_circuit_and_branch_local_updates_as_cfg_unit_comparison() {
    let unit = analyze(
        "fun rhs(): Boolean = true\n\
         fun choose(flag: Boolean, left: Int, right: Int): Int =\n\
             if (flag) { left } else { right }\n\
         fun conjunction(left: Boolean): Boolean = left && rhs()\n\
         fun disjunction(left: Boolean): Boolean = left || rhs()\n\
         fun update(flag: Boolean, input: Int): Int {\n\
             var total: Int = input\n\
             if (flag) { total = total + 1 } else { total = total + 2 }\n\
             return total\n\
         }\n\
         fun same(flag: Boolean, input: Int, replacement: Int): Int {\n\
             var total: Int = input\n\
             if (flag) { total = replacement } else { total = replacement }\n\
             return total\n\
         }\n\
         fun discard(flag: Boolean): Unit {\n\
             if (flag) { 1 }\n\
             when { flag -> 2 }\n\
         }",
    );
    let roots = lower_roots(
        &unit,
        &[
            "rhs",
            "choose",
            "conjunction",
            "disjunction",
            "update",
            "same",
            "discard",
        ],
    );
    let rendered = roots
        .iter()
        .map(|root| root.body.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    for name in [
        "choose",
        "conjunction",
        "disjunction",
        "update",
        "same",
        "discard",
    ] {
        assert!(rendered.contains(&format!("func {name:?}")));
    }
    assert!(rendered.matches("cond %v").count() >= 4);
    assert!(rendered.matches("branch bb").count() >= 8);
    assert!(rendered.contains("bb3(%v"));
    assert_eq!(roots[5].body.matches("return %v").count(), 1);
    for (root, rhs_when_true, short_value) in [(&roots[2], true, false), (&roots[3], false, true)] {
        assert_short_circuit(&unit, root, rhs_when_true, short_value);
    }
    let choose = roots[1].function();
    let (yes, no) = conditional(choose, choose.entry_block().unwrap());
    let yes_merge = branch(choose, yes.target);
    let no_merge = branch(choose, no.target);
    assert_eq!(yes_merge.target, no_merge.target);
    assert_ne!(yes_merge.arguments[0], no_merge.arguments[0]);
    assert_eq!(
        returned(choose),
        [choose.block(yes_merge.target).unwrap().parameters[0]]
    );
}

fn assert_short_circuit(unit: &UnitSource, root: &Root, rhs_when_true: bool, short_value: bool) {
    let function = root.function();
    let module = root.program.module(root.entry.module()).unwrap();
    let rhs_origin =
        Origin::Source(unit.declaration_span(unit.declaration("rhs", SymbolKind::Function)));
    let calls = function
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => Some((instruction, *callee, receiver, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(call, callee, receiver, arguments)] = calls.as_slice() else {
        panic!("one RHS call");
    };
    assert_eq!(module.function(*callee).unwrap().origin, rhs_origin);
    assert!(receiver.is_none());
    assert!(arguments.is_empty());
    assert_eq!(unit.sources().slice(call.origin.span()), Ok("rhs()"));
    // The legacy @f0 oracle is now tied to the actual rhs declaration, not a guessed ID.
    let call_text = format!("call @f{}()", callee.index());
    assert!(root.body.contains(&call_text));
    assert!(root.body.find("cond %v").unwrap() < root.body.find(&call_text).unwrap());
    let (yes, no) = conditional(function, function.entry_block().unwrap());
    let (rhs, short) = if rhs_when_true { (yes, no) } else { (no, yes) };
    assert_eq!(rhs.target, call.block);
    assert_ne!(short.target, call.block);
    let short_result = constant(function, short.target, ScalarConstant::Boolean(short_value));
    let rhs_merge = branch(function, rhs.target);
    let short_merge = branch(function, short.target);
    assert_eq!(rhs_merge.target, short_merge.target);
    assert_eq!(rhs_merge.arguments.first(), call.results.first());
    assert_eq!(short_merge.arguments.first(), Some(&short_result));
    assert_eq!(
        returned(function),
        [function.block(rhs_merge.target).unwrap().parameters[0]]
    );
}

#[test]
fn carries_move_only_string_bindings_across_if_and_short_circuit_edges_unit_comparison() {
    let unit = analyze(
        "fun keep(own joined: String, flag: Boolean): String {\n\
             if (flag) { println(joined) } else { println(joined) }\n\
             return joined\n\
         }\n\
         fun compare(own joined: String, flag: Boolean): String {\n\
             val matches: Boolean = flag && joined == \"hello\"\n\
             if (matches) { println(joined) }\n\
             return joined\n\
         }",
    );
    let roots = lower_roots(&unit, &["keep", "compare"]);
    for root in &roots {
        assert!(root.body.matches("bb").count() >= 4, "{}", root.body);
        assert!(root.body.contains("branch bb"));
        assert!(root.body.contains("return %v"));
        assert_owner_lineage(root.function());
    }
}

fn assert_owner_lineage(function: &Function) {
    let owner = function.blocks[0].parameters[0];
    assert!(
        matches!(owner, EntityId::Value(_)),
        "original own String parameter"
    );
    let mut lineage = BTreeSet::from([owner]);
    loop {
        let before = lineage.len();
        for block in &function.blocks {
            for edge in edges(&block.terminator.as_ref().unwrap().kind) {
                for (argument, parameter) in edge
                    .arguments
                    .iter()
                    .zip(&function.block(edge.target).unwrap().parameters)
                {
                    if lineage.contains(argument) {
                        lineage.insert(*parameter);
                    }
                }
            }
        }
        if lineage.len() == before {
            break;
        }
    }
    assert!(
        lineage.len() >= 4,
        "owner must be rebound, not merely mentioned"
    );
    for block in &function.blocks {
        let live = block
            .parameters
            .iter()
            .filter(|entity| lineage.contains(*entity))
            .copied()
            .collect::<Vec<_>>();
        let [owner] = live.as_slice() else {
            panic!("exactly one live joined owner per block");
        };
        for edge in edges(&block.terminator.as_ref().unwrap().kind) {
            assert!(lineage.contains(&forwarded(function, edge, *owner)));
        }
    }
    let result = returned(function);
    assert_eq!(result.len(), 1);
    assert!(lineage.contains(&result[0]));
    for instruction in &function.instructions {
        if let Operation::Drop { owner } = instruction.operation {
            assert!(
                !lineage.contains(&EntityId::Value(owner)),
                "returned owner must never be dropped"
            );
        }
    }
}

#[test]
fn comma_when_drops_owner_only_on_the_match_that_skips_consumption_unit_comparison() {
    let unit = analyze(
        "class Node {}\nfun probe(own node: Node): Boolean = false\nfun select(flag: Boolean, own node: Node): Int = when { flag, probe(node) -> 0; else -> 1 }",
    );
    let roots = lower_roots(&unit, &["probe", "select"]);
    let rendered = roots
        .iter()
        .map(|root| root.body.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    assert_eq!(rendered.matches("drop ").count(), 2, "{rendered}");
    let root = &roots[1];
    let function = root.function();
    let entry = &function.blocks[0];
    let owner = entry.parameters[1];
    let (matched, fallthrough) = conditional(function, entry.id);
    let matched_owner = forwarded(function, matched, owner);
    let consumed_owner = forwarded(function, fallthrough, owner);
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one select cleanup, only on the skipped-consuming alternative");
    };
    assert_eq!(drop.block, matched.target);
    let EntityId::Value(matched_owner) = matched_owner else {
        panic!("owned Node value");
    };
    assert_eq!(
        drop.operation,
        Operation::Drop {
            owner: matched_owner
        }
    );
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one probe call");
    };
    assert_eq!(call.block, fallthrough.target);
    assert_eq!(unit.sources().slice(call.origin.span()), Ok("probe(node)"));
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &call.operation
    else {
        unreachable!()
    };
    assert!(receiver.is_none());
    assert_eq!(arguments, &[consumed_owner]);
    let callee = root
        .program
        .module(root.entry.module())
        .unwrap()
        .function(*callee)
        .unwrap();
    assert_eq!(
        callee.origin,
        Origin::Source(unit.declaration_span(unit.declaration("probe", SymbolKind::Function)))
    );
    let [EntityId::Value(callee_owner)] = callee.blocks[0].parameters.as_slice() else {
        panic!("probe receives the transferred owner");
    };
    let callee_drops = callee
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(callee_drops, [*callee_owner]);
    let (second_match, otherwise) = conditional(function, fallthrough.target);
    let TerminatorKind::Conditional { condition, .. } = &function
        .block(fallthrough.target)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        unreachable!()
    };
    assert_eq!(call.results, [EntityId::Value(*condition)]);
    let first_body = branch(function, matched.target);
    let second_body = branch(function, second_match.target);
    assert_eq!(first_body.target, second_body.target);
    assert!(
        !first_body
            .arguments
            .contains(&EntityId::Value(matched_owner))
    );
    assert!(!second_match.arguments.contains(&consumed_owner));
    assert!(!otherwise.arguments.contains(&consumed_owner));
    assert_eq!(
        branch(function, first_body.target).target,
        branch(function, otherwise.target).target
    );
}

#[test]
fn lowers_boolean_when_subjectless_chains_and_diverging_entries_unit_comparison() {
    let unit = analyze(
        "fun select(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             false -> 2\n\
         }\n\
         fun predicate(left: Boolean, right: Boolean): Int = when {\n\
             left && right -> 1\n\
             left -> 2\n\
             else -> 3\n\
         }\n\
         fun grouped(left: Boolean, right: Boolean): Int = when {\n\
             left, right -> 7\n\
             else -> 8\n\
         }\n\
         fun early(flag: Boolean): Int = when (flag) {\n\
             true -> return 4\n\
             false -> 5\n\
         }\n\
         fun update(flag: Boolean, input: Int): Int {\n\
             var total: Int = input\n\
             when { flag -> total = 6 }\n\
             return total\n\
         }",
    );
    let roots = lower_roots(
        &unit,
        &["select", "predicate", "grouped", "early", "update"],
    );
    let rendered = roots
        .iter()
        .map(|root| root.body.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    for name in ["select", "predicate", "grouped", "early", "update"] {
        assert!(rendered.contains(&format!("func {name:?}")));
    }
    assert!(rendered.matches("cond %v").count() >= 6);
    assert!(rendered.matches("return %v").count() >= 4);
    assert!(rendered.contains("bb3(%v"));
    let select = roots[0].function();
    let (yes, no) = conditional(select, select.entry_block().unwrap());
    let yes_result = constant(select, yes.target, ScalarConstant::Integer(1));
    let no_result = constant(select, no.target, ScalarConstant::Integer(2));
    let yes_merge = branch(select, yes.target);
    let no_merge = branch(select, no.target);
    assert_eq!(yes_merge.target, no_merge.target);
    assert_eq!(yes_merge.arguments[0], yes_result);
    assert_eq!(no_merge.arguments[0], no_result);
    assert_eq!(
        returned(select),
        [select.block(yes_merge.target).unwrap().parameters[0]]
    );
    let grouped = roots[2].function();
    let (first_match, next) = conditional(grouped, grouped.entry_block().unwrap());
    let (second_match, otherwise) = conditional(grouped, next.target);
    let first_body = branch(grouped, first_match.target);
    let second_body = branch(grouped, second_match.target);
    assert_eq!(first_body.target, second_body.target);
    constant(grouped, first_body.target, ScalarConstant::Integer(7));
    constant(grouped, otherwise.target, ScalarConstant::Integer(8));
    let early = roots[3].function();
    let (yes, no) = conditional(early, early.entry_block().unwrap());
    let early_result = constant(early, yes.target, ScalarConstant::Integer(4));
    let normal_result = constant(early, no.target, ScalarConstant::Integer(5));
    assert_eq!(returned(early), [early_result, normal_result]);
}

#[test]
fn lowers_while_loop_break_continue_and_nested_loop_targets_unit_comparison() {
    let unit = analyze(
        "fun increment(limit: Int): Int {\n\
             var current: Int = 0\n\
             while (current < limit) { current += 1 }\n\
             return current\n\
         }\n\
         fun exits(flag: Boolean): Int {\n\
             var result: Int = 0\n\
             loop {\n\
                 if (flag) { { result = 1 } break } else { continue }\n\
             }\n\
             return result\n\
         }\n\
         fun nested(outer: Boolean, inner: Boolean): Int {\n\
             var total: Int = 0\n\
             loop {\n\
                 while (inner) { break }\n\
                 if (outer) { { total = 2 } break } else { continue }\n\
             }\n\
             return total\n\
         }\n\
         fun spin(): Unit { loop { continue } }",
    );
    let roots = lower_roots(&unit, &["increment", "exits", "nested", "spin"]);
    let rendered = roots
        .iter()
        .map(|root| root.body.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    for name in ["increment", "exits", "nested", "spin"] {
        assert!(rendered.contains(&format!("func {name:?}")));
    }
    assert!(rendered.matches("branch bb1(").count() >= 6);
    assert!(rendered.matches("bb1(%v").count() >= 3);
    assert!(!roots[3].body.contains("return"));
    for root in &roots[1..] {
        let function = root.function();
        let header = branch(function, function.entry_block().unwrap()).target;
        let continues = function
            .blocks
            .iter()
            .filter(|block| {
                unit.sources()
                    .slice(block.terminator.as_ref().unwrap().origin.span())
                    == Ok("continue")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            continues.len(),
            1,
            "one original continue in each loop fixture"
        );
        assert_eq!(
            branch(function, continues[0].id).target,
            header,
            "continue must select its nearest original loop"
        );
    }
    let nested = roots[2].function();
    let outer_header = branch(nested, nested.entry_block().unwrap()).target;
    let inner_header = branch(nested, outer_header).target;
    assert_ne!(inner_header, outer_header);
    let (inner_body, inner_false) = conditional(nested, inner_header);
    let inner_break = branch(nested, inner_body.target);
    let inner_fallthrough = branch(nested, inner_false.target);
    assert_eq!(
        inner_break.target, inner_fallthrough.target,
        "inner break reaches the inner exit"
    );
    assert_ne!(
        inner_break.target, outer_header,
        "inner break does not continue the outer loop"
    );
    let (outer_break, outer_continue) = conditional(nested, inner_break.target);
    assert_eq!(branch(nested, outer_continue.target).target, outer_header);
    let two = constant(nested, outer_break.target, ScalarConstant::Integer(2));
    assert_eq!(returned(nested), [two]);
    let exits = roots[1].function();
    let header = branch(exits, exits.entry_block().unwrap()).target;
    let (break_block, _) = conditional(exits, header);
    assert_eq!(
        returned(exits),
        [constant(
            exits,
            break_block.target,
            ScalarConstant::Integer(1)
        )]
    );
    let spin = roots[3].function();
    let header = branch(spin, spin.entry_block().unwrap()).target;
    assert_eq!(
        branch(spin, header).target,
        header,
        "spin is an actual self-loop with no return"
    );
}
