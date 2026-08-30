import type { CSSProperties } from "react";

import aiderLogo from "../assets/harness-logos/aider.svg?url";
import continueLogo from "../assets/harness-logos/continue.svg?url";
import reasonixLogo from "../assets/harness-logos/reasonix.svg?url";
import ampLogo from "@lobehub/icons-static-svg/icons/amp.svg?url";
import claudeCodeLogo from "@lobehub/icons-static-svg/icons/claudecode.svg?url";
import clineLogo from "@lobehub/icons-static-svg/icons/cline.svg?url";
import codexLogo from "@lobehub/icons-static-svg/icons/codex.svg?url";
import cursorLogo from "@lobehub/icons-static-svg/icons/cursor.svg?url";
import geminiCliLogo from "@lobehub/icons-static-svg/icons/geminicli.svg?url";
import gooseLogo from "@lobehub/icons-static-svg/icons/goose.svg?url";
import kimiLogo from "@lobehub/icons-static-svg/icons/kimi.svg?url";
import openCodeLogo from "@lobehub/icons-static-svg/icons/opencode.svg?url";
import piLogo from "@lobehub/icons-static-svg/icons/pi.svg?url";
import qwenLogo from "@lobehub/icons-static-svg/icons/qwen.svg?url";
import rooCodeLogo from "@lobehub/icons-static-svg/icons/roocode.svg?url";

const HARNESS_LOGOS: Record<string, string> = {
  "claude-code": claudeCodeLogo,
  codex: codexLogo,
  opencode: openCodeLogo,
  pi: piLogo,
  reasonix: reasonixLogo,
  "kimi-cli": kimiLogo,
  "gemini-cli": geminiCliLogo,
  "qwen-code": qwenLogo,
  cursor: cursorLogo,
  cline: clineLogo,
  "roo-code": rooCodeLogo,
  aider: aiderLogo,
  amp: ampLogo,
  goose: gooseLogo,
  continue: continueLogo,
};

const HARNESS_LABELS: Record<string, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  pi: "Pi",
  reasonix: "Reasonix",
  "kimi-cli": "Kimi CLI",
  "gemini-cli": "Gemini CLI",
  "qwen-code": "Qwen Code",
  cursor: "Cursor",
  cline: "Cline",
  "roo-code": "Roo Code",
  aider: "Aider",
  amp: "Amp",
  goose: "Goose",
  continue: "Continue",
};

const SIZE_CLASSES = {
  sm: "h-5 w-5",
  md: "h-7 w-7",
  lg: "h-9 w-9",
} as const;

export type HarnessLogoSize = keyof typeof SIZE_CLASSES;

export interface HarnessLogoProps {
  harnessType: string;
  size?: HarnessLogoSize;
  className?: string;
}

/**
 * Renders a bundled monochrome harness mark. The CSS mask keeps the source
 * SVG offline and lets the surrounding text color control the logo color.
 * Unknown harness ids get a readable initial instead of a misleading brand.
 */
export function HarnessLogo({
  harnessType,
  size = "md",
  className = "",
}: HarnessLogoProps) {
  const src = HARNESS_LOGOS[harnessType];
  const label = HARNESS_LABELS[harnessType] ?? harnessType;
  const classes = `${SIZE_CLASSES[size]} ${className}`.trim();

  if (!src) {
    return (
      <span
        aria-hidden="true"
        className={`inline-flex shrink-0 items-center justify-center rounded-md border border-slate-600 text-xs font-semibold text-slate-400 ${classes}`}
        title={`${label} logo unavailable`}
      >
        {label.slice(0, 1).toUpperCase() || "?"}
      </span>
    );
  }

  const mask = `url("${src}")`;
  const maskStyle: CSSProperties = {
    maskImage: mask,
    WebkitMaskImage: mask,
  };

  return (
    <span
      aria-hidden="true"
      className={`harness-logo ${classes}`}
      style={maskStyle}
      title={`${label} logo`}
    />
  );
}

export const SUPPORTED_HARNESS_LOGO_IDS = Object.freeze(Object.keys(HARNESS_LOGOS));
