//! Real Resource Map continuations: lexical owner lifetime is independent of statement shape.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
pub(super) mod borrowed {
    //! Newly admitted shared Resource Continue: callee provider lease and caller owner stay distinct.
    use super::super::identity::Input;
    use crate::ssa::{
        map_provider_exits_pipeline_tests::{
            fixture::{Failure, failure, need},
            query_graph::{Graph, Uses, loan, result, value},
            query_inputs::one,
        },
        model::*,
    };

    pub(super) fn check(
        program: &Program,
        input: &Input,
        parsed: &lang_frontend::parser::ParsedFile,
    ) -> Result<(), Failure> {
        let module = one(program.modules.iter(), "one original module")?;
        let scan = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(input.scan)),
            "actual Shared scan",
        )?;
        let g = Graph::new(scan)?;
        let [anchor] = g.block(scan.entry_block().unwrap())?.parameters.as_slice() else {
            return Err(failure("single original caller loan", scan));
        };
        need(
            matches!(
                scan.entity(*anchor).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            ),
            "Shared parameter ABI",
            anchor,
        )?;
        let op = |predicate: fn(&Operation) -> bool| {
            one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| predicate(&i.operation)),
                "unique Shared provider op",
            )
        };
        let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
        let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
        let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
        let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
        let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
        need(
            [first, guard, key, val]
                .iter()
                .all(|i| i.origin == Origin::Source(input.for_span)),
            "original Shared For origins",
            input.for_span,
        )?;
        let next_span = parsed
            .ast()
            .expressions()
            .get(input.continued.unwrap())
            .unwrap()
            .span();
        need(
            next.origin == Origin::Source(next_span),
            "actual Shared Continue Next origin",
            next,
        )?;
        let Operation::MapIterationFirst { source } = first.operation else {
            unreachable!()
        };
        let birth = g.definition(EntityId::Loan(source))?;
        let Operation::SharedReborrow { source: caller } = birth.operation else {
            return Err(failure("provider owns its reborrow", birth));
        };
        let mut roots = super::graph::transport(&g, EntityId::Loan(caller), *anchor)?;
        loop {
            let before = roots.len();
            for (_, edge) in &g.edges {
                for (&argument, &parameter) in
                    edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
                {
                    if roots.contains(&argument) {
                        roots.insert(parameter);
                    }
                }
            }
            if before == roots.len() {
                break;
            }
        }
        for &root in &roots {
            super::graph::transport(&g, root, *anchor)?;
        }
        let Operation::MapIterationHasNext {
            source: hs,
            cursor: hc,
        } = guard.operation
        else {
            unreachable!()
        };
        let Operation::MapIterationKeyLoan {
            source: bs,
            cursor: bc,
        } = key.operation
        else {
            unreachable!()
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(guard.block)?.kind
        else {
            return Err(failure("provider branch", guard));
        };
        need(
            *condition == value(guard)?
                && when_true.target == key.block
                && val.block == key.block
                && next.block == key.block
                && val.operation
                    == Operation::MapIterationValueLoan {
                        source: bs,
                        cursor: bc,
                    }
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    },
            "actual provider guard/K/V/Next",
            (guard, key, val, next),
        )?;
        g.incoming_from(guard.block, &[first.block, key.block])?;
        g.incoming_from(key.block, &[guard.block])?;
        g.incoming_from(when_false.target, &[guard.block])?;
        let entry = g.branch(first.block)?;
        let back = g.branch(key.block)?;
        need(
            g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
                && g.transported(entry, result(first)?)? == EntityId::Value(hc)
                && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
                && back.target == guard.block
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?,
            "all Shared provider incoming columns",
            (entry, back),
        )?;
        let xs = g.transported(when_false, EntityId::Loan(hs))?;
        let end = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
            ),
            "unique own-provider End",
        )?;
        need(
            end.block == when_false.target && !roots.contains(&xs),
            "End only callee lease on exhaustion",
            end,
        )?;
        let read=one(g.instructions.iter().copied().filter(|i|matches!(&i.operation,Operation::DirectCall{arguments,receiver:None,..} if arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual Shared K/V read")?;
        let Operation::DirectCall { callee, .. } = read.operation else {
            unreachable!()
        };
        need(
            module
                .function(callee)
                .is_some_and(|f| f.origin == Origin::Source(input.read)),
            "original read callee",
            read,
        )?;
        let mut uses = Uses::default();
        for cursor in [
            result(first)?,
            EntityId::Value(hc),
            EntityId::Value(bc),
            result(next)?,
            g.transported(when_false, EntityId::Value(hc))?,
        ] {
            uses.protect(cursor);
        }
        uses.permit(EntityId::Value(hc), guard);
        for i in [key, val, next] {
            uses.permit(EntityId::Value(bc), i);
        }

        for &root in &roots {
            uses.protect(root);
        }
        uses.permit(EntityId::Loan(caller), birth);
        for entity in [EntityId::Loan(hs), EntityId::Loan(bs), xs] {
            for root in super::graph::transport(&g, entity, EntityId::Loan(source))? {
                uses.protect(root);
            }
        }
        uses.permit(EntityId::Loan(source), first);
        uses.permit(EntityId::Loan(hs), guard);
        for i in [key, val, next] {
            uses.permit(EntityId::Loan(bs), i);
        }
        uses.permit(xs, end);
        let mut component_ends = vec![];
        for component in [key, val] {
            let entity = result(component)?;
            uses.protect(entity);
            uses.permit(entity, read);
            let end = one(
                g.instructions.iter().copied().filter(|i| {
                    i.operation
                        == Operation::BorrowEnd {
                            loan: loan(component).unwrap(),
                        }
                }),
                "component End",
            )?;
            uses.permit(entity, end);
            component_ends.push(end);
        }
        need(
            component_ends.iter().all(|i| i.block == next.block)
                && g.position(read)? < g.position(component_ends[1])?
                && g.position(component_ends[1])? < g.position(component_ends[0])?
                && g.position(component_ends[0])? < g.position(next)?,
            "Shared read/value End/key End/Next",
            &component_ends,
        )?;
        need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::BorrowEnd{loan} if roots.contains(&EntityId::Loan(loan)))) && !g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if matches!(scan.entity(EntityId::Value(owner)).unwrap().ty,EntityType::Value(t) if matches!(module.type_kind(t),Some(SsaTypeKind::MapContainer{..}))))),"callee preserves caller loan and never drops its Map",scan.id)?;
        uses.check(&g, None)?;
        let expected = roots
            .iter()
            .copied()
            .chain([
                EntityId::Loan(source),
                EntityId::Loan(hs),
                EntityId::Loan(bs),
                xs,
            ])
            .collect();
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &[scan.entity(*anchor).unwrap().ty.semantic_type()],
            &expected,
        )?;
        // Caller owns separate per-call loans. Both end after the actual scan returns;
        // any post-scan MapPut must follow the first loan End on that exact owner.
        let caller = one(
            module.functions.iter().filter(|f| {
                f.instructions
                    .iter()
                    .filter(
                        |i| matches!(i.operation,Operation::DirectCall{callee,..} if callee==scan.id),
                    )
                    .count()
                    == 2
            }),
            "actual caller has populated and empty calls",
        )?;
        let cg = Graph::with_abort(caller)?;
        let calls = cg
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation,Operation::DirectCall{callee,..} if callee==scan.id))
            .collect::<Vec<_>>();
        let mut original = None;
        for call in &calls {
            let Operation::DirectCall {
                arguments,
                receiver: None,
                ..
            } = &call.operation
            else {
                unreachable!()
            };
            let [EntityId::Loan(argument)] = arguments.as_slice() else {
                return Err(failure("caller Borrow delivery", call));
            };
            let birth = cg.definition(EntityId::Loan(*argument))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("caller scoped loan", birth));
            };
            let root = cg.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("caller original root", root));
            };
            let ended = one(
                cg.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: *argument }),
                "caller loan end",
            )?;
            need(
                birth.block == call.block
                    && ended.block == call.block
                    && cg.position(birth)? < cg.position(call)?
                    && cg.position(call)? < cg.position(ended)?,
                "caller loan only ends after scan return",
                call,
            )?;
            if original.is_none() {
                original = Some((owner, ended));
            }
        }
        let (owner, ended) = original.unwrap();
        let puts = cg
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
            .collect::<Vec<_>>();
        let mut current = owner;
        for put in puts {
            let Operation::MapPut { owner: actual, .. } = put.operation else {
                unreachable!()
            };
            need(
                actual == owner
                    && put.block == ended.block
                    && cg.position(ended)? < cg.position(put)?,
                "post-scan put uses caller owner after loan End",
                put,
            )?;
            current = value(put)?;
        }
        let sizes = cg
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
            .collect::<Vec<_>>();
        need(sizes.len() == 1, "actual caller post-use size", &sizes)?;
        need(
            matches!(sizes[0].operation,Operation::MapSize{owner:EntityId::Value(v)} if v==current)
                && sizes[0].block == ended.block
                && cg.position(ended)? < cg.position(sizes[0])?,
            "caller size after its own loan End",
            sizes[0],
        )?;
        Ok(())
    }
}
pub(super) mod graph;
use super::super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: &'static str,
    pub owned: bool,
    pub source: String,
    pub golden: String,
}

