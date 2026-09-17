# Copyright (c) 2026, Nathaniel Berry
# Licensed under the Apache License, Version 2.0.

"""Textual application: list drafts, edit, generate, finalize, settings."""

from __future__ import annotations

from pathlib import Path
from typing import ClassVar

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import (
    Button,
    Footer,
    Header,
    Input,
    Label,
    ListItem,
    ListView,
    Select,
    Static,
    TextArea,
)

from symsight.brand import BrandError, list_brands, resolve_brand
from symsight.config import AppConfig, save_project_config
from symsight.draft_io import list_drafts, read_draft, write_draft_content
from symsight.finalize import FinalizeError, finalize_draft
from symsight.generate import GenerateError, generate_and_write
from symsight.models import Brand, ContentFormat, GenerateRequest
from symsight.textutil import char_count, word_count


class GenerateScreen(ModalScreen[bool]):
    """Modal form to generate a new draft."""

    CSS = """
    GenerateScreen {
        align: center middle;
    }
    #gen-dialog {
        width: 80;
        height: auto;
        max-height: 90%;
        border: thick $accent;
        background: $surface;
        padding: 1 2;
    }
    #gen-dialog Input, #gen-dialog Select {
        margin-bottom: 1;
    }
    #gen-actions {
        height: auto;
        align: right middle;
    }
    """

    def __init__(self, cfg: AppConfig, brand: Brand) -> None:
        super().__init__()
        self.cfg = cfg
        self.brand = brand

    def compose(self) -> ComposeResult:
        type_opts = [(t, t) for t in self.brand.types] or [("general", "general")]
        with Vertical(id="gen-dialog"):
            yield Label(f"Generate — {self.brand.display_name}")
            yield Label("Type")
            yield Select(type_opts, id="gen-type", value=type_opts[0][1], allow_blank=False)
            yield Label("Format")
            yield Select(
                [("article", "article"), ("social", "social")],
                id="gen-format",
                value="article",
                allow_blank=False,
            )
            yield Label("Topic (optional)")
            yield Input(placeholder="Focus topic…", id="gen-topic")
            yield Label("Min words (article)")
            yield Input(
                value=str(self.brand.formats.article.default_min_words),
                id="gen-min-words",
            )
            yield Label("Max words (article)")
            yield Input(
                value=str(self.brand.formats.article.default_max_words),
                id="gen-max-words",
            )
            yield Label("Max chars (social)")
            yield Input(
                value=str(self.brand.formats.social.default_max_chars),
                id="gen-max-chars",
            )
            yield Label("Web search: auto | on | off")
            yield Select(
                [("auto", "auto"), ("on", "on"), ("off", "off")],
                id="gen-search",
                value="auto",
                allow_blank=False,
            )
            yield Static("", id="gen-status")
            with Horizontal(id="gen-actions"):
                yield Button("Cancel", id="gen-cancel")
                yield Button("Generate", variant="primary", id="gen-go")

    @on(Button.Pressed, "#gen-cancel")
    def cancel(self) -> None:
        self.dismiss(False)

    @on(Button.Pressed, "#gen-go")
    def go(self) -> None:
        self._run_generate()

    @work(thread=True)
    def _run_generate(self) -> None:
        def status(msg: str) -> None:
            self.app.call_from_thread(self.query_one("#gen-status", Static).update, msg)

        status("Generating…")
        try:
            type_id = str(self.query_one("#gen-type", Select).value)
            fmt = ContentFormat(str(self.query_one("#gen-format", Select).value))
            topic = self.query_one("#gen-topic", Input).value.strip() or None
            min_w = int(self.query_one("#gen-min-words", Input).value or "0") or None
            max_w = int(self.query_one("#gen-max-words", Input).value or "0") or None
            max_c = int(self.query_one("#gen-max-chars", Input).value or "0") or None
            search_mode = str(self.query_one("#gen-search", Select).value)
            use_search: bool | None
            if search_mode == "on":
                use_search = True
            elif search_mode == "off":
                use_search = False
            else:
                use_search = None

            req = GenerateRequest(
                brand=self.brand,
                type_id=type_id,
                format=fmt,
                topic=topic,
                min_words=min_w,
                max_words=max_w,
                max_chars=max_c,
                use_search=use_search,
            )
            path = generate_and_write(req, self.cfg)
            status(f"Wrote {path.name}")
            self.app.call_from_thread(self.dismiss, True)
        except (GenerateError, RuntimeError, ValueError, BrandError) as exc:
            status(f"Failed: {exc}")
        except Exception as exc:  # noqa: BLE001
            status(f"Error: {exc}")


