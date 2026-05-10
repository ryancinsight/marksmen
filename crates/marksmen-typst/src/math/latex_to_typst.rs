//! LaTeX math → Typst math syntax translation.
//!
//! ## Theorem: Translation Correctness
//!
//! For a LaTeX math expression `L`, the translated Typst expression `T`
//! must render to a visually identical glyph sequence under Typst's math
//! engine. The translation is a syntactic rewrite operating on the following
//! grammar classes:
//!
//! 1. **Commands** (`\cmd{args}` → `cmd(args)`)
//! 2. **Environments** (`\begin{env}...\end{env}` → Typst equivalents)
//! 3. **Symbols** (`\alpha` → `alpha`, `\infty` → `infinity`)
//! 4. **Operators** (`\frac{a}{b}` → `frac(a, b)`)
//! 5. **Pass-through**: Operators `+`, `-`, `=`, `^`, `_` are identical in both

/// Translate a LaTeX math expression to Typst math syntax.
///
/// Handles common LaTeX constructs. Unrecognized commands are passed
/// through with a tracing warning — Typst will render them as-is or
/// report an error.
pub fn latex_to_typst(latex: &str) -> String {
    let mut result = String::with_capacity(latex.len());
    let chars: Vec<char> = latex.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        match chars[i] {
            '\\' if i + 1 < len => {
                // Parse the command name.
                let cmd_start = i + 1;
                let mut cmd_end = cmd_start;
                while cmd_end < len && chars[cmd_end].is_ascii_alphabetic() {
                    cmd_end += 1;
                }

                if cmd_end == cmd_start {
                    // Escaped character (e.g., `\\`, `\,`, `\;`, `\!`).
                    match chars[cmd_start] {
                        '\\' => {
                            result.push_str("\\ ");
                            i = cmd_start + 1;
                        }
                        ',' => {
                            result.push_str("thin ");
                            i = cmd_start + 1;
                        }
                        ';' => {
                            result.push_str("med ");
                            i = cmd_start + 1;
                        }
                        '!' => {
                            // Negative thin space — approximate with nothing.
                            i = cmd_start + 1;
                        }
                        '{' | '}' => {
                            result.push(chars[cmd_start]);
                            i = cmd_start + 1;
                        }
                        _ => {
                            result.push('\\');
                            result.push(chars[cmd_start]);
                            i = cmd_start + 1;
                        }
                    }
                    continue;
                }

                let cmd = &latex[cmd_start..cmd_end];
                i = cmd_end;

                match cmd {
                    // --- Fractions ---
                    "frac" => {
                        let (arg1, next1) = extract_braced_arg(&chars, i);
                        let (arg2, next2) = extract_braced_arg(&chars, next1);
                        result.push_str(&format!(
                            "frac({}, {})",
                            latex_to_typst(&arg1),
                            latex_to_typst(&arg2)
                        ));
                        i = next2;
                    }
                    // --- Square root ---
                    "sqrt" => {
                        let (arg, next) = extract_braced_arg(&chars, i);
                        result.push_str(&format!("sqrt({})", latex_to_typst(&arg)));
                        i = next;
                    }
                    // --- Text mode ---
                    "text" | "mathrm" | "textrm" => {
                        let (arg, next) = extract_braced_arg(&chars, i);
                        result.push_str(&format!("\"{}\"", arg));
                        i = next;
                    }
                    // --- Bold/italic ---
                    "mathbf" | "boldsymbol" | "bm" => {
                        let (arg, next) = extract_braced_arg(&chars, i);
                        result.push_str(&format!("bold({})", latex_to_typst(&arg)));
                        i = next;
                    }
                    "mathit" => {
                        let (arg, next) = extract_braced_arg(&chars, i);
                        // Typst math is italic by default.
                        result.push_str(&latex_to_typst(&arg));
                        i = next;
                    }
                    // --- Environments (begin/end) ---
                    "begin" => {
                        let (env_name, next) = extract_braced_arg(&chars, i);
                        i = next;
                        let (env_body, env_end) = extract_environment(&chars, i, &env_name);
                        i = env_end;
                        result.push_str(&translate_environment(&env_name, &env_body));
                    }
                    "end" => {
                        // Should have been consumed by `begin` handler.
                        // Skip the argument if encountered standalone.
                        let (_, next) = extract_braced_arg(&chars, i);
                        i = next;
                    }
                    // --- Left/right delimiters ---
                    "left" | "right" => {
                        if i < len {
                            let delim = match chars[i] {
                                '(' => "(",
                                ')' => ")",
                                '[' => "[",
                                ']' => "]",
                                '|' => "|",
                                '.' => "",
                                _ => {
                                    result.push(chars[i]);
                                    i += 1;
                                    continue;
                                }
                            };
                            if cmd == "left" {
                                result.push_str(delim);
                            } else {
                                result.push_str(delim);
                            }
                            i += 1;
                        }
                    }
                    // --- Greek letters and symbols ---
                    _ => {
                        if let Some(typst_sym) = translate_symbol(cmd) {
                            // Wrap in spaces to prevent concatenation with adjacent
                            // single-character identifiers (e.g. `i\hbar` → `i planck.reduce`
                            // instead of `iplanck.reduce`).
                            result.push(' ');
                            result.push_str(typst_sym);
                            result.push(' ');
                        } else {
                            tracing::warn!(
                                latex_command = cmd,
                                "Unrecognized LaTeX command, passing through"
                            );
                            result.push_str(cmd);
                        }
                    }
                }
            }
            '{' => {
                result.push('(');
                i += 1;
            }
            '}' => {
                result.push(')');
                i += 1;
            }
            '&' => {
                result.push(',');
                i += 1;
            }
            c => {
                // In Typst math, a sequence of ASCII letters is parsed as a
                // single multi-character identifier (e.g. `ac` → unknown variable
                // `ac`). In LaTeX, adjacent letters represent independent variables
                // with implicit multiplication. Similarly, a digit immediately
                // followed by a letter (e.g. `4ac`) produces `4 * ac` in Typst
                // rather than `4 * a * c`.
                //
                // Invariant: any plain ASCII alpha character emitted here must be
                // preceded by a space if the previous non-space character emitted
                // is also a plain ASCII alpha or a digit, preventing multi-char
                // identifier formation.
                if c.is_ascii_alphabetic() {
                    let last = result.chars().rev().find(|ch| !ch.is_whitespace());
                    if let Some(prev) = last
                        && prev.is_ascii_alphanumeric()
                    {
                        result.push(' ');
                    }
                }
                result.push(c);
                i += 1;
            }
        }
    }

    result
}

