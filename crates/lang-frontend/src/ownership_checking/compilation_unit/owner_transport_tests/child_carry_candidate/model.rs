//! Independent handwritten topology oracle, NOT emitted CFG or a Carry permission.
//! Generations below are model counters, never SSA EntityId or source identity.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Block {
    ParentHeader,
    ChildHeader,
    ChildBody,
    Resume,
    Exit,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Live {
    root: UnitSymbolId,
    generation: usize,
}

struct Model {
    replace: UnitSymbolId,
    carry: UnitSymbolId,
    child_call: UnitExpressionId,
    parent_call: UnitExpressionId,
    suffix: UnitExpressionId,
    parent_loop: UnitStatementId,
    child_loop: UnitStatementId,
}

impl Model {
    fn from_source(fixture: &Fixture, owned: &CompilationUnitOwnership) -> Option<Self> {
        let evidence = extract_source_evidence(fixture, owned)?;
        shape::matches(fixture, owned, &evidence)?;
        // The shape has already located exactly one child and one following parent
        // replacement. Preserve actual call identities, not model-generated IDs.
        let mut calls = owned
            .ownership_primitives()
            .iter()
            .map(|primitive| primitive.descriptor().expression())
            .collect::<Vec<_>>();
        calls.sort_by_key(|call| {
            fixture.parsed[0]
                .ast()
                .expressions()
                .get(call.expression())
                .unwrap()
                .span()
                .start()
        });
        let [child_call, parent_call] = calls.as_slice() else {
            return None;
        };
        let mut loops = fixture.parsed[0]
            .ast()
            .statements()
            .iter()
            .filter(|(_, node)| matches!(node.payload(), Statement::While { .. }))
            .collect::<Vec<_>>();
        loops.sort_by_key(|(_, node)| node.span().start());
        let [(parent, _), (child, _)] = loops.as_slice() else {
            return None;
        };
        Some(Self {
            replace: evidence.replaced,
            carry: evidence.carried,
            child_call: *child_call,
            parent_call: *parent_call,
            suffix: evidence.suffix,
            parent_loop: UnitStatementId::new(fixture.source(0), *parent),
            child_loop: UnitStatementId::new(fixture.source(0), *child),
        })
    }

    fn run(&self, outer_count: usize, inner_count: usize) {
        assert_ne!(self.parent_loop, self.child_loop);
        assert_eq!(self.parent_loop.source_unit(), self.replace.source_unit());
        assert_eq!(self.child_loop.source_unit(), self.carry.source_unit());
        let mut live = [
            Live {
                root: self.replace,
                generation: 0,
            },
            Live {
                root: self.carry,
                generation: 0,
            },
        ];
        let mut block = Block::ParentHeader;
        let mut outer = outer_count;
        let mut inner = 0;
        let mut retirements = Vec::new();
        let mut replacements = 0;
        let mut suffixes = 0;
        let mut final_drops = Vec::new();
        // Explicit oracle edges: header true/false, body success backedge and
        // resume success backedge. They are NOT inferred production predecessors.
        while block != Block::Done {
            assert_eq!(live[0].root, self.replace);
            assert_eq!(
                live[1],
                Live {
                    root: self.carry,
                    generation: 0
                }
            );
            assert_ne!(live[0].root, live[1].root);
            block = match block {
                Block::ParentHeader if outer == 0 => Block::Exit,
                Block::ParentHeader => {
                    inner = inner_count;
                    Block::ChildHeader
                }
                Block::ChildHeader if inner == 0 => Block::Resume,
                Block::ChildHeader => Block::ChildBody,
                Block::ChildBody => {
                    retirements.push((self.child_call, live[0]));
                    replacements += 1;
                    live[0].generation = replacements;
                    inner -= 1;
                    Block::ChildHeader
                }
                Block::Resume => {
                    retirements.push((self.parent_call, live[0]));
                    replacements += 1;
                    live[0].generation = replacements;
                    outer -= 1;
                    Block::ParentHeader
                }
                Block::Exit => {
                    // Abstract final partition: a retires on loop exit; b is
                    // borrowed by the actual suffix then retired at its return.
                    // This ordering is a model obligation, not proved lowering.
                    final_drops.push(live[0]);
                    assert_eq!(self.suffix.source_unit(), self.carry.source_unit());
                    suffixes += 1;
                    final_drops.push(live[1]);
                    Block::Done
                }
                Block::Done => unreachable!("loop condition excludes terminal state"),
            };
        }
        assert_eq!(replacements, outer_count * (inner_count + 1));
        assert_eq!(suffixes, 1);
        assert_eq!(final_drops, live);
        // Every old a generation is retired once; current a and original b
        // survive to the final partition. No Carry generation is manufactured.
        for (generation, (_, retired)) in retirements.iter().enumerate() {
            assert_eq!(
                *retired,
                Live {
                    root: self.replace,
                    generation
                }
            );
        }
        assert_eq!(retirements.len(), replacements);
    }
}

#[test]
fn ordinary_independent_topology_tracks_real_root_roles_and_normal_exits() {
    let used = SOURCE.strip_suffix("}\n}").unwrap().to_owned() + "}\nprintln(b)\n}";
    let fixture = Fixture::new(&[("model.ko", &used)]);
    let owned = fixture.shared();
    let model = Model::from_source(&fixture, &owned).unwrap();
    assert_eq!(model.replace, fixture.local(0, "a"));
    assert_eq!(model.carry, fixture.local(0, "b"));
    assert_ne!(model.child_call, model.parent_call);
    for outer in 0..=2 {
        for inner in 0..=2 {
            model.run(outer, inner);
        }
    }
}
