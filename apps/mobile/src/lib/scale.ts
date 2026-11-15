import { rem } from "nativewind";
import { Dimensions } from "react-native";

/**
 * Everything 1.5× on a large window (D105): Samsung DeX, a monitor or AR glasses show the
 * app as wide as a desktop, where phone sizes read too small. Tablets stay below the bar.
 */
export const LARGE_WINDOW = 1400;
const BASE_REM = 14;

export function uiScaleFor(windowWidth: number): number {
  return windowWidth >= LARGE_WINDOW ? 1.5 : 1;
}

let current = uiScaleFor(Dimensions.get("window").width);

/** The current scale: 1, or 1.5 on a large window. */
export function uiScale(): number {
  return current;
}

/** A size in points (icons, fixed widths) at the current scale. */
export function ui(points: number): number {
  return Math.round(points * current);
}

/**
 * Follows the window: Tailwind sizes (text, spacing) at once through `rem`; sizes given
 * with `ui()` at their next render.
 */
export function applyUiScale(windowWidth: number) {
  current = uiScaleFor(windowWidth);
  rem.set(BASE_REM * current);
}

applyUiScale(Dimensions.get("window").width);
