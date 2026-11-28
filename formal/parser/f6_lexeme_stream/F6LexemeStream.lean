/-
  SPEC-0505 (F6 eighth slice): Machine-checked unbounded model proofs for
  `validate_lexemes` lexeme-stream contiguous partition, strict start
  monotonicity, and unique trailing empty EOF contract
  (`crates/lang-frontend/src/parser/engine.rs`).

  Discharges the strict-monotonic-starts TCB precondition used by
  `diagnostic_anchor_index` (`binary_search_by_key`) and `angle_group_closers`.
  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `lexeme_stream_kani.rs`.
-/

namespace F6LexemeStream

structure LexSpan where
  start : Nat
  stop : Nat
  isEof : Bool
  deriving DecidableEq, Repr

/-- Exact functional model of `validate_lexeme_spans_core` loop from index `i`. -/
def validateLoop (L : List LexSpan) (sourceLen : Nat) : Nat → Nat → Bool → Bool
  | i, nextStart, sawEof =>
    if h : i < L.length then
      let item := L[i]
      if item.start != nextStart || item.start > item.stop then
        false
      else if item.isEof then
        if sawEof || i + 1 != L.length || item.start != item.stop || item.start != sourceLen then
          false
        else
          validateLoop L sourceLen (i + 1) nextStart true
      else
        if item.start == item.stop then
          false
        else
          validateLoop L sourceLen (i + 1) item.stop sawEof
    else
      sawEof && (nextStart == sourceLen)
termination_by i _ _ => L.length - i

def validateStream (L : List LexSpan) (sourceLen : Nat) : Bool :=
  if L.isEmpty then false else validateLoop L sourceLen 0 0 false

/-! ### V1: Any valid stream is non-empty -/

theorem valid_nonempty (L : List LexSpan) (sourceLen : Nat)
    (h : validateStream L sourceLen = true) :
    0 < L.length := by
  unfold validateStream at h
  cases L with
  | nil => simp at h
  | cons _ _ => simp

/-! ### Helper: `sawEof = false` forces at least one remaining element -/

private theorem validateLoop_false_implies_lt (L : List LexSpan) (sourceLen : Nat)
    (i nextStart : Nat) (hval : validateLoop L sourceLen i nextStart false = true) :
    i < L.length := by
  cases Nat.lt_or_ge i L.length with
  | inl hlt => exact hlt
  | inr hge =>
    have hnot : ¬(i < L.length) := Nat.not_lt.mpr hge
    unfold validateLoop at hval
    simp [hnot] at hval

/-! ### Core inductive characterization of `validateLoop` with `sawEof = false` -/

private theorem validateLoop_spec (L : List LexSpan) (sourceLen : Nat) :
    ∀ (i : Nat) (nextStart : Nat),
      i ≤ L.length →
      validateLoop L sourceLen i nextStart false = true →
      (∀ (k : Nat) (hk : k < L.length), i ≤ k →
        (k + 1 = L.length →
          L[k].isEof = true ∧ L[k].start = sourceLen ∧ L[k].stop = sourceLen) ∧
        (k + 1 < L.length →
          L[k].isEof = false ∧ L[k].start < L[k].stop ∧
          ∀ (hk1 : k + 1 < L.length), L[k + 1].start = L[k].stop) ∧
        (k = i → L[k].start = nextStart)) := by
  intro i nextStart hle hval
  have hlt : i < L.length := validateLoop_false_implies_lt L sourceLen i nextStart hval
  unfold validateLoop at hval
  simp only [hlt, ↓reduceDIte] at hval
  by_cases hbad : (L[i].start != nextStart || decide (L[i].start > L[i].stop)) = true
  · simp [hbad] at hval
  · simp only [hbad, Bool.false_eq_true, ↓reduceIte] at hval
    have hstart : L[i].start = nextStart := by
      simp only [Bool.or_eq_true, bne_iff_ne, ne_eq, decide_eq_true_eq, not_or] at hbad
      omega
    have hle_span : L[i].start ≤ L[i].stop := by
      simp only [Bool.or_eq_true, bne_iff_ne, ne_eq, decide_eq_true_eq, not_or] at hbad
      omega
    cases heof : L[i].isEof with
    | true =>
      simp only [heof, ↓reduceIte] at hval
      split at hval
      · simp at hval
      · rename_i heofbad
        simp only [Bool.false_or, Bool.or_eq_true, bne_iff_ne, ne_eq, not_or] at heofbad
        have hlast : i + 1 = L.length := by omega
        have hempty : L[i].start = L[i].stop := by omega
        have hsrc : L[i].start = sourceLen := by omega
        intro k hk hik
        have hki : k = i := by omega
        subst hki
        refine ⟨?_, ?_, ?_⟩
        · intro _
          exact ⟨heof, hsrc, hempty.symm.trans hsrc⟩
        · intro hlt2
          omega
        · intro _
          exact hstart
    | false =>
      simp only [heof, Bool.false_eq_true, ↓reduceIte] at hval
      by_cases hempty : (L[i].start == L[i].stop) = true
      · simp [hempty] at hval
      · simp only [hempty, Bool.false_eq_true, ↓reduceIte] at hval
        have hlt_span : L[i].start < L[i].stop := by
          simp only [beq_iff_eq] at hempty
          omega
        have hnext_lt : i + 1 < L.length :=
          validateLoop_false_implies_lt L sourceLen (i + 1) L[i].stop hval
        have ih := validateLoop_spec L sourceLen (i + 1) L[i].stop (by omega) hval
        intro k hk hik
        by_cases hki : k = i
        · subst hki
          refine ⟨?_, ?_, ?_⟩
          · intro hlast
            omega
          · intro hlt2
            have hnext := (ih (k + 1) hlt2 (Nat.le_refl _)).2.2 rfl
            exact ⟨heof, hlt_span, fun _ => hnext⟩
          · intro _
            exact hstart
        · have hgt : i + 1 ≤ k := by omega
          have hres := ih k hk hgt
          exact ⟨hres.1, hres.2.1, fun hc => absurd hc hki⟩
