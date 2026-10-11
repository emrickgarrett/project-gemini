use crate::math::Vec3;

/// Dense XZ hash used by separation, chain lightning, ricochet, and pierce.
/// Rebuilt every tick from the enemy list, so it is not part of the snapshot.
#[derive(Clone, Debug)]
pub struct SpatialGrid {
    pub cell: f32,
    pub origin_x: f32,
    pub origin_z: f32,
    pub width: i32,
    pub depth: i32,
    pub cells: Vec<Vec<usize>>,
}

impl Default for SpatialGrid {
    fn default() -> Self {
        Self {
            cell: 4.0,
            origin_x: 0.0,
            origin_z: 0.0,
            width: 1,
            depth: 1,
            cells: vec![Vec::new()],
        }
    }
}

impl SpatialGrid {
    pub fn new(cell: f32, half_extent: f32) -> Self {
        let cell = cell.max(0.5);
        let size = half_extent * 2.0;
        let width = (size / cell).ceil() as i32 + 1;
        let depth = width;
        let count = (width * depth).max(1) as usize;
        Self {
            cell,
            origin_x: -half_extent,
            origin_z: -half_extent,
            width,
            depth,
            cells: vec![Vec::new(); count],
        }
    }

    fn index(&self, x: f32, z: f32) -> Option<usize> {
        let cx = ((x - self.origin_x) / self.cell).floor() as i32;
        let cz = ((z - self.origin_z) / self.cell).floor() as i32;
        if cx < 0 || cz < 0 || cx >= self.width || cz >= self.depth {
            return None;
        }
        Some((cz * self.width + cx) as usize)
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.clear();
        }
    }

    pub fn insert(&mut self, idx: usize, pos: Vec3) {
        if let Some(i) = self.index(pos.x, pos.z) {
            self.cells[i].push(idx);
        }
    }

    pub fn rebuild<T>(&mut self, items: &[T], pos: impl Fn(&T) -> Vec3) {
        self.clear();
        for (idx, item) in items.iter().enumerate() {
            self.insert(idx, pos(item));
        }
    }

    /// Indices whose XZ position is within `radius`, sorted for a stable tick.
    pub fn query_radius<T>(
        &self,
        items: &[T],
        x: f32,
        z: f32,
        radius: f32,
        pos: impl Fn(&T) -> Vec3,
    ) -> Vec<usize> {
        let r_cells = (radius / self.cell).ceil() as i32 + 1;
        let cx = ((x - self.origin_x) / self.cell).floor() as i32;
        let cz = ((z - self.origin_z) / self.cell).floor() as i32;
        let r2 = radius * radius;
        let mut out = Vec::new();
        for dz in -r_cells..=r_cells {
            for dx in -r_cells..=r_cells {
                let ix = cx + dx;
                let iz = cz + dz;
                if ix < 0 || iz < 0 || ix >= self.width || iz >= self.depth {
                    continue;
                }
                let cell = &self.cells[(iz * self.width + ix) as usize];
                for &idx in cell {
                    let p = pos(&items[idx]);
                    let ddx = p.x - x;
                    let ddz = p.z - z;
                    if ddx * ddx + ddz * ddz <= r2 {
                        out.push(idx);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

