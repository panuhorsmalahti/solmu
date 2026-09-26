use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Right,
    Down,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node {
    Pane(u64),
    Split {
        axis: Axis,
        ratio: u16,
        first: Box<Node>,
        second: Box<Node>,
    },
}

#[derive(Clone)]
pub struct Divider {
    pub path: Vec<bool>,
    pub axis: Axis,
    pub area: Rect,
    pub parent: Rect,
}

impl Node {
    pub fn valid(&self) -> bool {
        match self {
            Self::Pane(_) => true,
            Self::Split {
                ratio,
                first,
                second,
                ..
            } => (100..=900).contains(ratio) && first.valid() && second.valid(),
        }
    }
    pub fn contains(&self, id: u64) -> bool {
        match self {
            Self::Pane(pane) => *pane == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }
    pub fn ids(&self) -> Vec<u64> {
        match self {
            Self::Pane(id) => vec![*id],
            Self::Split { first, second, .. } => {
                let mut ids = first.ids();
                ids.extend(second.ids());
                ids
            }
        }
    }
    pub fn split(&mut self, selected: u64, new: u64, axis: Axis) -> bool {
        match self {
            Self::Pane(id) if *id == selected => {
                *self = Self::Split {
                    axis,
                    ratio: 500,
                    first: Box::new(Self::Pane(selected)),
                    second: Box::new(Self::Pane(new)),
                };
                true
            }
            Self::Pane(_) => false,
            Self::Split { first, second, .. } => {
                first.split(selected, new, axis) || second.split(selected, new, axis)
            }
        }
    }
    pub fn remove(self, id: u64) -> Option<Self> {
        match self {
            Self::Pane(pane) => {
                if pane == id {
                    None
                } else {
                    Some(Self::Pane(pane))
                }
            }
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(id), second.remove(id)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(node), None) | (None, Some(node)) => Some(node),
                (None, None) => None,
            },
        }
    }
    pub fn swap(&mut self, a: u64, b: u64) {
        match self {
            Self::Pane(id) => {
                if *id == a {
                    *id = b;
                } else if *id == b {
                    *id = a;
                }
            }
            Self::Split { first, second, .. } => {
                first.swap(a, b);
                second.swap(a, b);
            }
        }
    }
    pub fn regions(&self, area: Rect) -> Vec<(u64, Rect)> {
        match self {
            Self::Pane(id) => vec![(*id, area)],
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (a, _, b) = parts(area, *axis, *ratio);
                let mut regions = first.regions(a);
                regions.extend(second.regions(b));
                regions
            }
        }
    }
    pub fn dividers(&self, area: Rect) -> Vec<Divider> {
        let mut dividers = Vec::new();
        self.collect_dividers(area, &mut Vec::new(), &mut dividers);
        dividers
    }
    fn collect_dividers(&self, area: Rect, path: &mut Vec<bool>, output: &mut Vec<Divider>) {
        if let Self::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            let (a, divider, b) = parts(area, *axis, *ratio);
            output.push(Divider {
                path: path.clone(),
                axis: *axis,
                area: divider,
                parent: area,
            });
            path.push(false);
            first.collect_dividers(a, path, output);
            path.pop();
            path.push(true);
            second.collect_dividers(b, path, output);
            path.pop();
        }
    }
    pub fn set_ratio(&mut self, path: &[bool], ratio: u16) {
        if let Self::Split {
            ratio: current,
            first,
            second,
            ..
        } = self
        {
            if let Some((branch, rest)) = path.split_first() {
                if *branch {
                    second.set_ratio(rest, ratio)
                } else {
                    first.set_ratio(rest, ratio)
                }
            } else {
                *current = ratio.clamp(100, 900);
            }
        }
    }
    pub fn resize_near(&mut self, id: u64, axis: Axis, delta: i16) -> bool {
        if let Self::Split {
            axis: direction,
            ratio,
            first,
            second,
        } = self
        {
            let child = if first.contains(id) { first } else { second };
            if child.resize_near(id, axis, delta) {
                return true;
            }
            if *direction == axis {
                *ratio = (*ratio as i16 + delta).clamp(100, 900) as u16;
                return true;
            }
        }
        false
    }
}

pub fn parts(area: Rect, axis: Axis, ratio: u16) -> (Rect, Rect, Rect) {
    let length = if axis == Axis::Right {
        area.width
    } else {
        area.height
    };
    let usable = length.saturating_sub(1);
    let first = (u32::from(usable) * u32::from(ratio) / 1000) as u16;
    let second = usable.saturating_sub(first);
    let gap = u16::from(length > 0);
    if axis == Axis::Right {
        (
            Rect::new(area.x, area.y, first, area.height),
            Rect::new(area.x + first, area.y, gap, area.height),
            Rect::new(area.x + first + gap, area.y, second, area.height),
        )
    } else {
        (
            Rect::new(area.x, area.y, area.width, first),
            Rect::new(area.x, area.y + first, area.width, gap),
            Rect::new(area.x, area.y + first + gap, area.width, second),
        )
    }
}