pub(crate) fn sources() -> Vec<Case> {
    let base =
        include_str!("../../../../tests/fixtures/map-provider-resource-0331/map-resource.ko");
    let mut cases = Vec::new();
    for (label, prefix, suffix, nonempty, empty) in [
        (
            "original-extra",
            "",
            "println(\"exhausted\"); println(\"extra-continuation\")",
            "exhausted\nextra-continuation\n",
            "exhausted\nextra-continuation\n",
        ),
        ("no-continuation", "", "", "", ""),
        (
            "prefix-and-calls",
            "println(\"before-for\");",
            "println(\"first\"); println(\"second\"); println(\"third\")",
            "first\nsecond\nthird\n",
            "first\nsecond\nthird\n",
        ),
        (
            "later-source-use",
            "",
            "readMap(source); println(\"tail\")",
            "later-borrow\ntail\n",
            "later-borrow\ntail\n",
        ),
        (
            "cfg-then-source-use",
            "",
            "if (source.size == 0) { println(\"zero\") } else { println(\"two\") }; readMap(source); println(\"tail\")",
            "two\nlater-borrow\ntail\n",
            "zero\nlater-borrow\ntail\n",
        ),
    ] {
        let mut source = base
            .replace(
                "  for ((key, value) in source)",
                &format!("  {prefix} for ((key, value) in source)"),
            )
            .replace("println(\"exhausted\")", suffix)
            + "\nfun readMap(source: Map<String, Resource>): Unit { println(\"later-borrow\") }\n";
        if label == "original-extra" {
            source = include_str!("../../../../tests/fixtures/map-provider-resource-0331/boundary-extra-continuation.ko").to_owned();
        }
        let before = if prefix.is_empty() {
            ""
        } else {
            "before-for\n"
        };
        let golden = format!(
            "nonempty\n{before}z\nvalue-borrow\na\nvalue-borrow\n{nonempty}resource-drop\nresource-drop\nempty\n{before}{empty}entry-end\n"
        );
        cases.push(Case {
            label,
            owned: true,
            source,
            golden,
        });
    }
    cases
}

