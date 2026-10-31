//! Settings' Java mods section (docs/java-mod-translation.md): the selected instance's Java mod
//! jars and their translation into Horizon content mods (`horizon_port_pack::translate`). The
//! game translates new jars itself when it starts; here the player turns that off, adds jars (a
//! file picker, or jars dropped on the section), translates now to see what ports, turns each
//! mod on or off, and removes a jar (to the trash, after a second click).
use super::*;
use gpui::ExternalPaths;
use horizon_port_pack::check::Verdict;
use horizon_port_pack::mixins::Group;
use horizon_port_pack::translate::{self, Entry, State};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct JavaMods {
    /// The game folder the list was read from; None reads it again at the next draw.
    home: Option<PathBuf>,
    settings: translate::Settings,
    list: Vec<Entry>,
    /// The translated mods on in `horizon-mods.txt`.
    on: BTreeSet<String>,
    /// A translation is running.
    busy: bool,
    /// What the last one did, or why it failed.
    note: Option<Result<String, String>>,
    /// The jar whose Remove asks for a second click.
    confirm_remove: Option<String>,
    /// Jars whose mixins and libraries are listed.
    open: BTreeSet<String>,
    /// The Java the Java layer would run on (the chosen one, else the newest found) and its
    /// version, looked up with the list.
    java: Option<(PathBuf, Option<u32>)>,
}

impl JavaMods {
    /// Reads the settings and translations of `home` (small files) when they aren't the ones
    /// shown.
    fn read(&mut self, home: &Path) {
        if self.home.as_deref() == Some(home) {
            return;
        }
        self.home = Some(home.to_owned());
        self.settings = translate::Settings::load(home);
        self.java = horizon_java::locate::libjvm(&self.settings.java)
            .ok()
            .and_then(|lib| horizon_java::locate::home_of(&lib))
            .map(|home| {
                let version = horizon_java::locate::version_of(&home);
                (home, version)
            });
        self.list = translate::entries(home);
        self.on = self
            .list
            .iter()
            .filter(|e| translate::is_enabled(home, &e.mod_id))
            .map(|e| e.mod_id.clone())
            .collect();
    }

    /// The game may have translated jars since: read again at the next draw.
    pub(super) fn stale(&mut self) {
        self.home = None;
    }
}

/// The jars among dropped or picked files.
fn jars(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
        .cloned()
        .collect()
}

