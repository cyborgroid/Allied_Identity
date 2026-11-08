//! Allied Identity 본체 (allied_core).
//!
//! C# 호스트가 C ABI 로 불러 쓰는 인지 엔진이다. 모든 요청·응답은 UTF-8 JSON 문자열.
//!
//! ```text
//! allied_engine_new(config)      -> 엔진 핸들 (실패 시 NULL, allied_last_error 로 이유 확인)
//! allied_engine_call(h, request) -> {"ok":true,"result":...} | {"ok":false,"error":"..."}
//! allied_engine_free(h)
//! allied_string_free(s)          -> 엔진이 돌려준 모든 문자열은 이것으로 해제
//! allied_version()
//! ```

pub mod calc;
pub mod compose;
pub mod engine;
pub mod forge;
pub mod identity;
pub mod intent;
pub mod knowledge;
pub mod text;

use engine::{Config, Engine, Request};
use serde_json::json;
use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s.replace('\0', "")).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

unsafe fn read_str<'a>(p: *const c_char) -> Result<&'a str, String> {
    if p.is_null() {
        return Err("NULL 문자열".into());
    }
    CStr::from_ptr(p).to_str().map_err(|e| format!("UTF-8 아님: {e}"))
}

/// 엔진 버전 문자열. `allied_string_free` 로 해제한다.
#[no_mangle]
pub extern "C" fn allied_version() -> *mut c_char {
    into_c(env!("CARGO_PKG_VERSION").to_string())
}

/// # Safety
/// `config` 는 NUL 로 끝나는 UTF-8 JSON 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn allied_engine_new(config: *const c_char) -> *mut Engine {
    let result = catch_unwind(|| -> Result<Engine, String> {
        let cfg: Config = serde_json::from_str(read_str(config)?).map_err(|e| format!("설정 JSON: {e}"))?;
        Engine::open(cfg)
    });
    match result {
        Ok(Ok(engine)) => Box::into_raw(Box::new(engine)),
        Ok(Err(e)) => {
            LAST_ERROR.with(|l| *l.borrow_mut() = Some(e));
            std::ptr::null_mut()
        }
        Err(_) => {
            LAST_ERROR.with(|l| *l.borrow_mut() = Some("엔진 초기화 중 패닉".into()));
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `engine` 은 `allied_engine_new` 가 돌려준 유효한 핸들, `request` 는 UTF-8 JSON 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn allied_engine_call(engine: *mut Engine, request: *const c_char) -> *mut c_char {
    let reply = match engine.as_mut() {
        None => json!({"ok": false, "error": "NULL 엔진 핸들"}),
        Some(engine) => {
            let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
                let req: Request = serde_json::from_str(read_str(request)?).map_err(|e| format!("요청 JSON: {e}"))?;
                engine.handle(req)
            }));
            match outcome {
                Ok(Ok(result)) => json!({"ok": true, "result": result}),
                Ok(Err(e)) => json!({"ok": false, "error": e}),
                Err(_) => json!({"ok": false, "error": "요청 처리 중 패닉"}),
            }
        }
    };
    into_c(reply.to_string())
}

/// # Safety
/// `engine` 은 `allied_engine_new` 가 돌려준 핸들이거나 NULL 이어야 하며, 한 번만 해제한다.
#[no_mangle]
pub unsafe extern "C" fn allied_engine_free(engine: *mut Engine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// # Safety
/// `s` 는 이 라이브러리가 돌려준 문자열이거나 NULL 이어야 하며, 한 번만 해제한다.
#[no_mangle]
pub unsafe extern "C" fn allied_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// 마지막 `allied_engine_new` 실패 이유. 없으면 NULL. `allied_string_free` 로 해제한다.
#[no_mangle]
pub extern "C" fn allied_last_error() -> *mut c_char {
    LAST_ERROR.with(|l| l.borrow_mut().take()).map(into_c).unwrap_or(std::ptr::null_mut())
}
