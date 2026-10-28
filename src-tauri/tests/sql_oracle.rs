//! Direct test of sql.rs against the local Oracle Docker container.
//! Verifies init_sql_commands, get_table_list, run_sql, get_tree_children.
//!
//! This requires the local Oracle Docker environment. Run it explicitly with:
//! `cargo test --test sql_oracle -- --ignored --nocapture`
use hopsdb_app_lib::sql;
use hopsdb_app_lib::ssh::SshSession;

async fn connect() -> SshSession {
    let s = SshSession::connect("oracle@127.0.0.1:2222", "oracle123")
        .await
        .expect("connect failed");
    // Init env + start sqlplus (the ConnectionManager command set)
    for cmd in [
        "export ORACLE_HOME=/opt/oracle/product/26ai/dbhomeFree",
        "export PATH=$ORACLE_HOME/bin:$PATH",
        "sqlplus system/hopsdb123@localhost:1521/FREEPDB1",
    ] {
        s.run_exec(cmd).await.unwrap_or_default();
    }
    s
}

#[tokio::test]
#[ignore = "requires the local Oracle Docker environment"]
async fn full_sql_chain() {
    println!("=== sql.rs direct test (Oracle container) ===");
    let s = connect().await;

    // 1. init_sql_commands (html mode)
    println!("\n[1] init_sql_commands...");
    match sql::init_sql_commands(&s, "Oracle").await {
        Ok(()) => println!("    ✅ ok"),
        Err(e) => println!("    ❌ {e}"),
    }

    // 2. get_table_list (Oracle)
    println!("\n[2] get_table_list(\"Oracle\")...");
    match sql::get_table_list(&s, "Oracle").await {
        Ok(tables) => println!(
            "    ✅ {} tables: {:?}",
            tables.len(),
            &tables[..tables.len().min(5)]
        ),
        Err(e) => println!("    ❌ {e}"),
    }

    // 3. run_sql (simple select)
    println!("\n[3] run_sql(\"select * from user_tables where rownum<=3\")...");
    match sql::run_sql(
        &s,
        "select table_name from user_tables where rownum<=3",
        "Oracle",
    )
    .await
    {
        Ok(r) => println!(
            "    ✅ columns={:?} rows={} sample={:?}",
            r.columns,
            r.rows.len(),
            r.rows.get(0)
        ),
        Err(e) => println!("    ❌ {e}"),
    }

    // 4. get_tree_children (TABLE columns for a known table HELP)
    println!("\n[4] get_tree_children(\"TABLE\", HELP, Oracle)...");
    match sql::get_tree_children(&s, "TABLE", "HELP", "Oracle").await {
        Ok(cols) => println!(
            "    ✅ {} columns: {:?}",
            cols.len(),
            &cols[..cols.len().min(5)]
        ),
        Err(e) => println!("    ❌ {e}"),
    }

    // 5. get_tree_children SEQUENCE
    println!("\n[5] get_tree_children(\"SEQUENCE\",..., Oracle)...");
    match sql::get_tree_children(&s, "SEQUENCE", "", "Oracle").await {
        Ok(seqs) => println!(
            "    ✅ {} sequences: {:?}",
            seqs.len(),
            &seqs[..seqs.len().min(3)]
        ),
        Err(e) => println!("    ❌ {e}"),
    }

    s.close(false).await;
    println!("\n=== DONE ===");
}
