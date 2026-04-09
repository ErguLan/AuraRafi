import os

with open("crates/raf_editor/src/app.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("let lang = self.settings.language;", "let _lang = self.settings.language;")
with open("crates/raf_editor/src/app.rs", "w", encoding="utf-8") as f:
    f.write(text)

with open("crates/raf_editor/src/panels/ai_chat.rs", "r", encoding="utf-8") as f:
    text = f.read().replace("self.sending = true;", "// self.sending = true;")
with open("crates/raf_editor/src/panels/ai_chat.rs", "w", encoding="utf-8") as f:
    f.write(text)
