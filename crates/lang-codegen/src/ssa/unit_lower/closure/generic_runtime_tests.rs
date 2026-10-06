//! Generic helpers retain concrete Resource element and environment identities in source ABI.
use crate::ssa::{
    model::Operation,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
    unit_plan::plan_unit_instances,
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    source::SourceMap,
    type_checking::{UnitCallableTarget, standard_environments},
};

#[test]
fn unit_runtime_generic_helper_returns_resource_in_three_environments() {
    for container in ["Array", "List"] {
        for (environment, initializer) in [
            ("pointer", "{ index -> Leaf(index) }"),
            ("shared", "{ index -> Leaf(index + scale.number) }"),
            ("owned", "move { index -> Leaf(index + scale.number) }"),
        ] {
            let text = format!(
                "package test\nclass Leaf(val number: Int) {{ deinit() {{ println(\"leaf\") }} }}\nfun <T> generate(size: Int, callback: (Int)->T): {container}<T> = {container}<T>(size, callback)\nfun entry(): Int {{ val scale = Leaf(7)\nval callback: (Int)->Leaf = {initializer}\nval items = generate<Leaf>(3, callback)\nreturn items.size }}"
            );
            let mut sources = SourceMap::new();
            let (source, file) = parsed(&mut sources, "test/generic-resource.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "test/generic-resource.ko",
                source,
                &file,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let arena = typed.types().types().len();
            let entry = declaration(&names, "test", "entry");
            let helper = declaration(&names, "test", "generate");
            let plan = plan_unit_instances(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
            )
            .expect("selected concrete generic Resource route");
            let instances = plan
                .iter()
                .filter(|instance| {
                    instance.key().target() == UnitCallableTarget::Declaration(helper)
                })
                .collect::<Vec<_>>();
            assert_eq!(instances.len(), 1);
            assert_eq!(instances[0].key().type_arguments().len(), 1);
            assert_eq!(
                instances[0].key().callable_arguments().len(),
                1,
                "generic element and selected callback identity both belong to the complete key"
            );
            let (program, _) = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
            )
            .unwrap_or_else(|error| panic!("{container}/{environment}: {error:?}"));
            let module = &program.modules[0];
            assert!(
                module
                    .functions
                    .iter()
                    .any(
                        |function| function.instructions.iter().any(|instruction| matches!(
                            instruction.operation,
                            Operation::ContainerGenerateBorrowed { .. }
                        ))
                    )
            );
            let thunk = module
                .functions
                .iter()
                .find(|function| function.name.contains(".thunk"))
                .expect("selected concrete initializer body");
            assert!(thunk.instructions.iter().any(|instruction|
                matches!(instruction.operation, Operation::HeapAllocate { owner, .. }
                    if module.deinit(owner).is_some())),
                "the complete Resource callback result retains its existing hidden destructor");
            crate::llvm::render_verified_program(&program)
                .expect("generic nominal helper reaches LLVM");
            assert_eq!(typed.types().types().len(), arena);
        }
    }
}
