import { invoke } from "@tauri-apps/api/core";

export interface PromptClipboardAttachment {
  dataUrl: string;
  mimeType: string;
  name: string;
}

export interface PromptClipboardInput {
  attachments: PromptClipboardAttachment[];
  text: string;
}

export async function copyPromptImagesToClipboard(attachments: PromptClipboardAttachment[]): Promise<void> {
  if (attachments.length === 0) {
    return;
  }

  if (isTauriRuntime()) {
    try {
      await invoke<void>("copy_prompt_card_to_clipboard", {
        params: {
          attachments: attachments.map((attachment) => ({
            dataUrl: attachment.dataUrl,
            mimeType: attachment.mimeType,
            name: attachment.name,
          })),
          text: "",
        },
      });
      return;
    } catch {
      // Windows and Linux currently rely on the WebView clipboard implementation.
    }
  }

  await copyPromptImagesWithWebClipboard(attachments);
}

export async function copyPromptTextToClipboard(text: string): Promise<void> {
  await navigator.clipboard.writeText(text.trimEnd());
}

async function copyPromptImagesWithWebClipboard(attachments: PromptClipboardAttachment[]) {
  if (typeof navigator.clipboard.write === "function" && typeof ClipboardItem !== "undefined") {
    const imageItems = attachments
      .map(promptImageAttachmentToBlob)
      .filter((blob): blob is Blob => Boolean(blob))
      .map((blob) => new ClipboardItem({ [blob.type]: blob }));
    if (imageItems.length > 0) {
      try {
        await navigator.clipboard.write(imageItems);
        return;
      } catch {
        // Browser preview can reject image clipboard items; keep a text fallback.
      }
    }
  }

  await navigator.clipboard.writeText(attachments.map((attachment) => `[image: ${attachment.name}]`).join("\n"));
}

function promptImageAttachmentToBlob(attachment: PromptClipboardAttachment) {
  const match = /^data:([^;,]+)(;base64)?,(.*)$/u.exec(attachment.dataUrl);
  if (!match) {
    return null;
  }

  const mimeType = attachment.mimeType || match[1] || "image/png";
  const rawData = match[2] ? atob(match[3]) : decodeURIComponent(match[3]);
  const bytes = new Uint8Array(rawData.length);
  for (let index = 0; index < rawData.length; index += 1) {
    bytes[index] = rawData.charCodeAt(index);
  }
  return new Blob([bytes], { type: mimeType });
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