/// Translate a known LaTeX symbol/command to Typst syntax.
fn translate_symbol(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        // --- Greek lowercase ---
        "alpha" => "alpha",
        "beta" => "beta",
        "gamma" => "gamma",
        "delta" => "delta",
        "epsilon" | "varepsilon" => "epsilon",
        "zeta" => "zeta",
        "eta" => "eta",
        "theta" | "vartheta" => "theta",
        "iota" => "iota",
        "kappa" => "kappa",
        "lambda" => "lambda",
        "mu" => "mu",
        "nu" => "nu",
        "xi" => "xi",
        "pi" | "varpi" => "pi",
        "rho" | "varrho" => "rho",
        "sigma" | "varsigma" => "sigma",
        "tau" => "tau",
        "upsilon" => "upsilon",
        "phi" | "varphi" => "phi",
        "chi" => "chi",
        "psi" => "psi",
        "omega" => "omega",

        // --- Greek uppercase ---
        "Gamma" => "Gamma",
        "Delta" => "Delta",
        "Theta" => "Theta",
        "Lambda" => "Lambda",
        "Xi" => "Xi",
        "Pi" => "Pi",
        "Sigma" => "Sigma",
        "Upsilon" => "Upsilon",
        "Phi" => "Phi",
        "Psi" => "Psi",
        "Omega" => "Omega",

        // --- Operators ---
        "sum" => "sum",
        "prod" => "product",
        "int" | "integral" => "integral",
        "iint" => "integral.double",
        "iiint" => "integral.triple",
        "oint" => "integral.cont",
        "lim" => "lim",
        "sup" => "sup",
        "inf" => "inf",
        "max" => "max",
        "min" => "min",
        "log" => "log",
        "ln" => "ln",
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "exp" => "exp",
        "det" => "det",
        "dim" => "dim",
        "ker" => "ker",
        "mod" => "mod",
        "gcd" => "gcd",

        // --- Relations ---
        "ne" | "neq" => "eq.not",
        "le" | "leq" => "lt.eq",
        "ge" | "geq" => "gt.eq",
        "approx" => "approx",
        "equiv" => "equiv",
        "sim" => "tilde.op",
        "propto" => "prop",
        "subset" => "subset",
        "supset" => "supset",
        "subseteq" => "subset.eq",
        "supseteq" => "supset.eq",
        "in" => "in",
        "notin" => "in.not",
        "forall" => "forall",
        "exists" => "exists",
        "nexists" => "exists.not",

        // --- Arrows ---
        "to" | "rightarrow" => "arrow.r",
        "leftarrow" => "arrow.l",
        "leftrightarrow" => "arrow.l.r",
        "Rightarrow" => "arrow.r.double",
        "Leftarrow" => "arrow.l.double",
        "Leftrightarrow" => "arrow.l.r.double",
        "mapsto" => "arrow.r.bar",
        "implies" => "==>",
        "iff" => "<==>",

        // --- Misc ---
        "infty" | "infinity" => "infinity",
        "partial" => "diff",
        "nabla" => "nabla",
        "cdot" => "dot.op",
        "cdots" => "dots.h.c",
        "ldots" | "dots" => "dots.h",
        "vdots" => "dots.v",
        "ddots" => "dots.down",
        "times" => "times",
        "div" => "div",
        "pm" => "plus.minus",
        "mp" => "minus.plus",
        "circ" => "compose",
        "star" => "star",
        "dagger" => "dagger",
        "hbar" => "planck.reduce",
        "ell" => "ell",
        "emptyset" => "nothing",
        "quad" => "quad",
        "qquad" => "wide",

        _ => return None,
    })
}