pub(super) fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    observe_with(case, unit, |_, _, _, _| Ok(()))
}
pub(super) fn observe_with(
    case: &Case,
    unit: bool,
    inspect: impl FnOnce(
        &crate::ssa::model::Program,
        &super::identity::Input,
        &SourceMap,
        &lang_frontend::parser::ParsedFile,
    ) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    let input = super::identity::ast(&sources, &parsed, case.owned)?;
    let mut report = Report::default();
    let outcome = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| {
            report.query(
                "actual Resource FE scope",
                &super::identity::unit(&input, &parsed, names, typed, owned),
            );
        },
    );
    diagnostic!(
        "RESOURCE CONTINUATION case={} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        outcome.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = outcome?;
    assert_diagnostic!(report.passed(), "{report:#?}");
    if case.owned {
        graph::check(&program, &input, &sources, &parsed)?;
    } else {
        borrowed::check(&program, &input, &parsed)?;
    }
    inspect(&program, &input, &sources, &parsed)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e))?;
    Ok(())
}

#[test]
fn resource_continuations_reach_both_verified_pipelines() {
    let results = sources()
        .iter()
        .flat_map(|case| [false, true].map(|unit| (case.label, unit, observe(case, unit))))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 10);
    assert_diagnostic!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}

pub(in crate::ssa) mod legacy_instance_witnesses {
//! Stage A: unchanged legacy fixtures through strict MemoryDocument and rooted Unit.
//! Selected-root Programs do not prove whole-file/shared-mapper/multi-root equivalence.
//! Keep all original legacy tests and native contracts; these witnesses do not execute native code.
use crate::test_support::diagnostics::assert_text_eq;
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
    assert_text_eq!(render_program(&program), render_program(&repeated));
    // Full module verification with no native entry wrapper. These are parameterized roots.
    assert_text_eq!(
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
}
