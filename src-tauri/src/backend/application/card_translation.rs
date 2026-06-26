use super::prelude::*;

impl AppService {
    pub(crate) fn check_opencode_translation_availability(
        &self,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(crate::backend::card_translation::check_opencode_translation_availability())
    }

    pub(crate) fn translate_conversation_card_with_opencode(
        &self,
        params: crate::backend::card_translation::OpencodeTranslationRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        crate::backend::card_translation::translate_conversation_card_with_opencode(params)
    }
}
