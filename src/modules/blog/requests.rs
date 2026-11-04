use loco_rs::prelude::*;

use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashSet;

use super::controllers::validation;

#[derive(Debug)]
pub(super) enum InputError {
    Payload,
    Field(&'static str, &'static str),
}

impl InputError {
    pub(super) fn response(self) -> Response {
        match self {
            Self::Payload => (
                axum::http::StatusCode::BAD_REQUEST,
                Json(data!({"message": "Invalid request payload"})),
            )
                .into_response(),
            Self::Field(field, message) => validation(field, message),
        }
    }
}

// Go's JSON binding treats null strings as their zero value.
fn null_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct PostInput {
    #[serde(deserialize_with = "null_string")]
    pub(super) title: String,
    #[serde(deserialize_with = "null_string")]
    pub(super) slug: String,
    #[serde(deserialize_with = "null_string")]
    pub(super) excerpt: String,
    #[serde(deserialize_with = "null_string")]
    pub(super) cover_image_url: String,
    #[serde(deserialize_with = "null_string")]
    pub(super) content_html: String,
    #[serde(deserialize_with = "null_string")]
    pub(super) status: String,
    pub(super) category_id: Option<i64>,
    pub(super) tag_ids: Option<Vec<i64>>,
    #[serde(skip)]
    pub(super) fields: HashSet<String>,
}

fn trim(value: &mut String) {
    value.truncate(value.trim_end().len());
    let leading = value.len() - value.trim_start().len();
    value.drain(..leading);
}

impl PostInput {
    pub(super) fn parse(value: Value, create: bool) -> std::result::Result<Self, InputError> {
        let fields: HashSet<_> = value
            .as_object()
            .ok_or(InputError::Payload)?
            .keys()
            .cloned()
            .collect();
        if !create && fields.is_empty() {
            return Err(InputError::Field(
                "payload",
                "at least one field is required",
            ));
        }
        let mut input: Self = serde_json::from_value(value).map_err(|_| InputError::Payload)?;
        input.fields = fields;
        trim(&mut input.title);
        input.slug = input.slug.trim().to_lowercase();
        trim(&mut input.excerpt);
        trim(&mut input.cover_image_url);
        for (field, value, limit) in [
            ("title", &input.title, 255),
            ("content_html", &input.content_html, 100_000),
            ("slug", &input.slug, 255),
            ("excerpt", &input.excerpt, 500),
        ] {
            if (create || input.fields.contains(field)) && value.chars().count() > limit {
                return Err(InputError::Field(field, "exceeds the maximum length"));
            }
            if (create || input.fields.contains(field))
                && matches!(field, "title" | "content_html")
                && value.trim().is_empty()
            {
                return Err(InputError::Field(field, "is required"));
            }
        }
        if create && input.status.is_empty() {
            "draft".clone_into(&mut input.status);
        }
        if (create || input.fields.contains("status"))
            && !matches!(input.status.as_str(), "draft" | "published")
        {
            return Err(InputError::Field("status", "must be draft or published"));
        }
        if !input.cover_image_url.is_empty() {
            let valid = input
                .cover_image_url
                .parse::<axum::http::Uri>()
                .is_ok_and(|uri| {
                    matches!(uri.scheme_str(), Some("http" | "https"))
                        && uri.host().is_some_and(|host| !host.is_empty())
                });
            if !valid || input.cover_image_url.len() > 2048 {
                return Err(InputError::Field(
                    "cover_image_url",
                    "must be a valid http or https URL no longer than 2048 characters",
                ));
            }
        }
        let tags = input.tag_ids.as_deref().unwrap_or_default();
        if tags.len() > 20 {
            return Err(InputError::Field(
                "tag_ids",
                "must contain no more than 20 tags",
            ));
        }
        let mut seen = HashSet::new();
        if tags.iter().any(|id| *id <= 0 || !seen.insert(*id)) {
            return Err(InputError::Field(
                "tag_ids",
                "must contain unique valid tag identifiers",
            ));
        }
        Ok(input)
    }
}
