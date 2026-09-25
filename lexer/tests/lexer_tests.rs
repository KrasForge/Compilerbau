use c1::lexer::Token::{self, *};
use logos::Logos;

#[test]
fn empty_stream() {
    let token = Token::lexer("").next();
    assert_eq!(token, None);
}

#[test]
fn bool_literals() {
    for (input, output) in [("true", BoolLiteral(true)), ("false", BoolLiteral(false))] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn float_literals() {
    for (input, output) in [
        ("1.0", FloatLiteral(1.0)),
        (".01", FloatLiteral(0.01)),
        ("3.1e-12", FloatLiteral(3.1e-12)),
        ("2E3", FloatLiteral(2E3)),
    ] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn int_literals() {
    for (input, output) in [("13", IntLiteral(13)), ("001", IntLiteral(001))] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn string_literals() {
    for (input, output) in [
        ("\"\"", StringLiteral(String::new())),
        ("\"Hallo\"", StringLiteral("Hallo".to_owned())),
    ] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn identifiers() {
    for (input, output) in [
        ("Lexer", Ident("Lexer".to_owned())),
        ("is_power_of_two", Ident("is_power_of_two".to_owned())),
        ("LOG_2", Ident("LOG_2".to_owned())),
        ("_unused_ident", Ident("_unused_ident".to_owned())),
    ] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn keywords() {
    for (input, output) in [
        ("bool", KwBool),
        ("do", KwDo),
        ("else", KwElse),
        ("for", KwFor),
        ("float", KwFloat),
        ("if", KwIf),
        ("int", KwInt),
        ("print", KwPrint),
        ("return", KwReturn),
        ("void", KwVoid),
        ("while", KwWhile),
    ] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn operators() {
    for (input, output) in [
        ("+", Add),
        ("-", Sub),
        ("*", Mul),
        ("/", Div),
        ("=", Assign),
        ("==", Eq),
        ("!=", Neq),
        ("<", Lt),
        (">", Gt),
        ("<=", Leq),
        (">=", Geq),
        ("&&", LogAnd),
        ("||", LogOr),
    ] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn separators() {
    for (input, output) in [
        (";", Semicolon),
        (",", Comma),
        ("(", LParen),
        (")", RParen),
        ("{", LBrace),
        ("}", RBrace),
    ] {
        let token = Token::lexer(input).next();
        assert_eq!(token, Some(Ok(output)));
    }
}

#[test]
fn unclosed_c_comment() {
    let mut lexer = Token::lexer("/*");
    assert_eq!(lexer.next(), Some(Ok(Token::Div)));
    assert_eq!(lexer.next(), Some(Ok(Token::Mul)));
}

#[test]
fn illegal_character() {
    let mut lexer = Token::lexer("100%");
    assert_eq!(lexer.next(), Some(Ok(Token::IntLiteral(100))));
    assert_eq!(lexer.next(), Some(Err(())));
}

#[test]
fn overflow() {
    let token = Token::lexer("9223372036854775808").next();
    assert_eq!(token, Some(Err(())));
}

// `-1` should be parsed as two tokens.
#[test]
fn neg_int_two_tokens() {
    let mut lexer = Token::lexer("-1");
    assert_eq!(lexer.next(), Some(Ok(Token::Sub)));
    assert_eq!(lexer.next(), Some(Ok(Token::IntLiteral(1))));
    assert_eq!(lexer.next(), None);
}

// `e0` should be an ident and not a float literal.
#[test]
fn e0_ident() {
    let token = Token::lexer("e0").next();
    assert_eq!(token, Some(Ok(Ident("e0".to_owned()))));
}

// ---------------------------------------------------------------------------
// additional tests
// ---------------------------------------------------------------------------

/// Lexes the whole input, turning errors into `None`.
fn lex_all(input: &str) -> Vec<Option<Token>> {
    Token::lexer(input).map(Result::ok).collect()
}

fn ident(name: &str) -> Option<Token> {
    Some(Ident(name.to_owned()))
}

fn string(value: &str) -> Option<Token> {
    Some(StringLiteral(value.to_owned()))
}

#[test]
fn more_float_literals() {
    for (input, output) in [
        ("1e10", FloatLiteral(1e10)),
        ("1.5E+3", FloatLiteral(1.5E+3)),
        (".5e-2", FloatLiteral(0.5e-2)),
        ("007.25", FloatLiteral(7.25)),
    ] {
        assert_eq!(lex_all(input), [Some(output)], "input: {input}");
    }
}

// `1.` is no float literal: the `.` must not be the last character.
#[test]
fn trailing_dot_is_no_float() {
    assert_eq!(lex_all("1."), [Some(IntLiteral(1)), None]);
}

// `2.0e-1.4` is no single float literal: the exponent must be an integer.
#[test]
fn fractional_exponent_is_no_float() {
    assert_eq!(
        lex_all("2.0e-1.4"),
        [Some(FloatLiteral(2.0e-1)), Some(FloatLiteral(0.4))]
    );
}

// Hex and binary literals are not supported.
#[test]
fn no_hex_or_binary_literals() {
    assert_eq!(lex_all("0x12"), [Some(IntLiteral(0)), ident("x12")]);
    assert_eq!(lex_all("0b100"), [Some(IntLiteral(0)), ident("b100")]);
}

// Identifiers must not start with a digit or contain `-`.
#[test]
fn invalid_identifiers() {
    assert_eq!(lex_all("123foo"), [Some(IntLiteral(123)), ident("foo")]);
    assert_eq!(
        lex_all("is-invalid"),
        [ident("is"), Some(Sub), ident("invalid")]
    );
}

// Keywords are only recognized as whole words.
#[test]
fn keyword_prefixes_are_identifiers() {
    for input in ["iff", "integer", "do_", "while2", "truex", "falsey", "Int"] {
        assert_eq!(lex_all(input), [ident(input)], "input: {input}");
    }
}

// `""Laser""` is no single string literal, as quotes can't be nested.
#[test]
fn string_literal_no_nested_quotes() {
    assert_eq!(
        lex_all("\"\"Laser\"\""),
        [string(""), ident("Laser"), string("")]
    );
}

#[test]
fn string_literal_keeps_content() {
    assert_eq!(
        lex_all("\"int x = 1; // /* */\""),
        [string("int x = 1; // /* */")]
    );
}

// String literals must not contain line breaks.
#[test]
fn string_literal_no_line_break() {
    assert_eq!(lex_all("\"a\nb\"").first(), Some(&None));
    assert_eq!(lex_all("\"a\r\nb\"").first(), Some(&None));
}

#[test]
fn unterminated_string_literal() {
    assert_eq!(lex_all("\"abc").first(), Some(&None));
}

#[test]
fn block_comments_are_skipped() {
    for input in ["/* */", "/***/", "/**/", "/* a ** b / c */", "/*\nint\n*/"] {
        assert_eq!(lex_all(input), [], "input: {input:?}");
    }
    assert_eq!(
        lex_all("int/* comment */x"),
        [Some(KwInt), ident("x")]
    );
}

// Block comments don't nest: the comment ends at the first `*/`.
#[test]
fn block_comments_do_not_nest() {
    assert_eq!(lex_all("/*/* */*/"), [Some(Mul), Some(Div)]);
}

#[test]
fn block_comment_ends_at_first_terminator() {
    assert_eq!(lex_all("/* a */ b /* c */"), [ident("b")]);
}

#[test]
fn line_comments_are_skipped() {
    for input in ["//", "//*", "// int x = 1;", "//* not a block comment"] {
        assert_eq!(lex_all(input), [], "input: {input:?}");
    }
    assert_eq!(lex_all("// comment\nint"), [Some(KwInt)]);
    assert_eq!(lex_all("// comment\r\nint"), [Some(KwInt)]);
    assert_eq!(lex_all("//* comment\n*/"), [Some(Mul), Some(Div)]);
}

#[test]
fn whitespace_is_skipped() {
    assert_eq!(
        lex_all(" \t\r\n int \r\n\t x \n"),
        [Some(KwInt), ident("x")]
    );
}

#[test]
fn max_int_literal() {
    assert_eq!(
        lex_all("9223372036854775807"),
        [Some(IntLiteral(i64::MAX))]
    );
}

// Operators are lexed greedily, e.g. `<=` is one token, not `<` `=`.
#[test]
fn operators_longest_match() {
    assert_eq!(
        lex_all("a<=b==c!=d>=e"),
        [
            ident("a"), Some(Leq), ident("b"), Some(Eq), ident("c"),
            Some(Neq), ident("d"), Some(Geq), ident("e"),
        ]
    );
    assert_eq!(lex_all("==="), [Some(Eq), Some(Assign)]);
}

// Single `&`, `|` and `!` are no C1 tokens.
#[test]
fn incomplete_operators_are_errors() {
    assert_eq!(lex_all("a & b"), [ident("a"), None, ident("b")]);
    assert_eq!(lex_all("a | b"), [ident("a"), None, ident("b")]);
    assert_eq!(lex_all("!a"), [None, ident("a")]);
}

#[test]
fn unknown_characters_are_errors() {
    for input in ["#", "%", "^", "[", "]", "'", "~", "@", "$", "?", ":", "ä"] {
        assert_eq!(lex_all(input), [None], "input: {input}");
    }
}

// The `Lexer` wrapper reports the offending text and its span.
#[test]
fn lexical_error_has_text_and_span() {
    use c1::lexer::{LexicalError, Lexer};

    let tokens: Vec<_> = Lexer::new("int x = 1 # 2;").collect();
    assert_eq!(
        tokens[4],
        Err(LexicalError {
            text: "#".to_owned(),
            span: 10..11,
        })
    );
    assert_eq!(tokens[5], Ok((12, IntLiteral(2), 13)));
}

#[test]
fn small_program() {
    let input = r#"
        // computes the answer
        int main() {
            float f = 4.2e1; /* not used */
            if (f >= 42.0 && true) print("yes");
            return 42;
        }
    "#;
    assert_eq!(
        lex_all(input),
        [
            Some(KwInt), ident("main"), Some(LParen), Some(RParen), Some(LBrace),
            Some(KwFloat), ident("f"), Some(Assign), Some(FloatLiteral(42.0)), Some(Semicolon),
            Some(KwIf), Some(LParen), ident("f"), Some(Geq), Some(FloatLiteral(42.0)),
            Some(LogAnd), Some(BoolLiteral(true)), Some(RParen),
            Some(KwPrint), Some(LParen), string("yes"), Some(RParen), Some(Semicolon),
            Some(KwReturn), Some(IntLiteral(42)), Some(Semicolon),
            Some(RBrace),
        ]
    );
}
