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

    pub(crate) fn translate_conversation_card(
        &self,
        params: crate::backend::card_translation::ConversationTranslationRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationResult> {
        crate::backend::card_translation::translate_conversation_card(params)
    }

    pub(crate) fn test_conversation_translation_connection(
        &self,
        params: crate::backend::card_translation::ConversationTranslationConnectionRequest,
    ) -> AppResult<crate::backend::card_translation::OpencodeTranslationAvailability> {
        Ok(crate::backend::card_translation::test_conversation_translation_connection(params))
    }

    pub(crate) fn list_conversation_translation_models(
        &self,
        params: crate::backend::card_translation::ConversationTranslationModelsRequest,
    ) -> AppResult<crate::backend::card_translation::ConversationTranslationModelsResult> {
        Ok(crate::backend::card_translation::list_conversation_translation_models(params))
    }
}
