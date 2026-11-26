/-
  SPEC-0506 (F6 ninth slice): Machine-checked unbounded model proofs for
  `diagnostic_anchor_prefix` prefix-sum monotonicity and exact interval
  anchor query soundness (`crates/lang-frontend/src/parser/engine/recovery/diagnostic_anchor.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `diagnostic_anchor_kani.rs`.
-/

namespace F6DiagnosticAnchor

/-- Count of `true` marks in the half-open prefix `[0, k)`. -/
def countPrefix (marks : List Bool) (k : Nat) : Nat :=
  (marks.take k).foldl (fun acc b => acc + if b then 1 else 0) 0

/-- Exact functional model of `finalize_anchor_prefix` starting from `acc` over a suffix. -/
def scanPrefix : Nat → List Bool → List Nat
  | acc, [] => [acc]
  | acc, b :: rest =>
    let next := acc + if b then 1 else 0
    acc :: scanPrefix next rest

/-- Full `diagnostic_anchor_prefix` table built from boolean anchor marks. -/
def buildPrefix (marks : List Bool) : List Nat :=
  scanPrefix 0 marks

/-- Exact functional model of `prefix_has_anchor(prefix, start, end)`. -/
def prefixHasAnchor (pref : List Nat) (start stop : Nat) : Bool :=
  match pref[start]?, pref[stop]? with
  | some before, some after => decide (before < after)
  | _, _ => false

/-! ### Helper lemmas for `scanPrefix` -/

private theorem scanPrefix_length (acc : Nat) (marks : List Bool) :
    (scanPrefix acc marks).length = marks.length + 1 := by
  induction marks generalizing acc with
  | nil => rfl
  | cons b rest ih =>
    simp [scanPrefix, ih]

private theorem scanPrefix_zero (acc : Nat) (marks : List Bool) :
    (scanPrefix acc marks)[0]'(by rw [scanPrefix_length]; omega) = acc := by
  cases marks <;> rfl

private theorem scanPrefix_step (acc : Nat) (marks : List Bool)
    (k : Nat) (hk : k < marks.length) :
    (scanPrefix acc marks)[k + 1]'(by rw [scanPrefix_length]; omega) =
    (scanPrefix acc marks)[k]'(by rw [scanPrefix_length]; omega) +
      if marks[k] then 1 else 0 := by
  induction marks generalizing acc k with
  | nil => simp at hk
  | cons b rest ih =>
    cases k with
    | zero =>
      simp [scanPrefix, scanPrefix_zero]
    | succ k' =>
      simp only [scanPrefix, List.getElem_cons_succ]
      exact ih (acc + if b then 1 else 0) k' (by simp at hk; omega)

/-! ### A1: Length and initial zero of `buildPrefix` -/

theorem prefix_length_and_zero (marks : List Bool) :
    (buildPrefix marks).length = marks.length + 1 ∧
    (buildPrefix marks)[0]'(by unfold buildPrefix; rw [scanPrefix_length]; omega) = 0 := by
  unfold buildPrefix
  exact ⟨scanPrefix_length 0 marks, scanPrefix_zero 0 marks⟩

/-! ### A2: Monotonicity of `buildPrefix` (`∀ i ≤ j ≤ marks.length, pref[i] ≤ pref[j]`) -/

theorem prefix_monotone (marks : List Bool)
    (i j : Nat) (hij : i ≤ j) (hj : j ≤ marks.length) :
    (buildPrefix marks)[i]'(by unfold buildPrefix; rw [scanPrefix_length]; omega) ≤
    (buildPrefix marks)[j]'(by unfold buildPrefix; rw [scanPrefix_length]; omega) := by
  induction j with
  | zero =>
    have hi : i = 0 := by omega
    subst hi
    exact Nat.le_refl _
  | succ j' ih =>
    by_cases hEq : i = j' + 1
    · subst hEq
      exact Nat.le_refl _
    · have hij' : i ≤ j' := by omega
      have h1 := ih hij' (by omega)
      have hstep := scanPrefix_step 0 marks j' (by omega)
      unfold buildPrefix at h1 ⊢
      omega

/-! ### A3: Exact count difference on `[start, stop)` -/

def rangeCount (marks : List Bool) (start stop : Nat) : Nat :=
  ((marks.drop start).take (stop - start)).foldl (fun acc b => acc + if b then 1 else 0) 0

private theorem foldl_add_shift (init : Nat) (xs : List Bool) :
    xs.foldl (fun acc b => acc + if b then 1 else 0) init =
    init + xs.foldl (fun acc b => acc + if b then 1 else 0) 0 := by
  induction xs generalizing init with
  | nil => simp
  | cons b rest ih =>
    simp only [List.foldl_cons, Nat.zero_add]
    rw [ih (init + if b then 1 else 0), ih (if b then 1 else 0)]
    omega

private theorem scanPrefix_get_eq_countPrefix (acc : Nat) (marks : List Bool)
    (k : Nat) (hk : k ≤ marks.length) :
    (scanPrefix acc marks)[k]'(by rw [scanPrefix_length]; omega) =
    acc + countPrefix marks k := by
  induction marks generalizing acc k with
  | nil =>
    have hk0 : k = 0 := by simp at hk; omega
    subst hk0
    simp [scanPrefix, countPrefix]
  | cons b rest ih =>
    cases k with
    | zero =>
      simp [scanPrefix, countPrefix]
    | succ k' =>
      simp only [scanPrefix, List.getElem_cons_succ, countPrefix, List.take_succ_cons,
        List.foldl_cons, Nat.zero_add]
      rw [ih (acc + if b then 1 else 0) k' (by simp at hk; omega)]
      rw [foldl_add_shift (if b then 1 else 0) (rest.take k')]
      unfold countPrefix
      omega

theorem prefix_get_eq_countPrefix (marks : List Bool)
    (k : Nat) (hk : k ≤ marks.length) :
    (buildPrefix marks)[k]'(by unfold buildPrefix; rw [scanPrefix_length]; omega) =
    countPrefix marks k := by
  unfold buildPrefix
  rw [scanPrefix_get_eq_countPrefix 0 marks k hk]
  omega

/-! ### A4: `prefixHasAnchor` iff an anchor exists in `[start, stop)` -/

private theorem prefix_strict_lt_iff_exists (marks : List Bool) :
    ∀ (stop : Nat) (hstop : stop ≤ marks.length) (start : Nat) (hstart : start ≤ stop),
      ((buildPrefix marks)[start]'(by unfold buildPrefix; rw [scanPrefix_length]; omega) <
       (buildPrefix marks)[stop]'(by unfold buildPrefix; rw [scanPrefix_length]; omega)) ↔
      (∃ (k : Nat) (hk : k < marks.length), start ≤ k ∧ k < stop ∧ marks[k] = true) := by
  intro stop
  induction stop with
  | zero =>
    intro hstop start hstart
    have hs : start = 0 := by omega
    subst hs
    constructor
    · intro hlt; omega
    · rintro ⟨k, _, _, hk, _⟩; omega
  | succ s ih =>
    intro hs start hstart
    by_cases heq : start = s + 1
    · subst heq
      constructor
      · intro hlt; omega
      · rintro ⟨k, _, h1, h2, _⟩; omega
    · have hle : start ≤ s := by omega
      have hstep := scanPrefix_step 0 marks s (by omega)
      have hmono := prefix_monotone marks start s hle (by omega)
      have hih := ih (by omega) start hle
      unfold buildPrefix at hmono hih ⊢
      constructor
      · intro hlt
        cases hms : marks[s]'(by omega) with
        | true =>
          exact ⟨s, by omega, hle, by omega, hms⟩
        | false =>
          rw [hms] at hstep
          simp only [Bool.false_eq_true, ↓reduceIte, Nat.add_zero] at hstep
          have hlt' : (scanPrefix 0 marks)[start]'(by rw [scanPrefix_length]; omega) <
              (scanPrefix 0 marks)[s]'(by rw [scanPrefix_length]; omega) := by omega
          rcases hih.mp hlt' with ⟨k, hk, h1, h2, hmk⟩
          exact ⟨k, hk, h1, by omega, hmk⟩
      · rintro ⟨k, hk, h1, h2, hmk⟩
        by_cases hks : k = s
        · subst hks
          rw [hmk] at hstep
          simp only [↓reduceIte] at hstep
          omega
        · have hlt' : (scanPrefix 0 marks)[start]'(by rw [scanPrefix_length]; omega) <
              (scanPrefix 0 marks)[s]'(by rw [scanPrefix_length]; omega) :=
            hih.mpr ⟨k, hk, h1, by omega, hmk⟩
          omega

theorem hasAnchor_iff_exists_in_range (marks : List Bool) (start stop : Nat) :
    prefixHasAnchor (buildPrefix marks) start stop = true ↔
    (stop ≤ marks.length ∧ ∃ (k : Nat) (hk : k < marks.length), start ≤ k ∧ k < stop ∧ marks[k] = true) := by
  unfold prefixHasAnchor
  have hlen : (buildPrefix marks).length = marks.length + 1 := (prefix_length_and_zero marks).1
  by_cases hs : start < (buildPrefix marks).length
  · by_cases he : stop < (buildPrefix marks).length
    · simp only [List.getElem?_eq_getElem hs, List.getElem?_eq_getElem he, decide_eq_true_eq]
      by_cases hle : start ≤ stop
      · constructor
        · intro hlt
          exact ⟨by omega, (prefix_strict_lt_iff_exists marks stop (by omega) start hle).mp hlt⟩
        · rintro ⟨_, hex⟩
          exact (prefix_strict_lt_iff_exists marks stop (by omega) start hle).mpr hex
      · have hgt : stop ≤ start := by omega
        have hmono := prefix_monotone marks stop start hgt (by omega)
        constructor
        · intro hlt; omega
        · rintro ⟨_, ⟨k, _, h1, h2, _⟩⟩; omega
    · simp only [List.getElem?_eq_none (by omega : (buildPrefix marks).length ≤ stop),
        List.getElem?_eq_getElem hs]
      constructor
      · intro h; simp at h
      · rintro ⟨hstop, _⟩; omega
  · simp only [List.getElem?_eq_none (by omega : (buildPrefix marks).length ≤ start)]
    constructor
    · intro h; simp at h
    · rintro ⟨hstop, ⟨k, _, h1, h2, _⟩⟩; omega

/-! ### A5: Empty fast-path and out-of-bounds queries always return `false` -/

theorem hasAnchor_empty_and_oob_false (pref : List Nat) (start stop : Nat)
    (h : pref = [] ∨ pref.length ≤ start ∨ pref.length ≤ stop) :
    prefixHasAnchor pref start stop = false := by
  unfold prefixHasAnchor
  rcases h with rfl | hstart | hstop
  · simp
  · simp [List.getElem?_eq_none hstart]
  · cases pref[start]? <;> simp [List.getElem?_eq_none hstop]

end F6DiagnosticAnchor
