// Copyright (c) 2026, Nathaniel T. Berry
// Licensed under the Apache License, Version 2.0.

//! Image generation from a brand file's `image` block.
//!
//! The style text is brand content. This module does not supply a default
//! style. Model, aspect, and quality fall back only when the brand omits them.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};

use crate::error::{ImageError, LlmError};
use crate::llm::{redact_secrets, validate_base_url};
use crate::models::Brand;

const HTTP_TIMEOUT_SECS: u64 = 120;

const ASPECTS: &[&str] = &[
    "1:1", "1:2", "16:9", "19.5:9", "2:1", "2:3", "20:9", "21:9", "3:2", "3:4", "4:3", "5:2",
    "9:16", "9:19.5", "9:20", "auto",
];

const QUALITIES: &[&str] = &["low", "medium", "auto"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRequest {
    pub model: String,
    pub prompt: String,
    pub aspect: String,
    pub quality: String,
}

/// CLI overrides. Empty means "use the brand file".
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageOverrides<'a> {
    pub aspect: Option<&'a str>,
    pub quality: Option<&'a str>,
    pub model: Option<&'a str>,
}

pub trait ImageClient {
    fn create_image(&self, req: &ImageRequest) -> Result<Vec<u8>, ImageError>;
}

enum ParsedImage {
    Bytes(Vec<u8>),
    Url(String),
}

fn known_aspects() -> String {
    ASPECTS.join(", ")
}

fn pick<'a>(override_value: Option<&'a str>, brand_value: &'a str, fallback: &'a str) -> &'a str {
    if let Some(value) = override_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return value;
    }
    let brand_value = brand_value.trim();
    if brand_value.is_empty() {
        fallback
    } else {
        brand_value
    }
}

/// Build the request from the brand file plus optional CLI overrides.
pub fn prepare_image_request(
    brand: &Brand,
    scene: &str,
    aspect: Option<&str>,
    quality: Option<&str>,
    model: Option<&str>,
) -> Result<ImageRequest, ImageError> {
    let style = brand.image.style.trim();
    if style.is_empty() {
        return Err(ImageError::MissingStyle {
            brand_id: brand.id.clone(),
        });
    }
    let scene = scene.trim();
    if scene.is_empty() {
        return Err(ImageError::EmptyScene);
    }
    let aspect = pick(aspect, &brand.image.aspect, "16:9").to_string();
    if !ASPECTS.contains(&aspect.as_str()) {
        return Err(ImageError::BadAspect {
            aspect,
            known: known_aspects(),
        });
    }
    let quality = pick(quality, &brand.image.quality, "low").to_string();
    if !QUALITIES.contains(&quality.as_str()) {
        return Err(ImageError::BadQuality);
    }
    let model = pick(model, &brand.image.model, "grok-imagine-image-2.0").to_string();
    Ok(ImageRequest {
        model,
        prompt: format!("{style}\n\nScene: {scene}"),
        aspect,
        quality,
    })
}

pub fn image_body(req: &ImageRequest) -> Value {
    json!({
        "model": req.model,
        "prompt": req.prompt,
        "aspect_ratio": req.aspect,
        "response_format": "b64_json",
        "quality": req.quality,
        "n": 1,
    })
}

fn truncate(text: &str) -> String {
    let mut end = 500.min(text.len());
    while !text.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    text[..end].to_string()
}

fn parse_payload(payload: &Value) -> Result<ParsedImage, ImageError> {
    let Some(data) = payload.get("data").and_then(Value::as_array) else {
        let message = payload
            .get("error")
            .map(ToString::to_string)
            .unwrap_or_else(|| payload.to_string());
        return Err(ImageError::EmptyPayload(truncate(&message)));
    };
    if data.is_empty() {
        let message = payload
            .get("error")
            .map(ToString::to_string)
            .unwrap_or_else(|| payload.to_string());
        return Err(ImageError::EmptyPayload(truncate(&message)));
    }
    let item = &data[0];
    if let Some(encoded) = item.get("b64_json").and_then(Value::as_str) {
        let bytes = B64.decode(encoded.trim()).map_err(|err| {
            ImageError::Message(format!("image API returned invalid base64: {err}"))
        })?;
        return Ok(ParsedImage::Bytes(bytes));
    }
    if let Some(url) = item.get("url").and_then(Value::as_str) {
        if !url.is_empty() {
            return Ok(ParsedImage::Url(url.to_string()));
        }
    }
    Err(ImageError::NoImage)
}

