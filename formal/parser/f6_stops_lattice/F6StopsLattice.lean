/-
  SPEC-0508 (F6 sixth slice): Machine-checked algebraic proofs for the
  Expression / TypeRef / Declaration boundary propagation lattice
  (`Stops`, `TypeStops`, `DeclarationStops`) in
  `crates/lang-frontend/src/parser/engine/boundary.rs`.

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, complete finite lattice)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `stops_kani.rs`.
-/

namespace F6StopsLattice

structure ExprStops where
  rightParen : Bool
  rightBracket : Bool
  comma : Bool
  interpEnd : Bool
  rightBrace : Bool
  arrow : Bool
  lambdaComma : Bool
  fileDecl : Bool
  elseKw : Bool
  semicolon : Bool
  blockStmtSep : Bool
  blockElements : Bool
  deriving DecidableEq, Repr

structure TypeStops where
  comma : Bool
  greater : Bool
  rightParen : Bool
  rightBracket : Bool
  interpEnd : Bool
  equal : Bool
  leftBrace : Bool
  rightBrace : Bool
  blockElement : Bool
  file : Bool
  arrow : Bool
  extensionName : Bool
  semicolon : Bool
  classBody : Bool
  classFile : Bool
  blockStmtSep : Bool
  deriving DecidableEq, Repr

structure DeclStops where
  comma : Bool
  rightParen : Bool
  greater : Bool
  leftBrace : Bool
  rightBrace : Bool
  blockElement : Bool
  rightBracket : Bool
  interpEnd : Bool
  file : Bool
  semicolon : Bool
  classMember : Bool
  arrow : Bool
  elseKw : Bool
  controlExpr : Bool
  classFile : Bool
  supertypeSuffix : Bool
  typeElement : Bool
  blockStmtSep : Bool
  deriving DecidableEq, Repr

inductive HeaderOwner where
  | file
  | member
  | declaration
  deriving DecidableEq, Repr

inductive TokenClass where
  | eof
  | rightParen
  | rightBracket
  | rightBrace
  | interpEnd
  | comma
  | semicolon
  | arrow
  | elseKw
  | leftBrace
  | valVar
  | other
  deriving DecidableEq, Repr

def ExprStops.containsHard (s : ExprStops) : TokenClass → Bool
  | .eof => true
  | .rightParen => s.rightParen
  | .rightBracket => s.rightBracket
  | .interpEnd => s.interpEnd
  | _ => false

def ExprStops.contains (s : ExprStops) : TokenClass → Bool
  | .eof => true
  | .rightParen => s.rightParen
  | .rightBracket => s.rightBracket
  | .rightBrace => s.rightBrace
  | .interpEnd => s.interpEnd
  | .comma => s.comma || s.lambdaComma
  | .semicolon => s.fileDecl || s.semicolon
  | .arrow => s.arrow
  | .elseKw => s.elseKw
  | .leftBrace | .valVar => s.blockElements
  | .other => false

def ExprStops.blockExpr (outer : ExprStops) : ExprStops :=
  { rightParen := outer.rightParen
    rightBracket := outer.rightBracket
    comma := false
    interpEnd := outer.interpEnd
    rightBrace := true
    arrow := false
    lambdaComma := false
    fileDecl := false
    elseKw := false
    semicolon := false
    blockStmtSep := false
    blockElements := true }

def ExprStops.lambdaExpr (outer : ExprStops) : ExprStops :=
  { rightParen := outer.rightParen
    rightBracket := outer.rightBracket
    comma := false
    interpEnd := outer.interpEnd
    rightBrace := true
    arrow := true
    lambdaComma := true
    fileDecl := false
    elseKw := false
    semicolon := false
    blockStmtSep := false
    blockElements := true }

def ExprStops.withoutLambdaSoft (s : ExprStops) : ExprStops :=
  { s with
    lambdaComma := false
    arrow := false
    fileDecl := false
    semicolon := false
    blockElements := false }

