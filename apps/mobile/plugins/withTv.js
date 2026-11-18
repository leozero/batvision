// Android TV (D109): the phone's APK also installs on a TV. The TV launcher lists apps that
// open from LEANBACK_LAUNCHER and show a banner; Play offers the app to TVs only when
// neither a touchscreen nor the leanback feature are required.
const { copyFileSync, mkdirSync } = require("node:fs");
const { join } = require("node:path");
const {
  AndroidConfig,
  withAndroidManifest,
  withDangerousMod,
  withPodfile,
} = require("expo/config-plugins");

const BANNER = "tv_banner";

function addFeature(manifest, name) {
  manifest["uses-feature"] ??= [];
  const features = manifest["uses-feature"];
  const existing = features.find((f) => f.$["android:name"] === name);
  if (existing) existing.$["android:required"] = "false";
  else features.push({ $: { "android:name": name, "android:required": "false" } });
}

function withTvManifest(config) {
  return withAndroidManifest(config, (mod) => {
    const manifest = mod.modResults.manifest;
    addFeature(manifest, "android.software.leanback");
    addFeature(manifest, "android.hardware.touchscreen");

    const application = AndroidConfig.Manifest.getMainApplicationOrThrow(mod.modResults);
    application.$["android:banner"] = `@drawable/${BANNER}`;

    const activity = AndroidConfig.Manifest.getMainActivityOrThrow(mod.modResults);
    const launcher = (activity["intent-filter"] ?? []).find((filter) =>
      (filter.action ?? []).some((a) => a.$["android:name"] === "android.intent.action.MAIN"),
    );
    if (launcher) {
      launcher.category ??= [];
      const name = "android.intent.category.LEANBACK_LAUNCHER";
      if (!launcher.category.some((c) => c.$["android:name"] === name))
        launcher.category.push({ $: { "android:name": name } });
    }
    return mod;
  });
}

/** The banner the TV launcher shows (320 × 180, rendered by `pnpm graphics`). */
function withTvBanner(config) {
  return withDangerousMod(config, [
    "android",
    (mod) => {
      const res = join(mod.modRequest.platformProjectRoot, "app/src/main/res/drawable-xhdpi");
      mkdirSync(res, { recursive: true });
      copyFileSync(
        join(mod.modRequest.projectRoot, "assets/tv-banner.png"),
        join(res, `${BANNER}.png`),
      );
      return mod;
    },
  ]);
}

const PODS_MARK = "BatVision: react-native-tvos";

/**
 * `react-native` is react-native-tvos under an npm alias: Node resolves it to a folder named
 * `react-native-tvos`, where the podspecs of react-native-svg and Reanimated look for
 * `react-native/package.json` and stop ("no implicit conversion of nil into String"). They
 * take the folder holding `react-native` from REACT_NATIVE_NODE_MODULES_DIR instead: the
 * app's own node_modules, where the alias points to the fork.
 */
function withTvosPods(config) {
  return withPodfile(config, (mod) => {
    if (!mod.modResults.contents.includes(PODS_MARK))
      mod.modResults.contents = `# ${PODS_MARK} (D109): where podspecs find \`react-native\`.
ENV['REACT_NATIVE_NODE_MODULES_DIR'] ||= File.expand_path('../node_modules', __dir__)

${mod.modResults.contents}`;
    return mod;
  });
}

module.exports = function withTv(config) {
  return withTvosPods(withTvBanner(withTvManifest(config)));
};
