# Privacy policy

_Last updated: November 12, 2026_

BatVision is a client for a **self-hosted** media server: you (or someone you trust) run the BatVision server, and the apps connect to it. The developer of BatVision does not operate any server for you and does not receive your data.

## What the developer collects

**Nothing.** The BatVision apps contain no analytics, no advertising, no crash reporting service and no tracking of any kind. The developer receives no personal data, usage data or device identifiers from the apps.

## What the apps store and where it goes

- **On your device:** the address of your BatVision server, your device's sign-in token (encrypted with the Android keystore on Android, the system keychain on desktop), the movies and episodes you download, their pictures and subtitles, your playback positions and your download settings.
- **To your own BatVision server only:** your username, password and two-factor code when you sign in, your playback progress, and the downloads you request. This server is chosen and operated by you or your household; its administrator can see your account and viewing history there.
- **To GitHub:** to tell you about new versions, the apps read the public list of BatVision releases on `api.github.com`. Like any web request, it reveals your IP address to GitHub; nothing else is sent. See [GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

Movie and series information (titles, posters, ratings) is fetched by your server from [TMDB](https://www.themoviedb.org/), not by the apps.

## Permissions (Android)

- **Network access:** to reach your server.
- **Notifications and foreground service:** to show the progress of a download while it runs in the background, with a Pause button.

## Deleting your data

Signing out removes the sign-in token from the device. Uninstalling the app removes everything it stored, downloads included. Your account and history on the server are managed by its administrator, from the BatVision web interface.

## Children

BatVision is not directed at children under 13 and does not knowingly collect their data.

## Changes and contact

Changes to this policy are published at this address and in the [BatVision repository](https://github.com/leozero/batvision). Questions: [developer@leozero.fr](mailto:developer@leozero.fr).
