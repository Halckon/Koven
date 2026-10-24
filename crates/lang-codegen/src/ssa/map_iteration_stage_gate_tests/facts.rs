//! 公共只读事实 oracle；query 失败只记录，不能短路已经具备资格的 lower。
use super::{
    Report,
    fixture::{FOR_TEXT, File, Fixture, loop_sites},
};
use lang_frontend::{
    name_resolution::{
        SourceUnitKey, SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn kind(constructor: IntrinsicTypeConstructor) -> Option<MapIterationKind> {
    match constructor {
        IntrinsicTypeConstructor::Map => Some(MapIterationKind::Map),
        IntrinsicTypeConstructor::MutableMap => Some(MapIterationKind::MutableMap),
        _ => None,
    }
}

pub(super) fn empty_entry(file: &File, name_span: Span) -> bool {
    file.parsed.ast().items().iter().any(|(_, node)| {
        let Item::Function {
            name: NameMarker::Present(name),
            form,
            ..
        } = node.payload()
        else {
            return false;
        };
        if *name != name_span {
            return false;
        }
        let block = match form {
            FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            }
            | FunctionForm::ImplicitUnitBlock(block) => *block,
            _ => return false,
        };
        file.parsed.ast().statements().get(block).is_ok_and(
            |node| matches!(node.payload(), Statement::Block { elements } if elements.is_empty()),
        )
    })
}

pub(super) fn unit(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    whole_set: bool,
    report: &mut Report,
) {
    let count = if whole_set { 2 } else { 1 };
    let memory = matches!(names.names().index().source_units(), [source]
        if source.key() == &SourceUnitKey::MemoryDocument);
    report.check(
        "unit exact typed/owned Map counts",
        typed.map_iterations().len() == count && owned.map_iterations().len() == count,
        (typed.map_iterations(), owned.map_iterations()),
    );
    let mut all_sites = Vec::new();
    for source_names in names.names().source_units() {
        let source_unit = source_names.source_unit();
        let Some(file) = fixture
            .files
            .iter()
            .find(|file| file.parsed.source_id() == source_names.resolution().source_id())
        else {
            report.check("unit parsed source identity", false, source_unit);
            continue;
        };
        let sites = match loop_sites(file) {
            Ok(sites) => sites,
            Err(error) => {
                report.check("AST query", false, error);
                continue;
            }
        };
        report.check(
            "unit AST fixture loop count",
            sites.len()
                == if memory {
                    count
                } else {
                    usize::from(file.constructor.is_some())
                },
            (&file.path, &sites),
        );
        for site in sites {
            let statement = UnitStatementId::new(source_unit, site.statement);
            let source_expression = UnitExpressionId::new(source_unit, site.source);
            let descriptor = typed.map_iteration(statement);
            let plan = owned.map_iteration(statement);
            all_sites.push((statement, site.span));
            report.check(
                "unit complete AST For/path/source",
                fixture.sources.slice(site.span) == Ok(FOR_TEXT)
                    && fixture.sources.slice(site.source_span) == Ok("source")
                    && fixture.sources.source_name(site.span.source_id()) == Ok(file.path.as_str()),
                &site,
            );
            let (Some(descriptor), Some(plan)) = (descriptor, plan) else {
                report.check("unit Map facts exist", false, (descriptor, plan));
                continue;
            };
            report.check(
                "unit descriptor identity",
                descriptor.statement() == statement
                    && descriptor.source() == source_expression
                    && Some(descriptor.source_type()) == typed.expression_type(source_expression)
                    && plan.descriptor() == descriptor
                    && typed.sequential_iteration(statement).is_none()
                    && owned.iteration(statement).is_none(),
                (descriptor, plan.descriptor()),
            );
            let source_type = typed.types().get(descriptor.source_type());
            report.check("unit intrinsic distinct K/V", matches!(source_type,
                Some(UnitTypeKind::Intrinsic { constructor, arguments })
                if kind(*constructor) == Some(descriptor.kind())
                    && file.constructor.is_none_or(|expected| expected == (*constructor, descriptor.kind()))
                    && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()]
                    && typed.types().get(descriptor.key_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    && typed.types().get(descriptor.value_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))), source_type);
            let references = names
                .names()
                .references()
                .iter()
                .filter_map(|reference| {
                    if reference.span() == site.source_span
                        && let UnitReferenceTarget::Symbol(symbol) = reference.target()
                    {
                        Some(*symbol)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let source = if let [source] = references.as_slice() {
                Some(*source)
            } else {
                None
            };
            let source_symbol = source.and_then(|id| {
                source_names
                    .resolution()
                    .symbols()
                    .iter()
                    .find(|symbol| symbol.id() == id.symbol())
            });
            let parameters = typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|signature| signature.callable())
                .flat_map(|callable| callable.parameters())
                .filter(|parameter| source.is_some() && parameter.symbol() == source)
                .collect::<Vec<_>>();
            report.check(
                "unit direct Name and unique real Borrow parameter",
                file.parsed.ast().expressions().get(site.source).is_ok_and(|node| {
                matches!(node.payload(), Expression::Name)
            })
                    && source_symbol.is_some_and(|symbol| {
                        symbol.kind() == SymbolKind::ValueParameter && !symbol.is_synthetic()
                            && symbol.span().source_id() == file.parsed.source_id()
                    })
                    && matches!(parameters.as_slice(), [parameter]
                        if parameter.mode() == ParameterMode::Borrow && parameter.ty() == descriptor.source_type()),
                (&references, source_symbol, parameters),
            );
            report.check(
                "unit actual Borrow source origin",
                matches!(plan.source(), Some(UnitLoanTarget::Place(place))
                if Some(place.root()) == source && place.is_root())
                    && matches!(plan.source_kind(), IterationSource::BorrowedSource(_))
                    && plan.source_access() == UnitIterationSourceAccess::Shared
                    && source.is_some_and(|source| {
                        source.source_unit() == source_unit
                            && typed.symbol_type(source) == Some(descriptor.source_type())
                            && owned.binding_kind(source) == Some(OwnershipBindingKind::Shared)
                    }),
                (source, plan.source_kind(), plan.source_access()),
            );
            report.check(
                "unit separate component identities",
                descriptor.key_binding() != descriptor.value_binding()
                    && plan.loans()[0] != plan.loans()[1],
                plan.loans(),
            );
            for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
                .into_iter()
                .enumerate()
            {
                let symbol = source_names.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == site.bindings[index]
                        && !symbol.is_synthetic()
                        && symbol.kind() == SymbolKind::ForBinding
                });
                let binding = descriptor.bindings()[index];
                let component_type = [descriptor.key_type(), descriptor.value_type()][index];
                let loan = plan.loan(component);
                report.check(
                    "unit real binding/type/Shared loan",
                    binding.source_unit() == source_unit
                        && symbol.is_some_and(|symbol| symbol.id() == binding.symbol())
                        && typed.symbol_type(binding) == Some(component_type)
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && plan.bindings()[index].symbol() == binding
                        && plan.bindings()[index].kind() == OwnershipBindingKind::Shared
                        && loan == &plan.loans()[index]
                        && loan.component() == component
                        && loan.statement() == statement
                        && loan.source() == source_expression
                        && loan.binding() == binding
                        && loan.kind() == LoanKind::Shared
                        && Some(loan.origin()) == plan.source(),
                    (symbol, loan),
                );
            }
        }
    }
    report.check(
        "unit exact AST source-set count",
        all_sites.len() == count,
        &all_sites,
    );
    if whole_set {
        report.check(
            "unit both Map kinds in unused functions",
            typed
                .map_iterations()
                .iter()
                .any(|descriptor| descriptor.kind() == MapIterationKind::Map)
                && typed
                    .map_iterations()
                    .iter()
                    .any(|descriptor| descriptor.kind() == MapIterationKind::MutableMap),
            typed.map_iterations(),
        );
        report.check(
            "unit actual source grouping",
            all_sites.len() == 2
                && (all_sites[0].0.source_unit() == all_sites[1].0.source_unit()) == memory,
            &all_sites,
        );
    }
}
