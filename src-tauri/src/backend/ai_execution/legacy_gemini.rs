//! Temporary Gemini CLI translation seam.
//!
//! Delete this module when Gemini has a registered Native/ACP backend. New
//! Agent integrations must be added to `AgentRegistry`, never to this seam.

use super::{
    execute_structured_text, AiCliRuntime, AiCommandOptions, AiExecutionError,
    AiStructuredTextRequest,
};

pub(crate) fn execute_translation(
    model: Option<String>,
    prompt: String,
    options: AiCommandOptions,
) -> Result<String, AiExecutionError> {
    execute_structured_text(AiStructuredTextRequest {
        runtime: AiCliRuntime::Gemini,
        model,
        prompt,
        options,
    })
    .map(|result| result.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::ai_execution::structured_text_args;

    #[test]
    fn gemini_legacy_arguments_keep_model_and_prompt_flags() {
        assert_eq!(
            structured_text_args(AiCliRuntime::Gemini, Some("gemini-2.5"), "prompt"),
            ["--model", "gemini-2.5", "--prompt", "prompt"]
        );
    }
}
