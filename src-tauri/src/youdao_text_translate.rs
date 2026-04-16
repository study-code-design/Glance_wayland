use std::sync::Arc;

use reqwest::Client;

use crate::error::{AppError, AppResult};
use crate::models::TextTranslationResult;

pub struct YoudaoTextTranslateClient {
    http: Arc<Client>,
}

impl YoudaoTextTranslateClient {
    pub fn new(http: Arc<Client>) -> Self {
        Self { http }
    }

    pub async fn translate(
        &self,
        text: &str,
        from: &str,
        to: &str,
    ) -> AppResult<TextTranslationResult> {
        let from_code = normalize_lang_for_dict(from);
        let to_code = normalize_lang_for_dict(to);

        let le = if to_code == "zh" {
            &from_code
        } else {
            &to_code
        };

        let resp = self
            .http
            .get("https://dict.youdao.com/jsonapi_s")
            .query(&[
                ("doctype", "json"),
                ("jsonversion", "4"),
                ("q", text),
                ("le", le),
            ])
            .send()
            .await
            .map_err(|e| AppError::Api(format!("Youdao dict request failed: {e}")))?;

        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| AppError::Api(format!("Youdao dict read body failed: {e}")))?;

        if !status.is_success() {
            return Err(AppError::Api(format!(
                "Youdao dict HTTP {}",
                status.as_u16()
            )));
        }

        let body: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| AppError::Api(format!("Youdao dict parse failed: {e}")))?;

        let mut translated = String::new();
        let mut alternatives: Vec<String> = Vec::new();
        let mut detected = from.to_string();

        // EC (English-Chinese dictionary) translations — primary + alternatives
        if let Some(ec) = body.get("ec") {
            if let Some(word) = ec.get("word").and_then(|w| w.as_str()) {
                if detected == "auto" && !word.is_empty() {
                    detected = "en".to_string();
                }
            }
            if let Some(trs) = ec.get("trs").and_then(|t| t.as_array()) {
                for (i, tr) in trs.iter().enumerate() {
                    if let Some(text_val) = tr
                        .get("tr")
                        .and_then(|arr| arr.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|item| item.get("#text"))
                        .and_then(|v| v.as_str())
                    {
                        if i == 0 {
                            translated = text_val.to_string();
                        } else {
                            alternatives.push(text_val.to_string());
                        }
                    }
                }
            }
        }

        // web_trans — also collect as alternatives
        if let Some(wt) = body.get("web_trans") {
            if let Some(items) = wt.get("web-translation").and_then(|v| v.as_array()) {
                if let Some(first) = items.first() {
                    if let Some(key) = first.get("key").and_then(|v| v.as_str()) {
                        if detected == "auto" {
                            detected = if key.chars().any(|c| '\u{4e00}' <= c && c <= '\u{9fff}') {
                                "zh-CHS".to_string()
                            } else {
                                "en".to_string()
                            };
                        }
                    }
                    if let Some(trans) = first.get("trans").and_then(|v| v.as_array()) {
                        for (i, t) in trans.iter().enumerate() {
                            if let Some(val) = t.get("value").and_then(|v| v.as_str()) {
                                if translated.is_empty() && i == 0 {
                                    translated = val.to_string();
                                } else if !val.is_empty() {
                                    alternatives.push(val.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        if translated.is_empty() {
            return Err(AppError::Api(
                "Youdao dict returned no translation".into(),
            ));
        }

        Ok(TextTranslationResult {
            translated_text: translated,
            from_lang_detected: normalize_dict_lang(&detected),
            alternatives,
        })
    }
}

fn normalize_lang_for_dict(code: &str) -> String {
    match code {
        "zh-CHS" | "zh-CN" | "zh" => "zh".to_string(),
        "zh-CHT" | "zh-TW" => "zh".to_string(),
        "en" => "en".to_string(),
        "ja" => "ja".to_string(),
        "ko" => "ko".to_string(),
        "fr" => "fr".to_string(),
        "de" => "de".to_string(),
        "ru" => "ru".to_string(),
        "es" => "es".to_string(),
        "auto" => "en".to_string(),
        other => other.to_string(),
    }
}

fn normalize_dict_lang(code: &str) -> String {
    match code {
        "zh-CHS" | "zh-CN" | "zh" => "zh-CHS".to_string(),
        "zh-CHT" | "zh-TW" => "zh-CHT".to_string(),
        "en" => "en".to_string(),
        "ja" => "ja".to_string(),
        "ko" => "ko".to_string(),
        "fr" => "fr".to_string(),
        "de" => "de".to_string(),
        "ru" => "ru".to_string(),
        "es" => "es".to_string(),
        other => other.to_string(),
    }
}