# Architecture diagrams

Each `.mmd` source produces an adaptive SVG and light and dark PNGs. The architecture guide links to the SVGs. Browsers select the theme through `prefers-color-scheme`.

MmdToImg renders a diagram when you save its source in Neovim. To regenerate all diagrams, run:

```vim
:MmdToImgAll docs/architecture/diagrams
```

The renderer needs the Mermaid CLI (`mmdc`) on your PATH. Commit generated images with their sources. Readers can then view the diagrams without Neovim.
