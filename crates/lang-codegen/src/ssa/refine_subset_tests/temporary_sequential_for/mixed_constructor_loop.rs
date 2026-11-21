//! Mixed structural termination preserves the continuing constructor branch.
use super::TERMINATING_CONSTRUCTOR_PREFIX;
use crate::ssa::{
    model::{Operation, TerminatorKind},
    unit_lower_test_support::{analyze, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    parser::{Expression, Statement},
    source::SourceMap,
    type_checking::{UnitExpressionId, UnitStatementId, standard_environments},
};
use std::process::Command;

#[test]
fn mixed_structural_loop_constructor_only_allocates_on_continuing_arm() {
    assert_eq!(
        TERMINATING_CONSTRUCTOR_PREFIX
            .matches("while (true) { return }")
            .count(),
        2
    );
    let original = TERMINATING_CONSTRUCTOR_PREFIX
        .replacen("while (true) { return }", "loop { return }", 1)
        .replacen("while (true) { return }", "loop { break }", 1)
        .replace(
            "            })\n        }",
            "            })\n            println(\"constructed\")\n        }",
        );
    assert_eq!(original.matches("println(\"constructed\")").count(), 1);
    let text = format!("{original}\nfun main(): Unit {{ f(true); f(false) }}");

    let mut sources = SourceMap::new();
    let (id, file) = parsed(&mut sources, "structurally-terminated.ko", &text);
    let (env, types) = standard_environments();
    let mut routed_modes = Vec::new();
    for input in [
        SourceUnitInput::new("root", "structurally-terminated.ko", id, &file),
        SourceUnitInput::memory_document(id, &file),
    ] {
        let inputs = [input];
        let (names, typed, owned) = analyze(&sources, &inputs, &env, &types);
        let [source] = names.names().index().source_units() else {
            panic!("single checked physical source");
        };
        let unit = source.id();
        let facts = typed.types();
        let loops: Vec<_> = file
            .ast()
            .statements()
            .iter()
            .filter_map(|(stmt, node)| {
                matches!(node.payload(), Statement::Loop { .. })
                    .then_some(UnitStatementId::new(unit, stmt))
            })
            .collect();
        assert_eq!(loops.len(), 2);
        let completion: Vec<_> = loops
            .iter()
            .map(|&stmt| facts.loop_falls_through(stmt))
            .collect();
        assert_eq!(completion, [Some(false), Some(true)]);
        let constructors = file
            .ast()
            .expressions()
            .iter()
            .filter_map(|(expr, node)| {
                let expression = UnitExpressionId::new(unit, expr);
                facts
                    .construction(expression)
                    .map(|descriptor| (expression, node, descriptor))
            })
            .collect::<Vec<_>>();
        let [(constructor, node, descriptor)] = constructors.as_slice() else {
            panic!("one typed nominal constructor, got {constructors:?}");
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            panic!("real source call shape");
        };
        let [prefix, last] = arguments.as_slice() else {
            panic!("two source operands")
        };
        let prefix = UnitExpressionId::new(unit, prefix.value);
        let last = UnitExpressionId::new(unit, last.value);
        assert_eq!(facts.expression_falls_through(prefix), Some(true));
        assert_eq!(facts.expression_falls_through(last), Some(true));
        assert_eq!(facts.expression_falls_through(*constructor), Some(true));
        let plans: Vec<_> = owned
            .ownership()
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == *constructor)
            .collect();
        let [plan] = plans.as_slice() else {
            panic!("one sealed constructor plan")
        };
        assert_eq!(plan.target(), descriptor.target());
        assert!(plan.terminating_operand().is_none());
        assert!(plan.root_obligation().is_some());
        assert_eq!(plan.deliveries().len(), 2);
        assert_eq!(plan.deliveries()[0].argument(), prefix);
        assert_eq!(plan.deliveries()[1].argument(), last);
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "main")
            .unwrap()
            .id();
        let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        });
        let (program, entry_function) =
            lowered.unwrap_or_else(|error| panic!("routes={routes:?}; {error:?}"));
        let [module] = program.modules.as_slice() else {
            panic!("one module")
        };
        let callable = module
            .functions
            .iter()
            .find(|f| f.name.contains(".f.d"))
            .unwrap();
        let prefix_function = module
            .functions
            .iter()
            .find(|f| f.name.contains(".firstArgument.d"))
            .unwrap();
        // Observe, but do not assume, the route until real Rust/codegen evaluation.
        let refines = routes.contains(&callable.name);
        eprintln!("mixed constructor actual Refine={refines}; routes={routes:?}");
        routed_modes.push(refines);
        assert_eq!(callable.instructions.iter().filter(|op| matches!(op.operation, Operation::DirectCall { callee, .. } if callee==prefix_function.id)).count(),1);
        let mut allocations = callable
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }));
        let allocation = allocations
            .next()
            .expect("continuing operand constructs the owner");
        assert!(allocations.next().is_none());
        let [crate::ssa::model::EntityId::Value(owner)] = allocation.results.as_slice() else {
            panic!("actual constructor owner");
        };
        let mut drops = callable.instructions.iter().filter(|i| {
            matches!(
                i.operation, Operation::Drop { owner: actual } if actual == *owner
            )
        });
        let drop = drops.next().expect("constructed owner is dropped");
        assert!(drops.next().is_none());
        assert_eq!(drop.block, allocation.block);
        let block = callable.block(allocation.block).unwrap();
        let position = |id| {
            block
                .instructions
                .iter()
                .position(|actual| *actual == id)
                .unwrap()
        };
        assert!(position(allocation.id) < position(drop.id));

        let first_call = callable.instructions.iter().find(|i| matches!(
            i.operation, Operation::DirectCall { callee, .. } if callee == prefix_function.id
        )).unwrap();
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &callable
            .block(first_call.block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!("completed prefix precedes the conditional");
        };
        assert!(!reaches(callable, when_true.target, allocation.block));
        assert!(reaches(callable, when_false.target, allocation.block));

        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry_function).unwrap();
        let directory = crate::native_tests::TestDirectory::create();
        let object = directory.join("program.o");
        let program = directory.join("program");
        crate::emit_native_unit_object(
            &sources, &inputs, &names, &types, &typed, &owned, entry, &object,
        )
        .unwrap();
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&program).output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"first argument\nfirst argument\nconstructed\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
    assert_eq!(
        routed_modes[0], routed_modes[1],
        "rooted and memory routes agree"
    );
}

fn reaches(
    function: &crate::ssa::model::Function,
    start: crate::ssa::model::BlockId,
    end: crate::ssa::model::BlockId,
) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    let mut pending = vec![start];
    while let Some(block) = pending.pop() {
        if block == end {
            return true;
        }
        if seen.insert(block) {
            pending.extend(
                function
                    .block(block)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .kind
                    .targets(),
            );
        }
    }
    false
}
