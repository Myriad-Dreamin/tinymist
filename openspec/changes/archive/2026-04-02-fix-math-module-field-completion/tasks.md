## 1. Relax math-mode module export filtering

- [x] 1.1 Update math-mode field access completion so code-interpolated, module-valued targets do not drop exported pure functions from the dot-access completion path.
- [x] 1.2 Keep the current math-mode filtering and postfix behavior for non-module targets such as symbols, content values, and other postfix-capable expressions.

## 2. Add regression coverage

- [x] 2.1 Add a completion fixture for `$ #calc./* range 0..1 */ $` that snapshots exported function members such as `odd`.
- [x] 2.2 Add a prefix-filtered math completion fixture for `$ #calc.o/* range 0..1 */ $` that verifies matching module functions remain available in math mode.
- [x] 2.3 Re-run existing non-module math field and postfix fixtures, such as `field_math_dot.typ` and `field_math_postfix.typ`, to confirm they keep their current behavior.

## 3. Validate the change

- [x] 3.1 Run focused `tinymist-query` completion snapshot tests covering the new math-module fixtures and review the resulting snapshots.
