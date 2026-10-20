//! Test-only, real single MemoryDocument products shared by native, generated export, and LLVM consumers.
//! Foreign-fact, fault, counter, calibration, and sanitizer consumers remain CI-only.
//! No synthetic facts, observer callback, or single-file compatibility path.

#[path = "native_comma_when_cleanup_tests.rs"]
mod comma_when_cleanup_tests;
#[path = "native_nullable_rc_proof_tests.rs"]
mod nullable_rc_proof_tests;
#[path = "native_pending_construction_tests.rs"]
mod pending_construction_tests;

use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        DeclarationId, SourceUnitInput, SourceUnitKey, SymbolKind, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        ConstEnabledOwnedUnit, ValidatedCompilationUnitOwnership,
        check_compilation_unit_constant_ownership, const_owned_compilation_unit_view,
        owned_compilation_unit_view,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        ConstEnabledTypedUnit, FinalTypeAdmissionError, TypeEnvironment,
        ValidatedCompilationUnitTypes, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

use crate::ssa::{
    LoweringError,
    model::{FunctionId, Program},
    unit_lower::{constant::lower_const_owned_unit_with_entry, lower_owned_unit_with_entry},
};

/// Existing frozen source with clean raw types but an L0080 final-admission rejection.
pub(crate) const STRICT_REJECTION: &str = include_str!(
    "../../lang-frontend/tests/fixtures/final-type-admission-0319/r01_bogus_member.ko"
);

#[derive(Debug, Default)]
pub(crate) struct FrontendReport {
    pub(crate) stages: Vec<(&'static str, usize)>,
    pub(crate) diagnostics: Vec<(&'static str, Diagnostic)>,
}

impl FrontendReport {
    fn record(&mut self, stage: &'static str, diagnostics: &[Diagnostic]) {
        self.stages.push((stage, diagnostics.len()));
        self.diagnostics.extend(
            diagnostics
                .iter()
                .cloned()
                .map(|diagnostic| (stage, diagnostic)),
        );
    }
}

pub(crate) struct SourceRejection {
    pub(crate) sources: SourceMap,
    pub(crate) report: FrontendReport,
}

impl std::fmt::Debug for SourceRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.report, formatter)
    }
}

enum OwnedProducts {
    Basic(
        ValidatedCompilationUnitTypes,
        ValidatedCompilationUnitOwnership,
    ),
    Constant(ConstEnabledTypedUnit, ConstEnabledOwnedUnit),
}

/// Own actual phase products, never an input slice or a sealed view borrowing this struct.
pub(crate) struct UnitSource {
    sources: SourceMap,
    parsed: ParsedFile,
    names: ValidatedCompilationUnitNames,
    environment: TypeEnvironment,
    products: OwnedProducts,
    pub(crate) report: FrontendReport,
}

