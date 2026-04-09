import os

files = [
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\panels\schematic_view.rs",
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\panels\ai_chat.rs",
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\app.rs" # Just in case it's missing too
]

for fpath in files:
    with open(fpath, "r", encoding="utf-8") as f:
        content = f.read()

    if "use raf_core::i18n::t;" not in content:
        # Find the first `use ` and insert before it
        idx = content.find("use ")
        if idx != -1:
            content = content[:idx] + "use raf_core::i18n::t;\n" + content[idx:]
        else:
            content = "use raf_core::i18n::t;\n" + content
            
        with open(fpath, "w", encoding="utf-8") as f:
            f.write(content)

