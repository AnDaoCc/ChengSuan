use crate::{
    investment::{self, load, save},
    market::parsers,
    storage,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
fn cost() -> Value {
    json!({"schemaVersion":1,"id":"9837dd97-4990-47d3-9e96-3579bebf2430","name":"旧版保留方案","description":"","currency":"CNY","items":[],"rates":[],"createdAt":"2026-10-12T00:00:00Z","updatedAt":"2026-10-12T00:00:00Z"})
}
fn seed(doc: &mut Value) {
    let asset = parsers::asset(
        "cg:bitcoin",
        "BTC",
        "比特币",
        "crypto",
        "CRYPTO",
        "coingecko",
        "bitcoin",
        "USD",
    );
    doc["instruments"] = json!([asset]);
    doc["watchlist"] = json!(["cg:bitcoin"]);
    doc["trades"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"accountId":doc["accounts"][0]["id"],"instrumentId":"cg:bitcoin","side":"buy","executedAt":"2026-10-12T00:00:00Z","sequence":1,"quantity":"0.000000000123456789","unitPrice":"60000.0000000000001","fee":"0.00000000001","currency":"USDT","fxToCny":"6.70001234567890123456789","fxDate":"2026-10-12","fxSource":"实际成交换算依据","note":"测试记录"}]);
}
#[test]
fn version_two_migration_preserves_cost_and_creates_consistent_backup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("中文便携目录");
    std::fs::create_dir_all(&root).unwrap();
    let db = Connection::open(root.join("chengsuan.db")).unwrap();
    db.execute_batch("CREATE TABLE plans(id TEXT PRIMARY KEY,payload TEXT NOT NULL,updated_at TEXT NOT NULL);CREATE TABLE settings(id INTEGER PRIMARY KEY,payload TEXT NOT NULL);CREATE INDEX plans_updated ON plans(updated_at);PRAGMA user_version=2;").unwrap();
    db.pragma_update(None, "application_id", storage::APP_ID)
        .unwrap();
    db.execute(
        "INSERT INTO plans VALUES(?1,?2,?3)",
        params![
            cost()["id"].as_str(),
            cost().to_string(),
            cost()["updatedAt"].as_str()
        ],
    )
    .unwrap();
    drop(db);
    let store = storage::open(root.clone()).unwrap();
    let db = store.db.lock().unwrap();
    assert_eq!(storage::load(&db).unwrap().0, vec![cost()]);
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    let doc = load(&db).unwrap();
    assert_eq!(doc["accounts"][0]["name"], "我的投资");
    assert!(doc["trades"].as_array().unwrap().is_empty());
    let backups = std::fs::read_dir(root.join("backups"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(backups.len(), 1);
    let saved = Connection::open(backups[0].path()).unwrap();
    assert_eq!(
        saved
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(storage::load(&saved).unwrap().0, vec![cost()]);
}
#[test]
fn investment_restart_backup_restore_and_directory_move() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("first");
    let store = storage::open(root.clone()).unwrap();
    let original;
    {
        let mut db = store.db.lock().unwrap();
        storage::put(&mut db, &[cost()], false).unwrap();
        let mut doc = load(&db).unwrap();
        seed(&mut doc);
        let revision = save(&mut db, &doc).unwrap();
        assert_eq!(revision, 1);
        original = load(&db).unwrap();
        let path = root.join("backups/investment.db");
        storage::backup(&db, &path).unwrap();
        let backup = Connection::open(path).unwrap();
        assert_eq!(load(&backup).unwrap(), original);
        let mut changed = original.clone();
        changed["trades"] = json!([]);
        save(&mut db, &changed).unwrap();
        rusqlite::backup::Backup::new(&backup, &mut db)
            .unwrap()
            .run_to_completion(128, std::time::Duration::from_millis(1), None)
            .unwrap();
        assert_eq!(load(&db).unwrap(), original);
        assert_eq!(storage::load(&db).unwrap().0, vec![cost()]);
    }
    drop(store);
    let moved = temp.path().join("移动后的中文路径");
    std::fs::rename(root, &moved).unwrap();
    let store = storage::open(moved).unwrap();
    assert_eq!(load(&store.db.lock().unwrap()).unwrap(), original);
}
#[test]
fn sequential_revision_validation_and_transaction_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage::open(temp.path().join("data")).unwrap();
    let mut db = store.db.lock().unwrap();
    let mut doc = load(&db).unwrap();
    seed(&mut doc);
    save(&mut db, &doc).unwrap();
    assert!(save(&mut db, &doc).unwrap_err().contains("已变化"));
    let before = load(&db).unwrap();
    let mut invalid = before.clone();
    invalid["trades"][0]["side"] = json!("sell");
    assert!(save(&mut db, &invalid).unwrap_err().contains("卖出数量"));
    assert_eq!(load(&db).unwrap(), before);
    db.execute_batch("CREATE TRIGGER reject_trade BEFORE INSERT ON investment_trades BEGIN SELECT RAISE(ABORT,'simulated write failure');END;").unwrap();
    let mut next = before.clone();
    next["accounts"][0]["name"] = json!("不能提交的修改");
    assert!(save(&mut db, &next).is_err());
    assert_eq!(load(&db).unwrap(), before);
}
#[test]
fn native_contract_pool_direction_and_integer_guards() {
    let temp = tempfile::tempdir().unwrap();
    let store = storage::open(temp.path().join("data")).unwrap();
    let mut doc = load(&store.db.lock().unwrap()).unwrap();
    seed(&mut doc);
    doc["instruments"][0] = parsers::asset(
        "cn:SH:600000",
        "600000",
        "浦发银行",
        "stock",
        "SH",
        "cn",
        "1.600000",
        "CNY",
    );
    doc["trades"][0]["instrumentId"] = json!("cn:SH:600000");
    doc["watchlist"] = json!([]);
    assert!(investment::validate(&doc).unwrap_err().contains("整数"));
    let mut token = parsers::asset(
        "chain:eth:0xabc",
        "ABC",
        "代币",
        "crypto",
        "CRYPTO",
        "gecko",
        "0xabc",
        "USD",
    );
    token["chain"] = json!("eth");
    token["contract"] = json!("0xabc");
    token["pool"] = json!({"address":"pool","name":"pool","liquidity":"1","tokenSide":"base"});
    assert!(investment::validate_instrument(&token)
        .unwrap_err()
        .contains("池列表"));
}
