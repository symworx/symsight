# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""CLI entrypoint behavior."""

from __future__ import annotations

from pathlib import Path

import pytest

from symsight.cli import main


def test_no_subcommand_prints_help(capsys: pytest.CaptureFixture[str]) -> None:
    assert main([]) == 0
    out = capsys.readouterr().out
    assert "generate" in out
    assert "image" in out
    assert "setup" in out
    assert "tui" in out


def test_brands_lists_example_writer() -> None:
    assert main(["brands"]) == 0


def test_generate_help_mentions_tone(capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as exc:
        main(["generate", "--help"])
    assert exc.value.code == 0
    out = capsys.readouterr().out
    assert "--tone" in out
    assert "--voice" in out
    assert "--context" in out
    assert "--local" in out


def test_setup_off_tty_prints_shortcuts(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr("symsight.setup.stdin_is_tty", lambda: False)
    monkeypatch.setattr("symsight.setup.stdout_is_tty", lambda: False)
    code = main(["--config-root", str(tmp_path), "setup"])
    assert code == 0
    out = capsys.readouterr().out
    assert ".symsight.toml" in out
    assert "new:    symsight create" in out
    assert "edit:   symsight edit" in out
    assert "delete: symsight delete" in out
    assert not (tmp_path / ".symsight.toml").exists()


def test_create_writes_starter_and_does_not_replace(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    code = main(["--config-root", str(tmp_path), "create"])
    assert code == 0
    text = (tmp_path / ".symsight.toml").read_text(encoding="utf-8")
    assert 'base_url = "https://api.x.ai/v1"' in text
    assert 'active_brand = "example-writer"' in text
    assert "created:" in capsys.readouterr().out
    assert not (tmp_path / ".env").exists()

    (tmp_path / ".symsight.toml").write_text("kept = true\n", encoding="utf-8")
    code = main(["--config-root", str(tmp_path), "create"])
    assert code == 0
    assert "was not replaced" in capsys.readouterr().out
    assert (tmp_path / ".symsight.toml").read_text(encoding="utf-8") == "kept = true\n"


def test_edit_missing_and_editor(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    code = main(["--config-root", str(tmp_path), "edit"])
    assert code == 1
    assert "symsight create" in capsys.readouterr().err

    (tmp_path / ".symsight.toml").write_text("x = 1\n", encoding="utf-8")
    monkeypatch.delenv("VISUAL", raising=False)
    monkeypatch.delenv("EDITOR", raising=False)
    code = main(["--config-root", str(tmp_path), "edit"])
    assert code == 1
    assert "VISUAL or EDITOR" in capsys.readouterr().err

    script = tmp_path / "mark.sh"
    script.write_text("#!/bin/sh\necho marked >> \"$1\"\n", encoding="utf-8")
    script.chmod(0o755)
    monkeypatch.setenv("VISUAL", str(script))
    monkeypatch.setenv("EDITOR", "/bin/false")
    code = main(["--config-root", str(tmp_path), "edit"])
    assert code == 0
    assert "marked" in (tmp_path / ".symsight.toml").read_text(encoding="utf-8")
    assert "edited:" in capsys.readouterr().out


def test_delete_off_tty_requires_yes(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr("symsight.setup.stdin_is_tty", lambda: False)
    (tmp_path / ".symsight.toml").write_text("x = 1\n", encoding="utf-8")
    (tmp_path / ".env").write_text("XAI_API_KEY=unit-test-key-not-real\n", encoding="utf-8")
    code = main(["--config-root", str(tmp_path), "delete"])
    assert code == 1
    assert "pass --yes" in capsys.readouterr().err
    assert (tmp_path / ".symsight.toml").is_file()

    code = main(["--config-root", str(tmp_path), "delete", "--yes"])
    assert code == 0
    assert not (tmp_path / ".symsight.toml").exists()
    assert (tmp_path / ".env").is_file()
    captured = capsys.readouterr()
    assert "unit-test-key-not-real" not in captured.out
    assert "unit-test-key-not-real" not in captured.err

    code = main(["--config-root", str(tmp_path), "delete", "--yes"])
    assert code == 1
    assert "Nothing to delete" in capsys.readouterr().err


def test_generate_local_switches_only_a_remote_base_url(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from symsight.config import DEFAULT_LOCAL_BASE_URL, AppConfig

    fixture = Path(__file__).parent / "fixtures" / "fixture-co.yaml"
    seen: dict[str, str] = {}

    def fake(_req: object, cfg: AppConfig) -> Path:
        seen["base_url"] = cfg.base_url
        raise RuntimeError("stopped in test")

    monkeypatch.setattr("symsight.cli.generate_and_write", fake)
    (tmp_path / ".symsight.toml").write_text(
        'base_url = "https://api.x.ai/v1"\nmodel = "grok-4.5"\n',
        encoding="utf-8",
    )
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "generate",
            "--local",
            "--brand-file",
            str(fixture),
            "--type",
            "general",
            "--topic",
            "widgets",
            "--no-search",
        ]
    )
    assert code == 1
    assert seen["base_url"] == DEFAULT_LOCAL_BASE_URL

    (tmp_path / ".symsight.toml").write_text(
        'base_url = "http://127.0.0.1:9999/v1"\n',
        encoding="utf-8",
    )
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "generate",
            "--local",
            "--brand-file",
            str(fixture),
            "--type",
            "general",
            "--topic",
            "widgets",
            "--no-search",
        ]
    )
    assert code == 1
    assert seen["base_url"] == "http://127.0.0.1:9999/v1"

    code = main(
        [
            "--config-root",
            str(tmp_path),
            "generate",
            "--brand-file",
            str(fixture),
            "--type",
            "general",
            "--topic",
            "widgets",
            "--no-search",
        ]
    )
    assert code == 1
    assert seen["base_url"] == "http://127.0.0.1:9999/v1"


def test_unknown_voice_alias_is_rejected(
    capsys: pytest.CaptureFixture[str], tmp_path: Path
) -> None:
    fixture = Path(__file__).parent / "fixtures" / "fixture-co.yaml"
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "generate",
            "--brand-file",
            str(fixture),
            "--voice",
            "briefing",
            "--topic",
            "widgets",
        ]
    )
    assert code == 1
    err = capsys.readouterr().err
    assert "Unknown tone 'briefing'" in err
