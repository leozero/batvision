// Expo's defaults already handle the pnpm workspace (shared packages in ../../packages).
const { getDefaultConfig } = require("expo/metro-config");
const { withNativeWind } = require("nativewind/metro");

module.exports = withNativeWind(getDefaultConfig(__dirname), { input: "./src/global.css" });
