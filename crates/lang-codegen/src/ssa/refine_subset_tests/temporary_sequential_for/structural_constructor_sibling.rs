//! Separate control with structural termination, not an edit to the literal-while baseline.
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
fn structurally_terminated_constructor_prefix_checks_source_route_and_native() {
    assert_eq!(
        TERMINATING_CONSTRUCTOR_PREFIX
            .matches("while (true) { return }")
            .count(),
        2
    );
    let original =
        TERMINATING_CONSTRUCTOR_PREFIX.replace("while (true) { return }", "loop { return }");
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
        assert!(
            loops
                .iter()
                .all(|&stmt| facts.loop_falls_through(stmt) == Some(false))
        );
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
        assert_eq!(facts.expression_falls_through(last), Some(false));
        assert_eq!(facts.expression_falls_through(*constructor), Some(false));
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
        assert_eq!(plan.terminating_operand(), Some(last));
        assert!(plan.root_obligation().is_none());
        let [delivery] = plan.deliveries() else {
            panic!("only the completed prefix is delivered")
        };
        assert_eq!(delivery.argument(), prefix);
        assert_eq!(delivery.evaluation_index(), 0);
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
        eprintln!("structural loop constructor actual Refine={refines}; routes={routes:?}");
        routed_modes.push(refines);
        assert_eq!(callable.instructions.iter().filter(|op| matches!(op.operation, Operation::DirectCall { callee, .. } if callee==prefix_function.id)).count(),1);
        assert!(!callable.instructions.iter().any(|op| matches!(
            op.operation,
            Operation::HeapAllocate { .. } | Operation::Drop { .. }
        )));
        assert_eq!(
            callable
                .blocks
                .iter()
                .filter_map(|b| b.terminator.as_ref())
                .filter(|t| matches!(&t.kind,TerminatorKind::Return{values} if values.is_empty()))
                .count(),
            2
        );
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
        assert_eq!(run.stdout, b"first argument\nfirst argument\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
    assert_eq!(
        routed_modes[0], routed_modes[1],
        "rooted and memory routes agree"
    );
}
