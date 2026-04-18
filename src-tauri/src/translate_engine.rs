use std::sync::Arc;

use crate::bing_translate::BingTranslateClient;
use crate::error::AppResult;
use crate::llm_translate::LlmTranslateClient;
use crate::models::{LlmConfig, TextTranslateEngine, TextTranslationResult};

#[derive(Clone)]
pub struct TextTranslator {
    bing: Arc<BingTranslateClient>,
    llm: Arc<LlmTranslateClient>,
}

impl TextTranslator {
    pub fn new(bing: BingTranslateClient, llm: LlmTranslateClient) -> Self {
        Self {
            bing: Arc::new(bing),
            llm: Arc::new(llm),
        }
    }

    pub async fn translate(
        &self,
        text: &str,
        from: &str,
        to: &str,
        engine: TextTranslateEngine,
        llm_config: &LlmConfig,
    ) -> AppResult<TextTranslationResult> {
        match engine {
            TextTranslateEngine::Bing => self.bing.translate(text, from, to).await,
            TextTranslateEngine::Llm => {
                self.llm
                    .translate(
                        text,
                        from,
                        to,
                        &llm_config.base_url,
                        &llm_config.api_key,
                        &llm_config.model,
                    )
                    .await
            }
        }
    }
}