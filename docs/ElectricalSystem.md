# Electrical System

Este documento explica de forma directa como está armado el sistema eléctrico de AuraRafi, qué piezas toca cada crate y cómo se puede extender sin meterse a romper el corazón del engine.

## Qué es este sistema realmente

El sistema eléctrico no es un “modo raro” del scene editor. Es otro dominio completo dentro del proyecto.

La idea actual es esta:

- El editor muestra un canvas de schematics y herramientas de edición.
- `raf_electronics` guarda los datos reales del circuito.
- `raf_render` dibuja símbolos y primitivas visuales reutilizables.
- `raf_core` sigue dando infraestructura general como proyecto, config, i18n y command bus.

Eso evita mezclar lógica de videojuegos con lógica de circuitos.

## Cómo se reparten las capas

### `raf_editor`

Aquí vive la UX del schematic editor.

- `panels/schematic_view.rs`: coordina el panel y el estado alto nivel.
- `panels/schematic_view/canvas.rs`: aquí está lo pesado; hit testing, grid, wire placement, drag, zoom, contexto y dibujo del canvas.
- `panels/schematic_panels.rs`: inspector lateral e “hierarchy” del circuito.
- `schematic_document.rs`: helper pequeño para cargar y guardar `schematic.ron`.

El editor ya no trata al schematic como si fuera una escena 3D disfrazada. Ahora cambia paneles, acciones globales y persistencia según el tipo de proyecto.

### `raf_electronics`

Aquí vive el dato real del circuito.

- `component.rs`: define componentes, pines, modelos de simulación y parseo de valores.
- `schematic.rs`: contiene componentes y wires; también expone helpers como duplicate/remove/test.
- `netlist.rs`: reconstruye las nets a partir de pines y wires.
- `simulation.rs`: corre simulación DC.
- `drc.rs`: corre reglas eléctricas base.
- `library.rs`: librería de componentes disponibles para colocar.

La regla importante aquí es que el editor consume este dato, no lo reinventa.

### `raf_render`

El render se queda con la responsabilidad de los símbolos dibujados por código.

Ahora los símbolos del schematic están centralizados en:

- `raf_render/src/ApiGraphicBasic/schematic_symbols.rs`

Eso importa porque evita spaghetti visual dentro del panel del editor. Si el símbolo cambia, cambia en el render layer, no en veinte callbacks de UI.

## Flujo de un schematic

El flujo completo hoy va más o menos así:

1. Se abre un proyecto Electronics.
2. El editor carga `schematic.ron`.
3. `SchematicViewPanel` muestra componentes, grid y wires.
4. Al mover, colocar o cablear, el panel marca el documento como modificado.
5. Al guardar, se serializa de nuevo a `schematic.ron`.
6. Si se corre DRC o simulación, el cálculo baja a `raf_electronics`.

El punto importante es que `scene.ron` y `schematic.ron` ya no comparten responsabilidad. Cada dominio guarda su documento correcto.

## Librería de componentes

La librería tiene tres fuentes prácticas:

### Componentes built-in

Salen de `ComponentLibrary::default_library()`.

Hoy trae lo básico:

- Resistor
- Capacitor
- LED
- Magnet
- Battery
- Ground

### Assets externos en disco

`ComponentLibrary::load_external_assets()` lee `.ron` desde `ElectricalAssets/`.

Esto sirve para usuarios que quieren meter componentes data-driven sin compilar una extensión de Rust.

### Extensiones registradas por código

Aquí está la parte nueva.

`raf_electronics` ahora tiene un registro para extensiones eléctricas. La gracia es que un mod en código puede agregar cosas así:

```rust
use raf_electronics::{
    register_component_template,
    ComponentTemplate,
    ElectronicComponent,
};

register_component_template(ComponentTemplate {
    name: "Thermistor NTC".to_string(),
    category: "Sensors".to_string(),
    description: "External mod component".to_string(),
    template: ElectronicComponent::resistor("10k"),
});
```

Eso no obliga a editar la librería base ni a tocar el editor.

## Reglas DRC

El DRC base sigue teniendo sus reglas internas:

- floating pins
- missing values
- isolated component
- unnamed net
- short circuit
- led without resistor

La mejora nueva es que ahora también existe un hook para reglas externas.

Un mod puede implementar una regla y registrarla:

```rust
use raf_electronics::{register_drc_rule, DrcIssue, DrcSeverity, ElectricalRule, Schematic};

struct SchoolRule;

impl ElectricalRule for SchoolRule {
    fn id(&self) -> &str {
        "school_rule_voltage_limit"
    }

    fn check(&self, schematic: &Schematic) -> Vec<DrcIssue> {
        let _ = schematic;
        vec![DrcIssue {
            severity: DrcSeverity::Info,
            rule: self.id().to_string(),
            message: "Example external rule".to_string(),
            components: vec![],
            location: None,
        }]
    }
}

register_drc_rule(Box::new(SchoolRule));
```

Cuando corre `run_drc(...)`, primero pasan las reglas internas y luego se agregan las reglas externas.

Eso abre la puerta a:

- reglas educativas
- validaciones de laboratorio
- restricciones de un fabricante
- teoría personalizada para una escuela, curso o empresa

## Relación con complements

El sistema de `complements` sigue siendo la puerta grande para extensiones del engine.

La diferencia ahora es esta:

