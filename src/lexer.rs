// last updated: may 28, 2026
// Lexer: converts source text into a sequence of tokens
// This is the first stage of the compiler

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Data types
    Int,
    Float,
    Boolean,
    StringType,
    Void,

    // Keywords
    Mut,
    Class,
    Throwable,
    Method, // 'func'
    Constructor,
    Return,
    If,      // if (...) { ...
    Else,    // } else { ... }
    While,
    For,
    Break,
    Continue,
    Public,
    Private,
    Static,
    Uses, // inheritance marker
    Throw,
    Release,
    Zombify, // mark a variable as intentionally leaked
    Unsafe,
    New,
    Use, // import
    Include,
    IncludeNext,
    Returns, // return type marker in function signature
    Super,
    Define,
    Ifdef,
    Ifndef,
    Endif,
    Pragma,
    Delete,

    // Literals
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    True,
    False,
    Null,

    // Identifiers
    Ident(String),

    // Punctuation and operators
    LBrace, RBrace,
    LParen, RParen,
    LBracket, RBracket,
    LAngle, RAngle,
    Semicolon,
    Colon,
    DoubleColon,
    Comma,
    Dot,
    Equals,
    EqEq,
    NotEq,
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp, // single &
    Pipe, // single |
    AmpAmp, // &&
    PipePipe, // ||
    LtEq,
    GtEq,
    Arrow, // ->
    At,
    Ellipsis, // ...

    Newline, // inserted after certain tokens to help parsing
    Eof,
}

// Each token carries its line and column number for error reporting
#[derive(Debug, Clone)]
pub struct TokenWithPos {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

// Convert a source string into a vector of tokens with positions
pub fn tokenize(source: &str) -> Vec<TokenWithPos> {
    let mut tokens = Vec::<TokenWithPos>::new();
    let mut chars = source.chars().peekable();
    let mut line = 1usize;
    let mut col = 1usize;

    while let Some(&c) = chars.peek() {
        match c {
            // Newline handling: insert a Newline token when appropriate
            '\n' => {
                if let Some(prev) = tokens.last() {
                    match &prev.token {
                        Token::Ident(_)
                        | Token::IntLit(_)
                        | Token::FloatLit(_)
                        | Token::StringLit(_)
                        | Token::True
                        | Token::False
                        | Token::Null
                        | Token::RParen
                        | Token::RBracket
                        | Token::RBrace
                        | Token::Return
                        | Token::Break
                        | Token::Continue => {
                            tokens.push(TokenWithPos {
                                token: Token::Newline,
                                line,
                                col,
                            });
                        }
                        _ => {}
                    }
                }
                chars.next();
                line += 1;
                col = 1;
            }

            // Skip whitespace
            ' ' | '\t' | '\r' => {
                chars.next();
                col += 1;
            }

            // Single‑line comment: skip until newline
            '/' if matches!(chars.clone().nth(1), Some('/')) => {
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                }
            }

            // String literal: collect characters until closing quote
            '"' => {
                let start_line = line;
                let start_col = col;
                chars.next();
                col += 1;
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        if let Some(&esc) = chars.peek() {
                            chars.next();
                            match esc {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                '\\' => s.push('\\'),
                                '"' => s.push('"'),
                                _ => {
                                    s.push('\\');
                                    s.push(esc);
                                }
                            }
                        }
                    } else {
                        s.push(c);
                    }
                }
                tokens.push(TokenWithPos {
                    token: Token::StringLit(s),
                    line: start_line,
                    col: start_col,
                });
                col += 1;
            }