impl UnitSource {
    pub(crate) fn analyze(name: &str, text: &str) -> Result<Self, SourceRejection> {
        let mut sources = SourceMap::new();
        let source = sources.add_source(name, text).expect("unique source");
        let lexed = lex(&sources, source).expect("internal lexing failure");
        let parsed = parse_file(&sources, &lexed).expect("internal parsing failure");
        let mut report = FrontendReport::default();
        // ParsedFile already includes the original lexical diagnostics.
        report.record("parse", parsed.diagnostics());
        if !parsed.diagnostics().is_empty() {
            return Err(SourceRejection { sources, report });
        }
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("internal index failure");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("internal unit name resolution failure");
        // Unit names include index diagnostics; do not truncate to local resolution diagnostics.
        report.record("names", names.diagnostics());
        if !names.diagnostics().is_empty() {
            return Err(SourceRejection { sources, report });
        }
        let names = names
            .validate()
            .expect("incomplete unit names without diagnostics");
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .expect("internal unit type checking failure");
        if !raw.diagnostics().is_empty() {
            report.record("types", raw.diagnostics());
            return Err(SourceRejection { sources, report });
        }
        // This is the strict gate, not basic validate(). It runs before either ownership path.
        match complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw) {
            Ok(complete) => {
                assert!(complete.is_same_analysis(&raw));
                assert!(std::ptr::eq(complete.types(), &raw));
            }
            Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
                report.record("types", &diagnostics);
                return Err(SourceRejection { sources, report });
            }
            Err(FinalTypeAdmissionError::Internal(error)) => {
                panic!("internal final type admission failure, not source rejection: {error:?}");
            }
        }
        report.record("types", raw.diagnostics());
        let outcome = analyze_basic_unit_ownership(&sources, &inputs, &names, &environment, raw)
            .expect("internal basic unit ownership failure");
        let products = match outcome.into_result() {
            Ok((typed, owned)) => {
                report.record("ownership", owned.diagnostics());
                if !owned.diagnostics().is_empty() {
                    return Err(SourceRejection { sources, report });
                }
                assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
                let owned = owned.validate().expect("incomplete basic ownership facts");
                OwnedProducts::Basic(typed, owned)
            }
            Err(raw) => {
                // The original NotBasic recovery is the only route to constant capability.
                // Never inspect the source spelling or force all sources through this path.
                let typed = raw
                    .validate_constants()
                    .expect("complete types acquired neither basic nor constant capability");
                let owned = check_compilation_unit_constant_ownership(
                    &sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                )
                .expect("internal constant unit ownership failure");
                report.record("ownership", owned.ownership().diagnostics());
                if !owned.ownership().diagnostics().is_empty() {
                    return Err(SourceRejection { sources, report });
                }
                assert!(
                    owned.ownership().deferred().is_empty(),
                    "{:?}",
                    owned.ownership().deferred()
                );
                let owned = owned
                    .validate()
                    .expect("incomplete constant ownership facts");
                OwnedProducts::Constant(typed, owned)
            }
        };
        Ok(Self {
            sources,
            parsed,
            names,
            environment,
            products,
            report,
        })
    }

    fn entry(&self) -> DeclarationId {
        self.declaration("entry", SymbolKind::Function)
    }

    pub(crate) fn declaration(&self, name: &str, kind: SymbolKind) -> DeclarationId {
        let index = self.names.names().index();
        let [source] = index.source_units() else {
            panic!("expected exactly one original memory document");
        };
        assert_eq!(source.source_id(), self.parsed.source_id());
        assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
        let entries = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == name
                    && declaration.kind() == kind
                    && declaration.source_unit() == source.id()
                    && declaration.package() == source.package()
                    && declaration.name_span().source_id() == self.parsed.source_id()
                    && self.parsed.roots().contains(&declaration.root())
            })
            .collect::<Vec<_>>();
        let [entry] = entries.as_slice() else {
            panic!("expected one real top-level declaration {name} {kind:?}: {entries:?}");
        };
        entry.id()
    }

    fn lower(&self) -> (Program, FunctionId) {
        self.lower_entry(self.entry())
            .expect("source must lower through its actual Unit capability to verified SSA")
    }

    pub(crate) fn lower_entry(
        &self,
        entry: DeclarationId,
    ) -> Result<(Program, FunctionId), LoweringError> {
        let inputs = [SourceUnitInput::memory_document(
            self.parsed.source_id(),
            &self.parsed,
        )];
        match &self.products {
            OwnedProducts::Basic(typed, owned) => {
                let unit = owned_compilation_unit_view(
                    &self.sources,
                    &inputs,
                    &self.names,
                    &self.environment,
                    typed,
                    owned,
                )
                .expect("original basic products construct the sealed view");
                lower_owned_unit_with_entry(&unit, entry)
            }
            OwnedProducts::Constant(typed, owned) => {
                let unit = const_owned_compilation_unit_view(
                    &self.sources,
                    &inputs,
                    &self.names,
                    &self.environment,
                    typed,
                    owned,
                )
                .expect("original constant products construct the sealed view");
                lower_const_owned_unit_with_entry(&unit, entry)
            }
        }
    }

    /// Preserve all four original clean-phase assertions at the stricter Unit boundary.
    pub(crate) fn assert_frontend_clean(&self) {
        assert_eq!(
            self.report.stages,
            [("parse", 0), ("names", 0), ("types", 0), ("ownership", 0)]
        );
        assert!(self.report.diagnostics.is_empty(), "{:?}", self.report);
    }

    pub(crate) fn emit(
        &self,
        entry: impl Into<crate::NativeUnitEntry>,
        output: &std::path::Path,
    ) -> Result<(), crate::NativeObjectError> {
        let inputs = [SourceUnitInput::memory_document(
            self.parsed.source_id(),
            &self.parsed,
        )];
        let entry = entry.into();
        match &self.products {
            OwnedProducts::Basic(typed, owned) => {
                let view = owned_compilation_unit_view(
                    &self.sources,
                    &inputs,
                    &self.names,
                    &self.environment,
                    typed,
                    owned,
                )?;
                crate::emit_native_owned_unit_object(&view, entry, output)
            }
            OwnedProducts::Constant(typed, owned) => {
                let view = const_owned_compilation_unit_view(
                    &self.sources,
                    &inputs,
                    &self.names,
                    &self.environment,
                    typed,
                    owned,
                )?;
                crate::emit_native_const_owned_unit_object(&view, entry, output)
            }
        }
    }

    /// CI-only identity consumers borrow real products from independent analyses.
    pub(crate) fn emit_with_products_from(
        &self,
        typed_source: &Self,
        owned_source: &Self,
        entry: DeclarationId,
        output: &std::path::Path,
    ) -> Result<(), crate::NativeObjectError> {
        let inputs = [SourceUnitInput::memory_document(
            self.parsed.source_id(),
            &self.parsed,
        )];
        match (&typed_source.products, &owned_source.products) {
            (OwnedProducts::Basic(typed, _), OwnedProducts::Basic(_, owned)) => {
                let view = owned_compilation_unit_view(
                    &self.sources,
                    &inputs,
                    &self.names,
                    &self.environment,
                    typed,
                    owned,
                )?;
                crate::emit_native_owned_unit_object(&view, entry, output)
            }
            (OwnedProducts::Constant(typed, _), OwnedProducts::Constant(_, owned)) => {
                let view = const_owned_compilation_unit_view(
                    &self.sources,
                    &inputs,
                    &self.names,
                    &self.environment,
                    typed,
                    owned,
                )?;
                crate::emit_native_const_owned_unit_object(&view, entry, output)
            }
            _ => panic!("identity fixture must retain the original capability on both sides"),
        }
    }

    /// CI-only SourceMap identity case; inputs and all phase products remain the first chain.
    pub(crate) fn emit_with_sources_from(
        &self,
        source_owner: &Self,
        entry: DeclarationId,
        output: &std::path::Path,
    ) -> Result<(), crate::NativeObjectError> {
        let inputs = [SourceUnitInput::memory_document(
            self.parsed.source_id(),
            &self.parsed,
        )];
        let OwnedProducts::Basic(typed, owned) = &self.products else {
            panic!("the original SourceMap identity fixture has basic capability");
        };
        let view = owned_compilation_unit_view(
            &source_owner.sources,
            &inputs,
            &self.names,
            &self.environment,
            typed,
            owned,
        )?;
        crate::emit_native_owned_unit_object(&view, entry, output)
    }

    pub(crate) fn lower_to_llvm(&self) -> String {
        let (program, entry) = self.lower();
        let (repeated, repeated_entry) = self.lower();
        assert_eq!(entry.index(), repeated_entry.index());
        assert_eq!(entry.module().index(), repeated_entry.module().index());
        assert_eq!(
            crate::ssa::render_program(&program),
            crate::ssa::render_program(&repeated),
            "recursive type registration must be deterministic"
        );
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("Unit LLVM must verify");
        assert_eq!(
            llvm,
            crate::llvm::render_verified_program_with_entry(&repeated, repeated_entry)
                .expect("repeated Unit LLVM must verify")
        );
        llvm
    }

    /// Bind a calibration target to its actual frontend item and returned SSA function.
    /// The name comes from the Program, not a copy of Unit mangling rules.
    pub(crate) fn definition(&self, source_name: &str, deinit: bool) -> (usize, String) {
        let declarations = self
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| declaration.name() == source_name)
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("fixture must uniquely identify source declaration {source_name}");
        };
        let typed = match &self.products {
            OwnedProducts::Basic(typed, _) => typed.types(),
            OwnedProducts::Constant(typed, _) => typed.types(),
        };
        let signature = typed
            .signatures()
            .declaration(declaration.id())
            .expect("original declaration signature");
        let item = if deinit {
            let descriptor = signature
                .nominal()
                .and_then(|nominal| nominal.deinit())
                .expect("original nominal has a real deinit descriptor");
            assert_eq!(descriptor.owner(), declaration.id());
            assert_eq!(descriptor.item().source_unit(), declaration.source_unit());
            descriptor.item().item()
        } else {
            assert!(signature.callable().is_some(), "real source function");
            declaration.root()
        };
        let span = self.parsed.ast().items().get(item).unwrap().span();
        assert_eq!(span.source_id(), self.parsed.source_id());
        let (program, entry) = self.lower();
        let functions = program
            .module(entry.module())
            .unwrap()
            .functions
            .iter()
            .filter(|function| function.origin.span() == span)
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("fixture requires one real SSA function for {source_name}, deinit={deinit}");
        };
        (function.id.index(), function.name.clone())
    }

    /// Retain the original witness-uniqueness check across every real emitted deinit.
    pub(crate) fn deinit_definitions(&self) -> Vec<(usize, String)> {
        let typed = match &self.products {
            OwnedProducts::Basic(typed, _) => typed.types(),
            OwnedProducts::Constant(typed, _) => typed.types(),
        };
        let spans = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|signature| signature.nominal().and_then(|nominal| nominal.deinit()))
            .map(|descriptor| {
                self.parsed
                    .ast()
                    .items()
                    .get(descriptor.item().item())
                    .unwrap()
                    .span()
            })
            .collect::<Vec<_>>();
        let (program, entry) = self.lower();
        program
            .module(entry.module())
            .unwrap()
            .functions
            .iter()
            .filter(|function| spans.contains(&function.origin.span()))
            .map(|function| (function.id.index(), function.name.clone()))
            .collect()
    }
}

