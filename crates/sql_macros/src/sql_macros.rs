use proc_macro::{Delimiter, Literal, Span, TokenStream, TokenTree};
use sqlformat::{FormatOptions, QueryParams};
use std::sync::LazyLock;
use syn::Error;

use sql::thread_safe_connection::{self, ConnectionTarget, ThreadSafeConnection};

static SQLITE: LazyLock<ThreadSafeConnection> = LazyLock::new(|| {
    ThreadSafeConnection::new(
        ConnectionTarget::memory(":memory:"),
        None,
        Some(thread_safe_connection::locking_queue()),
    )
});

#[proc_macro]
pub fn sql(tokens: TokenStream) -> TokenStream {
    let (spans, sql) = make_sql(tokens);

    let error = match SQLITE.read(|connection| Ok(connection.sql_has_syntax_error(sql.trim()))) {
        Ok(error) => error,
        Err(error) => Some((format!("{error:#}"), 0)),
    };

    let formatted_sql = sqlformat::format(&sql, &QueryParams::None, &FormatOptions::default());

    if let Some((error, error_offset)) = error {
        create_error(spans, error_offset, &error, &formatted_sql)
    } else {
        TokenStream::from(TokenTree::Literal(Literal::string(sql.trim())))
    }
}

fn create_error(
    spans: Vec<(usize, Span)>,
    error_offset: usize,
    error: &str,
    formatted_sql: &str,
) -> TokenStream {
    let error_span = spans
        .into_iter()
        .skip_while(|(offset, _)| offset <= &error_offset)
        .map(|(_, span)| span)
        .next()
        .unwrap_or_else(Span::call_site);
    let error_text = format!("Sql Error: {error}\nFor Query: {formatted_sql}");
    TokenStream::from(Error::new(error_span.into(), error_text).into_compile_error())
}

fn make_sql(tokens: TokenStream) -> (Vec<(usize, Span)>, String) {
    let mut sql_tokens = Vec::new();
    flatten_stream(tokens, &mut sql_tokens);
    let mut spans = Vec::new();
    let mut sql = String::new();
    for (token_text, span) in sql_tokens {
        sql.push_str(&token_text);
        spans.push((sql.len(), span));
    }
    (spans, sql)
}

fn flatten_stream(tokens: TokenStream, result: &mut Vec<(String, Span)>) {
    for token_tree in tokens {
        match token_tree {
            TokenTree::Group(group) => {
                result.push((open_delimiter(group.delimiter()), group.span()));
                flatten_stream(group.stream(), result);
                result.push((close_delimiter(group.delimiter()), group.span()));
            }
            TokenTree::Ident(ident) => {
                result.push((format!("{ident} "), ident.span()));
            }
            TokenTree::Literal(literal) => {
                result.push((format!("{literal} "), literal.span()));
            }
            TokenTree::Punct(punct) => result.push((punct.to_string(), punct.span())),
        }
    }
}

fn open_delimiter(delimiter: Delimiter) -> String {
    match delimiter {
        Delimiter::Parenthesis => "( ".to_string(),
        Delimiter::Brace => "[ ".to_string(),
        Delimiter::Bracket => "{ ".to_string(),
        Delimiter::None => String::new(),
    }
}

fn close_delimiter(delimiter: Delimiter) -> String {
    match delimiter {
        Delimiter::Parenthesis => " ) ".to_string(),
        Delimiter::Brace => " ] ".to_string(),
        Delimiter::Bracket => " } ".to_string(),
        Delimiter::None => String::new(),
    }
}
