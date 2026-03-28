## 1. Update citation completion insertion logic

- [ ] 1.1 Add a citation-only helper in `crates/tinymist-query/src/analysis/completion/typst_specific.rs` that chooses `<key>` for bibliography keys representable as Typst label literals and `label("key")` otherwise. typst parser detects it using `is_valid_label_literal_id`:
    ```rust
    use unicode_ident::is_xid_continue;
    /// Whether a character can continue an identifier.
    #[inline]
    pub fn is_id_continue(c: char) -> bool {
        is_xid_continue(c) || c == '_' || c == '-'
    }

    /// Whether a character can be part of a label literal's name.
    #[inline]
    fn is_valid_in_label_literal(c: char) -> bool {
        is_id_continue(c) || matches!(c, ':' | '.')
    }

    /// Returns true if this string is valid in a label literal.
    pub fn is_valid_label_literal_id(id: &str) -> bool {
        !id.is_empty() && id.chars().all(is_valid_in_label_literal)
    }
    ```
- [ ] 1.2 Route bibliography-key and bibliography-title completion items through the shared citation insertion helper while keeping non-citation label and reference completion behavior unchanged.

## 2. Add regression coverage for explicit-label bibliography keys

- [ ] 2.1 Add a completion fixture with a bibliography key such as `DBLP:books/lib/Knuth86a` that currently requires `label("...")`, and snapshot the accepted completion text for the raw-key item.
- [ ] 2.2 Add or extend coverage for the title-backed completion item of the same bibliography entry so it inserts the same `label("...")` text as the raw-key item.
- [ ] 2.3 Re-run the existing compatible-key citation fixtures, such as `completion_title2.typ`, to confirm keys that already work as label literals still insert `<key>`.

## 3. Validate the change

- [ ] 3.1 Run focused `tinymist-query` completion snapshot tests covering the new explicit-label fixture and the existing compatible-key citation fixtures, then review the snapshot diffs.
