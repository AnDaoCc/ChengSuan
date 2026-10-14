use crate::model::{validate_plan, validate_settings};
use fs2::FileExt;
use rusqlite::{backup::Backup, params, Connection, OptionalExtension};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
pub const VERSION: i64 = 3;
pub const APP_ID: i64 = 0x43534E31;
pub struct Store {
    pub db: Mutex<Connection>,
    pub root: PathBuf,
    pub _lock: File,
}
pub fn backup(conn: &Connection, path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err("备份文件已经存在，未覆盖".into());
    }
    let mut dest = Connection::open(path).map_err(|e| e.to_string())?;
    let result = Backup::new(conn, &mut dest)
        .and_then(|b| b.run_to_completion(128, Duration::from_millis(10), None))
        .map_err(|e| e.to_string());
    if result.is_err() {
        drop(dest);
        let _ = fs::remove_file(path);
    }
    result
}
pub fn timestamp_path(root: &Path, reason: &str) -> PathBuf {
    root.join("backups").join(format!(
        "{}-{}-{}.db",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        reason,
        uuid::Uuid::new_v4()
    ))
}
pub fn migrate(db: &mut Connection, root: &Path) -> Result<(), String> {
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let app_id: i64 = db
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version > VERSION {
        return Err("数据库来自更高版本，请使用更新的澄算；旧数据未修改".into());
    }
    if app_id != 0 && app_id != APP_ID {
        return Err("该文件不是澄算数据库".into());
    }
    let check: String = db
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if check != "ok" {
        return Err(format!(
            "数据库检查失败：{check}；请选择有效备份，原数据库未覆盖"
        ));
    }
    if version > 0 && version < VERSION {
        backup(db, &timestamp_path(root, "before-migration"))?;
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    if version < 1 {
        tx.execute_batch("CREATE TABLE plans(id TEXT PRIMARY KEY,payload TEXT NOT NULL); CREATE TABLE settings(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);").map_err(|e|e.to_string())?;
    }
    if version < 2 {
        tx.execute_batch("ALTER TABLE plans ADD COLUMN updated_at TEXT NOT NULL DEFAULT ''; CREATE INDEX plans_updated ON plans(updated_at);").map_err(|e|e.to_string())?;
    }
    if version < 3 {
        crate::investment::migrate(&tx)?;
    }
    tx.pragma_update(None, "application_id", APP_ID)
        .map_err(|e| e.to_string())?;
    tx.pragma_update(None, "user_version", VERSION)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}
pub fn open(root: PathBuf) -> Result<Store, String> {
    fs::create_dir_all(&root).map_err(|e| {
        format!("无法在 EXE 所在目录创建 ChengSuanData：{e}。请将 EXE 和数据移到普通可写文件夹。")
    })?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".lock"))
        .map_err(|e| format!("数据目录不可写：{e}"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "该数据目录正在被另一个澄算实例使用，请先关闭另一个实例".to_string())?;
    for dir in ["caches", "logs", "backups", "exports", "webview"] {
        fs::create_dir_all(root.join(dir)).map_err(|e| format!("无法创建数据子目录：{e}"))?;
    }
    let probe = root.join(format!(".write-check-{}", uuid::Uuid::new_v4()));
    fs::write(&probe, b"write check").map_err(|e| format!("数据目录不可写：{e}"))?;
    fs::remove_file(probe).map_err(|e| e.to_string())?;
    let mut db =
        Connection::open(root.join("chengsuan.db")).map_err(|e| format!("无法打开数据库：{e}"))?;
    db.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    migrate(&mut db, &root)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
        .map_err(|e| e.to_string())?;
    Ok(Store {
        db: Mutex::new(db),
        root,
        _lock: lock,
    })
}
pub fn load(db: &Connection) -> Result<(Vec<Value>, Value), String> {
    let mut stmt = db
        .prepare("SELECT payload FROM plans ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut plans = Vec::new();
    for row in rows {
        let p: Value =
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        validate_plan(&p)?;
        plans.push(p);
    }
    let settings: Option<String> = db
        .query_row("SELECT payload FROM settings WHERE id=1", [], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    let settings = match settings {
        Some(s) => serde_json::from_str(&s).map_err(|e| e.to_string())?,
        None => {
            serde_json::json!({"theme":"system","defaultCurrency":"CNY","precision":null,"lastFormat":"xlsx","lastScope":"current","lastDetail":"full"})
        }
    };
    validate_settings(&settings)?;
    Ok((plans, settings))
}
pub fn put(db: &mut Connection, plans: &[Value], insert_only: bool) -> Result<(), String> {
    for p in plans {
        validate_plan(p)?;
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for p in plans {
        let sql = if insert_only {
            "INSERT INTO plans(id,payload,updated_at) VALUES(?1,?2,?3)"
        } else {
            "INSERT INTO plans(id,payload,updated_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,updated_at=excluded.updated_at"
        };
        tx.execute(
            sql,
            params![p["id"].as_str(), p.to_string(), p["updatedAt"].as_str()],
        )
        .map_err(|e| format!("保存失败（未提交）：{e}"))?;
    }
    tx.commit().map_err(|e| format!("数据库提交失败：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> Value {
        serde_json::json!({"schemaVersion":1,"id":"9837dd97-4990-47d3-9e96-3579bebf2430","name":"测试方案","description":"","currency":"CNY","items":[],"rates":[],"createdAt":"2026-10-12T00:00:00Z","updatedAt":"2026-10-12T00:00:00Z"})
    }
    #[test]
    fn persistence_backup_and_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let s = open(tmp.path().join("data")).unwrap();
        assert!(open(s.root.clone()).is_err());
        {
            let mut db = s.db.lock().unwrap();
            put(&mut db, &[p()], false).unwrap();
            let file = s.root.join("backups/test.db");
            backup(&db, &file).unwrap();
            let copy = Connection::open(file).unwrap();
            assert_eq!(load(&copy).unwrap().0.len(), 1);
        }
        let root = s.root.clone();
        drop(s);
        let s = open(root).unwrap();
        assert_eq!(
            load(&s.db.lock().unwrap()).unwrap().0[0]["name"],
            "测试方案"
        );
    }
    #[test]
    fn import_is_transactional() {
        let tmp = tempfile::tempdir().unwrap();
        let s = open(tmp.path().join("data")).unwrap();
        let mut db = s.db.lock().unwrap();
        assert!(put(&mut db, &[p(), p()], true).is_err());
        assert!(load(&db).unwrap().0.is_empty());
    }
    #[test]
    fn old_version_preserved_and_backed_up() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("backups")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE plans(id TEXT PRIMARY KEY,payload TEXT NOT NULL); CREATE TABLE settings(id INTEGER PRIMARY KEY,payload TEXT NOT NULL); PRAGMA user_version=1;").unwrap();
        db.execute(
            "INSERT INTO plans VALUES(?1,?2)",
            params![p()["id"].as_str(), p().to_string()],
        )
        .unwrap();
        migrate(&mut db, tmp.path()).unwrap();
        assert_eq!(load(&db).unwrap().0.len(), 1);
        assert_eq!(fs::read_dir(tmp.path().join("backups")).unwrap().count(), 1);
    }
    #[test]
    fn future_and_corrupt_db_are_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        db.pragma_update(None, "user_version", 999).unwrap();
        assert!(migrate(&mut db, tmp.path()).is_err());
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            999
        );
        fs::create_dir_all(tmp.path().join("bad")).unwrap();
        fs::write(tmp.path().join("bad/chengsuan.db"), b"not sqlite").unwrap();
        assert!(open(tmp.path().join("bad")).is_err());
        assert_eq!(
            fs::read(tmp.path().join("bad/chengsuan.db")).unwrap(),
            b"not sqlite"
        );
    }
    #[test]
    fn rejects_invalid_rules_on_native_boundary() {
        let mut plan = p();
        plan["items"] = serde_json::json!([{"id":"b8d8922b-5423-4dce-8582-2ddbc36362d6","name":"x","note":"","currency":"CNY","kind":"usage","unit":"kg","unitPrice":"1","quantity":"1","count":"1","extraPercent":"0","billingQuantity":"0","period":"month","quantityMode":"use","billing":"ceil"}]);
        assert!(validate_plan(&plan).is_err());
    }
}