/// Write one image. `client` performs the HTTP call so tests never hit the API.
pub fn render_image(
    brand: &Brand,
    scene: &str,
    dest: &Path,
    aspect: Option<&str>,
    quality: Option<&str>,
    model: Option<&str>,
    client: &dyn ImageClient,
) -> Result<PathBuf, ImageError> {
    let req = prepare_image_request(brand, scene, aspect, quality, model)?;
    let bytes = client.create_image(&req)?;
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(dest, bytes)?;
    Ok(dest.to_path_buf())
}

pub struct XaiImageClient {
    api_key: String,
    base_url: String,
    http: reqwest::blocking::Client,
}

impl XaiImageClient {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Result<Self, ImageError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ImageError::Message(
                "XAI_API_KEY is not set. Export it or add it to a git-ignored .env file."
                    .to_string(),
            ));
        }
        let base_url = base_url.into();
        validate_base_url(&base_url).map_err(|err| match err {
            LlmError::Http(message) => ImageError::Message(message),
        })?;
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .use_rustls_tls()
            .build()
            .map_err(|err| ImageError::Message(redact_secrets(&err.to_string())))?;
        Ok(Self {
            api_key,
            base_url,
            http,
        })
    }
}

impl ImageClient for XaiImageClient {
    fn create_image(&self, req: &ImageRequest) -> Result<Vec<u8>, ImageError> {
        let url = format!("{}/images/generations", self.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&image_body(req))
            .send()
            .map_err(|err| ImageError::Message(redact_secrets(&err.to_string())))?;
        let status = response.status();
        let value: Value = response.json().map_err(|err| {
            ImageError::Message(redact_secrets(&format!("status {status}: {err}")))
        })?;
        if !status.is_success() {
            return Err(ImageError::Http {
                status: status.as_u16(),
                detail: truncate(&redact_secrets(&value.to_string())),
            });
        }
        match parse_payload(&value)? {
            ParsedImage::Bytes(bytes) => Ok(bytes),
            ParsedImage::Url(image_url) => {
                let fetched = self
                    .http
                    .get(&image_url)
                    .send()
                    .map_err(|err| ImageError::Message(redact_secrets(&err.to_string())))?;
                let fetched_status = fetched.status();
                if !fetched_status.is_success() {
                    return Err(ImageError::Http {
                        status: fetched_status.as_u16(),
                        detail: "image URL fetch failed".to_string(),
                    });
                }
                let bytes = fetched
                    .bytes()
                    .map_err(|err| ImageError::Message(redact_secrets(&err.to_string())))?;
                Ok(bytes.to_vec())
            }
        }
    }
}

pub fn render_image_with_xai(
    brand: &Brand,
    scene: &str,
    dest: &Path,
    overrides: ImageOverrides<'_>,
    api_key: &str,
    base_url: &str,
) -> Result<PathBuf, ImageError> {
    // Validate the brand and the scene before asking for a key or the network.
    prepare_image_request(
        brand,
        scene,
        overrides.aspect,
        overrides.quality,
        overrides.model,
    )?;
    let client = XaiImageClient::new(api_key, base_url)?;
    render_image(
        brand,
        scene,
        dest,
        overrides.aspect,
        overrides.quality,
        overrides.model,
        &client,
    )
}

/// Test double. Records the request and returns a fixed byte string.
#[derive(Debug)]
pub struct ScriptedImageClient {
    bytes: Vec<u8>,
    calls: Mutex<Vec<ImageRequest>>,
}

impl ScriptedImageClient {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<ImageRequest> {
        self.calls.lock().expect("scripted image calls").clone()
    }
}

