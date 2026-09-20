pub const DEFAULT_QUERY_ROW_LIMIT: u32 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlDialect {
    MySql,
    Postgres,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlLimitRewrite {
    pub sql: String,
    pub applied: bool,
}

pub fn statement_kind(sql: &str) -> &'static str {
    let trimmed = sql.trim_start();
    let first = trimmed.split_whitespace().next().unwrap_or_default();
    match first.to_ascii_uppercase().as_str() {
        "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "WITH" | "TABLE" | "VALUES" => {
            "query"
        }
        "INSERT" => "insert",
        "UPDATE" => "update",
        "DELETE" => "delete",
        "CREATE" => "create",
        "ALTER" => "alter",
        "DROP" => "drop",
        "TRUNCATE" => "truncate",
        "USE" | "SET" | "RESET" => "session",
        "COPY" => "copy",
        _ => "other",
    }
}

/// If a result-returning statement has no top-level LIMIT/FETCH, insert a default LIMIT
/// so the database stops producing rows before the client buffers them.
pub fn apply_default_query_limit(sql: &str, dialect: SqlDialect) -> SqlLimitRewrite {
    apply_default_query_limit_with(sql, dialect, DEFAULT_QUERY_ROW_LIMIT)
}

fn apply_default_query_limit_with(sql: &str, dialect: SqlDialect, limit: u32) -> SqlLimitRewrite {
    if sql.trim().is_empty() || limit == 0 {
        return SqlLimitRewrite {
            sql: sql.to_string(),
            applied: false,
        };
    }

    let mut out = String::with_capacity(sql.len() + 16);
    let mut applied = false;
    let mut start = 0;

    while start < sql.len() {
        let end = next_statement_end(sql, start, dialect);
        let statement = &sql[start..end];
        if should_apply_default_limit(statement, dialect) {
            out.push_str(&insert_limit(statement, limit, dialect));
            applied = true;
        } else {
            out.push_str(statement);
        }
        start = end;
    }

    SqlLimitRewrite { sql: out, applied }
}

fn should_apply_default_limit(sql: &str, dialect: SqlDialect) -> bool {
    is_result_query(sql, dialect) && !has_top_level_row_limit(sql, dialect)
}

fn is_result_query(sql: &str, dialect: SqlDialect) -> bool {
    let mut cur = Cursor::new(sql, dialect);
    cur.skip_ws_and_comments();
    let Some(keyword) = cur.peek_keyword() else {
        return false;
    };
    if keyword_is(&keyword, &["EXPLAIN", "SHOW", "DESCRIBE", "DESC"]) {
        return false;
    }
    if keyword.eq_ignore_ascii_case("WITH") {
        if !skip_with_clause(&mut cur) {
            return false;
        }
        cur.skip_ws_and_comments();
        let Some(next) = cur.peek_keyword() else {
            return false;
        };
        return keyword_is(&next, &["SELECT", "TABLE", "VALUES"]);
    }
    keyword_is(&keyword, &["SELECT", "TABLE", "VALUES"])
}

fn has_top_level_row_limit(sql: &str, dialect: SqlDialect) -> bool {
    let mut cur = Cursor::new(sql, dialect);
    loop {
        cur.skip_ws_and_comments();
        if cur.eof() || cur.peek() == Some(';') {
            return false;
        }
        match cur.peek() {
            Some('(') => {
                if !cur.skip_balanced_paren() {
                    return false;
                }
            }
            Some('\'') | Some('"') | Some('`') => {
                if !cur.skip_quoted() {
                    return false;
                }
            }
            Some('$') if dialect == SqlDialect::Postgres => {
                if !cur.skip_dollar_quote_or_char() {
                    return false;
                }
            }
            Some(ch) if is_ident_start(ch) => {
                let start = cur.i;
                cur.skip_ident();
                let keyword = &sql[start..cur.i];
                if keyword.eq_ignore_ascii_case("LIMIT") && looks_like_limit_clause(&mut cur) {
                    return true;
                }
                if keyword.eq_ignore_ascii_case("FETCH") && looks_like_fetch_clause(&mut cur) {
                    return true;
                }
            }
            Some(_) => {
                cur.bump();
            }
            None => return false,
        }
    }
}

