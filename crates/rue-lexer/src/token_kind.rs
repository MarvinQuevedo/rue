#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Ident,
    Int,
    String { is_terminated: bool },

    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,

    Fun,
    Type,
    Struct,
    Enum,
    Let,
    Const,
    If,
    Else,
    Return,
    Raise,
    Assert,
    Nil,
    True,
    False,
    As,
    Is,

    Dot,
    Comma,
    Colon,
    PathSeparator,
    Semicolon,
    Arrow,
    FatArrow,
    Spread,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Not,
    LessThan,
    GreaterThan,
    LessThanEquals,
    GreaterThanEquals,
    Equals,
    NotEquals,
    Assign,

    Whitespace,
    LineComment,
    BlockComment { is_terminated: bool },
    Unknown,
}
