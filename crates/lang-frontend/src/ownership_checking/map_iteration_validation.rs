//! Map 产物只读交叉验证；required roots/exits 来自实际遍历，不扫描 AST arena。
use super::{
    ClosureCaptureSource, DropTarget, IterationCleanupAction as Action, IterationExitKind as Exit,
    IterationFactError, IterationSource, LoanKind, LoanTarget, MapIterationComponent as Component,
    MapIterationOwnershipPlan, MapIterationSourceAccess, OwnershipBindingKind,
    OwnershipCheckedFile,
};
use crate::{
    name_resolution::NameResolution,
    parser::{ParsedFile, Statement},
    source::Span,
    type_checking::TypedFile,
};

pub(super) fn validate_map_iteration_facts(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), IterationFactError> {
    let reject = |statement: Option<crate::ast::StatementId>, reason| IterationFactError {
        span: statement
            .and_then(|statement| parsed.ast().statements().get(statement).ok())
            .map(|node| node.span()),
        reason,
    };
    if let Some((statement, reason)) =
        crate::type_checking::map_iteration_validation::file_map_iteration_rows_error(
            parsed, names, typed,
        )
    {
        return Err(reject(Some(statement), reason));
    }
    if owned.map_iteration_descriptors != typed.map_iterations() {
        let matching_prefix = owned
            .map_iteration_descriptors
            .iter()
            .zip(typed.map_iterations())
            .take_while(|(actual, expected)| actual == expected)
            .count();
        let failed = typed
            .map_iterations()
            .get(matching_prefix)
            .or_else(|| owned.map_iteration_descriptors.get(matching_prefix));
        return Err(reject(
            failed.map(|plan| plan.statement()),
            "Map typed descriptor snapshot mismatch",
        ));
    }
    if let Some(pair) = owned.map_iterations.windows(2).find(|pair| {
        pair[0].descriptor().statement().index() >= pair[1].descriptor().statement().index()
    }) {
        return Err(reject(
            Some(pair[1].descriptor().statement()),
            "duplicate or unsorted Map ownership plan",
        ));
    }
    for template in &owned.map_iteration_templates {
        let statement = template.descriptor().statement();
        if owned
            .map_iterations
            .iter()
            .filter(|plan| plan.descriptor().statement() == statement)
            .count()
            != 1
        {
            return Err(reject(
                Some(statement),
                "missing or duplicate acquired Map ownership plan",
            ));
        }
    }
    for plan in &owned.map_iterations {
        let statement = plan.descriptor().statement();
        if owned
            .map_iteration_templates
            .iter()
            .filter(|template| template.descriptor().statement() == statement)
            .count()
            != 1
        {
            return Err(reject(
                Some(statement),
                "missing or duplicate actual Map source template",
            ));
        }
    }
    for (point, action) in owned.cleanup_steps() {
        let statement = match action {
            Action::EndElement(statement) if typed.map_iteration(*statement).is_some() => {
                return Err(reject(
                    Some(*statement),
                    "Map cleanup must never publish an element action",
                ));
            }
            Action::EndBinding { statement, .. }
            | Action::FinishProvider(statement)
            | Action::EndSource(statement)
                if typed.map_iteration(*statement).is_some() =>
            {
                Some(*statement)
            }
            Action::Drop(drop) => {
                if let DropTarget::Named(symbol) = drop.target()
                    && let Some(plan) = typed
                        .map_iterations()
                        .iter()
                        .find(|plan| plan.bindings().contains(&symbol))
                {
                    return Err(reject(
                        Some(plan.statement()),
                        "Map component must never acquire a payload drop",
                    ));
                }
                None
            }
            _ => None,
        };
        if let Some(statement) = statement
            && owned
                .map_iteration(statement)
                .is_none_or(|plan| !plan.exits().iter().any(|exit| exit.point() == *point))
        {
            return Err(reject(
                Some(statement),
                "orphan Map cleanup outside its actual exit points",
            ));
        }
    }
    for plan in &owned.map_iterations {
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        let Some(template) = owned
            .map_iteration_templates
            .iter()
            .find(|template| template.descriptor().statement() == statement)
        else {
            return Err(reject(
                Some(statement),
                "missing actual Map source template",
            ));
        };
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| reject(Some(statement), "unknown Map iteration statement"))?;
        if !crate::type_checking::map_iteration_validation::validate_file_map_iteration(
            parsed, names, typed, statement,
        ) || typed.map_iteration(statement) != Some(descriptor)
            || owned.iteration(statement).is_some()
            || descriptor != template.descriptor()
            || plan.source_kind() != template.source_kind()
            || plan.source_access() != template.source_access()
            || plan.bindings() != template.bindings()
            || plan.loans() != template.loans()
        {
            return Err(reject(
                Some(statement),
                "Map provider differs from actual typed/source loan traversal",
            ));
        }
        let Statement::For { source, body, .. } = node.payload() else {
            return Err(reject(
                Some(statement),
                "Map provider is not a For statement",
            ));
        };
        if *source != descriptor.source() {
            return Err(reject(
                Some(statement),
                "Map source evaluation identity mismatch",
            ));
        }
        let IterationSource::BorrowedSource(origin) = plan.source_kind() else {
            return Err(reject(
                Some(statement),
                "Map provider requires a Shared source lease",
            ));
        };
        if (plan.source_access() == MapIterationSourceAccess::Temporary)
            != matches!(origin, LoanTarget::Temporary(_))
        {
            return Err(reject(
                Some(statement),
                "Map source capability differs from its real owner",
            ));
        }
        let symbols = descriptor.bindings();
        for (index, component) in [Component::Key, Component::Value].into_iter().enumerate() {
            let binding = &plan.bindings()[index];
            let loan = &plan.loans()[index];
            if binding.symbol() != symbols[index]
                || binding.kind() != OwnershipBindingKind::Shared
                || owned.binding_kind(symbols[index]) != Some(OwnershipBindingKind::Shared)
                || loan.component() != component
                || loan.statement() != statement
                || loan.source() != *source
                || loan.binding() != symbols[index]
                || loan.kind() != LoanKind::Shared
                || loan.origin() != origin
            {
                return Err(reject(
                    Some(statement),
                    "Map component loan binding/provider/source lineage mismatch",
                ));
            }
        }
        let required: Vec<_> = owned
            .map_iteration_required_exits
            .iter()
            .filter(|exit| exit.statement == statement)
            .collect();
        if required.is_empty()
            || !required.iter().any(|exit| exit.kind == Exit::Exhaustion)
            || required.iter().any(|expected| {
                plan.exits()
                    .iter()
                    .filter(|exit| {
                        exit.kind() == expected.kind
                            && exit.point() == expected.point
                            && exit.condition() == expected.condition
                    })
                    .count()
                    != 1
            })
            || plan.exits().iter().any(|exit| {
                !required.iter().any(|expected| {
                    exit.kind() == expected.kind
                        && exit.point() == expected.point
                        && exit.condition() == expected.condition
                })
            })
        {
            return Err(reject(
                Some(statement),
                "missing or foreign Map exit from actual drop traversal",
            ));
        }
        let body_span = parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|_| reject(Some(statement), "unknown Map iteration body"))?
            .span();
        for exit in plan.exits() {
            if owned.cleanup_conditions().get(exit.condition()).is_none() {
                return Err(reject(Some(statement), "unknown Map exit path condition"));
            }
            let actual: Vec<_> = owned
                .cleanup_steps()
                .iter()
                .filter(|(point, _)| *point == exit.point())
                .map(|(_, action)| *action)
                .collect();
            if exit.actions() != actual.as_slice()
                || owned
                    .iterations()
                    .iter()
                    .flat_map(|other| other.exits())
                    .chain(
                        owned
                            .map_iterations()
                            .iter()
                            .flat_map(|other| other.exits()),
                    )
                    .any(|other| other.point() == exit.point() && other.actions() != exit.actions())
            {
                return Err(reject(
                    Some(statement),
                    "mixed-provider cleanup point is not one complete sequence",
                ));
            }
            validate_exit(plan, exit, names, owned, body_span)
                .map_err(|reason| reject(Some(statement), reason))?;
        }
    }
    if let Some(exit) = owned
        .map_iteration_required_exits
        .iter()
        .find(|exit| owned.map_iteration(exit.statement).is_none())
    {
        return Err(reject(
            Some(exit.statement),
            "Map required exit has no ownership plan",
        ));
    }
    Ok(())
}