fn looks_like_limit_clause(cur: &mut Cursor<'_>) -> bool {
    let saved = cur.i;
    cur.skip_ws_and_comments();
    let ok = match cur.peek() {
        Some('(') | Some('$') => true,
        Some(ch) if ch.is_ascii_digit() => true,
        Some(_) => {
            let Some(next) = cur.peek_keyword() else {
                cur.i = saved;
                return false;
            };
            next.eq_ignore_ascii_case("ALL")
        }
        None => false,
    };
    cur.i = saved;
    ok
}

fn looks_like_fetch_clause(cur: &mut Cursor<'_>) -> bool {
    let saved = cur.i;
    cur.skip_ws_and_comments();
    let ok = cur
        .peek_keyword()
        .is_some_and(|keyword| keyword_is(&keyword, &["FIRST", "NEXT"]));
    cur.i = saved;
    ok
}

fn insert_limit(sql: &str, limit: u32, dialect: SqlDialect) -> String {
    let idx = insertion_index(sql, dialect).unwrap_or(sql.len());
    let prefix = sql[..idx].trim_end();
    let suffix = sql[idx..].trim_start();
    if suffix.is_empty() {
        format!("{prefix} LIMIT {limit}")
    } else if suffix.starts_with(';') {
        format!("{prefix} LIMIT {limit}{suffix}")
    } else {
        format!("{prefix} LIMIT {limit} {suffix}")
    }
}

fn insertion_index(sql: &str, dialect: SqlDialect) -> Option<usize> {
    let mut cur = Cursor::new(sql, dialect);
    let mut last_token_end = None;
    let mut offset_start = None;
    let mut lock_start = None;

    loop {
        cur.skip_ws_and_comments();
        if cur.eof() {
            break;
        }
        if cur.peek() == Some(';') {
            break;
        }

        let token_start = cur.i;
        match cur.peek() {
            Some('(') => {
                if !cur.skip_balanced_paren() {
                    return last_token_end;
                }
                last_token_end = Some(cur.i);
            }
            Some('\'') | Some('"') | Some('`') => {
                if !cur.skip_quoted() {
                    return last_token_end;
                }
                last_token_end = Some(cur.i);
            }
            Some('$') if dialect == SqlDialect::Postgres => {
                if !cur.skip_dollar_quote_or_char() {
                    return last_token_end;
                }
                last_token_end = Some(cur.i);
            }
            Some(ch) if is_ident_start(ch) => {
                cur.skip_ident();
                let keyword = &sql[token_start..cur.i];
                if offset_start.is_none() && keyword.eq_ignore_ascii_case("OFFSET") {
                    offset_start = Some(token_start);
                }
                if lock_start.is_none() && is_locking_start(keyword, &mut cur) {
                    lock_start = Some(token_start);
                }
                last_token_end = Some(cur.i);
            }
            Some(_) => {
                cur.bump();
                last_token_end = Some(cur.i);
            }
            None => break,
        }
    }

    offset_start.or(lock_start).or(last_token_end)
}

fn is_locking_start(keyword: &str, cur: &mut Cursor<'_>) -> bool {
    if keyword.eq_ignore_ascii_case("FOR") {
        let saved = cur.i;
        cur.skip_ws_and_comments();
        let next = cur.peek_keyword();
        cur.i = saved;
        return next
            .is_some_and(|value| keyword_is(&value, &["UPDATE", "SHARE", "NO", "KEY", "NOWAIT"]));
    }
    if keyword.eq_ignore_ascii_case("LOCK") {
        let saved = cur.i;
        cur.skip_ws_and_comments();
        let next = cur.peek_keyword();
        cur.i = saved;
        return next.is_some_and(|value| value.eq_ignore_ascii_case("IN"));
    }
    false
}

