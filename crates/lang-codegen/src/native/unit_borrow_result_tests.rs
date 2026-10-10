//! 普通跨文件源码的 shared 返回与词法终止，真实 object/link/run。
use super::*;

#[test]
fn native_borrow_last_use_unit_preserves_permissions_and_owner_cleanup() {
    check_normal_cases(crate::native_borrow_last_use_cases::cases());
}

#[test]
fn native_borrow_direct_alias_unit_preserves_real_storage_and_owner_cleanup() {
    check_normal_cases(crate::native_borrow_last_use_cases::alias_cases());
}

#[test]
fn native_borrow_stable_place_unit_preserves_storage_and_cleanup() {
    check_normal_cases(crate::native_borrow_place_cases::cases());
}

#[test]
fn native_nullable_map_promotion_unit_transfers_and_drops_generic_payloads() {
    check_normal_cases(crate::native_map_promotion_cases::cases());
}

fn check_normal_cases(cases: impl IntoIterator<Item = crate::native_borrow_last_use_cases::Case>) {
    for case in cases {
        let (provider, entry_text) = case.source.split_once("fun entry()").unwrap();
        let analysis = analyze_sources(
            &format!("package p\n{provider}"),
            &format!("package q\nimport p.*\nfun entry(){entry_text}"),
        );
        let (program, entry) = lower_scalar_unit_with_entry(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let counted =
            crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, case.allocations);
        crate::native_tests::boxed_enum_tests::assert_success(&counted, case.stdout);
        let directory = TestDirectory::create();
        let object = directory.join("last-use.o");
        let executable = directory.join("last-use");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{}: {linked:?}", case.name);
        let run = Command::new(&executable).output().unwrap();
        crate::native_tests::boxed_enum_tests::assert_success(&run, case.stdout);
        assert_no_sibling_temporary(&directory.0);
    }
}

#[test]
fn native_borrow_storage_unit_bundle_callback_preserves_owners() {
    // Keep the original formerly unsupported Bundle callback source unchanged.
    let analysis = analyze_unit();
    let declaration = analysis.declaration("q", "supportedBorrow");
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        declaration,
    )
    .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // String and Bundle allocate; the empty closure stores its environment inline.
    let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 2);
    assert!(counted.status.success(), "{counted:?}");
    assert!(counted.stdout.is_empty(), "{counted:?}");
    assert!(counted.stderr.is_empty(), "{counted:?}");

    let directory = TestDirectory::create();
    let object = directory.join("borrow-bundle.o");
    let executable = directory.join("borrow-bundle");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        declaration,
        &object,
    )
    .unwrap();
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn native_borrow_storage_unit_generic_projection_and_nullable() {
    for callable in ["view", "wrap"] {
        for (name, text, expected) in crate::native_borrow_storage_cases::cases(callable) {
            // Keep the helper body in another source; the actual binding stays in entry.
            let (provider, entry) = text.split_once("fun entry()").unwrap();
            let analysis = analyze_sources(
                &format!("package p\n{provider}"),
                &format!("package q\nimport p.*\nfun entry(){entry}"),
            );
            let directory = TestDirectory::create();
            let object = directory.join("borrow-storage.o");
            let executable = directory.join("borrow-storage");
            emit_native_unit_object(
                &analysis.sources,
                &analysis.inputs(),
                &analysis.names,
                &analysis.environment,
                &analysis.typed,
                &analysis.owned,
                analysis.declaration("q", "entry"),
                &object,
            )
            .unwrap_or_else(|error| panic!("{name}/{callable}: {error:?}"));
            let linked = Command::new(crate::test_support::clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(linked.status.success(), "{name}/{callable}: {linked:?}");
            let run = Command::new(&executable).output().unwrap();
            assert!(run.status.success(), "{name}/{callable}: {run:?}");
            assert_eq!(run.stdout, expected, "{name}/{callable}");
        }
    }
}

#[test]
fn native_borrow_result_unit_direct_and_wrapper_keep_source_until_scope_end() {
    for callable in ["view", "wrap"] {
        let provider = "package p\nfun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(own source: String) { println(source) }";
        let consumer = format!(
            "package q\nfun entry() {{ val source = \"kept\".clone(); {{ borrow val item = p.{callable}(source); println((item)) }}; p.consume(source) }}"
        );
        let analysis = analyze_sources(provider, &consumer);
        let directory = TestDirectory::create();
        let object = directory.join("borrow-result.o");
        let executable = directory.join("borrow-result");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .expect("ordinary shared result must emit object");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"kept\nkept\n");
    }
}

