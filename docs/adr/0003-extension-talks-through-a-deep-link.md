# The Extension talks to the app through a `grapplehook://` deep link

The Extension hands a link to Grapplehook by opening `grapplehook://open?url=<encoded URL>&quality=<value>`, where `quality` is optional and takes the same values as the app's quality picker. The operating system starts Grapplehook, or passes the link to the copy already running. We chose this over native messaging, which needs a host manifest installed for each browser and each OS, and over a local HTTP server, which any web page could call and which would need its own authentication.

## Consequences

- **Any web page can open a `grapplehook://` link,** not only the Extension. The app treats every field as untrusted: it accepts only `http(s)` URLs, and it never starts a download by itself.
- **The link format is a public contract.** The Extension lives in its own repo and ships through browser stores, so old versions stay installed for a long time. The app may add new optional fields, but it never renames or removes a field or changes what one means. It ignores fields it doesn't know.
- **Linux AppImage registers the scheme itself.** An AppImage has no installer, so Grapplehook registers `grapplehook://` for the current user every time it starts. If the AppImage is moved or deleted, links do nothing until it is run again.