/// Check the adapter's f<index>.<SSA-name> identity against one actual defined LLVM function.
pub(crate) fn llvm_definition<'ctx>(
    module: &inkwell::module::Module<'ctx>,
    identity: &(usize, String),
) -> inkwell::values::FunctionValue<'ctx> {
    let expected = format!("f{}.{}", identity.0, identity.1);
    let functions = module
        .get_functions()
        .filter(|function| {
            function.count_basic_blocks() > 0 && function.get_name().to_str().unwrap() == expected
        })
        .collect::<Vec<_>>();
    assert_eq!(
        functions.len(),
        1,
        "unique actual Unit definition {expected}"
    );
    functions[0]
}

#[test]
fn memory_document_basic_source_reaches_verified_ssa_llvm_and_native() {
    let text = "package ordinary\nfun value(): Int = 37\n\
        fun entry(): Unit { if (value() == 37) { println(\"basic-memory\") } }";
    let unit = UnitSource::analyze("display name, not a path", text).expect("ordinary source");
    assert!(matches!(unit.products, OwnedProducts::Basic(..)));
    assert_original_entry(&unit, "display name, not a path", text);
    let llvm = unit.lower_to_llvm();
    assert!(llvm.contains("define i32 @main("), "{llvm}");
    assert_native_output(&unit, b"basic-memory\n");
}

