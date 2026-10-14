mod exchange_rates;
mod investment;
#[cfg(test)]
mod investment_tests;
mod market;
mod model;
mod storage;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};
use storage::Store;
use tauri::{Manager, State};
fn err(store: &Store, message: String) -> String {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(store.root.join("logs/errors.log"))
    {
        let _ = writeln!(f, "{} {}", chrono::Local::now().to_rfc3339(), message);
    }
    message
}
#[tauri::command]
fn load_state(store: State<Store>) -> Result<Value, String> {
    let db = store.db.lock().map_err(|e| e.to_string())?;
    let (plans, settings) = storage::load(&db).map_err(|e| err(&store, e))?;
    Ok(json!({"plans":plans,"settings":settings,"dataDirectory":store.root}))
}
#[tauri::command]
fn save_plan(store: State<Store>, plan: Value) -> Result<(), String> {
    let mut db = store.db.lock().map_err(|e| e.to_string())?;
    storage::put(&mut db, &[plan], false).map_err(|e| err(&store, e))
}
#[tauri::command]
fn delete_plan(store: State<Store>, id: String) -> Result<(), String> {
    uuid::Uuid::parse_str(&id).map_err(|_| "ID 无效")?;
    store
        .db
        .lock()
        .map_err(|e| e.to_string())?
        .execute("DELETE FROM plans WHERE id=?1", [id])
        .map_err(|e| err(&store, e.to_string()))?;
    Ok(())
}
#[tauri::command]
fn save_settings(store: State<Store>, settings: Value) -> Result<(), String> {
    model::validate_settings(&settings)?;
    store.db.lock().map_err(|e|e.to_string())?.execute("INSERT INTO settings(id,payload) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[settings.to_string()]).map_err(|e|err(&store,e.to_string()))?;
    Ok(())
}
#[tauri::command]
fn import_plans(store: State<Store>, plans: Vec<Value>) -> Result<(), String> {
    if plans.is_empty() || plans.len() > 1000 {
        return Err("导入方案数量无效".into());
    }
    let mut db = store.db.lock().map_err(|e| e.to_string())?;
    storage::put(&mut db, &plans, true).map_err(|e| err(&store, e))
}
#[tauri::command]
async fn write_export(
    store: State<'_, Store>,
    extension: String,
    name: String,
    bytes: Vec<u8>,
) -> Result<Option<String>, String> {
    if !["xlsx", "csv", "chengsuan"].contains(&extension.as_str())
        || bytes.len() > 100 * 1024 * 1024
    {
        return Err("导出格式或大小无效".into());
    }
    let safe: String = name
        .chars()
        .filter(|c| !"<>:\"/\\|?*".contains(*c) && !c.is_control())
        .take(100)
        .collect();
    let suggested = if safe.trim().is_empty() {
        "澄算导出"
    } else {
        safe.trim().trim_end_matches('.')
    };
    let picked = rfd::AsyncFileDialog::new()
        .set_title("导出澄算成本方案")
        .set_directory(store.root.join("exports"))
        .set_file_name(format!("{suggested}.{extension}"))
        .add_filter("澄算导出文件", &[&extension])
        .save_file()
        .await;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let mut path = picked.path().to_path_buf();
    if path.extension().is_none() {
        path.set_extension(&extension);
    }
    if path.exists()
        && !rfd::AsyncMessageDialog::new()
            .set_title("确认覆盖文件")
            .set_description(format!("文件 {} 已存在，是否替换？", path.display()))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
            .await
            .eq(&rfd::MessageDialogResult::Yes)
    {
        return Ok(None);
    }
    // Write a sibling temporary file first; replacement never truncates an existing export on write failure.
    let temp = path.with_file_name(format!(".chengsuan-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        replace_export(&temp, &path)?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&temp);
        return Err(err(
            &store,
            format!("无法写入导出文件（请检查权限和剩余空间）：{e}"),
        ));
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}
fn replace_export(temp: &std::path::Path, path: &std::path::Path) -> std::io::Result<()> {
    if !path.exists() {
        return fs::rename(temp, path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let a: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                a.as_ptr(),
                b.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(temp, path)
    }
}
#[tauri::command]
async fn read_project() -> Result<Option<String>, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("导入澄算工程")
        .add_filter("澄算工程", &["chengsuan"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let meta = fs::metadata(file.path()).map_err(|e| e.to_string())?;
    if meta.len() > 20 * 1024 * 1024 {
        return Err("工程文件超过 20 MB".into());
    }
    fs::read_to_string(file.path())
        .map(Some)
        .map_err(|e| format!("无法读取工程文件：{e}"))
}
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    if text.len() > 10 * 1024 * 1024 {
        return Err("剪贴板内容过大".into());
    }
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| format!("剪贴板暂不可用：{e}"))
}
#[tauri::command]
fn create_backup(store: State<Store>) -> Result<String, String> {
    let file = storage::timestamp_path(&store.root, "manual");
    let db = store.db.lock().map_err(|e| e.to_string())?;
    storage::backup(&db, &file).map_err(|e| err(&store, e))?;
    Ok(file.to_string_lossy().into_owned())
}
#[tauri::command]
async fn restore_backup(app: tauri::AppHandle, store: State<'_, Store>) -> Result<bool, String> {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("选择澄算数据库备份")
        .set_directory(store.root.join("backups"))
        .add_filter("SQLite 备份", &["db"])
        .pick_file()
        .await
    else {
        return Ok(false);
    };
    if fs::canonicalize(file.path()).map_err(|e| e.to_string())?
        == fs::canonicalize(store.root.join("chengsuan.db")).map_err(|e| e.to_string())?
    {
        return Err("请选择备份文件，不能恢复当前活动数据库".into());
    }
    let source = rusqlite::Connection::open_with_flags(
        file.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("备份无法打开：{e}"))?;
    let app_id: i64 = source
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let version: i64 = source
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if app_id != storage::APP_ID || version > storage::VERSION || version < 1 {
        return Err("备份不是可兼容的澄算数据库".into());
    }
    let check: String = source
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if check != "ok" {
        return Err("备份已损坏，未修改当前数据".into());
    }
    let (plans, _) = storage::load(&source)?;
    let investment_info = if version >= 3 {
        let d = investment::load(&source)?;
        format!(
            "{} 个投资账户、{} 笔投资交易",
            d["accounts"].as_array().map_or(0, Vec::len),
            d["trades"].as_array().map_or(0, Vec::len)
        )
    } else {
        "这是旧版备份，不包含投资记录；恢复后投资模块将为空".into()
    };
    if rfd::AsyncMessageDialog::new()
        .set_title("恢复将替换当前方案、投资记录和设置")
        .set_description(format!(
            "备份含 {} 个成本方案；{}。恢复前会自动保存当前数据库的安全备份。是否继续？",
            plans.len(),
            investment_info
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await
        != rfd::MessageDialogResult::Yes
    {
        return Ok(false);
    }
    let mut db = store.db.lock().map_err(|e| e.to_string())?;
    storage::backup(&db, &storage::timestamp_path(&store.root, "before-restore"))?;
    let staged = store
        .root
        .join("backups")
        .join(format!("restore-stage-{}.db", uuid::Uuid::new_v4()));
    storage::backup(&source, &staged)?;
    let result = (|| -> Result<(), String> {
        let mut ready = rusqlite::Connection::open(&staged).map_err(|e| e.to_string())?;
        storage::migrate(&mut ready, &store.root)?;
        rusqlite::backup::Backup::new(&ready, &mut db)
            .and_then(|b| b.run_to_completion(128, std::time::Duration::from_millis(10), None))
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(staged);
    result.map_err(|e| err(&store, e))?;
    drop(db);
    app.state::<std::sync::Arc<market::MarketRuntime>>()
        .force_refresh();
    let _ = tauri::Emitter::emit(&app, "investment:restored", ());
    Ok(true)
}
#[tauri::command]
async fn fetch_rates(sources: Vec<String>, target: String) -> Result<Vec<Value>, String> {
    if sources.len() > 100
        || sources
            .iter()
            .chain(std::iter::once(&target))
            .any(|s| s.len() != 3 || !s.bytes().all(|b| b.is_ascii_uppercase()))
    {
        return Err("币种无效".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let mut rates = Vec::new();
    for source in sources {
        if source == target {
            continue;
        }
        let response = client
            .get("https://api.frankfurter.dev/v2/rates")
            .query(&[
                ("base", source.as_str()),
                ("quotes", target.as_str()),
                ("expand", "providers"),
            ])
            .send()
            .await
            .map_err(|e| format!("获取汇率失败，原汇率保留；可手动填写：{e}"))?
            .error_for_status()
            .map_err(|e| format!("汇率服务响应失败：{e}"))?;
        let data = response.bytes().await.map_err(|e| e.to_string())?;
        if data.len() > 1024 * 1024 {
            return Err("汇率响应过大".into());
        }
        let snapshot = exchange_rates::parse_response(
            &data,
            &source,
            &target,
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        )?;
        // Preserve JSON numeric lexemes; never deserialize exchange rates into f64.
        rates.push(snapshot);
    }
    Ok(rates)
}
fn native_error(message: &str) {
    #[cfg(windows)]
    {
        let title: Vec<u16> = "澄算启动失败".encode_utf16().chain(Some(0)).collect();
        let body: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                title.as_ptr(),
                0x10,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}
pub fn run() {
    let result = (|| -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let root = exe
            .parent()
            .ok_or("无法定位 EXE 目录")?
            .join("ChengSuanData");
        let store = storage::open(root.clone())?;
        tauri::webview_version().map_err(|_|"缺少 Microsoft Edge WebView2 Runtime。请在系统中安装官方 WebView2 Runtime 后重新打开澄算，现有数据不会删除。".to_string())?;
        let mut context = tauri::generate_context!();
        context.config_mut().app.app_directories_override = Some(
            tauri::utils::config::AppDirectoriesOverride::Root(root.clone()),
        );
        let runtime = std::sync::Arc::new(market::MarketRuntime::new(root.clone())?);
        let monitor = runtime.clone();
        let app = tauri::Builder::default()
            .manage(store)
            .manage(runtime.clone())
            .invoke_handler(tauri::generate_handler![
                load_state,
                save_plan,
                delete_plan,
                save_settings,
                import_plans,
                write_export,
                read_project,
                copy_text,
                create_backup,
                restore_backup,
                fetch_rates,
                investment::investment_load,
                investment::investment_save,
                investment::investment_alert_report,
                market::market_search,
                market::market_visible,
                market::market_snapshot,
                market::market_refresh,
                market::market_resolve,
                market::market_fx,
                market::market_candles,
                market::market_secret_status,
                market::market_secret_save
            ])
            .setup(move |app| {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("澄算 ChengSuan")
                .inner_size(1280.0, 860.0)
                .min_inner_size(900.0, 650.0)
                .data_directory(root.join("webview"))
                .build()?;
                monitor.start(app.handle().clone());
                Ok(())
            })
            .build(context)
            .map_err(|e| e.to_string())?;
        app.run(move |_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                runtime.stop();
            }
        });
        Ok(())
    })();
    if let Err(e) = result {
        native_error(&e);
    }
}

#[cfg(test)]
mod live_tests {
    #[test]
    #[ignore = "requires the public exchange rate service"]
    fn live_usd_cny_rate_can_be_saved_and_reloaded() {
        let rates =
            tauri::async_runtime::block_on(super::fetch_rates(vec!["USD".into()], "CNY".into()))
                .unwrap();
        assert_eq!(rates.len(), 1);
        assert!(rates[0]["provider"].as_str().unwrap().chars().count() <= 200);
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let plan = serde_json::json!({"schemaVersion":1,"id":uuid::Uuid::new_v4().to_string(),"name":"汇率联网回归测试","description":"","currency":"CNY","items":[],"rates":rates,"createdAt":now,"updatedAt":now});
        let directory = tempfile::tempdir().unwrap();
        let store = super::storage::open(directory.path().join("data")).unwrap();
        let mut db = store.db.lock().unwrap();
        super::storage::put(&mut db, &[plan.clone()], false).unwrap();
        let (loaded, _) = super::storage::load(&db).unwrap();
        assert_eq!(loaded[0]["rates"], plan["rates"]);
        println!(
            "Actual live snapshot saved and reloaded: {}",
            loaded[0]["rates"]
        );
    }
}
