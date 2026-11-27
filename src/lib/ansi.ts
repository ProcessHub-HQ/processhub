import type { CSSProperties } from "react";

export interface AnsiSegment {
  text: string;
  offset: number;
  style?: CSSProperties;
}

interface AnsiState {
  fg?: string;
  bg?: string;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  inverse: boolean;
}

const DEFAULT_FG = "#4d5865";
const DEFAULT_BG = "#f8f9fb";

// VS Code light-terminal palette; readable on the app's light log canvas.
const ANSI_16 = [
  "#000000",
  "#cd3131",
  "#00bc00",
  "#949800",
  "#0451a5",
  "#bc05bc",
  "#0598bc",
  "#555555",
  "#666666",
  "#cd3131",
  "#14ce14",
  "#b5ba00",
  "#0451a5",
  "#bc05bc",
  "#0598bc",
  "#a5a5a5",
];

const ESC = String.fromCharCode(27);
const BEL = String.fromCharCode(7);
const CSI_PATTERN = new RegExp(`${ESC}\\[([0-9;:?]*)([ -/]*)([@-~])`, "g");
const OSC_PATTERN = new RegExp(
  `${ESC}\\][^${BEL}${ESC}]*(?:${BEL}|${ESC}\\\\)`,
  "g",
);

function resetState(): AnsiState {
  return {
    fg: undefined,
    bg: undefined,
    bold: false,
    dim: false,
    italic: false,
    underline: false,
    strike: false,
    inverse: false,
  };
}

function color256(index: number): string {
  const clamped = Math.max(0, Math.min(255, index));
  if (clamped < 16) return ANSI_16[clamped];
  if (clamped < 232) {
    const cube = clamped - 16;
    const red = Math.floor(cube / 36);
    const green = Math.floor((cube % 36) / 6);
    const blue = cube % 6;
    const channel = (value: number) => (value === 0 ? 0 : 55 + value * 40);
    return `rgb(${channel(red)} ${channel(green)} ${channel(blue)})`;
  }
  const gray = 8 + (clamped - 232) * 10;
  return `rgb(${gray} ${gray} ${gray})`;
}

function applySgr(state: AnsiState, params: number[]) {
  for (let index = 0; index < params.length; index += 1) {
    const code = params[index];
    if (code === 0) {
      Object.assign(state, resetState());
      continue;
    }
    if (code === 1) {
      state.bold = true;
      continue;
    }
    if (code === 2) {
      state.dim = true;
      continue;
    }
    if (code === 3) {
      state.italic = true;
      continue;
    }
    if (code === 4) {
      state.underline = true;
      continue;
    }
    if (code === 7) {
      state.inverse = true;
      continue;
    }
    if (code === 9) {
      state.strike = true;
      continue;
    }
    if (code === 22) {
      state.bold = false;
      state.dim = false;
      continue;
    }
    if (code === 23) {
      state.italic = false;
      continue;
    }
    if (code === 24) {
      state.underline = false;
      continue;
    }
    if (code === 27) {
      state.inverse = false;
      continue;
    }
    if (code === 29) {
      state.strike = false;
      continue;
    }
    if (code === 39) {
      state.fg = undefined;
      continue;
    }
    if (code === 49) {
      state.bg = undefined;
      continue;
    }
    if (code >= 30 && code <= 37) {
      state.fg = ANSI_16[code - 30];
      continue;
    }
    if (code >= 90 && code <= 97) {
      state.fg = ANSI_16[code - 90 + 8];
      continue;
    }
    if (code >= 40 && code <= 47) {
      state.bg = ANSI_16[code - 40];
      continue;
    }
    if (code >= 100 && code <= 107) {
      state.bg = ANSI_16[code - 100 + 8];
      continue;
    }
    if (code === 38 || code === 48) {
      const isForeground = code === 38;
      if (params[index + 1] === 5 && index + 2 < params.length) {
        const color = color256(params[index + 2]);
        if (isForeground) state.fg = color;
        else state.bg = color;
        index += 2;
        continue;
      }
      if (params[index + 1] === 2 && index + 4 < params.length) {
        const color = `rgb(${params[index + 2]} ${params[index + 3]} ${params[index + 4]})`;
        if (isForeground) state.fg = color;
        else state.bg = color;
        index += 4;
      }
    }
  }
}

function styleFor(state: AnsiState): CSSProperties | undefined {
  const foreground = state.inverse ? (state.bg ?? DEFAULT_BG) : state.fg;
  const background = state.inverse ? (state.fg ?? DEFAULT_FG) : state.bg;
  const decorations = [
    state.underline ? "underline" : "",
    state.strike ? "line-through" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const style: CSSProperties = {};
  if (foreground) style.color = foreground;
  if (background) style.backgroundColor = background;
  if (state.bold) style.fontWeight = 600;
  if (state.italic) style.fontStyle = "italic";
  if (decorations) style.textDecoration = decorations;
  if (state.dim) style.opacity = 0.65;
  return Object.keys(style).length > 0 ? style : undefined;
}

function normalize(text: string): string {
  const withoutOsc = text.replace(OSC_PATTERN, "");
  const lines = withoutOsc.split("\n").map((line) => {
    if (!line.includes("\r")) return line;
    // Progress output redraws with carriage returns; show the final state.
    return line.slice(line.lastIndexOf("\r") + 1);
  });
  return lines.join("\n").replace(/\r?\n$/, "");
}

function parseParams(raw: string): number[] {
  if (!raw) return [0];
  return raw
    .split(/[;:]/)
    .map((part) => (part === "" ? 0 : Number.parseInt(part, 10)))
    .map((value) => (Number.isFinite(value) ? value : 0));
}

export function parseAnsi(input: string): AnsiSegment[] {
  const text = normalize(input);
  if (!text) return [{ text: "", offset: 0 }];
  if (!text.includes(ESC)) return [{ text, offset: 0 }];

  const state = resetState();
  const segments: AnsiSegment[] = [];
  let cursor = 0;
  CSI_PATTERN.lastIndex = 0;
  let match = CSI_PATTERN.exec(text);
  while (match !== null) {
    if (match.index > cursor) {
      segments.push({
        text: text.slice(cursor, match.index),
        offset: cursor,
        style: styleFor(state),
      });
    }
    if (match[3] === "m") applySgr(state, parseParams(match[1]));
    cursor = match.index + match[0].length;
    match = CSI_PATTERN.exec(text);
  }
  if (cursor < text.length) {
    segments.push({
      text: text.slice(cursor),
      offset: cursor,
      style: styleFor(state),
    });
  }
  if (segments.length === 0) return [{ text: "", offset: 0 }];
  return segments;
}