#[test]
fn memory_document_constant_source_reaches_verified_ssa_llvm_and_native() {
    let text = "package constants\nconst val ANSWER: Int = 37\n\
        fun entry(): Unit { if (ANSWER == 37) { println(\"constant-memory\") } }";
    let unit = UnitSource::analyze("constant memory document", text).expect("constant source");
    let OwnedProducts::Constant(typed, owned) = &unit.products else {
        panic!("the actual constant requires its dedicated capability");
    };
    assert_eq!(typed.constants().declarations().len(), 1);
    assert_eq!(owned.materializations().len(), 1);
    assert_original_entry(&unit, "constant memory document", text);
    let llvm = unit.lower_to_llvm();
    assert!(llvm.contains("define i32 @main("), "{llvm}");
    assert_native_output(&unit, b"constant-memory\n");
}

/// The two ordinary positive controls retain the real object, link, and output checks.
fn assert_native_output(unit: &UnitSource, expected: &[u8]) {
    use std::{fs, process::Command};

    let directory = crate::native_tests::TestDirectory::create();
    let object = directory.join("memory-document.o");
    let executable = directory.join("memory-document");
    unit.emit(unit.entry(), &object)
        .expect("original MemoryDocument must emit a native object");
    let bytes = fs::read(&object).expect("real native object");
    assert!(bytes.len() >= 20, "native object contains its full header");
    crate::test_support::assert_native_object(&bytes);
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("existing native Clang linker must launch");
    assert_eq!(linked.status.code(), Some(0), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("the newly linked MemoryDocument executable must launch");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, expected, "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

fn assert_original_entry(unit: &UnitSource, name: &str, text: &str) {
    let source = unit.parsed.source_id();
    assert_eq!(unit.sources.source_name(source), Ok(name));
    assert_eq!(unit.sources.source_text(source), Ok(text));
    assert_eq!(
        unit.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 0)]
    );
    assert!(unit.report.diagnostics.is_empty());
    let declaration = &unit.names.names().index().declarations()[unit.entry().index()];
    assert_eq!(unit.sources.slice(declaration.name_span()), Ok("entry"));
    let (program, entry) = unit.lower();
    crate::ssa::verify::verify_program(&program).expect("real source SSA verifies");
    let function = program
        .module(entry.module())
        .and_then(|module| module.function(entry))
        .expect("returned entry belongs to the returned Program");
    let origin = function.origin.span();
    assert_eq!(origin.source_id(), source);
    assert!(origin.start() <= declaration.name_span().start());
    assert!(origin.end() >= declaration.name_span().end());
    assert!(function.return_types.is_empty());
    assert!(function.blocks[0].parameters.is_empty());
}

