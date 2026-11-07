<p align="right"><a href="README.md"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/media/lang/ru-link-dark.svg"><img src="docs/media/lang/ru-link-light.svg" width="56" height="22" alt="Русский"></picture></a><picture><source media="(prefers-color-scheme: dark)" srcset="docs/media/lang/en-current-dark.svg"><img src="docs/media/lang/en-current-light.svg" width="76" height="22" alt="· English"></picture></p>

<p align="center"><img src="docs/media/hero-en.png" width="830" alt="Utter: the Drop by the notch listens, shows the words as you speak, and pastes the text"></p>

<p align="center"><a href="https://github.com/Gattapelatta/utter/releases/latest"><img src="docs/media/download-en.svg" width="311" alt="Download for Mac · macOS 13+"></a></p>

## What is it

Utter is free dictation for your Mac. Hold a key, talk, let go — and the text is right where your cursor was. Telegram, Notes, Slack, your browser, your code — anywhere.

It also translates: speak in your own language and get English pasted instead. Proper, natural English — business or casual, your call.

## What it does

- **Dictation.** Hold <kbd>Right ⌥</kbd>, talk in any language, let go — done. The text comes out clean, no "um"s and "uh"s.
- **Translation to English.** Same thing, just with <kbd>Right ⌘</kbd>. Speak your language, get English. Business or casual style, you pick.
- **You see what it hears.** While you talk, the words show up right under the notch. Let go of the key and a moment later the clean version is pasted.
- **Cleans up, doesn't rewrite.** It drops the "um"s, "uh"s and repeats and adds the commas and periods. The meaning stays yours. Always.
- **The Drop.** A tiny character living by the notch. One look and you know what's going on: listening, thinking, or all done.

You can change both keys in Settings. And yes, you can talk for a long time: Utter works on the recording while you're still speaking, so the text is ready almost as soon as you stop.

## How to install

1. Grab `Utter_…_universal.dmg` from the [latest release](https://github.com/Gattapelatta/utter/releases/latest), open it and drag **Utter** into **Applications**.
2. Open it. The first time, macOS won't let it in — that's expected: Utter doesn't have a paid Apple signature yet. Go to **System Settings → Privacy & Security**, scroll down to **Security** and click **Open Anyway**, then **Open**. You only do this once.
3. From there Utter walks you through it: microphone access, Accessibility (so it can paste), and a free Gemini key. Get the key at [aistudio.google.com/apikey](https://aistudio.google.com/apikey) and just copy it — Utter picks it up from the clipboard.

Utter updates itself too. When a new version is out, click **About → Install** (or use the menu bar icon). It installs as soon as you finish dictating — nothing gets cut off mid-sentence.

Works on macOS 13 and newer, on any Mac — Apple Silicon or Intel.

## What about privacy?

- Your voice goes straight from your Mac to the speech service you picked: Google Gemini by default, or Groq, or OpenAI. And it goes **with your own key**. Utter has no servers of its own. None.
- The key lives in the macOS Keychain. Your dictation history stays on your Mac, and you can turn it off.
- Heads-up: on Gemini's free tier Google may use requests to improve its products. On a paid tier it doesn't. More in [Google's terms](https://ai.google.dev/gemini-api/terms).

## FAQ

**Is it really free?**<br>
Utter is, completely. Gemini's free tier is plenty for everyday dictation — several hundred requests a day. Groq has a free tier too. OpenAI is pay-as-you-go, and it's not the same thing as a ChatGPT subscription.

**Gemini says my location isn't supported.**<br>
Yeah, Gemini isn't available everywhere. Turn on a VPN to a country where it works — or just switch to Groq or OpenAI in **Settings → Provider**.

**After an update my Mac asks for Keychain access.**<br>
Click **Always Allow**. It happens once after each update, because Utter is signed with my own signature, not a paid Apple one. Your microphone and Accessibility permissions stay put.

**Does it work on Intel Macs?**<br>
It should: Utter is built for both Apple Silicon and Intel. But I've only tested it on Apple Silicon with macOS 15 — I don't have an Intel Mac. If something's off, let me know in [Issues](https://github.com/Gattapelatta/utter/issues).

## What's next

- A Windows version.
- A paid Apple signature, so the first launch doesn't send you into Settings.
- **The Drop for AI agents.** You know how it goes: you give Claude or ChatGPT a task, switch to another window — and forget about it, or just don't know when it's done. I want the Drop to keep an eye on the agent and tell you when the task is finished. You'll be able to hook up ChatGPT or Claude, your pick.

## Want to help?

Found a bug or have an idea — drop it in [Issues](https://github.com/Gattapelatta/utter/issues), I'd love to hear it. And if you want to dig into the code: Utter is built with [Tauri 2](https://tauri.app), the Rust side lives in `src-tauri`, the React UI in `src`.

```bash
pnpm install
pnpm test                                          # UI tests
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm tauri dev
```

## License

[MIT](LICENSE) — take it and use it.
