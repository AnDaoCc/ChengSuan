use serde_json::{json, Value};
use std::{fs, path::Path};

pub fn read(root: &Path) -> Result<Value, String> {
    let file = root.join("investment-secrets.bin");
    if !file.exists() {
        return Ok(json!({}));
    }
    let encrypted = fs::read(file).map_err(|_| "无法读取行情授权配置")?;
    let bytes = unprotect(&encrypted)?;
    serde_json::from_slice(&bytes).map_err(|_| "行情授权配置无效，请重新填写".into())
}
pub fn save(root: &Path, patch: &Value) -> Result<Value, String> {
    let mut values = read(root).unwrap_or_else(|_| json!({}));
    for key in ["allTick", "coinGecko"] {
        if let Some(value) = patch.get(key) {
            let token = value.as_str().ok_or("密钥必须为文本")?.trim();
            if token.len() > 2048 || token.chars().any(|c| c.is_control()) {
                return Err("密钥格式无效".into());
            }
            if token.is_empty() {
                values.as_object_mut().unwrap().remove(key);
            } else {
                values[key] = json!(token);
            }
        }
    }
    let encrypted = protect(values.to_string().as_bytes())?;
    let temporary = root.join("investment-secrets.tmp");
    fs::write(&temporary, encrypted).map_err(|_| "无法写入行情授权配置")?;
    super::replace_file(&temporary, &root.join("investment-secrets.bin"))
        .map_err(|_| "无法保存行情授权配置")?;
    Ok(status(&values))
}
pub fn status(values: &Value) -> Value {
    json!({"allTick":values["allTick"].as_str().is_some_and(|v|!v.is_empty()),"coinGecko":values["coinGecko"].as_str().is_some_and(|v|!v.is_empty())})
}
#[cfg(windows)]
fn protect(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    if unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    } == 0
    {
        return Err("Windows 无法加密行情密钥".into());
    }
    let data = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(out.pbData as *mut _);
    }
    Ok(data)
}
#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    if unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    } == 0
    {
        return Err("行情密钥无法在当前 Windows 用户下解密，请重新填写；持仓数据仍保留".into());
    }
    let data = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(out.pbData as *mut _);
    }
    Ok(data)
}
#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("此版本的密钥存储需要 Windows DPAPI".into())
}
#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("此版本的密钥存储需要 Windows DPAPI".into())
}
#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn secrets_roundtrip_and_do_not_store_plaintext() {
        let root = tempfile::tempdir().unwrap();
        super::save(
            root.path(),
            &serde_json::json!({"allTick":"test-secret-value"}),
        )
        .unwrap();
        let bytes = std::fs::read(root.path().join("investment-secrets.bin")).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("test-secret-value"));
        assert_eq!(
            super::read(root.path()).unwrap()["allTick"],
            "test-secret-value"
        );
        assert_eq!(
            super::status(&super::read(root.path()).unwrap())["allTick"],
            true
        );
    }
}
