import { openExternalUrl } from "../services/externalLinks";

export function openExternalLink(url: string): void {
  void openExternalUrl(url);
}
