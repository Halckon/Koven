/-
  SPEC-0492 (F6 second slice): Machine-checked unbounded model proofs for
  `OrdinaryGroups` stack-count invariants, `RecoveryBoundaries::include_unclosed`
  monoid & non-shielding laws, and `GroupClosers::confirmed_closer` watermark
  monotonicity & safety (`crates/lang-frontend/src/parser/engine/recovery/ordinary_groups.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani bounded direct-Rust harnesses in `ordinary_group_kani.rs`.
-/

namespace F6OrdinaryGroups

inductive Kind where
  | paren
  | bracket
  | brace
  deriving DecidableEq, Repr

structure RecoveryBoundaries where
  localB : Option Nat := none
  callerElse : Option Nat := none
  callerBlock : Option Nat := none
  blockStmtSep : Option Nat := none
  deriving DecidableEq, Repr

/-- Exact model of Rust `Option<NonZeroUsize>::max` (where `None < Some(_)`). -/
def maxOpt : Option Nat → Option Nat → Option Nat
  | none, b => b
  | a, none => a
  | some a, some b => some (Nat.max a b)

/-- Exact model of `RecoveryBoundaries::include_unclosed`. -/
def includeUnclosed (parent child : RecoveryBoundaries) : RecoveryBoundaries :=
  { localB := maxOpt parent.localB child.localB
    callerElse := maxOpt parent.callerElse child.callerElse
    callerBlock := maxOpt parent.callerBlock child.callerBlock
    blockStmtSep := maxOpt parent.blockStmtSep child.blockStmtSep }

/-- Exact model of `at_or_after_cursor` in `GroupClosers::confirmed_closer`. -/
def atOrAfter (cursor : Nat) : Option Nat → Bool
  | none => false
  | some raw => decide (cursor ≤ raw)

/-! ### O1: `maxOpt` and `includeUnclosed` algebraic & non-shielding laws -/

theorem maxOpt_assoc (a b c : Option Nat) :
    maxOpt (maxOpt a b) c = maxOpt a (maxOpt b c) := by
  cases a <;> cases b <;> cases c <;> simp [maxOpt, Nat.max_assoc]

theorem maxOpt_comm (a b : Option Nat) :
    maxOpt a b = maxOpt b a := by
  cases a <;> cases b <;> simp [maxOpt, Nat.max_comm]

theorem maxOpt_idem (a : Option Nat) :
    maxOpt a a = a := by
  cases a <;> simp [maxOpt]

theorem maxOpt_atOrAfter_left (cursor : Nat) (a b : Option Nat)
    (h : atOrAfter cursor a = true) :
    atOrAfter cursor (maxOpt a b) = true := by
  cases a with
  | none => simp [atOrAfter] at h
  | some x =>
    cases b with
    | none => simpa [maxOpt] using h
    | some y =>
      simp only [atOrAfter, decide_eq_true_eq] at h
      simp only [maxOpt, atOrAfter, decide_eq_true_eq]
      exact Nat.le_trans h (Nat.le_max_left x y)

theorem maxOpt_atOrAfter_right (cursor : Nat) (a b : Option Nat)
    (h : atOrAfter cursor b = true) :
    atOrAfter cursor (maxOpt a b) = true := by
  rw [maxOpt_comm]
  exact maxOpt_atOrAfter_left cursor b a h

theorem includeUnclosed_assoc (a b c : RecoveryBoundaries) :
    includeUnclosed (includeUnclosed a b) c = includeUnclosed a (includeUnclosed b c) := by
  cases a; cases b; cases c
  simp [includeUnclosed, maxOpt_assoc]

theorem includeUnclosed_comm (a b : RecoveryBoundaries) :
    includeUnclosed a b = includeUnclosed b a := by
  cases a; cases b
  simp [includeUnclosed, maxOpt_comm]

theorem includeUnclosed_idem (a : RecoveryBoundaries) :
    includeUnclosed a a = a := by
  cases a
  simp [includeUnclosed, maxOpt_idem]

theorem includeUnclosed_preserves_child_local (cursor : Nat) (parent child : RecoveryBoundaries)
    (h : atOrAfter cursor child.localB = true) :
    atOrAfter cursor (includeUnclosed parent child).localB = true :=
  maxOpt_atOrAfter_right cursor parent.localB child.localB h

theorem includeUnclosed_preserves_child_else (cursor : Nat) (parent child : RecoveryBoundaries)
    (h : atOrAfter cursor child.callerElse = true) :
    atOrAfter cursor (includeUnclosed parent child).callerElse = true :=
  maxOpt_atOrAfter_right cursor parent.callerElse child.callerElse h

theorem includeUnclosed_preserves_child_block (cursor : Nat) (parent child : RecoveryBoundaries)
    (h : atOrAfter cursor child.callerBlock = true) :
    atOrAfter cursor (includeUnclosed parent child).callerBlock = true :=
  maxOpt_atOrAfter_right cursor parent.callerBlock child.callerBlock h

theorem includeUnclosed_preserves_child_sep (cursor : Nat) (parent child : RecoveryBoundaries)
    (h : atOrAfter cursor child.blockStmtSep = true) :
    atOrAfter cursor (includeUnclosed parent child).blockStmtSep = true :=
  maxOpt_atOrAfter_right cursor parent.blockStmtSep child.blockStmtSep h

/-! ### O2 & O3: `OrdinaryGroups` stack-count invariant & non-underflowing pop -/

structure OpenGroup where
  raw : Nat
  kind : Kind
  boundaries : RecoveryBoundaries
  deriving DecidableEq, Repr

structure ActiveCounts where
  paren : Nat := 0
  bracket : Nat := 0
  brace : Nat := 0
  deriving DecidableEq, Repr

def ActiveCounts.get (c : ActiveCounts) : Kind → Nat
  | .paren => c.paren
  | .bracket => c.bracket
  | .brace => c.brace

def ActiveCounts.inc (c : ActiveCounts) : Kind → ActiveCounts
  | .paren => { c with paren := c.paren + 1 }
  | .bracket => { c with bracket := c.bracket + 1 }
  | .brace => { c with brace := c.brace + 1 }

def ActiveCounts.dec (c : ActiveCounts) : Kind → ActiveCounts
  | .paren => { c with paren := c.paren - 1 }
  | .bracket => { c with bracket := c.bracket - 1 }
  | .brace => { c with brace := c.brace - 1 }

theorem ActiveCounts.get_inc (c : ActiveCounts) (k1 k2 : Kind) :
    (c.inc k1).get k2 = (if k1 == k2 then 1 else 0) + c.get k2 := by
  cases k1 <;> cases k2 <;> simp [ActiveCounts.inc, ActiveCounts.get, Nat.add_comm]

theorem ActiveCounts.get_dec (c : ActiveCounts) (k1 k2 : Kind) :
    (c.dec k1).get k2 = if k1 == k2 then c.get k2 - 1 else c.get k2 := by
  cases k1 <;> cases k2 <;> simp [ActiveCounts.dec, ActiveCounts.get]

def countKind : List OpenGroup → Kind → Nat
  | [], _ => 0
  | g :: rest, k => (if g.kind == k then 1 else 0) + countKind rest k

structure OrdinaryGroupsState where
  groups : List OpenGroup := []
  active : ActiveCounts := {}
  deriving DecidableEq, Repr

def WellFormed (st : OrdinaryGroupsState) : Prop :=
  ∀ k : Kind, st.active.get k = countKind st.groups k

theorem empty_wellFormed : WellFormed {} := by
  intro k
  cases k <;> rfl

/-- Exact model of the `while let Some(mut group) = self.groups.pop()` loop in `visit_group`. -/
def popUntil (k : Kind) (acc : RecoveryBoundaries) :
    List OpenGroup → ActiveCounts → Option (OpenGroup × List OpenGroup × ActiveCounts)
  | [], _ => none
  | g :: rest, active =>
    let active' := active.dec g.kind
    let acc' := includeUnclosed acc g.boundaries
    if g.kind == k then
      some ({ g with boundaries := acc' }, rest, active')
    else
      popUntil k acc' rest active'

/-- O2: Whenever `active` matches `countKind` and `active.get k > 0`, `popUntil`
    always finds a matching opener of kind `k`, never underflows any counter, and
    preserves `WellFormed`. -/
theorem popUntil_complete_and_wf (k : Kind) (acc : RecoveryBoundaries) :
    ∀ (stack : List OpenGroup) (active : ActiveCounts),
      (∀ k' : Kind, active.get k' = countKind stack k') →
      active.get k > 0 →
      ∃ matched rest active',
        popUntil k acc stack active = some (matched, rest, active') ∧
        matched.kind = k ∧
        (∀ k' : Kind, active'.get k' = countKind rest k') := by
  intro stack
  induction stack generalizing acc with
  | nil =>
    intro active hwf hpos
    have h0 := hwf k
    simp [countKind] at h0
    omega
  | cons g rest ih =>
    intro active hwf hpos
    have hwf_dec : ∀ k' : Kind, (active.dec g.kind).get k' = countKind rest k' := by
      intro k'
      rw [ActiveCounts.get_dec]
      have hk := hwf k'
      simp only [countKind] at hk
      by_cases heq : g.kind = k'
      · subst heq
        simp only [beq_self_eq_true, ↓reduceIte] at hk ⊢
        omega
      · have hne : (g.kind == k') = false := beq_false_of_ne heq
        simp only [hne, Bool.false_eq_true, ↓reduceIte] at hk ⊢
        omega
    by_cases hgk : g.kind = k
    · subst hgk
      refine ⟨{ g with boundaries := includeUnclosed acc g.boundaries }, rest, active.dec g.kind, ?_, rfl, hwf_dec⟩
      simp [popUntil]
    · have hne : (g.kind == k) = false := beq_false_of_ne hgk
      have hpos' : (active.dec g.kind).get k > 0 := by
        rw [ActiveCounts.get_dec, hne]
        simpa using hpos
      obtain ⟨matched, rest', active', hpop, hkind, hwf'⟩ :=
        ih (includeUnclosed acc g.boundaries) (active.dec g.kind) hwf_dec hpos'
      refine ⟨matched, rest', active', ?_, hkind, hwf'⟩
      simp [popUntil, hne, hpop]

/-- Exact model of `OrdinaryGroups::visit_group`. -/
def visitGroup (st : OrdinaryGroupsState) (raw : Nat) (k : Kind) (opening : Bool) :
    OrdinaryGroupsState × Option OpenGroup :=
  if opening then
    ({ groups := { raw := raw, kind := k, boundaries := {} } :: st.groups
       active := st.active.inc k }, none)
  else if k == Kind.brace && st.active.get Kind.brace == 0 then
    ({ groups := [], active := {} }, none)
  else if st.active.get k != 0 then
    match popUntil k {} st.groups st.active with
    | some (matched, rest, active') => ({ groups := rest, active := active' }, some matched)
    | none => (st, none)
  else
    (st, none)

/-- O3a: Single-step `visitGroup` preserves `WellFormed`. -/
theorem visitGroup_preserves_wf (st : OrdinaryGroupsState) (raw : Nat) (k : Kind) (opening : Bool)
    (hwf : WellFormed st) :
    WellFormed (visitGroup st raw k opening).1 := by
  unfold visitGroup
  by_cases hop : opening = true
  · simp only [hop, ↓reduceIte]
    intro k'
    rw [ActiveCounts.get_inc]
    simp [countKind, hwf k']
  · simp only [hop, Bool.false_eq_true, ↓reduceIte]
    by_cases hbrace : (k == Kind.brace && st.active.get Kind.brace == 0) = true
    · simp only [hbrace, ↓reduceIte]
      exact empty_wellFormed
    · simp only [hbrace, ↓reduceIte]
      by_cases hact : (st.active.get k != 0) = true
      · simp only [hact, ↓reduceIte]
        have hpos : st.active.get k > 0 := by
          simp only [bne_iff_ne, ne_eq] at hact
          omega
        obtain ⟨matched, rest, active', hpop, _, hwf'⟩ :=
          popUntil_complete_and_wf k {} st.groups st.active hwf hpos
        simp [hpop, WellFormed, hwf']
      · simp only [hact, ↓reduceIte]
        exact hwf

/-- O3b: `visitGroup` on a closing delimiter returns `some` iff `countKind st.groups k > 0`. -/
theorem visitGroup_close_matches_iff (st : OrdinaryGroupsState) (raw : Nat) (k : Kind)
    (hwf : WellFormed st) :
    (visitGroup st raw k false).2.isSome = true ↔ countKind st.groups k > 0 := by
  unfold visitGroup
  simp only [Bool.false_eq_true, ↓reduceIte]
  have hcnt := hwf k
  by_cases hbrace : (k == Kind.brace && st.active.get Kind.brace == 0) = true
  · simp only [hbrace, ↓reduceIte, Option.isSome_none, Bool.false_eq_true, false_iff]
    simp only [Bool.and_eq_true, beq_iff_eq] at hbrace
    rcases hbrace with ⟨rfl, hzero⟩
    omega
  · simp only [hbrace, ↓reduceIte]
    by_cases hact : (st.active.get k != 0) = true
    · simp only [hact, ↓reduceIte]
      have hpos : st.active.get k > 0 := by
        simp only [bne_iff_ne, ne_eq] at hact
        omega
      obtain ⟨matched, rest, active', hpop, _, _⟩ :=
        popUntil_complete_and_wf k {} st.groups st.active hwf hpos
      simp [hpop]
      omega
    · simp only [hact, ↓reduceIte, Option.isSome_none, Bool.false_eq_true, false_iff]
      simp only [bne_iff_ne, ne_eq, decide_eq_true_eq, Decidable.not_not] at hact
      omega

def runEvents (st : OrdinaryGroupsState) : List (Nat × Kind × Bool) → OrdinaryGroupsState
  | [] => st
  | (raw, k, opening) :: rest => runEvents (visitGroup st raw k opening).1 rest

/-- O3c: Arbitrary-length delimiter sequences preserve `WellFormed`. -/
theorem runEvents_preserves_wf (events : List (Nat × Kind × Bool)) :
    ∀ st : OrdinaryGroupsState, WellFormed st → WellFormed (runEvents st events) := by
  induction events with
  | nil =>
    intro st hwf
    exact hwf
  | cons ev rest ih =>
    rcases ev with ⟨raw, k, opening⟩
    intro st hwf
    exact ih (visitGroup st raw k opening).1 (visitGroup_preserves_wf st raw k opening hwf)

/-! ### O4 & O5: `GroupClosers::confirmed_closer` monotonicity & boundary safety -/

structure StopFlags where
  hasElse : Bool
  hasBlockElements : Bool
  hasBlockStmtSep : Bool
  deriving DecidableEq, Repr

def StopFlags.le (a b : StopFlags) : Prop :=
  (a.hasElse = true → b.hasElse = true) ∧
  (a.hasBlockElements = true → b.hasBlockElements = true) ∧
  (a.hasBlockStmtSep = true → b.hasBlockStmtSep = true)

def blockedByBoundaries (b : RecoveryBoundaries) (cursor : Nat) (stops : StopFlags) : Bool :=
  atOrAfter cursor b.localB ||
  (stops.hasElse && atOrAfter cursor b.callerElse) ||
  (stops.hasBlockElements && atOrAfter cursor b.callerBlock) ||
  (stops.hasBlockStmtSep && atOrAfter cursor b.blockStmtSep)

/-- Exact model of `GroupClosers::confirmed_closer` once opener lookup and enclosing-closer
    existence (`enclosingClosed`) are resolved. -/
def confirmedCloserCore (closer : Nat) (enclosingClosed : Bool) (b : RecoveryBoundaries)
    (cursor : Nat) (stops : StopFlags) : Option Nat :=
  if !enclosingClosed then none
  else if blockedByBoundaries b cursor stops then none
  else some closer

theorem atOrAfter_antitone_cursor {c1 c2 : Nat} (hle : c1 ≤ c2) (opt : Option Nat)
    (h : atOrAfter c2 opt = true) :
    atOrAfter c1 opt = true := by
  cases opt with
  | none => simp [atOrAfter] at h
  | some raw =>
    simp only [atOrAfter, decide_eq_true_eq] at h ⊢
    omega

/-- O4a: Stop-set antitonicity — adding caller stops can only turn `some closer` into `none`. -/
theorem confirmedCloser_antitone_stops (closer : Nat) (enc : Bool) (b : RecoveryBoundaries)
    (cursor : Nat) (s1 s2 : StopFlags)
    (hle : s1.le s2)
    (h : confirmedCloserCore closer enc b cursor s2 = some closer) :
    confirmedCloserCore closer enc b cursor s1 = some closer := by
  unfold confirmedCloserCore at h ⊢
  cases enc with
  | false => simp at h
  | true =>
    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at h ⊢
    cases hb2 : blockedByBoundaries b cursor s2 with
    | true => simp [hb2] at h
    | false =>
      have hb1 : blockedByBoundaries b cursor s1 = false := by
        cases h1 : blockedByBoundaries b cursor s1 with
        | false => rfl
        | true =>
          exfalso
          rcases hle with ⟨hElse, hBlk, hSep⟩
          unfold blockedByBoundaries at hb2 h1
          simp only [Bool.or_eq_true, Bool.and_eq_true] at h1
          simp only [Bool.or_eq_false_iff, Bool.and_eq_false_iff] at hb2
          rcases hb2 with ⟨⟨⟨hL2, hE2⟩, hB2⟩, hS2⟩
          rcases h1 with ((hL1 | ⟨hsE, hE1⟩) | ⟨hsB, hB1⟩) | ⟨hsS, hS1⟩
          · simp [hL1] at hL2
          · have hsE2 := hElse hsE
            cases hE2 with
            | inl h0 => simp [hsE2] at h0
            | inr h0 => simp [hE1] at h0
          · have hsB2 := hBlk hsB
            cases hB2 with
            | inl h0 => simp [hsB2] at h0
            | inr h0 => simp [hB1] at h0
          · have hsS2 := hSep hsS
            cases hS2 with
            | inl h0 => simp [hsS2] at h0
            | inr h0 => simp [hS1] at h0
      simp [hb1]

/-- O4b: Cursor monotonicity — advancing `cursor1 ≤ cursor2` never revokes a confirmed closer. -/
theorem confirmedCloser_monotone_cursor (closer : Nat) (enc : Bool) (b : RecoveryBoundaries)
    (c1 c2 : Nat) (stops : StopFlags)
    (hle : c1 ≤ c2)
    (h : confirmedCloserCore closer enc b c1 stops = some closer) :
    confirmedCloserCore closer enc b c2 stops = some closer := by
  unfold confirmedCloserCore at h ⊢
  cases enc with
  | false => simp at h
  | true =>
    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at h ⊢
    cases hb1 : blockedByBoundaries b c1 stops with
    | true => simp [hb1] at h
    | false =>
      have hb2 : blockedByBoundaries b c2 stops = false := by
        cases h2 : blockedByBoundaries b c2 stops with
        | false => rfl
        | true =>
          exfalso
          unfold blockedByBoundaries at hb1 h2
          simp only [Bool.or_eq_true, Bool.and_eq_true] at h2
          simp only [Bool.or_eq_false_iff, Bool.and_eq_false_iff] at hb1
          rcases hb1 with ⟨⟨⟨hL1, hE1⟩, hB1⟩, hS1⟩
          rcases h2 with ((hL2 | ⟨hsE, hE2⟩) | ⟨hsB, hB2⟩) | ⟨hsS, hS2⟩
          · have := atOrAfter_antitone_cursor hle b.localB hL2; simp [this] at hL1
          · have := atOrAfter_antitone_cursor hle b.callerElse hE2
            cases hE1 with
            | inl h => simp [hsE] at h
            | inr h => simp [this] at h
          · have := atOrAfter_antitone_cursor hle b.callerBlock hB2
            cases hB1 with
            | inl h => simp [hsB] at h
            | inr h => simp [this] at h
          · have := atOrAfter_antitone_cursor hle b.blockStmtSep hS2
            cases hS1 with
            | inl h => simp [hsS] at h
            | inr h => simp [this] at h
      simp [hb2]

/-- O5: Boundary safety — a confirmed closer guarantees enclosing closure and zero active
    watermarks at or after `cursor`. -/
theorem confirmedCloser_boundary_safety (closer : Nat) (enc : Bool) (b : RecoveryBoundaries)
    (cursor : Nat) (stops : StopFlags) (c : Nat)
    (h : confirmedCloserCore closer enc b cursor stops = some c) :
    c = closer ∧
    enc = true ∧
    atOrAfter cursor b.localB = false ∧
    (stops.hasElse = true → atOrAfter cursor b.callerElse = false) ∧
    (stops.hasBlockElements = true → atOrAfter cursor b.callerBlock = false) ∧
    (stops.hasBlockStmtSep = true → atOrAfter cursor b.blockStmtSep = false) := by
  unfold confirmedCloserCore at h
  cases enc with
  | false => simp at h
  | true =>
    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at h
    cases hb : blockedByBoundaries b cursor stops with
    | true => simp [hb] at h
    | false =>
      simp only [hb, Bool.false_eq_true, ↓reduceIte, Option.some.injEq] at h
      unfold blockedByBoundaries at hb
      simp only [Bool.or_eq_false_iff, Bool.and_eq_false_iff] at hb
      rcases hb with ⟨⟨⟨hL, hE⟩, hB⟩, hS⟩
      refine ⟨h.symm, rfl, hL, ?_, ?_, ?_⟩
      · intro hs; cases hE with | inl h0 => simp [hs] at h0 | inr h0 => exact h0
      · intro hs; cases hB with | inl h0 => simp [hs] at h0 | inr h0 => exact h0
      · intro hs; cases hS with | inl h0 => simp [hs] at h0 | inr h0 => exact h0

end F6OrdinaryGroups
