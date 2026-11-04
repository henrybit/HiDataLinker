/// Preview requests are capped here. The engines already clamp `preview_table` to 1_000.
pub const PREVIEW_ROW_CAP: u32 = 1_000;

const MAX_SQL_CHARS: usize = 100_000;

/// Authorization mode for agent SQL tools.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlAuthMode {
    /// SELECT / WITH…SELECT / EXPLAIN / SHOW / DESCRIBE / TABLE / VALUES only.
    ReadOnly,
    /// Allows DML/DDL; still blocks multi-statements and file/OS side-channel functions.
    Full,
}

impl SqlAuthMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "full" => Self::Full,
            _ => Self::ReadOnly,
        }
    }
}

/// Validate SQL for the agent `query` tool under the given authorization mode.
pub fn assert_agent_sql(sql: &str, mode: SqlAuthMode) -> Result<String, String> {
    match mode {
        SqlAuthMode::ReadOnly => assert_read_only_sql(sql),
        SqlAuthMode::Full => assert_full_auth_sql(sql),
    }
}

/// Keep a single read-only statement. Writes, multi-statements, and file/admin functions are rejected.
pub fn assert_read_only_sql(sql: &str) -> Result<String, String> {
    let statement = normalize_single_statement(sql)?;
    let tokens = tokenize(&statement)?;
    let opening = opening_keyword(&tokens).ok_or_else(|| {
        "statement must start with SELECT, WITH, EXPLAIN, SHOW, DESCRIBE, DESC, TABLE, or VALUES"
            .to_string()
    })?;
    if !is_allowed_opening(&opening) {
        return Err(format!(
            "only read-only statements are allowed, not {opening}"
        ));
    }
    if opening.eq_ignore_ascii_case("WITH") {
        let verb_at = with_main_verb(&tokens)?;
        let verb = match &tokens[verb_at] {
            Token::Ident(text) => text.clone(),
            _ => return Err("WITH query must end in SELECT, TABLE, or VALUES".into()),
        };
        if !matches!(
            verb.to_ascii_uppercase().as_str(),
            "SELECT" | "TABLE" | "VALUES"
        ) {
            return Err(format!(
                "WITH query must end in SELECT, TABLE, or VALUES, not {verb}"
            ));
        }
    }
    assert_no_writes(&tokens, &opening)?;
    Ok(statement)
}

/// Full authorization: one statement, no file/OS side channels.
pub fn assert_full_auth_sql(sql: &str) -> Result<String, String> {
    let statement = normalize_single_statement(sql)?;
    let tokens = tokenize(&statement)?;
    if opening_keyword(&tokens).is_none() {
        return Err("statement is empty".into());
    }
    assert_no_dangerous_side_channels(&tokens)?;
    Ok(statement)
}

fn normalize_single_statement(sql: &str) -> Result<String, String> {
    if sql.contains('\0') {
        return Err("SQL contains a null byte".into());
    }
    if sql.chars().count() > MAX_SQL_CHARS {
        return Err("SQL is too long".into());
    }
    let statements = split_statements(sql)?;
    if statements.len() != 1 {
        return Err("only one SQL statement is allowed".into());
    }
    Ok(statements.into_iter().next().unwrap())
}

pub fn clamp_preview_limit(limit: u32) -> u32 {
    limit.clamp(1, PREVIEW_ROW_CAP)
}

fn is_allowed_opening(keyword: &str) -> bool {
    matches!(
        keyword.to_ascii_uppercase().as_str(),
        "SELECT" | "WITH" | "EXPLAIN" | "SHOW" | "DESCRIBE" | "DESC" | "TABLE" | "VALUES"
    )
}

