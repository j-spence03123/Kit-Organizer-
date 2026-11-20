//! `kitaudio://` — streams previews to the webview.
//!
//! Every supported format is decoded with Symphonia and served as a 16-bit
//! WAV, so playback works identically on every platform's webview. Only
//! files recorded in the database (imported sources or organised
//! destinations) can be read, which keeps the protocol from becoming a
//! general file-read primitive.

use crate::state::AppState;
use percent_encoding::percent_decode_str;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

const MAX_PREVIEW_SECS: f64 = 180.0;

fn respond(code: StatusCode, body: Vec<u8>, ctype: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, ctype)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

pub fn handle<R: Runtime>(ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let raw = request.uri().path().trim_start_matches('/').to_string();
    std::thread::spawn(move || {
        let path = match percent_decode_str(&raw).decode_utf8() {
            Ok(p) => p.to_string(),
            Err(_) => return responder.respond(respond(StatusCode::BAD_REQUEST, b"bad path".to_vec(), "text/plain")),
        };
        let state = app.state::<AppState>();
        let known = state.db().is_known_path(&path).unwrap_or(false);
        if !known {
            return responder.respond(respond(StatusCode::FORBIDDEN, b"not a known sample".to_vec(), "text/plain"));
        }
        match kit_core::audio::decode_to_wav(std::path::Path::new(&path), MAX_PREVIEW_SECS) {
            Ok(wav) => responder.respond(respond(StatusCode::OK, wav, "audio/wav")),
            Err(e) => responder.respond(respond(StatusCode::UNPROCESSABLE_ENTITY, e.to_string().into_bytes(), "text/plain")),
        }
    });
}
