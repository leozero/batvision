import { useState } from "react";
import { type LayoutChangeEvent, useWindowDimensions } from "react-native";
import { uiScaleFor } from "./scale";

/**
 * Poster sizes that follow the window (D105): a phone keeps three columns, a tablet a few
 * more, and a large window (Samsung DeX, a monitor, AR glasses) gets bigger posters rather
 * than a dozen small ones, 1.5× like everything there.
 */
export function posterTarget(windowWidth: number): number {
  // In the window's own units: 1.5× bigger on a large window, like the rest.
  const scale = uiScaleFor(windowWidth);
  const width = windowWidth / scale;
  return (width < 600 ? 110 : width < 1100 ? 140 : 180) * scale;
}

/** Columns and the exact width of each poster in a grid `inner` wide. */
export function posterGrid(
  inner: number,
  windowWidth: number,
  gap = 12,
): { columns: number; itemWidth: number } {
  const target = posterTarget(windowWidth);
  const columns = Math.max(3, Math.floor((inner + gap) / (target + gap)));
  const itemWidth = Math.max(0, Math.floor((inner - gap * (columns - 1)) / columns));
  return { columns, itemWidth };
}

/** A poster in a horizontal row (Home): wider on large windows. */
export function rowPosterWidth(windowWidth: number): number {
  const scale = uiScaleFor(windowWidth);
  return Math.round(Math.min(200, Math.max(128, windowWidth / scale / 9)) * scale);
}

/** A grid filling the window, `padding` on each side (unscaled points). */
export function useWindowGrid(padding: number, gap = 12) {
  const { width } = useWindowDimensions();
  const scale = uiScaleFor(width);
  return posterGrid(width - padding * 2 * scale, width, gap * scale);
}

/**
 * A grid filling its container, measured once laid out (the window width minus `guess`
 * until then).
 */
export function useMeasuredGrid(guess: number, gap = 12) {
  const { width } = useWindowDimensions();
  const scale = uiScaleFor(width);
  const [inner, setInner] = useState<number | null>(null);
  const onLayout = (e: LayoutChangeEvent) => setInner(e.nativeEvent.layout.width);
  return { ...posterGrid(inner ?? width - guess * scale, width, gap * scale), onLayout };
}