def ExprStops.withoutFileSemi (s : ExprStops) : ExprStops :=
  { s with
    fileDecl := false
    semicolon := false }

def TypeStops.fromExpr (s : ExprStops) : TypeStops :=
  { comma := s.comma || s.lambdaComma
    greater := false
    rightParen := s.rightParen
    rightBracket := s.rightBracket
    interpEnd := s.interpEnd
    equal := false
    leftBrace := s.blockElements
    rightBrace := s.rightBrace
    blockElement := s.blockElements
    file := s.fileDecl
    arrow := s.arrow
    extensionName := false
    semicolon := s.semicolon
    classBody := false
    classFile := false
    blockStmtSep := s.blockStmtSep }

def TypeStops.contains (t : TypeStops) : TokenClass → Bool
  | .eof => true
  | .comma => t.comma
  | .rightParen => t.rightParen
  | .rightBracket => t.rightBracket
  | .interpEnd => t.interpEnd
  | .arrow => t.arrow
  | .leftBrace => t.leftBrace || t.classBody
  | .rightBrace => t.rightBrace
  | .valVar => t.blockElement
  | .semicolon => t.file || t.semicolon
  | _ => false

def TypeStops.forProvenFnParams (t : TypeStops) : TypeStops :=
  { t with
    greater := false
    rightBracket := false
    arrow := false
    leftBrace := false
    rightBrace := false
    classBody := false }

def DeclStops.fromExprHard (s : ExprStops) : DeclStops :=
  { comma := false
    rightParen := s.rightParen
    greater := false
    leftBrace := false
    rightBrace := s.rightBrace
    blockElement := false
    rightBracket := s.rightBracket
    interpEnd := s.interpEnd
    file := false
    semicolon := false
    classMember := false
    arrow := false
    elseKw := false
    controlExpr := false
    classFile := false
    supertypeSuffix := false
    typeElement := false
    blockStmtSep := s.blockStmtSep }

def DeclStops.fromExpr (s : ExprStops) : DeclStops :=
  { DeclStops.fromExprHard s with
    comma := s.comma || s.lambdaComma
    arrow := s.arrow
    elseKw := s.elseKw
    file := s.fileDecl
    semicolon := s.semicolon
    leftBrace := s.blockElements
    blockElement := s.blockElements
    controlExpr := s.blockElements }

def DeclStops.containsHard (d : DeclStops) : TokenClass → Bool
  | .rightParen => d.rightParen
  | .rightBracket => d.rightBracket
  | .rightBrace => d.rightBrace
  | .leftBrace => d.leftBrace
  | .interpEnd => d.interpEnd
  | .valVar => d.blockElement
  | _ => false

def TypeStops.supertypeRecovery (t : TypeStops) (owner : HeaderOwner) : DeclStops :=
  { comma := t.comma
    rightParen := t.rightParen
    greater := t.greater
    leftBrace := t.leftBrace || t.classBody
    rightBrace := t.rightBrace
    blockElement := false
    rightBracket := t.rightBracket
    interpEnd := t.interpEnd
    file := false
    semicolon := true
    classMember := owner == .member
    arrow := t.arrow
    elseKw := false
    controlExpr := false
    classFile := owner == .file
    supertypeSuffix := true
    typeElement := true
    blockStmtSep := false }

/-! ### S1: Every hard stop in `ExprStops` is a stop -/

theorem containsHard_implies_contains (s : ExprStops) (tok : TokenClass) :
    s.containsHard tok = true → s.contains tok = true := by
  cases tok <;> simp [ExprStops.containsHard, ExprStops.contains]

/-! ### S2: `blockExpr` preserves all outer hard delimiters and adds `rightBrace` -/

theorem blockExpr_preserves_hard (outer : ExprStops) (tok : TokenClass) :
    (outer.containsHard tok = true → (ExprStops.blockExpr outer).containsHard tok = true) ∧
    (ExprStops.blockExpr outer).rightBrace = true := by
  constructor
  · cases tok <;> simp [ExprStops.containsHard, ExprStops.blockExpr]
  · rfl

