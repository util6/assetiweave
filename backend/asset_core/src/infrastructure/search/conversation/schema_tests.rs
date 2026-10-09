use super::*;
use tantivy::{collector::TopDocs, doc, query::QueryParser, Index};

#[test]
fn conversation_schema_exposes_stable_document_fields() {
    let fields = build_conversation_schema();

    assert_eq!(
        fields.schema.get_field_name(fields.document_kind),
        "document_kind"
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
    assert_eq!(fields.schema.get_field_name(fields.card_kind), "card_kind");
    assert_eq!(
        fields.schema.get_field_name(fields.semantic_role),
        "semantic_role"
    );
    assert_eq!(
        fields.schema.get_field_name(fields.id_fragment),
        "id_fragment"
    );
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
            fields.document_kind => "card",
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
