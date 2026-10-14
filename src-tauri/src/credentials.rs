//! Access tokens belong to the operating-system credential store, never the webview or SQLite.
use crate::domain::AppError;
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(test)]
use std::sync::Mutex;
use zeroize::Zeroizing;

pub(crate) struct Credentials {
    target: String,
    #[cfg(test)]
    memory: Option<Mutex<Option<Zeroizing<String>>>>,
}
impl Credentials {
    pub fn new(directory: &Path) -> Self {
        Self {
            // Separate native-test profiles without accessing the user's real credential.
            target: format!(
                "ChessPush/Lichess/{:x}",
                Sha256::digest(directory.to_string_lossy().as_bytes())
            ),
            #[cfg(test)]
            memory: None,
        }
    }
    #[cfg(test)]
    pub fn memory() -> Self {
        Self {
            target: String::new(),
            memory: Some(Mutex::new(None)),
        }
    }
    #[cfg(test)]
    pub fn poison_for_test(&self) {
        // Simulate the same unavailable-store error without touching Windows credentials.
        let memory = self.memory.as_ref().expect("test memory store");
        let _ = std::panic::catch_unwind(|| {
            let _guard = memory.lock().unwrap();
            panic!("synthetic credential-store failure");
        });
    }
    pub fn read(&self) -> Result<Option<Zeroizing<String>>, AppError> {
        #[cfg(test)]
        if let Some(memory) = &self.memory {
            return Ok(memory.lock().map_err(|_| unavailable())?.clone());
        }
        platform::read(&self.target)
    }
    pub fn save(&self, token: &str) -> Result<(), AppError> {
        #[cfg(test)]
        if let Some(memory) = &self.memory {
            *memory.lock().map_err(|_| unavailable())? = Some(Zeroizing::new(token.to_owned()));
            return Ok(());
        }
        platform::save(&self.target, token)
    }
    pub fn delete(&self) -> Result<(), AppError> {
        #[cfg(test)]
        if let Some(memory) = &self.memory {
            *memory.lock().map_err(|_| unavailable())? = None;
            return Ok(());
        }
        platform::delete(&self.target)
    }
}
fn unavailable() -> AppError {
    AppError::new("credentials", "Хранилище учётных данных недоступно")
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::{ERROR_NOT_FOUND, GetLastError},
        Security::Credentials::*,
    };
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    pub fn read(target: &str) -> Result<Option<Zeroizing<String>>, AppError> {
        let name = wide(target);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        // Windows allocates this credential; copy only its blob and free it on every path.
        if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(unavailable())
            };
        }
        let result = unsafe {
            let blob = &*credential;
            if blob.CredentialBlobSize == 0
                || blob.CredentialBlobSize > 2560
                || blob.CredentialBlob.is_null()
            {
                Err(unavailable())
            } else {
                String::from_utf8(
                    std::slice::from_raw_parts(
                        blob.CredentialBlob,
                        blob.CredentialBlobSize as usize,
                    )
                    .to_vec(),
                )
                .map(Zeroizing::new)
                .map_err(|_| unavailable())
            }
        };
        unsafe {
            CredFree(credential.cast());
        }
        result.map(Some)
    }
    pub fn save(target: &str, token: &str) -> Result<(), AppError> {
        if token.is_empty() || token.len() > 2048 {
            return Err(unavailable());
        }
        let mut name = wide(target);
        let mut user = wide("Lichess");
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: token.len() as u32,
            CredentialBlob: token.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user.as_mut_ptr(),
            ..Default::default()
        };
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            Err(unavailable())
        } else {
            Ok(())
        }
    }
    pub fn delete(target: &str) -> Result<(), AppError> {
        let name = wide(target);
        if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0
            || unsafe { GetLastError() } == ERROR_NOT_FOUND
        {
            Ok(())
        } else {
            Err(unavailable())
        }
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    // No plaintext fallback. A native keychain adapter is required before enabling other platforms.
    pub fn read(_: &str) -> Result<Option<Zeroizing<String>>, AppError> {
        Err(unavailable())
    }
    pub fn save(_: &str, _: &str) -> Result<(), AppError> {
        Err(unavailable())
    }
    pub fn delete(_: &str) -> Result<(), AppError> {
        Err(unavailable())
    }
}
