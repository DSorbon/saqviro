# Security

Saqviro opens files from anywhere, so we treat bugs in how it reads them as security bugs.

## Reporting

Please use [GitHub's private vulnerability reporting](https://github.com/saqviro/saqviro/security/advisories/new). Do not open a public issue.

Include the Saqviro version, your system, and a way to reproduce it. A sample file helps most.

## What we care about most

- A file that makes Saqviro read, write or run something it should not.
- An archive that writes outside its target folder.
- An update that installs without a valid signature.
- Any data leaving the device without the user asking.

## How Saqviro is built to limit damage

- C and C++ parsers run as separate processes, not in the app.
- Files are processed on the device. The app connects to the network only when you press Check for updates.
- Updates are verified against a public key built into the app.
