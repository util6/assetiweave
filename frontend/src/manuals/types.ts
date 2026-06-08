import type { Locale } from "../i18n/messages";

export interface ManualSection {
  heading: string;
  body?: string;
  cautions?: string[];
  items?: string[];
  keywords?: string[];
  outcomes?: string[];
  steps?: string[];
}

export interface ManualContent {
  title: string;
  subtitle: string;
  overview: string;
  sections: ManualSection[];
}

export interface ManualDocument {
  routeKey: string;
  content: Record<Locale, ManualContent>;
}
