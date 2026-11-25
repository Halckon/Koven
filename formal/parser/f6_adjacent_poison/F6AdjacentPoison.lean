/-
  SPEC-0499 (F6 fourth slice): Machine-checked unbounded model proofs for
  `ElsePoisonTail` memoization soundness in `recover_poison_before_else` and
  same-line brace preservation in `adjacent_poison_tail_boundary`
  (`crates/lang-frontend/src/parser/engine/recovery/adjacent_poison.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harness in `adjacent_poison_kani.rs`.
-/

namespace F6AdjacentPoison

inductive LexClass where
  | trivia
  | poison
  | elseKw
  | leftBrace
  | other
  deriving DecidableEq, Repr

inductive ElseScanResult where
  | foundElse (boundary : Nat)
  | rejected (boundary : Nat)
  | exhausted
  deriving DecidableEq, Repr

/-- Exact functional model of `scan_poison_before_else_core` starting at index `i`
    with flag `seenPoison` indicating whether at least one ordinary poison was seen. -/
def scanElseFrom (L : List LexClass) : Nat → Bool → ElseScanResult
  | i, seenPoison =>
    if h : i < L.length then
      match L[i] with
      | .trivia => scanElseFrom L (i + 1) seenPoison
      | .poison => scanElseFrom L (i + 1) true
      | .elseKw =>
        if seenPoison then .foundElse i else .rejected i
      | .leftBrace | .other => .rejected i
    else
      .exhausted
termination_by i _ => L.length - i

def scanElse (L : List LexClass) (start : Nat) : ElseScanResult :=
  scanElseFrom L start false

/-! ### E1: Bounds & stopper characterization of `scanElseFrom` -/

theorem scanElseFrom_bounds (L : List LexClass) :
    ∀ (i : Nat) (seen : Bool) (b : Nat),
      (scanElseFrom L i seen = .foundElse b ∨ scanElseFrom L i seen = .rejected b) →
      i ≤ b ∧ b < L.length := by
  intro i seen b hres
  unfold scanElseFrom at hres
  split at hres
  · rename_i hlt
    cases htok : L[i] with
    | trivia =>
      simp only [htok] at hres
      have ih := scanElseFrom_bounds L (i + 1) seen b hres
      omega
    | poison =>
      simp only [htok] at hres
      have ih := scanElseFrom_bounds L (i + 1) true b hres
      omega
    | elseKw =>
      simp only [htok] at hres
      cases seen with
      | true =>
        simp only [↓reduceIte, ElseScanResult.foundElse.injEq, reduceCtorEq, or_false] at hres
        subst hres
        omega
      | false =>
        simp only [Bool.false_eq_true, ↓reduceIte, reduceCtorEq, ElseScanResult.rejected.injEq, false_or] at hres
        subst hres
        omega
    | leftBrace =>
      simp only [htok, reduceCtorEq, ElseScanResult.rejected.injEq, false_or] at hres
      subst hres
      omega
    | other =>
      simp only [htok, reduceCtorEq, ElseScanResult.rejected.injEq, false_or] at hres
      subst hres
      omega
  · simp at hres
termination_by i _ _ _ => L.length - i

theorem scanElse_bounds (L : List LexClass) (start b : Nat)
    (h : scanElse L start = .foundElse b ∨ scanElse L start = .rejected b) :
    start ≤ b ∧ b < L.length :=
  scanElseFrom_bounds L start false b h

/-! ### E2: `ElsePoisonTail` cache soundness (`∀ c ∈ [start, boundary]`) -/

/-- Helper: if `scanElseFrom L i seen = .rejected b`, then for every `c` with `i ≤ c ≤ b`,
    starting fresh from `c` with `seen' = false` ALSO yields `.rejected b`. -/
theorem scanElseFrom_rejected_subrange (L : List LexClass) :
    ∀ (i : Nat) (seen : Bool) (b c : Nat),
      scanElseFrom L i seen = .rejected b →
      i ≤ c →
      c ≤ b →
      scanElseFrom L c false = .rejected b := by
  intro i seen b c hrej hic hcb
  unfold scanElseFrom at hrej
  split at hrej
  · rename_i hlt
    cases htok : L[i] with
    | trivia =>
      simp only [htok] at hrej
      by_cases hEq : c = i
      · subst c
        unfold scanElseFrom
        simp only [hlt, ↓reduceDIte, htok]
        have hbounds := scanElseFrom_bounds L (i + 1) seen b (Or.inr hrej)
        exact scanElseFrom_rejected_subrange L (i + 1) seen b (i + 1) hrej (Nat.le_refl _) hbounds.1
      · exact scanElseFrom_rejected_subrange L (i + 1) seen b c hrej (by omega) hcb
    | poison =>
      simp only [htok] at hrej
      by_cases hEq : c = i
      · subst c
        unfold scanElseFrom
        simp only [hlt, ↓reduceDIte, htok]
        exact hrej
      · exact scanElseFrom_rejected_subrange L (i + 1) true b c hrej (by omega) hcb
    | elseKw =>
      simp only [htok] at hrej
      cases seen with
      | true => simp at hrej
      | false =>
        simp only [Bool.false_eq_true, ↓reduceIte, ElseScanResult.rejected.injEq] at hrej
        subst hrej
        have hEq : c = i := by omega
        subst hEq
        unfold scanElseFrom
        simp [hlt, htok]
    | leftBrace =>
      simp only [htok, ElseScanResult.rejected.injEq] at hrej
      subst hrej
      have hEq : c = i := by omega
      subst hEq
      unfold scanElseFrom
      simp [hlt, htok]
    | other =>
      simp only [htok, ElseScanResult.rejected.injEq] at hrej
      subst hrej
      have hEq : c = i := by omega
      subst hEq
      unfold scanElseFrom
      simp [hlt, htok]
  · simp at hrej
termination_by i _ _ _ _ _ _ => L.length - i

/-- E2 (Main Cache Soundness Theorem): Whenever `scanElse L start = .rejected boundary`,
    for EVERY cursor `c` in `start..=boundary`, a fresh `scanElse L c` ALSO returns `.rejected boundary`
    (never `.foundElse`), proving the `ElsePoisonTail` cache shortcut is 100% sound. -/
theorem elsePoisonCache_sound (L : List LexClass) (start boundary c : Nat)
    (hrej : scanElse L start = .rejected boundary)
    (hlo : start ≤ c)
    (hhi : c ≤ boundary) :
    scanElse L c = .rejected boundary :=
  scanElseFrom_rejected_subrange L start false boundary c hrej hlo hhi

/-! ### E3: `foundElse` implies `L[b] = elseKw` -/

theorem scanElseFrom_foundElse_is_else (L : List LexClass) :
    ∀ (i : Nat) (seen : Bool) (b : Nat),
      scanElseFrom L i seen = .foundElse b →
      ∃ hlt : b < L.length, L[b] = .elseKw := by
  intro i seen b h
  unfold scanElseFrom at h
  split at h
  · rename_i hlt
    cases htok : L[i] with
    | trivia =>
      simp only [htok] at h
      exact scanElseFrom_foundElse_is_else L (i + 1) seen b h
    | poison =>
      simp only [htok] at h
      exact scanElseFrom_foundElse_is_else L (i + 1) true b h
    | elseKw =>
      simp only [htok] at h
      cases seen with
      | true =>
        simp only [↓reduceIte, ElseScanResult.foundElse.injEq] at h
        subst h
        exact ⟨hlt, htok⟩
      | false => simp at h
    | leftBrace => simp [htok] at h
    | other => simp [htok] at h
  · simp at h
termination_by i _ _ _ => L.length - i

theorem scanElse_foundElse_is_else (L : List LexClass) (start b : Nat)
    (h : scanElse L start = .foundElse b) :
    ∃ hlt : b < L.length, L[b] = .elseKw :=
  scanElseFrom_foundElse_is_else L start false b h

/-! ### E4: `adjacent_poison_tail_boundary` never consumes a same-line `LeftBrace` -/

structure AdjItem where
  cls : LexClass
  hasLineBreakBefore : Bool
  deriving DecidableEq, Repr

/-- Exact functional model of `adjacent_poison_tail_boundary` loop. -/
def scanAdjacentTail (L : List AdjItem) : Nat → Bool → Option Nat
  | i, seenPoison =>
    if h : i < L.length then
      let item := L[i]
      if item.cls == .trivia then
        scanAdjacentTail L (i + 1) seenPoison
      else if item.cls != .poison || item.hasLineBreakBefore then
        let trailingLambda := seenPoison && (item.cls == .leftBrace) && !item.hasLineBreakBefore
        if seenPoison && !trailingLambda then some i else none
      else
        scanAdjacentTail L (i + 1) true
    else
      none
termination_by i _ => L.length - i

/-- E4: `scanAdjacentTail` never returns a same-line `leftBrace` as the poison boundary,
    preserving trailing-lambda dispatch to Pratt. -/
theorem scanAdjacentTail_never_same_line_brace (L : List AdjItem) :
    ∀ (i : Nat) (seen : Bool) (b : Nat),
      scanAdjacentTail L i seen = some b →
      ∃ hlt : b < L.length,
        ¬(L[b].cls = .leftBrace ∧ L[b].hasLineBreakBefore = false) := by
  intro i seen b hres
  unfold scanAdjacentTail at hres
  split at hres
  · rename_i hlt
    dsimp only at hres
    split at hres
    · exact scanAdjacentTail_never_same_line_brace L (i + 1) seen b hres
    · split at hres
      · split at hres
        · rename_i hret
          cases hres
          refine ⟨hlt, ?_⟩
          rintro ⟨hcls, hnl⟩
          simp [hcls, hnl] at hret
        · cases hres
      · exact scanAdjacentTail_never_same_line_brace L (i + 1) true b hres
  · cases hres
termination_by i _ _ _ => L.length - i

end F6AdjacentPoison
