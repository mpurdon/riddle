//! Byte-level text helpers every crate cuts strings with. One home for the
//! char-boundary walk: three crates carried their own copy, and the two
//! without the `end > 0` guard could underflow on a string that starts with
//! a multi-byte character.

/// The largest index `<= max` that is a char boundary of `text`, so a byte
/// slice `&text[..n]` never splits a character. Returns 0 for an empty text.
pub fn floor_char_boundary(text: &str, max: usize) -> usize {
    let mut end = max.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// A count with its noun: `1 message`, `3 messages`, `2 Meshes`. One home
/// for the English every Activity sentence and Action summary needs, so no
/// call site hand-rolls `if n == 1 { "" } else { "s" }`.
pub fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        return format!("1 {noun}");
    }
    let lower = noun.to_ascii_lowercase();
    if lower.ends_with("sh")
        || lower.ends_with("ch")
        || lower.ends_with('s')
        || lower.ends_with('x')
    {
        format!("{n} {noun}es")
    } else {
        format!("{n} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::{floor_char_boundary, plural};

    #[test]
    fn a_count_reads_with_its_noun_in_the_right_number() {
        assert_eq!(plural(1, "message"), "1 message");
        assert_eq!(plural(0, "message"), "0 messages");
        assert_eq!(plural(3, "Mesh"), "3 Meshes");
        assert_eq!(plural(2, "account"), "2 accounts");
    }

    #[test]
    fn a_cut_never_lands_inside_a_character() {
        let s = "añb";
        assert_eq!(floor_char_boundary(s, 2), 1, "inside ñ backs up to 1");
        assert_eq!(floor_char_boundary(s, 3), 3);
        assert_eq!(floor_char_boundary(s, 99), s.len());
        assert_eq!(floor_char_boundary("ñ", 1), 0);
        assert_eq!(floor_char_boundary("", 4), 0);
    }
}