- `complements` manejan presencia en UI, dominio y ciclo de vida.
- `raf_electronics::extensions` maneja aportes específicos del dominio eléctrico.

Entonces un complemento Electronics puede hacer dos cosas:

1. Mostrar su panel, tab o ventana.
2. Registrar componentes y reglas DRC al iniciar.

Eso era justo el hueco que faltaba. Antes podías meter un “mod”, pero no había una vía limpia para inyectar conocimiento eléctrico sin pegarlo a mano al código base.

## Qué todavía no está cerrado

Hay varias cosas que ya tienen base, pero todavía no están en modo final:

- El bridge C++ sigue siendo más orientado a command bus y lógica headless que a registrar componentes eléctricos nativos.
- Las extensiones eléctricas actuales están pensadas primero para source mods en Rust.
- El mismo patrón para “teorías matemáticas” o paquetes de nodos avanzados todavía no está bajado a su crate final.

O sea: la base correcta ya está puesta para electricidad, y luego esa misma receta se puede clonar en `raf_nodes`, `raf_ai` o donde toque.

## Cómo recomiendo opensourcing esto

Si se va a abrir al público, el mensaje correcto no es “modifica el engine”.

El mensaje correcto es:

- si quieres UI o comportamiento global, usa complements
- si quieres componentes nuevos, registra templates
- si quieres reglas eléctricas nuevas, registra `ElectricalRule`
- si quieres contenido sin compilar, usa `.ron` en `ElectricalAssets/`

Eso le da a la comunidad tres niveles de entrada:

- básico: assets `.ron`
- medio: source mods en Rust
- avanzado: DLL/C++ vía FFI y command bus

## Resumen corto

La arquitectura eléctrica ya no depende de meter lógica nueva a mano dentro del editor.

Ahora el circuito se divide limpio entre:

- UX en `raf_editor`
- datos y reglas en `raf_electronics`
- símbolos en `raf_render`
- extensiones generales en `complements`
- extensiones eléctricas específicas en `raf_electronics::extensions`

Ese era el paso necesario para opensourcing sin convertir el schematic system en otro bloque monolítico.

## Nueva capa: PCB 2D sincronizado

Ahora el dominio eléctrico ya no termina en el schematic.

Se agregó una base nueva para PCB 2D dentro de `raf_electronics::pcb` y su UX en `raf_editor`.

La idea real que ya quedó bajada al código es esta:

- `schematic.ron` sigue siendo la verdad lógica y de simulación.
- `pcb_layout.ron` guarda la parte física: contorno, placement, trazos y airwires pendientes.
- el PCB no reemplaza al schematic; se sincroniza desde él.

### Qué guarda hoy el PCB

El layout nuevo guarda estas piezas:

- `BoardOutline`: polígono del contorno de la placa.
- `PcbComponentPlacement`: referencia estable al componente del schematic, posición física, capa, rotación, lock y footprint.
- `PcbTrace`: trazos de cobre 2D por net y capa.
- `PcbAirwire`: conexiones pendientes de rutear que salen del netlist sincronizado.

Esto permite una separación sana:

- schematic para conectividad y simulación
- PCB para fabricación y acomodo físico

### Sync schematic -> PCB

El sync actual hace varias cosas útiles sin destruir el trabajo manual del usuario:

1. agrega al PCB los componentes nuevos que aparecieron en el schematic
2. conserva la posición manual de los componentes ya colocados
3. actualiza designator, value, footprint y nets por pin
4. limpia componentes huérfanos que ya no existen en el schematic
5. recalcula airwires a partir del estado actual de trazos y pads

Ese punto de preservar placement era obligatorio. Si cada save reconstruyera todo el board desde cero, el PCB sería inutilizable.

### Editor PCB

En el editor ahora hay una vista nueva `PCB View` para proyectos Electronics.

Trae una base funcional para:

- mover componentes físicos sobre la placa
- ver footprints y pads reales en 2D
- enrutar airwires a trazos ortogonales básicos
- dibujar un contorno de board nuevo y cerrarlo antes de exportar
- inspeccionar board, componentes, trazos y airwires desde paneles laterales

Aquí hay una diferencia importante con el schematic:

- en schematic el dibujo puede apoyarse en símbolos dibujados por código
- en PCB la base nueva trabaja con footprint geometry y preview asset references, no con `ApiGraphicBasic`

Todavía faltan previews visuales más ricas y una librería de footprints más extensa, pero la arquitectura ya quedó en el sitio correcto para crecer sin mezclar símbolos lógicos con geometría física.

### Sobre datasheets y footprints

La arquitectura nueva no intenta parsear datasheets arbitrarios de forma automática.

Eso fue intencional.

La ruta base ahora es:

- símbolo esquemático por un lado
- footprint PCB por otro lado
- footprint definido y validado por librería propia

Primero se trabaja con footprints controlados y consistentes. Después ya se puede construir una capa asistida para importar medidas o plantillas externas sin volver frágil el núcleo del editor.

### Gerber hoy

La exportación Gerber sigue en modo placeholder, pero ya no depende de una visión “primero 3D”.

Ahora el placeholder se apoya en el `PcbLayout` y reporta cosas que sí importan para fabricación:

- si el contorno está cerrado o no
- cuántos componentes físicos hay
- cuántos trazos hay
- cuántos airwires siguen abiertos
- qué capas se van a generar cuando el writer final esté listo

O sea: el camino correcto ya es schematic -> pcb_layout -> gerber.