# Control-chain member SSA receipt

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改多control成员晚期SSA发射 · **唯一真源**：control_bridge_receipt及原发射事件

## Scope

The bounded three-control owner chain selects its second and third members as one group.
This receipt connects those completed members to actual late SSA emission. It does not
manufacture a first-control entry catalog, authorize arbitrary nested control flow, or prove
complete dynamic owner provenance across the two inter-control links.

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
these fault tests are compiled locally but executed only in CI.

The existing single-bridge receipt remains separate and uses the same emission checks.
This member receipt is not arbitrary runtime-trace induction or complete inter-control
owner-link authorization.
