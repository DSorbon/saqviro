import {
  Activity,
  Archive,
  ArrowLeftRight,
  AudioLines,
  Barcode,
  Braces,
  Clock,
  Combine,
  Crop,
  EyeOff,
  FileText,
  Fingerprint,
  FlipHorizontal2,
  FolderOpen,
  FolderTree,
  Gauge,
  Hash,
  History,
  House,
  Image,
  Info,
  KeyRound,
  Link,
  Lock,
  Music,
  Palette,
  QrCode,
  Regex,
  RotateCw,
  Scaling,
  Scissors,
  Search,
  Settings,
  ShieldCheck,
  Shrink,
  Split,
  Stamp,
  Star,
  Tag,
  Terminal,
  Type,
  Video,
  Volume2,
  Workflow,
} from "@lucide/svelte";
import type { Component } from "svelte";
import type { Tool } from "./types";

// Lucide's components are typed for its own props; only `size` is used here.
export type Icon = Component<{ size?: number }>;

const asIcon = (icon: unknown) => icon as Icon;

/** Icons of the sidebar, by section. */
const SECTIONS: Record<string, unknown> = {
  home: House,
  favorites: Star,
  recent: History,
  workflows: Workflow,
  activity: Activity,
  pdf: FileText,
  image: Image,
  video: Video,
  audio: Music,
  files: FolderOpen,
  text: Type,
  developer: Terminal,
  privacy: ShieldCheck,
  ocr: Search,
  settings: Settings,
};

export const sectionIcon = (id: string): Icon => asIcon(SECTIONS[id] ?? FolderOpen);

/** What a tool does, from its id. The first match wins. */
const ACTIONS: [RegExp, unknown][] = [
  [/compress|normalize-audio|faststart/, Shrink],
  [/merge|contact-sheet/, Combine],
  [/split|extract|frames/, Split],
  [/crop/, Crop],
  [/trim|silence|mute/, Scissors],
  [/rotate/, RotateCw],
  [/flip/, FlipHorizontal2],
  [/resize|scale|favicon/, Scaling],
  [/watermark|page-numbers|border|corners/, Stamp],
  [/metadata/, EyeOff],
  [/tags/, Tag],
  [/password|encrypt|decrypt/, Lock],
  [/key|cert|jwt|hmac/, KeyRound],
  [/qr/, QrCode],
  [/barcode/, Barcode],
  [/zip|tar/, Archive],
  [/sha|hash|checksum|fingerprint/, Fingerprint],
  [/tree|folder|duplicates|empty|large/, FolderTree],
  [/regex/, Regex],
  [/url|slug/, Link],
  [/color/, Palette],
  [/cron|timestamp/, Clock],
  [/json|xml|yaml|csv|sql/, Braces],
  [/uuid|ulid|number/, Hash],
  [/waveform|spectrogram/, AudioLines],
  [/volume|sample/, Volume2],
  [/speed|fps/, Gauge],
  [/info|inspect|lookup|view/, Info],
  [/convert|-to-|to-|from-/, ArrowLeftRight],
];

export const toolIcon = (tool: Tool): Icon =>
  asIcon(ACTIONS.find(([pattern]) => pattern.test(tool.id))?.[1] ?? SECTIONS[tintOf(tool)] ?? FileText);

/** The colour family of a tool, one per sidebar section. */
export function tintOf(tool: Tool): string {
  switch (tool.category) {
    case "qr":
      return "text";
    case "network":
    case "system":
    case "screen":
      return "developer";
    case "ocr":
      return "text";
    default:
      return tool.category;
  }
}