class SettingsScreen(ModalScreen[bool]):
    """Edit drafts/final dirs and active brand."""

    CSS = """
    SettingsScreen { align: center middle; }
    #set-dialog {
        width: 80;
        height: auto;
        border: thick $accent;
        background: $surface;
        padding: 1 2;
    }
    #set-dialog Input { margin-bottom: 1; }
    #set-actions { height: auto; align: right middle; }
    """

    def __init__(self, cfg: AppConfig) -> None:
        super().__init__()
        self.cfg = cfg

    def compose(self) -> ComposeResult:
        brands = list_brands(self.cfg.brands_dir)
        opts = [(b.id, b.id) for b in brands] or [
            (self.cfg.active_brand, self.cfg.active_brand)
        ]
        with Vertical(id="set-dialog"):
            yield Label("Settings")
            yield Label("Drafts directory")
            yield Input(value=str(self.cfg.drafts_dir), id="set-drafts")
            yield Label("Final directory")
            yield Input(value=str(self.cfg.final_dir), id="set-final")
            yield Label("Active brand")
            yield Select(
                opts,
                id="set-brand",
                value=self.cfg.active_brand
                if self.cfg.active_brand in {o[1] for o in opts}
                else opts[0][1],
                allow_blank=False,
            )
            yield Label("Brands directory")
            yield Input(value=str(self.cfg.brands_dir), id="set-brands-dir")
            with Horizontal(id="set-actions"):
                yield Button("Cancel", id="set-cancel")
                yield Button("Save", variant="primary", id="set-save")

    @on(Button.Pressed, "#set-cancel")
    def cancel(self) -> None:
        self.dismiss(False)

    @on(Button.Pressed, "#set-save")
    def save(self) -> None:
        drafts = Path(self.query_one("#set-drafts", Input).value).expanduser()
        final = Path(self.query_one("#set-final", Input).value).expanduser()
        brands_dir = Path(self.query_one("#set-brands-dir", Input).value).expanduser()
        brand = str(self.query_one("#set-brand", Select).value)
        self.cfg = self.cfg.model_copy(
            update={
                "drafts_dir": drafts if drafts.is_absolute() else (self.cfg.project_root / drafts).resolve(),
                "final_dir": final if final.is_absolute() else (self.cfg.project_root / final).resolve(),
                "brands_dir": brands_dir
                if brands_dir.is_absolute()
                else (self.cfg.project_root / brands_dir).resolve(),
                "active_brand": brand,
            }
        )
        save_project_config(self.cfg)
        # stash on app
        app = self.app
        if isinstance(app, SymsightApp):
            app.cfg = self.cfg
        self.dismiss(True)


