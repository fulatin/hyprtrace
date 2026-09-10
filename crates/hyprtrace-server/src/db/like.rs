/// One unit of a LIKE pattern.
enum LikeUnit {
    /// `%` — matches any sequence, including an empty one.
    AnySeq,
    /// `_` — matches exactly one character.
    AnyChar,
    /// Anything else, including a wildcard that was escaped with `\`.
    Literal(char),
}

/// Decode the pattern unit starting at `i`, plus how many chars it consumed.
/// A backslash escapes `%`, `_` and itself, matching SQLite's `LIKE ... ESCAPE
/// '\'`. A trailing lone backslash is a literal backslash.
fn like_unit(p: &[char], i: usize) -> Option<(LikeUnit, usize)> {
    let c = *p.get(i)?;
    match c {
        '\\' if i + 1 < p.len() => Some((LikeUnit::Literal(p[i + 1]), 2)),
        '%' => Some((LikeUnit::AnySeq, 1)),
        '_' => Some((LikeUnit::AnyChar, 1)),
        _ => Some((LikeUnit::Literal(c), 1)),
    }
}

/// SQLite LIKE-style pattern match: `%` = any sequence, `_` = one char, `\`
/// escapes either of them. Case-insensitive. Used for app-category rules.
///
/// Two things differ from the previous implementation. `?` used to be accepted
/// as a second single-char wildcard even though SQLite's LIKE has no `?`
/// operator, so a rule written `Code?` silently also matched `Coder`; it is a
/// literal question mark now. And `%` is checked before literal equality, so a
/// pattern `%` no longer matches a class that happens to contain a percent
/// sign — the wildcard always wins, and a literal `%` is spelled `\%`.
pub(crate) fn like_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    // Position of the most recent `%` and the text offset it will retry from.
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        match like_unit(&p, pi) {
            Some((LikeUnit::AnySeq, _)) => {
                star = pi;
                mark = ti;
                pi += 1;
            }
            Some((LikeUnit::AnyChar, w)) => {
                pi += w;
                ti += 1;
            }
            Some((LikeUnit::Literal(c), w)) if c == t[ti] => {
                pi += w;
                ti += 1;
            }
            // Pattern exhausted or a literal mismatch: backtrack to the last
            // `%` and have it absorb one more character.
            _ if star != usize::MAX => {
                pi = star + 1;
                mark += 1;
                ti = mark;
            }
            _ => return false,
        }
    }
    // Trailing `%`s match the empty tail.
    while let Some((LikeUnit::AnySeq, w)) = like_unit(&p, pi) {
        pi += w;
    }
    pi == p.len()
}
