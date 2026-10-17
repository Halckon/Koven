//! 从同一真实 AST 定位 pattern、body 子树和 Pair 声明，不以字符串偏移造身份。

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    parser::{ClassifierKind, Expression, ForBinding, Item, NameMarker, Statement},
    source::Span,
};

use super::{
    Case,
    analysis::{Blocked, Fixture, blocked},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Body {
    Empty,
    Independent {
        receiver: ExpressionId,
        member: ExpressionId,
        call: ExpressionId,
        name_span: Span,
        number: Span,
    },
    Derived {
        declaration: Span,
        initializer: ExpressionId,
    },
}

#[derive(Debug)]
pub(super) struct Sites {
    pub(super) statement: StatementId,
    pub(super) source: ExpressionId,
    pub(super) source_span: Span,
    pub(super) pattern: Span,
    pub(super) markers: Vec<Span>,
    pub(super) nominal: Option<(ItemId, Span)>,
    pub(super) body: Body,
}

fn local(
    fixture: &Fixture,
    statement: StatementId,
    expected: &str,
) -> Result<(Span, Option<TypeRefId>, ExpressionId), Blocked> {
    let ast = fixture.file.ast();
    let statement = ast
        .statements()
        .get(statement)
        .map_err(|error| blocked("AST body", error))?;
    let Statement::LocalVariable { declaration } = statement.payload() else {
        return Err(blocked("AST body", "expected a local variable statement"));
    };
    let item = ast
        .items()
        .get(*declaration)
        .map_err(|error| blocked("AST body", error))?;
    let Item::Variable {
        name: NameMarker::Present(name),
        type_ref,
        initializer,
        ..
    } = item.payload()
    else {
        return Err(blocked("AST body", "expected a real named local variable"));
    };
    if fixture.sources.slice(*name) != Ok(expected) {
        return Err(blocked("AST body", (name, expected)));
    }
    Ok((*name, *type_ref, *initializer))
}

pub(super) fn sites(fixture: &Fixture, case: Case) -> Result<Sites, Blocked> {
    let ast = fixture.file.ast();
    let sources = &fixture.sources;
    let loops = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Statement::For {
                binding,
                source,
                body,
                ..
            } => Some((id, binding, *source, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(statement, binding, source, body)] = loops.as_slice() else {
        return Err(blocked("AST for", "expected exactly one ordinary for"));
    };
    let ForBinding::Destructuring {
        left_paren_span,
        names,
        right_paren_span: Some(right),
    } = binding
    else {
        return Err(blocked(
            "AST pattern",
            "expected real complete destructuring delimiters",
        ));
    };
    if sources.slice(*left_paren_span) != Ok("(") || sources.slice(*right) != Ok(")") {
        return Err(blocked("AST pattern", "parentheses must be real tokens"));
    }
    let pattern = sources
        .span(
            fixture.file.source_id(),
            left_paren_span.start(),
            right.end(),
        )
        .map_err(|error| blocked("AST pattern", error))?;
    let markers = names
        .iter()
        .map(|marker| match marker {
            NameMarker::Present(span) => Ok(*span),
            other => Err(blocked("AST pattern", other)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if sources.slice(pattern) != Ok(case.pattern())
        || markers.len() != case.markers().len()
        || !markers
            .iter()
            .zip(case.markers())
            .all(|(span, expected)| sources.slice(*span) == Ok(*expected))
    {
        return Err(blocked("AST pattern", (&markers, pattern)));
    }
    let source_node = ast
        .expressions()
        .get(*source)
        .map_err(|error| blocked("AST source", error))?;
    let source_name = if case == Case::SequentialArity {
        "xs"
    } else {
        "m"
    };
    if !matches!(source_node.payload(), Expression::Name)
        || sources.slice(source_node.span()) != Ok(source_name)
    {
        return Err(blocked(
            "AST source",
            "expected the real provider name expression",
        ));
    }
    let classifiers = ast
        .items()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Item::Classifier(classifier) => Some((id, classifier)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let nominal = if case == Case::SequentialArity {
        let [(id, classifier)] = classifiers.as_slice() else {
            return Err(blocked(
                "AST nominal",
                "expected exactly one Pair declaration",
            ));
        };
        let NameMarker::Present(span) = classifier.name else {
            return Err(blocked("AST nominal", classifier.name));
        };
        if !matches!(classifier.kind, ClassifierKind::ValueClass { .. })
            || sources.slice(span) != Ok("Pair")
        {
            return Err(blocked("AST nominal", "expected the real value class Pair"));
        }
        Some((*id, span))
    } else {
        if !classifiers.is_empty() {
            return Err(blocked(
                "AST nominal",
                "Map fixtures must use only the intrinsic",
            ));
        }
        None
    };
    let body_node = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST body", error))?;
    let Statement::Block { elements } = body_node.payload() else {
        return Err(blocked("AST body", "expected a real block"));
    };
    let body = match case {
        Case::SequentialArity if elements.is_empty() => Body::Empty,
        Case::SequentialArity => return Err(blocked("AST body", "expected empty arity body")),
        Case::IndependentBody => {
            let [number, bad] = elements.as_slice() else {
                return Err(blocked(
                    "AST body",
                    "expected exactly number and bad locals",
                ));
            };
            let (number, annotation, initializer) = local(fixture, *number, "n")?;
            let Some(annotation) = annotation else {
                return Err(blocked("AST body", "n requires explicit Int annotation"));
            };
            let annotation = ast
                .type_refs()
                .get(annotation)
                .map_err(|error| blocked("AST body", error))?;
            let initializer = ast
                .expressions()
                .get(initializer)
                .map_err(|error| blocked("AST body", error))?;
            if sources.slice(annotation.span()) != Ok("Int")
                || sources.slice(initializer.span()) != Ok("1")
            {
                return Err(blocked("AST body", "expected n: Int = 1"));
            }
            let (_, annotation, call) = local(fixture, *bad, "bad")?;
            if annotation.is_some() {
                return Err(blocked(
                    "AST body",
                    "bad must retain its inferred deferred type",
                ));
            }
            let call_node = ast
                .expressions()
                .get(call)
                .map_err(|error| blocked("AST body", error))?;
            let Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } = call_node.payload()
            else {
                return Err(blocked(
                    "AST body",
                    "bad initializer must be the actual call",
                ));
            };
            if !arguments.is_empty() || !type_arguments.is_empty() {
                return Err(blocked("AST body", "expected a no-argument member call"));
            }
            let member = ast
                .expressions()
                .get(*callee)
                .map_err(|error| blocked("AST body", error))?;
            let Expression::Member {
                receiver,
                name_span,
                ..
            } = member.payload()
            else {
                return Err(blocked("AST body", "expected actual member callee"));
            };
            let receiver_node = ast
                .expressions()
                .get(*receiver)
                .map_err(|error| blocked("AST body", error))?;
            if sources.slice(*name_span) != Ok("fooBar")
                || sources.slice(receiver_node.span()) != Ok("n")
                || !matches!(receiver_node.payload(), Expression::Name)
            {
                return Err(blocked("AST body", "expected actual n.fooBar member"));
            }
            Body::Independent {
                receiver: *receiver,
                member: *callee,
                call,
                name_span: *name_span,
                number,
            }
        }
        Case::PoisonedBinding => {
            let [derived] = elements.as_slice() else {
                return Err(blocked("AST body", "expected exactly one derived local"));
            };
            let (declaration, annotation, initializer) = local(fixture, *derived, "derived")?;
            let node = ast
                .expressions()
                .get(initializer)
                .map_err(|error| blocked("AST body", error))?;
            if annotation.is_some()
                || !matches!(node.payload(), Expression::Name)
                || sources.slice(node.span()) != Ok("key")
            {
                return Err(blocked("AST body", "expected inferred derived = key"));
            }
            Body::Derived {
                declaration,
                initializer,
            }
        }
    };
    Ok(Sites {
        statement: *statement,
        source: *source,
        source_span: source_node.span(),
        pattern,
        markers,
        nominal,
        body,
    })
}
