//! Screen-space label placement; never edits board geometry or fabrication text.
use glam::Vec2;
use raf_ui::UiRect;
use std::collections::HashMap;

pub(super) const FONT_SIZE: f32 = 14.0;
pub(super) const LINE_HEIGHT: f32 = 18.0;
const GAP: f32 = 5.0;
const CELL: f32 = 64.0;

pub(super) struct LabelRequest {
    pub index: usize,
    pub body: UiRect,
    pub text: String,
    pub width: f32,
    pub selected: bool,
}

pub(super) struct LabelPlacement {
    pub text: String,
    pub rect: UiRect,
}

#[derive(Default)]
struct Occupancy(HashMap<(i32, i32), Vec<UiRect>>);

impl Occupancy {
    fn cells(rect: UiRect) -> impl Iterator<Item = (i32, i32)> {
        let x = (rect.x / CELL).floor() as i32;
        let y = (rect.y / CELL).floor() as i32;
        let right = (rect.right() / CELL).floor() as i32;
        let bottom = (rect.bottom() / CELL).floor() as i32;
        (x..=right).flat_map(move |x| (y..=bottom).map(move |y| (x, y)))
    }

    fn insert(&mut self, rect: UiRect) {
        for cell in Self::cells(rect) {
            self.0.entry(cell).or_default().push(rect);
        }
    }

    fn overlaps(&self, rect: UiRect) -> bool {
        Self::cells(rect).any(|cell| {
            self.0.get(&cell).is_some_and(|items| {
                items
                    .iter()
                    .any(|item| !rect.intersection(*item).is_empty())
            })
        })
    }
}

pub(super) fn place_labels(
    mut requests: Vec<LabelRequest>,
    size: Vec2,
    reserved: &[UiRect],
) -> HashMap<usize, LabelPlacement> {
    let viewport = UiRect::new(2.0, 2.0, (size.x - 4.0).max(0.0), (size.y - 4.0).max(0.0));
    let mut occupied = Occupancy::default();
    for rect in reserved {
        occupied.insert(*rect);
    }
    // All footprints are obstacles, including those whose labels lose a space.
    for request in &requests {
        occupied.insert(request.body);
    }
    requests.sort_by_key(|r| (!r.selected, r.index));
    let mut result = HashMap::new();
    for request in requests {
        let height = LINE_HEIGHT * request.text.lines().count().max(1) as f32;
        let full = find_space(request.body, request.width, height, viewport, &occupied);
        let placement = if let Some(rect) = full {
            Some(LabelPlacement {
                text: request.text,
                rect,
            })
        } else if let Some((reference, _)) = request.text.split_once('\n') {
            // At overview density keep a readable reference before the value.
            let width = (reference.chars().count() as f32 * FONT_SIZE * 0.65 + 8.0).max(24.0);
            find_space(request.body, width, LINE_HEIGHT, viewport, &occupied).map(|rect| {
                LabelPlacement {
                    text: reference.to_string(),
                    rect,
                }
            })
        } else {
            None
        };
        if let Some(placement) = placement {
            occupied.insert(UiRect::new(
                placement.rect.x - 2.0,
                placement.rect.y - 2.0,
                placement.rect.width + 4.0,
                placement.rect.height + 4.0,
            ));
            result.insert(request.index, placement);
        }
    }
    result
}

fn find_space(
    body: UiRect,
    width: f32,
    height: f32,
    viewport: UiRect,
    occupied: &Occupancy,
) -> Option<UiRect> {
    let cx = body.x + body.width * 0.5;
    let cy = body.y + body.height * 0.5;
    for extra in [0.0, 12.0, 24.0] {
        let gap = GAP + extra;
        for (x, y) in [
            (cx - width * 0.5, body.y - height - gap),
            (cx - width * 0.5, body.bottom() + gap),
            (body.x - width - gap, cy - height * 0.5),
            (body.right() + gap, cy - height * 0.5),
            (body.x - width - gap, body.y - height - GAP),
            (body.right() + gap, body.y - height - GAP),
            (body.x - width - gap, body.bottom() + GAP),
            (body.right() + gap, body.bottom() + GAP),
        ] {
            let rect = UiRect::new(x, y, width, height);
            if rect.x >= viewport.x
                && rect.y >= viewport.y
                && rect.right() <= viewport.right()
                && rect.bottom() <= viewport.bottom()
                && !occupied.overlaps(rect)
            {
                return Some(rect);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electronics_heart_labels_avoid_pads_neighbors_and_minimap_at_board_scale() {
        let positions = [
            (15., 12.),
            (25., 10.),
            (35., 10.),
            (45., 12.),
            (30., 40.),
            (30., 27.),
            (15., 20.),
            (25., 18.),
            (35., 18.),
            (45., 20.),
            (30., 33.),
        ];
        let bodies: Vec<_> = positions
            .iter()
            .map(|&(x, y)| UiRect::new(90. + x * 7.56, 40. + y * 7.56, 24., 14.))
            .collect();
        let reserved = [UiRect::new(600., 6., 180., 140.)];
        let requests = bodies
            .iter()
            .enumerate()
            .map(|(index, &body)| LabelRequest {
                index,
                body,
                text: format!("R{index}\n1k"),
                width: 36.,
                selected: index == 5,
            })
            .collect();
        let placed = place_labels(requests, Vec2::new(800., 600.), &reserved);
        assert_eq!(placed.len(), bodies.len());
        for (index, label) in &placed {
            assert!(bodies
                .iter()
                .chain(&reserved)
                .all(|r| label.rect.intersection(*r).is_empty()));
            assert!(placed
                .iter()
                .all(|(other, r)| other == index || label.rect.intersection(r.rect).is_empty()));
        }
    }
}