#[test]
fn strict_type_qualification_rejects_before_ownership_or_llvm() {
    // Raw types are intentionally clean; the actual complete gate finds the unresolved member.
    let text = STRICT_REJECTION;
    let Err(rejection) = UnitSource::analyze("strict.ko", text) else {
        panic!("rejected source must not produce an owned UnitSource that can lower");
    };
    assert_eq!(
        rejection.report.stages,
        [("parse", 0), ("names", 0), ("types", 1)]
    );
    let [(stage, diagnostic)] = rejection.report.diagnostics.as_slice() else {
        panic!("one complete source diagnostic is required");
    };
    let sources = &rejection.sources;
    let source = diagnostic.primary_span().source_id();
    let parsed = parse_file(sources, &lex(sources, source).unwrap()).unwrap();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let raw = check_compilation_unit_types(sources, &inputs, &names, &environment).unwrap();
    assert!(raw.diagnostics().is_empty());
    let Err(FinalTypeAdmissionError::Diagnostics(expected)) =
        complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
    else {
        panic!("the source must fail strict qualification, not an internal invariant");
    };
    assert!(
        raw.diagnostics().is_empty(),
        "complete must not rewrite raw recovery"
    );
    assert_eq!(*stage, "types");
    assert_eq!(
        expected.len(),
        1,
        "complete retains the single original error"
    );
    assert_eq!(diagnostic, &expected[0], "preserve the full diagnostic");
    assert_eq!(diagnostic.code().to_string(), "L0080");
    let span = diagnostic.primary_span();
    assert_eq!(
        rejection.sources.source_name(span.source_id()),
        Ok("strict.ko")
    );
    assert_eq!(rejection.sources.slice(span), Ok("fooBar"));
    let start = text.find("fooBar").unwrap();
    assert_eq!((span.start(), span.end()), (start, start + 6));
}
