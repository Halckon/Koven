//! Original owned-source consumers: same-analysis Unit identities and exact cleanup.
//! The unchanged SSA observers prove the former private phi/condition obligations.
use crate::ssa::model::{EntityType, Operation, Origin, Program};
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SourceUnitKey, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, UnitDropFact, UnitDropPoint,
        UnitDropTarget, UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitIterationOwnershipPlan, UnitIterationSourceAccess, UnitLoanTarget,
        check_compilation_unit_ownership, owned_compilation_unit_view,
    },
    parser::{
        Expression, ForBinding, Item, NameMarker, ParameterModeMarker, ParsedFile, Statement,
        parse_file,
    },
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, IterationProvider, ParameterMode,
        SequentialContainerKind, UnitCallTarget, UnitExpressionId, UnitStatementId,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ssa::lower_frontend_tests) enum Control {
    Continue,
    Return,
}

pub(in crate::ssa::lower_frontend_tests) struct Expected<'a> {
    pub(in crate::ssa::lower_frontend_tests) function: &'a str,
    pub(in crate::ssa::lower_frontend_tests) live: bool,
    pub(in crate::ssa::lower_frontend_tests) container: SequentialContainerKind,
    pub(in crate::ssa::lower_frontend_tests) own: bool,
}

pub(in crate::ssa::lower_frontend_tests) struct Origins {
    pub(in crate::ssa::lower_frontend_tests) seed: Option<Span>,
    pub(in crate::ssa::lower_frontend_tests) control: Span,
}

struct Facts<'a> {
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    names: &'a ValidatedCompilationUnitNames,
    typed: &'a CompilationUnitTypes,
    owned: &'a CompilationUnitOwnership,
    source: SourceUnitId,
}

