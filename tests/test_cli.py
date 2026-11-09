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


def test_setup_local_writes_toml(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "setup",
            "--local",
            "--base-url",
            "http://127.0.0.1:11434/v1",
            "--model",
            "llama3.2",
        ]
    )
    assert code == 0
    text = (tmp_path / ".symsight.toml").read_text(encoding="utf-8")
    assert 'base_url = "http://127.0.0.1:11434/v1"' in text
    assert 'model = "llama3.2"' in text
    assert not (tmp_path / ".env").exists()
    out = capsys.readouterr().out
    assert "llama3.2" in out
    assert "Web search is unavailable" in out


def test_setup_remote_writes_env_and_hides_the_key(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    secret = "unit-test-key-not-real"
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "setup",
            "--remote",
            "--api-key",
            secret,
        ]
    )
    assert code == 0
    env = (tmp_path / ".env").read_text(encoding="utf-8")
    assert f"XAI_API_KEY={secret}" in env
    captured = capsys.readouterr()
    assert secret not in captured.out
    assert secret not in captured.err
    toml = (tmp_path / ".symsight.toml").read_text(encoding="utf-8")
    assert "https://api.x.ai/v1" in toml
    assert secret not in toml


def test_setup_rejects_both_modes(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    code = main(["--config-root", str(tmp_path), "setup", "--local", "--remote"])
    assert code == 1
    assert "only one" in capsys.readouterr().err
    assert not (tmp_path / ".symsight.toml").exists()


def test_setup_local_requires_a_model_off_tty(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr("symsight.cli._stdin_is_tty", lambda: False)
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "setup",
            "--local",
            "--base-url",
            "http://127.0.0.1:11434/v1",
        ]
    )
    assert code == 1
    assert "--model" in capsys.readouterr().err
    assert not (tmp_path / ".symsight.toml").exists()


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
