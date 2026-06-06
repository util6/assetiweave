import {
  Archive,
  Boxes,
  Brain,
  Command,
  FileCode2,
  FileText,
  Gauge,
  Grid3X3,
  Layers3,
  Navigation,
  Rocket,
  Settings,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import type { NavigationIcon } from "./types";

const iconRegistry = {
  archive: Archive,
  boxes: Boxes,
  brain: Brain,
  command: Command,
  "file-code": FileCode2,
  "file-text": FileText,
  gauge: Gauge,
  grid: Grid3X3,
  layers: Layers3,
  navigation: Navigation,
  rocket: Rocket,
  settings: Settings,
  shield: ShieldCheck,
  sparkles: Sparkles,
} satisfies Record<NavigationIcon, typeof Archive>;

export function MenuIcon({ name, size = 19 }: { name: NavigationIcon; size?: number }) {
  const Icon = iconRegistry[name];
  return <Icon size={size} />;
}
