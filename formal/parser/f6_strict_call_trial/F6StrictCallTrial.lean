/-
  SPEC-0507 (F6 tenth slice): Machine-checked unbounded model proofs for
  `StrictCallTrialIndex` pure indexing, reverse-DP `path_ends`, `CallTrial`
  classification, and recursion-budget enforcement
  (`crates/lang-frontend/src/parser/trial/kernel.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `trial/kernel_kani.rs`.
-/

namespace F6StrictCallTrial

/-! ### Part 1: Trivia-skipping `raw_to_significant` & `significant_to_raw` -/

/-- Exact functional model of `fill_next_significant` starting from index `r`
    with `sigCount` non-trivia elements strictly before `r`. -/
def buildRawToSigAux (isTrivia : List Bool) (r sigCount : Nat) : List (Option Nat) :=
  match isTrivia with
  | [] => []
  | true :: rest =>
    let tail := buildRawToSigAux rest (r + 1) sigCount
    let next := tail.head?.join
    next :: tail
  | false :: rest =>
    some sigCount :: buildRawToSigAux rest (r + 1) (sigCount + 1)

def buildRawToSig (isTrivia : List Bool) : List (Option Nat) :=
  buildRawToSigAux isTrivia 0 0

def buildSigToRawAux (isTrivia : List Bool) (r : Nat) : List Nat :=
  match isTrivia with
  | [] => []
  | true :: rest => buildSigToRawAux rest (r + 1)
  | false :: rest => r :: buildSigToRawAux rest (r + 1)

def buildSigToRaw (isTrivia : List Bool) : List Nat :=
  buildSigToRawAux isTrivia 0

private theorem buildRawToSigAux_length (xs : List Bool) (r sig : Nat) :
    (buildRawToSigAux xs r sig).length = xs.length := by
  induction xs generalizing r sig with
  | nil => rfl
  | cons b rest ih =>
    cases b <;> simp [buildRawToSigAux, ih]

private theorem buildRawToSigAux_lower_bound (xs : List Bool) (r sig : Nat) :
    ∀ x ∈ buildRawToSigAux xs r sig, ∀ s, x = some s → sig ≤ s := by
  induction xs generalizing r sig with
  | nil => simp [buildRawToSigAux]
  | cons b rest ih =>
    cases b with
    | false =>
      intro x hx s hxs
      simp only [buildRawToSigAux, List.mem_cons] at hx
      rcases hx with rfl | hmem
      · injection hxs with heq; omega
      · have := ih (r + 1) (sig + 1) x hmem s hxs
        omega
    | true =>
      intro x hx s hxs
      simp only [buildRawToSigAux, List.mem_cons] at hx
      rcases hx with rfl | hmem
      · cases htail : buildRawToSigAux rest (r + 1) sig with
        | nil => simp [htail] at hxs
        | cons y ys =>
          simp only [htail, List.head?_cons, Option.join] at hxs
          have hy : y ∈ buildRawToSigAux rest (r + 1) sig := by rw [htail]; simp
          exact ih (r + 1) sig y hy s hxs
      · exact ih (r + 1) sig x hmem s hxs

private theorem buildRawToSigAux_monotone (xs : List Bool) (r sig : Nat) :
    ∀ (i j : Nat) (hi : i < xs.length) (hj : j < xs.length),
      i ≤ j →
      ∀ s2, (buildRawToSigAux xs r sig)[j]'(by rw [buildRawToSigAux_length]; exact hj) = some s2 →
      ∃ s1, (buildRawToSigAux xs r sig)[i]'(by rw [buildRawToSigAux_length]; exact hi) = some s1 ∧ s1 ≤ s2 := by
  induction xs generalizing r sig with
  | nil => intro _ _ hi; simp at hi
  | cons b rest ih =>
    intro i j hi hj hij s2 hs2
    cases b with
    | false =>
      cases i with
      | zero =>
        cases j with
        | zero =>
          simp [buildRawToSigAux] at hs2
          exact ⟨sig, by simp [buildRawToSigAux], by omega⟩
        | succ j' =>
          simp only [buildRawToSigAux, List.getElem_cons_succ] at hs2
          have hmem : (buildRawToSigAux rest (r + 1) (sig + 1))[j']'(by
              rw [buildRawToSigAux_length]; simp only [List.length_cons] at hj; omega) ∈
              buildRawToSigAux rest (r + 1) (sig + 1) := List.getElem_mem _
          have hlb := buildRawToSigAux_lower_bound rest (r + 1) (sig + 1) _ hmem s2 hs2
          exact ⟨sig, by simp [buildRawToSigAux], by omega⟩
      | succ i' =>
        cases j with
        | zero => omega
        | succ j' =>
          simp only [buildRawToSigAux, List.getElem_cons_succ] at hs2 ⊢
          exact ih (r + 1) (sig + 1) i' j' (by simp at hi; omega) (by simp at hj; omega) (by omega) s2 hs2
    | true =>
      cases i with
      | zero =>
        cases j with
        | zero => exact ⟨s2, hs2, Nat.le_refl _⟩
        | succ j' =>
          simp only [buildRawToSigAux, List.getElem_cons_succ] at hs2
          have hj' : j' < rest.length := by simp only [List.length_cons] at hj; omega
          have hres := ih (r + 1) sig 0 j' (by omega) hj' (Nat.zero_le _) s2 hs2
          rcases hres with ⟨s1, hs1, hle⟩
          refine ⟨s1, ?_, hle⟩
          simp only [buildRawToSigAux, List.getElem_cons_zero]
          cases htail : buildRawToSigAux rest (r + 1) sig with
          | nil =>
            have hlen := buildRawToSigAux_length rest (r + 1) sig
            rw [htail] at hlen
            simp only [List.length_nil] at hlen
            omega
          | cons y ys =>
            simp only [htail, List.getElem_cons_zero] at hs1
            simp [hs1]
      | succ i' =>
        cases j with
        | zero => omega
        | succ j' =>
          simp only [buildRawToSigAux, List.getElem_cons_succ] at hs2 ⊢
          exact ih (r + 1) sig i' j' (by simp at hi; omega) (by simp at hj; omega) (by omega) s2 hs2

/-! ### T1: Non-trivia positions map immediately to their own significant index -/

private theorem buildRawToSigAux_non_trivia_self (xs : List Bool) (r sig : Nat) :
    ∀ (k : Nat) (hk : k < xs.length),
      xs[k] = false →
      ∃ s, (buildRawToSigAux xs r sig)[k]'(by rw [buildRawToSigAux_length]; exact hk) = some s := by
  induction xs generalizing r sig with
  | nil => intro _ hk; simp at hk
  | cons b rest ih =>
    intro k hk hfalse
    cases b with
    | false =>
      cases k with
      | zero => exact ⟨sig, by simp [buildRawToSigAux]⟩
      | succ k' =>
        simp only [buildRawToSigAux, List.getElem_cons_succ]
        exact ih (r + 1) (sig + 1) k' (by simp at hk; omega) hfalse
    | true =>
      cases k with
      | zero => simp at hfalse
      | succ k' =>
        simp only [buildRawToSigAux, List.getElem_cons_succ]
        exact ih (r + 1) sig k' (by simp at hk; omega) hfalse

theorem rawToSig_non_trivia_always_some (isTrivia : List Bool)
    (k : Nat) (hk : k < isTrivia.length) (h : isTrivia[k] = false) :
    ∃ s, (buildRawToSig isTrivia)[k]'(by unfold buildRawToSig; rw [buildRawToSigAux_length]; exact hk) = some s :=
  buildRawToSigAux_non_trivia_self isTrivia 0 0 k hk h

/-! ### T2: `raw_to_significant` is monotone across raw checkpoints -/

theorem rawToSig_monotone (isTrivia : List Bool)
    (r1 r2 : Nat) (hr1 : r1 < isTrivia.length) (hr2 : r2 < isTrivia.length)
    (hle : r1 ≤ r2) (s2 : Nat)
    (hs2 : (buildRawToSig isTrivia)[r2]'(by unfold buildRawToSig; rw [buildRawToSigAux_length]; exact hr2) = some s2) :
    ∃ s1, (buildRawToSig isTrivia)[r1]'(by unfold buildRawToSig; rw [buildRawToSigAux_length]; exact hr1) = some s1 ∧ s1 ≤ s2 :=
  buildRawToSigAux_monotone isTrivia 0 0 r1 r2 hr1 hr2 hle s2 hs2

/-! ### T3: `significant_to_raw` is strictly increasing -/

private theorem buildSigToRawAux_lower_bound (xs : List Bool) (r : Nat) :
    ∀ x ∈ buildSigToRawAux xs r, r ≤ x := by
  induction xs generalizing r with
  | nil => simp [buildSigToRawAux]
  | cons b rest ih =>
    cases b with
    | true =>
      intro x hx
      have := ih (r + 1) x hx
      omega
    | false =>
      intro x hx
      simp only [buildSigToRawAux, List.mem_cons] at hx
      rcases hx with rfl | hmem
      · exact Nat.le_refl _
      · have := ih (r + 1) x hmem
        omega

private theorem buildSigToRawAux_strict_mono (xs : List Bool) (r : Nat) :
    ∀ (s1 s2 : Nat) (hs1 : s1 < (buildSigToRawAux xs r).length)
      (hs2 : s2 < (buildSigToRawAux xs r).length),
      s1 < s2 →
      (buildSigToRawAux xs r)[s1] < (buildSigToRawAux xs r)[s2] := by
  induction xs generalizing r with
  | nil => intro _ _ hs1; simp [buildSigToRawAux] at hs1
  | cons b rest ih =>
    cases b with
    | true => exact ih (r + 1)
    | false =>
      intro s1 s2 hs1 hs2 hlt
      have hs1_len : s1 < (buildSigToRawAux rest (r + 1)).length + 1 := by
        simpa [buildSigToRawAux] using hs1
      have hs2_len : s2 < (buildSigToRawAux rest (r + 1)).length + 1 := by
        simpa [buildSigToRawAux] using hs2
      cases s1 with
      | zero =>
        cases s2 with
        | zero => omega
        | succ s2' =>
          simp only [buildSigToRawAux, List.getElem_cons_zero, List.getElem_cons_succ]
          have hmem : (buildSigToRawAux rest (r + 1))[s2']'(by omega) ∈
              buildSigToRawAux rest (r + 1) := List.getElem_mem _
          have hlb := buildSigToRawAux_lower_bound rest (r + 1) _ hmem
          omega
      | succ s1' =>
        cases s2 with
        | zero => omega
        | succ s2' =>
          simp only [buildSigToRawAux, List.getElem_cons_succ]
          exact ih (r + 1) s1' s2' (by omega) (by omega) (by omega)

theorem sigToRaw_strict_mono (isTrivia : List Bool)
    (s1 s2 : Nat) (hs1 : s1 < (buildSigToRaw isTrivia).length)
    (hs2 : s2 < (buildSigToRaw isTrivia).length) (hlt : s1 < s2) :
    (buildSigToRaw isTrivia)[s1] < (buildSigToRaw isTrivia)[s2] :=
  buildSigToRawAux_strict_mono isTrivia 0 s1 s2 hs1 hs2 hlt

/-! ### Part 2: Reverse-DP `path_ends` strict advance & maximality -/

inductive PathTok where
  | ident
  | dot
  | other
  deriving DecidableEq, Repr

/-- Exact functional model of `resolve_path_end` reverse DP at index `i`. -/
def pathEndAt (toks : List PathTok) (i : Nat) : Option Nat :=
  if h : i < toks.length then
    if toks[i] == PathTok.ident then
      if h2 : i + 2 < toks.length then
        if toks[i + 1] == PathTok.dot && toks[i + 2] == PathTok.ident then
          pathEndAt toks (i + 2)
        else
          some (i + 1)
      else
        some (i + 1)
    else
      none
  else
    none
termination_by toks.length - i

/-! ### T4: Every `pathEndAt toks i = some e` strictly advances, stays in bounds, and is maximal -/

theorem pathEndAt_strict_advance_bounds_and_maximal (toks : List PathTok) :
    ∀ (i e : Nat),
      pathEndAt toks i = some e →
      i < e ∧ e ≤ toks.length ∧
      (∀ (he1 : e + 1 < toks.length), ¬(toks[e] = PathTok.dot ∧ toks[e + 1] = PathTok.ident)) := by
  intro i e h
  unfold pathEndAt at h
  split at h
  · rename_i hi
    split at h
    · rename_i hident
      split at h
      · rename_i hi2
        split at h
        · rename_i hmem
          have ih := pathEndAt_strict_advance_bounds_and_maximal toks (i + 2) e h
          exact ⟨by omega, ih.2.1, ih.2.2⟩
        · rename_i hnomem
          injection h with he
          subst he
          refine ⟨by omega, by omega, ?_⟩
          intro he1 ⟨hdot, hid⟩
          apply hnomem
          simp [hdot, hid]
      · rename_i hnoi2
        injection h with he
        subst he
        refine ⟨by omega, by omega, ?_⟩
        intro he1 _
        omega
    · simp at h
  · simp at h
termination_by i _ _ => toks.length - i

/-! ### Part 3: `CallTrial` classification and recursion-budget enforcement -/

structure ListSuccess where
  stop : Nat
  closing : Nat
  deriving DecidableEq, Repr

structure ListTrial where
  success : Option ListSuccess
  exploredDepth : Nat
  deriving DecidableEq, Repr

inductive CallTrialModel where
  | match_ (closing suffix additionalDepth : Nat)
  | noMatch (additionalDepth : Nat) (closedList : Option Nat)
  deriving DecidableEq, Repr

def classifyCallTrial (list : ListTrial) (isCallSuffix : Bool) : CallTrialModel :=
  match list.success with
  | some s =>
    if isCallSuffix then
      CallTrialModel.match_ s.closing s.stop list.exploredDepth
    else
      CallTrialModel.noMatch list.exploredDepth (some s.closing)
  | none =>
    CallTrialModel.noMatch list.exploredDepth none

/-! ### T5: `classifyCallTrial` preserves strict `opener < closing < suffix` ordering and closed-list evidence -/

theorem classifyCallTrial_match_ordering (opener closing depth : Nat)
    (hopen : opener < closing) (isCallSuffix : Bool) :
    let list : ListTrial := { success := some { stop := closing + 1, closing := closing }, exploredDepth := depth }
    (isCallSuffix = true →
      classifyCallTrial list isCallSuffix = CallTrialModel.match_ closing (closing + 1) depth ∧
      opener < closing ∧ closing < closing + 1) ∧
    (isCallSuffix = false →
      classifyCallTrial list isCallSuffix = CallTrialModel.noMatch depth (some closing)) := by
  constructor
  · intro ht
    simp [classifyCallTrial, ht]
    omega
  · intro hf
    simp [classifyCallTrial, hf]

/-- Exact functional model of `check_recursion_budget`. -/
def checkBudget (baseline additional maxDepth : Nat) : Bool :=
  decide (baseline + additional ≤ maxDepth)

/-! ### T6: Recursion budget exactness and anti-monotonicity in `baseline` -/

theorem checkBudget_iff_and_antitone (baseline additional maxDepth : Nat) :
    (checkBudget baseline additional maxDepth = true ↔ baseline + additional ≤ maxDepth) ∧
    (∀ b', baseline ≤ b' → checkBudget baseline additional maxDepth = false →
      checkBudget b' additional maxDepth = false) := by
  unfold checkBudget
  constructor
  · exact decide_eq_true_iff
  · intro b' hle hf
    simp only [decide_eq_false_iff_not] at hf ⊢
    omega

end F6StrictCallTrial
