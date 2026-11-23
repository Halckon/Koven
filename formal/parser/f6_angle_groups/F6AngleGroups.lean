/-
SPEC-0490 (F6 Lean 4 track): Unbounded algorithm-level machine proof for
`match_angle_group_delimiters` (8-state backward DP) vs. forward `OrdinaryGroups::visit`.

Evidence level: `proved (model, unbounded)` — proves obligations P1–P4 for all
delimiter sequences of arbitrary length `n ≥ 0` using only Lean 4 core (`Init`),
zero Mathlib dependencies, and zero unproven placeholders or extra axioms.
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
  simp only [hasActive, addActive]
  cases k <;> cases k' <;> (
    match v, hv with
    | 0, _ | 1, _ | 2, _ | 3, _ | 4, _ | 5, _ | 6, _ | 7, _ => rfl
    | _ + 8, hv => omega)

theorem hasActive_empty (k : Kind) :
    hasActive emptyActive k = false := by
  cases k <;> rfl

/-! ### Forward `OrdinaryGroups` stack model -/

def stackHasKind : List (Nat × Kind) → Kind → Bool
  | [], _ => false
  | (_, k') :: rest, k => (k' == k) || stackHasKind rest k

def popUntilKind : List (Nat × Kind) → Kind → List (Nat × Kind)
  | [], _ => []
  | (_, k') :: rest, k =>
    if k' == k then rest else popUntilKind rest k

/-- Exact model of `OrdinaryGroups::visit_group`'s pop loop returning both the
popped opener's `raw` identity and the remaining stack. -/
def popUntilKindWithOpener : List (Nat × Kind) → Kind → Option (Nat × List (Nat × Kind))
  | [], _ => none
  | (r, k') :: rest, k =>
    if k' == k then some (r, rest) else popUntilKindWithOpener rest k

/-- Exact model of `OrdinaryGroups::visit(raw, symbol)` returning the updated stack
and `Option Nat` (the popped opener's `raw`, if a group closed). -/
def ordinaryVisitStep (s : List (Nat × Kind)) (d : Delim) : List (Nat × Kind) × Option Nat :=
  if d.opening then
    ((d.raw, d.kind) :: s, none)
  else if d.kind == .brace && !stackHasKind s .brace then
    ([], none)
  else if stackHasKind s d.kind then
    match popUntilKindWithOpener s d.kind with
    | some (openerRaw, rest) => (rest, some openerRaw)
    | none => (s, none)
  else
    (s, none)

theorem stackHasKind_append (s1 s2 : List (Nat × Kind)) (k : Kind) :
    stackHasKind (s1 ++ s2) k = (stackHasKind s1 k || stackHasKind s2 k) := by
  induction s1 with
  | nil => simp [stackHasKind]
  | cons hd tl ih =>
    rcases hd with ⟨r, k'⟩
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
      have hne : (k' == k) = false := Bool.eq_false_iff.mpr heq
      have htl : stackHasKind tl k = true := by
        simp [stackHasKind, hne] at h
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
    simp only [stackHasKind, Bool.or_eq_false_iff] at h
    simp [h.1, ih h.2]

def AllRawGt (s : List (Nat × Kind)) (target : Nat) : Prop :=
  ∀ p ∈ s, target < p.1

theorem popUntilKindWithOpener_some_of_hasKind (s : List (Nat × Kind)) (k : Kind)
    (h : stackHasKind s k = true) :
    ∃ r, popUntilKindWithOpener s k = some (r, popUntilKind s k) ∧
      (∀ target, AllRawGt s target → target < r ∧ AllRawGt (popUntilKind s k) target) := by
  induction s with
  | nil => simp [stackHasKind] at h
  | cons hd tl ih =>
    rcases hd with ⟨r0, k0⟩
    simp only [popUntilKindWithOpener, popUntilKind]
    by_cases heq : (k0 == k) = true
    · refine ⟨r0, by simp [heq], ?_⟩
      intro target hall
      simp only [heq, ↓reduceIte]
      refine ⟨hall (r0, k0) (List.Mem.head _), ?_⟩
      intro p hp
      exact hall p (List.Mem.tail _ hp)
    · have hne : (k0 == k) = false := Bool.eq_false_iff.mpr heq
      have htl : stackHasKind tl k = true := by
        simp [stackHasKind, hne] at h
        exact h
      rcases ih htl with ⟨r, hpop, hgt⟩
      refine ⟨r, by simp [hne, hpop], ?_⟩
      intro target hall
      simp only [hne, Bool.false_eq_true, ↓reduceIte]
      have hall_tl : AllRawGt tl target := fun p hp => hall p (List.Mem.tail _ hp)
      exact hgt target hall_tl

theorem popUntilKindWithOpener_append_left (s1 s2 : List (Nat × Kind)) (k : Kind)
    (h : stackHasKind s1 k = true) :
    ∃ r, popUntilKindWithOpener (s1 ++ s2) k = some (r, popUntilKind s1 k ++ s2) ∧
      (∀ target, AllRawGt s1 target → target < r ∧ AllRawGt (popUntilKind s1 k) target) := by
  induction s1 with
  | nil => simp [stackHasKind] at h
  | cons hd tl ih =>
    rcases hd with ⟨r0, k0⟩
    simp only [List.cons_append, popUntilKindWithOpener, popUntilKind]
    by_cases heq : (k0 == k) = true
    · refine ⟨r0, by simp [heq], ?_⟩
      intro target hall
      simp only [heq, ↓reduceIte]
      refine ⟨hall (r0, k0) (List.Mem.head _), ?_⟩
      intro p hp
      exact hall p (List.Mem.tail _ hp)
    · have hne : (k0 == k) = false := Bool.eq_false_iff.mpr heq
      have htl : stackHasKind tl k = true := by
        simp [stackHasKind, hne] at h
        exact h
      rcases ih htl with ⟨r, hpop, hgt⟩
      refine ⟨r, by simp [hne, hpop], ?_⟩
      intro target hall
      simp only [hne, Bool.false_eq_true, ↓reduceIte]
      have hall_tl : AllRawGt tl target := fun p hp => hall p (List.Mem.tail _ hp)
      exact hgt target hall_tl

theorem popUntilKindWithOpener_append_right (s1 : List (Nat × Kind)) (r : Nat) (k : Kind)
    (h : stackHasKind s1 k = false) :
    popUntilKindWithOpener (s1 ++ [(r, k)]) k = some (r, []) := by
  induction s1 with
  | nil => simp [popUntilKindWithOpener]
  | cons hd tl ih =>
    rcases hd with ⟨r', k'⟩
    simp only [List.cons_append, popUntilKindWithOpener]
    simp only [stackHasKind, Bool.or_eq_false_iff] at h
    simp [h.1, ih h.2]

/-- Forward boundary scanner matching `OrdinaryGroups::visit` under an external
ancestor active mask `active`. Stops at the first closer belonging to `active`
or an unmatched right brace (`Kind.brace`), or at EOF (`D.length`). -/
def forwardStop (D : List Delim) (i : Nat) (_hi : i ≤ D.length)
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

/-- Exact forward scan mirroring `OrdinaryGroups::visit` starting from state `s` at `j`,
returning the `raw` of the first closer whose `ordinaryVisitStep` pops `some targetRaw`. -/
def ordinaryVisitFind (D : List Delim) (j : Nat) (_hj : j ≤ D.length)
    (s : List (Nat × Kind)) (targetRaw : Nat) : Option Nat :=
  if hlt : j < D.length then
    let d := D[j]
    let (s', popped) := ordinaryVisitStep s d
    if popped == some targetRaw then
      some d.raw
    else
      ordinaryVisitFind D (j + 1) hlt s' targetRaw
  else
    none
termination_by D.length - j

/-- Exact Spec forward matcher: starts a brand-new empty `OrdinaryGroups` at `i`
and returns the `raw` of the first closer whose `visit` returns `some D[i].raw`. -/
def ordinaryVisitMatch (D : List Delim) (i : Nat) (hi : i < D.length) : Option Nat :=
  if D[i].opening then
    ordinaryVisitFind D i (Nat.le_of_lt hi) [] D[i].raw
  else
    none

/-! ### Backward 8-state DP model (`match_angle_group_delimiters`) -/

/-- P1 (by construction): `dpBoundary` always returns a bounded index `i ≤ b ≤ D.length`
and performs zero out-of-bounds accesses. -/
def dpBoundary (D : List Delim) (i : Nat) (hi : i ≤ D.length) (active : Fin 8) :
    { b : Nat // i ≤ b ∧ b ≤ D.length } :=
  if hlt : i < D.length then
    let d := D[i]
    if d.opening then
      match dpBoundary D (i + 1) hlt (addActive active d.kind) with
      | ⟨endIdx, hge, hle⟩ =>
        if hlt1 : endIdx < D.length then
          if !D[endIdx].opening && (D[endIdx].kind == d.kind) then
            match dpBoundary D (endIdx + 1) hlt1 active with
            | ⟨end2, hge2, hle2⟩ => ⟨end2, by omega, hle2⟩
          else
            ⟨endIdx, by omega, hle⟩
        else
          ⟨endIdx, by omega, hle⟩
    else if hasActive active d.kind || (d.kind == .brace) then
      ⟨i, Nat.le_refl i, hi⟩
    else
      match dpBoundary D (i + 1) hlt active with
      | ⟨endIdx, hge, hle⟩ => ⟨endIdx, by omega, hle⟩
  else
    ⟨D.length, by omega, Nat.le_refl _⟩
termination_by D.length - i

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

/-- Ancestor-mask formulation of the forward matcher (proved equivalent to both
`dpMatch` and `ordinaryVisitMatch` below). -/
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
      have hge1 := (dpBoundary D (i + 1) hlt (addActive active D[i].kind)).property.1
      conv => lhs; unfold dpBoundary
      simp only [hlt, ↓reduceDIte, hop, ↓reduceIte, ← ih1]
      by_cases hlt1 : (dpBoundary D (i + 1) hlt (addActive active D[i].kind)).val < D.length
      · simp only [hlt1, ↓reduceDIte]
        by_cases hcl : (!D[(dpBoundary D (i + 1) hlt (addActive active D[i].kind)).val].opening &&
            (D[(dpBoundary D (i + 1) hlt (addActive active D[i].kind)).val].kind == D[i].kind)) = true
        · simp only [hcl, ↓reduceIte]
          exact dpBoundary_eq_forwardStop D
            ((dpBoundary D (i + 1) hlt (addActive active D[i].kind)).val + 1)
            hlt1 active
        · simp only [hcl, ↓reduceIte]
      · simp only [hlt1, ↓reduceDIte]
  · unfold dpBoundary forwardStop
    simp [hlt]
termination_by D.length - i

/-- P3a: `dpMatch` equals the ancestor-mask forward matcher `forwardMatch`. -/
theorem dpMatch_eq_forwardMatch (D : List Delim) (i : Nat) (hi : i < D.length) :
    dpMatch D i hi = forwardMatch D i hi := by
  unfold dpMatch forwardMatch
  dsimp only
  rw [dpBoundary_eq_forwardStop]

def StrictlyIncreasingRaw (D : List Delim) : Prop :=
  ∀ a b, a < b → (ha : a < D.length) → (hb : b < D.length) → D[a].raw < D[b].raw

def AllFutureRawGt (D : List Delim) (j target : Nat) : Prop :=
  ∀ m, j ≤ m → (hm : m < D.length) → target < D[m].raw

theorem ordinaryVisitFind_none_of_all_gt (D : List Delim) (j : Nat) (hj : j ≤ D.length)
    (s : List (Nat × Kind)) (target : Nat)
    (hs : AllRawGt s target) (hf : AllFutureRawGt D j target) :
    ordinaryVisitFind D j hj s target = none := by
  by_cases hlt : j < D.length
  · have hfut_next : AllFutureRawGt D (j + 1) target :=
      fun m hm hml => hf m (by omega) hml
    cases hop : D[j].opening
    · by_cases hbr : (D[j].kind == .brace && !stackHasKind s .brace) = true
      · conv => lhs; unfold ordinaryVisitFind
        simp only [hlt, ↓reduceDIte, ordinaryVisitStep, hop, Bool.false_eq_true, hbr, ↓reduceIte]
        exact ordinaryVisitFind_none_of_all_gt D (j + 1) hlt [] target (fun _ hp => nomatch hp) hfut_next
      · have hbr_f : (D[j].kind == .brace && !stackHasKind s .brace) = false :=
          Bool.eq_false_iff.mpr hbr
        by_cases hhas : stackHasKind s D[j].kind = true
        · rcases popUntilKindWithOpener_some_of_hasKind s D[j].kind hhas with ⟨r', hpop, hgt⟩
          rcases hgt target hs with ⟨hr_gt, hrest_gt⟩
          have hne : (some r' == some target) = false := by
            cases h : (some r' == some target)
            · rfl
            · have heq := LawfulBEq.eq_of_beq h
              cases heq
              omega
          conv => lhs; unfold ordinaryVisitFind
          simp only [hlt, ↓reduceDIte, ordinaryVisitStep, hop, Bool.false_eq_true, hbr_f, hhas,
            hpop, hne, ↓reduceIte]
          exact ordinaryVisitFind_none_of_all_gt D (j + 1) hlt (popUntilKind s D[j].kind) target hrest_gt hfut_next
        · have hhas_f : stackHasKind s D[j].kind = false := Bool.eq_false_iff.mpr hhas
          conv => lhs; unfold ordinaryVisitFind
          simp only [hlt, ↓reduceDIte, ordinaryVisitStep, hop, Bool.false_eq_true, hbr_f, hhas_f, ↓reduceIte]
          exact ordinaryVisitFind_none_of_all_gt D (j + 1) hlt s target hs hfut_next
    · have hs' : AllRawGt ((D[j].raw, D[j].kind) :: s) target := by
        intro p hp
        cases hp with
        | head => exact hf j (Nat.le_refl j) hlt
        | tail _ hp' => exact hs p hp'
      conv => lhs; unfold ordinaryVisitFind
      simp only [hlt, ↓reduceDIte, ordinaryVisitStep, hop, ↓reduceIte]
      exact ordinaryVisitFind_none_of_all_gt D (j + 1) hlt ((D[j].raw, D[j].kind) :: s) target hs' hfut_next
  · unfold ordinaryVisitFind; simp [hlt]
termination_by D.length - j

theorem ordinaryVisitFind_append_singleton (D : List Delim) (j : Nat) (hj : j ≤ D.length)
    (s : List (Nat × Kind)) (r : Nat) (k : Kind)
    (hs : AllRawGt s r) (hf : AllFutureRawGt D j r) :
    ordinaryVisitFind D j hj (s ++ [(r, k)]) r =
      let endIdx := forwardStop D j hj s (addActive emptyActive k)
      if hlt : endIdx < D.length then
        if !D[endIdx].opening && (D[endIdx].kind == k) then
          some D[endIdx].raw
        else
          none
      else
        none := by
  by_cases hlt : j < D.length
  · have hfut_next : AllFutureRawGt D (j + 1) r :=
      fun m hm hml => hf m (by omega) hml
    cases hop : D[j].opening
    · by_cases hhas : stackHasKind s D[j].kind = true
      · have hhas_app : stackHasKind (s ++ [(r, k)]) D[j].kind = true := by
          rw [stackHasKind_append, hhas]; rfl
        have hbr_app : (D[j].kind == .brace && !stackHasKind (s ++ [(r, k)]) .brace) = false := by
          cases hkj : (D[j].kind == .brace)
          · rfl
          · have hk_eq : D[j].kind = .brace := LawfulBEq.eq_of_beq hkj
            simp [hk_eq ▸ hhas_app]
        rcases popUntilKindWithOpener_append_left s [(r, k)] D[j].kind hhas with ⟨r', hpop, hgt⟩
        rcases hgt r hs with ⟨hr_gt, hrest_gt⟩
        have hne : (some r' == some r) = false := by
          cases h : (some r' == some r)
          · rfl
          · have heq := LawfulBEq.eq_of_beq h
            cases heq
            omega
        have hstepL : ordinaryVisitFind D j hj (s ++ [(r, k)]) r =
            ordinaryVisitFind D (j + 1) hlt (popUntilKind s D[j].kind ++ [(r, k)]) r := by
          conv => lhs; unfold ordinaryVisitFind
          simp [hlt, ordinaryVisitStep, hop, hbr_app, hhas_app, hpop, hne]
        have hstepR : forwardStop D j hj s (addActive emptyActive k) =
            forwardStop D (j + 1) hlt (popUntilKind s D[j].kind) (addActive emptyActive k) := by
          conv => lhs; unfold forwardStop
          simp [hlt, hop, hhas]
        rw [hstepL]
        simp only [hstepR]
        exact ordinaryVisitFind_append_singleton D (j + 1) hlt (popUntilKind s D[j].kind) r k hrest_gt hfut_next
      · have hhas_f : stackHasKind s D[j].kind = false := Bool.eq_false_iff.mpr hhas
        by_cases hkk : D[j].kind = k
        · have hhas_app : stackHasKind (s ++ [(r, k)]) D[j].kind = true := by
            rw [stackHasKind_append, hkk]; simp [stackHasKind]
          have hbr_app : (D[j].kind == .brace && !stackHasKind (s ++ [(r, k)]) .brace) = false := by
            cases hkj : (D[j].kind == .brace)
            · rfl
            · have hk_eq : D[j].kind = .brace := LawfulBEq.eq_of_beq hkj
              simp [hk_eq ▸ hhas_app]
          have hpop : popUntilKindWithOpener (s ++ [(r, k)]) D[j].kind = some (r, []) := by
            rw [hkk]; exact popUntilKindWithOpener_append_right s r k (hkk ▸ hhas_f)
          have hact : hasActive (addActive emptyActive k) D[j].kind = true := by
            rw [hasActive_addActive, hkk]; simp
          have hstepL : ordinaryVisitFind D j hj (s ++ [(r, k)]) r = some D[j].raw := by
            conv => lhs; unfold ordinaryVisitFind
            simp [hlt, ordinaryVisitStep, hop, hbr_app, hhas_app, hpop]
          have hstepR : forwardStop D j hj s (addActive emptyActive k) = j := by
            conv => lhs; unfold forwardStop
            simp [hlt, hop, hhas_f, hact]
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
          have hhas_app : stackHasKind (s ++ [(r, k)]) D[j].kind = false := by
            rw [stackHasKind_append, hhas_f]; simp [stackHasKind, hbeq]
          have hact : hasActive (addActive emptyActive k) D[j].kind = false := by
            rw [hasActive_addActive, hasActive_empty, hbeq]; rfl
          by_cases hbrace : D[j].kind = .brace
          · have hhas_br : stackHasKind s .brace = false := hbrace ▸ hhas_f
            have hbr_app : (D[j].kind == .brace && !stackHasKind (s ++ [(r, k)]) .brace) = true := by
              rw [hbrace]; simp [hbrace ▸ hhas_app]
            have hstepL : ordinaryVisitFind D j hj (s ++ [(r, k)]) r =
                ordinaryVisitFind D (j + 1) hlt [] r := by
              conv => lhs; unfold ordinaryVisitFind
              simp [hlt, ordinaryVisitStep, hop, hbr_app]
            have hstepR : forwardStop D j hj s (addActive emptyActive k) = j := by
              conv => lhs; unfold forwardStop
              simp [hlt, hop, hbrace, hhas_br]
            rw [hstepL, ordinaryVisitFind_none_of_all_gt D (j + 1) hlt [] r (fun _ hp => nomatch hp) hfut_next]
            simp [hstepR, hlt, hop, hbeq']
          · have hbr_eq : (D[j].kind == .brace) = false := by
              cases h : (D[j].kind == .brace)
              · rfl
              · exact absurd (LawfulBEq.eq_of_beq h) hbrace
            have hstepL : ordinaryVisitFind D j hj (s ++ [(r, k)]) r =
                ordinaryVisitFind D (j + 1) hlt (s ++ [(r, k)]) r := by
              conv => lhs; unfold ordinaryVisitFind
              simp [hlt, ordinaryVisitStep, hop, hbr_eq, hhas_app]
            have hstepR : forwardStop D j hj s (addActive emptyActive k) =
                forwardStop D (j + 1) hlt s (addActive emptyActive k) := by
              conv => lhs; unfold forwardStop
              simp [hlt, hop, hhas_f, hact, hbr_eq]
            rw [hstepL]
            simp only [hstepR]
            exact ordinaryVisitFind_append_singleton D (j + 1) hlt s r k hs hfut_next
    · have hs' : AllRawGt ((D[j].raw, D[j].kind) :: s) r := by
        intro p hp
        cases hp with
        | head => exact hf j (Nat.le_refl j) hlt
        | tail _ hp' => exact hs p hp'
      have hstepL : ordinaryVisitFind D j hj (s ++ [(r, k)]) r =
          ordinaryVisitFind D (j + 1) hlt (((D[j].raw, D[j].kind) :: s) ++ [(r, k)]) r := by
        conv => lhs; unfold ordinaryVisitFind
        simp [hlt, ordinaryVisitStep, hop]
      have hstepR : forwardStop D j hj s (addActive emptyActive k) =
          forwardStop D (j + 1) hlt ((D[j].raw, D[j].kind) :: s) (addActive emptyActive k) := by
        conv => lhs; unfold forwardStop
        simp [hlt, hop]
      rw [hstepL]
      simp only [hstepR]
      exact ordinaryVisitFind_append_singleton D (j + 1) hlt ((D[j].raw, D[j].kind) :: s) r k hs' hfut_next
  · have heqL : ordinaryVisitFind D j hj (s ++ [(r, k)]) r = none := by
      unfold ordinaryVisitFind; simp [hlt]
    have heqR : forwardStop D j hj s (addActive emptyActive k) = D.length := by
      unfold forwardStop; simp [hlt]
    rw [heqL]
    simp [heqR]
termination_by D.length - j

/-- Equivalence between the explicit opener-returning `OrdinaryGroups::visit` model
and `forwardMatch` under strictly increasing `raw` indices. -/
theorem ordinaryVisitMatch_eq_forwardMatch (D : List Delim)
    (hinc : StrictlyIncreasingRaw D) (i : Nat) (hi : i < D.length) :
    ordinaryVisitMatch D i hi = forwardMatch D i hi := by
  unfold ordinaryVisitMatch forwardMatch
  dsimp only
  cases hop : D[i].opening
  · simp
  · simp only [↓reduceIte]
    have hfirst : ordinaryVisitFind D i (Nat.le_of_lt hi) [] D[i].raw =
        ordinaryVisitFind D (i + 1) hi ([] ++ [(D[i].raw, D[i].kind)]) D[i].raw := by
      conv => lhs; unfold ordinaryVisitFind
      simp [hi, ordinaryVisitStep, hop]
    rw [hfirst]
    have hfut : AllFutureRawGt D (i + 1) D[i].raw :=
      fun m hm hml => hinc i m (by omega) hi hml
    exact ordinaryVisitFind_append_singleton D (i + 1) hi [] D[i].raw D[i].kind
      (fun _ hp => nomatch hp) hfut

/-- P3 (Full Spec equivalence theorem, unbounded `∀ n ≥ 0`): under strictly increasing
`raw` indices, the backward 8-state DP match equals starting a fresh empty
`OrdinaryGroups` at opener `i` and taking the first closer whose `visit` returns
`some D[i].raw`. -/
theorem dpMatch_eq_ordinaryVisitMatch (D : List Delim)
    (hinc : StrictlyIncreasingRaw D) (i : Nat) (hi : i < D.length) :
    dpMatch D i hi = ordinaryVisitMatch D i hi := by
  rw [dpMatch_eq_forwardMatch, ← ordinaryVisitMatch_eq_forwardMatch D hinc i hi]

/-- P4 (`NonZeroUsize` non-lossiness): under strictly increasing `raw` indices,
every matched closer has `raw ≥ 1`, so `NonZeroUsize::new(closer_raw)` never loses
a valid match. -/
theorem dpMatch_some_pos (D : List Delim) (hinc : StrictlyIncreasingRaw D)
    (i : Nat) (hi : i < D.length) (c : Nat) (hm : dpMatch D i hi = some c) :
    1 ≤ c := by
  unfold dpMatch at hm
  dsimp only at hm
  cases hop : D[i].opening
  · simp [hop] at hm
  · simp only [hop, ↓reduceIte] at hm
    have hge := (dpBoundary D (i + 1) hi (addActive emptyActive D[i].kind)).property.1
    by_cases hlt1 : (dpBoundary D (i + 1) hi (addActive emptyActive D[i].kind)).val < D.length
    · simp only [hlt1, ↓reduceDIte] at hm
      by_cases hcl : (!D[(dpBoundary D (i + 1) hi (addActive emptyActive D[i].kind)).val].opening &&
          (D[(dpBoundary D (i + 1) hi (addActive emptyActive D[i].kind)).val].kind == D[i].kind)) = true
      · simp only [hcl, ↓reduceIte] at hm
        cases hm
        have hlt_raw : D[i].raw < D[(dpBoundary D (i + 1) hi (addActive emptyActive D[i].kind)).val].raw :=
          hinc i _ (by omega) hi hlt1
        omega
      · simp only [hcl, ↓reduceIte] at hm
        cases hm
    · simp only [hlt1, ↓reduceDIte] at hm
      cases hm

end F6AngleGroups
