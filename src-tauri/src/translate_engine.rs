use std::sync::Arc;

use crate::bing_translate::BingTranslateClient;
use crate::error::AppResult;
use crate::google_translate::GoogleTranslateClient;
use crate::models::{TextTranslateEngine, TextTranslationResult};
use crate::youdao_text_translate::YoudaoTextTranslateClient;

const FALLBACK_ORDER: &[TextTranslateEngine] = &[
    TextTranslateEngine::Google,
    TextTranslateEngine::Bing,
    TextTranslateEngine::Youdao,
];

#[derive(Clone)]
pub struct TextTranslator {
    google: Arc<GoogleTranslateClient>,
    bing: Arc<BingTranslateClient>,
    youdao: Arc<YoudaoTextTranslateClient>,
}

impl TextTranslator {
    pub fn new(
        google: GoogleTranslateClient,
        bing: BingTranslateClient,
        youdao: YoudaoTextTranslateClient,
    ) -> Self {
        Self {
            google: Arc::new(google),
            bing: Arc::new(bing),
            youdao: Arc::new(youdao),
        }
    }

    pub async fn translate(
        &self,
        text: &str,
        from: &str,
        to: &str,
        preferred: TextTranslateEngine,
    ) -> AppResult<TextTranslationResult> {
        let engines = build_engine_order(preferred);

        let mut last_err = None;
        for engine in engines {
            let result = match engine {
                TextTranslateEngine::Google => {
                    tracing::debug!("trying Google translate");
                    self.google.translate(text, from, to).await
                }
                TextTranslateEngine::Bing => {
                    tracing::debug!("trying Bing translate");
                    self.bing.translate(text, from, to).await
                }
                TextTranslateEngine::Youdao => {
                    tracing::debug!("trying Youdao text translate");
                    self.youdao.translate(text, from, to).await
                }
            };

            match result {
                Ok(r) => {
                    tracing::info!("text translation succeeded with {engine:?}");
                    return Ok(TextTranslationResult {
                        translated_text: r.translated_text,
                        from_lang_detected: r.from_lang_detected,
                    });
                }
                Err(e) => {
                    tracing::warn!("text translation with {engine:?} failed: {e}");
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| crate::error::AppError::Api("all translation engines failed".into())))
    }
}

fn build_engine_order(preferred: TextTranslateEngine) -> Vec<TextTranslateEngine> {
    let mut engines = vec![preferred];
    for &e in FALLBACK_ORDER {
        if e != preferred {
            engines.push(e);
        }
    }
    engines
}
