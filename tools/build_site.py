#!/usr/bin/env python3
"""Builds the documentation website in docs/, for GitHub Pages ("Deploy from a branch", folder
/docs): every Markdown page of the repository becomes an HTML page under docs/site/, docs/README.md
becomes the home page docs/index.html, and images stay where they are (docs/media/). Links between
pages point at their HTML; links to code and other files point at the repository on GitHub.

    python3 tools/build_site.py            # write docs/index.html and docs/site/
    python3 tools/build_site.py --check    # fail when the site is out of date

Needs markdown-it-py: pip install -r tools/site-requirements.txt
"""

import argparse
import html
import json
import posixpath
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "https://github.com/winnerfn8/Rusty-Minecraft"
SITE = "docs/site"
HOME = "docs/index.html"
SITE_NAME = "Rusty Minecraft docs"
# Pages left out: GitHub's own templates, a copy of an upstream crate, agent skills, CLAUDE.md (a
# link to AGENTS.md).
EXCLUDE = (".github/", "third_party/", ".claude/", "CLAUDE.md")
# Groups after the ones docs/README.md defines, by where a page lives.
FOLDER_GROUPS = [
    ("API reference", lambda p: p.startswith("docs/api/")),
    ("Benchmarks and performance", lambda p: p.startswith(("benchmarks/", "docs/performance/", "launcher/benchmarks/"))
     or ("benchmark" in p and p.startswith("docs/"))),
    ("Project", lambda p: "/" not in p or p.startswith("roadmap/")),
    ("Crates, launcher and tools", lambda p: p.startswith(("crates/", "launcher/", "tools/", "packaging/"))),
]


def sources():
    """The Markdown files the site is made of, as repository paths."""
    out = subprocess.check_output(["git", "ls-files", "*.md"], cwd=ROOT, text=True).split()
    return sorted(p for p in out if not p.startswith(EXCLUDE) and p not in EXCLUDE)


def output_of(src):
    """Where a page is written: docs/README.md is the home, docs/x.md docs/site/x.html, anything
    else docs/site/repo/<path>.html."""
    if src == "docs/README.md":
        return HOME
    stem = src[: -len(".md")] + ".html"
    if src.startswith("docs/"):
        return f"{SITE}/{stem[len('docs/'):]}"
    return f"{SITE}/repo/{stem}"


def relative(target, page):
    """`target` (a repository path) as seen from the page written at `page`."""
    return posixpath.relpath(target, posixpath.dirname(page) or ".")


def slug(text, seen):
    """GitHub's heading anchor: lower case, punctuation dropped, spaces as hyphens; repeats
    numbered."""
    s = re.sub(r"[^\w\- ]", "", text.strip().lower()).replace(" ", "-")
    base, n = s, seen.get(s, 0)
    seen[s] = n + 1
    return base if n == 0 else f"{base}-{n}"


_DIRS = {}


def dirs_of(files):
    """Every folder holding a tracked file (computed once per file set)."""
    key = id(files)
    if key not in _DIRS:
        dirs = set()
        for f in files:
            parts = f.split("/")[:-1]
            for i in range(1, len(parts) + 1):
                dirs.add("/".join(parts[:i]))
        _DIRS[key] = dirs
    return _DIRS[key]


def resolve(href, src, page, pages, files):
    """A link of page `src` (written at `page`) as the site serves it."""
    if not href or re.match(r"^[a-z][a-z0-9+.-]*:", href, re.I) or href.startswith(("#", "//")):
        return href
    path, _, fragment = href.partition("#")
    if not path:
        return href
    target = posixpath.normpath(posixpath.join(posixpath.dirname(src), path))
    if target.startswith(".."):
        return href
    anchor = f"#{fragment}" if fragment else ""
    if target in pages:
        return relative(output_of(target), page) + anchor
    if target.startswith("docs/") and target in files:
        return relative(target, page) + anchor
    if target in files or target in dirs_of(files):
        kind = "blob" if target in files else "tree"
        return f"{REPO}/{kind}/HEAD/{target}{anchor}"
    return href


def image(src_attr, src, page, files):
    """An image's address: beside the site when it is under docs/, else the repository's copy."""
    if not src_attr or re.match(r"^[a-z][a-z0-9+.-]*:", src_attr, re.I) or src_attr.startswith("//"):
        return src_attr
    target = posixpath.normpath(posixpath.join(posixpath.dirname(src), src_attr))
    if target.startswith("docs/"):
        return relative(target, page)
    if target in files:
        return f"{REPO}/raw/HEAD/{target}"
    return src_attr