#[test]
fn native_borrow_result_unit_scalar_pointer_reads_copyable_storage() {
    let analysis = analyze_sources(
        "package p\nfun view(source: Int): borrow Int from source = source\nfun wrap(source: Int): borrow Int from source = view(source)\nfun observe(own source: Int) { if (source == 7) { println(\"seven\") } else { error(\"wrong value\") } }",
        "package q\nfun entry() { val source = 7; { borrow val item = p.wrap(source); p.observe(item) }; p.observe(source) }",
    );
    let directory = TestDirectory::create();
    let object = directory.join("borrow-int.o");
    let executable = directory.join("borrow-int");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .unwrap();
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"seven\nseven\n");
}

#[test]
fn native_map_require_unit_borrows_storage_and_aborts_on_missing() {
    for (name, text, expected) in
        crate::native_map_require_cases::cases()
            .into_iter()
            .chain(std::iter::once((
                "missing",
                crate::native_map_require_cases::MISSING,
                &b""[..],
            )))
    {
        let (provider, entry) = text.split_once("fun entry()").unwrap();
        let analysis = analyze_sources(
            &format!("package p\n{provider}"),
            &format!("package q\nimport p.*\nfun entry(){entry}"),
        );
        let directory = TestDirectory::create();
        let object = directory.join("map-require.o");
        let executable = directory.join("map-require");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{name}: {linked:?}");
        let run = Command::new(&executable).output().unwrap();
        if name == "missing" {
            assert!(!run.status.success());
        } else {
            assert!(run.status.success(), "{name}: {run:?}");
            assert_eq!(run.stdout, expected);
        }
    }
}

#[test]
fn native_map_with_unit_generic_matrix() {
    for (name, text, expected) in crate::native_map_with_cases::cases() {
        // Keep the helper body in another source; the actual binding stays in entry.
        let (provider, entry) = text.split_once("fun entry()").unwrap();
        let analysis = analyze_sources(
            &format!("package p\n{provider}"),
            &format!("package q\nimport p.*\nfun entry(){entry}"),
        );
        let directory = TestDirectory::create();
        let object = directory.join("borrow-storage.o");
        let executable = directory.join("borrow-storage");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{name}: {linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
}

#[test]
fn native_map_with_unit_key_and_capture_owners_release_exactly_once() {
    for (name, text, expected) in crate::native_map_with_cases::cases()
        .into_iter()
        .filter(|(name, _, _)| matches!(*name, "capture" | "resource"))
    {
        let (provider, entry) = text.split_once("fun entry()").unwrap();
        let analysis = analyze_sources(
            &format!("package p\n{provider}"),
            &format!("package q\nimport p.*\nfun entry(){entry}"),
        );
        let (program, entry) = lower_scalar_unit_with_entry(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        // Same normal source owner inventory as the single-file counterpart.
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(
            &llvm,
            if name == "capture" { 8 } else { 6 },
        );
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected, "{name}");
    }
}

#[test]
fn borrow_call_lowering_unit_groups_and_field_forwarding_run_without_owner_leaks() {
    for (name, provider, body, allocations, expected) in [
        (
            "grouped-binding",
            "fun view(source: String): borrow String from source = source",
            "val source = \"kept\".clone(); borrow val item = ((view(source))); println(item)",
            1,
            &b"kept\n"[..],
        ),
        (
            "grouped-return",
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = ((view(source)))",
            "val source = \"kept\".clone(); borrow val item = wrap(source); println(item)",
            1,
            &b"kept\n"[..],
        ),
        (
            "field-forwarding",
            "class Packet(val text: String)\nfun view(source: String): borrow String from source = source\nfun wrap(source: Packet): borrow String from source = view(source.text)\nfun consume(own source: Packet) {}",
            "val source = Packet(\"kept\".clone()); borrow val item = wrap(source); println(item); consume(source)",
            2,
            &b"kept\n"[..],
        ),
    ] {
        let analysis = analyze_sources(
            &format!("package p\n{provider}"),
            &format!("package q\nimport p.*\nfun entry() {{ {body} }}"),
        );
        let (program, entry) = lower_scalar_unit_with_entry(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let counted =
            crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
        crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
        let directory = TestDirectory::create();
        let object = directory.join("borrow-call.o");
        let executable = directory.join("borrow-call");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
            &object,
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{name}: {linked:?}");
        let run = Command::new(&executable).output().unwrap();
        crate::native_tests::boxed_enum_tests::assert_success(&run, expected);
    }
}
