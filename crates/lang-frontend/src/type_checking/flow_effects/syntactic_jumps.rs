//! Prove a missing normal successor without type facts or constant evaluation.

use std::collections::BTreeMap;

use super::{
    SyntacticBooleanOutcomes,
    when_prefix::{visit_when_prefix, when_operand},
};
use crate::{
    ast::{AstError, ExpressionId, StatementId},
    parser::{Expression, Item, PrefixOperator, Statement, StringPart, WhenEntry},
};

#[derive(Clone, Copy)]
enum Node {
    Expression(ExpressionId),
    Statement(StatementId),
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey {
    Expression(usize),
    Statement(usize),
}

impl Node {
    fn key(self) -> NodeKey {
        match self {
            Self::Expression(id) => NodeKey::Expression(id.index()),
            Self::Statement(id) => NodeKey::Statement(id.index()),
        }
    }
}

enum Rule<'a> {
    Known(bool),
    Sequence(Vec<Node>),
    If {
        condition: Node,
        then_branch: Node,
        else_branch: Option<Node>,
    },
    When {
        subject: Option<ExpressionId>,
        entries: &'a [WhenEntry],
    },
}

enum Work<'a> {
    Visit(Node),
    Finish(Node, Rule<'a>),
}

pub(super) struct SyntacticJumps<'a, 'b> {
    outcomes: &'b SyntacticBooleanOutcomes<'a>,
    stops: BTreeMap<NodeKey, bool>,
}

impl<'a, 'b> SyntacticJumps<'a, 'b> {
    pub(super) fn new(outcomes: &'b SyntacticBooleanOutcomes<'a>) -> Self {
        Self {
            outcomes,
            stops: BTreeMap::new(),
        }
    }

    pub(super) fn expression_stops(&mut self, id: ExpressionId) -> Result<bool, AstError> {
        self.prove(Node::Expression(id))
    }

    pub(super) fn statement_stops(&mut self, id: StatementId) -> Result<bool, AstError> {
        self.prove(Node::Statement(id))
    }

    fn prove(&mut self, root: Node) -> Result<bool, AstError> {
        if let Some(&stops) = self.stops.get(&root.key()) {
            return Ok(stops);
        }
        let mut pending = vec![Work::Visit(root)];
        while let Some(work) = pending.pop() {
            match work {
                Work::Visit(node) if !self.stops.contains_key(&node.key()) => {
                    let rule = self.rule(node)?;
                    let children = pending.len();
                    match &rule {
                        Rule::Known(value) => {
                            self.stops.insert(node.key(), *value);
                            continue;
                        }
                        Rule::Sequence(nodes) => {
                            pending.extend(nodes.iter().rev().copied().map(Work::Visit));
                        }
                        Rule::If {
                            condition,
                            then_branch,
                            else_branch,
                        } => {
                            pending.extend(else_branch.iter().copied().map(Work::Visit));
                            pending.push(Work::Visit(*then_branch));
                            pending.push(Work::Visit(*condition));
                        }
                        Rule::When { subject, entries } => {
                            for entry in entries.iter().rev() {
                                pending.push(Work::Visit(Node::Statement(entry.body)));
                                pending.extend(
                                    entry
                                        .conditions
                                        .iter()
                                        .rev()
                                        .filter_map(when_operand)
                                        .map(|operand| Work::Visit(Node::Expression(operand))),
                                );
                            }
                            pending.extend(
                                subject.iter().map(|id| Work::Visit(Node::Expression(*id))),
                            );
                        }
                    }
                    // The stack visits children before publishing the parent proof.
                    pending.insert(children, Work::Finish(node, rule));
                }
                Work::Visit(_) => {}
                Work::Finish(node, rule) => {
                    let stops = |child: Node| self.stops[&child.key()];
                    let result = match rule {
                        Rule::Known(value) => value,
                        Rule::Sequence(nodes) => nodes.into_iter().any(stops),
                        Rule::If {
                            condition,
                            then_branch,
                            else_branch,
                        } => {
                            stops(condition)
                                || (stops(then_branch) && else_branch.is_some_and(stops))
                        }
                        Rule::When { subject, entries } => {
                            if subject.is_some_and(|id| stops(Node::Expression(id))) {
                                true
                            } else {
                                let mut may_complete = false;
                                let may_miss = visit_when_prefix(
                                    entries,
                                    subject.is_some(),
                                    |id| Ok(stops(Node::Expression(id))),
                                    |id| self.outcomes.possible_results(id),
                                    |entry, _, may_match| {
                                        may_complete |=
                                            may_match && !stops(Node::Statement(entry.body));
                                    },
                                )?;
                                !may_miss && !may_complete
                            }
                        }
                    };
                    self.stops.insert(node.key(), result);
                }
            }
        }
        Ok(self.stops[&root.key()])
    }