fn one<T>(values: &[T]) -> &T {
    let [value] = values else {
        panic!("one exact original identity, got {}", values.len());
    };
    value
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

impl Facts<'_> {
    fn expression(&self, id: UnitExpressionId) -> &Expression {
        assert_eq!(id.source_unit(), self.source);
        self.parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
            .payload()
    }

    fn span(&self, id: UnitExpressionId) -> Span {
        assert_eq!(id.source_unit(), self.source);
        self.parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
            .span()
    }

    fn statement_span(&self, id: UnitStatementId) -> Span {
        assert_eq!(id.source_unit(), self.source);
        self.parsed
            .ast()
            .statements()
            .get(id.statement())
            .unwrap()
            .span()
    }

    fn symbol_span(&self, id: UnitSymbolId) -> Span {
        assert_eq!(id.source_unit(), self.source);
        let symbols = one(self.names.names().source_units())
            .resolution()
            .symbols();
        let symbols = symbols
            .iter()
            .filter(|symbol| symbol.id() == id.symbol())
            .collect::<Vec<_>>();
        let symbol = one(&symbols);
        assert!(!symbol.is_synthetic());
        symbol.span()
    }

    fn reference(&self, id: UnitExpressionId) -> UnitSymbolId {
        assert!(matches!(self.expression(id), Expression::Name));
        let references = self
            .names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == self.source
                    && reference.span() == self.span(id)
                    && reference.namespace() == Some(Namespace::Value)
            })
            .collect::<Vec<_>>();
        let UnitReferenceTarget::Symbol(symbol) = one(&references).target() else {
            panic!("the original Name resolves to its own source-qualified symbol");
        };
        *symbol
    }

    fn declaration(&self, name: &str) -> (DeclarationId, Span) {
        let input = one(self.names.names().index().source_units());
        let declarations = self
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == name
                    && declaration.kind() == SymbolKind::Function
                    && declaration.source_unit() == self.source
                    && declaration.package() == input.package()
                    && declaration.name_span().source_id() == self.parsed.source_id()
                    && self.parsed.roots().contains(&declaration.root())
            })
            .collect::<Vec<_>>();
        let declaration = one(&declarations);
        let node = self.parsed.ast().items().get(declaration.root()).unwrap();
        let Item::Function {
            name: NameMarker::Present(marker),
            ..
        } = node.payload()
        else {
            panic!("the exact original function AST root");
        };
        assert_eq!(*marker, declaration.name_span());
        assert_eq!(self.sources.slice(*marker).unwrap(), name);
        assert_eq!(node.span().source_id(), input.source_id());
        (declaration.id(), node.span())
    }

    fn variable(&self, symbol: UnitSymbolId) -> UnitExpressionId {
        let marker = self.symbol_span(symbol);
        let variables = self
            .parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if *name == marker => Some(UnitExpressionId::new(self.source, *initializer)),
                _ => None,
            })
            .collect::<Vec<_>>();
        *one(&variables)
    }

    fn drop(&self, point: UnitDropPoint, target: UnitDropTarget, origin: Span) -> UnitDropFact {
        let drops = self
            .owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == point && fact.target() == target)
            .collect::<Vec<_>>();
        let fact = **one(&drops);
        assert_eq!(fact.value_origin(), origin);
        fact
    }

    fn named_drop(&self, point: UnitDropPoint, symbol: UnitSymbolId) -> Action {
        Action::Drop(self.drop(
            point,
            UnitDropTarget::Named(symbol),
            self.symbol_span(symbol),
        ))
    }

    fn bindings(&self, plan: &UnitIterationOwnershipPlan) {
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        assert_eq!(statement.source_unit(), self.source);
        assert_eq!(self.typed.sequential_iteration(statement), Some(descriptor));
        assert_eq!(self.owned.iteration(statement), Some(plan));
        assert_eq!(descriptor.source().source_unit(), self.source);
        assert_eq!(
            self.typed.expression_type(descriptor.source()),
            Some(descriptor.source_type())
        );
        assert_eq!(descriptor.delivery(), ParameterMode::Borrow);
        let node = self
            .parsed
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap();
        let Statement::For {
            binding, source, ..
        } = node.payload()
        else {
            panic!("descriptor identifies the actual original For AST");
        };
        assert_eq!(descriptor.source().expression(), *source);
        let ForBinding::Name(NameMarker::Present(marker)) = binding else {
            panic!("the original named element binding");
        };
        let symbols = descriptor.binding().symbols().collect::<Vec<_>>();
        let symbol = *one(&symbols);
        assert_eq!(self.symbol_span(symbol), *marker);
        assert_eq!(
            self.typed.symbol_type(symbol),
            Some(descriptor.element_type())
        );
        assert_eq!(
            self.owned.binding_kind(symbol),
            Some(OwnershipBindingKind::Shared)
        );
        assert_eq!(
            plan.bindings()
                .iter()
                .map(|binding| binding.symbol())
                .collect::<Vec<_>>(),
            symbols
        );
        for binding in plan.bindings() {
            assert_eq!(binding.symbol().source_unit(), self.source);
            assert_eq!(binding.kind(), OwnershipBindingKind::Shared);
            assert_eq!(
                binding.declaration_span(),
                self.symbol_span(binding.symbol())
            );
        }
    }

    fn end_element(&self, plan: &UnitIterationOwnershipPlan, actions: &mut Vec<Action>) {
        let statement = plan.descriptor().statement();
        actions.extend(
            plan.bindings()
                .iter()
                .rev()
                .map(|binding| Action::EndBinding {
                    statement,
                    symbol: binding.symbol(),
                }),
        );
        actions.push(Action::EndElement(statement));
    }

    fn finish(&self, plan: &UnitIterationOwnershipPlan, actions: &mut Vec<Action>) {
        let statement = plan.descriptor().statement();
        actions.extend([
            Action::FinishProvider(statement),
            Action::EndSource(statement),
        ]);
    }

    fn origin(&self, expected: &Expected<'_>, control: Control, map: bool) -> Origins {
        let (declaration, function_span) = self.declaration(expected.function);
        // Select the original For AST inside this declaration, before consulting ownership.
        let loops = self
            .parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                let Statement::For { source, .. } = node.payload() else {
                    return None;
                };
                let source = UnitExpressionId::new(self.source, *source);
                (contains(function_span, node.span())
                    && matches!(self.expression(source), Expression::Name)
                    && self.sources.slice(self.span(source)).unwrap() == "keys")
                    .then_some(UnitStatementId::new(self.source, id))
            })
            .collect::<Vec<_>>();
        let statement = *one(&loops);
        let plan = self
            .owned
            .iteration(statement)
            .expect("the original keys provider");
        self.bindings(plan);
        let descriptor = plan.descriptor();
        assert_eq!(
            descriptor.provider(),
            IterationProvider::Sequential(expected.container)
        );
        assert_eq!(
            self.typed.expression_category(descriptor.source()),
            Some(ExpressionCategory::Place)
        );
        assert_eq!(plan.source_access(), UnitIterationSourceAccess::Owned);
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            panic!("the actual owned source root place");
        };
        assert!(place.is_root());
        let symbol = self.reference(descriptor.source());
        assert_eq!(place.root(), symbol);
        assert_eq!(
            self.typed.symbol_type(symbol),
            Some(descriptor.source_type())
        );
        let marker = self.symbol_span(symbol);
        assert!(contains(function_span, marker));
        let signature = self
            .typed
            .signatures()
            .declaration(declaration)
            .unwrap()
            .callable()
            .unwrap();
        let seed = if expected.own {
            assert_eq!(
                self.owned.binding_kind(symbol),
                Some(OwnershipBindingKind::Owned)
            );
            let parameter = one(signature.parameters());
            assert_eq!(parameter.mode(), ParameterMode::Value);
            assert_eq!(parameter.symbol(), Some(symbol));
            assert_eq!(parameter.ty(), descriptor.source_type());
            let roots = self
                .parsed
                .roots()
                .iter()
                .filter_map(|id| {
                    let node = self.parsed.ast().items().get(*id).unwrap();
                    if node.span() != function_span {
                        return None;
                    }
                    let Item::Function { parameters, .. } = node.payload() else {
                        return None;
                    };
                    Some(parameters)
                })
                .collect::<Vec<_>>();
            let original = one(one(&roots));
            assert_eq!(original.span, parameter.span());
            assert_eq!(original.name, NameMarker::Present(marker));
            assert!(matches!(
                original.mode_marker,
                Some(ParameterModeMarker::Own(_))
            ));
            None
        } else {
            assert!(signature.parameters().is_empty());
            assert!(self.owned.formed_owned_locals().contains(&symbol));
            let mut initializer = self.variable(symbol);
            assert!(contains(function_span, self.span(initializer)));
            assert!(self.span(initializer).end() < self.statement_span(statement).start());
            while let Expression::Group { expression } = self.expression(initializer) {
                let inner = UnitExpressionId::new(self.source, *expression);
                assert!(contains(self.span(initializer), self.span(inner)));
                assert_eq!(
                    self.typed.expression_type(initializer),
                    Some(descriptor.source_type())
                );
                initializer = inner;
            }
            let constructors = self
                .typed
                .container_constructions()
                .iter()
                .filter(|constructor| constructor.expression() == initializer)
                .collect::<Vec<_>>();
            let constructor = one(&constructors);
            assert_eq!(constructor.container(), expected.container);
            assert_eq!(constructor.container_type(), descriptor.source_type());
            assert_eq!(constructor.element_type(), descriptor.element_type());
            Some(self.span(initializer))
        };

        let loop_span = self.statement_span(statement);
        let controls = self
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                (contains(loop_span, node.span())
                    && matches!(
                        (control, node.payload()),
                        (Control::Continue, Expression::Continue { .. })
                            | (Control::Return, Expression::Return { value: None, .. })
                    ))
                .then_some(UnitExpressionId::new(self.source, id))
            })
            .collect::<Vec<_>>();
        let transfer = *one(&controls);
        let explicit = match control {
            Control::Continue => Exit::Continue(transfer),
            Control::Return => Exit::Return(transfer),
        };
        let Statement::For { body, .. } = self
            .parsed
            .ast()
            .statements()
            .get(statement.statement())
            .unwrap()
            .payload()
        else {
            unreachable!();
        };
        let body = UnitStatementId::new(self.source, *body);

        let puts = self
            .typed
            .map_puts()
            .iter()
            .filter(|put| contains(function_span, self.span(put.expression())))
            .collect::<Vec<_>>();
        assert_eq!(puts.len(), usize::from(map));
        let cache = if map {
            let put = one(&puts);
            assert!(contains(loop_span, self.span(put.expression())));
            let cache = self.reference(put.receiver());
            assert_eq!(
                self.sources.slice(self.symbol_span(cache)).unwrap(),
                "cache"
            );
            let construction = self.variable(cache);
            let constructions = self
                .typed
                .map_constructions()
                .iter()
                .filter(|fact| fact.expression() == construction)
                .collect::<Vec<_>>();
            one(&constructions);
            Some(cache)
        } else {
            None
        };

        // The decoy is independently located by its real declaration, never by container type.
        let decoys = self
            .parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (contains(loop_span, *name) && self.sources.slice(*name).unwrap() == "decoy")
                    .then_some(*name)
            })
            .collect::<Vec<_>>();
        assert!(decoys.len() <= 1);
        let decoy = decoys.first().map(|marker| {
            let references = self
                .parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, node)| {
                    (matches!(node.payload(), Expression::Name)
                        && contains(loop_span, node.span())
                        && self.sources.slice(node.span()).unwrap() == "decoy")
                        .then_some(UnitExpressionId::new(self.source, id))
                })
                .collect::<Vec<_>>();
            let decoy = self.reference(*one(&references));
            assert_eq!(self.symbol_span(decoy), *marker);
            assert_ne!(decoy, symbol);
            decoy
        });

        let mut outer = self
            .owned
            .iterations()
            .iter()
            .filter(|candidate| {
                candidate.descriptor().statement() != statement
                    && contains(
                        self.statement_span(candidate.descriptor().statement()),
                        loop_span,
                    )
            })
            .collect::<Vec<_>>();
        outer.sort_by_key(|candidate| {
            std::cmp::Reverse(
                self.statement_span(candidate.descriptor().statement())
                    .start(),
            )
        });
        assert!(outer.len() <= 1, "only the original fresh-inner nesting");
        for outer in &outer {
            self.bindings(outer);
            assert_eq!(outer.source_access(), UnitIterationSourceAccess::Temporary);
            assert_eq!(
                outer.source(),
                Some(&UnitLoanTarget::Temporary(outer.descriptor().source()))
            );
            assert_eq!(
                outer.descriptor().provider(),
                IterationProvider::Sequential(SequentialContainerKind::Array)
            );
            assert!(contains(
                self.statement_span(outer.descriptor().statement()),
                marker
            ));
            assert!(seed.is_some_and(|seed| contains(
                self.statement_span(outer.descriptor().statement()),
                seed
            )));
        }

        assert_eq!(plan.exits().len(), 3);
        for (kind, point) in [
            (explicit, UnitDropPoint::ControlTransfer(transfer)),
            (Exit::Fallthrough, UnitDropPoint::AfterStatement(body)),
            (Exit::Exhaustion, UnitDropPoint::LoopExit(statement)),
        ] {
            let exits = plan
                .exits()
                .iter()
                .filter(|exit| exit.kind() == kind)
                .collect::<Vec<_>>();
            let exit = one(&exits);
            assert_eq!(exit.point(), point);
            let mut actions = Vec::new();
            if matches!(kind, Exit::Return(_))
                && let Some(decoy) = decoy
            {
                actions.push(self.named_drop(point, decoy));
            }
            if kind != Exit::Exhaustion {
                self.end_element(plan, &mut actions);
            }
            if kind == Exit::Exhaustion || matches!(kind, Exit::Return(_)) {
                self.finish(plan, &mut actions);
            }
            if matches!(kind, Exit::Return(_)) {
                if let Some(cache) = cache {
                    actions.push(self.named_drop(point, cache));
                }
                actions.push(self.named_drop(point, symbol));
                for outer in &outer {
                    self.end_element(outer, &mut actions);
                    self.finish(outer, &mut actions);
                    let source = outer.descriptor().source();
                    actions.push(Action::Drop(self.drop(
                        point,
                        UnitDropTarget::Temporary(source),
                        self.span(source),
                    )));
                    let exits = outer
                        .exits()
                        .iter()
                        .filter(|exit| exit.kind() == kind)
                        .collect::<Vec<_>>();
                    assert_eq!(one(&exits).point(), point);
                    assert_eq!(
                        one(&exits).actions(),
                        exit.actions(),
                        "nested Return shares the complete sequence"
                    );
                }
            } else if kind == Exit::Exhaustion && !expected.live {
                actions.push(self.named_drop(point, symbol));
            }
            assert_eq!(
                exit.actions(),
                actions,
                "exact ordered cleanup, no filtered subsequence"
            );
            assert_eq!(
                self.owned.iteration_cleanup_at(point),
                Some(actions.as_slice())
            );
        }

        let drops = self
            .owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), if control == Control::Return { 2 } else { 1 });
        let size_reads = self
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                let Expression::Member {
                    receiver,
                    name_span,
                    safe: false,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                let receiver = UnitExpressionId::new(self.source, *receiver);
                (contains(function_span, node.span())
                    && node.span().start() > loop_span.end()
                    && self.sources.slice(*name_span).unwrap() == "size"
                    && matches!(self.expression(receiver), Expression::Name)
                    && self.reference(receiver) == symbol)
                    .then_some(UnitExpressionId::new(self.source, id))
            })
            .collect::<Vec<_>>();
        assert_eq!(size_reads.len(), usize::from(expected.live));
        let final_point = if expected.live {
            let expression = *one(&size_reads);
            let size = self
                .typed
                .container_size(expression)
                .expect("the actual typed post-read");
            assert_eq!(size.expression(), expression);
            assert_eq!(size.span(), self.span(expression));
            assert_eq!(self.reference(size.receiver()), symbol);
            assert_eq!(size.container(), expected.container);
            assert_eq!(size.container_type(), descriptor.source_type());
            assert_eq!(size.element_type(), descriptor.element_type());
            UnitDropPoint::CallReturn(expression)
        } else {
            UnitDropPoint::LoopExit(statement)
        };
        self.drop(final_point, UnitDropTarget::Named(symbol), marker);
        for fact in drops {
            assert_eq!(fact.value_origin(), marker);
            assert!(
                fact.point() == final_point
                    || (control == Control::Return
                        && fact.point() == UnitDropPoint::ControlTransfer(transfer))
            );
        }
        Origins {
            seed,
            control: self.span(transfer),
        }
    }
}

