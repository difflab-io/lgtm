# CLI module diagrams

Each `.mmd` file generates an adaptive SVG and light and dark PNGs. The module pages use SVG image links. Browsers choose the light or dark theme through `prefers-color-scheme`.

To regenerate the diagrams in Neovim, run:

```vim
:MmdToImgAll docs/architecture/cli/diagrams
```

The renderer needs `mmdc` on your PATH. Commit the generated images with their sources.
