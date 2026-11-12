import { PermissionsAndroid, Platform } from "react-native";

/** Android 13+: the download notification needs this permission (D72). */
export function askNotifications() {
  if (Number(Platform.Version) >= 33)
    void PermissionsAndroid.request(PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS);
}