def text_of(tokens):
    return "".join(t.content for t in tokens if t.type in ("text", "code_inline"))


def render(md, src, page, pages, files):
    """A page's HTML body, title, table of contents and plain text."""
    text = (ROOT / src).read_text(encoding="utf-8")
    tokens = md.parse(text)
    seen, toc, title = {}, [], None
    attr_href = re.compile(r'(\b(?:href|src)\s*=\s*")([^"]*)(")')

    def raw(content):
        def fix(m):
            if m.group(1).lstrip().startswith("src"):
                return m.group(1) + image(m.group(2), src, page, files) + m.group(3)
            return m.group(1) + resolve(m.group(2), src, page, pages, files) + m.group(3)
        return attr_href.sub(fix, content)

    for i, t in enumerate(tokens):
        if t.type == "heading_open":
            inline = tokens[i + 1]
            label = text_of(inline.children or [])
            anchor = slug(label, seen)
            t.attrSet("id", anchor)
            level = int(t.tag[1])
            if level == 1 and title is None:
                title = label
            elif level in (2, 3):
                toc.append((level, anchor, label))
        elif t.type == "html_block":
            t.content = raw(t.content)
        elif t.type == "inline":
            for c in t.children or []:
                if c.type == "link_open":
                    c.attrSet("href", resolve(c.attrGet("href"), src, page, pages, files))
                elif c.type == "image":
                    c.attrSet("src", image(c.attrGet("src"), src, page, files))
                elif c.type == "html_inline":
                    c.content = raw(c.content)
    body = md.renderer.render(tokens, md.options, {})
    # Wide tables scroll on their own instead of the page.
    body = body.replace("<table>", '<div class="table"><table>').replace("</table>", "</table></div>")
    plain = re.sub(r"\s+", " ", re.sub(r"<[^>]+>", " ", body))
    return body, title or posixpath.basename(src), toc, html.unescape(plain).strip()


def groups(pages):
    """The navigation: docs/README.md's own sections with the pages they link, in order, then
    the rest by folder."""
    readme = (ROOT / "docs/README.md").read_text(encoding="utf-8")
    # The repository's README, then what docs/README.md introduces before its first section.
    current = ("Start here", ["README.md"] if "README.md" in pages else [])
    out, placed = [current], set(current[1])
    for line in readme.splitlines():
        heading = re.match(r"^##\s+(.*)", line)
        if heading:
            current = (heading.group(1).strip(), [])
            out.append(current)
            continue
        for href in re.findall(r"\]\(([^)#\s]+)(?:#[^)]*)?\)", line):
            target = posixpath.normpath(posixpath.join("docs", href))
            if target in pages and target not in placed:
                current[1].append(target)
                placed.add(target)
    out = [g for g in out if g[1]]
    for name, belongs in FOLDER_GROUPS:
        members = [p for p in pages if p not in placed and p != "docs/README.md" and belongs(p)]
        placed.update(members)
        if members:
            out.append((name, members))
    rest = [p for p in pages if p not in placed and p != "docs/README.md"]
    if rest:
        out.append(("More", rest))
    return out


def nav_html(nav, titles, page):
    parts = []
    for name, members in nav:
        items = []
        for p in members:
            out = output_of(p)
            here = ' aria-current="page"' if out == page else ""
            items.append(f'<li><a href="{relative(out, page)}"{here}>{html.escape(titles[p])}</a></li>')
        here = any(output_of(p) == page for p in members) or (page == HOME and name == nav[0][0])
        is_open = " open" if here else ""
        parts.append(f'<details{is_open}><summary>{html.escape(name)}</summary><ul>{"".join(items)}</ul></details>')
    return "".join(parts)


def page_html(src, page, title, body, toc, nav, titles):
    root = relative("docs", page)
    asset = lambda name: relative(f"{SITE}/assets/{name}", page)
    toc_html = ""
    if len(toc) > 1:
        rows = "".join(
            f'<li class="l{level}"><a href="#{anchor}">{html.escape(label)}</a></li>'
            for level, anchor, label in toc
        )
        toc_html = f'<nav class="toc" aria-label="On this page"><p>On this page</p><ul>{rows}</ul></nav>'
    full_title = SITE_NAME if src == "docs/README.md" else f"{title} · {SITE_NAME}"
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(full_title)}</title>
<meta name="description" content="{html.escape(title)}: Rusty Minecraft, Minecraft: Java Edition 26.3 rewritten in Rust, a fork of Horizon.">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600;700&family=Geist+Mono:wght@400;500&display=swap">
<link rel="stylesheet" href="{asset('tokens.css')}">
<link rel="stylesheet" href="{asset('site.css')}">
<script>try{{var t=localStorage.getItem("theme");if(t)document.documentElement.dataset.theme=t;}}catch(e){{}}</script>
</head>
<body data-root="{root}">
<header class="bar">
  <button class="menu" aria-label="Pages" aria-expanded="false" aria-controls="side">☰</button>
  <a class="brand" href="{relative(HOME, page)}"><span class="mark" aria-hidden="true"></span>Rusty Minecraft <span>docs</span></a>
  <div class="search"><input type="search" placeholder="Search the docs" aria-label="Search the docs" autocomplete="off"><ul class="results" hidden></ul></div>
  <button class="theme" aria-label="Switch light and dark">◐</button>
  <a class="repo" href="{REPO}">GitHub</a>
