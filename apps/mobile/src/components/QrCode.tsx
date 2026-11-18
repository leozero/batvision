import { create } from "qrcode";
import { useMemo } from "react";
import Svg, { Path, Rect } from "react-native-svg";

/** Quiet zone around the code, in modules: phones read it more easily. */
const MARGIN = 4;

/** A QR code drawn with react-native-svg: black on white, whatever the theme. */
export function QrCode({ value, size, testID }: { value: string; size: number; testID?: string }) {
  const { d, side } = useMemo(() => {
    const { modules } = create(value, { errorCorrectionLevel: "M" });
    let path = "";
    for (let row = 0; row < modules.size; row++)
      for (let col = 0; col < modules.size; col++)
        if (modules.get(row, col)) path += `M${col + MARGIN} ${row + MARGIN}h1v1h-1z`;
    return { d: path, side: modules.size + MARGIN * 2 };
  }, [value]);
  return (
    <Svg testID={testID} width={size} height={size} viewBox={`0 0 ${side} ${side}`}>
      <Rect width={side} height={side} fill="#ffffff" />
      <Path d={d} fill="#000000" />
    </Svg>
  );
}