fn denied(keyword: &str) -> bool {
    matches!(
        keyword,
        "INSERT"
            | "UPDATE"
            | "DELETE"
            | "DROP"
            | "ALTER"
            | "TRUNCATE"
            | "GRANT"
            | "REVOKE"
            | "MERGE"
            | "CALL"
            | "EXECUTE"
            | "EXEC"
            | "COPY"
            | "LOAD"
            | "ATTACH"
            | "DETACH"
            | "PRAGMA"
            | "VACUUM"
            | "SET"
            | "USE"
            | "DO"
            | "HANDLER"
            | "RENAME"
            | "LOCK"
            | "OUTFILE"
            | "DUMPFILE"
            | "INTO"
            | "BEGIN"
            | "COMMIT"
            | "ROLLBACK"
            | "SAVEPOINT"
            | "PREPARE"
            | "DEALLOCATE"
            | "KILL"
            | "SHUTDOWN"
            | "PURGE"
            | "UPSERT"
            | "CREATE"
            | "REPLACE"
            | "PG_READ_FILE"
            | "PG_READ_BINARY_FILE"
            | "PG_LS_DIR"
            | "PG_STAT_FILE"
            | "LO_IMPORT"
            | "LO_EXPORT"
            | "LOAD_FILE"
            | "DBLINK"
            | "DBLINK_EXEC"
    )
}

fn assert_no_writes(tokens: &[Token], opening: &str) -> Result<(), String> {
    for (index, token) in tokens.iter().enumerate() {
        let Token::Ident(name) = token else {
            continue;
        };
        let upper = name.to_ascii_uppercase();
        if upper == "REPLACE" && matches!(tokens.get(index + 1), Some(Token::LParen)) {
            continue;
        }
        if upper == "CREATE" && opening.eq_ignore_ascii_case("SHOW") {
            continue;
        }
        if denied(&upper) {
            return Err(format!("read-only queries cannot use {upper}"));
        }
    }
    Ok(())
}

fn is_dangerous_side_channel(keyword: &str) -> bool {
    matches!(
        keyword,
        "OUTFILE"
            | "DUMPFILE"
            | "PG_READ_FILE"
            | "PG_READ_BINARY_FILE"
            | "PG_LS_DIR"
            | "PG_STAT_FILE"
            | "LO_IMPORT"
            | "LO_EXPORT"
            | "LOAD_FILE"
            | "DBLINK"
            | "DBLINK_EXEC"
            | "SHUTDOWN"
            | "KILL"
    )
}

fn assert_no_dangerous_side_channels(tokens: &[Token]) -> Result<(), String> {
    for token in tokens {
        let Token::Ident(name) = token else {
            continue;
        };
        let upper = name.to_ascii_uppercase();
        if is_dangerous_side_channel(&upper) {
            return Err(format!("SQL cannot use {upper} even in full authorization mode"));
        }
    }
    Ok(())
}

fn opening_keyword(tokens: &[Token]) -> Option<String> {
    tokens.iter().find_map(|token| match token {
        Token::Ident(text) => Some(text.clone()),
        _ => None,
    })
}

fn with_main_verb(tokens: &[Token]) -> Result<usize, String> {
    let start = tokens
        .iter()
        .position(|token| matches!(token, Token::Ident(_)))
        .ok_or_else(|| "WITH clause is empty".to_string())?;
    let mut index = start + 1;
    if ident_eq(tokens, index, "RECURSIVE") {
        index += 1;
    }
    loop {
        if !is_ident(tokens, index) {
            return Err("WITH clause is missing a name".into());
        }
        index += 1;
        if is_lparen(tokens, index) {
            let after = skip_balanced(tokens, index)?;
            if ident_eq(tokens, after, "AS") {
                index = after;
            }
        }
        if ident_eq(tokens, index, "AS") {
            index += 1;
        } else {
            return Err("WITH clause is missing AS".into());
        }
        if ident_eq(tokens, index, "NOT") {
            index += 1;
            if ident_eq(tokens, index, "MATERIALIZED") {
                index += 1;
            }
        } else if ident_eq(tokens, index, "MATERIALIZED") {
            index += 1;
        }
        if !is_lparen(tokens, index) {
            return Err("WITH clause is missing a subquery".into());
        }
        index = skip_balanced(tokens, index)?;
        if is_comma(tokens, index) {
            index += 1;
            continue;
        }
        break;
    }
    if !is_ident(tokens, index) {
        return Err("WITH query must end in SELECT, TABLE, or VALUES".into());
    }
    Ok(index)
}

