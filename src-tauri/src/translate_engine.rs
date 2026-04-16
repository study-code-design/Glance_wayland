use std::sync::Arc;

use crate::bing_translate::BingTranslateClient;
use crate::error::AppResult;
use crate::models::{TextTranslateEngine, TextTranslationResult};

#[derive(Clone)]
pub struct TextTranslator {
    bing: Arc<BingTranslateClient>,
}

impl TextTranslator {
    pub fn new(bing: BingTranslateClient) -> Self {
        Self {
            bing: Arc::new(bing),
        }
    }

    pub async fn translate(
        &self,
        text: &str,
        from: &str,
        to: &str,
        _preferred: TextTranslateEngine,
    ) -> AppResult<TextTranslationResult> {
        self.bing.translate(text, from, to).await
    }
}