fn validate_exit(
    plan: &MapIterationOwnershipPlan,
    exit: &super::IterationExitPlan,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    body_span: Span,
) -> Result<(), &'static str> {
    let statement = plan.descriptor().statement();
    let symbols = plan.descriptor().bindings();
    let actions = exit.actions();
    let exhausted = exit.kind() == Exit::Exhaustion;
    let terminal = matches!(
        exit.kind(),
        Exit::Break(_) | Exit::Return(_) | Exit::Exhaustion
    );
    let positions = |wanted| {
        actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| (*action == wanted).then_some(index))
            .collect::<Vec<_>>()
    };
    let key = positions(Action::EndBinding {
        statement,
        symbol: symbols[0],
    });
    let value = positions(Action::EndBinding {
        statement,
        symbol: symbols[1],
    });
    let finish = positions(Action::FinishProvider(statement));
    let source = positions(Action::EndSource(statement));
    if actions.contains(&Action::EndElement(statement))
        || actions.iter().any(|action| {
            matches!(action,
            Action::EndBinding { statement: owner, symbol }
                if *owner == statement && !symbols.contains(symbol))
        })
        || key.len() != usize::from(!exhausted)
        || value.len() != usize::from(!exhausted)
        || finish.len() != usize::from(terminal)
        || source.len() != usize::from(terminal)
        || (!exhausted && value[0] >= key[0])
        || (terminal && (finish[0] >= source[0] || (!exhausted && key[0] >= finish[0])))
    {
        return Err("Map cleanup requires value/key, provider/source and no element action");
    }
    let binding_end = |symbol| {
        if exhausted {
            None
        } else if symbol == symbols[0] {
            Some(key[0])
        } else if symbol == symbols[1] {
            Some(value[0])
        } else {
            None
        }
    };
    let mut temporary_drops = 0;
    for (position, action) in actions.iter().enumerate() {
        match action {
            Action::EndCallLoan(end) => {
                let Some(loan) = owned
                    .loans()
                    .iter()
                    .find(|loan| loan.call() == end.call() && loan.argument() == end.argument())
                else {
                    return Err("Map cleanup refers to an unknown call loan");
                };
                if !owned.loan_ends().contains(end)
                    || matches!(loan.target(), LoanTarget::Place(place)
                        if binding_end(place.root()).is_some_and(|end| position >= end))
                {
                    return Err("derived call loan ends after its Map binding");
                }
            }
            Action::EndCaptureLoan {
                closure,
                source: captured,
                ..
            } => {
                if !owned.captures().iter().any(|capture| {
                    capture.lambda() == *closure
                        && capture.source() == *captured
                        && capture.mode() == super::ClosureCaptureMode::Shared
                }) {
                    return Err("Map cleanup refers to an unknown shared capture");
                }
                if let ClosureCaptureSource::Symbol(symbol) = captured
                    && binding_end(*symbol).is_some_and(|end| position >= end)
                {
                    return Err("derived capture loan ends after its Map binding");
                }
            }
            Action::Drop(drop) => {
                if let DropTarget::Named(symbol) = drop.target() {
                    if symbols.contains(&symbol) {
                        return Err("Map binding must not acquire a payload drop");
                    }
                    if let Some(binding) = names.symbols().get(symbol.index())
                        && contains(body_span, binding.span())
                        && (exhausted || position >= value[0])
                    {
                        return Err("body owner drop must precede Map binding ends");
                    }
                    if matches!(plan.source(), Some(LoanTarget::Place(place)) if place.root() == symbol)
                        && (!terminal || position <= source[0])
                    {
                        return Err("Map source owner dropped while provider lease is active");
                    }
                }
                if let Some(LoanTarget::Temporary(owner)) = plan.source()
                    && drop.target() == DropTarget::Temporary(*owner)
                {
                    temporary_drops += 1;
                    if !terminal || position <= source[0] {
                        return Err("Map hidden source owner dropped before its source lease ends");
                    }
                }
            }
            _ => {}
        }
    }
    if matches!(plan.source(), Some(LoanTarget::Temporary(_)))
        && temporary_drops != usize::from(terminal)
    {
        return Err("Map hidden source owner has missing or duplicate cleanup");
    }
    Ok(())
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
