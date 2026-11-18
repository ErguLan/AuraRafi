//! Persistent broad phase with a balanced tree and incremental bounds refits.
use super::shapes::BoxShape;
use raf_core::scene::{Aabb, SceneGraph, SceneNodeId, WorldTransformCache};

struct Node {
    bounds: Aabb,
    parent: Option<usize>,
    children: Option<[usize; 2]>,
    leaf: Option<SceneNodeId>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpatialMetrics {
    pub builds: u64,
    pub refits: u64,
    pub visited: u64,
    pub candidates: u64,
}

#[derive(Default)]
pub(crate) struct SpatialIndex {
    ids: Vec<SceneNodeId>,
    nodes: Vec<Node>,
    leaves: Vec<Option<(usize, BoxShape)>>,
    root: Option<usize>,
    revision: Option<u64>,
    metrics: std::cell::Cell<SpatialMetrics>,
}

impl SpatialIndex {
    pub fn rebuild(&mut self, scene: &SceneGraph, ids: &[SceneNodeId]) {
        let revision = scene.geometry_revision();
        if self.revision == Some(revision) && self.ids == ids {
            return;
        }
        self.revision = Some(revision);
        let transforms = WorldTransformCache::build(scene);
        let shape = |id| {
            BoxShape::from_collider(&scene.get(id)?.collider, transforms.world_matrix(id)?)
        };
        if self.ids != ids || ids.iter().any(|id| self.shape(*id).is_none()) {
            self.ids.clear();
            self.ids.extend_from_slice(ids);
            self.nodes.clear();
            self.leaves.clear();
            self.leaves.resize(scene.len(), None);
            let mut entries: Vec<_> = ids.iter().filter_map(|&id| Some((id, shape(id)?))).collect();
            self.root = (!entries.is_empty()).then(|| self.build(&mut entries, None));
            let mut metrics = self.metrics.get();
            metrics.builds += 1;
            self.metrics.set(metrics);
        } else {
            for &id in ids {
                if let Some(shape) = shape(id) {
                    self.update_shape(id, shape);
                }
            }
        }
    }

    fn build(&mut self, entries: &mut [(SceneNodeId, BoxShape)], parent: Option<usize>) -> usize {
        let index = self.nodes.len();
        let bounds = entries.iter().map(|(_, shape)| shape.bounds()).reduce(merge).unwrap();
        self.nodes.push(Node { bounds, parent, children: None, leaf: None });
        if let [(id, shape)] = entries {
            self.nodes[index].leaf = Some(*id);
            self.leaves[id.0] = Some((index, *shape));
            return index;
        }
        let size = bounds.size();
        let axis = if size.x >= size.y && size.x >= size.z { 0 } else if size.y >= size.z { 1 } else { 2 };
        let middle = entries.len() / 2;
        entries.select_nth_unstable_by(middle, |a,b| a.1.center[axis].total_cmp(&b.1.center[axis]).then(a.0.0.cmp(&b.0.0)));
        let (left, right) = entries.split_at_mut(middle);
        let children = [self.build(left, Some(index)), self.build(right, Some(index))];
        self.nodes[index].children = Some(children);
        index
    }

    pub fn shape(&self, id: SceneNodeId) -> Option<BoxShape> {
        self.leaves.get(id.0).copied().flatten().map(|(_, shape)| shape)
    }

    pub fn update_shape(&mut self, id: SceneNodeId, shape: BoxShape) {
        let Some(Some((index, old))) = self.leaves.get_mut(id.0) else { return; };
        if *old == shape { return; }
        *old = shape;
        let index = *index;
        self.update(id, shape.bounds());
        debug_assert!(self.nodes[index].leaf == Some(id));
    }

    pub fn update(&mut self, id: SceneNodeId, bounds: Aabb) {
        let Some(Some((mut index, _))) = self.leaves.get(id.0).copied() else { return; };
        if same_bounds(self.nodes[index].bounds, bounds) { return; }
        self.nodes[index].bounds = bounds;
        let mut metrics = self.metrics.get();
        metrics.refits += 1;
        self.metrics.set(metrics);
        while let Some(parent) = self.nodes[index].parent {
            let [left, right] = self.nodes[parent].children.unwrap();
            let bounds = merge(self.nodes[left].bounds, self.nodes[right].bounds);
            if same_bounds(self.nodes[parent].bounds, bounds) { break; }
            self.nodes[parent].bounds = bounds;
            index = parent;
        }
    }

    pub fn query(&self, bounds: Aabb, output: &mut Vec<SceneNodeId>) {
        output.clear();
        let mut metrics = self.metrics.get();
        if let Some(root) = self.root {
            self.visit(root, bounds, output, &mut metrics.visited);
        }
        output.sort_unstable_by_key(|id| id.0);
        metrics.candidates += output.len() as u64;
        self.metrics.set(metrics);
    }

    fn visit(&self, index: usize, bounds: Aabb, output: &mut Vec<SceneNodeId>, visited: &mut u64) {
        *visited += 1;
        let node = &self.nodes[index];
        if !node.bounds.intersects(&bounds) { return; }
        if let Some(id) = node.leaf {
            output.push(id);
        } else if let Some([left, right]) = node.children {
            self.visit(left, bounds, output, visited);
            self.visit(right, bounds, output, visited);
        }
    }

    pub fn metrics(&self) -> SpatialMetrics { self.metrics.get() }
}

fn same_bounds(a: Aabb, b: Aabb) -> bool { a.min == b.min && a.max == b.max }
fn merge(a: Aabb, b: Aabb) -> Aabb {
    Aabb { min: a.min.min(b.min), max: a.max.max(b.max) }
}