class SymsightApp(App[None]):
    """Main TUI."""

    TITLE = "symsight"
    CSS = """
    #sidebar {
        width: 36;
        border: solid $primary;
    }
    #main {
        width: 1fr;
    }
    #meta {
        height: 3;
        dock: top;
        padding: 0 1;
        color: $text-muted;
    }
    #editor {
        height: 1fr;
    }
    #status {
        height: 1;
        dock: bottom;
        padding: 0 1;
        background: $panel;
    }
    ListView {
        height: 1fr;
    }
    """

    BINDINGS: ClassVar[list[Binding | tuple[str, str, str]]] = [
        Binding("q", "quit", "Quit"),
        Binding("r", "refresh", "Refresh"),
        Binding("g", "generate", "Generate"),
        Binding("s", "save", "Save"),
        Binding("f", "finalize", "Finalize"),
        Binding("comma", "settings", "Settings"),
        Binding("question_mark", "help", "Help"),
    ]

    def __init__(self, cfg: AppConfig) -> None:
        super().__init__()
        self.cfg = cfg
        self.current_path: Path | None = None
        self.brand: Brand | None = None

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal():
            with Vertical(id="sidebar"):
                yield Label("Drafts")
                yield ListView(id="draft-list")
            with Vertical(id="main"):
                yield Static("No draft selected", id="meta")
                yield TextArea(id="editor", language="markdown")
                yield Static("", id="status")
        yield Footer()

    def on_mount(self) -> None:
        self._load_brand()
        self.action_refresh()
        self.query_one("#status", Static).update(
            f"drafts={self.cfg.drafts_dir}  final={self.cfg.final_dir}  brand={self.cfg.active_brand}"
        )

    def _load_brand(self) -> None:
        try:
            self.brand = resolve_brand(
                brands_dir=self.cfg.brands_dir,
                brand_id=self.cfg.active_brand,
            )
        except BrandError as exc:
            self.brand = None
            self.notify(str(exc), severity="error")

    def action_refresh(self) -> None:
        lv = self.query_one("#draft-list", ListView)
        lv.clear()
        self.cfg.drafts_dir.mkdir(parents=True, exist_ok=True)
        drafts = list_drafts(self.cfg.drafts_dir)
        for d in drafts:
            path = d.path
            assert path is not None
            label = f"{path.name}"
            item = ListItem(Label(label), name=str(path))
            lv.append(item)
        self.query_one("#status", Static).update(
            f"{len(drafts)} draft(s)  |  brand={self.cfg.active_brand}"
        )

    @on(ListView.Selected, "#draft-list")
    def open_selected(self, event: ListView.Selected) -> None:
        item = event.item
        if item is None or not item.name:
            return
        path = Path(item.name)
        self._open_draft(path)

    def _open_draft(self, path: Path) -> None:
        try:
            draft = read_draft(path)
        except OSError as exc:
            self.notify(f"Cannot open: {exc}", severity="error")
            return
        self.current_path = path
        fm = draft.front_matter
        fmt = fm.get("format", "article")
        counts = (
            f"chars={fm.get('char_count', char_count(draft.body))}"
            if fmt == "social"
            else f"words={fm.get('word_count', word_count(draft.body))}"
        )
        self.query_one("#meta", Static).update(
            f"{draft.title}  |  type={fm.get('type', '?')}  format={fmt}  "
            f"status={fm.get('status', '?')}  {counts}"
        )
        editor = self.query_one("#editor", TextArea)
        editor.load_text(draft.body)

    def action_save(self) -> None:
        if not self.current_path:
            self.notify("No draft open", severity="warning")
            return
        body = self.query_one("#editor", TextArea).text
        draft = read_draft(self.current_path)
        fm = dict(draft.front_matter)
        fmt = str(fm.get("format", "article"))
        if fmt == "social":
            fm["char_count"] = char_count(body)
        else:
            fm["word_count"] = word_count(body)
        disclaimer = None
        if fm.get("disclaimer") and self.brand and self.brand.disclaimer:
            disclaimer = self.brand.disclaimer
        write_draft_content(
            self.current_path,
            front=fm,
            body=body,
            disclaimer=disclaimer,
        )
        self.notify(f"Saved {self.current_path.name}")
        self._open_draft(self.current_path)

    def action_finalize(self) -> None:
        if not self.current_path:
            self.notify("No draft open", severity="warning")
            return
        # save first
        self.action_save()
        try:
            dest = finalize_draft(
                self.current_path,
                final_dir=self.cfg.final_dir,
                brand=self.brand,
                copy=False,
            )
        except FinalizeError as exc:
            self.notify(str(exc), severity="error")
            return
        self.notify(f"Finalized → {dest}")
        self.current_path = None
        self.query_one("#editor", TextArea).load_text("")
        self.query_one("#meta", Static).update("No draft selected")
        self.action_refresh()

    def action_generate(self) -> None:
        if not self.brand:
            self._load_brand()
        if not self.brand:
            self.notify("No brand loaded — open Settings", severity="error")
            return

        def done(ok: bool | None) -> None:
            if ok:
                self.action_refresh()
                # open newest
                drafts = list_drafts(self.cfg.drafts_dir)
                if drafts and drafts[0].path:
                    self._open_draft(Path(drafts[0].path))

        self.push_screen(GenerateScreen(self.cfg, self.brand), done)

    def action_settings(self) -> None:
        def done(ok: bool | None) -> None:
            if ok:
                self._load_brand()
                self.action_refresh()
                self.query_one("#status", Static).update(
                    f"drafts={self.cfg.drafts_dir}  final={self.cfg.final_dir}  "
                    f"brand={self.cfg.active_brand}"
                )

        self.push_screen(SettingsScreen(self.cfg), done)

    def action_help(self) -> None:
        self.notify(
            "Keys: g generate · s save · f finalize · r refresh · , settings · q quit",
            timeout=6,
        )


def run_tui(cfg: AppConfig) -> None:
    SymsightApp(cfg).run()
