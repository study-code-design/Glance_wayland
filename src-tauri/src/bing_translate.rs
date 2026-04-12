use std::sync::Arc;

use reqwest::Client;
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};
use crate::models::TextTranslationResult;

pub struct BingTranslateClient {
    http: Arc<Client>,
    token: Arc<RwLock<Option<BingToken>>>,
}

#[derive(Clone)]
struct BingToken {
    value: String,
    key: String,
    ig: String,
    iid: String,
}

impl BingTranslateClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self {
            http,
            token: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn translate(
        &self,
        text: &str,
        from: &str,
        to: &str,
    ) -> AppResult<TextTranslationResult> {
        let token = self.get_or_refresh_token().await?;

        let from_bing = map_lang_code_bing(from);
        let to_bing = map_lang_code_bing(to);

        let resp = self
            .http
            .post("https://www.bing.com/ttranslatev3")
            .query(&[
                ("isVertical", "1"),
                ("", ""),
                ("IG", &token.ig),
                ("IID", &token.iid),
            ])
            .form(&[
                ("fromLang", from_bing.as_str()),
                ("to", to_bing.as_str()),
                ("text", text),
                ("key", &token.value),
                ("tokenSig", &token.key),
            ])
            .send()
            .await
            .map_err(|e| AppError::Api(format!("Bing translate request failed: {e}")))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            *self.token.write().await = None;
            return Err(AppError::Api("Bing translate rate limited, token reset".into()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Api(format!("Bing translate parse failed: {e}")))?;

        if let Some(err) = body.get("statusCode").and_then(|v| v.as_u64()) {
            if err >= 400 {
                *self.token.write().await = None;
                return Err(AppError::Api(format!("Bing translate error: statusCode={err}")));
            }
        }

        let translated = body
            .pointer("/0/translations/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let detected = body
            .pointer("/0/detectedLanguage/language")
            .and_then(|v| v.as_str())
            .unwrap_or(from)
            .to_string();

        if translated.is_empty() {
            return Err(AppError::Api(
                "Bing translate returned empty result".into(),
            ));
        }

        Ok(TextTranslationResult {
            translated_text: translated,
            from_lang_detected: normalize_bing_lang(&detected),
        })
    }

    async fn get_or_refresh_token(&self) -> AppResult<BingToken> {
        {
            let guard = self.token.read().await;
            if let Some(ref t) = *guard {
                return Ok(t.clone());
            }
        }

        let resp = self
            .http
            .get("https://www.bing.com/translator")
            .send()
            .await
            .map_err(|e| AppError::Api(format!("Bing token fetch failed: {e}")))?;

        let html = resp
            .text()
            .await
            .map_err(|e| AppError::Api(format!("Bing token read failed: {e}")))?;

        let value = extract_var(&html, "params_RichTranslateHelper = [")
            .or_else(|| extract_var(&html, "params_RichTranslateHelper = ["))
            .and_then(|s| {
                let start = s.find('"')?;
                let end = s[start + 1..].find('"')?;
                Some(s[start + 1..start + 1 + end].to_string())
            })
            .ok_or_else(|| AppError::Api("Bing token value not found in page".into()))?;

        let key = extract_var(&html, "params_RichTranslateHelper = [")
            .and_then(|s| {
                let after_first = s.find("\",\"")?;
                let rest = &s[after_first + 3..];
                let end = rest.find('"')?;
                Some(rest[..end].to_string())
            })
            .ok_or_else(|| AppError::Api("Bing token key not found in page".into()))?;

        let ig = extract_var(&html, "IG:\"")
            .or_else(|| extract_var(&html, "IG: \""))
            .ok_or_else(|| AppError::Api("Bing IG not found in page".into()))?;

        let iid = extract_iid(&html).unwrap_or_else(|| "translator.5023".to_string());

        let token = BingToken {
            value,
            key,
            ig,
            iid,
        };

        *self.token.write().await = Some(token.clone());
        Ok(token)
    }
}

fn extract_var(html: &str, prefix: &str) -> Option<String> {
    let start = html.find(prefix)?;
    let rest = &html[start + prefix.len()..];
    let end = rest.find(';').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn extract_iid(html: &str) -> Option<String> {
    let start = html.find("data-iid=\"")?;
    let rest = &html[start + 10..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn map_lang_code_bing(code: &str) -> String {
    match code {
        "zh-CHS" => "zh-Hans".to_string(),
        "zh-CHT" => "zh-Hant".to_string(),
        "auto" => "auto-detect".to_string(),
        _ => code.to_string(),
    }
}

fn normalize_bing_lang(code: &str) -> String {
    match code {
        "zh-Hans" => "zh-CHS".to_string(),
        "zh-Hant" => "zh-CHT".to_string(),
        "auto-detect" => "auto".to_string(),
        other => other.to_lowercase(),
    }
}
