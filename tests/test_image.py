# Copyright (c) 2026, Nathaniel T. Berry
# Licensed under the Apache License, Version 2.0.
"""Image command reads style from the brand file and does not call the API."""

from __future__ import annotations

from pathlib import Path

from symsight.cli import main

FIXTURE = Path(__file__).parent / "fixtures" / "fixture-co.yaml"

BRAND = """\
id: lab
display_name: Lab
full_name: Lab
voice: plain
image:
  style: |
    Spare ink lines.
  aspect: "1:1"
  quality: medium
types:
  note:
    user_template: "{topic}"
"""


def test_image_refuses_a_brand_with_no_style(tmp_path: Path, capsys) -> None:
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "image",
            "--brand-file",
            str(FIXTURE),
            "--prompt",
            "a panel",
            "--out",
            str(tmp_path / "panel.png"),
        ]
    )
    assert code == 1
    err = capsys.readouterr().err
    assert "image.style" in err
    assert not (tmp_path / "panel.png").exists()


def test_image_sends_the_brand_style_and_leaves_aspect_to_the_brand(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    brand_path = tmp_path / "lab.yaml"
    brand_path.write_text(BRAND, encoding="utf-8")
    seen: dict[str, object] = {}

    def fake(brand, scene, dest, *, aspect, quality, model, api_key, base_url):
        seen["style"] = brand.image.style
        seen["aspect_on_brand"] = brand.image.aspect
        seen["scene"] = scene
        seen["aspect"] = aspect
        seen["quality"] = quality
        seen["api_key"] = api_key
        dest.write_bytes(b"png")
        return dest

    monkeypatch.setattr("symsight.cli.render_image", fake)
    monkeypatch.setenv("XAI_API_KEY", "test-key")
    dest = tmp_path / "panel.png"
    code = main(
        [
            "--config-root",
            str(tmp_path),
            "image",
            "--brand-file",
            str(brand_path),
            "--prompt",
            "two figures",
            "--out",
            str(dest),
        ]
    )
    assert code == 0
    assert "Spare ink lines." in str(seen["style"])
    assert seen["aspect_on_brand"] == "1:1"
    assert seen["scene"] == "two figures"
    assert seen["aspect"] is None
    assert seen["quality"] is None
    assert seen["api_key"] == "test-key"
    assert dest.read_bytes() == b"png"
    assert "Image written:" in capsys.readouterr().out
