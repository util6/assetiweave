use crate::backend::dto::AppResult;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};
use uuid::Uuid;

const PROMPT_CLIPBOARD_ATTACHMENT_LIMIT: usize = 6;
const PROMPT_CLIPBOARD_MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const PROMPT_CLIPBOARD_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptClipboardParams {
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) attachments: Vec<PromptClipboardImageAttachment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptClipboardImageAttachment {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) mime_type: String,
    pub(crate) data_url: String,
}

pub(crate) fn copy_prompt_card_to_clipboard(params: PromptClipboardParams) -> AppResult<()> {
    if params.attachments.len() > PROMPT_CLIPBOARD_ATTACHMENT_LIMIT {
        return Err(format!(
            "too many prompt image attachments: maximum is {PROMPT_CLIPBOARD_ATTACHMENT_LIMIT}"
        ));
    }

    let cache_root = prompt_clipboard_cache_root();
    prune_prompt_clipboard_cache(&cache_root);
    let cache_dir = cache_root.join(Uuid::new_v4().to_string());
    fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;

    let text_path = cache_dir.join("prompt.txt");
    fs::write(&text_path, params.text.as_bytes()).map_err(|error| error.to_string())?;

    let mut image_paths = Vec::new();
    for (index, attachment) in params.attachments.iter().enumerate() {
        let image = decode_prompt_clipboard_image(attachment)?;
        let file_name = prompt_clipboard_image_file_name(index, &attachment.name, &image.mime_type);
        let image_path = cache_dir.join(file_name);
        fs::write(&image_path, image.bytes).map_err(|error| error.to_string())?;
        image_paths.push(image_path);
    }

    write_prompt_clipboard(&text_path, &image_paths)
}

#[derive(Debug)]
struct DecodedPromptClipboardImage {
    mime_type: String,
    bytes: Vec<u8>,
}

fn decode_prompt_clipboard_image(
    attachment: &PromptClipboardImageAttachment,
) -> AppResult<DecodedPromptClipboardImage> {
    let (metadata, payload) = attachment
        .data_url
        .split_once(',')
        .ok_or_else(|| "image attachment data URL is malformed".to_string())?;
    let metadata = metadata
        .strip_prefix("data:")
        .ok_or_else(|| "image attachment data URL must start with data:".to_string())?;
    let mut metadata_parts = metadata.split(';');
    let data_url_mime_type = metadata_parts.next().unwrap_or_default();
    let base64_encoded = metadata_parts.any(|part| part.eq_ignore_ascii_case("base64"));
    if !base64_encoded {
        return Err("image attachment data URL must be base64 encoded".to_string());
    }

    let mime_type = if data_url_mime_type.trim().is_empty() {
        attachment.mime_type.trim()
    } else {
        data_url_mime_type.trim()
    };
    if !mime_type.starts_with("image/") {
        return Err("clipboard attachment must be an image".to_string());
    }

    let bytes = STANDARD
        .decode(payload)
        .map_err(|error| error.to_string())?;
    if bytes.len() > PROMPT_CLIPBOARD_MAX_IMAGE_BYTES {
        return Err(format!(
            "image attachment is too large: maximum is {} MiB",
            PROMPT_CLIPBOARD_MAX_IMAGE_BYTES / 1024 / 1024
        ));
    }

    Ok(DecodedPromptClipboardImage {
        mime_type: mime_type.to_string(),
        bytes,
    })
}

fn prompt_clipboard_image_file_name(index: usize, name: &str, mime_type: &str) -> String {
    let extension = image_extension_for_mime_type(mime_type).unwrap_or("png");
    let stem = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(safe_file_stem)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "image".to_string());
    format!("{:02}-{}.{}", index + 1, stem, extension)
}

fn image_extension_for_mime_type(mime_type: &str) -> Option<&'static str> {
    match mime_type.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/tiff" => Some("tiff"),
        "image/heic" => Some("heic"),
        _ => None,
    }
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn prompt_clipboard_cache_root() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("AssetIWeave")
        .join("prompt-clipboard")
}

fn prune_prompt_clipboard_cache(cache_root: &Path) {
    let Ok(entries) = fs::read_dir(cache_root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified_at) = metadata.modified() else {
            continue;
        };
        if now
            .duration_since(modified_at)
            .is_ok_and(|age| age > PROMPT_CLIPBOARD_CACHE_TTL)
        {
            if metadata.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn write_prompt_clipboard(text_path: &Path, image_paths: &[PathBuf]) -> AppResult<()> {
    let mut command = Command::new("/usr/bin/osascript");
    command
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(MACOS_PROMPT_CLIPBOARD_SCRIPT)
        .arg("--")
        .arg(text_path);
    for image_path in image_paths {
        command.arg(image_path);
    }

    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err("failed to write macOS pasteboard".to_string())
    } else {
        Err(stderr)
    }
}

#[cfg(not(target_os = "macos"))]
fn write_prompt_clipboard(_text_path: &Path, _image_paths: &[PathBuf]) -> AppResult<()> {
    Err("native prompt image clipboard copy is currently only available on macOS".to_string())
}

#[cfg(target_os = "macos")]
const MACOS_PROMPT_CLIPBOARD_SCRIPT: &str = r#"
function run(argv) {
  ObjC.import("AppKit");
  ObjC.import("Foundation");

  const pasteboard = $.NSPasteboard.generalPasteboard;
  const items = $.NSMutableArray.array;
  const textPath = argv[0];
  const textData = $.NSData.dataWithContentsOfFile($(textPath));
  const textString = $.NSString.alloc.initWithDataEncoding(textData, $.NSUTF8StringEncoding);
  const hasText = textString && textString.length > 0;
  const hasFiles = argv.length > 1;

  if (hasText) {
    const textItem = $.NSPasteboardItem.alloc.init;
    textItem.setStringForType(textString, $.NSPasteboardTypeString);
    textItem.setStringForType(htmlForText(textString), $("public.html"));
    items.addObject(textItem);
  }

  for (let index = 1; index < argv.length; index += 1) {
    const path = argv[index];
    const url = $.NSURL.fileURLWithPath($(path));
    const fileItem = $.NSPasteboardItem.alloc.init;
    fileItem.setStringForType(url.absoluteString, $.NSPasteboardTypeFileURL);
    fileItem.setStringForType(url.absoluteString, $("public.file-url"));
    const imageData = $.NSData.dataWithContentsOfURL(url);
    const imageType = imagePasteboardType(path);
    if (imageData && imageType) {
      fileItem.setDataForType(imageData, $(imageType));
    }
    items.addObject(fileItem);
  }

  if (!hasText && !hasFiles) {
    return "empty";
  }

  pasteboard.clearContents;
  if (items.count === 0) {
    return "empty";
  }
  return pasteboard.writeObjects(items) ? "ok" : "failed";
}

function imagePasteboardType(path) {
  const lower = path.toLowerCase();
  if (lower.endsWith(".png")) return "public.png";
  if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) return "public.jpeg";
  if (lower.endsWith(".gif")) return "com.compuserve.gif";
  if (lower.endsWith(".webp")) return "org.webmproject.webp";
  if (lower.endsWith(".tif") || lower.endsWith(".tiff")) return "public.tiff";
  if (lower.endsWith(".heic")) return "public.heic";
  return null;
}

function htmlForText(value) {
  const text = ObjC.unwrap(value);
  return [
    "<!doctype html><html><body><pre>",
    String(text)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;"),
    "</pre></body></html>",
  ].join("");
}
"#;

#[cfg(test)]
mod tests {
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

        assert!(error.contains("image"));
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
}
