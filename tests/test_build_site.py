"""tools/build_site.py: the documentation website made from the repository's Markdown."""

import importlib.util
import unittest

from tools import build_site as site

HAS_MARKDOWN_IT = importlib.util.find_spec("markdown_it") is not None


class BuildSiteTests(unittest.TestCase):
    def test_pages_are_written_beside_the_docs(self):
        self.assertEqual(site.output_of("docs/README.md"), "docs/index.html")
        self.assertEqual(site.output_of("docs/mods.md"), "docs/site/mods.html")
        self.assertEqual(site.output_of("docs/api/launch.md"), "docs/site/api/launch.html")
        self.assertEqual(site.output_of("README.md"), "docs/site/repo/README.html")
        self.assertEqual(site.output_of("crates/horizon-java/java/README.md"),
                         "docs/site/repo/crates/horizon-java/java/README.html")

    def test_anchors_are_githubs(self):
        seen = {}
        self.assertEqual(site.slug("Phase 4: the Java API layer and Fabric API", seen),
                         "phase-4-the-java-api-layer-and-fabric-api")
        self.assertEqual(site.slug("Measurements", seen), "measurements")
        self.assertEqual(site.slug("Measurements", seen), "measurements-1")
        self.assertEqual(site.slug("`horizon-ctl` and MCP", seen), "horizon-ctl-and-mcp")

    def test_links_go_to_pages_images_or_the_repository(self):
        pages = {"docs/mods.md", "docs/java-mod-runtime.md", "README.md"}
        files = {"docs/mods.md", "docs/java-mod-runtime.md", "README.md", "docs/media/a.webp",
                 "crates/horizon-java/src/jvm/mod.rs"}
        page = site.output_of("docs/mods.md")

        def resolve(href, src="docs/mods.md"):
            return site.resolve(href, src, page, pages, files)

        self.assertEqual(resolve("java-mod-runtime.md#limits"), "java-mod-runtime.html#limits")
        self.assertEqual(resolve("../README.md"), "repo/README.html")
        self.assertEqual(resolve("media/a.webp"), "../media/a.webp")
        self.assertEqual(resolve("../crates/horizon-java/src/jvm/mod.rs"),
                         site.REPO + "/blob/HEAD/crates/horizon-java/src/jvm/mod.rs")
        self.assertEqual(resolve("../crates/horizon-java"), site.REPO + "/tree/HEAD/crates/horizon-java")
        for kept in ["https://example.com/x.md", "#top", "mailto:a@b.c", "../../outside.md"]:
            self.assertEqual(resolve(kept), kept)
        self.assertEqual(site.image("media/a.webp", "docs/mods.md", page, files), "../media/a.webp")

    @unittest.skipUnless(HAS_MARKDOWN_IT, "markdown-it-py is not installed (tools/site-requirements.txt)")
    def test_the_committed_site_is_up_to_date(self):
        want = site.build()
        stale = [p for p, text in want.items()
                 if not (site.ROOT / p).is_file() or (site.ROOT / p).read_text(encoding="utf-8") != text]
        self.assertEqual(stale, [], "run python3 tools/build_site.py")


if __name__ == "__main__":
    unittest.main()
