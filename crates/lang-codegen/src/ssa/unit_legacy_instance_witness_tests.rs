//! Stage A: unchanged legacy fixtures through strict MemoryDocument and rooted Unit.
//! Selected-root Programs do not prove whole-file/shared-mapper/multi-root equivalence.
//! Keep all original legacy tests and native contracts; these witnesses do not execute native code.
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    name_resolution::{
        DeclarationId, SourceUnitInput, SourceUnitKey, SymbolKind, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    source::Span,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::{
    llvm::render_verified_program,
    ssa::{
        model::{
            EntityType, Function, FunctionId, LoanKind, Module, Operation, Origin, Program,
            ScalarConstant, SsaTypeKind, TerminatorKind,
        },
        render::render_program,
        unit_lower::lower_owned_unit_with_entry,
        unit_lower_test_support::parsed,
        verify::verify_program,
    },
};

#[derive(Clone, Copy, Debug)]
enum Entry {
    Memory,
    Rooted,
}

const ENTRIES: [Entry; 2] = [Entry::Memory, Entry::Rooted];

// Exact original strings: no package, own-mode edits, wrapper, main, or source splitting.
const UNREACHABLE: &str =
    "fun inspect(): Int { return 0\nval unused: Int? = 1\nval asserted = unused!! }";
const GENERICS: &str = "fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own input: T): T = input\n\
         fun useInt(input: Int): Int = relay<Int>(input)\n\
         fun useIntAgain(input: Int): Int = relay(input)\n\
         fun useLong(input: Long): Long = relay(input)\n\
         fun recursive(input: Int): Int = recurse(input)";
const OVERLOADS: &str = "fun <T> choose(own input: T): T = input\n\
         fun <T> choose(own input: T, fallback: Boolean): T = input\n\
         fun one(input: Int): Int = choose(input)\n\
         fun two(input: Int): Int = choose(input, true)";

struct Observed {
    program: Program,
    root: FunctionId,
    declarations: Vec<(DeclarationId, String, Span, String)>,
}

impl Observed {
    fn module(&self) -> &Module {
        self.program
            .module(self.root.module())
            .expect("returned module")
    }

    fn entry(&self) -> &Function {
        self.module().function(self.root).expect("returned root")
    }

    fn declaration(&self, exact: &str) -> (DeclarationId, Span) {
        let matches = self
            .declarations
            .iter()
            .filter(|(_, _, _, text)| text == exact)
            .collect::<Vec<_>>();
        let [declaration] = matches.as_slice() else {
            panic!("one exact original declaration: {exact}");
        };
        (declaration.0, declaration.2)
    }

    fn function(&self, exact: &str) -> &Function {
        let (_, span) = self.declaration(exact);
        let functions = self
            .module()
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one instance of this original declaration in this selected-root Program");
        };
        function
    }
}

