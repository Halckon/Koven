use super::*;
use lang_frontend::{
    ownership_checking::{DropPoint, IterationCleanupAction, check_ownership},
    parser::{Expression, Statement},
    type_checking::{FinalTypeAdmissionError, complete_file_types},
};

#[test]
fn mutable_iteration_source_uses_reaching_writes() {
    let mut differences = Vec::new();
    let cases: &[(&str, &str, &[usize])] = &[
        (
            "eager binary exit propagates through the call",
            "var row = p; while (flag) { for (item in row) {}; take((if (true) { break } else 0) + 1); row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "eager binary rhs after break cannot continue",
            "var row = p; while (flag) { for (item in row) {}; (if (true) { break } else 0) + (if (flag) { row = if (outside()) p else p; continue } else 1) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "listOf inferred first element propagates exit",
            "var row = p; while (flag) { for (item in row) {}; listOf(if (true) { break } else 0); row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "listOf explicit element skips later write and continue",
            "var row = p; while (flag) { for (item in row) {}; listOf<Int>(if (true) { break } else 0, if (flag) { row = if (outside()) p else p; continue } else 1) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "runtime length is evaluated before initializer creation",
            "var row = p; while (flag) { for (item in row) {}; List<Int>(if (true) { break } else 0, if (flag) { row = if (outside()) p else p; continue } else init) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "reachable later constructor operand still writes",
            "var row = p; while (flag) { for (item in row) {}; listOf(0, if (flag) { row = if (outside()) p else p; 1 } else 1); break }; for (item in row) {}",
            &[0],
        ),
        (
            "unary operand propagates abrupt exit",
            "var row = p; while (flag) { for (item in row) {}; -(if (true) { break } else 0); row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "nominal constructor arguments keep source order",
            "var row = p; while (flag) { for (item in row) {}; Duo(second = if (true) { break } else 0, first = if (flag) { row = if (outside()) p else p; continue } else 1) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "mutable list insertAt skips its value after index exit",
            "var row = p; val xs = mutableListOf(1); while (flag) { for (item in row) {}; xs.insertAt(if (true) { break } else 0, if (flag) { row = if (outside()) p else p; continue } else 1) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "mutable map put skips its value after key exit",
            "var row = p; val xs = mutableMapOf<Int, Int>(); while (flag) { for (item in row) {}; xs.put(if (true) { break } else 0, if (flag) { row = if (outside()) p else p; continue } else 1) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "index expression preserves receiver exit",
            "var row = p; while (flag) { for (item in row) {}; (if (true) { break } else p)[if (flag) { row = if (outside()) p else p; continue } else 0] }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "map index preserves receiver exit",
            "var row = p; val xs = mapOf<Int, Int>(); while (flag) { for (item in row) {}; (if (true) { break } else xs)[if (flag) { row = if (outside()) p else p; continue } else 0] }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "list index write skips rhs after index exit",
            "var row = p; val xs = mutableListOf(1); while (flag) { for (item in row) {}; xs[if (true) { break } else 0] = if (flag) { row = if (outside()) p else p; continue } else 1 }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "map index write skips rhs after key exit",
            "var row = p; val xs = mutableMapOf<Int, Int>(); while (flag) { for (item in row) {}; xs[if (true) { break } else 0] = if (flag) { row = if (outside()) p else p; continue } else 1 }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "special method skips argument after receiver exit",
            "var row = p; val xs = mutableListOf(1); while (flag) { for (item in row) {}; (if (true) { break } else xs).add(if (flag) { row = if (outside()) p else p; continue } else 1) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "string interpolation propagates its first exit",
            "var row = p; while (flag) { for (item in row) {}; \"${if (true) { break } else 0}${if (flag) { row = if (outside()) p else p; continue } else 1}\" }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "short-circuit truth keeps the overwrite",
            "var row = if (outside()) p else p; if (flag && (if (flag) { row = p; true } else false)) { for (item in row) {} }; for (item in row) {}",
            &[0],
        ),
        (
            "negated truth keeps the overwrite on the false edge",
            "var row = if (outside()) p else p; if (!(flag && (if (flag) { row = p; true } else false))) {} else { for (item in row) {} }",
            &[0],
        ),
        (
            "when guard consumes the true edge",
            "var row = if (outside()) p else p; when { flag && (if (flag) { row = p; true } else false) -> { for (item in row) {} }; else -> {} }",
            &[0],
        ),
        (
            "when result preserves the true edge",
            "var row = if (outside()) p else p; if (when { flag -> { row = p; true }; else -> false }) { for (item in row) {} }",
            &[0],
        ),
        (
            "Boolean when subject keeps literal match correlation",
            "var row = if (outside()) p else p; when (flag && (if (flag) { row = p; true } else false)) { (true) -> { for (item in row) {} }; else -> {} }",
            &[0],
        ),
        (
            "required Elvis rhs preserves the true edge",
            "var row = if (outside()) p else p; val absent: Nothing? = null; if (absent ?: (if (flag) { row = p; true } else false)) { for (item in row) {} }",
            &[0],
        ),
        (
            "abrupt when membership operand cannot reach backedge",
            "var row = p; while (flag) { for (item in row) {}; when (1) { in (if (true) { break } else 0..<2) -> {}; else -> {} }; row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "while body consumes the true edge",
            "var row = if (outside()) p else p; while (flag && (if (flag) { row = p; true } else false)) { for (item in row) {}; break }; for (item in row) {}",
            &[0],
        ),
        (
            "abrupt call argument cannot reach the backedge",
            "var row = p; while (flag) { for (item in row) {}; row = identity(if (true) { break } else p); row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "generic call arguments preserve evaluation order",
            "var row = p; while (flag) { for (item in row) {}; pair(second = if (true) { break } else p, first = if (flag) { row = if (outside()) p else p; continue } else p) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "overload trials preserve abrupt argument prefix",
            "var row = p; while (flag) { for (item in row) {}; choose(if (true) { break } else p, { n -> val observed: Int = n }, if (flag) { row = if (outside()) p else p; continue } else p) }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "delayed generic lambda uses its source-position bindings",
            "var row = p; consume({ for (item in row) {} }, if (flag) { row = if (outside()) p else p; 1 } else 1); for (item in row) {}",
            &[0],
        ),
        (
            "winning overload lambda uses its source-position bindings",
            "var row = p; choose(p, { n -> val observed: Int = n; for (item in row) {} }, if (flag) { row = if (outside()) p else p; p } else p); for (item in row) {}",
            &[0],
        ),
        (
            "overwrite",
            "var row = if (outside()) p else p; row = p; for (item in row) {}",
            &[0],
        ),
        (
            "both branches overwrite",
            "var row = if (outside()) p else p; if (flag) { row = p } else { row = p }; for (item in row) {}",
            &[0],
        ),
        (
            "one branch writes pending",
            "var row = p; if (flag) { row = if (outside()) p else p } else { row = p }; for (item in row) {}",
            &[],
        ),
        (
            "one branch returns",
            "var row = p; if (flag) { row = if (outside()) p else p; return }; for (item in row) {}",
            &[0],
        ),
        (
            "fallthrough backedge",
            "var row = p; while (flag) { for (item in row) {}; row = if (outside()) p else p }",
            &[],
        ),
        (
            "continue backedge",
            "var row = p; while (flag) { for (item in row) {}; row = if (outside()) p else p; continue }",
            &[],
        ),
        (
            "transitive backedge retains independent provider",
            "var row = p; var next = p; while (flag) { for (item in row) {}; for (item in p) {}; row = next; next = if (outside()) p else p }",
            &[1],
        ),
        (
            "break does not reach header",
            "var row = p; while (flag) { for (item in row) {}; row = if (outside()) p else p; break }; for (item in row) {}",
            &[0],
        ),
        (
            "unconditional loop restores",
            "var row = if (outside()) p else p; loop { row = p; break }; for (item in row) {}",
            &[0],
        ),
        (
            "unreachable backedge",
            "var row = p; while (flag) { for (item in row) {}; break; row = if (outside()) p else p; continue }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "known branch break",
            "var row = p; while (flag) { for (item in row) {}; if (true) { break }; row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "abrupt assignment rhs cannot reach backedge",
            "var row = p; while (flag) { for (item in row) {}; row = if (true) { break } else p; row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "non-exiting nested loop",
            "var row = p; while (flag) { for (item in row) {}; loop {}; row = if (outside()) p else p }; for (item in row) {}",
            &[0, 1],
        ),
        (
            "when overwrites on both paths",
            "var row = if (outside()) p else p; when { flag -> { row = p }; else -> { row = p } }; for (item in row) {}",
            &[0],
        ),
        (
            "required short-circuit rhs overwrites",
            "var row = if (outside()) p else p; true && (if (flag) { row = p; true } else { row = p; false }); for (item in row) {}",
            &[0],
        ),
        (
            "skipped short-circuit rhs cannot taint",
            "var row = p; false && (if (flag) { row = if (outside()) p else p; true } else { false }); for (item in row) {}",
            &[0],
        ),
        (
            "nested loop break",
            "var row = p; while (flag) { while (flag) { row = if (outside()) p else p; break }; for (item in row) {} }",
            &[],
        ),
        (
            "unknown when condition can write",
            "var row = p; when { outside() -> { row = if (outside()) p else p }; else -> {} }; for (item in row) {}",
            &[],
        ),
        (
            "unreached when alternative cannot write",
            "var row = p; when (flag) { true, false, (if (flag) { row = if (outside()) p else p; true } else true) -> {}; else -> {} }; for (item in row) {}",
            &[0],
        ),
        (
            "constant logical result bypasses branch",
            "var row = p; if (false && true) { row = if (outside()) p else p }; for (item in row) {}",
            &[0],
        ),
        (
            "immutable write cannot restore a provider",
            "val row = if (outside()) p else p; row = p; for (item in row) {}",
            &[],
        ),
    ];
    for &(case, body, expected) in cases {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun identity(own input: List<Int>): List<Int> = input
             fun take(value: Int): Unit {{}}
             class Duo(val first: Int, val second: Int)
             fun <T> pair(first: T, second: T): Unit {{}}
             fun <T> consume(callback: () -> Unit, value: T): Unit {{}}
             fun choose(first: List<Int>, action: (Int) -> Unit, last: List<Int>): Unit {{}}
             fun choose(first: List<Int>, action: (String) -> Unit, last: List<Int>): Unit {{}}
             fun test(p: List<Int>, flag: Boolean, init: (Int) -> Int): Unit {{ {body} }}"
        );
        let (_, file) = parsed(&mut sources, "mutable-provider.ko", &text);
        let (mut names_env, types_env) = standard_environments();
        names_env.declare_function("outside").unwrap();
        let names = resolve_names(&sources, &file, &names_env).unwrap();
        assert!(
            names.diagnostics().is_empty(),
            "{case}: {:?}",
            names.diagnostics()
        );
        let typed = check_types(&sources, &file, &names, &types_env).unwrap();
        assert!(
            typed.diagnostics().is_empty(),
            "{case}: {:?}",
            typed.diagnostics()
        );
        let mut loop_statements = file
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::For { .. }))
            .collect::<Vec<_>>();
        loop_statements.sort_unstable_by_key(|(_, node)| node.span().start());
        let plans = loop_statements
            .iter()
            .enumerate()
            .filter_map(|(ordinal, &(id, _))| typed.sequential_iteration(id).map(|_| ordinal))
            .collect::<Vec<_>>();
        if plans != expected {
            differences.push((case, plans, expected));
        }
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
            complete_file_types(&sources, &file, &names, &types_env, &typed)
        else {
            panic!("{case}: expected source diagnostic");
        };
        assert_eq!(
            diagnostics.len(),
            text.matches("outside()").count(),
            "{case}: {diagnostics:?}"
        );
        for diagnostic in &diagnostics {
            assert_eq!(diagnostic.code().to_string(), "L0165", "{case}");
            assert!(matches!(
                sources.slice(diagnostic.primary_span()),
                Ok("outside" | "outside()")
            ));
        }
    }
    assert!(differences.is_empty(), "{differences:?}");
}

