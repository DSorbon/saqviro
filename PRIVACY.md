# Privacy

Saqviro runs on your computer. Your files never leave it.

## What the app does not do

- It does not upload your files or their contents anywhere.
- It has no account, no analytics, no ads and no telemetry.
- It makes no network request on its own, not even at startup.

## The one network request

**Check for updates** in Settings asks GitHub for the latest version. It happens only when you press the button. GitHub sees what it sees of any download: your IP address and the app's user agent. No file name, tool name or identifier is sent.

Tools that need the network would say so in their manifest with `permissions.network: true`. There are none today.

## What stays on your computer

- **Preferences**: theme, favorites, the results folder and your workflows, in the app's local storage.
- **Usage counts**: which tools you use, to rank them for you. Never which files. Clear them under Settings → History.
- **Temporary files**: each job has a private folder that is removed when the job ends, however it ends.

## Passwords and passphrases

What you type for PDF passwords, HMAC keys and encryption is used for that one job and then forgotten. It is not stored, and it is not passed to other programs on a command line. Password Strength Checker works entirely on this device.

## Files you open

Tools treat every file as untrusted. Video and PDF engines run as separate processes, archives cannot write outside their folder, and SVG files cannot pull in other files. See [SECURITY.md](SECURITY.md) to report a problem.