fn observe(mode: Entry, text: &str, root_name: &str, roots: usize) -> Observed {
    let mut sources = lang_frontend::source::SourceMap::new();
    let (source, file) = parsed(&mut sources, "lowering.ko", text);
    assert_eq!(sources.source_name(source), Ok("lowering.ko"));
    assert_eq!(sources.source_text(source), Ok(text));
    assert_eq!(file.roots().len(), roots);
    let input = match mode {
        Entry::Memory => SourceUnitInput::memory_document(source, &file),
        Entry::Rooted => SourceUnitInput::new("root", "lowering.ko", source, &file),
    };
    let inputs = [input];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("original source index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("Unit names");
    assert!(
        names.diagnostics().is_empty(),
        "{mode:?}: {:?}",
        names.diagnostics()
    );
    let names = names.validate().expect("complete names");
    let [document] = names.names().index().source_units() else {
        panic!("one unchanged document");
    };
    assert_eq!(document.source_id(), source);
    assert_eq!(
        matches!(document.key(), SourceUnitKey::MemoryDocument),
        matches!(mode, Entry::Memory)
    );
    let raw =
        check_compilation_unit_types(&sources, &inputs, &names, &environment).expect("Unit types");
    assert!(
        raw.diagnostics().is_empty(),
        "{mode:?}: {:?}",
        raw.diagnostics()
    );
    let complete = complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
        .expect("strict final admission, never a skipped witness");
    assert!(complete.is_same_analysis(&raw));
    assert!(std::ptr::eq(complete.types(), &raw));
    let outcome = analyze_basic_unit_ownership(&sources, &inputs, &names, &environment, raw)
        .expect("actual Unit ownership");
    let (typed, owned) = outcome
        .into_result()
        .expect("these originals are basic sources");
    assert!(
        owned.diagnostics().is_empty(),
        "{mode:?}: {:?}",
        owned.diagnostics()
    );
    assert!(
        owned.deferred().is_empty(),
        "{mode:?}: {:?}",
        owned.deferred()
    );
    if text == UNREACHABLE {
        assert_eq!(typed.types().non_null_assertions().len(), 1);
        assert!(owned.non_null_assertions().is_empty());
    }
    let owned = owned.validate().expect("complete ownership");
    let declarations = names
        .names()
        .index()
        .declarations()
        .iter()
        .map(|declaration| {
            assert_eq!(declaration.source_unit(), document.id());
            assert_eq!(declaration.package(), document.package());
            assert_eq!(declaration.kind(), SymbolKind::Function);
            assert!(file.roots().contains(&declaration.root()));
            let span = file.ast().items().get(declaration.root()).unwrap().span();
            assert_eq!(span.source_id(), source);
            (
                declaration.id(),
                declaration.name().to_owned(),
                span,
                sources.slice(span).unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), roots);
    let selected = declarations
        .iter()
        .filter(|(_, name, _, _)| name == root_name)
        .collect::<Vec<_>>();
    let [selected] = selected.as_slice() else {
        panic!("one original non-generic root: {root_name}");
    };
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .expect("same-analysis sealed Unit view");
    let lower = || {
        lower_owned_unit_with_entry(&view, selected.0)
            .unwrap_or_else(|error| panic!("{mode:?}/{root_name}: {error:?}"))
    };
    let (program, root) = lower();
    verify_program(&program).expect("full original-root SSA verification");
    let function = program
        .module(root.module())
        .unwrap()
        .function(root)
        .unwrap();
    assert_eq!(function.origin, Origin::Source(selected.2));
    assert!(!function.blocks.is_empty());
    let (repeated, repeated_root) = lower();
    verify_program(&repeated).expect("full repeated SSA verification");
    assert_eq!(root.module().index(), repeated_root.module().index());
    assert_eq!(root.index(), repeated_root.index());
    assert_eq!(
        repeated
            .module(repeated_root.module())
            .unwrap()
            .function(repeated_root)
            .unwrap()
            .origin,
        function.origin
    );
    assert_eq!(render_program(&program), render_program(&repeated));
    // Full module verification with no native entry wrapper. These are parameterized roots.
    assert_eq!(
        render_verified_program(&program).expect("full LLVM verification"),
        render_verified_program(&repeated).expect("full repeated LLVM verification")
    );
    Observed {
        program,
        root,
        declarations,
    }
}

fn assert_integer_signature(module: &Module, function: &Function, bits: u16, borrowed: bool) {
    let [result] = function.return_types.as_slice() else {
        panic!("one scalar result");
    };
    let integer = SsaTypeKind::Integer { bits, signed: true };
    assert_eq!(module.type_kind(*result), Some(&integer));
    let parameter = function.blocks[0].parameters[0];
    let ty = function.entity(parameter).unwrap().ty;
    if borrowed {
        assert_eq!(
            ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *result
            }
        );
    } else {
        assert_eq!(ty, EntityType::Value(*result));
    }
    assert!(function.receiver.is_none());
}

fn direct_targets(function: &Function) -> Vec<FunctionId> {
    function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => Some(callee),
            _ => None,
        })
        .collect()
}

#[test]
fn unreachable_assertion_keeps_typed_fact_without_runtime_layout_in_both_unit_keys() {
    for mode in ENTRIES {
        let observed = observe(mode, UNREACHABLE, "inspect", 1);
        assert_eq!(observed.module().functions.len(), 1);
        assert_eq!(
            observed.module().types,
            [SsaTypeKind::Integer {
                bits: 32,
                signed: true
            }]
        );
        let function = observed.entry();
        assert!(function.blocks[0].parameters.is_empty());
        let operations = function
            .instructions
            .iter()
            .map(|instruction| &instruction.operation)
            .collect::<Vec<_>>();
        assert_eq!(
            operations,
            [&Operation::Constant(ScalarConstant::Integer(0))]
        );
        assert_eq!(function.blocks.len(), 1);
        let [result] = function.instructions[0].results.as_slice() else {
            panic!("one original zero result");
        };
        let crate::ssa::model::EntityId::Value(result) = result else {
            panic!("zero is a Value");
        };
        assert_eq!(
            function.blocks[0].terminator.as_ref().unwrap().kind,
            TerminatorKind::Return {
                values: vec![*result]
            }
        );
    }
}