</header>
<div class="layout">
<aside id="side" class="side">{nav_html(nav, titles, page)}</aside>
<main>
<article class="doc">
{body}
<footer class="source">From <a href="{REPO}/blob/HEAD/{src}"><code>{html.escape(src)}</code></a>. Rusty Minecraft is a fork of <a href="https://github.com/horizon-rust/horizon">Horizon</a> (MIT).</footer>
</article>
</main>
{toc_html}
</div>
<script src="{asset('site.js')}"></script>
</body>
</html>
"""


CSS = """/* The documentation site's own styles (tools/build_site.py); colours come from tokens.css,
   the design system's tokens (crates/horizon-theme). */
* { box-sizing: border-box; }
html { scroll-padding-top: 72px; }
body { margin: 0; background: var(--page); color: var(--ink); font: 16px/1.6 var(--font); -webkit-font-smoothing: antialiased; }
a { color: var(--accent); text-underline-offset: 2px; }
code, pre, kbd { font-family: var(--mono); font-size: 0.9em; }
.bar { position: sticky; top: 0; z-index: 10; display: flex; align-items: center; gap: 12px; height: 56px; padding: 0 16px;
  background: var(--side); border-bottom: 1px solid var(--rule); }
.brand { display: flex; align-items: center; gap: 8px; font-weight: 600; color: var(--ink); text-decoration: none; white-space: nowrap; }
.brand span:not(.mark) { color: var(--muted); font-weight: 500; }
.mark { width: 18px; height: 18px; background: var(--face); box-shadow: inset 0 -3px 0 var(--face-lip), inset 0 3px 0 var(--face-bevel); border: 2px solid var(--outline); }
.search { position: relative; flex: 1; max-width: 420px; margin-left: auto; }
.search input { width: 100%; padding: 7px 12px; border: 1px solid var(--lip); border-radius: 6px; background: var(--well); color: var(--ink); font: inherit; font-size: 14px; }
.results { position: absolute; top: 40px; left: 0; right: 0; max-height: 60vh; overflow: auto; margin: 0; padding: 4px; list-style: none;
  background: var(--panel); border: 1px solid var(--lip); border-radius: 8px; box-shadow: 0 12px 32px rgba(0,0,0,.25); }
