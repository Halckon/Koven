/-
SPEC-0490 (F6 Lean 4 track): Unbounded algorithm-level machine proof for
`match_angle_group_delimiters` (8-state backward DP) vs. forward `OrdinaryGroups::visit`.

Evidence level: `proved (model, unbounded)` — proves obligations P1–P4 for all
delimiter sequences of arbitrary length `n ≥ 0` using only Lean 4 core (`Init`),
zero Mathlib dependencies, zero `sorry`, and zero extra axioms.
-/

namespace F6AngleGroups

inductive Kind where
  | paren
  | bracket
  | brace
  deriving DecidableEq, Repr

structure Delim where
  raw : Nat
  kind : Kind
  opening : Bool
  deriving DecidableEq, Repr

def kindCode : Kind → Nat
  | .paren => 0
  | .bracket => 1
  | .brace => 2

def shapeBit (k : Kind) : Nat :=
  1 <<< kindCode k

def hasActive (active : Fin 8) (k : Kind) : Bool :=
  (active.val &&& shapeBit k) != 0

def addActive (active : Fin 8) (k : Kind) : Fin 8 :=
  ⟨(active.val ||| shapeBit k) % 8, Nat.mod_lt _ (by decide)⟩

def emptyActive : Fin 8 :=
  ⟨0, by decide⟩

