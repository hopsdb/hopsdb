//! End-to-end smoke test: SSH → sqlplus → HTML table.
//!
//! This test needs the local Oracle Docker environment and is intentionally
//! skipped by the default test suite. Run it explicitly with:
//! `cargo test --test e2e_oracle -- --ignored --nocapture`
use hopsdb_app_lib::ssh::SshSession;

#[tokio::test]
#[ignore = "requires the local Oracle Docker environment"]
async fn e2e_oracle_docker() {
    println!("=== E2E test: HopsDB backend vs local Oracle Docker container ===");

    // 1. Connect (host:port now supported)
    println!("[1] SSH connect oracle@127.0.0.1:2222 ...");
    let s = match SshSession::connect("oracle@127.0.0.1:2222", "oracle123").await {
        Ok(s) => {
            println!("    ✅ connected as {}@{}", s.user, s.host);
            s
        }
        Err(e) => {
            println!("    ❌ {e}");
            assert!(false, "connect failed: {e}");
            return;
        }
    };

    // 2. Plain command tab path
    println!("[2] Shell command: pwd ...");
    let pwd_out = s
        .run_user_command("pwd")
        .await
        .map(|outcome| outcome.lines)
        .unwrap_or_default();
    let pwd_joined = pwd_out.join("\n");
    assert!(
        pwd_joined.contains("/home/oracle"),
        "expected pwd output, got: {pwd_joined}"
    );
    println!("    ✅ pwd returned /home/oracle");

    // 3. Nested SSH line with its own password
    println!("[3] Nested SSH command with line password ...");
    let nested_out = s
        .run_exec_with_password("ssh oracle@127.0.0.1", "oracle123")
        .await
        .unwrap_or_default();
    let nested_joined = nested_out.join("\n");
    assert!(
        !nested_joined.contains("Command did not return"),
        "nested ssh did not reach a prompt: {nested_joined}"
    );
    let nested_pwd = s
        .run_user_command("pwd")
        .await
        .map(|outcome| outcome.lines)
        .unwrap_or_default();
    let nested_pwd_joined = nested_pwd.join("\n");
    assert!(
        nested_pwd_joined.contains("/home/oracle"),
        "expected nested pwd output, got: {nested_pwd_joined}"
    );
    let _ = s.run_user_command("exit").await;
    println!("    ✅ nested ssh returned command output");

    // 4. Init commands (env + start sqlplus)
    println!("[4] Init commands (env + sqlplus)...");
    for cmd in [
        "export ORACLE_HOME=/opt/oracle/product/26ai/dbhomeFree",
        "export PATH=$ORACLE_HOME/bin:$PATH",
        "sqlplus system/hopsdb123@localhost:1521/FREEPDB1",
    ] {
        let out = s.run_exec(cmd).await.unwrap_or_default();
        let joined = out.join(" ");
        if joined.contains("ORA-") || joined.contains("SP2-") {
            println!(
                "    ⚠️  error markers in output: {}",
                &joined[..joined.len().min(200)]
            );
        } else {
            println!("    ✅ {cmd}");
        }
    }

    // 5. SQL init (HTML mode)
    println!("[5] SQL init (HTML mode)...");
    for cmd in ["set pagesize 50000", "set markup html on entmap off"] {
        let _ = s.run_exec(cmd).await;
    }
    println!("    ✅ HTML mode on");

    // 6. Run a query
    println!("[6] Query user_tables (ROWNUM<=5)...");
    let out = s
        .run_exec("select table_name from user_tables where rownum <= 5;")
        .await
        .unwrap_or_default();
    let joined = out.join("\n");
    assert!(
        joined.contains("<table"),
        "expected HTML table, got: {joined}"
    );
    println!("    ✅ got HTML table with {} lines", out.len());
    for l in out.iter().take(8) {
        println!("    | {}", l);
    }

    // 7. Object tree query (Oracle data dictionary)
    println!("\n[7] Object tree: all_tables...");
    let out = s
        .run_exec("select table_name from all_tables where table_name not like'%$%' and rownum<=3 order by OWNER, table_name;")
        .await
        .unwrap_or_default();
    let joined = out.join("\n");
    assert!(joined.contains("<table"), "expected HTML table: {joined}");
    println!("    ✅ got HTML table, {} lines", out.len());
    for l in out.iter().take(8) {
        println!("    | {}", l);
    }

    // 8. Close
    s.close(false).await;
    println!("\n=== E2E PASSED ===");
}
