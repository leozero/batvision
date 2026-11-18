import { Platform } from "react-native";

/**
 * Running on Android TV (D109): the same app, driven by a remote. No downloads, no touch,
 * a sidebar instead of the tab bar, sign-in by code (D110, D111, D112).
 */
export const IS_TV = Platform.isTV;

/**
 * On a TV, vertical lists keep the remote's element clear of the screen's top and bottom
 * (Android keeps a focused view this far from a fading edge), and fade at the bottom only:
 * React Native draws its fades whatever the scroll position (D112).
 */
export const TV_EDGE = IS_TV ? { start: 0, end: 96 } : undefined;

/** The bottom padding of those lists: on a TV, room to lift the last element out of the fade. */
export function tvTail(phone: string): string {
  return IS_TV ? "pb-28" : phone;
}

/** The gap between a poster and its title: on a TV, the ring and the zoom need room. */
export function tvGap(phone: string): string {
  return IS_TV ? "mt-4" : phone;
}
