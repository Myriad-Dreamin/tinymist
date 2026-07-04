## Context

Math-mode field completion in tinymist already has special filtering for equation contexts. That filtering is useful for plain math expressions and postfix-capable values, but it currently applies too aggressively to code-interpolated module accesses such as `$ #calc. $`.

In practice, the completion pipeline can already resolve that `#calc` is a module-valued target, yet the later math-mode filtering step still drops exported pure functions that are available in code and markup mode. This creates the inconsistency reported in `#2401`: users inside equations see a narrower completion list for `#calc.` than they do outside equations even though the target expression is explicitly code-interpolated.

The implementation impact is narrow and centered in `crates/tinymist-query/src/analysis/completion/field_access.rs`, with regression coverage in the completion fixtures and snapshots under `crates/tinymist-query/src/fixtures/completion/`.

## Goals / Non-Goals

**Goals:**
- Preserve exported pure function members when the completion target is a code-interpolated, module-valued field access inside math mode.
- Keep the existing math-mode completion behavior for non-module targets such as symbols, content values, and postfix-capable expressions.
- Add focused regression coverage for both full-member and prefix-filtered module completion in equations.

**Non-Goals:**
- Changing plain math completion for non-code expressions such as `$ calc. $` or `$ calc.o $`.
- Broadening math-mode completion behavior beyond module-valued field access.
- Refactoring unrelated completion ranking or postfix logic.

## Decisions

### 1. Distinguish module-valued targets before applying math-mode filtering

The completion path should preserve module exports when the target expression is both code-interpolated and resolved as a module. This keeps the existing math-mode branch for true math expressions while preventing it from hiding legitimate module members from `#module.field` accesses.

Alternative considered:
- Relax math-mode filtering for all field accesses inside equations. Rejected because it would change long-standing completion behavior for symbols and other non-module values that rely on the current math-specific filtering.

### 2. Keep non-module math completion behavior unchanged

The special case should be narrow: only module-valued targets bypass the filtering that removes exported pure functions. Symbol field members and postfix completions should continue to come from the current math-mode logic so existing fixtures like `field_math_dot.typ` and `field_math_postfix.typ` remain stable.

Alternative considered:
- Route every `#expr.` access through the code-mode completion path. Rejected because code interpolation still lives inside an equation, and non-module values there should keep the math-aware completion experience users already depend on.

### 3. Lock the behavior with paired module fixtures

Regression coverage should include both:
- `$ #calc./* range 0..1 */ $` to prove exported functions such as `odd` are visible.
- `$ #calc.o/* range 0..1 */ $` to prove prefix filtering still keeps matching module exports.

Existing non-module math fixtures should continue to pass unchanged to show the new exception does not leak into other completion cases.

Alternative considered:
- Rely on a single broad fixture for `#calc.`. Rejected because it would miss regressions where prefix filtering still drops module exports even if the unfiltered member list looks correct.

## Risks / Trade-offs

- [Module detection could be too broad and expose code-mode members for non-module values] -> Mitigate by gating the relaxed path on resolved module-valued targets only.
- [A narrow exception in completion flow can be easy to regress later] -> Mitigate by adding explicit fixtures for both unfiltered and prefix-filtered `#calc` cases.
- [Behavior may diverge between code-interpolated and plain math expressions] -> Mitigate by making that distinction intentional in the spec and preserving plain math behavior in existing fixtures.

## Migration Plan

1. Adjust field-access completion to preserve exported pure functions for code-interpolated module-valued targets in math mode.
2. Re-run focused `tinymist-query` completion snapshot tests covering the new fixtures and existing non-module math fixtures.
3. Review snapshot output to confirm module exports were added only for the intended `#calc` cases.

## Open Questions

- No additional open questions are required for this change. If future issues show similar gaps for other context-sensitive completion targets, the same resolved-target-first pattern can be revisited more broadly.
