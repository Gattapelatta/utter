<p align="right"><a href="README.md"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/media/lang/ru-link-dark.svg"><img src="docs/media/lang/ru-link-light.svg" width="56" height="22" alt="Русский"></picture></a><picture><source media="(prefers-color-scheme: dark)" srcset="docs/media/lang/en-current-dark.svg"><img src="docs/media/lang/en-current-light.svg" width="76" height="22" alt="· English"></picture></p>

<p align="center"><img src="docs/media/hero-en.png" width="830" alt="Utter: the Drop by the notch listens, shows the words as you speak, and pastes the text"></p>

<p align="center"><a href="https://github.com/Gattapelatta/utter/releases/latest"><img src="docs/media/download-en.svg" width="311" alt="Download for Mac · macOS 13+"></a></p>

## What it does

- **Dictation.** Hold <kbd>Right ⌥</kbd>, speak in any language, let go: clean text lands in the app you're typing in — Notes, Slack, Telegram, your browser, your code editor.
- **Translation.** Hold <kbd>Right ⌘</kbd> and speak in your own language: the English translation is pasted instead, in a business or a casual style.
- **Live draft.** The words appear under the camera notch while you speak; the clean text follows a moment after you let go.
- **Cleanup.** "Um", "uh" and repeats go away, punctuation is added. The meaning is never changed.
- **The Drop.** A small character lives by the notch and shows what Utter is doing: listening, thinking, done.

Both keys can be changed in Settings. A long recording is processed while you are still speaking, so it is ready soon after you stop.

## Install

1. Download `Utter_…_universal.dmg` from the [latest release](https://github.com/Gattapelatta/utter/releases/latest), open it and drag **Utter** into **Applications**.
2. Open Utter. macOS will stop it the first time, because Utter isn't signed with a paid Apple certificate yet. Open **System Settings → Privacy & Security**, scroll down to **Security** and click **Open Anyway**, then **Open**. You only do this once.
3. Utter walks you through the rest: microphone access, Accessibility access (so it can paste), and a free Gemini key — get one at [aistudio.google.com/apikey](https://aistudio.google.com/apikey) and copy it; Utter picks it up from the clipboard.

Utter updates itself: when a new version is out, **About → Install** (or the menu bar icon) installs it after your current dictation.

Requires macOS 13 or later on Apple Silicon or Intel.

## Privacy

- Your voice goes straight from your Mac to the speech service you chose (Google Gemini by default, or Groq, or OpenAI) with **your own key**. Utter has no servers.
- The key is kept in the macOS Keychain. History stays on your Mac and can be turned off.
- On Gemini's free tier Google may use requests to improve its products; on a paid tier it doesn't. See [Google's terms](https://ai.google.dev/gemini-api/terms).

## Questions

**Is it really free?** Utter is. Gemini's free tier is enough for everyday dictation (several hundred requests a day across Utter's models); Groq has a free tier too. OpenAI is pay-as-you-go — it is not a ChatGPT subscription.

**Gemini says my location isn't supported.** Gemini isn't available in every country. A VPN to a supported country works, or choose Groq or OpenAI in Settings → Provider.

**After an update macOS asks for Keychain access.** Click **Always Allow**. This happens once per update because Utter is signed with its own certificate, not a paid Apple one. Your microphone and Accessibility permissions stay.

**Does it work on Intel Macs?** Utter is built for both Apple Silicon and Intel. It has been tested on Apple Silicon with macOS 15; Intel hasn't been tested yet — please open an issue if something is off.

## Roadmap

- Windows version.
- A paid Apple signature, so the first launch needs no extra step.

## Contributing

Bug reports and ideas are welcome in [Issues](https://github.com/Gattapelatta/utter/issues). Utter is a [Tauri 2](https://tauri.app) app: Rust in `src-tauri`, React in `src`.

```bash
pnpm install
pnpm test                                          # UI tests
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm tauri dev
```

## License

[MIT](LICENSE)