termination_by i _ _ _ => L.length - i

/-! ### V2: Last element of a valid stream is an empty EOF at `sourceLen` -/

theorem valid_last_is_eof_at_sourceLen (L : List LexSpan) (sourceLen : Nat)
    (h : validateStream L sourceLen = true) :
    ∃ (hlt : L.length - 1 < L.length),
      L[L.length - 1].isEof = true ∧
      L[L.length - 1].start = sourceLen ∧
      L[L.length - 1].stop = sourceLen := by
  have hpos := valid_nonempty L sourceLen h
  unfold validateStream at h
  have hne : L.isEmpty = false := by cases L <;> simp at hpos ⊢
  simp only [hne, Bool.false_eq_true, ↓reduceIte] at h
  have hspec := validateLoop_spec L sourceLen 0 0 (Nat.zero_le _) h
  refine ⟨by omega, ?_⟩
  exact (hspec (L.length - 1) (by omega) (Nat.zero_le _)).1 (by omega)

/-! ### V3: Every non-last element is non-EOF with strictly positive span -/

theorem valid_non_last_non_eof_and_non_empty (L : List LexSpan) (sourceLen : Nat)
    (h : validateStream L sourceLen = true)
    (k : Nat) (hk : k + 1 < L.length) :
    L[k].isEof = false ∧ L[k].start < L[k].stop := by
  have hpos := valid_nonempty L sourceLen h
  unfold validateStream at h
  have hne : L.isEmpty = false := by cases L <;> simp at hpos ⊢
  simp only [hne, Bool.false_eq_true, ↓reduceIte] at h
  have hspec := validateLoop_spec L sourceLen 0 0 (Nat.zero_le _) h
  have hrec := (hspec k (by omega) (Nat.zero_le _)).2.1 hk
  exact ⟨hrec.1, hrec.2.1⟩

/-! ### V4: Contiguous partition (`L[0].start = 0` and `L[k+1].start = L[k].stop`) -/

theorem valid_contiguous_partition (L : List LexSpan) (sourceLen : Nat)
    (h : validateStream L sourceLen = true) :
    (∀ (h0 : 0 < L.length), L[0].start = 0) ∧
    (∀ (k : Nat) (hk1 : k + 1 < L.length), L[k + 1].start = L[k].stop) := by
  have hpos := valid_nonempty L sourceLen h
  unfold validateStream at h
  have hne : L.isEmpty = false := by cases L <;> simp at hpos ⊢
  simp only [hne, Bool.false_eq_true, ↓reduceIte] at h
  have hspec := validateLoop_spec L sourceLen 0 0 (Nat.zero_le _) h
  constructor
  · intro h0
    exact (hspec 0 h0 (Nat.zero_le _)).2.2 rfl
  · intro k hk1
    exact (hspec k (by omega) (Nat.zero_le _)).2.1 hk1 |>.2.2 hk1

/-! ### V5: Strict monotonicity of starts (`∀ p < q < L.length, L[p].start < L[q].start`) -/

private theorem strict_mono_from_step (L : List LexSpan)
    (hstep : ∀ (k : Nat) (hk1 : k + 1 < L.length), L[k].start < L[k + 1].start) :
    ∀ (q : Nat) (hq : q < L.length) (p : Nat) (hp : p < q),
      (L[p]'(Nat.lt_trans hp hq)).start < L[q].start := by
  intro q
  induction q with
  | zero => intro _ _ hp; omega
  | succ q ih =>
    intro hq p hp
    have hlt_step := hstep q hq
    by_cases hpq : p = q
    · subst hpq
      exact hlt_step
    · have ih' := ih (by omega) p (by omega)
      omega

theorem valid_starts_strictly_increasing (L : List LexSpan) (sourceLen : Nat)
    (h : validateStream L sourceLen = true)
    (p q : Nat) (hpq : p < q) (hq : q < L.length) :
    (L[p]'(Nat.lt_trans hpq hq)).start < L[q].start := by
  have hpart := (valid_contiguous_partition L sourceLen h).2
  apply strict_mono_from_step L _ q hq p hpq
  intro k hk1
  have hnonempty := (valid_non_last_non_eof_and_non_empty L sourceLen h k hk1).2
  have hcont := hpart k hk1
  omega

end F6LexemeStream