            // Numeric literal: integer or float
            '0'..='9' => {
                let start_col = col;
                let mut num = String::new();
                let mut is_float = false;
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() {
                        num.push(c);
                        chars.next();
                        col += 1;
                    } else if c == '.' && !is_float {
                        is_float = true;
                        num.push(c);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let tok = if is_float {
                    Token::FloatLit(num.parse().unwrap_or(0.0))
                } else {
                    Token::IntLit(num.parse().unwrap_or(0))
                };
                tokens.push(TokenWithPos {
                    token: tok,
                    line,
                    col: start_col,
                });
            }

            // Identifier or keyword: alphanumeric + underscore
            'a'..='z' | 'A'..='Z' | '_' => {
                let start_col = col;
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        word.push(c);
                        chars.next();
                        col += 1;
                    } else {
                        break;
                    }
                }
                let tok = match word.as_str() {
                    "int" => Token::Int,
                    "float" => Token::Float,
                    "boolean" => Token::Boolean,
                    "String" => Token::StringType,
                    "void" => Token::Void,
                    "mut" => Token::Mut,
                    "class" => Token::Class,
                    "throwable" => Token::Throwable,
                    "func" => Token::Method,
                    "constructor" => Token::Constructor,
                    "return" => Token::Return,
                    "if" => Token::If,
                    "else" => Token::Else,
                    "while" => Token::While,
                    "for" => Token::For,
                    "break" => Token::Break,
                    "continue" => Token::Continue,
                    "public" => Token::Public,
                    "private" => Token::Private,
                    "static" => Token::Static,
                    "uses" => Token::Uses,
                    "throw" => Token::Throw,
                    "release" => Token::Release,
                    "zombify" => Token::Zombify,
                    "unsafe" => Token::Unsafe,
                    "new" => Token::New,
                    "use" => Token::Use,
                    "include" => Token::Include,
                    "includeNext" => Token::IncludeNext,
                    "returns" => Token::Returns,
                    "super" => Token::Super,
                    "define" => Token::Define,
                    "ifdef" => Token::Ifdef,
                    "ifndef" => Token::Ifndef,
                    "endif" => Token::Endif,
                    "pragma" => Token::Pragma,
                    "delete" => Token::Delete,
                    "true" => Token::True,
                    "false" => Token::False,
                    "null" => Token::Null,
                    "and" => Token::AmpAmp,
                    "or" => Token::PipePipe,
                    "not" => Token::Bang,
                    "is" => Token::EqEq,
                    "isnot" => Token::NotEq,
                    "==" => Token::EqEq,
                    "!=" => Token::NotEq,
                    "&&" => Token::AmpAmp,
                    "||" => Token::PipePipe,
                    "!" => Token::Bang,
                    _ => Token::Ident(word.clone()),
                };
                tokens.push(TokenWithPos {
                    token: tok,
                    line,
                    col: start_col,
                });
            }

            // Single‑character and multi‑character punctuation
            '{' => {
                tokens.push(TokenWithPos { token: Token::LBrace, line, col });
                chars.next();
                col += 1;
            }
            '}' => {
                tokens.push(TokenWithPos { token: Token::RBrace, line, col });
                chars.next();
                col += 1;
            }
            '(' => {
                tokens.push(TokenWithPos { token: Token::LParen, line, col });
                chars.next();
                col += 1;
            }
            ')' => {
                tokens.push(TokenWithPos { token: Token::RParen, line, col });
                chars.next();
                col += 1;
            }
            '[' => {
                tokens.push(TokenWithPos { token: Token::LBracket, line, col });
                chars.next();
                col += 1;
            }
            ']' => {
                tokens.push(TokenWithPos { token: Token::RBracket, line, col });
                chars.next();
                col += 1;
            }
            '<' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::LtEq, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::LAngle, line, col });
                }
            }
            '>' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::GtEq, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::RAngle, line, col });
                }
            }
            ';' => {
                tokens.push(TokenWithPos { token: Token::Semicolon, line, col });
                chars.next();
                col += 1;
            }
            ',' => {
                tokens.push(TokenWithPos { token: Token::Comma, line, col });
                chars.next();
                col += 1;
            }
            '.' => {
                let start_col = col;
                chars.next();
                col += 1;
                // Detect ellipsis
                if chars.peek() == Some(&'.') {
                    chars.next();
                    col += 1;
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        col += 1;
                        tokens.push(TokenWithPos { token: Token::Ellipsis, line, col: start_col });
                    } else {
                        // Two dots: treat as two separate dot tokens (rare)
                        tokens.push(TokenWithPos { token: Token::Dot, line, col: start_col });
                        tokens.push(TokenWithPos { token: Token::Dot, line, col: start_col + 1 });
                    }
                } else {
                    tokens.push(TokenWithPos { token: Token::Dot, line, col: start_col });
                }
            }
            '+' => {
                tokens.push(TokenWithPos { token: Token::Plus, line, col });
                chars.next();
                col += 1;
            }
            '%' => {
                tokens.push(TokenWithPos { token: Token::Percent, line, col });
                chars.next();
                col += 1;
            }
            '@' => {
                tokens.push(TokenWithPos { token: Token::At, line, col });
                chars.next();
                col += 1;
            }
            '*' => {
                tokens.push(TokenWithPos { token: Token::Star, line, col });
                chars.next();
                col += 1;
            }
            '-' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'>') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::Arrow, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::Minus, line, col });
                }
            }
            '=' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::EqEq, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::Equals, line, col });
                }
            }
            '!' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::NotEq, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::Bang, line, col });
                }
            }
            '&' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'&') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::AmpAmp, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::Amp, line, col });
                }
            }
            '|' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&'|') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::PipePipe, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::Pipe, line, col });
                }
            }
            ':' => {
                chars.next();
                col += 1;
                if chars.peek() == Some(&':') {
                    chars.next();
                    col += 1;
                    tokens.push(TokenWithPos { token: Token::DoubleColon, line, col });
                } else {
                    tokens.push(TokenWithPos { token: Token::Colon, line, col });
                }
            }
            '/' => {
                chars.next();
                col += 1;
                tokens.push(TokenWithPos { token: Token::Slash, line, col });
            }

            // Any other character is ignored (could be extended for error reporting)
            _ => {
                chars.next();
                col += 1;
            }
        }
    }

    // End of file token
    tokens.push(TokenWithPos {
        token: Token::Eof,
        line,
        col,
    });
    tokens
}