//! Bounded graph-to-Rhai compiler. Generated code uses the shared Host API.
//! Authoring validation stays separate; unsupported operations are errors.
use crate::{Node, NodeGraph, PinDataType as T, PinKind as K};
use std::collections::HashSet;

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("String serialization")
}

fn event_record(source: &str) -> Result<String, String> {
    if source.len() > 8192 {
        return Err("Event record exceeds 8192 bytes".into());
    }
    fn emit(value: serde_json::Value, depth: usize, count: &mut usize) -> Result<String, String> {
        *count += 1;
        if depth > 8 || *count > 256 {
            return Err("Event record exceeds depth/item budget".into());
        }
        Ok(match value {
            serde_json::Value::Null => "()".into(),
            serde_json::Value::Bool(v) => v.to_string(),
            serde_json::Value::Number(v) => {
                if let Some(integer) = v.as_i64() {
                    integer.to_string()
                } else {
                    number(&v.to_string())?
                }
            }
            serde_json::Value::String(v) if v.len() <= 4096 => quote(&v),
            serde_json::Value::Array(values) => format!(
                "[{}]",
                values
                    .into_iter()
                    .map(|v| emit(v, depth + 1, count))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            ),
            serde_json::Value::Object(values) => format!(
                "#{{{}}}",
                values
                    .into_iter()
                    .map(|(key, v)| {
                        if key.is_empty() || key.len() > 128 {
                            return Err("Invalid event record key".into());
                        }
                        Ok(format!("{}:{}", quote(&key), emit(v, depth + 1, count)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?
                    .join(",")
            ),
            _ => return Err("Event record string exceeds limit".into()),
        })
    }
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|error| format!("Invalid event JSON: {error}"))?;
    if !value.is_object() {
        return Err("Event record requires a JSON object".into());
    }
    emit(value, 0, &mut 0)
}
fn number(value: &str) -> Result<String, String> {
    let n = value
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("Invalid numeric value: {value}"))?;
    if !n.is_finite() {
        return Err("Non-finite node value".into());
    }
    Ok(format!("{n:?}"))
}
fn vector(value: &str) -> Result<String, String> {
    let values = value
        .split(',')
        .map(number)
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 3 {
        return Err("Vector property requires x,y,z".into());
    }
    Ok(format!("[{}]", values.join(",")))
}
fn boolean(value: &str) -> Result<String, String> {
    match value.trim() {
        "true" | "false" => Ok(value.trim().into()),
        _ => Err("Boolean requires true or false".into()),
    }
}
fn property<'a>(node: &'a Node, key: &str, default: &'a str) -> &'a str {
    node.properties
        .iter()
        .find(|p| p.key == key)
        .map(|p| p.value.as_str())
        .unwrap_or(default)
}

struct Generator<'a> {
    graph: &'a NodeGraph,
    expansions: usize,
}
impl Generator<'_> {
    fn input(
        &mut self,
        index: usize,
        pin: &str,
        key: &str,
        default: &str,
        depth: usize,
    ) -> Result<String, String> {
        if depth > 16 || self.expansions > 10000 {
            return Err("Graph data expansion budget exceeded or cycle".into());
        }
        self.expansions += 1;
        let node = &self.graph.nodes[index];
        let input = node
            .pins
            .iter()
            .find(|p| p.kind == K::Input && p.name == pin);
        if let Some(conn) = input.and_then(|p| {
            self.graph
                .connections
                .iter()
                .find(|c| c.to_pin == p.id && c.to_node == node.id)
        }) {
            let source = self
                .graph
                .nodes
                .iter()
                .position(|n| n.id == conn.from_node)
                .ok_or("Missing data node")?;
            let output = self.graph.nodes[source]
                .pins
                .iter()
                .find(|p| p.id == conn.from_pin)
                .ok_or("Missing data pin")?
                .name
                .clone();
            return self.output(source, &output, depth + 1);
        }
        let value = property(node, key, default);
        let kind = input.map(|p| p.data_type).unwrap_or(T::String);
        match kind {
            T::Float => number(value),
            T::Int => Ok(value
                .parse::<i64>()
                .map_err(|_| "Integer input required")?
                .to_string()),
            T::Bool => boolean(value),
            T::Vec3 => vector(value),
            _ => Ok(quote(value)),
        }
    }
    fn entity_input(
        &mut self,
        index: usize,
        pin: &str,
        key: &str,
        depth: usize,
    ) -> Result<String, String> {
        Ok(format!(
            "entity({})",
            self.input(index, pin, key, "", depth)?
        ))
    }
    fn output(&mut self, index: usize, pin: &str, depth: usize) -> Result<String, String> {
        if depth > 16 {
            return Err("Graph data cycle or depth exceeds 16".into());
        }
        let node = &self.graph.nodes[index];
        let slug = crate::catalog::descriptor_for_node(node)
            .ok_or("Unknown node")?
            .slug;
        Ok(match slug {
            "has-capability" => format!(
                "has_capability({}, {})",
                self.entity_input(index, "Entity", "entity", depth + 1)?,
                self.input(index, "Capability", "capability", "", depth + 1)?
            ),
            "get-entity-property" => format!(
                "get_property({}, {})",
                self.entity_input(index, "Entity", "entity", depth + 1)?,
                self.input(index, "Property", "property_name", "", depth + 1)?
            ),
            "entity-reference" => format!(
                "get_entity_reference({}, {})",
                self.entity_input(index, "Entity", "entity", depth + 1)?,
                self.input(index, "Property", "property_name", "", depth + 1)?
            ),
            "query-capability" => format!(
                "query_capability({}, {})",
                self.input(index, "Capability", "capability", "", depth + 1)?,
                self.input(index, "Limit", "query_limit", "32", depth + 1)?
            ),
            "query-nearby" => format!(
                "query_nearby({}, {}, {}, {})",
                self.input(index, "Capability", "capability", "", depth + 1)?,
                self.input(index, "Position", "position", "0,0,0", depth + 1)?,
                self.input(index, "Radius", "query_radius", "10", depth + 1)?,
                self.input(index, "Limit", "query_limit", "32", depth + 1)?
            ),
            "event-record" => event_record(property(node, "event_data", "{}"))?,
            "get-entity" => format!(
                "entity({})",
                self.input(index, "Reference", "reference", "", depth + 1)?
            ),
            "is-valid" => format!(
                "is_valid({})",
                self.entity_input(index, "Entity", "entity", depth + 1)?
            ),
            "get-active-camera" => "get_active_camera()".into(),
            "get-parent" => format!(
                "get_parent({})",
                self.entity_input(index, "Entity", "entity", depth + 1)?
            ),
            "find-child" => format!(
                "find_child({}, {})",
                self.entity_input(index, "Entity", "entity", depth + 1)?,
                quote(property(node, "child_name", ""))
            ),
            "get-position" => format!(
                "get_position({})",
                self.entity_input(index, "Entity", "entity", depth + 1)?
            ),
            "get-velocity" => format!(
                "get_velocity({})",
                self.entity_input(index, "Entity", "entity", depth + 1)?
            ),
            slug if crate::physics_nodes::is_event(slug) => match pin {
                "Entity" => "event_value.entity".into(),
                "Other" => "event_value.other".into(),
                _ => return Err(format!("Unknown physics output: {pin}")),
            },
            "on-update" | "on-late-update" | "on-event" if pin == "Delta Time" => "dt".into(),
            "on-event" if pin == "Value" => "event_value".into(),
            "add" => format!(
                "({}+{})",
                self.input(index, "A", "a", "0", depth + 1)?,
                self.input(index, "B", "b", "0", depth + 1)?
            ),
            "greater-than" | "less-than" | "equals" | "not-equals" => {
                let op = match slug {
                    "greater-than" => ">",
                    "less-than" => "<",
                    "equals" => "==",
                    _ => "!=",
                };
                format!(
                    "({}{op}{})",
                    self.input(index, "A", "a", "0", depth + 1)?,
                    self.input(index, "B", "b", "0", depth + 1)?
                )
            }
            "key-press" => format!(
                "was_key_just_pressed({})",
                quote(property(node, "key", "Space"))
            ),
            "mouse-click" if matches!(pin, "X" | "Y") => {
                return Err(
                    "Pointer coordinates are not exposed by the current runtime input contract"
                        .into(),
                )
            }
            "mouse-click" => format!(
                "this.mouse_now[{}]",
                match property(node, "button", "Left")
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "left" | "0" => 0,
                    "right" | "1" => 1,
                    "middle" | "2" => 2,
                    _ => return Err("Invalid mouse button".into()),
                }
            ),
            "for-loop" if pin == "Index" => format!("this.indices[{index}]"),
            _ => return Err(format!("Unsupported data output: {} / {pin}", node.name)),
        })
    }
    fn flow(&self, index: usize, pin: &str) -> Result<i64, String> {
        let node = &self.graph.nodes[index];
        let Some(pin) = node
            .pins
            .iter()
            .find(|p| p.kind == K::Output && p.name == pin && p.data_type == T::Flow)
        else {
            return Ok(-1);
        };
        let mut connections = self
            .graph
            .connections
            .iter()
            .filter(|c| c.from_node == node.id && c.from_pin == pin.id);
        let Some(conn) = connections.next() else {
            return Ok(-1);
        };
        if connections.next().is_some() {
            return Err("Flow fan-out requires explicit sequencing; one successor per pin".into());
        }
        Ok(self
            .graph
            .nodes
            .iter()
            .position(|n| n.id == conn.to_node)
            .ok_or("Missing flow node")? as i64)
    }
    fn action(&mut self, i: usize) -> Result<String, String> {
        let n = &self.graph.nodes[i];
        let slug = crate::catalog::descriptor_for_node(n)
            .ok_or_else(|| format!("Unknown node: {}", n.name))?
            .slug;
        let next = self.flow(
            i,
            match slug {
                "key-press" => "Pressed",
                "mouse-click" => "Clicked",
                _ => "Out",
            },
        )?;
        let entity = if crate::physics_nodes::is_event(slug)
            || matches!(
                slug,
                "on-start"
                    | "on-update"
                    | "on-late-update"
                    | "on-event"
                    | "if"
                    | "print"
                    | "spawn-entity"
                    | "for-loop"
                    | "while-loop"
                    | "delay"
                    | "key-press"
                    | "mouse-click"
                    | "clear-camera"
            ) {
            String::new()
        } else {
            let pin = if matches!(slug, "set-position" | "destroy-entity") {
                "Entity ID"
            } else {
                "Entity"
            };
            self.entity_input(i, pin, "entity", 0)?
        };
        let mut code = String::new();
        match slug {
            "set-capability" => code = format!("let h={entity}; h.set_capability({},{});", self.input(i,"Capability","capability","",0)?,self.input(i,"Enabled","enabled","true",0)?),
            "set-entity-property" => code = format!("let h={entity}; h.set_property({},{});",self.input(i,"Property","property_name","",0)?,self.input(i,"Value","value","",0)?),
            "has-capability"|"get-entity-property"|"query-capability"|"query-nearby"|"event-record"|"entity-reference" => return Err(format!("Data-only node {} cannot be in a flow chain", n.name)),
            slug if crate::physics_nodes::is_event(slug) => {},
            "set-velocity" | "add-velocity" | "apply-impulse" => {
                let (pin, key) = if slug == "apply-impulse" { ("Impulse", "impulse") } else { ("Velocity", "velocity") };
                code = format!("let h={entity}; h.{}({});", slug.replace('-', "_"), self.input(i, pin, key, "0,0,0", 0)?);
            }
            "configure-body" => code = format!("let h={entity}; h.configure_body({},{},{},{},{});",
                self.input(i,"Body Type","body_type","dynamic",0)?, self.input(i,"Gravity","gravity","true",0)?,
                self.input(i,"Trigger","trigger","false",0)?, self.input(i,"Mass","mass","1",0)?, self.input(i,"Damping","damping","0",0)?),
            "set-physics-enabled" => code = format!("let h={entity}; h.set_physics_enabled({});", self.input(i,"Enabled","enabled","true",0)?),
            "set-aabb-collider" => code = format!("let h={entity}; h.set_aabb_collider({},{});", self.input(i,"Min","collider_min","-0.5,-0.5,-0.5",0)?, self.input(i,"Max","collider_max","0.5,0.5,0.5",0)?),
            "remove-collider" => code = format!("let h={entity}; h.remove_collider();"),
            "get-velocity" => return Err("Get Velocity is data-only".into()),
            "on-start"|"on-update"|"on-late-update"|"on-event"=>{},
            "print"=>code=format!("print({});",self.input(i,"Message","message","",0)?),
            "if"=>return Ok(format!("next = if {} {{ {} }} else {{ {} }};",self.input(i,"Condition","condition","false",0)?,self.flow(i,"True")?,self.flow(i,"False")?)),
            "spawn-entity"=>code=format!("let h=spawn_entity({},{}); let p={}; h.set_position(p[0],p[1],p[2]);",self.input(i,"Name","entity_name","Entity",0)?,quote(property(n,"primitive","empty")),self.input(i,"Position","position","0,0,0",0)?),
            "destroy-entity"=>code=format!("destroy_entity({entity});"),
            "set-position"|"set-local-position"|"set-rotation"=>{
                let (pin,key,fun)=match slug{"set-position"=>("New Position","position","set_position"),"set-local-position"=>("Value","value","set_local_position"),_=>("Value","value","set_rotation")};
                code=format!("let h={entity}; let p={}; h.{fun}(p[0],p[1],p[2]);",self.input(i,pin,key,"0,0,0",0)?);
            }
            "add-camera"|"remove-camera"|"activate-camera"=>code=format!("{}({entity});",slug.replace('-',"_")),
            "clear-camera"=>code="clear_active_camera();".into(),
            "set-camera-fov"=>code=format!("set_fov({entity},{});",self.input(i,"FOV","fov_degrees","60",0)?),
            "set-camera-clip"=>code=format!("set_clip({entity},{},{});",self.input(i,"Near","near","0.1",0)?,self.input(i,"Far","far","1000",0)?),
            "set-camera-projection"=>code=format!("set_orthographic({entity},{},{});",self.input(i,"Orthographic","orthographic","false",0)?,self.input(i,"Scale","ortho_scale","10",0)?),
            "look-at"=>code=format!("let h={entity}; h.look_at({},{});",self.entity_input(i,"Target","target",0)?,self.input(i,"Aim Offset","aim_offset","0,0,0",0)?),
            "follow-camera"=>code=format!("let h={entity}; let t={}; if is_valid(h) && is_valid(t) {{ h.follow(t,{},{},{},dt); }}",self.entity_input(i,"Target","target",0)?,self.input(i,"Offset","offset","0,2,8",0)?,self.input(i,"Aim Offset","aim_offset","0,1,0",0)?,self.input(i,"Sharpness","sharpness","8",0)?),
            "get-entity"|"get-parent"|"find-child"|"get-position"|"get-active-camera"|"add"|"greater-than"|"less-than"|"equals"|"not-equals"=>return Err(format!("Data-only node {} cannot be in a flow chain",n.name)),
            "set-parent"=>code=format!("set_parent({entity},{},{});",self.entity_input(i,"Target","target",0)?,boolean(property(n,"keep_world","true"))?),
            "detach-parent"=>code=format!("detach_parent({entity},{});",boolean(property(n,"keep_world","true"))?),
            "send-event"=>code=if boolean(property(n,"broadcast","false"))?=="true" {
                format!("emit_event({},{});",quote(property(n,"event_name","")),self.input(i,"Value","value","",0)?)
            } else {format!("send_event({entity},{},{});",quote(property(n,"event_name","")),self.input(i,"Value","value","",0)?)},
            "key-press"|"mouse-click"=>return Ok(format!("next = if {} {{ {next} }} else {{ -1 }};",self.output(i,"",0)?)),
            "for-loop"=>return Ok(format!("let first={}; let last={}; if last-first>4096 || last<first {{ throw \"Invalid loop range\"; }} let saved=this.indices[{i}]; for index in first..last {{ this.indices[{i}]=index; this.run_flow({},dt,event_value); }} this.indices[{i}]=saved; next={};",self.input(i,"Start","start","0",0)?,self.input(i,"End","end","1",0)?,self.flow(i,"Loop Body")?,self.flow(i,"Completed")?)),
            "while-loop"=>return Ok(format!("let count=0; while {} {{ count+=1; if count>4096 {{ throw \"While budget exceeded\"; }} this.run_flow({},dt,event_value); }} next={};",self.input(i,"Condition","condition","false",0)?,self.flow(i,"Loop Body")?,self.flow(i,"Completed")?)),
            "delay"=>code=format!("let seconds={}; if seconds<0.0 || seconds>3600.0 {{ throw \"Delay seconds must be 0..3600\"; }} if this.pending.len>=128 {{ throw \"Delay queue budget exceeded\"; }} this.pending.push([get_elapsed_time()+seconds,{next},event_value,this.indices]); next=-1;",self.input(i,"Seconds","seconds","1.0",0)?),
            // Unsupported legacy executor nodes must fail visibly, never pass through.
            _=>return Err(format!("Node {} is not supported by the game runtime compiler",n.name)),
        }
        if slug == "delay" {
            return Ok(code);
        }
        Ok(format!("{code} next={next};"))
    }
}