impl ImageClient for ScriptedImageClient {
    fn create_image(&self, req: &ImageRequest) -> Result<Vec<u8>, ImageError> {
        self.calls
            .lock()
            .expect("scripted image calls")
            .push(req.clone());
        Ok(self.bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ImageSpec;

    fn brand_with_style(style: &str) -> Brand {
        Brand {
            id: "lab".into(),
            display_name: "Lab".into(),
            voice: "plain".into(),
            full_name: "Lab".into(),
            short_name: "Lab".into(),
            forbidden: Vec::new(),
            disclaimer: String::new(),
            types: Default::default(),
            tones: Default::default(),
            default_tone: String::new(),
            formats: Default::default(),
            image: ImageSpec {
                style: style.into(),
                ..ImageSpec::default()
            },
        }
    }

    #[test]
    fn prompt_uses_brand_style_and_rejects_a_blank_style() {
        let brand = brand_with_style("Spare ink lines.");
        let req = prepare_image_request(&brand, "two figures", None, None, None).unwrap();
        assert_eq!(req.prompt, "Spare ink lines.\n\nScene: two figures");
        assert_eq!(req.aspect, "16:9");
        assert_eq!(req.quality, "low");
        assert_eq!(req.model, "grok-imagine-image-2.0");
        let body = image_body(&req);
        assert_eq!(body["response_format"], "b64_json");
        assert_eq!(body["n"], 1);

        let blank = brand_with_style("  ");
        assert!(matches!(
            prepare_image_request(&blank, "scene", None, None, None),
            Err(ImageError::MissingStyle { .. })
        ));
        assert!(matches!(
            prepare_image_request(&brand, "  ", None, None, None),
            Err(ImageError::EmptyScene)
        ));
        assert!(matches!(
            prepare_image_request(&brand, "scene", Some("square"), None, None),
            Err(ImageError::BadAspect { .. })
        ));
    }

    #[test]
    fn overrides_replace_brand_defaults() {
        let mut brand = brand_with_style("Ink.");
        brand.image.aspect = "1:1".into();
        brand.image.quality = "medium".into();
        brand.image.model = "brand-model".into();
        let kept = prepare_image_request(&brand, "panel", None, None, None).unwrap();
        assert_eq!(kept.aspect, "1:1");
        assert_eq!(kept.quality, "medium");
        assert_eq!(kept.model, "brand-model");
        let replaced = prepare_image_request(
            &brand,
            "panel",
            Some("4:3"),
            Some("auto"),
            Some("other-model"),
        )
        .unwrap();
        assert_eq!(replaced.aspect, "4:3");
        assert_eq!(replaced.quality, "auto");
        assert_eq!(replaced.model, "other-model");
    }

    #[test]
    fn decode_b64_url_and_empty_payload() {
        let raw = B64.encode(b"png");
        let parsed = parse_payload(&json!({"data": [{"b64_json": raw}]})).unwrap();
        assert!(matches!(parsed, ParsedImage::Bytes(bytes) if bytes == b"png"));
        let url = parse_payload(&json!({"data": [{"url": "https://example.test/a.png"}]})).unwrap();
        assert!(matches!(url, ParsedImage::Url(value) if value.contains("example.test")));
        assert!(matches!(
            parse_payload(&json!({"error": "nope"})),
            Err(ImageError::EmptyPayload(_))
        ));
        assert!(matches!(
            parse_payload(&json!({"data": [{}]})),
            Err(ImageError::NoImage)
        ));
    }

    #[test]
    fn render_writes_scripted_bytes_and_records_the_prompt() {
        let dir = std::env::temp_dir().join(format!(
            "symsight-image-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("nested").join("panel.png");
        let client = ScriptedImageClient::new(b"png-bytes".to_vec());
        let path = render_image(
            &brand_with_style("Spare ink."),
            "a loop",
            &dest,
            Some("1:1"),
            None,
            None,
            &client,
        )
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"png-bytes");
        let calls = client.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].prompt.contains("Spare ink."));
        assert!(calls[0].prompt.contains("Scene: a loop"));
        assert_eq!(calls[0].aspect, "1:1");
        let _ = std::fs::remove_dir_all(dir);
    }
}
