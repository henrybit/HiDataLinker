use crate::error::{AppError, AppResult};

const MAX_IDENT_LEN: usize = 128;

pub fn validate_ident(name: &str) -> AppResult<()> {
    if name.is_empty() || name.len() > MAX_IDENT_LEN || name.contains('\0') || name.contains('/') {
        return Err(AppError::msg(format!("invalid identifier: {name}")));
    }
    Ok(())
}

pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

pub fn qualify(schema: &str, name: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(name))
}

pub fn quote_ident_pg(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub fn qualify_pg(schema: &str, name: &str) -> String {
    format!("{}.{}", quote_ident_pg(schema), quote_ident_pg(name))
}

pub fn quote_ident_mssql(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// Split `schema.object`. A bare name defaults to the `dbo` schema.
pub fn mssql_schema_object(object: &str) -> (&str, &str) {
    match object.split_once('.') {
        Some((schema, name)) if !schema.is_empty() && !name.is_empty() && !name.contains('.') => {
            (schema, name)
        }
        _ => ("dbo", object),
    }
}

pub fn qualify_mssql(database: &str, object: &str) -> String {
    let (schema, name) = mssql_schema_object(object);
    format!(
        "{}.{}.{}",
        quote_ident_mssql(database),
        quote_ident_mssql(schema),
        quote_ident_mssql(name)
    )
}

pub fn oracle_schema_password(name: &str) -> String {
    format!("{name}#Ora1")
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub fn create_mysql_database_sql(
    name: &str,
    charset: Option<&str>,
    collation: Option<&str>,
) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    let mut sql = format!("CREATE DATABASE {}", quote_ident(name));
    if let Some(charset) = nonempty(charset) {
        validate_ident(charset)?;
        sql.push_str(" CHARACTER SET ");
        sql.push_str(&quote_ident(charset));
    }
    if let Some(collation) = nonempty(collation) {
        validate_ident(collation)?;
        sql.push_str(" COLLATE ");
        sql.push_str(&quote_ident(collation));
    }
    Ok(sql)
}

pub fn create_pg_schema_sql(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(format!("CREATE SCHEMA {}", quote_ident_pg(name)))
}

pub fn drop_mysql_database_sql(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(format!("DROP DATABASE {}", quote_ident(name)))
}

pub fn drop_pg_schema_sql(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(format!("DROP SCHEMA {} CASCADE", quote_ident_pg(name)))
}

pub fn create_mssql_database_sql(name: &str, collation: Option<&str>) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    let mut sql = format!("CREATE DATABASE {}", quote_ident_mssql(name));
    if let Some(collation) = nonempty(collation) {
        validate_ident(collation)?;
        sql.push_str(" COLLATE ");
        sql.push_str(&quote_ident_mssql(collation));
    }
    Ok(sql)
}

pub fn drop_mssql_database_sql(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(format!("DROP DATABASE {}", quote_ident_mssql(name)))
}

pub fn oracle_set_schema_sql(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(format!(
        "ALTER SESSION SET CURRENT_SCHEMA = {}",
        quote_ident_pg(name)
    ))
}

/// Create an Oracle user (schema) if missing, then grant object privileges.
/// The password is `{name}#Ora1`.
pub fn create_oracle_schema_statements(name: &str) -> AppResult<Vec<String>> {
    let name = name.trim();
    validate_ident(name)?;
    let quoted = quote_ident_pg(name);
    let password = oracle_schema_password(name)
        .replace('"', "\"\"")
        .replace('\'', "''");
    let create = format!(
        "DECLARE\n  e_exists EXCEPTION;\n  PRAGMA EXCEPTION_INIT(e_exists, -1920);\nBEGIN\n  EXECUTE IMMEDIATE 'CREATE USER {quoted} IDENTIFIED BY \"{password}\" DEFAULT TABLESPACE USERS QUOTA UNLIMITED ON USERS';\nEXCEPTION\n  WHEN e_exists THEN NULL;\nEND;"
    );
    let grant = format!(
        "GRANT CREATE SESSION, CREATE TABLE, CREATE VIEW, CREATE SEQUENCE, CREATE PROCEDURE, CREATE TRIGGER TO {quoted}"
    );
    Ok(vec![create, grant])
}

pub fn drop_oracle_schema_sql(name: &str) -> AppResult<String> {
    let name = name.trim();
    validate_ident(name)?;
    Ok(format!("DROP USER {} CASCADE", quote_ident_pg(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_special_characters() {
        assert_eq!(quote_ident("users"), "`users`");
        assert_eq!(quote_ident("a`b"), "`a``b`");
        assert_eq!(qualify("shop", "orders"), "`shop`.`orders`");
        assert_eq!(quote_ident_pg("users"), "\"users\"");
        assert_eq!(quote_ident_pg("a\"b"), "\"a\"\"b\"");
        assert_eq!(qualify_pg("shop", "orders"), "\"shop\".\"orders\"");
    }

    #[test]
    fn rejects_empty_and_oversized_names() {
        assert!(validate_ident("").is_err());
        assert!(validate_ident(&"x".repeat(129)).is_err());
        assert!(validate_ident(&"x".repeat(128)).is_ok());
        assert!(validate_ident("orders").is_ok());
    }

    #[test]
    fn builds_mysql_create_database_sql() {
        assert_eq!(
            create_mysql_database_sql("shop", None, None).unwrap(),
            "CREATE DATABASE `shop`"
        );
        assert_eq!(
            create_mysql_database_sql(" shop ", Some("utf8mb4"), Some("utf8mb4_unicode_ci"))
                .unwrap(),
            "CREATE DATABASE `shop` CHARACTER SET `utf8mb4` COLLATE `utf8mb4_unicode_ci`"
        );
        assert_eq!(
            create_mysql_database_sql("a`b", Some("  "), Some("")).unwrap(),
            "CREATE DATABASE `a``b`"
        );
        assert!(create_mysql_database_sql("", None, None).is_err());
    }

    #[test]
    fn builds_pg_create_schema_sql() {
        assert_eq!(
            create_pg_schema_sql(" analytics ").unwrap(),
            "CREATE SCHEMA \"analytics\""
        );
        assert_eq!(
            create_pg_schema_sql("a\"b").unwrap(),
            "CREATE SCHEMA \"a\"\"b\""
        );
        assert!(create_pg_schema_sql("").is_err());
    }

    #[test]
    fn builds_drop_sql() {
        assert_eq!(
            drop_mysql_database_sql("shop").unwrap(),
            "DROP DATABASE `shop`"
        );
        assert_eq!(
            drop_pg_schema_sql("analytics").unwrap(),
            "DROP SCHEMA \"analytics\" CASCADE"
        );
        assert!(drop_mysql_database_sql("").is_err());
        assert!(drop_pg_schema_sql("").is_err());
    }

    #[test]
    fn quotes_and_builds_mssql_and_oracle_sql() {
        assert_eq!(quote_ident_mssql("a]b"), "[a]]b]");
        assert_eq!(
            qualify_mssql("Adventure", "dbo.Users"),
            "[Adventure].[dbo].[Users]"
        );
        assert_eq!(
            qualify_mssql("Adventure", "Users"),
            "[Adventure].[dbo].[Users]"
        );
        assert_eq!(
            create_mssql_database_sql("shop", Some("Latin1_General_CI_AS")).unwrap(),
            "CREATE DATABASE [shop] COLLATE [Latin1_General_CI_AS]"
        );
        assert_eq!(
            drop_mssql_database_sql("shop").unwrap(),
            "DROP DATABASE [shop]"
        );
        let oracle = create_oracle_schema_statements("shop").unwrap();
        assert_eq!(oracle.len(), 2);
        assert!(oracle[0].contains("CREATE USER \"shop\""));
        assert!(oracle[0].contains("IDENTIFIED BY \"shop#Ora1\""));
        assert!(oracle[1].starts_with("GRANT CREATE SESSION"));
        assert_eq!(
            drop_oracle_schema_sql("shop").unwrap(),
            "DROP USER \"shop\" CASCADE"
        );
    }
}
