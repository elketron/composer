# Releasing

`.github/workflows/release.yml` runs on a `v*` tag and publishes a GitHub release with:

| file | what |
|------|------|
| `composer-<v>-linux-x86_64.tar.gz` | the `composer` binary (server, TUI, CLI); also for WSL |
| `composer-app-<v>-android.apk` | the phone app (Android) |
| `composer-desktop-<v>-linux-x86_64.tar.gz` | the desktop app (`apps/desktop`, CEF beside the binary) |

```sh
git tag v0.1.0 && git push origin v0.1.0
```

A tag with a `-` (`v0.2.0-rc1`) makes a pre-release. To try the builds without publishing, run the
workflow by hand (Actions → release → Run workflow, any version); the files are in the run's
artifacts.

## Android signing (once)

Android only updates an app signed with the same key, so make one key and keep it safe:

```sh
keytool -genkeypair -keystore composer-release.jks -alias composer -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 composer-release.jks | gh secret set ANDROID_KEYSTORE_BASE64
gh secret set ANDROID_KEYSTORE_PASSWORD     # prompts
gh secret set ANDROID_KEY_ALIAS --body composer
gh secret set ANDROID_KEY_PASSWORD          # prompts
```

Without these secrets the APK is signed with a throwaway debug key (the run warns): fine for a
try, but every build then needs an uninstall first.

## Versions

The workspace version (`Cargo.toml`) and the app's (`apps/composer_app/pubspec.yaml`) are what
`/api/info` and the app report; bump them before tagging. The tag sets the file names and the
app's build name; the Android version code is the workflow run number.
