use super::*;

#[test]
fn decodes_base64_image_data_urls() {
    let image = decode_prompt_clipboard_image(&PromptClipboardImageAttachment {
        name: "diagram.png".to_string(),
        mime_type: "image/png".to_string(),
        data_url: "data:image/png;base64,aGVsbG8=".to_string(),
    })
    .expect("decode image");

    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.bytes, b"hello");
}

#[test]
fn rejects_non_image_data_urls() {
    let error = decode_prompt_clipboard_image(&PromptClipboardImageAttachment {
        name: "note.txt".to_string(),
        mime_type: "text/plain".to_string(),
        data_url: "data:text/plain;base64,aGVsbG8=".to_string(),
    })
    .expect_err("reject non-image");

    assert!(error.to_string().contains("image"));
}

#[test]
fn creates_safe_prompt_clipboard_file_names() {
    assert_eq!(
        prompt_clipboard_image_file_name(0, "../screen shot.png", "image/png"),
        "01-screen-shot.png"
    );
    assert_eq!(
        prompt_clipboard_image_file_name(1, "diagram", "image/jpeg"),
        "02-diagram.jpg"
    );
}
