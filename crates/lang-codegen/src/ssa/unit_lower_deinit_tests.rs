use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry};
use crate::ssa::{
    model::{EntityType, LoanKind, Operation},
    render::render_program,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn unit_resource_deinit_body_and_its_calls_are_reachable_from_layout() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/resource.ko",
        "package p\n\
         fun observe(value: Int): Unit {}\n\
         class Resource(val value: Int) { deinit() { observe(this.value) } }\n\
         class Unused { deinit() {} }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/entry.ko",
        "package q\nimport p.Resource\nfun entry(): Unit { val resource = Resource(7) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/resource.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/entry.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("reachable resource deinit lowers");
    let module = &program.modules[0];
    assert_eq!(
        module.functions.len(),
        3,
        "entry, hidden deinit and body-only helper"
    );
    let deinit = module
        .functions
        .iter()
        .find(|function| function.name.contains("deinit"))
        .expect("hidden resource deinit function");
    assert!(matches!(
        deinit.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    let Some(EntityType::Loan { target, .. }) = deinit.receiver() else {
        unreachable!()
    };
    assert_eq!(module.deinit(target), Some(deinit.id()));
    assert!(deinit.return_types.is_empty());
    assert!(
        deinit
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
    );
    let (reordered, _) = lower_scalar_unit_with_entry(
        &sources,
        &[inputs[1], inputs[0]],
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input order cannot alter hidden deinit identities");
    assert_eq!(render_program(&program), render_program(&reordered));
}

#[test]
fn constant_unit_resource_deinit_body_reuses_constant_materialization() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         const val VALUE: Int = 7\n\
         fun observe(value: Int): Unit {}\n\
         class Resource { deinit() { observe(VALUE) } }\n\
         fun entry(): Unit { val resource = Resource() }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    let (program, _) = lower_constant_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("constant-enabled resource deinit lowers");
    assert_eq!(
        program.modules[0].functions.len(),
        3,
        "constant entry also reaches deinit helper"
    );
}

#[test]
fn unit_resource_deinit_reachability_reaches_a_fixed_point() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun observe(value: Int): Unit {}\n\
         class Inner { deinit() { observe(1) } }\n\
         class Outer { deinit() { val inner = Inner() } }\n\
         fun entry(): Unit { val outer = Outer() }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("resource created only inside deinit is planned transitively");
    assert_eq!(
        program.modules[0].functions.len(),
        4,
        "entry, two deinits, body-only helper"
    );
}

#[test]
fn unit_resource_deinit_nullable_class_reaches_hidden_body_and_keeps_conditional_owner() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun observe(value: Int): Unit {}\n\
         class Resource { deinit() { observe(1) } }\n\
         fun entry(): Unit { val absent: Resource? = null; val present: Resource? = Resource() }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let arena = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("nullable class resource uses the existing conditional inner drop ABI");
    assert_eq!(typed.types().types().len(), arena);
    let module = &program.modules[0];
    assert_eq!(
        module.functions.len(),
        3,
        "entry, hidden deinit, deinit-only helper"
    );
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.entry"))
        .unwrap();
    for has in [
        entry
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::NullableNull { .. })),
        entry
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::NullableWrap { .. })),
    ] {
        assert!(
            has,
            "both null and wrapped owners retain distinct nullable operations"
        );
    }
    crate::llvm::render_verified_program(&program)
        .expect("conditional nullable deinit verifies through LLVM");
}

#[test]
fn unit_resource_deinit_nullable_unsupported_wrappers_keep_null_rejection_span() {
    for wrapper in ["Payload", "Rc<Payload>", "Box<Payload>"] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/main.ko",
            &format!(
                "package p\n\
                 class Resource {{ deinit() {{}} }}\n\
                 value class Payload(val resource: Resource)\n\
                 fun entry(): Unit {{ val absent: {wrapper}? = null }}"
            ),
        );
        let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let null_span = parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| sources.slice(node.span()) == Ok("null"))
            .unwrap()
            .1
            .span();
        let arena = typed.types().types().len();
        let error = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .err()
        .expect("only existing nullable Class handles gain hidden-deinit reachability");
        assert_eq!(
            error.kind,
            crate::ssa::LoweringErrorKind::UnsupportedNode,
            "{wrapper}"
        );
        assert_eq!(error.span, Some(null_span), "{wrapper}");
        assert_eq!(typed.types().types().len(), arena);
    }
}

#[test]
fn unit_resource_deinit_rejects_unimplemented_resource_shapes() {
    for text in [
        "class Resource<T>(val value: T) { deinit() {} }\nfun entry(): Unit { val resource = Resource<Int>(7) }",
        "interface Marker {}\nclass Resource: Marker { deinit() {} }\nfun entry(): Unit { val resource = Resource() }",
        "class Resource { deinit() {} }\nclass Wrapper<T>(val value: T)\nfun entry(): Unit { val wrapper = Wrapper<Resource>(Resource()) }",
        "class Resource { deinit() { val action = { 1 } } }\nfun entry(): Unit { val resource = Resource() }",
        "class Resource(val value: Int) { deinit() {} }\nfun entry(): Unit { val resource = Resource(7); val action: move () -> Int = move { resource.value } }",
    ] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(&mut sources, "p/main.ko", &format!("package p\n{text}"));
        let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let error = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .err()
        .expect("unimplemented resource representation must fail before executable SSA");
        assert_eq!(
            error.kind,
            crate::ssa::LoweringErrorKind::UnsupportedNode,
            "{text}"
        );
        assert!(
            error.span.is_some(),
            "resource rejection retains source origin"
        );
    }
}

#[test]
fn unit_resource_deinit_copies_scalar_fields_without_consuming_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun take(own value: Int): Unit {}\n\
         class Resource(val value: Int) { deinit() { val copied = this.value; take(copied) } }\n\
         fun entry(): Unit { val resource = Resource(7) }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Copyable field read uses readonly receiver");
    let deinit = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("deinit"))
        .unwrap();
    assert!(
        deinit
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. }))
    );
    assert!(
        !deinit
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    crate::llvm::render_verified_program(&program)
        .expect("readonly field copy verifies through LLVM");
}

#[test]
fn unit_resource_deinit_plans_generic_helpers_without_resolving_callable_values() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun <T : Copyable> sink(own value: T): Unit {}\n\
         fun <T : Copyable> relay(own value: T): Unit { sink(value) }\n\
         class Resource(val value: Int) { deinit() { relay(this.value) } }\n\
         fun entry(): Unit { val resource = Resource(7) }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let instances = crate::ssa::unit_plan::plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("hidden resource body preserves ordinary generic callable planning");
    assert_eq!(
        instances.len(),
        4,
        "entry, hidden body and two concrete generic helpers"
    );
}