/// Extract a brace-delimited argument `{...}` starting at position `i`.
///
/// Returns `(content, next_position)`. If no brace is found, returns an
/// empty string and the same position.
fn extract_braced_arg(chars: &[char], mut i: usize) -> (String, usize) {
    // Skip whitespace.
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }

    if i >= chars.len() || chars[i] != '{' {
        return (String::new(), i);
    }

    i += 1; // Skip opening `{`.
    let mut depth = 1;
    let start = i;

    while i < chars.len() && depth > 0 {
        match chars[i] {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            i += 1;
        }
    }

    let content: String = chars[start..i].iter().collect();
    if i < chars.len() {
        i += 1; // Skip closing `}`.
    }
    (content, i)
}

/// Extract the body of a LaTeX environment up to `\end{env_name}`.
fn extract_environment(chars: &[char], start: usize, env_name: &str) -> (String, usize) {
    let source: String = chars[start..].iter().collect();
    let end_marker = format!("\\end{{{}}}", env_name);

    if let Some(pos) = source.find(&end_marker) {
        let body = &source[..pos];
        let end_pos = start + pos + end_marker.len();
        (body.to_string(), end_pos)
    } else {
        // No matching \end found — return everything remaining.
        (source, chars.len())
    }
}

/// Translate a LaTeX environment to Typst syntax.
fn translate_environment(env_name: &str, body: &str) -> String {
    match env_name {
        "pmatrix" => {
            let rows = parse_matrix_body(body);
            format_typst_matrix("paren", &rows)
        }
        "bmatrix" => {
            let rows = parse_matrix_body(body);
            format_typst_matrix("bracket", &rows)
        }
        "vmatrix" => {
            let rows = parse_matrix_body(body);
            format_typst_matrix("vert", &rows)
        }
        "matrix" => {
            let rows = parse_matrix_body(body);
            format_typst_matrix("", &rows)
        }
        "aligned" | "align" | "align*" => {
            // Each row is separated by `\\`, columns by `&`.
            let rows: Vec<&str> = body.split("\\\\").collect();
            let mut result = String::new();
            for (i, row) in rows.iter().enumerate() {
                let trimmed = row.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // In aligned, `&` marks alignment points.
                let translated = latex_to_typst(trimmed);
                result.push_str(&translated);
                if i + 1 < rows.len() {
                    result.push_str(" \\ ");
                }
            }
            result
        }
        "cases" => {
            let rows: Vec<&str> = body.split("\\\\").collect();
            let mut result = String::from("cases(");
            for (i, row) in rows.iter().enumerate() {
                let trimmed = row.trim();
                if trimmed.is_empty() {
                    continue;
                }
                result.push_str(&latex_to_typst(trimmed));
                if i + 1 < rows.len() {
                    result.push_str(", ");
                }
            }
            result.push(')');
            result
        }
        _ => {
            tracing::warn!(
                environment = env_name,
                "Unrecognized LaTeX environment, passing through body"
            );
            latex_to_typst(body)
        }
    }
}

