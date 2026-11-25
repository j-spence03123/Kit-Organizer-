# Built-in training

Put a configuration exported from Kit Organizer here to build your training
into the app:

1. In the app: **Settings → Export settings, categories & learned rules**.
2. Save it as `starter/kit-organizer-config.json` (this folder).
3. Build: `npm run tauri build`. The build log says
   *"Bundling learned rules from starter/kit-organizer-config.json"*.

Anyone who installs that build gets, on first launch:

- **all your learned rules** (merged with any of their own);
- **your categories**, only if they have never saved their own;
- **not** your personal settings (output folder, budget, etc.).

When you ship a newer build with more training, the new rules are merged on
its first launch. Replace the file and rebuild whenever you want to update it.

**Privacy:** the file contains folder and file names from your library (as
examples in the rules). It is listed in `.gitignore` so it is not pushed to
GitHub by accident; remove that line if you do want it in the repository.

Folder-based decisions only help people who have the same packs with the same
folder names; name-based rules help everyone.
