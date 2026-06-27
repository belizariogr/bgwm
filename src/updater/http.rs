//! Minimal synchronous HTTP(S) GET client built on WinHTTP.
//!
//! Used by the updater to query the GitHub releases API and download the
//! installer. Native WinHTTP is preferred over a third-party HTTP/TLS stack so
//! the app stays dependency-light and uses the OS trust store and proxy config.

use std::ffi::c_void;

use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_INTERNET_SCHEME_HTTPS, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

const READ_CHUNK: usize = 16 * 1024;
const USER_AGENT: &str = concat!("bgwm-updater/", env!("CARGO_PKG_VERSION"));

/// Performs an HTTPS GET and returns the response body.
///
/// WinHTTP transparently follows redirects (used by GitHub asset downloads).
pub fn get(url: &str, accept: Option<&str>) -> Result<Vec<u8>, String> {
    let agent = wide(USER_AGENT);
    unsafe {
        let session = WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            return Err(format!("WinHttpOpen failed: {}", last_error()));
        }
        let result = with_session(session, url, accept);
        let _ = WinHttpCloseHandle(session);
        result
    }
}

unsafe fn with_session(
    session: *mut c_void,
    url: &str,
    accept: Option<&str>,
) -> Result<Vec<u8>, String> {
    let url_w: Vec<u16> = url.encode_utf16().collect();
    let mut comp = std::mem::zeroed::<URL_COMPONENTS>();
    comp.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
    comp.dwSchemeLength = u32::MAX;
    comp.dwHostNameLength = u32::MAX;
    comp.dwUrlPathLength = u32::MAX;
    comp.dwExtraInfoLength = u32::MAX;
    WinHttpCrackUrl(&url_w, 0, &mut comp).map_err(|e| format!("WinHttpCrackUrl failed: {e}"))?;

    let host = pwstr_to_wide(comp.lpszHostName.0, comp.dwHostNameLength);
    let mut path = pwstr_to_wide(
        comp.lpszUrlPath.0,
        comp.dwUrlPathLength.saturating_add(comp.dwExtraInfoLength),
    );
    if path.len() <= 1 {
        path = wide("/");
    }
    let secure = comp.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS;
    let port = comp.nPort;

    let connect = WinHttpConnect(session, PCWSTR(host.as_ptr()), port, 0);
    if connect.is_null() {
        return Err(format!("WinHttpConnect failed: {}", last_error()));
    }
    let result = with_connection(connect, &path, secure, accept);
    let _ = WinHttpCloseHandle(connect);
    result
}

unsafe fn with_connection(
    connect: *mut c_void,
    path: &[u16],
    secure: bool,
    accept: Option<&str>,
) -> Result<Vec<u8>, String> {
    let flags = if secure {
        WINHTTP_FLAG_SECURE
    } else {
        WINHTTP_OPEN_REQUEST_FLAGS(0)
    };
    let verb = wide("GET");
    let request = WinHttpOpenRequest(
        connect,
        PCWSTR(verb.as_ptr()),
        PCWSTR(path.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        std::ptr::null(),
        flags,
    );
    if request.is_null() {
        return Err(format!("WinHttpOpenRequest failed: {}", last_error()));
    }
    let result = exchange(request, accept);
    let _ = WinHttpCloseHandle(request);
    result
}

unsafe fn exchange(request: *mut c_void, accept: Option<&str>) -> Result<Vec<u8>, String> {
    let headers: Option<Vec<u16>> =
        accept.map(|value| format!("Accept: {value}").encode_utf16().collect());

    WinHttpSendRequest(request, headers.as_deref(), None, 0, 0, 0)
        .map_err(|e| format!("WinHttpSendRequest failed: {e}"))?;
    WinHttpReceiveResponse(request, std::ptr::null_mut())
        .map_err(|e| format!("WinHttpReceiveResponse failed: {e}"))?;

    let status = query_status_code(request)?;
    if status != 200 {
        return Err(format!("unexpected HTTP status {status}"));
    }

    let mut out = Vec::new();
    let mut buffer = vec![0u8; READ_CHUNK];
    loop {
        let mut read: u32 = 0;
        WinHttpReadData(
            request,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
            &mut read,
        )
        .map_err(|e| format!("WinHttpReadData failed: {e}"))?;
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buffer[..read as usize]);
    }
    Ok(out)
}

unsafe fn query_status_code(request: *mut c_void) -> Result<u32, String> {
    let mut status: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let mut index: u32 = 0;
    WinHttpQueryHeaders(
        request,
        WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
        PCWSTR::null(),
        Some(&mut status as *mut u32 as *mut c_void),
        &mut size,
        &mut index,
    )
    .map_err(|e| format!("WinHttpQueryHeaders failed: {e}"))?;
    Ok(status)
}

/// Copies a counted, non-null-terminated `PWSTR` substring into an owned,
/// null-terminated wide string.
unsafe fn pwstr_to_wide(ptr: *const u16, len: u32) -> Vec<u16> {
    if ptr.is_null() || len == 0 {
        return vec![0];
    }
    let slice = std::slice::from_raw_parts(ptr, len as usize);
    let mut owned = slice.to_vec();
    owned.push(0);
    owned
}

fn last_error() -> windows::core::Error {
    windows::core::Error::from_win32()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
