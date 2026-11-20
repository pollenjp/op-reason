# op-reason documents

## Where to start

![What you want to know → where to read](plantuml/out/01_map.svg)

- [README.md](../README.md): what op-reason is, installation, usage, things to know
- [README.ja.md](../README.ja.md): the same in Japanese
- [architecture/README.md](architecture/README.md): how it works now, the runner contract (op-reason.exe --run and the launcher), the log format
- [manual-verification.md](manual-verification.md) / [manual-verification.ja.md](manual-verification.ja.md): checks
  with the real 1Password app
- [skill/SKILL.md](../skill/SKILL.md): the guide for AI agents (`op-reason --skill`)
- [html/relay-screens.html](html/relay-screens.html): where the SSH relay's windows, toasts and notifications appear, as a
  page that plays them scene by scene (English, and Japanese with `#ja`)

## Diagrams

The diagrams are PlantUML (`plantuml/`, `architecture/plantuml/`) rendered to SVG in `out/`.

The animations of the READMEs are SVGs in `images/`, made by `images/make_readme_svgs.py`: change the script, not the
SVGs ([README.md, Development](../README.md#development)). The HTML page `html/relay-screens.html` plays the scene of
the relay's toasts with the same texts and times; the script's `--check` fails when they are no longer in it.