/-! ### S3: `lambdaExpr` preserves all outer hard delimiters and adds brace/comma/arrow -/

theorem lambdaExpr_preserves_hard (outer : ExprStops) (tok : TokenClass) :
    (outer.containsHard tok = true → (ExprStops.lambdaExpr outer).containsHard tok = true) ∧
    (ExprStops.lambdaExpr outer).rightBrace = true ∧
    (ExprStops.lambdaExpr outer).lambdaComma = true ∧
    (ExprStops.lambdaExpr outer).arrow = true := by
  refine ⟨?_, rfl, rfl, rfl⟩
  cases tok <;> simp [ExprStops.containsHard, ExprStops.lambdaExpr]

/-! ### S4: Soft-stop stripping never drops any hard delimiter -/

theorem stripSoft_preserves_hard (s : ExprStops) (tok : TokenClass) :
    (s.withoutLambdaSoft.containsHard tok = s.containsHard tok) ∧
    (s.withoutFileSemi.containsHard tok = s.containsHard tok) := by
  cases tok <;> simp [ExprStops.containsHard, ExprStops.withoutLambdaSoft, ExprStops.withoutFileSemi]

/-! ### S5: `TypeStops.fromExpr` preserves all hard delimiters and `rightBrace` -/

theorem typeFromExpr_preserves_hard (s : ExprStops) (tok : TokenClass) :
    (s.containsHard tok = true → (TypeStops.fromExpr s).contains tok = true) ∧
    (TypeStops.fromExpr s).rightBrace = s.rightBrace := by
  constructor
  · cases tok <;> simp [ExprStops.containsHard, TypeStops.fromExpr, TypeStops.contains]
  · rfl

/-! ### S6: `DeclStops.fromExprHard` ⊆ `DeclStops.fromExpr` and both preserve non-EOF hard stops -/

theorem declFromExpr_monotone_and_preserves_hard (s : ExprStops) (tok : TokenClass) :
    ((DeclStops.fromExprHard s).containsHard tok = true → (DeclStops.fromExpr s).containsHard tok = true) ∧
    (tok ≠ .eof → s.containsHard tok = true → (DeclStops.fromExprHard s).containsHard tok = true) := by
  constructor
  · cases tok <;> simp [DeclStops.containsHard, DeclStops.fromExprHard, DeclStops.fromExpr]
  · cases tok <;> simp [ExprStops.containsHard, DeclStops.containsHard, DeclStops.fromExprHard]

/-! ### S7: `supertypeRecovery` preserves hard delimiters and injects owner/supertype flags -/

theorem supertypeRecovery_preserves_hard_and_owner (t : TypeStops) (owner : HeaderOwner) :
    let d := t.supertypeRecovery owner
    d.rightParen = t.rightParen ∧
    d.rightBracket = t.rightBracket ∧
    d.rightBrace = t.rightBrace ∧
    d.interpEnd = t.interpEnd ∧
    d.supertypeSuffix = true ∧
    d.typeElement = true ∧
    d.semicolon = true ∧
    d.classFile = (owner == .file) ∧
    d.classMember = (owner == .member) := by
  simp [TypeStops.supertypeRecovery]

/-! ### S8: `forProvenFnParams` preserves inner hard stops while clearing outer delimiters -/

theorem forProvenFnParams_preserves_inner_hard_clears_outer (t : TypeStops) :
    let p := t.forProvenFnParams
    p.rightParen = t.rightParen ∧
    p.interpEnd = t.interpEnd ∧
    p.comma = t.comma ∧
    p.file = t.file ∧
    p.semicolon = t.semicolon ∧
    p.blockStmtSep = t.blockStmtSep ∧
    p.greater = false ∧
    p.rightBracket = false ∧
    p.arrow = false ∧
    p.leftBrace = false ∧
    p.rightBrace = false ∧
    p.classBody = false := by
  simp [TypeStops.forProvenFnParams]

end F6StopsLattice
