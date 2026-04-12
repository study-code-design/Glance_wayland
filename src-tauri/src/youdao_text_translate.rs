use std::sync::Arc;

use reqwest::Client;
use serde::Deserialize;

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
        let typ = youdao_type_code(from, to);

        let resp = self
            .http
            .get("https://fanyi.youdao.com/translate")
            .query(&[
                ("doctype", "json"),
                ("type", &typ),
                ("i", text),
            ])
            .send()
            .await
            .map_err(|e| AppError::Api(format!("Youdao text translate request failed: {e}")))?;

        let body: YoudaoResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Api(format!("Youdao text translate parse failed: {e}")))?;

        if body.errorCode != 0 {
            return Err(AppError::Api(format!(
                "Youdao text translate error: errorCode={}",
                body.errorCode
            )));
        }

        let mut translated = String::new();
        for row in &body.translateResult {
            for seg in row {
                translated.push_str(&seg.tgt);
            }
        }

        if translated.is_empty() {
            return Err(AppError::Api(
                "Youdao text translate returned empty result".into(),
            ));
        }

        let detected = if from == "auto" {
            body.r#type
                .split("2")
                .next()
                .unwrap_or("auto")
                .to_string()
        } else {
            from.to_string()
        };

        Ok(TextTranslationResult {
            translated_text: translated,
            from_lang_detected: normalize_youdao_lang(&detected),
        })
    }
}

fn youdao_type_code(from: &str, to: &str) -> String {
    let from = normalize_to_youdao(from);
    let to = normalize_to_youdao(to);
    if from == "AUTO" || to == "AUTO" {
        "AUTO".to_string()
    } else {
        format!("{from}2{to}")
    }
}

fn normalize_to_youdao(code: &str) -> &'static str {
    match code {
        "zh-CHS" | "zh-CN" | "zh" => "ZH_CN",
        "zh-CHT" | "zh-TW" => "ZH_CHT",
        "en" => "EN",
        "ja" => "JA",
        "ko" => "KO",
        "fr" => "FR",
        "de" => "DE",
        "ru" => "RU",
        "es" => "ES",
        "pt" => "PT",
        "it" => "IT",
        "vi" => "VI",
        "id" => "ID",
        "ar" => "AR",
        "nl" => "NL",
        "th" => "TH",
        "auto" => "AUTO",
        _ => "AUTO",
    }
}

fn normalize_youdao_lang(code: &str) -> String {
    match code {
        "ZH_CN" => "zh-CHS".to_string(),
        "ZH_CHT" => "zh-CHT".to_string(),
        "EN" => "en".to_string(),
        "JA" => "ja".to_string(),
        "KO" => "ko".to_string(),
        "FR" => "fr".to_string(),
        "DE" => "de".to_string(),
        "RU" => "ru".to_string(),
        "ES" => "es".to_string(),
        "PT" => "pt".to_string(),
        "IT" => "it".to_string(),
        other => other.to_lowercase(),
    }
}

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct YoudaoResponse {
    #[serde(rename = "type")]
    r#type: String,
    #[serde(default)]
    errorCode: i64,
    #[serde(default)]
    translateResult: Vec<Vec<YoudaoSegment>>,
}

#[derive(Debug, Deserialize)]
struct YoudaoSegment {
    #[serde(default)]
    tgt: String,
    #[serde(default)]
    src: String,
}
