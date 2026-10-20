//! Ordinary source controls for the explicitly unsupported companion callable backend.

use lang_frontend::type_checking::{UnitCallTarget, complete_compilation_unit_types};

use super::*;

struct Fixture {
    sources: SourceMap,
    source: SourceId,
    parsed: ParsedFile,
    names: ValidatedCompilationUnitNames,
    environment: TypeEnvironment,
    typed: ValidatedCompilationUnitTypes,
    owned: ValidatedCompilationUnitOwnership,
}

impl Fixture {
    fn new(text: &str) -> Self {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(&mut sources, "main.ko", text);
        let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("real compilation index");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("ordinary names")
            .validate()
            .expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .expect("ordinary types")
            .validate()
            .expect("valid types");
        assert!(typed.types().diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &environment, typed.types())
            .expect("companion source remains fully admitted by the Unit frontend");
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .expect("ordinary ownership")
                .validate()
                .expect("valid ownership");
        Self {
            sources,
            source,
            parsed,
            names,
            environment,
            typed,
            owned,
        }
    }

    fn inputs(&self) -> [SourceUnitInput<'_>; 1] {
        [SourceUnitInput::new(
            "root",
            "main.ko",
            self.source,
            &self.parsed,
        )]
    }

    fn entry(&self) -> DeclarationId {
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "main")
            .expect("real entry declaration")
            .id()
    }
}

#[test]
fn selected_companion_calls_report_precise_native_capability() {
    let mut observations = Vec::new();
    const INDEPENDENT: &str = "class Factory { companion object { fun identity(value: Int): Int = value } }\nfun main(): Unit { val result = Factory.identity(7); if (result == 7) { println(\"factory\") } }";
    for (text, expected_start, expected_end, expected_call) in [
        (
            include_str!(
                "../../../lang-frontend/tests/fixtures/final-admission-static-recovery-0323/p28_companion.ko"
            ),
            95,
            105,
            "P.origin()",
        ),
        (
            include_str!(
                "../../../lang-frontend/tests/fixtures/final-admission-static-recovery-0323/companion_minimal.ko"
            ),
            95,
            105,
            "P.origin()",
        ),
        (INDEPENDENT, 109, 128, "Factory.identity(7)"),
    ] {
        let fixture = Fixture::new(text);
        let inputs = fixture.inputs();
        let call = fixture
            .typed
            .types()
            .calls()
            .iter()
            .find(|call| {
                fixture
                    .parsed
                    .ast()
                    .expressions()
                    .get(call.expression().expression())
                    .is_ok_and(|node| fixture.sources.slice(node.span()) == Ok(expected_call))
            })
            .expect("real selected companion call");
        let UnitCallTarget::Symbol(symbol) = call.target() else {
            panic!("companion call has an actual selected Symbol");
        };
        assert!(call.receiver().is_none());
        let callable = fixture
            .typed
            .types()
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal())
            .flat_map(|nominal| nominal.companion_members())
            .find(|callable| {
                callable.target()
                    == lang_frontend::type_checking::UnitCallableTarget::Symbol(symbol)
            })
            .expect("selected identity belongs to an actual companion signature");
        assert!(callable.has_body());
        assert!(callable.receiver().is_none());
        assert_eq!(
            callable.type_parameters().len(),
            call.instance().type_arguments().len()
        );
        for previous in [None, Some(b"previous object".as_slice())] {
            let directory = TestDirectory::create();
            let object = directory.join("program.o");
            if let Some(bytes) = previous {
                fs::write(&object, bytes).expect("ordinary existing output");
            }
            fs::write(directory.join("neighbor.txt"), b"ordinary neighbor").unwrap();
            let before = directory_bytes(&directory.0);
            let error = emit_native_unit_object(
                &fixture.sources,
                &inputs,
                &fixture.names,
                &fixture.environment,
                &fixture.typed,
                &fixture.owned,
                fixture.entry(),
                &object,
            )
            .expect_err("selected companion callable remains an explicit capability boundary");
            eprintln!("companion native boundary: {error:?}");
            let slice = error
                .span()
                .and_then(|span| fixture.sources.slice(span).ok())
                .map(str::to_owned);
            observations.push((
                error,
                fixture.source,
                slice,
                expected_start,
                expected_end,
                expected_call,
            ));
            assert_eq!(
                directory_bytes(&directory.0),
                before,
                "complete output directory bytes"
            );
            match previous {
                Some(bytes) => assert_eq!(fs::read(&object).unwrap(), bytes),
                None => assert!(!object.exists()),
            }
            assert_no_sibling_temporary(&directory.0);
        }
    }
    assert_eq!(
        observations.len(),
        6,
        "two unchanged sources plus an independent source, each with two output states"
    );
    for (error, source, slice, expected_start, expected_end, expected_call) in observations {
        assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
        assert_eq!(
            error.to_string(),
            "native object UnsupportedSource: companion function calls are not supported by native code generation"
        );
        assert!(
            error.diagnostic().is_none(),
            "not a fabricated language diagnostic"
        );
        let span = error.span().expect("original call source span");
        assert_eq!(span.source_id(), source);
        assert_eq!((span.start(), span.end()), (expected_start, expected_end));
        assert_eq!(slice.as_deref(), Some(expected_call));
    }
}

#[test]
fn unused_companion_preserves_top_level_and_instance_native_calls() {
    let fixture = Fixture::new(
        "class Holder(val item: Int) {\n\
             companion object { fun create(): Holder = Holder(1) }\n\
             fun value(): Int = item\n\
         }\n\
         fun top(): Int = 7\n\
         fun main(): Unit { val holder = Holder(1); if (holder.value() + top() == 8) { println(\"neighbors\") } }",
    );
    let inputs = fixture.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    emit_native_unit_object(
        &fixture.sources,
        &inputs,
        &fixture.names,
        &fixture.environment,
        &fixture.typed,
        &fixture.owned,
        fixture.entry(),
        &object,
    )
    .expect("unused companion must not reject reachable ordinary functions");
    crate::test_support::assert_native_object(&fs::read(&object).expect("real native object"));
    assert_no_sibling_temporary(&directory.0);
    // This control emits an object only; it never links or launches a program.
}

fn directory_bytes(directory: &Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
    let mut entries = fs::read_dir(directory)
        .expect("owned directory")
        .map(|entry| {
            let entry = entry.expect("owned directory entry");
            assert!(
                entry.file_type().unwrap().is_file(),
                "only ordinary fixture files"
            );
            (
                entry.file_name(),
                fs::read(entry.path()).expect("whole file bytes"),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}
