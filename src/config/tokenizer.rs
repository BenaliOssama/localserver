#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Word(String),
    LBrace,
    RBrace,
    Semicolon,
}

pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut line = 1usize;

    for c in input.chars() {
        match c {
            '\n' => {
                push_word(&mut tokens, &mut current, line);
                line += 1;
            }
            '{' => {
                push_word(&mut tokens, &mut current, line);
                tokens.push(Token {
                    kind: TokenKind::LBrace,
                    line,
                });
            }
            '}' => {
                push_word(&mut tokens, &mut current, line);
                tokens.push(Token {
                    kind: TokenKind::RBrace,
                    line,
                });
            }
            ';' => {
                push_word(&mut tokens, &mut current, line);
                tokens.push(Token {
                    kind: TokenKind::Semicolon,
                    line,
                });
            }
            c if c.is_whitespace() => {
                push_word(&mut tokens, &mut current, line);
            }
            _ => current.push(c),
        }
    }
    push_word(&mut tokens, &mut current, line);
    tokens
}

fn push_word(tokens: &mut Vec<Token>, current: &mut String, line: usize) {
    if !current.is_empty() {
        tokens.push(Token {
            kind: TokenKind::Word(current.clone()),
            line,
        });
        current.clear();
    }
}
