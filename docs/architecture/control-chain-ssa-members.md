# Control-chain member SSA receipt

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改多control成员晚期SSA发射 · **唯一真源**：control_bridge_receipt及原发射事件

## Scope

The bounded three-control owner chain selects its second and third members as one group.
This receipt connects those completed members to actual late SSA emission. It does not
manufacture a first-control entry catalog, authorize arbitrary nested control flow, or prove
complete dynamic owner provenance across the two inter-control links. Both links below
check actual static Alias transport without claiming runtime generation induction.

## Atomic source

Only after scalar bindings, normal-body retirement, false-exit partition, final suffix, and
checked-tail validation succeed may the builder seal the group. Each member retains its
actual control identity, header/body/finished/exit blocks, checked success/abort blocks, and
root slots. Missing or duplicate checked evidence rejects the group before sealing.

## Emission contract

- Straight-line CFG folding remaps every member's recorded block references.
- Actual block layout connects root slots to SSA parameters.
- Replace records use control identity and member-local root index, not equal-sized sides.
- The original Alias events update the slot-to-entity mapping used by emission checks.
- Each tracked Replace's second result is the retired value; its same-body Drop occurs once.
- Exact emitted instruction lists, operations, results, terminators, and edge arguments are
  rechecked. Checked abort remains terminal, not a normal header predecessor.
- Completion is recorded only after all members pass; there is no fallback to a partial
  single-bridge receipt.

## Verification boundary

Normal tests cover two-root and sparse three-to-two-root chains, final dead-root cleanup
and live-root suffix cleanup, and native zero/two-round executions. CI-only faults remove
last-member terminator observation or redirect its retired-value Drop to the new generation;
additional group faults substitute another member's root edge argument or corrupt body/checked
mapping after a real fold. Each must hit once and withhold group completion. These fault tests
are compiled locally but executed only in CI.

The existing single-bridge receipt remains separate and uses the same emission checks.
This member receipt is not arbitrary runtime-trace induction or complete inter-control
owner-link authorization.

## Second-to-third transport

The group borrows the frontend's completed source/target vectors rather than zipping member
root lists; sparse retired roots are excluded by the original observed mapping. Before sealing,
the builder requires the second exit to be the actual third entry, a Goto to its header, and
an original whole-block Alias chain with exactly the intended source/intermediate uses.
A private Link follows CFG folding. Actual emission must preserve source/target Entity equality
and the real third-header edge arguments. Every target must be covered once, and the link is
observed once before any final group completion. CI-only wrong-target and actual emitted-edge argument-swap probes are compiled but
not run locally; both withhold Link and group completion. No first-control entry catalog or arbitrary runtime trace is synthesized.

## Leading first-to-second transport

A borrowed leading view exposes only the already completed first control geometry and
source/target mapping. The builder captures its actual blocks and source slots at its while
hook, checks the genuine exit-to-second-entry Alias chains, and seals a separate Anchor.
It does not invent a first full Member or scalar-entry permissions. Both Links remap
through folding and must each be observed exactly once; their target headers and parameters
are selected by control identity, not by a global last-member assumption. A CI-only actual
leading-edge argument swap must withhold all Link and group completion; local runs compile
that test without executing it. This closes static Alias transport, not dynamic induction.

## Leading normal-body consumption

The leading view now separately borrows the original completed normal rule and producer
catalog. At its real while hook the builder captures all normal-body roots, not merely the
live subset transported by the leading Link. The same control-qualified Replace→binding→Drop
permits consume its original instructions once. All leading steps must complete before the
whole group can finish. Late SSA checks also track its actual replacements and exactly one
retired-value Drop. Scalar-entry permissions remain limited to the two continuation members. Tests exercise a three-root leading body with a two-root outgoing link;
this prevents replacing the complete body root list with its live subset. No dynamic trace
induction or first-control scalar initialization permission is inferred.


## Leading checked-tail consumption

The leading view borrows the original counter/guard and completed ordered tail sites.
The real while hook snapshots scalar slots and the entry block before later controls can
rebind them. Whole-group sealing now requires the same checked-subtract receipt, terminal
abort, exclusive success edge, ordered counter writeback/comparison/guard update, and
success-to-own-header checks as continuation members. The Anchor carries those actual
checked blocks through CFG folding and late SSA layout revalidation. Missing receipt revokes
all group completion; its CI-only injection is compiled, never executed locally. This does
not grant first scalar initializer or false-exit consumption permission, nor prove dynamic
cross-iteration induction.