pub fn to_rhai(graph: &NodeGraph) -> Result<String, String> {
    if graph.nodes.len() > 64 || graph.connections.len() > 256 {
        return Err("Runtime graph budget: 64 nodes / 256 connections".into());
    }
    let validation = crate::compiler::compile(graph);
    if !validation.success {
        return Err(validation.errors.join("; "));
    }
    let mut ids = HashSet::new();
    let mut pins = HashSet::new();
    for node in &graph.nodes {
        let descriptor = crate::catalog::descriptor_for_node(node)
            .ok_or_else(|| format!("Unknown node: {}", node.name))?;
        if descriptor.category == crate::NodeCategory::Electronics {
            return Err(format!(
                "Node {} is not supported by the game runtime compiler",
                node.name
            ));
        }
        if !ids.insert(node.id) || node.pins.iter().any(|p| !pins.insert(p.id)) {
            return Err("Duplicate graph node or pin identity".into());
        }
    }
    let mut g = Generator {
        graph,
        expansions: 0,
    };
    // A method receiver carries mutable graph state through nested calls.
    // Rhai does not capture the caller's top-level scope in nested functions.
    let mut source=format!("let graph_state=#{{steps:0,pending:[],mouse_held:[false,false,false],mouse_now:[false,false,false],indices:[{}]}};\nfn run_flow(next,dt,event_value) {{ while next>=0 {{ this.steps+=1; if this.steps>4096 {{ throw \"Graph flow budget exceeded\"; }} switch next {{",vec!["0";graph.nodes.len()].join(","));
    // Only flow nodes are dispatched; data nodes are evaluated through their inputs.
    for i in 0..graph.nodes.len() {
        if graph.nodes[i].pins.iter().any(|p| p.data_type == T::Flow) {
            source.push_str(&format!("{i} => {{ {} }},", g.action(i)?));
        } else {
            // Validate even disconnected nodes so unsupported behavior is visible.
            let node = &graph.nodes[i];
            if crate::catalog::descriptor_for_node(node).is_none() {
                return Err(format!("Unknown node: {}", node.name));
            }
        }
    }
    source.push_str("_ => { throw \"Invalid graph flow\"; } } } }\n");
    for (slug, hook, args, value) in [
        ("on-start", "on_start", "", "()"),
        ("on-update", "on_update", "dt", "()"),
        ("on-late-update", "on_late_update", "dt", "()"),
        ("on-event", "on_event", "name,event_value", "event_value"),
    ] {
        source.push_str(&format!("fn {hook}({args}) {{"));
        source.push_str("graph_state.steps=0;");
        if args.is_empty() {
            source.push_str("let dt=0.0;");
        }
        if slug == "on-event" {
            source.push_str("let dt=get_delta_time();");
        }
        if slug == "on-update" {
            source.push_str("for b in 0..3 { let held=is_mouse_pressed(b); graph_state.mouse_now[b]=held && !graph_state.mouse_held[b]; graph_state.mouse_held[b]=held; }");
        }
        for (i, node) in graph.nodes.iter().enumerate() {
            if crate::catalog::descriptor_for_node(node).is_some_and(|d| d.slug == slug) {
                if slug == "on-event" {
                    source.push_str(&format!(
                        "if name=={} {{",
                        quote(property(node, "event_name", ""))
                    ));
                }
                source.push_str(&format!("graph_state.run_flow({i},dt,{value});"));
                if slug == "on-event" {
                    source.push('}');
                }
            }
        }
        if slug == "on-update" {
            source.push_str("let pending=graph_state.pending; graph_state.pending=[]; for job in pending { if job[0]<=get_elapsed_time() { let saved=graph_state.indices; graph_state.indices=job[3]; graph_state.run_flow(job[1],dt,job[2]); graph_state.indices=saved; } else { graph_state.pending.push(job); } }");
            for (i, node) in graph.nodes.iter().enumerate() {
                if crate::catalog::descriptor_for_node(node)
                    .is_some_and(|d| matches!(d.slug, "key-press" | "mouse-click"))
                {
                    source.push_str(&format!("graph_state.run_flow({i},dt,());"));
                }
            }
        }
        source.push_str("}\n");
    }
    let has_physics_events = graph.nodes.iter().any(|node| {
        crate::catalog::descriptor_for_node(node)
            .is_some_and(|descriptor| crate::physics_nodes::is_event(descriptor.slug))
    });
    if has_physics_events {
        source.push_str(
            "fn on_physics_event(event_value) { graph_state.steps=0; let dt=get_delta_time();",
        );
        for (i, node) in graph.nodes.iter().enumerate() {
            let slug = crate::catalog::descriptor_for_node(node)
                .ok_or("Unknown node")?
                .slug;
            if crate::physics_nodes::is_event(slug) {
                source.push_str(&format!(
                    "if event_value.kind=={} {{",
                    quote(&slug.trim_start_matches("on-").replace('-', "_"))
                ));
                let reference = property(node, "entity", "");
                if !reference.is_empty() {
                    source.push_str(&format!("let filter=entity({}); if is_valid(filter) && node_uuid(filter)==event_value.entity_uuid {{", quote(reference)));
                }
                source.push_str(&format!("graph_state.run_flow({i},dt,event_value);"));
                if !reference.is_empty() {
                    source.push('}');
                }
                source.push('}');
            }
        }
        source.push_str("}\n");
    }
    if source.len() > 256 * 1024 {
        return Err("Generated script exceeds 256 KiB".into());
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn behavior_nodes_and_structured_payload_compile_through_host_api() {
        for &(slug, _, _) in crate::scene_nodes::SCENE_NODES {
            if crate::behavior_nodes::create(slug).is_none() {
                continue;
            }
            let node = crate::catalog::create(slug).unwrap();
            let descriptor = crate::catalog::descriptor_for_slug(slug).unwrap();
            let mut keys = vec![
                descriptor.label_key.to_owned(),
                descriptor.description_key.to_owned(),
            ];
            keys.extend(node.pins.iter().map(|p| {
                format!(
                    "nodes.pin.{slug}.{}",
                    p.name.to_lowercase().replace(' ', "_")
                )
            }));
            keys.extend(node.properties.iter().map(|p| p.label_key.clone()));
            for lang in [raf_core::Language::English, raf_core::Language::Spanish] {
                for key in &keys {
                    assert_ne!(raf_core::i18n::t(key, lang), *key);
                }
            }
        }
        let mut graph = NodeGraph::new("Behavior");
        let start = crate::catalog::create("on-start").unwrap();
        let mut send = crate::catalog::create("send-event").unwrap();
        for property in &mut send.properties {
            if property.key == "broadcast" {
                property.value = "true".into();
            }
            if property.key == "event_name" {
                property.value = "power.changed".into();
            }
        }
        let mut data = crate::catalog::create("event-record").unwrap();
        data.properties[0].value = r#"{"amount":2,"state":{"enabled":true}}"#.into();
        let pin = |node: &Node, name: &str| node.pins.iter().find(|p| p.name == name).unwrap().id;
        graph.connect(start.id, pin(&start, "Out"), send.id, pin(&send, "In"));
        graph.connect(data.id, pin(&data, "Value"), send.id, pin(&send, "Value"));
        graph.add_node(start);
        graph.add_node(send);
        graph.add_node(data);
        let code = to_rhai(&graph).unwrap();
        assert!(code.contains("emit_event(\"power.changed\",#{"));
        assert!(code.contains("\"enabled\":true"));
        assert!(event_record(r#"{"invalid":""}"#).is_ok());
        assert!(event_record("[]").is_err());
    }
    #[test]
    fn physics_catalog_compiles_all_operations_and_has_bilingual_labels() {
        for &(slug, _, _) in crate::scene_nodes::SCENE_NODES {
            if crate::physics_nodes::create(slug).is_none() {
                continue;
            }
            let node = crate::catalog::create(slug).unwrap();
            let descriptor = crate::catalog::descriptor_for_slug(slug).unwrap();
            let mut keys = vec![
                descriptor.label_key.to_string(),
                descriptor.description_key.to_string(),
            ];
            keys.extend(node.pins.iter().map(|p| {
                format!(
                    "nodes.pin.{slug}.{}",
                    p.name.to_lowercase().replace(' ', "_")
                )
            }));
            keys.extend(node.properties.iter().map(|p| p.label_key.clone()));
            for lang in [raf_core::Language::English, raf_core::Language::Spanish] {
                for key in &keys {
                    assert_ne!(raf_core::i18n::t(key, lang), *key, "{key}");
                }
            }
            let mut graph = NodeGraph::new("Physics");
            graph.add_node(node);
            let code = to_rhai(&graph).unwrap();
            assert_eq!(
                code.contains("fn on_physics_event(event_value)"),
                crate::physics_nodes::is_event(slug)
            );
        }
    }
    #[test]
    fn rejects_duplicate_identity_and_invalid_property_before_execution() {
        let mut graph = NodeGraph::new("Invalid");
        let node = crate::catalog::create("on-start").unwrap();
        graph.add_node(node.clone());
        graph.add_node(node);
        assert!(to_rhai(&graph).is_err());
        let mut graph = NodeGraph::new("Invalid lens");
        let mut node = crate::catalog::create("set-camera-fov").unwrap();
        node.properties
            .iter_mut()
            .find(|p| p.key == "fov_degrees")
            .unwrap()
            .value = "NaN".into();
        graph.add_node(node);
        assert!(to_rhai(&graph).is_err());
    }
    #[test]
    fn disconnected_hardware_nodes_are_rejected_before_play() {
        for slug in [
            "serial-read",
            "serial-write",
            "read-sensor",
            "write-actuator",
        ] {
            let mut graph = NodeGraph::new("Unsupported hardware");
            graph.add_node(crate::catalog::create(slug).unwrap());
            assert!(to_rhai(&graph).unwrap_err().contains("not supported"));
        }
    }

    #[test]
    fn escaping_keeps_user_strings_out_of_generated_code() {
        let mut graph = NodeGraph::new("Escaping");
        let mut node = crate::catalog::create("print").unwrap();
        node.properties[0].value = "\"; destroy_entity(entity(\"Camera\")); //".into();
        graph.add_node(node);
        let source = to_rhai(&graph).unwrap();
        assert!(source.contains("\\\"; destroy_entity"));
        assert!(source.contains("fn on_late_update(dt)"));
    }
}