fn skip_balanced(tokens: &[Token], start: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    for (offset, token) in tokens.iter().enumerate().skip(start) {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(offset + 1);
                }
            }
            _ => {}
        }
    }
    Err("unbalanced parentheses".into())
}

fn is_ident(tokens: &[Token], index: usize) -> bool {
    matches!(tokens.get(index), Some(Token::Ident(_)))
}

fn is_lparen(tokens: &[Token], index: usize) -> bool {
    matches!(tokens.get(index), Some(Token::LParen))
}

fn is_comma(tokens: &[Token], index: usize) -> bool {
    matches!(tokens.get(index), Some(Token::Comma))
}

fn ident_eq(tokens: &[Token], index: usize, expected: &str) -> bool {
    match tokens.get(index) {
        Some(Token::Ident(text)) => text.eq_ignore_ascii_case(expected),
        _ => false,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    LParen,
    RParen,
    Comma,
}

fn tokenize(sql: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        if ch == '-' && chars.get(index + 1) == Some(&'-') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if ch == '#' {
            index += 1;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            let mut closed = false;
            while index + 1 < chars.len() {
                if chars[index] == '*' && chars[index + 1] == '/' {
                    index += 2;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err("unterminated comment".into());
            }
            continue;
        }
        if ch == '\'' {
            index = skip_quoted(&chars, index, '\'')?;
            continue;
        }
        if ch == '"' || ch == '`' {
            index = skip_quoted(&chars, index, ch)?;
            continue;
        }
        if ch == '[' {
            index = skip_bracket(&chars, index)?;
            continue;
        }
        if ch == '$' {
            if let Some(next) = skip_dollar(&chars, index)? {
                index = next;
                continue;
            }
        }
        if ch == '(' {
            tokens.push(Token::LParen);
            index += 1;
            continue;
        }
        if ch == ')' {
            tokens.push(Token::RParen);
            index += 1;
            continue;
        }
        if ch == ',' {
            tokens.push(Token::Comma);
            index += 1;
            continue;
        }
        if is_ident_start(ch) {
            let start = index;
            index += 1;
            while index < chars.len() && is_ident_continue(chars[index]) {
                index += 1;
            }
            tokens.push(Token::Ident(chars[start..index].iter().collect()));
            continue;
        }
        index += 1;
    }
    Ok(tokens)
}

fn skip_quoted(chars: &[char], start: usize, quote: char) -> Result<usize, String> {
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return Ok(index + 1);
        }
        index += 1;
    }
    Err("unterminated string".into())
}

fn skip_bracket(chars: &[char], start: usize) -> Result<usize, String> {
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == ']' {
            if chars.get(index + 1) == Some(&']') {
                index += 2;
                continue;
            }
            return Ok(index + 1);
        }
        index += 1;
    }
    Err("unterminated identifier".into())
}

fn skip_dollar(chars: &[char], start: usize) -> Result<Option<usize>, String> {
    let mut tag_end = start + 1;
    while tag_end < chars.len() && (chars[tag_end].is_ascii_alphanumeric() || chars[tag_end] == '_')
    {
        tag_end += 1;
    }
    if chars.get(tag_end) != Some(&'$') {
        return Ok(None);
    }
    let tag: String = chars[start..=tag_end].iter().collect();
    let mut index = tag_end + 1;
    let tag_chars: Vec<char> = tag.chars().collect();
    while index + tag_chars.len() <= chars.len() {
        if chars[index..index + tag_chars.len()] == tag_chars[..] {
            return Ok(Some(index + tag_chars.len()));
        }
        index += 1;
    }
    Err("unterminated dollar quote".into())
}