#[test]
fn original_generic_roots_preserve_concrete_call_and_recursive_identity() {
    for mode in ENTRIES {
        for (root, bits) in [
            ("useInt", 32),
            ("useIntAgain", 32),
            ("useLong", 64),
            ("recursive", 32),
        ] {
            let observed = observe(mode, GENERICS, root, 8);
            assert_integer_signature(observed.module(), observed.entry(), bits, true);
            assert_eq!(observed.entry().blocks[0].parameters.len(), 1);
            let (_, unused) = observed.declaration("fun <T> unused(own input: T): T = input");
            assert!(
                observed
                    .module()
                    .functions
                    .iter()
                    .all(|function| function.origin != Origin::Source(unused))
            );
            if root == "recursive" {
                assert_eq!(observed.module().functions.len(), 2);
                let recurse =
                    observed.function("fun <T> recurse(own input: T): T = recurse(input)");
                assert_integer_signature(observed.module(), recurse, bits, false);
                assert_eq!(recurse.blocks[0].parameters.len(), 1);
                assert_eq!(direct_targets(observed.entry()), [recurse.id]);
                assert_eq!(direct_targets(recurse), [recurse.id]);
            } else {
                assert_eq!(observed.module().functions.len(), 3);
                let relay = observed.function("fun <T> relay(own input: T): T = identity(input)");
                let identity = observed.function("fun <T> identity(own input: T): T = input");
                assert_ne!(relay.id, identity.id);
                for function in [relay, identity] {
                    assert_integer_signature(observed.module(), function, bits, false);
                    assert_eq!(function.blocks[0].parameters.len(), 1);
                }
                assert_eq!(direct_targets(observed.entry()), [relay.id]);
                assert_eq!(direct_targets(relay), [identity.id]);
                assert!(direct_targets(identity).is_empty());
            }
        }
    }
}

#[test]
fn original_overload_roots_select_the_exact_declaration_not_the_shared_spelling() {
    let first = "fun <T> choose(own input: T): T = input";
    let second = "fun <T> choose(own input: T, fallback: Boolean): T = input";
    for mode in ENTRIES {
        for (root, expected, excluded, parameters) in
            [("one", first, second, 1), ("two", second, first, 2)]
        {
            let observed = observe(mode, OVERLOADS, root, 4);
            let (expected_id, expected_span) = observed.declaration(expected);
            let (excluded_id, excluded_span) = observed.declaration(excluded);
            assert_ne!(expected_id, excluded_id);
            assert_ne!(expected_span, excluded_span);
            assert_eq!(observed.module().functions.len(), 2);
            let target = observed.function(expected);
            assert!(
                observed
                    .module()
                    .functions
                    .iter()
                    .all(|function| function.origin != Origin::Source(excluded_span))
            );
            assert_eq!(direct_targets(observed.entry()), [target.id]);
            assert!(direct_targets(target).is_empty());
            assert_integer_signature(observed.module(), observed.entry(), 32, true);
            assert_integer_signature(observed.module(), target, 32, false);
            assert_eq!(observed.entry().blocks[0].parameters.len(), 1);
            assert_eq!(target.blocks[0].parameters.len(), parameters);
            let calls = observed
                .entry()
                .instructions
                .iter()
                .filter_map(|instruction| {
                    if let Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } = &instruction.operation
                    {
                        Some((callee, receiver, arguments))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let [(callee, receiver, arguments)] = calls.as_slice() else {
                panic!("one call");
            };
            assert_eq!(**callee, target.id);
            assert!(receiver.is_none());
            assert_eq!(arguments.len(), parameters);
            if parameters == 2 {
                let parameter = target.blocks[0].parameters[1];
                let EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                } = target.entity(parameter).unwrap().ty
                else {
                    panic!("original fallback parameter stays Borrow");
                };
                assert_eq!(observed.module().type_kind(ty), Some(&SsaTypeKind::Boolean));
            }
            // No comparison of FunctionIds across these independently lowered Programs.
        }
    }
}
