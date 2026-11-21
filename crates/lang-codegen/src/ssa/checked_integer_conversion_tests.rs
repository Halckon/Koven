//! Ordinary source: identity and cross-width conversion preserve the real LLVM pipeline.
use super::{
    unit_lower::lower_owned_unit_with_entry, unit_lower_test_support::parsed,
    verify::verify_program,
};
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

#[test]
fn single_conversion_identity_reaches_verified_llvm() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("fun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let source = sources.add_source("conversion.ko", &text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source, &file)];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let mut entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "entry"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == source
            });
        let entry = entries.next().expect("conversion entry exists").id();
        assert!(entries.next().is_none(), "conversion entry is unique");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let (program, _) = lower_owned_unit_with_entry(&unit, entry).unwrap();
        assert_verified_llvm(&program);
    }
}

#[test]
fn unit_conversion_identity_reaches_verified_llvm() {
    for target in ["Int", "Long"] {
        let root = format!("value.to{target}()");
        let text = format!("package p\nfun entry(value: Int): {target} = {root}");
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "p/conversion.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/conversion.ko",
            source,
            &file,
        )];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.diagnostics().is_empty());
        let owned = owned.validate().unwrap();
        let mut entries = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "entry"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == source
            });
        let entry = entries.next().expect("conversion entry exists").id();
        assert!(entries.next().is_none(), "conversion entry is unique");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        let (program, _) = lower_owned_unit_with_entry(&unit, entry).unwrap();
        assert_verified_llvm(&program);
    }
}

fn assert_verified_llvm(program: &super::model::Program) {
    verify_program(program).unwrap();
    let llvm = crate::llvm::render_verified_program(program).unwrap();
    diagnostic!("verified conversion LLVM: {llvm}");
    assert!(llvm.contains("icmp slt i128"));
    assert!(llvm.contains("icmp sgt i128"));
    assert!(llvm.contains("call void @koven.abort.report"));
}

pub(in crate::ssa) mod map_iteration_exits {
    //! The Map context participates in ordinary CFG and real jump/return dispatch.
    //! BranchExit retains the actual break/continue collections.
    //! Path-local roles restore independently of cumulative exit evidence; ordinary
    use crate::ssa::lower_frontend::map_iteration::Frame;
    use crate::ssa::lower_frontend::map_iteration::*;

    pub(in crate::ssa) struct Context {
        pub(in crate::ssa) plan: MapIterationOwnershipPlan,
        pub(in crate::ssa) kind: IterationExitKind,
        pub(in crate::ssa) body: BlockId,
        pub(in crate::ssa) frame: Frame,
        pub(in crate::ssa) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
        pub(in crate::ssa) cursor: ValueId,
        pub(in crate::ssa) cursor_type: SsaTypeId,
        pub(in crate::ssa) next: Option<ValueId>,
        pub(in crate::ssa) cleaned: bool,
        pub(in crate::ssa) returned: Option<ValueId>,
        pub(in crate::ssa) cleanup_start: Option<usize>,
        pub(in crate::ssa) conditional: Option<conditional::State>,
        pub(in crate::ssa) nesting: nested::Nesting,
        pub(in crate::ssa) resumed: Option<checked::Roles>,
    }

    pub(in crate::ssa) mod exit_shape {
        //! Bound the Map body to normal straight-line execution or one direct tail transfer.
        //! This describes capability only; the exit/action/owner facts still come from Phase 3.
        use crate::ssa::lower_frontend::map_iteration::*;

        use lang_frontend::type_checking::MapIterationDescriptor;

        pub(in crate::ssa) fn body_exit(
            parsed: &ParsedFile,
            _names: &NameResolution,
            typed: &TypedFile,
            _descriptor: MapIterationDescriptor,
            body: StatementId,
            span: Span,
        ) -> Result<IterationExitKind, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            if qualify::straight_statement(parsed, typed, body)? {
                return Ok(IterationExitKind::Fallthrough);
            }
            let node = parsed.ast().statements().get(body).map_err(|_| missing())?;
            let Statement::Block { elements } = node.payload() else {
                return Err(unsupported());
            };
            let Some((last, prefix)) = elements.split_last() else {
                return Err(unsupported());
            };
            for statement in prefix {
                if !qualify::straight_statement(parsed, typed, *statement)? {
                    return Err(unsupported());
                }
            }
            let node = parsed
                .ast()
                .statements()
                .get(*last)
                .map_err(|_| missing())?;
            let Statement::Expression { expression } = node.payload() else {
                return Err(unsupported());
            };
            let node = parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| missing())?;