#[test]
fn single_pending_iteration_sources_keep_types_without_publishing_provider_plans() {
    let mut premature = Vec::new();
    for (provider, binding, alias) in [
        ("List<Int>", "item", false),
        ("List<Int>", "item", true),
        ("Map<Int, Int>", "(key, value)", false),
        ("Map<Int, Int>", "(key, value)", true),
        ("IntRange", "item", false),
        ("IntRange", "item", true),
        ("View<Int>", "item", false),
    ] {
        let pending = "if (outside()) p else p";
        let (setup, iteration_source) = if alias {
            (format!("val provider = ({pending});"), "provider")
        } else {
            (String::new(), pending)
        };
        let text = format!(
            "fun test(p: {provider}): Unit {{
                {setup} for ({binding} in {iteration_source}) {{}}
                for ({binding} in p) {{}}
            }}"
        );
        let mut sources = SourceMap::new();
        let (_, file) = parsed(&mut sources, "pending-iteration.ko", &text);
        let (mut names_env, types_env) = standard_environments();
        names_env.declare_function("outside").unwrap();
        let names = resolve_names(&sources, &file, &names_env).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &file, &names, &types_env).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let pending_ty = typed.expression_type(expression(&sources, &file, iteration_source));
        let is_map = provider.starts_with("Map");
        let plan_ids = if is_map {
            typed
                .map_iterations()
                .iter()
                .map(|plan| plan.source())
                .collect::<Vec<_>>()
        } else {
            typed
                .sequential_iterations()
                .iter()
                .map(|plan| plan.source())
                .collect()
        };
        let plan_sources = plan_ids
            .iter()
            .map(|&id| {
                sources
                    .slice(file.ast().expressions().get(id).unwrap().span())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        if plan_sources != ["p"] {
            premature.push((provider, alias, plan_sources.join(", ")));
        }
        assert_eq!(pending_ty, typed.expression_type(*plan_ids.last().unwrap()));
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) =
            complete_file_types(&sources, &file, &names, &types_env, &typed)
        else {
            panic!("{provider}: final admission must return a source diagnostic");
        };
        let [diagnostic] = diagnostics.as_slice() else {
            panic!("{provider}: {diagnostics:?}");
        };
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert!(matches!(
            sources.slice(diagnostic.primary_span()),
            Ok("outside" | "outside()")
        ));
        if is_map || provider == "IntRange" {
            let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
            let owned_sources = if is_map {
                owned
                    .map_iterations()
                    .iter()
                    .map(|plan| plan.descriptor().source())
                    .collect::<Vec<_>>()
            } else {
                owned
                    .iterations()
                    .iter()
                    .map(|plan| plan.descriptor().source())
                    .collect()
            };
            let stable = *plan_ids.last().unwrap();
            if is_map {
                // Existing ownership diagnostics can suppress the entire Map table.
                assert!(owned_sources.iter().all(|&source| source == stable));
            } else {
                assert_eq!(owned_sources, [stable], "alias={alias}");
                let source = expression(&sources, &file, iteration_source);
                let (pending_statement, _) = file
                    .ast()
                    .statements()
                    .iter()
                    .find(|(_, node)| {
                        matches!(node.payload(), Statement::For { source: candidate, .. } if *candidate == source)
                    })
                    .unwrap();
                assert!(
                    !owned.cleanup_steps().iter().any(|(_, action)| {
                        matches!(
                            action,
                            IterationCleanupAction::EndBinding { statement, .. }
                            | IterationCleanupAction::EndElement(statement)
                            | IterationCleanupAction::FinishProvider(statement)
                            | IterationCleanupAction::EndSource(statement)
                            if *statement == pending_statement
                        )
                    }),
                    "{:?}",
                    owned.cleanup_steps()
                );
            }
        }
    }
    assert!(premature.is_empty(), "{premature:?}");
}