fn skip_with_clause(cur: &mut Cursor<'_>) -> bool {
    if cur.take_keyword("WITH").is_none() {
        return false;
    }
    cur.skip_ws_and_comments();
    let _ = cur.take_keyword("RECURSIVE");
    loop {
        cur.skip_ws_and_comments();
        if !cur.skip_name() {
            return false;
        }
        cur.skip_ws_and_comments();
        if cur.peek() == Some('(') && !cur.skip_balanced_paren() {
            return false;
        }
        cur.skip_ws_and_comments();
        if cur.take_keyword("AS").is_none() {
            return false;
        }
        cur.skip_ws_and_comments();
        skip_materialized(cur);
        if cur.peek() != Some('(') || !cur.skip_balanced_paren() {
            return false;
        }
        skip_cte_tail(cur);
        cur.skip_ws_and_comments();
        if cur.peek() == Some(',') {
            cur.bump();
            continue;
        }
        return true;
    }
}

fn skip_materialized(cur: &mut Cursor<'_>) {
    cur.skip_ws_and_comments();
    if cur.take_keyword("NOT").is_some() {
        cur.skip_ws_and_comments();
        let _ = cur.take_keyword("MATERIALIZED");
        return;
    }
    let _ = cur.take_keyword("MATERIALIZED");
}

fn skip_cte_tail(cur: &mut Cursor<'_>) {
    loop {
        cur.skip_ws_and_comments();
        match cur.peek_keyword() {
            Some(keyword)
                if keyword_is(
                    &keyword,
                    &[
                        "SELECT", "TABLE", "VALUES", "INSERT", "UPDATE", "DELETE", "MERGE",
                    ],
                ) =>
            {
                return;
            }
            Some(_) if cur.peek() == Some(',') => return,
            Some(_) => {
                cur.skip_ident();
            }
            None => {
                if cur.peek() == Some(',') || cur.eof() {
                    return;
                }
                if cur.peek() == Some('(') {
                    if !cur.skip_balanced_paren() {
                        return;
                    }
                    continue;
                }
                cur.bump();
            }
        }
    }
}

fn next_statement_end(sql: &str, start: usize, dialect: SqlDialect) -> usize {
    let mut cur = Cursor::new(&sql[start..], dialect);
    loop {
        cur.skip_ws_and_comments();
        if cur.eof() {
            return sql.len();
        }
        match cur.peek() {
            Some(';') => {
                cur.bump();
                return start + cur.i;
            }
            Some('(') => {
                if !cur.skip_balanced_paren() {
                    return sql.len();
                }
            }
            Some('\'') | Some('"') | Some('`') => {
                if !cur.skip_quoted() {
                    return sql.len();
                }
            }
            Some('$') if dialect == SqlDialect::Postgres => {
                if !cur.skip_dollar_quote_or_char() {
                    return sql.len();
                }
            }
            Some(_) => {
                cur.bump();
            }
            None => return sql.len(),
        }
    }
}

fn keyword_is(value: &str, options: &[&str]) -> bool {
    options
        .iter()
        .any(|option| value.eq_ignore_ascii_case(option))
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

struct Cursor<'a> {
    src: &'a str,
    i: usize,
    dialect: SqlDialect,
}

impl<'a> Cursor<'a> {
    fn new(src: &'a str, dialect: SqlDialect) -> Self {
        Self { src, i: 0, dialect }
    }

    fn eof(&self) -> bool {
        self.i >= self.src.len()
    }