            match node.payload() {
                Expression::Continue { .. } => Ok(IterationExitKind::Continue(*expression)),
                Expression::Break { .. } => Ok(IterationExitKind::Break(*expression)),
                // Operand evaluation and concrete return ABI belong to ordinary lower_return.
                // Complete FE has already checked the return contract and ownership delivery.
                Expression::Return { .. } => Ok(IterationExitKind::Return(*expression)),
                _ => Err(unsupported()),
            }
        }

        pub(in crate::ssa) fn point(
            kind: IterationExitKind,
            statement: StatementId,
            body: StatementId,
        ) -> DropPoint {
            match kind {
                IterationExitKind::Fallthrough => DropPoint::AfterStatement(body),
                IterationExitKind::Continue(expression)
                | IterationExitKind::Break(expression)
                | IterationExitKind::Return(expression) => DropPoint::ControlTransfer(expression),
                IterationExitKind::Exhaustion => DropPoint::LoopExit(statement),
            }
        }

        pub(in crate::ssa) fn terminal(kind: IterationExitKind) -> bool {
            matches!(
                kind,
                IterationExitKind::Break(_)
                    | IterationExitKind::Return(_)
                    | IterationExitKind::Exhaustion
            )
        }

        pub(in crate::ssa) fn cleanup(
            owned: &OwnershipCheckedFile,
            plan: &MapIterationOwnershipPlan,
            body: StatementId,
            kind: IterationExitKind,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let descriptor = plan.descriptor();
            let statement = descriptor.statement();
            if plan.exits().len() != 2 {
                return Err(missing());
            }
            for expected_kind in [kind, IterationExitKind::Exhaustion] {
                let exit = unique(
                    plan.exits()
                        .iter()
                        .filter(|exit| exit.kind() == expected_kind),
                )
                .ok_or_else(missing)?;
                if exit.point() != point(expected_kind, statement, body)
                    || owned.cleanup_conditions().get(exit.condition())
                        != Some(&CleanupCondition::Always)
                    || owned
                        .cleanup_steps()
                        .iter()
                        .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
                        .collect::<Vec<_>>()
                        != exit.actions()
                {
                    return Err(missing());
                }
                let map_actions = exit
                    .actions()
                    .iter()
                    .filter(|action| !matches!(action, IterationCleanupAction::Drop(_)))
                    .copied()
                    .collect::<Vec<_>>();
                let mut expected = Vec::new();
                if expected_kind != IterationExitKind::Exhaustion {
                    expected.extend([
                        IterationCleanupAction::EndBinding {
                            statement,
                            symbol: descriptor.value_binding(),
                        },
                        IterationCleanupAction::EndBinding {
                            statement,
                            symbol: descriptor.key_binding(),
                        },
                    ]);
                }
                if terminal(expected_kind) {
                    expected.extend([
                        IterationCleanupAction::FinishProvider(statement),
                        IterationCleanupAction::EndSource(statement),
                    ]);
                }
                if map_actions != expected {
                    return Err(missing());
                }
                // Local drops form one prefix. The Map sequence is contiguous, and
                // only the source owner may follow EndSource in this bounded shape.
                let start = exit
                    .actions()
                    .iter()
                    .position(|action| expected.first() == Some(action))
                    .ok_or_else(missing)?;
                if exit.actions()[..start]
                    .iter()
                    .any(|action| !matches!(action, IterationCleanupAction::Drop(_)))
                    || exit.actions().get(start..start + expected.len())
                        != Some(expected.as_slice())
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let tail = &exit.actions()[start + expected.len()..];
                if !terminal(expected_kind) && !tail.is_empty() {
                    return Err(missing());
                }
                if plan.source_access() == MapIterationSourceAccess::Shared && !tail.is_empty() {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            Ok(())
        }
    }
}
