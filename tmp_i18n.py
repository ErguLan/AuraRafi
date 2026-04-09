import re
import json
import os

files_to_check = [
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\app.rs",
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\panels\schematic_view.rs",
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\panels\ai_chat.rs"
]

en_path = r"d:\Proyectos\ProyectRaf\crates\raf_core\locales\en.json"
es_path = r"d:\Proyectos\ProyectRaf\crates\raf_core\locales\es.json"

with open(en_path, "r", encoding="utf-8") as f:
    en_dict = json.load(f)
with open(es_path, "r", encoding="utf-8") as f:
    es_dict = json.load(f)

for fpath in files_to_check:
    with open(fpath, "r", encoding="utf-8") as f:
        content = f.read()
    
    matches = re.finditer(r'if (?:is_es|self\.settings\.language == Language::Spanish)\s*\{\s*"([^"]+)"\s*\}\s*else\s*\{\s*"([^"]+)"\s*\}', content)
    
    for m in matches:
        es_val = m.group(1)
        en_val = m.group(2)
        key = "app." + re.sub(r'[^a-z0-9]+', '_', en_val.lower()).strip('_')
        
        en_dict[key] = en_val
        es_dict[key] = es_val
        
        # Replace:
        if "app.rs" in fpath:
            content = content.replace(m.group(0), f't("{key}", self.settings.language)')
        else:
            # schematic & ai_chat have self.is_es, replace with self.lang
            content = content.replace(m.group(0), f't("{key}", self.lang)')

    # We also have to add `use raf_core::i18n::t;` to the files.
    if "use raf_core::i18n::t;" not in content:
        content = content.replace("use raf_core::config::", "use raf_core::i18n::t;\nuse raf_core::config::")

    # If it's a panel, change `is_es: bool` to `lang: Language`
    if "is_es: bool" in content:
        content = content.replace("pub is_es: bool", "pub lang: Language")
        content = content.replace("is_es: false,", "lang: Language::English,")
    
    # And update "let is_es = self.is_es;" or similar things that we don't need or rewrite them.
    # Actually, it's safer to just do `let lang = self.lang;`
    content = content.replace("let is_es = self.is_es;", "let lang = self.lang;")
    content = content.replace("let _is_es = self.is_es;", "let _lang = self.lang;")
    content = content.replace("let _is_es = self.settings.language == Language::Spanish;", "let _lang = self.settings.language;")
    content = content.replace("let is_es = self.settings.language == Language::Spanish;", "let lang = self.settings.language;")
    
    content = content.replace(".is_es = self.settings.language == Language::Spanish;", ".lang = self.settings.language;")
    
    with open(fpath, "w", encoding="utf-8") as f:
        f.write(content)

with open(en_path, "w", encoding="utf-8") as f:
    json.dump(en_dict, f, indent=2)

with open(es_path, "w", encoding="utf-8") as f:
    json.dump(es_dict, f, indent=2)
