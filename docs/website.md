# The documentation website

Every Markdown page of this repository is also a page of a static website: `docs/index.html` (this folder's
[README](README.md)) and `docs/site/` (the other pages, their search index, styles and script), with the
images left where they are in `docs/media/`. Links between pages go to their HTML, and links to code and
other files go to the repository on GitHub.

## Publishing it with GitHub Pages

1. On GitHub, open the repository's **Settings > Pages**.
2. Under **Build and deployment**, choose **Source: Deploy from a branch**, then the branch (the default
   branch, or another one to preview it) and the folder **/docs**, and **Save**.
3. A minute later the site is at `https://<owner>.github.io/<repository>/` (for this repository,
   `https://winnerfn8.github.io/Rusty-Minecraft/`). `docs/.nojekyll` makes Pages serve the files as they are.

No workflow is needed: the HTML is in the repository.

## Keeping it current

The pages are made from the Markdown by `tools/build_site.py`. After changing a page, regenerate them and commit
both:

```sh
pip install -r tools/site-requirements.txt   # markdown-it-py, once
python3 tools/build_site.py                  # writes docs/index.html and docs/site/
python3 tools/build_site.py --check          # says whether the site is up to date
```

The navigation follows [docs/README.md](README.md)'s sections, in order, then groups the remaining pages by folder
(API reference, benchmarks, the project's own files, crates and tools). The colours are the design system's
tokens (`crates/horizon-theme/tokens.css`, [design-system.md](design-system.md)), light or dark as the reader's
system prefers, with a switch in the top bar. Search runs in the browser over `docs/site/search.json`; `/`
focuses it.