#[test]
fn resolved_forward_constant_keeps_single_iteration_plan() {
    use lang_frontend::{
        name_resolution::{ReferenceTarget, SourceUnitKey, SymbolKind},
        parser::{ForBinding, NameMarker},
        type_checking::{
            ConstValue, IterationProvider, ParameterMode, SequentialContainerKind,
            UnitSequentialIterationBinding, complete_compilation_unit_types,
        },
    };

    let text = "const val FIRST = SECOND
         const val SECOND = true
         fun test(p: List<Int>): Unit { for (item in if (FIRST) p else p) {} }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "forward-constant.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let ast = file.ast();
    let (ne, te) = standard_environments();
    for (rooted, input) in [
        (false, SourceUnitInput::memory_document(source, &file)),
        (
            true,
            SourceUnitInput::new("root", "forward-constant.ko", source, &file),
        ),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        assert!(match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "forward-constant.ko",
            _ => false,
        });
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert!(typed.diagnostics().is_empty());
        assert!(
            complete_compilation_unit_types(&sources, &inputs, &names, &te, &typed)
                .unwrap()
                .is_same_analysis(&typed)
        );
        let typed = typed.validate_constants().unwrap();
        let raw = typed.types();
        let [iteration] = raw.sequential_iterations() else {
            panic!("one genuine typed iteration descriptor")
        };
        let unit = entry.id();
        let qualify = |id| UnitExpressionId::new(unit, id);
        assert_eq!(iteration.statement().source_unit(), unit);
        let Statement::For {
            source: iterated,
            binding: ForBinding::Name(NameMarker::Present(binder)),
            ..
        } = ast
            .statements()
            .get(iteration.statement().statement())
            .unwrap()
            .payload()
        else {
            panic!("the original named for")
        };
        assert_eq!(iteration.source(), qualify(*iterated));
        assert_eq!(
            iteration.provider(),
            IterationProvider::Sequential(SequentialContainerKind::List)
        );
        assert_eq!(iteration.delivery(), ParameterMode::Borrow);
        let int = raw.types().builtin(BuiltinType::Int).unwrap();
        assert_eq!(iteration.element_type(), int);
        assert_eq!(
            raw.expression_type(iteration.source()),
            Some(iteration.source_type())
        );
        let UnitSequentialIterationBinding::Name(item) = *iteration.binding() else {
            panic!("actual named iteration binding")
        };
        assert_eq!(item.source_unit(), unit);
        let local = names.names().source_units()[0].resolution();
        let symbol = local.symbols().get(item.symbol().index()).unwrap();
        assert_eq!(symbol.kind(), SymbolKind::ForBinding);
        assert_eq!(symbol.span(), *binder);
        assert_eq!(sources.slice(*binder), Ok("item"));
        assert_eq!(raw.symbol_type(item), Some(int));

        let Expression::If { condition, .. } = ast.expressions().get(*iterated).unwrap().payload()
        else {
            panic!("the original conditional iterator source")
        };
        let condition_span = ast.expressions().get(*condition).unwrap().span();
        assert_eq!(sources.slice(condition_span), Ok("FIRST"));
        let declarations = typed.constants().declarations();
        let first = declarations
            .iter()
            .find(|d| sources.slice(d.declaration_span()) == Ok("FIRST"))
            .unwrap();
        let second = declarations
            .iter()
            .find(|d| sources.slice(d.declaration_span()) == Ok("SECOND"))
            .unwrap();
        assert_ne!(first.symbol(), second.symbol());
        assert_eq!(first.value(), &ConstValue::Boolean(true));
        assert_eq!(second.value(), first.value());
        let condition = typed
            .constants()
            .uses()
            .iter()
            .find(|read| read.expression() == qualify(*condition))
            .unwrap();
        assert_eq!(condition.target(), first.symbol());
        assert_eq!(condition.value(), first.value());

        let p = local
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "p")
            .unwrap();
        let iterated_span = ast.expressions().get(*iterated).unwrap().span();
        assert_eq!(
            local
                .references()
                .iter()
                .filter(
                    |reference| reference.span().start() >= iterated_span.start()
                        && reference.span().end() <= iterated_span.end()
                        && reference.target() == &ReferenceTarget::Symbol(p.id())
                )
                .count(),
            2
        );
    }
}