pub(in crate::ssa::lower_frontend_tests) fn observe(
    text: &str,
    root: &str,
    expected: &[Expected<'_>],
    control: Control,
    map: bool,
) -> (Program, Vec<Origins>) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("lowering.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().unwrap();
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
    assert!(complete.is_same_analysis(&raw));
    assert!(std::ptr::eq(complete.types(), &raw));
    let typed = raw
        .validate()
        .expect("the original source has basic capability");
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let owned = owned.validate().unwrap();
    let input = one(names.names().index().source_units());
    assert_eq!(input.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(input.source_id(), parsed.source_id());
    assert_eq!(sources.source_name(source).unwrap(), "lowering.ko");
    assert_eq!(sources.source_text(source).unwrap(), text);
    let facts = Facts {
        sources: &sources,
        parsed: &parsed,
        names: &names,
        typed: typed.types(),
        owned: owned.ownership(),
        source: input.id(),
    };
    let mut declarations = names
        .names()
        .index()
        .declarations()
        .iter()
        .collect::<Vec<_>>();
    declarations.sort_by_key(|declaration| declaration.name_span().start());
    let expected_names = if expected.len() == 2 {
        vec!["freshLive", "freshDead", "main"]
    } else {
        vec![root]
    };
    assert_eq!(
        declarations
            .iter()
            .map(|declaration| declaration.name())
            .collect::<Vec<_>>(),
        expected_names
    );
    assert!(
        declarations
            .iter()
            .all(|declaration| declaration.kind() == SymbolKind::Function)
    );
    let (declaration, root_span) = facts.declaration(root);
    let origins = expected
        .iter()
        .map(|expected| facts.origin(expected, control, map))
        .collect::<Vec<_>>();
    let view =
        owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
    let (program, entry) =
        crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, declaration).unwrap();
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    assert_eq!(function.origin, Origin::Source(root_span));
    for expected in expected {
        let (target, span) = facts.declaration(expected.function);
        let functions = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| function.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let observed = one(&functions);
        if expected.own {
            let parameter = *one(&observed.blocks[0].parameters);
            assert!(matches!(
                observed.entity(parameter).unwrap().ty,
                EntityType::Value(_)
            ));
        }
        if expected.function != root {
            let calls = typed
                .types()
                .calls()
                .iter()
                .filter(|call| {
                    call.target() == UnitCallTarget::Declaration(target)
                        && contains(root_span, facts.span(call.expression()))
                })
                .collect::<Vec<_>>();
            let call = one(&calls);
            assert!(matches!(
                facts.expression(call.expression()),
                Expression::Call { .. }
            ));
            assert!(call.arguments().is_empty());
            assert!(call.receiver().is_none());
            let calls = function.instructions.iter().filter(|instruction| {
                instruction.origin == Origin::Source(facts.span(call.expression()))
                    && matches!(instruction.operation, Operation::DirectCall { callee, ref arguments, receiver: None } if callee == observed.id && arguments.is_empty())
            }).collect::<Vec<_>>();
            one(&calls);
        }
        if map {
            let puts = typed
                .types()
                .map_puts()
                .iter()
                .filter(|put| contains(span, facts.span(put.expression())))
                .collect::<Vec<_>>();
            let put = one(&puts);
            let instructions = observed
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.origin == Origin::Source(facts.span(put.expression()))
                        && matches!(instruction.operation, Operation::MapPut { .. })
                })
                .collect::<Vec<_>>();
            one(&instructions);
        }
    }
    if expected.len() == 2 {
        assert_eq!(root, "main");
        assert_eq!(program.modules.iter().flat_map(|module| &module.functions).filter(|function| matches!(function.origin, Origin::Source(span) if [facts.declaration("freshLive").1, facts.declaration("freshDead").1, root_span].contains(&span))).count(), 3);
    }
    crate::ssa::verify::verify_program(&program).unwrap();
    // Keep the original no-process-entry renderer, including own-parameter work roots.
    crate::llvm::render_verified_program(&program).unwrap();
    (program, origins)
}
