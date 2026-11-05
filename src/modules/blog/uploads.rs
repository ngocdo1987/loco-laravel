//! Upload endpoints — port of the upload API in
//! `goravel_base/app/modules/blog` (`UploadImage`, `UploadPostCover`,
//! `UploadOwnPostCover`). Behavioral contract:
//!
//! - multipart field `image` (or `cover_image` for the post-cover endpoint);
//!   a missing/unreadable form is a 422 "is required" validation error,
//!   matching goravel's `uploadRequiredImage` (`File` errors read as
//!   "not uploaded");
//! - the type is sniffed from magic bytes (goravel detects from content):
//!   jpeg, png, gif or webp, else 422 — checked **before** size like goravel;
//! - max 5 MB, else 422;
//! - files are stored through loco `ctx.storage` (local driver rooted at
//!   `storage/app/public`, wired in `after_context`) under `uploads/images`
//!   or `uploads/post-covers` with a random uuid name;
//! - the response exposes `path` (storage key) and `url`
//!   (`{proto}://{host}/storage/{path}`, honouring X-Forwarded-Proto/Host).

use loco_rs::prelude::*;

use axum::extract::Multipart;
use axum::http::{HeaderMap, StatusCode};

use super::{author, controllers, models::post};
use crate::models::_entities::users;

type User = auth::JWTWithUser<users::Model>;
type MultipartOrRejection =
    std::result::Result<Multipart, axum::extract::multipart::MultipartRejection>;

const MAX_BYTES: usize = 5 * 1024 * 1024;

/// Sniff the allowed image types from magic bytes, returning the extension
/// used for the stored key (content-based, like goravel's detection).
fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// goravel's `publicStorageURL`: X-Forwarded-Proto (default http) +
/// X-Forwarded-Host or Host header, then `/storage/{path}`.
fn public_url(headers: &HeaderMap, path: &str) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get(axum::http::header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .unwrap_or("localhost");
    format!("{proto}://{host}/storage/{path}")
}

#[derive(Debug)]
pub(super) enum UploadError {
    Validation(&'static str, &'static str),
    Failed,
}

impl UploadError {
    fn response(self) -> Response {
        match self {
            Self::Validation(field, message) => controllers::validation(field, message),
            Self::Failed => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(data!({"message": "Unable to process blog request"})),
            )
                .into_response(),
        }
    }
}

/// Validates content-sniffed type + size and stores the bytes through loco
/// storage (shared by the upload API and the dashboard's form cover upload).
pub(super) async fn store_image(
    ctx: &AppContext,
    headers: &HeaderMap,
    directory: &str,
    field: &'static str,
    bytes: &axum::body::Bytes,
) -> std::result::Result<(String, String), UploadError> {
    let Some(extension) = sniff_image(bytes) else {
        return Err(UploadError::Validation(
            field,
            "must be a jpeg, png, gif, or webp image",
        ));
    };
    if bytes.len() > MAX_BYTES {
        return Err(UploadError::Validation(field, "must not exceed 5 MB"));
    }
    let path = format!("{directory}/{}.{extension}", uuid::Uuid::new_v4());
    if let Err(storage_error) = ctx.storage.upload(std::path::Path::new(&path), bytes).await {
        tracing::error!(error = %storage_error, path, "upload failed");
        return Err(UploadError::Failed);
    }
    Ok((public_url(headers, &path), path))
}

async fn upload_image(
    ctx: &AppContext,
    headers: &HeaderMap,
    field: &'static str,
    directory: &str,
    multipart: MultipartOrRejection,
) -> std::result::Result<(String, String), UploadError> {
    let error = |field: &'static str, err: &axum::extract::multipart::MultipartError| {
        if err.body_text() == "Request payload is too large" {
            UploadError::Validation(field, "must not exceed 5 MB")
        } else {
            UploadError::Validation(field, "must be a readable image file")
        }
    };
    let mut multipart = multipart.map_err(|_| UploadError::Validation(field, "is required"))?;
    let mut payload = None;
    while let Some(part) = multipart
        .next_field()
        .await
        .map_err(|err| error(field, &err))?
    {
        if part.name() == Some(field) {
            payload = Some(part.bytes().await.map_err(|err| error(field, &err))?);
            break;
        }
    }
    let Some(bytes) = payload else {
        return Err(UploadError::Validation(field, "is required"));
    };
    store_image(ctx, headers, directory, field, &bytes).await
}

/// `POST /api/uploads/images` — generic image upload.
#[debug_handler]
pub(super) async fn image(
    _user: User,
    State(ctx): State<AppContext>,
    headers: HeaderMap,
    multipart: MultipartOrRejection,
) -> Result<Response> {
    let (url, path) = match upload_image(&ctx, &headers, "image", "uploads/images", multipart).await
    {
        Ok(stored) => stored,
        Err(error) => return Ok(error.response()),
    };
    format::render()
        .status(StatusCode::CREATED)
        .json(data!({"url": url, "path": path}))
}

/// `POST /api/uploads/post-covers` — standalone cover upload.
#[debug_handler]
pub(super) async fn post_cover(
    _user: User,
    State(ctx): State<AppContext>,
    headers: HeaderMap,
    multipart: MultipartOrRejection,
) -> Result<Response> {
    let (url, path) =
        match upload_image(&ctx, &headers, "image", "uploads/post-covers", multipart).await {
            Ok(stored) => stored,
            Err(error) => return Ok(error.response()),
        };
    format::render()
        .status(StatusCode::CREATED)
        .json(data!({"url": url, "path": path}))
}

/// `POST /api/me/posts/{id}/cover` — upload a cover and set it on an owned
/// post (404 when the post is not the caller's).
#[debug_handler]
pub(super) async fn cover(
    user: User,
    State(ctx): State<AppContext>,
    headers: HeaderMap,
    Path(id): Path<String>,
    multipart: MultipartOrRejection,
) -> Result<Response> {
    let (url, path) = match upload_image(
        &ctx,
        &headers,
        "cover_image",
        "uploads/post-covers",
        multipart,
    )
    .await
    {
        Ok(stored) => stored,
        Err(error) => return Ok(error.response()),
    };
    let row = match author::update_cover(&ctx.db, user.user.id, id.parse().unwrap_or(0), &url).await
    {
        Ok(row) => row,
        Err(error) => return error.response(),
    };
    let response = controllers::public_post(post::Model::load_relations(&ctx.db, row).await?);
    format::render().status(StatusCode::CREATED).json(data!({
        "message": "Post cover updated successfully",
        "url": url,
        "path": path,
        "post": response
    }))
}