.results a { display: block; padding: 8px 10px; border-radius: 6px; color: var(--ink); text-decoration: none; }
.results a:hover, .results a:focus { background: var(--raised); outline: none; }
.results small { display: block; color: var(--muted); font-size: 12px; line-height: 1.4; }
.theme, .menu { border: 1px solid var(--lip); background: var(--button); color: var(--ink); border-radius: 6px; padding: 4px 10px; font: inherit; cursor: pointer; }
.menu { display: none; }
.repo { color: var(--ink-soft); font-size: 14px; text-decoration: none; }
.layout { display: grid; grid-template-columns: 280px minmax(0, 1fr) 220px; max-width: 1440px; margin: 0 auto; }
.side { position: sticky; top: 56px; height: calc(100vh - 56px); overflow: auto; padding: 16px 12px 40px; border-right: 1px solid var(--rule); font-size: 14px; }
.side details { margin-bottom: 6px; }
.side summary { cursor: pointer; padding: 4px 8px; font-weight: 600; color: var(--ink-soft); list-style: none; }
.side summary::before { content: "▸ "; color: var(--muted); }
.side details[open] summary::before { content: "▾ "; }
.side ul { list-style: none; margin: 2px 0 8px; padding: 0 0 0 10px; }
.side a { display: block; padding: 3px 8px; border-radius: 5px; color: var(--ink-soft); text-decoration: none; line-height: 1.35; }
.side a:hover { background: var(--raised); color: var(--ink); }
.side a[aria-current] { background: var(--plate); color: var(--ink); font-weight: 600; }
main { min-width: 0; padding: 24px 40px 64px; }
.doc { max-width: 860px; }
.doc h1 { font-size: 2.1rem; line-height: 1.2; margin: 0.2em 0 0.6em; letter-spacing: -0.01em; }
.doc h2 { font-size: 1.45rem; margin: 2em 0 0.6em; padding-top: 0.4em; border-top: 1px solid var(--rule); }
.doc h3 { font-size: 1.15rem; margin: 1.6em 0 0.5em; }
.doc h4 { font-size: 1rem; margin: 1.4em 0 0.4em; }
.doc p, .doc li { color: var(--ink-soft); }
.doc strong { color: var(--ink); }
.doc img { max-width: 100%; height: auto; border-radius: 6px; }
.doc :not(pre) > code { background: var(--plate-soft); padding: 0.1em 0.35em; border-radius: 4px; color: var(--ink); }
.doc pre { background: var(--well); border: 1px solid var(--rule); border-radius: 8px; padding: 14px 16px; overflow: auto; line-height: 1.45; }
.doc blockquote { margin: 1em 0; padding: 0.4em 1em; border-left: 4px solid var(--accent); background: var(--plate-soft); border-radius: 0 6px 6px 0; }
.doc hr { border: 0; border-top: 1px solid var(--rule); margin: 2em 0; }
.doc .table { overflow-x: auto; margin: 1em 0; }
.doc table { border-collapse: collapse; font-size: 14px; min-width: 60%; }
.doc th, .doc td { border: 1px solid var(--rule); padding: 6px 10px; text-align: left; vertical-align: top; }
.doc th { background: var(--plate-soft); color: var(--ink); }
.doc tr:nth-child(even) td { background: color-mix(in srgb, var(--plate-soft) 40%, transparent); }
.source { margin-top: 4em; padding-top: 1em; border-top: 1px solid var(--rule); color: var(--muted); font-size: 13px; }
.toc { position: sticky; top: 56px; height: calc(100vh - 56px); overflow: auto; padding: 24px 16px; font-size: 13px; }
.toc p { margin: 0 0 6px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: .06em; font-size: 11px; }
.toc ul { list-style: none; margin: 0; padding: 0; }
.toc a { display: block; padding: 2px 0; color: var(--ink-soft); text-decoration: none; }
.toc a:hover { color: var(--accent); }
.toc .l3 a { padding-left: 12px; }
@media (max-width: 1180px) { .layout { grid-template-columns: 260px minmax(0, 1fr); } .toc { display: none; } }
@media (max-width: 860px) {
  .layout { display: block; }
  .menu { display: inline-block; }
  .repo { display: none; }
  .side { position: fixed; top: 56px; left: 0; z-index: 9; width: min(320px, 86vw); background: var(--side); transform: translateX(-100%); transition: transform .2s; }
  body.nav-open .side { transform: none; box-shadow: 8px 0 24px rgba(0,0,0,.3); }
  main { padding: 16px 16px 48px; }
  .doc h1 { font-size: 1.7rem; }
  .brand span:not(.mark) { display: none; }
}
"""

JS = """// The documentation site's search, menu and theme switch (tools/build_site.py).
(function () {
  var root = document.body.dataset.root || ".";
  var menu = document.querySelector(".menu");
  if (menu) menu.addEventListener("click", function () {
    var open = document.body.classList.toggle("nav-open");
    menu.setAttribute("aria-expanded", open ? "true" : "false");
  });
  var theme = document.querySelector(".theme");
  if (theme) theme.addEventListener("click", function () {
    var dark = document.documentElement.dataset.theme
      ? document.documentElement.dataset.theme === "dark"
      : window.matchMedia("(prefers-color-scheme: dark)").matches;
    var next = dark ? "light" : "dark";
    document.documentElement.dataset.theme = next;
    try { localStorage.setItem("theme", next); } catch (e) {}
  });
  var input = document.querySelector(".search input");
  var list = document.querySelector(".search .results");
  if (!input || !list) return;
  var index = null;
  function load() {
    if (index) return Promise.resolve(index);
    return fetch(root + "/site/search.json").then(function (r) { return r.json(); }).then(function (d) { index = d; return d; });
  }
  function show(query) {
    var words = query.toLowerCase().split(/\\s+/).filter(Boolean);
    if (!words.length) { list.hidden = true; list.innerHTML = ""; return; }
    load().then(function (pages) {
      var hits = [];
      pages.forEach(function (p) {
        var title = p.t.toLowerCase(), text = p.x.toLowerCase(), score = 0;
        for (var i = 0; i < words.length; i++) {
          var w = words[i];
          if (title.indexOf(w) >= 0) score += 10;
          else if (p.h.toLowerCase().indexOf(w) >= 0) score += 4;
          else if (text.indexOf(w) >= 0) score += 1;
          else return;
        }
        var at = text.indexOf(words[0]), start = Math.max(0, at - 50);
        if (start > 0) { var space = p.x.indexOf(" ", start); if (space >= 0 && space < at) start = space + 1; }
        var snippet = at < 0 ? p.x.slice(0, 120) : (start > 0 ? "… " : "") + p.x.slice(start, at + 90) + "…";
        hits.push({ p: p, s: score, n: snippet });
      });
      hits.sort(function (a, b) { return b.s - a.s; });
      list.innerHTML = "";
      hits.slice(0, 12).forEach(function (h) {
        var li = document.createElement("li"), a = document.createElement("a"), small = document.createElement("small");
        a.href = root + "/" + h.p.u;
        a.textContent = h.p.t;
        small.textContent = h.n;
        a.appendChild(small);
        li.appendChild(a);
        list.appendChild(li);
      });
      if (!hits.length) {
        var li = document.createElement("li");
        li.textContent = "Nothing found";
        li.style.padding = "8px 10px";
        list.appendChild(li);
      }
      list.hidden = false;
    });
  }
  input.addEventListener("input", function () { show(input.value); });
  input.addEventListener("focus", function () { if (input.value) show(input.value); });
  input.addEventListener("keydown", function (e) {
    if (e.key === "Escape") { list.hidden = true; input.blur(); }
    if (e.key === "Enter") { var first = list.querySelector("a"); if (first) window.location.href = first.href; }
  });
  document.addEventListener("click", function (e) { if (!e.target.closest(".search")) list.hidden = true; });
  document.addEventListener("keydown", function (e) {
    if (e.key === "/" && document.activeElement !== input) { e.preventDefault(); input.focus(); }
  });
})();
"""


def build():
    """The site's files: repository path to content."""
    try:
        from markdown_it import MarkdownIt
    except ImportError:
        sys.exit("tools/build_site.py needs markdown-it-py: pip install -r tools/site-requirements.txt")
    md = MarkdownIt("commonmark", {"html": True, "typographer": False}).enable(["table", "strikethrough"])
    pages = sources()
    files = set(subprocess.check_output(["git", "ls-files"], cwd=ROOT, text=True).split("\n")) - {""}
    rendered, titles = {}, {}
    for src in pages:
        page = output_of(src)
        body, title, toc, plain = render(md, src, page, set(pages), files)
        rendered[src] = (page, body, title, toc, plain)
        titles[src] = title
    nav = groups(pages)
    out = {}
    index = []
    for src in pages:
        page, body, title, toc, plain = rendered[src]
        out[page] = page_html(src, page, title, body, toc, nav, titles)
        index.append({
            "t": title,
            "u": relative(page, "docs/x"),
            "h": " · ".join(label for _, _, label in toc),
            "x": plain[:3000],
        })
    out[f"{SITE}/search.json"] = json.dumps(index, ensure_ascii=False, separators=(",", ":")) + "\n"
    out[f"{SITE}/assets/site.css"] = CSS
    out[f"{SITE}/assets/site.js"] = JS
    out[f"{SITE}/assets/tokens.css"] = (ROOT / "crates/horizon-theme/tokens.css").read_text(encoding="utf-8")
    out["docs/.nojekyll"] = ""
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail when docs/site or docs/index.html is out of date")
    args = parser.parse_args()
    want = build()
    site = ROOT / SITE
    have = {str(p.relative_to(ROOT)).replace("\\", "/") for p in site.rglob("*") if p.is_file()} if site.exists() else set()
    if args.check:
        stale = [p for p, text in want.items() if not (ROOT / p).is_file() or (ROOT / p).read_text(encoding="utf-8") != text]
        extra = sorted(have - set(want))
        if stale or extra:
            for p in stale[:20]:
                print(f"out of date: {p}")
            for p in extra[:20]:
                print(f"not made by the generator: {p}")
            print("Run python3 tools/build_site.py")
            return 1
        print(f"The documentation site is up to date ({len(want)} files).")
        return 0
    if site.exists():
        shutil.rmtree(site)
    for p, text in want.items():
        target = ROOT / p
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
    pages = sum(1 for p in want if p.endswith(".html"))
    print(f"Wrote {pages} pages to {HOME} and {SITE}/.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
