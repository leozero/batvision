// Signs release builds with the key from the environment (D65): CI gets it from GitHub
// secrets. Without it, release builds keep Expo's default debug signature (tests).
const { withAppBuildGradle } = require("expo/config-plugins");

const SIGNING = `
    release {
        if (System.getenv("BATVISION_KEYSTORE")) {
            storeFile file(System.getenv("BATVISION_KEYSTORE"))
            storePassword System.getenv("BATVISION_KEYSTORE_PASSWORD")
            keyAlias System.getenv("BATVISION_KEY_ALIAS")
            keyPassword System.getenv("BATVISION_KEY_PASSWORD")
        }
    }`;

module.exports = function withReleaseSigning(config) {
  return withAppBuildGradle(config, (gradle) => {
    let src = gradle.modResults.contents;
    if (!src.includes("BATVISION_KEYSTORE")) {
      // Add a release signing config next to the debug one…
      src = src.replace(/signingConfigs\s*\{/, (m) => `${m}${SIGNING}`);
      // …and use it for release builds when the key is there.
      src = src.replace(
        /(buildTypes\s*\{[\s\S]*?release\s*\{[\s\S]*?)signingConfig signingConfigs\.debug/,
        `$1signingConfig System.getenv("BATVISION_KEYSTORE") ? signingConfigs.release : signingConfigs.debug`,
      );
    }
    gradle.modResults.contents = src;
    return gradle;
  });
};