#[test]
fn pending_inner_loop_jumps_do_not_end_the_outer_provider() {
    let mut sources = SourceMap::new();
    let (_, file) = parsed(
        &mut sources,
        "nested-pending-loop.ko",
        "fun test(range: IntRange, flag: Boolean, stop: Boolean): Unit {
             for (outer in range) {
                 for (inner in if (outside()) range else range) {
                     if (flag) { break }
                     if (stop) { return }
                     continue
                 }
             }
         }",
    );
    let (mut names_env, types_env) = standard_environments();
    names_env.declare_function("outside").unwrap();
    let names = resolve_names(&sources, &file, &names_env).unwrap();
    let typed = check_types(&sources, &file, &names, &types_env).unwrap();
    let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
    let [outer] = owned.iterations() else {
        panic!("expected only the outer provider: {:?}", owned.iterations());
    };
    let outer_statement = outer.descriptor().statement();
    for (jump, node) in file.ast().expressions().iter() {
        let ended = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::FinishProvider(statement)
                    if *point == DropPoint::ControlTransfer(jump) =>
                {
                    Some(*statement)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        match node.payload() {
            Expression::Break { .. } | Expression::Continue { .. } => {
                assert!(ended.is_empty(), "{:?}", owned.cleanup_steps());
                assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
                    *point == DropPoint::ControlTransfer(jump)
                        && matches!(action, IterationCleanupAction::EndElement(_))
                }));
            }
            Expression::Return { .. } => assert_eq!(ended, [outer_statement]),
            _ => {}
        }
    }
}

