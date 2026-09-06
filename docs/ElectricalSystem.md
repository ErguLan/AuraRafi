# Electrical System

> Arquitectura activa: consultar [ELECTRONICS_NATIVE_ARCHITECTURE.md](ELECTRONICS_NATIVE_ARCHITECTURE.md).
> Las referencias a paneles y hosts anteriores que aparezcan más abajo son
> históricas y no describen el runtime nativo actual.

Este documento explica de forma directa como está armado el sistema eléctrico de AuraRafi, qué piezas toca cada crate y cómo se puede extender sin meterse a romper el corazón del engine.

## Qué es este sistema realmente

El sistema eléctrico no es un “modo raro” del scene editor. Es otro dominio completo dentro del proyecto.

La idea actual es esta:

- El editor muestra un canvas de schematics y herramientas de edición.
- `raf_electronics` guarda los datos reales del circuito.
- `raf_render` presenta superficies CAD y registra los assets visuales nativos.
- `raf_core` sigue dando infraestructura general como proyecto, config, i18n y command bus.

Eso evita mezclar lógica de videojuegos con lógica de circuitos.

## Cómo se reparten las capas

### `raf_editor`

Aquí vive la UX del schematic editor.

- `electronics_controller.rs`: documento vivo, cámara, selección, historial,
  escena y análisis.
- `electronics_analysis.rs`: tareas cancelables de DRC y simulación fuera del
  hilo de UI; solo entrega resultados al controlador nativo.
- `electronics_controller_interaction.rs`: input CAD, gestos, authoring,
  undo/redo y persistencia sobre el documento vivo.
- `native_electronics.rs`: host directo de la escena para ApiGraphicBasic.
- `native_workbench.rs`, `native_workbench_input.rs`,
  `native_workbench_surface.rs` y `panels/electronics_*_surface.rs`: shell,
  input semántico y RafUI retenido.
- `native_workbench_electronics.rs` y `panels/electronics_*_surface.rs`:
  inspector, navigator, toolbar y superficies retenidas del circuito.
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

### Assets visuales

Los símbolos visibles del schematic se sirven como PNG nativos desde
`editor/assets/electronics/library/` y RafUI los compone como una capa acotada
sobre el CAD. `CadScene` conserva rectángulos de hit-test, pines y wires, pero
ya no inventa el dibujo del componente con segmentos hardcodeados.

## Flujo de un schematic

El flujo completo hoy va más o menos así:

1. Se abre un proyecto Electronics.
2. El editor carga `schematic.ron`.
3. `NativeElectronicsEditor` deriva `CadScene` y RafUI compone los controles.
4. Al mover, colocar o cablear, el controlador registra historial y marca el documento como modificado.
5. Al guardar, se serializa de nuevo a `schematic.ron`.
6. Si se corre DRC o simulación, el controlador crea una tarea de análisis
   fuera del hilo de UI y el cálculo baja a `raf_electronics`.

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
- assets visuales nativos en `editor/assets/electronics/library/`
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

- Schematic y PCB comparten `CadScene` y el camino de presentación de
  `ApiGraphicBasic`; el dominio aporta geometría de hit-test y footprints sin
  duplicar hosts de render. El artwork del schematic viene del catálogo nativo.

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
