import os

with open("crates/raf_editor/src/app.rs", "r", encoding="utf-8") as f:
    text = f.read()

text = text.replace(", lang)", ", _lang)")
text = text.replace("(lang)", "(_lang)")
text = text.replace("let border_color =", "let _border_color =")
text = text.replace("let lang = _lang;", "")

with open("crates/raf_editor/src/app.rs", "w", encoding="utf-8") as f:
    f.write(text)
