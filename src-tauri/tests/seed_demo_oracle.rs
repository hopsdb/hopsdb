//! Seed a realistic Oracle test dataset for local UI verification.
//!
//! This requires the local Oracle Docker environment. Run it explicitly with:
//! `cargo test --test seed_demo_oracle -- --ignored --nocapture`

use hopsdb_app_lib::ssh::SshSession;

async fn exec(s: &SshSession, sql: &str) {
    let _ = s.run_exec(sql).await;
}

async fn drop_if_exists(s: &SshSession, table: &str) {
    let sql = format!(
        r#"
begin
  execute immediate 'drop table {table} purge';
exception
  when others then
    if sqlcode != -942 then
      raise;
    end if;
end;
/"#
    );
    exec(s, &sql).await;
}

/// Add table & column comments. Each line of the embedded SQL file is one
/// comment on DDL statement, executed individually.
async fn add_comments(s: &SshSession) {
    let sql = include_str!("comments.sql");
    for line in sql.lines() {
        let stmt = line.trim();
        if stmt.is_empty() || stmt.starts_with("--") {
            continue;
        }
        // The embedded statements carry no trailing ';' — append it here.
        exec(s, &format!("{stmt};")).await;
    }
}

#[tokio::test]
#[ignore = "requires the local Oracle Docker environment"]
async fn seed_demo_oracle() {
    println!("=== Seeding Oracle demo tables ===");

    let s = SshSession::connect("oracle@127.0.0.1:2222", "oracle123")
        .await
        .expect("ssh connect");

    for cmd in [
        "export ORACLE_HOME=/opt/oracle/product/26ai/dbhomeFree",
        "export PATH=$ORACLE_HOME/bin:$PATH",
        // NLS_LANG tells sqlplus the client charset is UTF-8 so Chinese/Japanese
        // in DML statements isn't silently turned into '?' when stored.
        "export NLS_LANG=AMERICAN_AMERICA.AL32UTF8",
        "sqlplus system/hopsdb123@localhost:1521/FREEPDB1",
        "set pagesize 50000",
        "set linesize 9999",
        "set autocommit off",
        "set exitcommit off",
        "set markup html on entmap off",
    ] {
        exec(&s, cmd).await;
    }

    for table in [
        "HOPS_BIG_LOGS",
        "HOPS_MESSAGES",
        "HOPS_EVENTS",
        "HOPS_TEXTS",
        "HOPS_ORDER_ITEMS",
        "HOPS_ORDERS",
        "HOPS_PRODUCTS",
        "HOPS_CUSTOMERS",
        "HOPS_CODES",
        "HOPS_NOTES",
    ] {
        drop_if_exists(&s, table).await;
    }

    exec(
        &s,
        r#"
create table HOPS_CUSTOMERS (
  CUSTOMER_ID number primary key,
  CUSTOMER_CODE varchar2(20),
  CUSTOMER_NAME varchar2(100),
  COUNTRY varchar2(20),
  CITY varchar2(40),
  STATUS varchar2(20),
  CREATED_AT date,
  REMARK varchar2(200)
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_PRODUCTS (
  PRODUCT_ID number primary key,
  PRODUCT_CODE varchar2(20),
  PRODUCT_NAME varchar2(120),
  CATEGORY varchar2(40),
  PRICE number(10,2),
  STOCK_QTY number,
  TAG varchar2(30),
  NOTE varchar2(200)
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_ORDERS (
  ORDER_ID number primary key,
  CUSTOMER_ID number,
  ORDER_NO varchar2(30),
  ORDER_STATUS varchar2(20),
  ORDER_DATE date,
  PAY_METHOD varchar2(20),
  TOTAL_AMOUNT number(12,2),
  REMARK varchar2(200),
  UPDATED_AT date
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_ORDER_ITEMS (
  ITEM_ID number primary key,
  ORDER_ID number,
  PRODUCT_ID number,
  QTY number,
  UNIT_PRICE number(10,2),
  LINE_TOTAL number(12,2),
  ITEM_NOTE varchar2(120),
  CREATED_AT date
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_NOTES (
  NOTE_ID number primary key,
  NOTE_TITLE varchar2(120),
  NOTE_BODY varchar2(400),
  LANG_TAG varchar2(20),
  LEVEL_TAG varchar2(20),
  CREATED_AT date
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_EVENTS (
  EVENT_ID number primary key,
  EVENT_TYPE varchar2(40),
  EVENT_NAME varchar2(120),
  EVENT_SOURCE varchar2(30),
  EVENT_STATUS varchar2(20),
  OCCURRED_AT date,
  DETAIL_TEXT varchar2(300),
  EXTRA_TEXT varchar2(120)
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_CODES (
  CODE_ID number primary key,
  CODE_GROUP varchar2(40),
  CODE_KEY varchar2(40),
  CODE_VALUE varchar2(120),
  SORT_NO number,
  ACTIVE_FLAG varchar2(1)
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_TEXTS (
  TEXT_ID number primary key,
  TITLE varchar2(120),
  BODY_TEXT varchar2(400),
  MARKER varchar2(30),
  LOCALE_TAG varchar2(20),
  CREATED_AT date,
  UPDATED_AT date
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_MESSAGES (
  MSG_ID number primary key,
  USER_NAME varchar2(60),
  SUBJECT varchar2(120),
  MESSAGE_BODY varchar2(400),
  LANG_TAG varchar2(20),
  SENT_AT date,
  STATUS_TAG varchar2(20),
  SYMBOLS varchar2(60)
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
create table HOPS_BIG_LOGS (
  LOG_ID number primary key,
  CUSTOMER_ID number,
  EVENT_KIND varchar2(30),
  CHANNEL_TAG varchar2(20),
  AMOUNT number(12,2),
  NOTE_TEXT varchar2(200),
  JP_TEXT varchar2(80),
  CN_TEXT varchar2(80),
  CREATED_AT date
)
/"#,
    )
    .await;

    exec(
        &s,
        r#"
insert into HOPS_CUSTOMERS values (1, 'C001', '山田太郎', 'JP', '東京', 'ACTIVE', date '2026-08-01', 'VIP / 重要');
insert into HOPS_CUSTOMERS values (2, 'C002', '张三', 'CN', '上海', 'ACTIVE', date '2026-08-02', '含有中文');
insert into HOPS_CUSTOMERS values (3, 'C003', 'Alice & Bob', 'US', 'New York', 'SUSPENDED', date '2026-08-03', 'symbols <> & /');
insert into HOPS_CUSTOMERS values (4, 'C004', '佐藤花子', 'JP', '大阪', 'ACTIVE', date '2026-08-04', 'かな/カナ mixed');
insert into HOPS_CUSTOMERS values (5, 'C005', '王五', 'CN', '北京', 'NEW', date '2026-08-05', '测试-空格-空白');
insert into HOPS_CUSTOMERS values (6, 'C006', 'Müller', 'DE', 'Berlin', 'ACTIVE', date '2026-08-06', 'äöüß');

insert into HOPS_PRODUCTS values (1, 'P001', '抹茶ラテ', 'DRINK', 18.50, 120, 'JP', '人気商品');
insert into HOPS_PRODUCTS values (2, 'P002', '红茶', 'DRINK', 12.00, 200, 'CN', '中国茶');
insert into HOPS_PRODUCTS values (3, 'P003', 'USB-C Cable', 'ACCESSORY', 9.90, 500, 'US', 'Fast charge');
insert into HOPS_PRODUCTS values (4, 'P004', 'ゲームパッド', 'ELECTRONICS', 59.00, 80, 'JP', '対応: PS5/PC');
insert into HOPS_PRODUCTS values (5, 'P005', '多功能包', 'BAG', 33.30, 60, 'CN', '防水 / 耐磨');
insert into HOPS_PRODUCTS values (6, 'P006', 'Notebook Pro', 'STATIONERY', 4.20, 1000, 'US', 'A5 size');

insert into HOPS_ORDERS values (1, 1, 'O20260822001', 'PAID', date '2026-08-10', 'CARD', 64.50, '初回注文', date '2026-08-10');
insert into HOPS_ORDERS values (2, 2, 'O20260822002', 'SHIPPED', date '2026-08-11', 'ALIPAY', 24.00, '含中文备注', date '2026-08-11');
insert into HOPS_ORDERS values (3, 3, 'O20260822003', 'CANCELLED', date '2026-08-12', 'PAYPAL', 9.90, 'special chars !@#$%^&*()', date '2026-08-12');
insert into HOPS_ORDERS values (4, 4, 'O20260822004', 'PAID', date '2026-08-13', 'CARD', 59.00, 'かなテスト', date '2026-08-13');

insert into HOPS_ORDER_ITEMS values (1, 1, 1, 1, 18.50, 18.50, '抹茶ラテ x1', date '2026-08-10');
insert into HOPS_ORDER_ITEMS values (2, 1, 3, 2, 9.90, 19.80, 'Cable x2', date '2026-08-10');
insert into HOPS_ORDER_ITEMS values (3, 2, 2, 1, 12.00, 12.00, '红茶', date '2026-08-11');
insert into HOPS_ORDER_ITEMS values (4, 4, 4, 1, 59.00, 59.00, 'ゲームパッド', date '2026-08-13');
insert into HOPS_ORDER_ITEMS values (5, 3, 3, 1, 9.90, 9.90, 'cancelled item', date '2026-08-12');

insert into HOPS_NOTES values (1, '日文メモ', 'テスト用の日本語メモです。', 'JA', 'INFO', date '2026-08-10');
insert into HOPS_NOTES values (2, '中文备注', '这是一个中文测试备注。', 'ZH', 'WARN', date '2026-08-11');
insert into HOPS_NOTES values (3, 'ASCII Note', 'Symbols <> & "quotes" and slashes / \\', 'EN', 'DEBUG', date '2026-08-12');
insert into HOPS_NOTES values (4, 'Mix 記録', '混合文字列 mixed 日本語 中文 English', 'MIX', 'INFO', date '2026-08-13');

insert into HOPS_EVENTS values (1, 'LOGIN', 'Login success', 'WEB', 'OK', date '2026-08-10', 'user=山田太郎', 'tag=alpha');
insert into HOPS_EVENTS values (2, 'PAY', 'Payment captured', 'API', 'OK', date '2026-08-11', 'amount=64.50', 'note=正常');
insert into HOPS_EVENTS values (3, 'SHIP', 'Shipment created', 'BATCH', 'OK', date '2026-08-12', 'tracking=JP123', 'status=発送');
insert into HOPS_EVENTS values (4, 'ERR', 'Validation failed', 'WEB', 'FAIL', date '2026-08-13', 'fields=<name>', '原因=空欄');

insert into HOPS_CODES values (1, 'STATUS', 'ACTIVE', 'Active / 有効', 1, 'Y');
insert into HOPS_CODES values (2, 'STATUS', 'NEW', 'New / 新規', 2, 'Y');
insert into HOPS_CODES values (3, 'STATUS', 'HOLD', 'On Hold / 保留', 3, 'Y');
insert into HOPS_CODES values (4, 'LANG', 'JA', 'Japanese', 1, 'Y');
insert into HOPS_CODES values (5, 'LANG', 'ZH', 'Chinese', 2, 'Y');
insert into HOPS_CODES values (6, 'LANG', 'EN', 'English', 3, 'Y');

insert into HOPS_TEXTS values (1, '説明書', 'これは説明書です。', 'DOC', 'JA', date '2026-08-10', date '2026-08-10');
insert into HOPS_TEXTS values (2, '使用说明', '这是一段测试说明。', 'DOC', 'ZH', date '2026-08-11', date '2026-08-11');
insert into HOPS_TEXTS values (3, 'Quick Guide', 'Line1' || chr(10) || 'Line2' || chr(10) || 'Line3', 'GUIDE', 'EN', date '2026-08-12', date '2026-08-12');
insert into HOPS_TEXTS values (4, '記号テキスト', 'Symbols: ~!@#$%^&*()_+{}|:"<>?', 'SYM', 'JA', date '2026-08-13', date '2026-08-13');

insert into HOPS_MESSAGES values (1, '山田太郎', 'こんにちは', 'おはようございます！', 'JA', date '2026-08-10', 'SENT', '★☆♪');
insert into HOPS_MESSAGES values (2, '张三', '你好', '这是一个带有中文的消息。', 'ZH', date '2026-08-11', 'SENT', '中文★');
insert into HOPS_MESSAGES values (3, 'alice', 'Re: quote', 'Quotes "double" and ''single'' and slash /', 'EN', date '2026-08-12', 'DRAFT', 'quotes');
insert into HOPS_MESSAGES values (4, 'bob', 'emoji-like', 'ASCII only <>& tested', 'EN', date '2026-08-13', 'QUEUED', 'x+y=z');

commit;
/"#,
    )
    .await;

    exec(
        &s,
        r#"
insert into HOPS_BIG_LOGS
select
  level as LOG_ID,
  mod(level, 6) + 1 as CUSTOMER_ID,
  case mod(level, 4)
    when 0 then 'LOGIN'
    when 1 then 'PAY'
    when 2 then 'SHIP'
    else 'SEARCH'
  end as EVENT_KIND,
  case mod(level, 3)
    when 0 then 'WEB'
    when 1 then 'APP'
    else 'BATCH'
  end as CHANNEL_TAG,
  round(mod(level * 13.37, 999), 2) as AMOUNT,
  'row #' || level || ' / special <> & chars' as NOTE_TEXT,
  case mod(level, 4)
    when 0 then '日本語ログ'
    when 1 then 'テスト行'
    when 2 then '確認済み'
    else '処理中'
  end as JP_TEXT,
  case mod(level, 4)
    when 0 then '中文日志'
    when 1 then '测试行'
    when 2 then '已确认'
    else '处理中'
  end as CN_TEXT,
  date '2026-08-01' + (level / 24) as CREATED_AT
from dual
connect by level <= 2200
/"#,
    )
    .await;

    exec(&s, "commit;").await;
    println!("    ✅ demo tables seeded");

    // Add table & column comments.
    add_comments(&s).await;
    exec(&s, "commit;").await;
    println!("    ✅ table & column comments added");

    s.close(false).await;
}
