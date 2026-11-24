# Publishing HomeNet Watch on GitHub

Once this is set up, you publish a new version by creating a **Release** on GitHub.
GitHub then builds the app for Windows, Mac and Linux by itself (about 10–15 minutes),
attaches the downloads to the release, and everyone's copy of HomeNet Watch can install
it from **About → Check for updates**.

## One-time setup

1. **Create the repository.** On github.com click **New repository**, name it (for example
   `homenet-watch`) and make it **Public** (the in-app updates can only read public releases).
2. **Upload the project.** Easiest with **GitHub Desktop**: *File → Add local repository*,
   choose this `app` folder, let it create the repository, then *Publish repository* to the
   one you just made. Everything in the folder goes up, including the hidden `.github` folder,
   `Cargo.lock` and `vendor` — they're all needed. (The `target` and `shots` folders are left out
   automatically.)
3. **Allow the build to publish files.** In the repository go to *Settings → Actions → General →
   Workflow permissions* and choose **Read and write permissions**. Save.

## Publishing a version

1. Make sure the version number in `Cargo.toml` is the one you want (ask Craft Anything:
   *"get version 0.7.0 ready to publish"* — it also updates the "What's new" list in About).
   Upload the changes with GitHub Desktop (*Commit*, then *Push*).
2. On GitHub open **Releases → Draft a new release**.
3. Under **Choose a tag**, type the version with a `v` in front, e.g. `v0.7.0`, and pick
   *Create new tag on publish*. The tag must match the version in `Cargo.toml`, or the build stops
   and tells you.
4. Give it a title and write what's new (this text is shown in the app's update box).
   Click **Publish release**.
5. Watch the **Actions** tab. When the "Release" run turns green, the release has these files:
   - `homenet-watch-windows-x86_64.exe`
   - `homenet-watch-macos-universal` (Intel and Apple silicon Macs)
   - `homenet-watch-linux-x86_64`
   - `SHA256SUMS.txt` (fingerprints; the app refuses any download that doesn't match)

## What people see

- **First download:** they get the file for their system from the release page.
- **Updates:** *About → Check for updates* (and a quiet check when the app starts, which they can
  turn off). It downloads the new file, checks its fingerprint, swaps it in and offers
  **Restart now**. Their devices, settings and history are kept.
- **Unsigned-app warnings:** the program isn't code-signed yet, so the first time:
  - *Windows* may show "Windows protected your PC" → **More info → Run anyway**.
  - *Mac*: open it with right-click → **Open** (or *System Settings → Privacy & Security →
    Open Anyway*). On Mac and Linux the downloaded file may also need to be made runnable:
    `chmod +x homenet-watch-*` in Terminal.
  Buying a code-signing certificate (Windows) and an Apple Developer membership (Mac) removes
  these warnings; that can be added to this build later.
- **Copies in protected folders** (like *Program Files*) can't replace themselves; the app then
  points people to the release page instead.

## Testing a build without publishing

In **Actions → Release → Run workflow** you can build all three versions without making a
release; the files appear at the bottom of that run as *homenet-watch-all*.
