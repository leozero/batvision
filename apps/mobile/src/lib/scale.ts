import { rem } from "nativewind";
import { Dimensions } from "react-native";

/**
 * Everything 1.5× on a large window (D105): Samsung DeX, a monitor or AR glasses show the
 * app as wide as a desktop, where phone sizes read too small. Tablets stay below the bar.
 *
 * Decided once, when the app starts: texts are measured with the sizes they first had, so a
 * scale changed while they're on screen leaves them cut. A window resized past the bar gets
 * its scale at the next start.
 */
export const LARGE_WINDOW = 1400;
const BASE_REM = 14;

const scale = Dimensions.get("window").width >= LARGE_WINDOW ? 1.5 : 1;
// Before anything renders. Only when it differs: setting it restyles every text.
if (rem.get() !== BASE_REM * scale) rem.set(BASE_REM * scale);

/** The scale for this run: 1, or 1.5 on a large window. */
export function uiScale(): number {
  return scale;
}

/** A size in points (icons, fixed widths) at this run's scale. */
export function ui(points: number): number {
  return Math.round(points * scale);
}
