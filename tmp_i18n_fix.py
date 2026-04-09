import re
import os

files = [
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\app.rs",
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\panels\schematic_view.rs",
    r"d:\Proyectos\ProyectRaf\crates\raf_editor\src\panels\ai_chat.rs"
]

for fpath in files:
    with open(fpath, "r", encoding="utf-8") as f:
        content = f.read()

    # 1. Missing imports
    if "use raf_core::i18n::t;" not in content:
        # replace use raf_core::config::{...} with use raf_core::i18n::t;
        content = re.sub(r'use raf_core::config::', 'use raf_core::i18n::t;\nuse raf_core::config::', content, count=1)

    # 2. Fix mismatched types in `app.rs`: console.log(LogLevel::Info, msg) should be &msg if msg is String
    # Python script earlier turned `if is_es { "..." } else { "..." }` (which returned &str)
    # into `t(...)` which returns String!
    # So we need to change: self.console.log(..., msg); to self.console.log(..., &msg);
    # But ONLY in app.rs
    content = re.sub(r'self\.console\.log\(([^,]+),\s*([a-zA-Z0-9_]+)\);', r'self.console.log(\1, &\2);', content)
    # Wait, msg is directly `t(...)` ? No, `let msg = t(...);` then `self.console.log(..., msg);`
    # Also `self.console.log(..., t(...));` -> `self.console.log(..., &t(...));`
    content = re.sub(r'self\.console\.log\(([^,]+),\s*t\(([^)]+)\)\);', r'self.console.log(\1, &t(\2));', content)

    # 3. Fix missing is_es in schematic_view.rs
    content = content.replace("if self.is_es { \"Simulacion DC activa\" } else { \"DC Simulation active\" }", "if self.lang == raf_core::config::Language::Spanish { \"Simulacion DC activa\" } else { \"DC Simulation active\" }")
    content = content.replace("if self.is_es { \"Simulacion no convergio\" } else { \"Simulation did not converge\" }", "if self.lang == raf_core::config::Language::Spanish { \"Simulacion no convergio\" } else { \"Simulation did not converge\" }")
    
    # 4. If any `is_es` was missed in `schematic_view.rs`:
    content = content.replace("is_es", "lang == raf_core::config::Language::Spanish")
    # Wait, variable `let is_es` might exist! Our previous python script replaced `let is_es = self.is_es;`
    # But maybe `if is_es` remained? Let's just fix the specific compiler errors:
    # "is_es not found in this scope" on `schematic_view.rs:1333 let options = if is_es {`
    # The previous python script replaced `let is_es = self.is_es;` with `let lang = self.lang;`
    # so `is_es` became undefined! We must change `if is_es` to `if lang == raf_core::config::Language::Spanish`
    # Wait, better yet, `options` shouldn't be an `if` anymore! Actually, `options` is an array of strings in schematic_view.
    content = re.sub(r'if is_es \{', 'if lang == raf_core::config::Language::Spanish {', content)
    
    with open(fpath, "w", encoding="utf-8") as f:
        f.write(content)
