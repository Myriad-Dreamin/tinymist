/// Trim style naming from a family name and fix bad names.
#[allow(dead_code)]
pub fn typst_typographic_family(mut family: &str) -> &str {
    // Separators between names, modifiers and styles.
    const SEPARATORS: [char; 3] = [' ', '-', '_'];

    // Modifiers that can appear in combination with suffixes.
    const MODIFIERS: &[&str] = &[
        "extra", "ext", "ex", "x", "semi", "sem", "sm", "demi", "dem", "ultra",
    ];

    // Style suffixes.
    #[rustfmt::skip]
    const SUFFIXES: &[&str] = &[
        "normal", "italic", "oblique", "slanted",
        "thin", "th", "hairline", "light", "lt", "regular", "medium", "med",
        "md", "bold", "bd", "demi", "extb", "black", "blk", "bk", "heavy",
        "narrow", "condensed", "cond", "cn", "cd", "compressed", "expanded", "exp"
    ];

    let mut extra = [].as_slice();
    let newcm = family.starts_with("NewCM") || family.starts_with("NewComputerModern");
    if newcm {
        extra = &["book"];
    }

    // Trim spacing and weird leading dots in Apple fonts.
    family = family.trim().trim_start_matches('.');

    // Lowercase the string so that the suffixes match case-insensitively.
    let lower = family.to_ascii_lowercase();
    let mut len = usize::MAX;
    let mut trimmed = lower.as_str();

    // Trim style suffixes repeatedly.
    while trimmed.len() < len {
        len = trimmed.len();

        // Find style suffix.
        let mut t = trimmed;
        let mut shortened = false;
        while let Some(s) = SUFFIXES.iter().chain(extra).find_map(|s| t.strip_suffix(s)) {
            shortened = true;
            t = s;
        }

        if !shortened {
            break;
        }

        // Strip optional separator.
        if let Some(s) = t.strip_suffix(SEPARATORS) {
            trimmed = s;
            t = s;
        }

        // Also allow an extra modifier, but apply it only if it is separated it
        // from the text before it (to prevent false positives).
        if let Some(t) = MODIFIERS.iter().find_map(|s| t.strip_suffix(s)) {
            if let Some(stripped) = t.strip_suffix(SEPARATORS) {
                trimmed = stripped;
            }
        }
    }

    // Apply style suffix trimming.
    family = &family[..len];

    if newcm {
        family = family.trim_end_matches("10");
    }

    // Fix bad names.
    match family {
        "Noto Sans Symbols2" => "Noto Sans Symbols 2",
        "NewComputerModern" => "New Computer Modern",
        "NewComputerModernMono" => "New Computer Modern Mono",
        "NewComputerModernSans" => "New Computer Modern Sans",
        "NewComputerModernMath" => "New Computer Modern Math",
        "NewCMUncial" | "NewComputerModernUncial" => "New Computer Modern Uncial",
        other => other,
    }
}
