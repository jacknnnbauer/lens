//! 在线词典的 API Key 存在 Windows 凭据管理器里（SPEC 13），不写进 settings.json。
//! 目标名 `Lens/online/<词典 id>`；App ID + Key 两段式的存成 `id\nkey`。

use crate::error::{Error, Result};

use super::{Credential, KeyKind};

fn target(dict_id: &str) -> String {
    format!("Lens/online/{dict_id}")
}

/// 把存储的字符串拆成凭据。
pub fn split(stored: &str, kind: KeyKind) -> Result<Credential> {
    match kind {
        KeyKind::IdAndKey => {
            let (id, key) = stored
                .split_once('\n')
                .ok_or_else(|| Error::Online(crate::i18n::t("凭据不完整，请重新填写 App ID 和 App Key", "Credentials are incomplete. Please re-enter the App ID and App Key").into()))?;
            Ok(Credential { id: Some(id.trim().to_string()), key: key.trim().to_string() })
        }
        _ => Ok(Credential { id: None, key: stored.trim().to_string() }),
    }
}

pub fn join(id: Option<&str>, key: &str) -> String {
    match id {
        Some(id) => format!("{}\n{}", id.trim(), key.trim()),
        None => key.trim().to_string(),
    }
}

#[cfg(windows)]
mod imp {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    use crate::error::{Error, Result};

    fn not_found(e: &windows::core::Error) -> bool {
        e.code() == ERROR_NOT_FOUND.to_hresult()
    }

    pub fn read(target: &str) -> Result<Option<String>> {
        let name = HSTRING::from(target);
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: CredReadW 成功时写入一个由系统分配的 CREDENTIALW，读完后用 CredFree 释放。
        unsafe {
            if let Err(e) = CredReadW(&name, CRED_TYPE_GENERIC, None, &mut cred) {
                return if not_found(&e) { Ok(None) } else { Err(Error::Online(crate::tf!("读取凭据失败：{}", "Failed to read the credential: {}", e))) };
            }
            let c = &*cred;
            let blob = if c.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
            };
            CredFree(cred as *const _);
            Ok(String::from_utf8(blob).ok().filter(|s| !s.trim().is_empty()))
        }
    }

    pub fn write(target: &str, secret: &str) -> Result<()> {
        let mut name: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
        let mut user: Vec<u16> = "Lens".encode_utf16().chain(Some(0)).collect();
        let mut blob = secret.as_bytes().to_vec();
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(name.as_mut_ptr()),
            UserName: PWSTR(user.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: cred 引用的缓冲区在调用期间有效；系统会复制内容。
        unsafe { CredWriteW(&cred, 0) }.map_err(|e| Error::Online(crate::tf!("保存凭据失败：{}", "Failed to save the credential: {}", e)))
    }

    pub fn delete(target: &str) -> Result<()> {
        let name = HSTRING::from(target);
        // SAFETY: 只传入以 0 结尾的目标名。
        match unsafe { CredDeleteW(&name, CRED_TYPE_GENERIC, None) } {
            Ok(()) => Ok(()),
            Err(e) if not_found(&e) => Ok(()),
            Err(e) => Err(Error::Online(crate::tf!("删除凭据失败：{}", "Failed to delete the credential: {}", e))),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use crate::error::{Error, Result};

    pub fn read(_: &str) -> Result<Option<String>> {
        Ok(None)
    }
    pub fn write(_: &str, _: &str) -> Result<()> {
        Err(Error::Online(crate::i18n::t("当前平台不支持保存凭据", "Saving credentials is not supported on this platform").into()))
    }
    pub fn delete(_: &str) -> Result<()> {
        Ok(())
    }
}

pub fn read(dict_id: &str) -> Result<Option<String>> {
    imp::read(&target(dict_id))
}

pub fn write(dict_id: &str, secret: &str) -> Result<()> {
    imp::write(&target(dict_id), secret)
}

pub fn delete(dict_id: &str) -> Result<()> {
    imp::delete(&target(dict_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_join() -> Result<()> {
        assert_eq!(split(" abc ", KeyKind::Key)?, Credential { id: None, key: "abc".into() });
        let s = join(Some(" id1 "), " k1 ");
        assert_eq!(split(&s, KeyKind::IdAndKey)?, Credential { id: Some("id1".into()), key: "k1".into() });
        assert!(split("only-key", KeyKind::IdAndKey).is_err());
        Ok(())
    }

    /// 真正读写凭据管理器；默认不跑（会在本机凭据管理器里短暂留下一条记录）。
    #[test]
    #[ignore]
    fn credential_manager_roundtrip() -> Result<()> {
        let id = "test-roundtrip";
        write(id, "secret\nvalue")?;
        assert_eq!(read(id)?.as_deref(), Some("secret\nvalue"));
        delete(id)?;
        assert_eq!(read(id)?, None);
        Ok(())
    }
}