/// Parse matrix body into rows of cells (split by `\\` and `&`).
fn parse_matrix_body(body: &str) -> Vec<Vec<String>> {
    body.split("\\\\")
        .filter(|row| !row.trim().is_empty())
        .map(|row| {
            row.split('&')
                .map(|cell| latex_to_typst(cell.trim()))
                .collect()
        })
        .collect()
}

/// Format a Typst matrix with the given delimiter type.
fn format_typst_matrix(delim: &str, rows: &[Vec<String>]) -> String {
    let mut result = String::from("mat(");
    if !delim.is_empty() {
        result = format!("mat(delim: \"{}\", ", delim);
    }

    for (i, row) in rows.iter().enumerate() {
        result.push_str(&row.join(", "));
        if i + 1 < rows.len() {
            result.push_str("; ");
        }
    }

    result.push(')');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_frac() {
        let result = latex_to_typst("\\frac{a}{b}");
        assert_eq!(result, "frac(a, b)");
    }

    #[test]
    fn translates_sqrt() {
        let result = latex_to_typst("\\sqrt{x}");
        assert_eq!(result, "sqrt(x)");
    }

    #[test]
    fn translates_greek() {
        assert_eq!(latex_to_typst("\\alpha"), " alpha ");
        assert_eq!(latex_to_typst("\\beta"), " beta ");
        assert_eq!(latex_to_typst("\\Omega"), " Omega ");
    }

    #[test]
    fn translates_operators() {
        assert_eq!(latex_to_typst("\\sum"), " sum ");
        assert_eq!(latex_to_typst("\\int"), " integral ");
        assert_eq!(latex_to_typst("\\infty"), " infinity ");
    }

    #[test]
    fn translates_relations() {
        assert_eq!(latex_to_typst("\\ne"), " eq.not ");
        assert_eq!(latex_to_typst("\\le"), " lt.eq ");
        assert_eq!(latex_to_typst("\\ge"), " gt.eq ");
    }

    #[test]
    fn translates_subscript_superscript() {
        let result = latex_to_typst("x^2_i");
        assert_eq!(result, "x^2_i");
    }

    #[test]
    fn translates_nested_frac() {
        let result = latex_to_typst("\\frac{\\sqrt{x}}{y}");
        assert_eq!(result, "frac(sqrt(x), y)");
    }

    #[test]
    fn translates_text_command() {
        let result = latex_to_typst("\\text{hello}");
        assert_eq!(result, "\"hello\"");
    }

    #[test]
    fn translates_bold() {
        let result = latex_to_typst("\\mathbf{v}");
        assert_eq!(result, "bold(v)");
    }

    #[test]
    fn passthrough_plain_math() {
        let result = latex_to_typst("x + y = z");
        assert_eq!(result, "x + y = z");
    }

    #[test]
    fn digit_alpha_inserts_space() {
        // `4ac` in LaTeX = 4 * a * c; Typst must see `4 a c`, not `4ac` (unknown ident).
        let result = latex_to_typst("4ac");
        assert_eq!(result, "4 a c");
    }

    #[test]
    fn adjacent_alpha_inserts_space() {
        // `ab` in LaTeX = a * b; Typst must see `a b`, not `ab`.
        let result = latex_to_typst("ab");
        assert_eq!(result, "a b");
    }

    #[test]
    fn superscript_alpha_no_spurious_space() {
        // `x^2_i` — the `i` follows `_`, not alphanumeric, so no space before `i`.
        let result = latex_to_typst("x^2_i");
        assert_eq!(result, "x^2_i");
    }

    #[test]
    fn quadratic_formula_implicit_mul() {
        // `b^2 - 4ac` — the `a` follows space (prev non-space is `4`, a digit),
        // so space injected before `a`; `c` follows `a` alpha, so space before `c`.
        let result = latex_to_typst("b^2 - 4ac");
        assert_eq!(result, "b^2 - 4 a c");
    }
}
