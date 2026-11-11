"""Reject direct blocking calls in retained presentation modules.

This is a deliberately narrow source guard, not a whole-program purity proof.
Worker modules, explicit persistence boundaries and test modules are separate.
Run from any directory; it never builds, writes files, or opens a window.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
RENDER = ROOT / "crates/raf_render/src/ApiGraphicBasic/ui_surface"
PATTERNS = (
    re.compile(r"\b(?:std::)?fs::(?:read\w*|write|create\w*|rename|remove\w*|metadata|canonicalize)\s*\("),
    re.compile(r"\b(?:std::)?thread::sleep\s*\("),
    re.compile(r"\.sync_(?:all|data)\s*\("),
    re.compile(r"\b(?:project_store::)?write_atomic\s*\("),
    re.compile(r"\b(?:std::)?process::Command\b"),
)


def mask_non_code(source):
    # Preserve offsets and newlines for actionable diagnostics. This scanner
    # intentionally does not resolve imports, aliases, macros or callees.
    token = re.compile(r'//[^\n]*|/\*.*?\*/|r(?P<hash>\#+)".*?"(?P=hash)|"(?:\\.|[^"\\])*"', re.S)
    return token.sub(lambda match: re.sub(r"[^\n]", " ", match.group()), source)


def production_code(source):
    masked = mask_non_code(source)
    test_attribute = re.compile(r"#\[cfg\(test\)\]\s*(?:(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*)?\{")
    chars = list(masked)
    for match in test_attribute.finditer(masked):
        depth = 1
        end = match.end()
        while end < len(masked) and depth:
            depth += (masked[end] == "{") - (masked[end] == "}")
            end += 1
        for index in range(match.start(), end):
            if chars[index] != "\n":
                chars[index] = " "
    return "".join(chars)


def inspect():
    paths = set((ROOT / "crates/raf_editor/src").rglob("*_surface.rs"))
    paths.add(ROOT / "crates/raf_editor/src/native_workbench_surface.rs")
    paths.update(RENDER / name for name in ("render.rs", "presentation.rs", "regional_layout.rs", "regional_paint.rs"))
    failures = []
    for path in sorted(paths):
        source = production_code(path.read_text(encoding="utf-8"))
        for pattern in PATTERNS:
            for match in pattern.finditer(source):
                line = source.count("\n", 0, match.start()) + 1
                failures.append(f"{path.relative_to(ROOT)}:{line}: blocking work in retained presentation: {match.group()}")
    return paths, failures


if __name__ == "__main__":
    paths, failures = inspect()
    if failures:
        print("\n".join(failures), file=sys.stderr)
        raise SystemExit(1)
    print(f"RafUI direct-blocking guard: {len(paths)} presentation modules checked.")
