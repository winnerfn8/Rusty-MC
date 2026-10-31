// The documentation site's search, menu and theme switch (tools/build_site.py).
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
    var words = query.toLowerCase().split(/\s+/).filter(Boolean);
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