    fn rest(&self) -> &'a str {
        &self.src[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.i += ch.len_utf8();
        Some(ch)
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            let before = self.i;
            self.skip_ws();
            if !self.skip_comment() && self.i == before {
                return;
            }
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
            self.bump();
        }
    }

    fn skip_comment(&mut self) -> bool {
        let rest = self.rest();
        if rest.starts_with("--") {
            self.i += 2;
            while let Some(ch) = self.peek() {
                self.bump();
                if ch == '\n' {
                    break;
                }
            }
            return true;
        }
        if rest.starts_with("/*") {
            self.i += 2;
            while !self.eof() {
                if self.rest().starts_with("*/") {
                    self.i += 2;
                    break;
                }
                self.bump();
            }
            return true;
        }
        false
    }

    fn skip_ident(&mut self) {
        if !matches!(self.peek(), Some(ch) if is_ident_start(ch)) {
            return;
        }
        self.bump();
        while matches!(self.peek(), Some(ch) if is_ident_continue(ch)) {
            self.bump();
        }
    }

    fn skip_name(&mut self) -> bool {
        match self.peek() {
            Some('"') | Some('`') => self.skip_quoted(),
            Some(ch) if is_ident_start(ch) => {
                self.skip_ident();
                true
            }
            _ => false,
        }
    }

    fn peek_keyword(&self) -> Option<String> {
        let mut tmp = Cursor::new(self.rest(), self.dialect);
        tmp.skip_ws_and_comments();
        let start = tmp.i;
        tmp.skip_ident();
        if tmp.i == start {
            return None;
        }
        Some(tmp.src[start..tmp.i].to_string())
    }

    fn take_keyword(&mut self, expected: &str) -> Option<()> {
        self.skip_ws_and_comments();
        let start = self.i;
        self.skip_ident();
        if self.i > start && self.src[start..self.i].eq_ignore_ascii_case(expected) {
            Some(())
        } else {
            self.i = start;
            None
        }
    }

    fn skip_quoted(&mut self) -> bool {
        let Some(quote) = self.peek() else {
            return false;
        };
        if quote != '\'' && quote != '"' && quote != '`' {
            return false;
        }
        self.bump();
        while let Some(ch) = self.peek() {
            if ch == '\\' && self.dialect == SqlDialect::MySql {
                self.bump();
                self.bump();
                continue;
            }
            if ch == quote {
                self.bump();
                if self.peek() == Some(quote) {
                    self.bump();
                    continue;
                }
                return true;
            }
            self.bump();
        }
        false
    }

    fn skip_dollar_quote_or_char(&mut self) -> bool {
        let rest = self.rest();
        if !rest.starts_with('$') {
            return false;
        }
        let after = &rest[1..];
        let tag_len = after
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .map(char::len_utf8)
            .sum::<usize>();
        let after_tag = &after[tag_len..];
        if !after_tag.starts_with('$') {
            self.bump();
            return true;
        }
        let closer = format!("${}$", &after[..tag_len]);
        self.i += 1 + tag_len + 1;
        if let Some(rel) = self.rest().find(&closer) {
            self.i += rel + closer.len();
            true
        } else {
            false
        }
    }

    fn skip_balanced_paren(&mut self) -> bool {
        if self.peek() != Some('(') {
            return false;
        }
        let mut depth = 0_u32;
        while let Some(ch) = self.peek() {
            match ch {
                '(' => {
                    depth += 1;
                    self.bump();
                }
                ')' => {
                    self.bump();
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return true;
                    }
                }
                '\'' | '"' | '`' => {
                    if !self.skip_quoted() {
                        return false;
                    }
                }
                '$' if self.dialect == SqlDialect::Postgres => {
                    if !self.skip_dollar_quote_or_char() {
                        return false;
                    }
                }
                '-' | '/' => {
                    if !self.skip_comment() {
                        self.bump();
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_sql_kind() {
        assert_eq!(statement_kind("  select 1"), "query");
        assert_eq!(statement_kind("INSERT INTO t VALUES (1)"), "insert");
        assert_eq!(statement_kind("drop table t"), "drop");
        assert_eq!(statement_kind("SET search_path TO public"), "session");
    }

    fn apply_pg(sql: &str) -> SqlLimitRewrite {
        apply_default_query_limit(sql, SqlDialect::Postgres)
    }

    fn apply_mysql(sql: &str) -> SqlLimitRewrite {
        apply_default_query_limit(sql, SqlDialect::MySql)
    }

    #[test]
    fn adds_limit_to_plain_select() {
        let rewritten = apply_pg("SELECT * FROM t");
        assert!(rewritten.applied);
        assert_eq!(rewritten.sql, "SELECT * FROM t LIMIT 500");
    }

    #[test]
    fn preserves_existing_limit() {
        let sql = "SELECT * FROM t LIMIT 10";
        let rewritten = apply_pg(sql);
        assert!(!rewritten.applied);
        assert_eq!(rewritten.sql, sql);
    }

    #[test]
    fn adds_outer_limit_when_only_subquery_has_one() {
        let rewritten = apply_mysql("SELECT * FROM (SELECT * FROM u LIMIT 1) x");
        assert!(rewritten.applied);
        assert_eq!(
            rewritten.sql,
            "SELECT * FROM (SELECT * FROM u LIMIT 1) x LIMIT 500"
        );
    }

    #[test]
    fn ignores_limit_in_comments_and_strings() {
        let rewritten = apply_pg("-- LIMIT 1\nSELECT 'LIMIT 10' FROM t");
        assert!(rewritten.applied);
        assert_eq!(
            rewritten.sql,
            "-- LIMIT 1\nSELECT 'LIMIT 10' FROM t LIMIT 500"
        );
    }

    #[test]
    fn inserts_limit_before_offset_and_for_update() {
        assert_eq!(
            apply_mysql("SELECT * FROM t OFFSET 10").sql,
            "SELECT * FROM t LIMIT 500 OFFSET 10"
        );
        assert_eq!(
            apply_pg("SELECT * FROM t FOR UPDATE").sql,
            "SELECT * FROM t LIMIT 500 FOR UPDATE"
        );
        assert_eq!(
            apply_mysql("SELECT * FROM t LOCK IN SHARE MODE").sql,
            "SELECT * FROM t LIMIT 500 LOCK IN SHARE MODE"
        );
    }

    #[test]
    fn preserves_fetch_first() {
        let sql = "SELECT * FROM t FETCH FIRST 10 ROWS ONLY";
        let rewritten = apply_pg(sql);
        assert!(!rewritten.applied);
        assert_eq!(rewritten.sql, sql);
    }

    #[test]
    fn limits_with_select_but_not_with_dml() {
        let select = apply_pg("WITH c AS (SELECT 1) SELECT * FROM c");
        assert!(select.applied);
        assert_eq!(select.sql, "WITH c AS (SELECT 1) SELECT * FROM c LIMIT 500");

        let update = "WITH c AS (SELECT 1) UPDATE t SET x = 1 FROM c";
        let rewritten = apply_pg(update);
        assert!(!rewritten.applied);
        assert_eq!(rewritten.sql, update);
    }

    #[test]
    fn skips_explain_insert_and_show() {
        for sql in [
            "EXPLAIN SELECT * FROM t",
            "INSERT INTO t SELECT * FROM u",
            "SHOW TABLES",
        ] {
            let rewritten = apply_mysql(sql);
            assert!(!rewritten.applied, "{sql}");
            assert_eq!(rewritten.sql, sql);
        }
    }

    #[test]
    fn limits_each_select_in_a_script() {
        let rewritten = apply_pg("SELECT * FROM a; INSERT INTO t VALUES (1); SELECT * FROM b;");
        assert!(rewritten.applied);
        assert_eq!(
            rewritten.sql,
            "SELECT * FROM a LIMIT 500; INSERT INTO t VALUES (1); SELECT * FROM b LIMIT 500;"
        );
    }

    #[test]
    fn preserves_trailing_semicolon_and_table_command() {
        assert_eq!(apply_pg("SELECT 1;").sql, "SELECT 1 LIMIT 500;");
        assert_eq!(apply_pg("TABLE users").sql, "TABLE users LIMIT 500");
    }

    #[test]
    fn handles_postgres_dollar_quotes() {
        let rewritten = apply_pg("SELECT $lim$ LIMIT 1 $lim$ FROM t");
        assert!(rewritten.applied);
        assert_eq!(rewritten.sql, "SELECT $lim$ LIMIT 1 $lim$ FROM t LIMIT 500");
    }

    #[test]
    fn limits_union_without_touching_existing_limit() {
        assert_eq!(
            apply_pg("SELECT a FROM t UNION SELECT b FROM u").sql,
            "SELECT a FROM t UNION SELECT b FROM u LIMIT 500"
        );
        let sql = "SELECT a FROM t UNION SELECT b FROM u LIMIT 20";
        let rewritten = apply_pg(sql);
        assert!(!rewritten.applied);
        assert_eq!(rewritten.sql, sql);
    }

    #[test]
    fn preserves_lowercase_limit() {
        let sql = "select * from t limit 3";
        assert!(!apply_mysql(sql).applied);
    }
}
