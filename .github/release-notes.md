Desktop and Android app installers. They aren't signed yet, so each system warns the first time:

- **macOS** (`.dmg`, `arm64` for Apple Silicon, `x64` for Intel): drag batvision to Applications, open it once, then allow it in System Settings → Privacy & Security → "Open Anyway". If macOS says the app is damaged, run `xattr -dr com.apple.quarantine /Applications/batvision.app`.
- **Windows** (`.exe`): SmartScreen shows "Windows protected your PC": click "More info" → "Run anyway".
- **Linux** (`.AppImage`): `chmod +x` the file, then run it.
- **Android** (`-android.apk`): open the file on the phone and allow installing apps from your browser when Android asks. Later versions show a banner in the app.

On first launch the app asks for your server's address.

The matching server image is `ghcr.io/leozero/batvision` tagged with the version number (without the `v`).
