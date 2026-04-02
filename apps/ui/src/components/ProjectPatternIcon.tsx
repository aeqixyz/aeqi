import { useEffect, useMemo, useState } from "react";

const BAYER_4X4 = [
  [0, 8, 2, 10],
  [12, 4, 14, 6],
  [3, 11, 1, 9],
  [15, 7, 13, 5],
] as const;

function hashString(value: string): number {
  let hash = 2166136261;
  for (let i = 0; i < value.length; i++) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t ^= t + Math.imul(t ^ (t >>> 7), 61 | t);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const hue = ((h % 360) + 360) % 360;
  const sat = Math.max(0, Math.min(100, s)) / 100;
  const light = Math.max(0, Math.min(100, l)) / 100;
  const c = (1 - Math.abs(2 * light - 1)) * sat;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = light - c / 2;
  let r = 0, g = 0, b = 0;
  if (hue < 60) { r = c; g = x; }
  else if (hue < 120) { r = x; g = c; }
  else if (hue < 180) { g = c; b = x; }
  else if (hue < 240) { g = x; b = c; }
  else if (hue < 300) { r = x; b = c; }
  else { r = c; b = x; }
  return [
    Math.round((r + m) * 255),
    Math.round((g + m) * 255),
    Math.round((b + m) * 255),
  ];
}

function makePatternDataUrl(seed: string, logicalSize = 22, cellSize = 2): string {
  if (typeof document === "undefined") return "";
  const canvas = document.createElement("canvas");
  canvas.width = logicalSize * cellSize;
  canvas.height = logicalSize * cellSize;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";

  const rand = mulberry32(hashString(seed));
  const hue = Math.floor(rand() * 360);
  const [offR, offG, offB] = hslToRgb(hue, 54 + Math.floor(rand() * 14), 36 + Math.floor(rand() * 12));
  const [onR, onG, onB] = hslToRgb(hue + (rand() > 0.5 ? 10 : -10), 86 + Math.floor(rand() * 10), 82 + Math.floor(rand() * 10));

  const center = (logicalSize - 1) / 2;
  const half = Math.max(center, 1);
  const gradientAngle = rand() * Math.PI * 2;
  const gradientDirX = Math.cos(gradientAngle);
  const gradientDirY = Math.sin(gradientAngle);
  const maxProjection = Math.abs(gradientDirX * half) + Math.abs(gradientDirY * half);
  const diagFreq = 0.34 + rand() * 0.12;
  const antiDiagFreq = 0.33 + rand() * 0.12;
  const diagPhase = rand() * Math.PI * 2;
  const antiDiagPhase = rand() * Math.PI * 2;

  ctx.fillStyle = `rgb(${offR} ${offG} ${offB})`;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = `rgb(${onR} ${onG} ${onB})`;
  const dotRadius = cellSize * 0.46;

  for (let y = 0; y < logicalSize; y++) {
    const dy = y - center;
    for (let x = 0; x < logicalSize; x++) {
      const dx = x - center;
      const projection = dx * gradientDirX + dy * gradientDirY;
      const gradient = (projection / maxProjection + 1) * 0.5;
      const diagonal = Math.sin((dx + dy) * diagFreq + diagPhase) * 0.5 + 0.5;
      const antiDiagonal = Math.sin((dx - dy) * antiDiagFreq + antiDiagPhase) * 0.5 + 0.5;
      const hatch = diagonal * 0.5 + antiDiagonal * 0.5;
      const signal = Math.max(0, Math.min(1, gradient + (hatch - 0.5) * 0.22));
      const level = Math.max(0, Math.min(15, Math.floor(signal * 16)));
      const thresholdIndex = BAYER_4X4[y & 3]![x & 3]!;
      if (level <= thresholdIndex) continue;
      const cx = x * cellSize + cellSize / 2;
      const cy = y * cellSize + cellSize / 2;
      ctx.beginPath();
      ctx.arc(cx, cy, dotRadius, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  return canvas.toDataURL("image/png");
}

interface Props {
  name: string;
  selected?: boolean;
}

export default function ProjectPatternIcon({ name, selected }: Props) {
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  const patternDataUrl = useMemo(
    () => makePatternDataUrl(name.trim().toLowerCase()),
    [name],
  );

  return (
    <div className={`project-icon ${selected ? "project-icon-selected" : ""}`}>
      {patternDataUrl ? (
        <img
          src={patternDataUrl}
          alt=""
          aria-hidden="true"
          className="project-icon-pattern"
        />
      ) : (
        <div className="project-icon-fallback" />
      )}
      <span className="project-icon-initial">{initial}</span>
    </div>
  );
}