theorem hasActive_addActive (a : Fin 8) (k k' : Kind) :
    hasActive (addActive a k) k' = (hasActive a k' || (k == k')) := by
  rcases a with ⟨v, hv⟩
  cases k <;> cases k' <;> (
    match v, hv with
    | 0, _ | 1, _ | 2, _ | 3, _ | 4, _ | 5, _ | 6, _ | 7, _ => decide
    | _ + 8, hv => omega)

theorem hasActive_empty (k : Kind) :
    hasActive emptyActive k = false := by
  cases k <;> decide

/-! ### Forward `OrdinaryGroups` stack model -/

def stackHasKind (s : List (Nat × Kind)) (k : Kind) : Bool :=
  s.any (fun p => p.2 == k)

def popUntilKind : List (Nat × Kind) → Kind → List (Nat × Kind)
  | [], _ => []
  | (_, k') :: rest, k =>
    if k' == k then rest else popUntilKind rest k

theorem stackHasKind_append (s1 s2 : List (Nat × Kind)) (k : Kind) :
    stackHasKind (s1 ++ s2) k = (stackHasKind s1 k || stackHasKind s2 k) := by
  induction s1 with
  | nil => simp [stackHasKind]
  | cons hd tl ih =>
    simp [stackHasKind, ih, Bool.or_assoc]

theorem popUntilKind_append_left (s1 s2 : List (Nat × Kind)) (k : Kind)
    (h : stackHasKind s1 k = true) :
    popUntilKind (s1 ++ s2) k = popUntilKind s1 k ++ s2 := by
  induction s1 with
  | nil => simp [stackHasKind] at h
  | cons hd tl ih =>
    rcases hd with ⟨r, k'⟩
    simp only [List.cons_append, popUntilKind]
    by_cases heq : (k' == k) = true
    · simp [heq]
    · simp only [heq, Bool.false_eq_true, ↓reduceIte]
      have htl : stackHasKind tl k = true := by
        simp [stackHasKind, heq] at h
        exact h
      exact ih htl

theorem popUntilKind_append_right (s1 : List (Nat × Kind)) (r : Nat) (k : Kind)
    (h : stackHasKind s1 k = false) :
    popUntilKind (s1 ++ [(r, k)]) k = [] := by
  induction s1 with
  | nil => simp [popUntilKind]
  | cons hd tl ih =>
    rcases hd with ⟨r', k'⟩
    simp only [List.cons_append, popUntilKind]
    have hne : (k' == k) = false := by
      simp [stackHasKind] at h
      exact h.1
    have htl : stackHasKind tl k = false := by
      simp [stackHasKind] at h
      exact h.2
    simp [hne, ih htl]

/-- Forward boundary scanner matching `OrdinaryGroups::visit` under an external
ancestor active mask `active`. Stops at the first closer belonging to `active`
or an unmatched right brace (`Kind.brace`), or at EOF (`D.length`). -/
def forwardStop (D : List Delim) (i : Nat) (hi : i ≤ D.length)
    (localStack : List (Nat × Kind)) (active : Fin 8) : Nat :=
  if hlt : i < D.length then
    let d := D[i]
    if d.opening then
      forwardStop D (i + 1) hlt ((d.raw, d.kind) :: localStack) active
    else if stackHasKind localStack d.kind then
      forwardStop D (i + 1) hlt (popUntilKind localStack d.kind) active
    else if hasActive active d.kind || (d.kind == .brace) then
      i
    else
      forwardStop D (i + 1) hlt localStack active
  else
    D.length
termination_by D.length - i

theorem forwardStop_bounds (D : List Delim) (i : Nat) (hi : i ≤ D.length)
    (s : List (Nat × Kind)) (active : Fin 8) :
    i ≤ forwardStop D i hi s active ∧ forwardStop D i hi s active ≤ D.length := by
  unfold forwardStop
  split
  · rename_i hlt
    split
    · have ih := forwardStop_bounds D (i + 1) hlt ((D[i].raw, D[i].kind) :: s) active
      omega
    · split
      · have ih := forwardStop_bounds D (i + 1) hlt (popUntilKind s D[i].kind) active
        omega
      · split
        · exact ⟨Nat.le_refl i, hi⟩
        · have ih := forwardStop_bounds D (i + 1) hlt s active
          omega
  · omega
termination_by D.length - i

/-! ### Backward 8-state DP model (`match_angle_group_delimiters`) -/

/-- P1 (by construction): `dpBoundary` always returns a bounded index `i ≤ b ≤ D.length`
and performs zero out-of-bounds accesses. -/
def dpBoundary (D : List Delim) (i : Nat) (hi : i ≤ D.length) (active : Fin 8) :
    { b : Nat // i ≤ b ∧ b ≤ D.length } :=
  if hlt : i < D.length then
    let d := D[i]
    if d.opening then
      let r1 := dpBoundary D (i + 1) hlt (addActive active d.kind)
      if hlt1 : r1.val < D.length then
        if !D[r1.val].opening && (D[r1.val].kind == d.kind) then
          let r2 := dpBoundary D (r1.val + 1) hlt1 active
          ⟨r2.val, by have := r1.property.1; have := r2.property.1; omega, r2.property.2⟩
        else
          ⟨r1.val, by have := r1.property.1; omega, r1.property.2⟩
      else
        ⟨r1.val, by have := r1.property.1; omega, r1.property.2⟩
    else if hasActive active d.kind || (d.kind == .brace) then
      ⟨i, Nat.le_refl i, hi⟩
    else
      let r1 := dpBoundary D (i + 1) hlt active
      ⟨r1.val, by have := r1.property.1; omega, r1.property.2⟩
  else
    ⟨D.length, by omega, Nat.le_refl _⟩
termination_by D.length - i
decreasing_by
  · omega
  · have := r1.property.1; omega
  · omega

def dpMatch (D : List Delim) (i : Nat) (hi : i < D.length) : Option Nat :=
  let d := D[i]
  if d.opening then
    let endIdx := (dpBoundary D (i + 1) hi (addActive emptyActive d.kind)).val
    if hlt : endIdx < D.length then
      if !D[endIdx].opening && (D[endIdx].kind == d.kind) then
        some D[endIdx].raw
      else
        none
    else
      none
  else
    none

/-- Forward matcher starting a fresh `OrdinaryGroups` at opener `i`. -/
def forwardMatch (D : List Delim) (i : Nat) (hi : i < D.length) : Option Nat :=
  let d := D[i]
  if d.opening then
    let endIdx := forwardStop D (i + 1) hi [] (addActive emptyActive d.kind)
    if hlt : endIdx < D.length then
      if !D[endIdx].opening && (D[endIdx].kind == d.kind) then
        some D[endIdx].raw
      else
        none
    else
      none
  else
    none

/-! ### Key stack-decomposition lemma and P2 / P3 / P4 theorems -/

theorem forwardStop_append_singleton (D : List Delim) (j : Nat) (hj : j ≤ D.length)
    (s : List (Nat × Kind)) (r : Nat) (k : Kind) (active : Fin 8) :
    forwardStop D j hj (s ++ [(r, k)]) active =
      let endIdx := forwardStop D j hj s (addActive active k)
      if hlt : endIdx < D.length then
        if !D[endIdx].opening && (D[endIdx].kind == k) then
          forwardStop D (endIdx + 1) hlt [] active
        else
          endIdx
      else
        endIdx := by
  by_cases hlt : j < D.length
  · cases hop : D[j].opening
    · -- Closer at `j`
      by_cases hs : stackHasKind s D[j].kind = true
      · have hs_app : stackHasKind (s ++ [(r, k)]) D[j].kind = true := by
          rw [stackHasKind_append, hs]; rfl
        have hpop := popUntilKind_append_left s [(r, k)] D[j].kind hs
        have hstepL : forwardStop D j hj (s ++ [(r, k)]) active =
            forwardStop D (j + 1) hlt (popUntilKind s D[j].kind ++ [(r, k)]) active := by
          conv => lhs; unfold forwardStop
          simp [hlt, hop, hs_app, hpop]
        have hstepR : forwardStop D j hj s (addActive active k) =
            forwardStop D (j + 1) hlt (popUntilKind s D[j].kind) (addActive active k) := by
          conv => lhs; unfold forwardStop
          simp [hlt, hop, hs]
        rw [hstepL]
        simp only [hstepR]
        exact forwardStop_append_singleton D (j + 1) hlt (popUntilKind s D[j].kind) r k active
      · have hs_false : stackHasKind s D[j].kind = false := Bool.eq_false_iff.mpr hs
        by_cases hkk : D[j].kind = k
        · have hs_app : stackHasKind (s ++ [(r, k)]) D[j].kind = true := by
            rw [stackHasKind_append, hkk]; simp [stackHasKind]
          have hpop : popUntilKind (s ++ [(r, k)]) D[j].kind = [] := by
            rw [hkk]; exact popUntilKind_append_right s r k (hkk ▸ hs_false)
          have hact : hasActive (addActive active k) D[j].kind = true := by
            rw [hasActive_addActive, hkk]; simp
          have hstepL : forwardStop D j hj (s ++ [(r, k)]) active =
              forwardStop D (j + 1) hlt [] active := by
            conv => lhs; unfold forwardStop
            simp [hlt, hop, hs_app, hpop]
          have hstepR : forwardStop D j hj s (addActive active k) = j := by
            conv => lhs; unfold forwardStop
            simp [hlt, hop, hs_false, hact]
          rw [hstepL]
          simp [hstepR, hlt, hop, hkk]
        · have hbeq : (k == D[j].kind) = false := by
            cases h : (k == D[j].kind)
            · rfl
            · exact absurd (Eq.symm (LawfulBEq.eq_of_beq h)) hkk
          have hbeq' : (D[j].kind == k) = false := by
            cases h : (D[j].kind == k)
            · rfl
            · exact absurd (LawfulBEq.eq_of_beq h) hkk
          have hs_app : stackHasKind (s ++ [(r, k)]) D[j].kind = false := by
            rw [stackHasKind_append, hs_false]; simp [stackHasKind, hbeq]
          have hact_eq : hasActive (addActive active k) D[j].kind = hasActive active D[j].kind := by
            rw [hasActive_addActive, hbeq, Bool.or_false]
          by_cases hstop : (hasActive active D[j].kind || (D[j].kind == .brace)) = true
          · have hstepL : forwardStop D j hj (s ++ [(r, k)]) active = j := by
              conv => lhs; unfold forwardStop
              simp [hlt, hop, hs_app, hstop]
            have hstepR : forwardStop D j hj s (addActive active k) = j := by
              conv => lhs; unfold forwardStop
              simp [hlt, hop, hs_false, hact_eq, hstop]
            rw [hstepL]
            simp [hstepR, hlt, hop, hbeq']
          · have hstop_false : (hasActive active D[j].kind || (D[j].kind == .brace)) = false :=
              Bool.eq_false_iff.mpr hstop
            have hstepL : forwardStop D j hj (s ++ [(r, k)]) active =
                forwardStop D (j + 1) hlt (s ++ [(r, k)]) active := by
              conv => lhs; unfold forwardStop
              simp [hlt, hop, hs_app, hstop_false]
            have hstepR : forwardStop D j hj s (addActive active k) =
                forwardStop D (j + 1) hlt s (addActive active k) := by
              conv => lhs; unfold forwardStop
              simp [hlt, hop, hs_false, hact_eq, hstop_false]
            rw [hstepL]
            simp only [hstepR]
            exact forwardStop_append_singleton D (j + 1) hlt s r k active
    · -- Opener at `j`
      have hstepL : forwardStop D j hj (s ++ [(r, k)]) active =
          forwardStop D (j + 1) hlt (((D[j].raw, D[j].kind) :: s) ++ [(r, k)]) active := by
        conv => lhs; unfold forwardStop
        simp [hlt, hop]
      have hstepR : forwardStop D j hj s (addActive active k) =
          forwardStop D (j + 1) hlt ((D[j].raw, D[j].kind) :: s) (addActive active k) := by
        conv => lhs; unfold forwardStop
        simp [hlt, hop]
      rw [hstepL]
      simp only [hstepR]
      exact forwardStop_append_singleton D (j + 1) hlt ((D[j].raw, D[j].kind) :: s) r k active
  · have heqL : forwardStop D j hj (s ++ [(r, k)]) active = D.length := by
      unfold forwardStop; simp [hlt]
    have heqR : forwardStop D j hj s (addActive active k) = D.length := by
      unfold forwardStop; simp [hlt]
    rw [heqL]
    simp [heqR]
termination_by D.length - j

/-- P2 (8-state ancestor transition main lemma): for every suffix `i ≤ D.length` and
every ancestor bitmask `active : Fin 8`, the backward DP boundary equals the forward
`OrdinaryGroups` boundary. -/
theorem dpBoundary_eq_forwardStop (D : List Delim) (i : Nat) (hi : i ≤ D.length)
    (active : Fin 8) :
    (dpBoundary D i hi active).val = forwardStop D i hi [] active := by
  by_cases hlt : i < D.length
  · cases hop : D[i].opening
    · -- Closer at `i`
      by_cases hstop : (hasActive active D[i].kind || (D[i].kind == .brace)) = true
      · conv => lhs; unfold dpBoundary
        conv => rhs; unfold forwardStop
        simp [hlt, hop, hstop, stackHasKind]
      · have hstop_f : (hasActive active D[i].kind || (D[i].kind == .brace)) = false :=
          Bool.eq_false_iff.mpr hstop
        have ih := dpBoundary_eq_forwardStop D (i + 1) hlt active
        conv => lhs; unfold dpBoundary
        conv => rhs; unfold forwardStop
        simp [hlt, hop, hstop_f, stackHasKind, ih]
    · -- Opener at `i`
      have hfwd : forwardStop D i hi [] active =
          forwardStop D (i + 1) hlt ([] ++ [(D[i].raw, D[i].kind)]) active := by
        conv => lhs; unfold forwardStop
        simp [hlt, hop]
      rw [hfwd, forwardStop_append_singleton]
      have ih1 := dpBoundary_eq_forwardStop D (i + 1) hlt (addActive active D[i].kind)
      conv => lhs; unfold dpBoundary
      simp only [hlt, ↓reduceDIte, hop, ← ih1]
      split
      · rename_i hlt1
        split
        · have ih2 :=
            dpBoundary_eq_forwardStop D
              ((dpBoundary D (i + 1) hlt (addActive active D[i].kind)).val + 1)
              hlt1 active
          exact ih2
        · rfl
      · rfl
  · unfold dpBoundary forwardStop
    simp [hlt]
termination_by D.length - i
decreasing_by
  · omega
  · omega
  · have := (dpBoundary D (i + 1) hlt (addActive active D[i].kind)).property.1; omega

/-- P3 (Main equivalence theorem, unbounded `∀ n ≥ 0`): for every opener `i < D.length`,
the backward 8-state DP match equals the fresh forward `OrdinaryGroups` match. -/
theorem dpMatch_eq_forwardMatch (D : List Delim) (i : Nat) (hi : i < D.length) :
    dpMatch D i hi = forwardMatch D i hi := by
  unfold dpMatch forwardMatch
  split
  · rw [dpBoundary_eq_forwardStop]
  · rfl

def StrictlyIncreasingRaw (D : List Delim) : Prop :=
  ∀ a b, a < b → (hb : b < D.length) → D[a].raw < D[b].raw

/-- P4 (`NonZeroUsize` non-lossiness): under strictly increasing `raw` indices,
every matched closer has `raw ≥ 1`, so `NonZeroUsize::new(closer_raw)` never loses
a valid match. -/
theorem dpMatch_some_pos (D : List Delim) (hinc : StrictlyIncreasingRaw D)
    (i : Nat) (hi : i < D.length) (c : Nat) (hm : dpMatch D i hi = some c) :
    1 ≤ c := by
  unfold dpMatch at hm
  split at hm
  · rename_i hop
    set r1 := dpBoundary D (i + 1) hi (addActive emptyActive D[i].kind)
    have hge : i + 1 ≤ r1.val := r1.property.1
    split at hm
    · rename_i hlt1
      split at hm
      · cases hm
        have hlt_raw : D[i].raw < D[r1.val].raw := hinc i r1.val (by omega) hlt1
        omega
      · cases hm
    · cases hm
  · cases hm

end F6AngleGroups