impl Launcher {
    /// Copies `add` into the instance's `mods/`, then translates every new or changed jar off the
    /// UI thread.
    fn translate_java_mods(&mut self, home: PathBuf, add: Vec<PathBuf>, cx: &mut Context<Self>) {
        if self.java_mods.busy {
            return;
        }
        self.java_mods.busy = true;
        self.java_mods.confirm_remove = None;
        let task = cx.background_executor().spawn(async move {
            for file in &add {
                translate::add(&home, file)?;
            }
            translate::sync(&home, &|_| {})
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let m = &mut this.java_mods;
                m.busy = false;
                m.note = Some(result.map(|out| {
                    if out.translated.is_empty() && out.removed.is_empty() {
                        tr("Everything here is translated already.").to_string()
                    } else {
                        trf(
                            "Translated: {count}. Turned on: {on}. Removed: {removed}.",
                            &[
                                ("count", &out.translated.len().to_string()),
                                ("on", &out.enabled.len().to_string()),
                                ("removed", &out.removed.len().to_string()),
                            ],
                        )
                    }
                }));
                m.stale();
                cx.notify();
            });
        })
        .detach();
    }

    fn choose_java_mods(&mut self, home: PathBuf, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(tr("Add").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| {
                    this.translate_java_mods(home, jars(&paths), cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The Java the Java layer runs on: a JDK or JRE folder, or its `java`.
    fn choose_java(&mut self, home: PathBuf, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some(tr("Choose Java…").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                let _ = this.update(cx, |this, cx| {
                    let m = &mut this.java_mods;
                    if horizon_java::locate::home_of(&path).is_some() {
                        m.settings.java = path.display().to_string();
                        if let Err(e) = m.settings.save(&home) {
                            m.note = Some(Err(e));
                        }
                    } else {
                        let shown = path.display().to_string();
                        m.note = Some(Err(trf("Not a Java folder: {path}", &[("path", &shown)])));
                    }
                    m.stale();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Moves a jar to the trash; the translation that follows uninstalls its mod.
    fn remove_java_mod(&mut self, home: PathBuf, jar: String, cx: &mut Context<Self>) {
        match horizon_launcher::desktop_seams::to_trash(&home.join(&jar)) {
            Ok(()) => self.translate_java_mods(home, Vec::new(), cx),
            Err(e) => {
                self.java_mods.note = Some(Err(format!("{jar}: {e}")));
                self.java_mods.confirm_remove = None;
            }
        }
    }

    /// The section: the switches, the actions and a row for each jar of the selected instance.
    pub(super) fn java_mods_section(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let instance = instances::selected();
        let home = instance.home.clone();
        let mut list = div().flex().flex_col();
        if instance.java.is_some() {
            list = list.child(index_row(
                1,
                tr("Translate Java mods"),
                Some(
                    div()
                        .child(trf(
                            "{instance} plays Minecraft: Java Edition, where Java mods run as they are.",
                            &[("instance", &instance.name)],
                        ))
                        .into_any_element(),
                ),
                None,
                &t,
            ));
        } else {
            self.java_mods.read(&home);
            list = list
                .child(self.java_mods_switches(&instance.name, &home, cx))
                .children(self.java_mods_rows(&home, cx));
        }
        let m = &self.java_mods;
        let note = m.note.clone().map(|n| match n {
            Ok(said) => text(theme::UI).text_color(t.ink).child(said),
            Err(e) => text(theme::UI).text_color(t.danger).child(e),
        });
        let drop_home = home.clone();
        div()
            .id("java-mods")
            .flex()
            .flex_col()
            .gap(px(16.))
            .border_2()
            .mx(px(-4.))
            .p(px(2.))
            .border_color(gpui::transparent_black())
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.border_color(t.accent))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                let found = jars(paths.paths());
                if !found.is_empty() {
                    this.translate_java_mods(drop_home.clone(), found, cx);
                    // Not the window's import as well.
                    cx.stop_propagation();
                }
                cx.notify();
            }))
            .child(text(theme::HEADING).text_color(t.ink).child(tr("Java mods")))
            .child(text(theme::UI).max_w(px(600.)).text_color(t.muted).child(tr(
                "Fabric, Quilt, NeoForge and Forge mod jars become Rusty Minecraft mods: their blocks, items, recipes, loot, worldgen and textures carry over. Their Java code doesn't run, so machines, new mobs and other behaviour are left out.",
            )))
            .child(list)
            .children(note)
    }

    /// The two switches, with the actions under the first.
    fn java_mods_switches(&self, instance: &str, home: &Path, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let m = &self.java_mods;
        let settings = m.settings.clone();
        let on = settings.translate;
        let at = home.to_owned();
        let toggle = switch("java-mods-translate", on, &t).on_click(cx.listener(
            move |this, _: &ClickEvent, _, cx| {
                let m = &mut this.java_mods;
                m.settings.translate = !m.settings.translate;
                if let Err(e) = m.settings.save(&at) {
                    m.note = Some(Err(e));
                }
                cx.notify();
            },
        ));
        let at = home.to_owned();
        let partial = switch("java-mods-partial", settings.partial, &t).on_click(cx.listener(
            move |this, _: &ClickEvent, _, cx| {
                let m = &mut this.java_mods;
                m.settings.partial = !m.settings.partial;
                let saved = m
                    .settings
                    .save(&at)
                    .and_then(|()| translate::apply_partial(&at, m.settings.partial));
                if let Err(e) = saved {
                    m.note = Some(Err(e));
                }
                m.stale();
                cx.notify();
            },
        ));
        let mut actions = div().flex().flex_wrap().gap(px(8.)).mt(px(6.));
        if m.busy {
            actions = actions.child(caps(tr("Translating…")).text_color(t.accent));
        } else {
            let (add, now, folder) = (home.to_owned(), home.to_owned(), home.join("mods"));
            actions = actions
                .child(
                    button("java-mods-add", "Add mod jar…", Kind::Primary, &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.choose_java_mods(add.clone(), cx);
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button("java-mods-now", "Translate now", Kind::Secondary, &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.translate_java_mods(now.clone(), Vec::new(), cx);
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    button("java-mods-folder", "Open folder", Kind::Ghost, &t).on_click(
                        move |_, _, cx| {
                            let _ = std::fs::create_dir_all(&folder);
                            cx.open_with_system(&folder);
                        },
                    ),
                );
        }
        let when = if on {
            "When {instance} starts, the jars in its mods folder are translated. Put jars there, add them here, or drop them on this section."
        } else {
            "Off: the game leaves the jars in {instance}'s mods folder alone. Translate now still translates them."
        };
        div()
            .flex()
            .flex_col()
            .child(index_row(
                1,
                tr("Translate Java mods"),
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(trf(when, &[("instance", instance)]))
                        .child(actions)
                        .into_any_element(),
                ),
                Some(toggle.into_any_element()),
                &t,
            ))
            .child(index_row(
                2,
                tr("Turn on mods that only partly translate"),
                Some(
                    div()
                        .child(tr("Their content plays; what their code does is missing."))
                        .into_any_element(),
                ),
                Some(partial.into_any_element()),
                &t,
            ))
            .child(self.java_mods_code(home, cx))
    }

    /// The Java layer's switch (off by default) and the Java it runs on.
    fn java_mods_code(&self, home: &Path, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let m = &self.java_mods;
        let at = home.to_owned();
        let code = switch("java-mods-code", m.settings.code, &t).on_click(cx.listener(
            move |this, _: &ClickEvent, _, cx| {
                let m = &mut this.java_mods;
                m.settings.code = !m.settings.code;
                if let Err(e) = m.settings.save(&at) {
                    m.note = Some(Err(e));
                }
                cx.notify();
            },
        ));
        let java = match &m.java {
            Some((path, version)) => trf(
                "Java: {path} (version {version})",
                &[
                    ("path", &path.display().to_string()),
                    ("version", &version.map_or("?".into(), |v| v.to_string())),
                ],
            ),
            None => tr("No Java found: install Java 25, or choose one.").to_string(),
        };
        let pick = home.to_owned();
        let mut buttons = div().flex().flex_wrap().gap(px(8.)).mt(px(6.)).child(
            button("java-mods-java", "Choose Java…", Kind::Secondary, &t).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.choose_java(pick.clone(), cx);
                    cx.notify();
                },
            )),
        );
        if !m.settings.java.is_empty() {
            let at = home.to_owned();
            buttons = buttons.child(
                button(
                    "java-mods-java-auto",
                    "Find Java automatically",
                    Kind::Ghost,
                    &t,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let m = &mut this.java_mods;
                    m.settings.java.clear();
                    if let Err(e) = m.settings.save(&at) {
                        m.note = Some(Err(e));
                    }
                    m.stale();
                    cx.notify();
                })),
            );
        }
        index_row(
            3,
            tr("Run their code with Java (experimental)"),
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(tr("Mods made for 26.x run their entry points and supported mixins on your own Java, with a part of vanilla's and Fabric API's API. Only games built with the Java layer do this; official builds don't yet."))
                    .child(
                        div()
                            .text_color(t.warn)
                            .child(tr("Java mods' code runs with full access to this computer, like native mods: turn this on only for mods you trust.")),
                    )
                    .child(div().font_family(theme::MONO).child(java))
                    .child(buttons)
                    .into_any_element(),
            ),
            Some(code.into_any_element()),
            &t,
        )
    }

    /// A row for each jar: its verdict, what ports and what doesn't, its switch and Remove.
    fn java_mods_rows(&self, home: &Path, cx: &mut Context<Self>) -> Vec<Div> {
        let t = self.theme;
        let m = &self.java_mods;
        if m.list.is_empty() {
            return vec![index_row(
                4,
                tr("No Java mods yet"),
                Some(
                    div()
                        .child(tr("Add a jar, or drop one on this section."))
                        .into_any_element(),
                ),
                None,
                &t,
            )];
        }
        m.list
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (label, color) = match (e.state, e.verdict) {
                    (State::Failed, _) | (_, None) => ("Failed", t.danger),
                    (_, Some(Verdict::No)) => ("Doesn't run", t.muted),
                    (State::NotInstalled, _) => ("Not installed", t.danger),
                    (State::Installed, Some(Verdict::Works)) => ("Works", t.success),
                    (State::Installed, Some(Verdict::Partly)) => ("Partly", t.warn),
                };
                let mut title = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(format!("{} {}", e.title(), e.version).trim().to_string())
                    .child(blocks::badge(label, color, &t));
                if e.state != State::Failed {
                    title = title.child(blocks::badge(e.loader_name().to_string(), t.muted, &t));
                }
                let mut detail = div().flex().flex_col().gap(px(2.)).child(e.summary.clone());
                if !e.skipped.is_empty() && e.state == State::Installed {
                    let shown: Vec<&str> = e.skipped.iter().take(3).map(String::as_str).collect();
                    let mut missing = shown.join("; ");
                    if e.skipped.len() > shown.len() {
                        missing.push_str("; …");
                    }
                    detail = detail.child(trf("Missing: {what}", &[("what", &missing)]));
                }
                detail = detail.children(e.notes.iter().map(|n| div().child(n.clone())));
                if !e.code.is_empty() {
                    detail = detail.child(trf("Code: {what}", &[("what", &e.code)]));
                }
                if !e.mixins.is_empty() {
                    let open = m.open.contains(&e.jar);
                    let jar = e.jar.clone();
                    detail = detail.child(
                        div().flex().child(
                            button(
                                SharedString::from(format!("java-mod-mixins-{i}")),
                                if open {
                                    tr("Hide the details")
                                } else {
                                    tr("Show each mixin and library")
                                },
                                Kind::Ghost,
                                &t,
                            )
                            .on_click(cx.listener(
                                move |this, _: &ClickEvent, _, cx| {
                                    let open = &mut this.java_mods.open;
                                    if !open.remove(&jar) {
                                        open.insert(jar.clone());
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                    );
                    if open {
                        detail = detail.children(e.mixins.iter().map(|l| {
                            let color = match l.group {
                                Group::Supported => t.success,
                                Group::NeedsJava => t.warn,
                                Group::Unsupported => t.danger,
                            };
                            div()
                                .flex()
                                .items_start()
                                .gap(px(6.))
                                .child(div().flex_none().mt(px(6.)).size(px(6.)).bg(color))
                                .child(div().flex_1().child(l.text.clone()))
                        }));
                    }
                }
                detail = detail.child(div().font_family(theme::MONO).child(e.jar.clone()));
                let mut right = div().flex().items_center().gap(px(8.));
                if e.state == State::Installed {
                    let on = m.on.contains(&e.mod_id);
                    let (id, at) = (e.mod_id.clone(), home.to_owned());
                    right = right.child(
                        switch(SharedString::from(format!("java-mod-on-{i}")), on, &t).on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if let Err(e) = translate::set_enabled(&at, &id, !on) {
                                    this.java_mods.note = Some(Err(e));
                                }
                                this.java_mods.stale();
                                cx.notify();
                            }),
                        ),
                    );
                }
                if !m.busy {
                    let asking = m.confirm_remove.as_deref() == Some(e.jar.as_str());
                    let (jar, at) = (e.jar.clone(), home.to_owned());
                    right = right.child(
                        button(
                            SharedString::from(format!("java-mod-remove-{i}")),
                            if asking { "Move to trash" } else { "Remove" },
                            if asking { Kind::Primary } else { Kind::Ghost },
                            &t,
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                if this.java_mods.confirm_remove.as_deref() == Some(jar.as_str()) {
                                    this.remove_java_mod(at.clone(), jar.clone(), cx);
                                } else {
                                    this.java_mods.confirm_remove = Some(jar.clone());
                                }
                                cx.notify();
                            },
                        )),
                    );
                }
                index_row(
                    i + 4,
                    title,
                    Some(detail.into_any_element()),
                    Some(right.into_any_element()),
                    &t,
                )
            })
            .collect()
    }
}
