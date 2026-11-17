import { type ComponentProps, createContext, useContext, useState } from "react";
import { Pressable as NativePressable, type StyleProp, type ViewStyle } from "react-native";
import { COLORS } from "../lib/colors";
import { IS_TV } from "../lib/tv";

type Props = Omit<ComponentProps<typeof NativePressable>, "style"> & {
  style?: StyleProp<ViewStyle>;
  /**
   * A card (a poster, an episode's still): the ring goes around its picture, which calls
   * `useFocusRing()`, rather than around the whole card with its title.
   */
  ringOnChild?: boolean;
};

/**
 * Where the remote is (D112): an amber ring drawn just outside the element, following its
 * rounded corners.
 */
const RING: ViewStyle = {
  outlineColor: COLORS.accent,
  outlineStyle: "solid",
  outlineWidth: 3,
  outlineOffset: 3,
};
/** A picture also grows a little. */
const PICTURE: ViewStyle = { ...RING, transform: [{ scale: 1.04 }] };

const Focused = createContext(false);

/** The ring for the picture of a card whose Pressable has `ringOnChild`. */
export function useFocusRing(): ViewStyle | null {
  return useContext(Focused) ? PICTURE : null;
}

/**
 * React Native's Pressable, which on a TV shows which element the remote is on. Phones and
 * tablets get the plain one.
 */
export function Pressable({ style, onFocus, onBlur, ringOnChild, children, ...props }: Props) {
  const [focused, setFocused] = useState(false);
  if (!IS_TV)
    return (
      <NativePressable style={style} onFocus={onFocus} onBlur={onBlur} {...props}>
        {children}
      </NativePressable>
    );
  return (
    <NativePressable
      {...props}
      onFocus={(e) => {
        setFocused(true);
        onFocus?.(e);
      }}
      onBlur={(e) => {
        setFocused(false);
        onBlur?.(e);
      }}
      style={[style, focused && !ringOnChild ? RING : null]}
    >
      {(state) => (
        <Focused.Provider value={focused}>
          {typeof children === "function" ? children(state) : children}
        </Focused.Provider>
      )}
    </NativePressable>
  );
}
