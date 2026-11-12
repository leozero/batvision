import type { ConfigContext, ExpoConfig } from "expo/config";

/** 0.2.0 → 200: each release must have a higher versionCode to install over the previous one. */
function versionCode(version: string) {
  const [major = 0, minor = 0, patch = 0] = version
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);
  return major * 10_000 + minor * 100 + patch;
}

export default ({ config }: ConfigContext): ExpoConfig => {
  const version = config.version ?? "0.0.0";
  return {
    ...config,
    name: config.name ?? "batvision",
    slug: config.slug ?? "batvision",
    version,
    android: { ...config.android, versionCode: versionCode(version) },
    plugins: [...(config.plugins ?? []), "./plugins/withReleaseSigning"],
  };
};