    fn rule(&self, node: Node) -> Result<Rule<'a>, AstError> {
        let ast = self.outcomes.ast;
        Ok(match node {
            Node::Expression(id) => match ast.expressions().get(id)?.payload() {
                Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. } => Rule::Known(true),
                Expression::Group { expression }
                | Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand: expression,
                    ..
                } => Rule::Sequence(vec![Node::Expression(*expression)]),
                Expression::String { parts } => Rule::Sequence(
                    parts
                        .iter()
                        .filter_map(|part| match part {
                            StringPart::Interpolation { expression, .. } => {
                                Some(Node::Expression(*expression))
                            }
                            _ => None,
                        })
                        .collect(),
                ),
                Expression::Member { receiver, .. } => {
                    Rule::Sequence(vec![Node::Expression(*receiver)])
                }
                Expression::Call {
                    callee, arguments, ..
                } => {
                    let receiver = match ast.expressions().get(*callee)?.payload() {
                        Expression::Name => None,
                        Expression::Member {
                            receiver,
                            safe: false,
                            ..
                        } => Some(*receiver),
                        // Only receiver evaluation is unconditional for a safe call.
                        Expression::Member {
                            receiver,
                            safe: true,
                            ..
                        } => return Ok(Rule::Sequence(vec![Node::Expression(*receiver)])),
                        _ => return Ok(Rule::Known(false)),
                    };
                    Rule::Sequence(
                        receiver
                            .into_iter()
                            .chain(arguments.iter().map(|arg| arg.value))
                            .map(Node::Expression)
                            .collect(),
                    )
                }
                Expression::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => Rule::If {
                    condition: Node::Expression(*condition),
                    then_branch: Node::Statement(*then_branch),
                    else_branch: else_branch.map(Node::Statement),
                },
                Expression::When {
                    subject, entries, ..
                } => Rule::When {
                    subject: *subject,
                    entries,
                },
                _ => Rule::Known(false),
            },
            Node::Statement(id) => match ast.statements().get(id)?.payload() {
                Statement::Block { elements } | Statement::ControlBody { elements } => {
                    Rule::Sequence(elements.iter().copied().map(Node::Statement).collect())
                }
                Statement::Expression { expression } => {
                    Rule::Sequence(vec![Node::Expression(*expression)])
                }
                Statement::LocalDestructuring { initializer, .. } => {
                    Rule::Sequence(vec![Node::Expression(*initializer)])
                }
                Statement::LocalVariable { declaration } => {
                    match ast.items().get(*declaration)?.payload() {
                        Item::Variable { initializer, .. } => {
                            Rule::Sequence(vec![Node::Expression(*initializer)])
                        }
                        _ => Rule::Known(false),
                    }
                }
                // Loop headers are evaluated before entering the nested loop body.
                Statement::While { condition, .. } => {
                    Rule::Sequence(vec![Node::Expression(*condition)])
                }
                Statement::For { source, .. } => Rule::Sequence(vec![Node::Expression(*source)]),
                // A jump within a nested loop or callable does not prove that this node stops.
                _ => Rule::Known(false),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::parse_file, source::SourceMap};

    #[test]
    fn a_safe_member_call_may_complete_without_evaluating_its_argument() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "safe-member.ko",
                "fun run(): Unit { loop { gate?.cond(break); gate(break)?.cond(0) } }",
            )
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let ast = parsed.ast();
        let outcomes = SyntacticBooleanOutcomes::new(ast);
        let mut jumps = SyntacticJumps::new(&outcomes);
        for (source_text, stops) in [("gate?.cond(break)", false), ("gate(break)?.cond(0)", true)] {
            let (call, _) = ast
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()) == Ok(source_text))
                .expect("the original safe member call");
            assert_eq!(
                jumps.expression_stops(call).unwrap(),
                stops,
                "{source_text}"
            );
        }
    }
}
