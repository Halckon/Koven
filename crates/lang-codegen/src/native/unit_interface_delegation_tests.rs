//! Interface default, inheritance and delegation native object/link/run coverage.
use super::*;

#[test]
fn interface_default_and_super_static_calls_link_and_run() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base { fun read(): Int = 7 }\n\
         interface Derived: Base {\n\
             fun inherited(): Int = super<Base>.read()\n\
         }\n\
         class Child: Derived {}\n\
         fun entry(): Unit {\n\
             val actual = Child().inherited()\n\
             if (actual == 7) { println(\"default-super\") } else { error(\"wrong default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("interface-default.o");
    let executable = directory.join("interface-default");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("interface default and super<I> calls must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked interface-default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"default-super\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn interface_default_abstract_requirement_override_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         class Child: Readable { override fun read(): Int = 7 }\n\
         fun entry(): Unit {\n\
             val actual = Child().throughRequirement()\n\
             if (actual == 7) { println(\"abstract-override\") }\
             else { error(\"wrong override\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("abstract-override.o");
    let executable = directory.join("abstract-override");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("abstract requirement must resolve to its concrete override");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked abstract override executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"abstract-override\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn inherited_and_unrelated_defaults_satisfy_abstract_requirements_natively() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived: Base { fun read(): Int = 7 }\n\
         class Child: Derived {}\n\
         interface Required {\n\
             fun otherRead(): Int\n\
             fun throughRequirement(): Int = this.otherRead()\n\
         }\n\
         interface Provided { fun otherRead(): Int = 5 }\n\
         class OtherChild: Required, Provided {}\n\
         fun entry(): Unit {\n\
             val actual = Child().throughRequirement() + OtherChild().throughRequirement()\n\
             if (actual == 12) { println(\"inherited-defaults\") }\
             else { error(\"wrong inherited default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("inherited-defaults.o");
    let executable = directory.join("inherited-defaults");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("inherited effective defaults must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked inherited default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inherited-defaults\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn list_inherited_owner_recipe_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Host<Y>: Derived<List<Y>> {}\n\
         fun entry(): Unit {\n\
             val actual = Host<Int>().throughRequirement()\n\
             if (actual == 7) { println(\"list-inherited-owner\") }\
             else { error(\"wrong inherited owner recipe\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("list-inherited-owner.o");
    let executable = directory.join("list-inherited-owner");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("List inherited owner recipe must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked List inherited-owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"list-inherited-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn parameter_independent_class_inherited_owner_recipe_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val marker: Int)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(): Unit {\n\
             val actual = Host<Int>().throughRequirement()\n\
             if (actual == 7) { println(\"class-inherited-owner\") }\
             else { error(\"wrong class inherited owner recipe\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("class-inherited-owner.o");
    let executable = directory.join("class-inherited-owner");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("parameter-independent class inherited owner recipe must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked class inherited-owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"class-inherited-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn dependent_class_inherited_owner_recipe_value_roundtrip_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base<A> { fun read(): Int }\n\
         interface Derived<B>: Base<String> {\n\
             fun read(): Int = 7\n\
             fun echo(own input: B): B = input\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(): Unit {\n\
             val result = Host<Int>().echo(Wrapper<Int>(7))\n\
             if (result.item == 7) { println(\"dependent-class-inherited-owner\") }\
             else { error(\"wrong dependent class inherited owner recipe\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("dependent-class-inherited-owner.o");
    let executable = directory.join("dependent-class-inherited-owner");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("dependent class inherited owner recipe must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked dependent class inherited-owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"dependent-class-inherited-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn borrow_only_interface_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         interface Derived: Readable { fun read(): Int = 2 }\n\
         class DefaultReader: Derived {}\n\
         class OverrideReader: Readable { override fun read(): Int = 7 }\n\
         class DefaultHost(val delegate: DefaultReader): Readable by delegate {}\n\
         class OverrideHost(val tag: Int, val delegate: OverrideReader): Readable by delegate {}\n\
         fun entry(): Unit {\n\
             val inherited = DefaultHost(DefaultReader()).read()\n\
             val overridden = OverrideHost(0, OverrideReader()).read()\n\
             if (inherited + overridden == 9) { println(\"borrow-delegate\") }\
             else { error(\"wrong delegate\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("borrow-delegate.o");
    let executable = directory.join("borrow-delegate");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("bodyful Borrow-only interface delegation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Borrow delegation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"borrow-delegate\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn same_requirement_delegation_chain_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         class Reader: Readable {\n\
             override fun read(): Int {\n\
                 println(\"chain-endpoint\")\n\
                 return 7\n\
             }\n\
         }\n\
         class Middle(val tag: Int, val reader: Reader): Readable by reader {}\n\
         class Host(val tag: Int, val middle: Middle): Readable by middle {}\n\
         class Outer(val tag: Int, val host: Host): Readable by host {}\n\
         fun entry(): Unit {\n\
             val actual = Outer(0, Host(1, Middle(2, Reader()))).read()\n\
             if (actual == 7) { println(\"delegate-chain\") }\
             else { error(\"wrong chain endpoint\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("delegate-chain.o");
    let executable = directory.join("delegate-chain");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("same-requirement delegation chain must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked delegation-chain executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"chain-endpoint\ndelegate-chain\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn identity_changing_delegation_chain_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base { fun read(): Int = 1 }\n\
         interface Derived: Base { fun read(): Int = 2 }\n\
         class Reader: Derived {\n\
             override fun read(): Int {\n\
                 println(\"replacement-endpoint\")\n\
                 return 7\n\
             }\n\
         }\n\
         class Middle(val reader: Reader): Derived by reader {}\n\
         class Host(val middle: Middle): Base by middle {}\n\
         fun entry(): Unit {\n\
             val actual = Host(Middle(Reader())).read()\n\
             if (actual == 7) { println(\"identity-chain\") }\
             else { error(\"wrong replacement endpoint\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("identity-chain.o");
    let executable = directory.join("identity-chain");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("identity-changing delegation chain must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked identity-chain executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"replacement-endpoint\nidentity-chain\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn generic_interface_owner_and_callable_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Mapper<T> {\n\
             fun <R> map(own input: R): R {\n\
                 println(\"generic-default\")\n\
                 return input\n\
             }\n\
         }\n\
         class DefaultMapper: Mapper<String> {}\n\
         class OverrideMapper: Mapper<String> {\n\
             override fun <R> map(own input: R): R {\n\
                 println(\"generic-override\")\n\
                 return input\n\
             }\n\
         }\n\
         class DefaultHost(val delegate: DefaultMapper): Mapper<String> by delegate {}\n\
         class OverrideHost(val delegate: OverrideMapper): Mapper<String> by delegate {}\n\
         fun entry(): Unit {\n\
             val inherited = DefaultHost(DefaultMapper()).map<Long>(7L)\n\
             val overridden = OverrideHost(OverrideMapper()).map<Long>(9L)\n\
             if (inherited + overridden == 16L) { println(\"generic-delegate\") }\
             else { error(\"wrong generic delegate\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("generic-delegate.o");
    let executable = directory.join("generic-delegate");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("generic interface owner/callable delegation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked generic-delegation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"generic-default\ngeneric-override\ngeneric-delegate\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn parameter_independent_generic_nominal_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>(val marker: Int): Readable {\n\
             override fun read(): Int = this.readMarker()\n\
             fun readMarker(): Int {\n\
                 println(\"generic-runtime-reader\")\n\
                 return this.marker\n\
             }\n\
         }\n\
         class Host<T>(val delegate: Reader<Int>): Readable by delegate {}\n\
         fun entry(): Unit {\n\
             val actual = Host<String>(Reader<Int>(7)).read()\n\
             if (actual == 7) { println(\"generic-runtime-layout\") }\
             else { error(\"wrong generic runtime layout\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("generic-runtime-layout.o");
    let executable = directory.join("generic-runtime-layout");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("parameter-independent generic nominal layout must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked generic-runtime executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"generic-runtime-reader\ngeneric-runtime-layout\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn nested_generic_nominal_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>(val item: T): Readable {\n\
             override fun read(): Int {\n\
                 println(\"nested-generic-reader\")\n\
                 return 7\n\
             }\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<T>(val delegate: Reader<Wrapper<T>>): Readable by delegate {}\n\
         fun entry(): Unit {\n\
             val actual = Host<String>(Reader<Wrapper<String>>(Wrapper<String>(\"payload\"))).read()\n\
             if (actual == 7) { println(\"nested-generic-delegate\") }\
             else { error(\"wrong nested generic delegation\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("nested-generic-delegate.o");
    let executable = directory.join("nested-generic-delegate");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("nested generic delegation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked nested-generic delegation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"nested-generic-reader\nnested-generic-delegate\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn inout_interface_default_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Mutable { inout fun probe(): Int = 7 }\n\
         class Counter(var count: Int): Mutable { fun read(): Int = count }\n\
         fun entry(): Unit {\n\
             val counter = Counter(5)\n\
             val defaultValue = counter.probe()\n\
             val actual = counter.read() + defaultValue\n\
             if (actual == 12) { println(\"inout-default\") }\
             else { error(\"wrong receiver default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("inout-default.o");
    let executable = directory.join("inout-default");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("Inout interface default must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Inout-default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inout-default\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn value_interface_default_move_only_and_copyable_specializations_link_and_run() {
    let analysis = analyze_sources(
        "package p\n\
         interface Finishable {\n\
             own fun finish(): Int = 40\n\
             own fun relay(): Int = finish()\n\
             own fun forward(): Int = this.relay()\n\
             own fun choose(flag: Boolean): Int {\n\
                 if (flag) { return finish() }\n\
                 return finish()\n\
             }\n\
         }\n\
         class Resource: Finishable {}\n\
         value class Counter(val item: Int): Finishable {}\n\
         fun entry(): Unit {\n\
             val counter = Counter(1)\n\
             val actual = Resource().forward() + counter.forward() + counter.forward() +\n\
                 Resource().choose(true) + counter.choose(false)\n\
             if (actual == 200) { println(\"value-default\") }\
             else { error(\"wrong Value receiver default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("value-default.o");
    let executable = directory.join("value-default");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("MoveOnly and Copyable Value interface defaults must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Value-default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"value-default\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}
