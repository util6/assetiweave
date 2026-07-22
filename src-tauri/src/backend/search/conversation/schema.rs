use tantivy::{
    schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STORED, STRING},
    tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer},
    Index,
};
use tantivy_jieba::JiebaTokenizer;

pub(super) const JIEBA_TOKENIZER: &str = "conversation_jieba";
pub(super) const NGRAM_TOKENIZER: &str = "conversation_ngram";

#[derive(Clone)]
pub(super) struct ConversationSearchSchema {
    pub(super) schema: Schema,
    pub(super) document_type: Field,
    pub(super) document_id: Field,
    pub(super) record_kind: Field,
    pub(super) session_id: Field,
    pub(super) question_id: Field,
    pub(super) turn_id: Field,
    pub(super) part_id: Field,
    pub(super) block_id: Field,
    pub(super) card_type: Field,
    pub(super) adapter_id: Field,
    pub(super) source_id: Field,
    pub(super) project_path: Field,
    pub(super) session_title_zh: Field,
    pub(super) session_title_en: Field,
    pub(super) session_title_ngram: Field,
    pub(super) question_title_zh: Field,
    pub(super) question_title_en: Field,
    pub(super) question_title_ngram: Field,
    pub(super) content_zh: Field,
    pub(super) content_en: Field,
    pub(super) code_command: Field,
}

pub(super) fn build_conversation_schema() -> ConversationSearchSchema {
    let mut builder = Schema::builder();
    let document_type = builder.add_text_field("document_type", STRING | STORED);
    let document_id = builder.add_text_field("document_id", STRING | STORED);
    let record_kind = builder.add_text_field("record_kind", STRING | STORED);
    let session_id = builder.add_text_field("session_id", STRING | STORED);
    let question_id = builder.add_text_field("question_id", STRING | STORED);
    let turn_id = builder.add_text_field("turn_id", STRING | STORED);
    let part_id = builder.add_text_field("part_id", STRING | STORED);
    let block_id = builder.add_text_field("block_id", STRING | STORED);
    let card_type = builder.add_text_field("card_type", STRING | STORED);
    let adapter_id = builder.add_text_field("adapter_id", STRING | STORED);
    let source_id = builder.add_text_field("source_id", STRING | STORED);
    let project_path = builder.add_text_field("project_path", STRING | STORED);
    let session_title_zh =
        builder.add_text_field("session_title_zh", indexed_text(JIEBA_TOKENIZER));
    let session_title_en = builder.add_text_field("session_title_en", indexed_text("default"));
    let session_title_ngram =
        builder.add_text_field("session_title_ngram", indexed_text(NGRAM_TOKENIZER));
    let question_title_zh =
        builder.add_text_field("question_title_zh", indexed_text(JIEBA_TOKENIZER));
    let question_title_en = builder.add_text_field("question_title_en", indexed_text("default"));
    let question_title_ngram =
        builder.add_text_field("question_title_ngram", indexed_text(NGRAM_TOKENIZER));
    let content_zh = builder.add_text_field("content_zh", indexed_text(JIEBA_TOKENIZER));
    let content_en = builder.add_text_field("content_en", indexed_text("default"));
    let code_command = builder.add_text_field("code_command", indexed_text("default"));

    ConversationSearchSchema {
        schema: builder.build(),
        document_type,
        document_id,
        record_kind,
        session_id,
        question_id,
        turn_id,
        part_id,
        block_id,
        card_type,
        adapter_id,
        source_id,
        project_path,
        session_title_zh,
        session_title_en,
        session_title_ngram,
        question_title_zh,
        question_title_en,
        question_title_ngram,
        content_zh,
        content_en,
        code_command,
    }
}

pub(super) fn register_conversation_tokenizers(index: &Index) {
    index.tokenizers().register(
        JIEBA_TOKENIZER,
        JiebaTokenizer::with_ordinal_position_mode(true),
    );
    let ngram =
        TextAnalyzer::builder(NgramTokenizer::all_ngrams(2, 15).expect("valid ngram range"))
            .filter(LowerCaser)
            .build();
    index.tokenizers().register(NGRAM_TOKENIZER, ngram);
}

fn indexed_text(tokenizer: &'static str) -> TextOptions {
    TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(tokenizer)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::{collector::TopDocs, doc, query::QueryParser, Index};

    #[test]
    fn conversation_schema_exposes_stable_document_fields() {
        let fields = build_conversation_schema();

        assert_eq!(
            fields.schema.get_field_name(fields.document_type),
            "document_type"
        );
        assert_eq!(
            fields.schema.get_field_name(fields.document_id),
            "document_id"
        );
        assert_eq!(
            fields.schema.get_field_name(fields.session_id),
            "session_id"
        );
        assert_eq!(
            fields.schema.get_field_name(fields.question_id),
            "question_id"
        );
        assert_eq!(fields.schema.get_field_name(fields.card_type), "card_type");
        assert_eq!(
            fields.schema.get_field_name(fields.content_zh),
            "content_zh"
        );
        assert_eq!(
            fields.schema.get_field_name(fields.content_en),
            "content_en"
        );
    }

    #[test]
    fn registered_jieba_tokenizer_indexes_and_searches_chinese_words() {
        let fields = build_conversation_schema();
        let index = Index::create_in_ram(fields.schema.clone());
        register_conversation_tokenizers(&index);
        let mut writer = index.writer(50_000_000).expect("create writer");
        writer
            .add_document(doc!(
                fields.document_type => "card",
                fields.document_id => "card-1",
                fields.content_zh => "本地全文搜索支持中文分词"
            ))
            .expect("index Chinese card");
        writer.commit().expect("commit Chinese card");

        let reader = index.reader().expect("create reader");
        let searcher = reader.searcher();
        let query = QueryParser::for_index(&index, vec![fields.content_zh])
            .parse_query("全文搜索")
            .expect("parse Chinese query");
        let hits = searcher
            .search(&query, &TopDocs::with_limit(10).order_by_score())
            .expect("search Chinese card");

        assert_eq!(hits.len(), 1);
    }
}