fn split_statements(sql: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut index = 0;
    let mut saw_token = false;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '-' && chars.get(index + 1) == Some(&'-') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            current.push(' ');
            continue;
        }
        if ch == '#' {
            index += 1;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            current.push(' ');
            continue;
        }
        if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            let mut closed = false;
            while index + 1 < chars.len() {
                if chars[index] == '*' && chars[index + 1] == '/' {
                    index += 2;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err("unterminated comment".into());
            }
            current.push(' ');
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '`' {
            let end = skip_quoted(&chars, index, ch)?;
            current.extend(chars[index..end].iter());
            saw_token = true;
            index = end;
            continue;
        }
        if ch == '[' {
            let end = skip_bracket(&chars, index)?;
            current.extend(chars[index..end].iter());
            saw_token = true;
            index = end;
            continue;
        }
        if ch == '$' {
            if let Some(end) = skip_dollar(&chars, index)? {
                current.extend(chars[index..end].iter());
                saw_token = true;
                index = end;
                continue;
            }
        }
        if ch == ';' {
            if saw_token {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                }
            }
            current.clear();
            saw_token = false;
            index += 1;
            continue;
        }
        current.push(ch);
        if !ch.is_whitespace() {
            saw_token = true;
        }
        index += 1;
    }
    if saw_token {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            statements.push(trimmed.to_string());
        }
    }
    Ok(statements)
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(sql: &str) {
        assert_read_only_sql(sql).unwrap_or_else(|error| panic!("{sql}: {error}"));
    }

    fn deny(sql: &str) {
        assert!(
            assert_read_only_sql(sql).is_err(),
            "expected rejection for {sql}"
        );
    }

    #[test]
    fn allows_a_single_select() {
        allow("SELECT id FROM orders WHERE status = 'open'");
        allow("  select 1;  ");
        allow("SELECT ';' AS marker");
        allow("/* note */ SELECT 1");
        allow("SELECT 1; -- trailing note");
        allow("WITH unpaid AS (SELECT * FROM orders) SELECT * FROM unpaid");
        allow("WITH RECURSIVE c AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM c WHERE n < 3) SELECT * FROM c");
        allow("EXPLAIN SELECT * FROM orders");
        allow("EXPLAIN ANALYZE SELECT * FROM orders");
        allow("SHOW CREATE TABLE orders");
        allow("DESCRIBE orders");
        allow("SELECT REPLACE(name, 'a', 'b') FROM orders");
        allow("TABLE orders");
        allow("VALUES (1)");
    }

    #[test]
    fn rejects_writes_and_batches() {
        deny("");
        deny("DELETE FROM orders");
        deny("SELECT 1; DELETE FROM orders");
        deny("SELECT 1; /* hidden */ DELETE FROM orders");
        deny("WITH gone AS (DELETE FROM orders RETURNING *) SELECT * FROM gone");
        deny("WITH unpaid AS (SELECT 1) DELETE FROM orders");
        deny("SELECT * FROM orders INTO OUTFILE '/tmp/x'");
        deny("SELECT * FROM orders FOR UPDATE");
        deny("REPLACE INTO orders VALUES (1)");
        deny("SELECT pg_read_file('/etc/passwd')");
        deny("SELECT LOAD_FILE('/etc/passwd')");
        deny("UPDATE orders SET paid = 1");
        deny("SHOW DATABASES; DROP DATABASE app");
        deny("SELECT 'unterminated");
    }

    #[test]
    fn caps_preview_limit() {
        assert_eq!(clamp_preview_limit(0), 1);
        assert_eq!(clamp_preview_limit(100), 100);
        assert_eq!(clamp_preview_limit(50_000), PREVIEW_ROW_CAP);
    }

    #[test]
    fn full_mode_allows_writes_but_blocks_side_channels() {
        assert_agent_sql("UPDATE orders SET paid = 1", SqlAuthMode::Full).unwrap();
        assert_agent_sql("INSERT INTO orders VALUES (1)", SqlAuthMode::Full).unwrap();
        assert_agent_sql("DELETE FROM orders WHERE id = 1", SqlAuthMode::Full).unwrap();
        assert_agent_sql("CREATE TABLE t (id INT)", SqlAuthMode::Full).unwrap();
        assert!(assert_agent_sql("UPDATE orders SET paid = 1", SqlAuthMode::ReadOnly).is_err());
        assert!(assert_agent_sql(
            "SELECT pg_read_file('/etc/passwd')",
            SqlAuthMode::Full
        )
        .is_err());
        assert!(assert_agent_sql(
            "SELECT 1; DELETE FROM orders",
            SqlAuthMode::Full
        )
        .is_err());
    }
}