#[test]
fn single_pending_iteration_bindings_do_not_authorize_nested_providers() {
    for (provider, setup, nested) in [
        (
            "List<List<Int>>",
            "",
            "for (row in if (outside()) p else p) { for (item in row) {} }",
        ),
        (
            "Map<Int, List<Int>>",
            "",
            "for ((key, row) in if (outside()) p else p) { for (item in row) {} }",
        ),
        (
            "List<Rows>",
            "",
            "for ((row) in if (outside()) p else p) { for (item in row) {} }",
        ),
        (
            "Rows",
            "val (row) = if (outside()) p else p",
            "for (item in row) {}",
        ),
        (
            "List<Int>",
            "var row = p; row = if (outside()) p else p",
            "for (item in row) {}",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "value class Rows(val row: List<Int>)
             fun test(p: {provider}, stable: List<Int>): Unit {{
                 {setup}
                 {nested}
                 for (item in stable) {{}}
             }}"
        );
        let (_, file) = parsed(&mut sources, "pending-binding.ko", &text);
        let (mut names_env, types_env) = standard_environments();
        names_env.declare_function("outside").unwrap();
        let names = resolve_names(&sources, &file, &names_env).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &file, &names, &types_env).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let stable = expression(&sources, &file, "stable");
        let row = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok("row"))
            .last()
            .unwrap()
            .0;
        assert_eq!(typed.expression_type(row), typed.expression_type(stable));
        assert!(typed.map_iterations().is_empty());
        let [plan] = typed.sequential_iterations() else {
            panic!("{provider}: {:?}", typed.sequential_iterations());
        };
        assert_eq!(plan.source(), stable);
    }
}
